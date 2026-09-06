//! Validated atlas load and paired mutable-state backup recovery.

use crate::planet_atlas::codec::container::{
    container_bytes, decode_container, decode_variable_container, variable_container_bytes,
};
use crate::planet_atlas::codec::dynamic::decode_dynamic;
use crate::planet_atlas::codec::genesis::decode_genesis;
use crate::planet_atlas::codec::{
    DYNAMIC_MAGIC, DYNAMIC_RECORD_BYTES, GENESIS_MAGIC, GENESIS_RECORD_BYTES, WATER_CYCLE_MAGIC,
};
use crate::planet_atlas::identity::stable_hash;
use crate::planet_atlas::manifest::validate_manifest;
use crate::planet_atlas::storage::{
    BIOMES_FILE, DYNAMIC_BACKUP_FILE, DYNAMIC_FILE, GENESIS_FILE, GEOLOGY_FILE, HISTORY_FILE,
    HYDROLOGY_FILE, MANIFEST_FILE, MAX_BIOMES_BYTES, MAX_DYNAMIC_BYTES, MAX_GENESIS_BYTES,
    MAX_GEOLOGY_BYTES, MAX_HISTORY_BYTES, MAX_HYDROLOGY_BYTES, MAX_MANIFEST_BYTES,
    MAX_WATER_CYCLE_BYTES, WATER_CYCLE_BACKUP_FILE, WATER_CYCLE_FILE, read_bounded, write_manifest,
};
use crate::planet_atlas::{
    ATLAS_DYNAMIC_VERSION, ATLAS_FORMAT_VERSION, AtlasError, AtlasManifest, BiomeModel,
    GeologyModel, HistoryLayers, HydrologyModel, PlanetAtlas, WATER_CYCLE_SCHEMA_VERSION,
    decode_water_cycle,
};
use std::path::Path;

impl PlanetAtlas {
    pub(super) fn load_planet_dir(
        planet_dir: &Path,
        production_only: bool,
    ) -> Result<Self, AtlasError> {
        let manifest_path = planet_dir.join(MANIFEST_FILE);
        let manifest_bytes = read_bounded(&manifest_path, MAX_MANIFEST_BYTES)?;
        let manifest_text = std::str::from_utf8(&manifest_bytes)
            .map_err(|_| AtlasError::Corrupt("manifest is not UTF-8".into()))?;
        let mut manifest: AtlasManifest = toml::from_str(manifest_text)
            .map_err(|error| AtlasError::Corrupt(format!("manifest TOML: {error}")))?;
        validate_manifest(&manifest, production_only)?;

        let genesis_payload = decode_container(
            &planet_dir.join(GENESIS_FILE),
            GENESIS_MAGIC,
            ATLAS_FORMAT_VERSION,
            manifest.atlas_face_side,
            GENESIS_RECORD_BYTES,
            MAX_GENESIS_BYTES,
        )?;
        if stable_hash(&genesis_payload) != manifest.genesis_checksum {
            return Err(AtlasError::Corrupt(
                "immutable genesis does not match the committed manifest".into(),
            ));
        }
        let genesis = decode_genesis(manifest.atlas_face_side, &genesis_payload)?;

        let dynamic_path = planet_dir.join(DYNAMIC_FILE);
        let dynamic_backup = planet_dir.join(DYNAMIC_BACKUP_FILE);
        let load_dynamic_payload =
            |path: &Path, expected_checksum: Option<u64>| -> Result<Vec<u8>, AtlasError> {
                let payload = decode_container(
                    path,
                    DYNAMIC_MAGIC,
                    ATLAS_DYNAMIC_VERSION,
                    manifest.atlas_face_side,
                    DYNAMIC_RECORD_BYTES,
                    MAX_DYNAMIC_BYTES,
                )?;
                if expected_checksum.is_some_and(|checksum| stable_hash(&payload) != checksum) {
                    return Err(AtlasError::Corrupt(format!(
                        "{} does not match the committed dynamic checksum",
                        path.display()
                    )));
                }
                Ok(payload)
            };
        let mut dynamic = match load_dynamic_payload(&dynamic_path, Some(manifest.dynamic_checksum))
        {
            Ok(payload) => decode_dynamic(manifest.atlas_face_side, &payload)?,
            Err(primary_error) => match load_dynamic_payload(&dynamic_backup, None) {
                Ok(payload) => {
                    let restored = container_bytes(
                        DYNAMIC_MAGIC,
                        ATLAS_DYNAMIC_VERSION,
                        manifest.atlas_face_side,
                        DYNAMIC_RECORD_BYTES,
                        &payload,
                    )?;
                    manifest.dynamic_checksum = stable_hash(&payload);
                    manifest.dynamic_bytes = restored.len() as u64;
                    let water_backup_payload = decode_variable_container(
                        &planet_dir.join(WATER_CYCLE_BACKUP_FILE),
                        WATER_CYCLE_MAGIC,
                        WATER_CYCLE_SCHEMA_VERSION,
                        manifest.atlas_face_side,
                        MAX_WATER_CYCLE_BYTES,
                    )?;
                    let water_restored = variable_container_bytes(
                        WATER_CYCLE_MAGIC,
                        WATER_CYCLE_SCHEMA_VERSION,
                        manifest.atlas_face_side,
                        &water_backup_payload,
                    )?;
                    manifest.water_cycle_checksum = stable_hash(&water_backup_payload);
                    manifest.water_cycle_bytes = water_restored.len() as u64;
                    crate::persist::atomic_write(&dynamic_path, &restored, false)?;
                    crate::persist::atomic_write(
                        &planet_dir.join(WATER_CYCLE_FILE),
                        &water_restored,
                        false,
                    )?;
                    write_manifest(planet_dir, &manifest)?;
                    eprintln!(
                        "warning: restored mutable planet atlas from backup after: {primary_error}"
                    );
                    decode_dynamic(manifest.atlas_face_side, &payload)?
                }
                Err(backup_error) => {
                    return Err(AtlasError::Corrupt(format!(
                        "dynamic atmosphere is unrecoverable; refusing to mint or destroy water (primary: {primary_error}; backup: {backup_error})"
                    )));
                }
            },
        };

        let history_bytes = read_bounded(&planet_dir.join(HISTORY_FILE), MAX_HISTORY_BYTES)?;
        if stable_hash(&history_bytes) != manifest.history_checksum {
            return Err(AtlasError::Corrupt(
                "history overlay does not match the committed manifest".into(),
            ));
        }
        let history_text = std::str::from_utf8(&history_bytes)
            .map_err(|_| AtlasError::Corrupt("history overlay is not UTF-8".into()))?;
        let history: HistoryLayers = toml::from_str(history_text)
            .map_err(|error| AtlasError::Corrupt(format!("history TOML: {error}")))?;
        let geology_bytes = read_bounded(&planet_dir.join(GEOLOGY_FILE), MAX_GEOLOGY_BYTES)?;
        if stable_hash(&geology_bytes) != manifest.geology_checksum
            || geology_bytes.len() as u64 != manifest.geology_bytes
        {
            return Err(AtlasError::Corrupt(
                "geology manifest does not match the committed manifest".into(),
            ));
        }
        let geology_text = std::str::from_utf8(&geology_bytes)
            .map_err(|_| AtlasError::Corrupt("geology manifest is not UTF-8".into()))?;
        let geology: GeologyModel = toml::from_str(geology_text)
            .map_err(|error| AtlasError::Corrupt(format!("geology TOML: {error}")))?;
        let hydrology_bytes = read_bounded(&planet_dir.join(HYDROLOGY_FILE), MAX_HYDROLOGY_BYTES)?;
        if stable_hash(&hydrology_bytes) != manifest.hydrology_checksum
            || hydrology_bytes.len() as u64 != manifest.hydrology_bytes
        {
            return Err(AtlasError::Corrupt(
                "hydrology manifest does not match the committed manifest".into(),
            ));
        }
        let hydrology_text = std::str::from_utf8(&hydrology_bytes)
            .map_err(|_| AtlasError::Corrupt("hydrology manifest is not UTF-8".into()))?;
        let hydrology: HydrologyModel = toml::from_str(hydrology_text)
            .map_err(|error| AtlasError::Corrupt(format!("hydrology TOML: {error}")))?;
        let biome_bytes = read_bounded(&planet_dir.join(BIOMES_FILE), MAX_BIOMES_BYTES)?;
        if stable_hash(&biome_bytes) != manifest.biome_checksum
            || biome_bytes.len() as u64 != manifest.biome_bytes
        {
            return Err(AtlasError::Corrupt(
                "biome/country manifest does not match the committed manifest".into(),
            ));
        }
        let biome_text = std::str::from_utf8(&biome_bytes)
            .map_err(|_| AtlasError::Corrupt("biome manifest is not UTF-8".into()))?;
        let biomes: BiomeModel = toml::from_str(biome_text)
            .map_err(|error| AtlasError::Corrupt(format!("biome TOML: {error}")))?;
        let water_path = planet_dir.join(WATER_CYCLE_FILE);
        let water_backup = planet_dir.join(WATER_CYCLE_BACKUP_FILE);
        let load_water_payload =
            |path: &Path, expected_checksum: Option<u64>| -> Result<Vec<u8>, AtlasError> {
                let payload = decode_variable_container(
                    path,
                    WATER_CYCLE_MAGIC,
                    WATER_CYCLE_SCHEMA_VERSION,
                    manifest.atlas_face_side,
                    MAX_WATER_CYCLE_BYTES,
                )?;
                if expected_checksum.is_some_and(|checksum| stable_hash(&payload) != checksum) {
                    return Err(AtlasError::Corrupt(format!(
                        "{} does not match the committed water-cycle checksum",
                        path.display()
                    )));
                }
                Ok(payload)
            };
        let water_cycle = match load_water_payload(&water_path, Some(manifest.water_cycle_checksum))
        {
            Ok(payload) => decode_water_cycle(manifest.atlas_face_side, &payload)?,
            Err(primary_error) => match load_water_payload(&water_backup, None) {
                Ok(payload) => {
                    let restored = variable_container_bytes(
                        WATER_CYCLE_MAGIC,
                        WATER_CYCLE_SCHEMA_VERSION,
                        manifest.atlas_face_side,
                        &payload,
                    )?;
                    manifest.water_cycle_checksum = stable_hash(&payload);
                    manifest.water_cycle_bytes = restored.len() as u64;
                    let dynamic_backup_payload = decode_container(
                        &planet_dir.join(DYNAMIC_BACKUP_FILE),
                        DYNAMIC_MAGIC,
                        ATLAS_DYNAMIC_VERSION,
                        manifest.atlas_face_side,
                        DYNAMIC_RECORD_BYTES,
                        MAX_DYNAMIC_BYTES,
                    )?;
                    let dynamic_restored = container_bytes(
                        DYNAMIC_MAGIC,
                        ATLAS_DYNAMIC_VERSION,
                        manifest.atlas_face_side,
                        DYNAMIC_RECORD_BYTES,
                        &dynamic_backup_payload,
                    )?;
                    manifest.dynamic_checksum = stable_hash(&dynamic_backup_payload);
                    manifest.dynamic_bytes = dynamic_restored.len() as u64;
                    dynamic = decode_dynamic(manifest.atlas_face_side, &dynamic_backup_payload)?;
                    crate::persist::atomic_write(&water_path, &restored, false)?;
                    crate::persist::atomic_write(
                        &planet_dir.join(DYNAMIC_FILE),
                        &dynamic_restored,
                        false,
                    )?;
                    write_manifest(planet_dir, &manifest)?;
                    eprintln!(
                        "warning: restored planetary water state from backup after: {primary_error}"
                    );
                    decode_water_cycle(manifest.atlas_face_side, &payload)?
                }
                Err(backup_error) => {
                    return Err(AtlasError::Corrupt(format!(
                        "water ledger is unrecoverable; refusing to reset planetary mass (primary: {primary_error}; backup: {backup_error})"
                    )));
                }
            },
        };
        let atlas = Self {
            manifest,
            genesis,
            dynamic,
            water_cycle,
            history,
            geology,
            hydrology,
            biomes,
        };
        atlas.validate()?;
        Ok(atlas)
    }
}
