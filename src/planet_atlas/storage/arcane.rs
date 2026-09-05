//! Arcane ledger and geography manifest checkpoint adapters.

use crate::planet_atlas::{AtlasError, AtlasManifest, PlanetAtlas};
use crate::planet_atlas::storage::{MANIFEST_FILE, MAX_MANIFEST_BYTES, read_bounded, write_manifest};
use std::path::{Path};

pub(crate) struct ArcaneManifestCheckpoint {
    pub schema_version: u32,
    pub algorithm_version: u32,
    pub unit_scale: u32,
    pub genesis_total: u64,
    pub last_clean_total: u64,
    pub reservoir_totals: [u64; 6],
    pub registry_hash: u64,
    pub ledger_checksum: u64,
    pub delta_checksum: u64,
}

pub(crate) fn update_arcane_manifest(
    world_dir: &Path,
    checkpoint: &ArcaneManifestCheckpoint,
) -> Result<(), AtlasError> {
    let planet_dir = PlanetAtlas::planet_dir(world_dir);
    let bytes = read_bounded(&planet_dir.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| AtlasError::Corrupt(format!("manifest is not UTF-8: {error}")))?;
    let mut manifest: AtlasManifest = toml::from_str(text)
        .map_err(|error| AtlasError::Corrupt(format!("manifest decode failed: {error}")))?;
    manifest.arcane_schema_version = checkpoint.schema_version;
    manifest.arcane_algorithm_version = checkpoint.algorithm_version;
    manifest.arcane_unit_scale = checkpoint.unit_scale;
    manifest.arcane_genesis_total = checkpoint.genesis_total;
    manifest.arcane_last_clean_total = checkpoint.last_clean_total;
    manifest.arcane_reservoir_totals = checkpoint.reservoir_totals;
    manifest.arcane_registry_hash = checkpoint.registry_hash;
    manifest.arcane_ledger_checksum = checkpoint.ledger_checksum;
    manifest.arcane_delta_checksum = checkpoint.delta_checksum;
    write_manifest(&planet_dir, &manifest)
}

pub(crate) fn verify_arcane_manifest_checkpoint(
    world_dir: &Path,
    checkpoint: &ArcaneManifestCheckpoint,
) -> Result<(), AtlasError> {
    let planet_dir = PlanetAtlas::planet_dir(world_dir);
    let bytes = read_bounded(&planet_dir.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| AtlasError::Corrupt(format!("manifest is not UTF-8: {error}")))?;
    let manifest: AtlasManifest = toml::from_str(text)
        .map_err(|error| AtlasError::Corrupt(format!("manifest decode failed: {error}")))?;
    let valid = manifest.arcane_schema_version == checkpoint.schema_version
        && manifest.arcane_algorithm_version == checkpoint.algorithm_version
        && manifest.arcane_unit_scale == checkpoint.unit_scale
        && manifest.arcane_genesis_total == checkpoint.genesis_total
        && manifest.arcane_last_clean_total == checkpoint.last_clean_total
        && manifest.arcane_reservoir_totals == checkpoint.reservoir_totals
        && manifest.arcane_registry_hash == checkpoint.registry_hash
        && manifest.arcane_ledger_checksum == checkpoint.ledger_checksum;
    if !valid {
        return Err(AtlasError::Corrupt(
            "arcane checkpoint does not match the qualified planet manifest".into(),
        ));
    }
    Ok(())
}

pub(crate) struct ArcaneGeographyManifestCheckpoint {
    pub schema_version: u32,
    pub algorithm_version: u32,
    pub dynamic_version: u32,
    pub genesis_total: u64,
    pub immutable_checksum: u64,
    pub dynamic_checksum: u64,
    pub site_catalog_checksum: u64,
    pub last_authoritative_time: u64,
}

pub(crate) fn update_arcane_geography_manifest(
    world_dir: &Path,
    checkpoint: &ArcaneGeographyManifestCheckpoint,
) -> Result<(), AtlasError> {
    let planet_dir = PlanetAtlas::planet_dir(world_dir);
    let bytes = read_bounded(&planet_dir.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| AtlasError::Corrupt(format!("manifest is not UTF-8: {error}")))?;
    let mut manifest: AtlasManifest = toml::from_str(text)
        .map_err(|error| AtlasError::Corrupt(format!("manifest decode failed: {error}")))?;
    manifest.arcane_geography_schema_version = checkpoint.schema_version;
    manifest.arcane_geography_algorithm_version = checkpoint.algorithm_version;
    manifest.arcane_geography_dynamic_version = checkpoint.dynamic_version;
    manifest.arcane_geography_genesis_total = checkpoint.genesis_total;
    manifest.arcane_geography_immutable_checksum = checkpoint.immutable_checksum;
    manifest.arcane_geography_dynamic_checksum = checkpoint.dynamic_checksum;
    manifest.arcane_geography_site_catalog_checksum = checkpoint.site_catalog_checksum;
    manifest.arcane_geography_last_authoritative_time = checkpoint.last_authoritative_time;
    write_manifest(&planet_dir, &manifest)
}

pub(crate) fn arcane_geography_manifest_payload(
    world_dir: &Path,
    checkpoint: &ArcaneGeographyManifestCheckpoint,
) -> Result<Vec<u8>, AtlasError> {
    let planet_dir = PlanetAtlas::planet_dir(world_dir);
    let bytes = read_bounded(&planet_dir.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| AtlasError::Corrupt(format!("manifest is not UTF-8: {error}")))?;
    let mut manifest: AtlasManifest = toml::from_str(text)
        .map_err(|error| AtlasError::Corrupt(format!("manifest decode failed: {error}")))?;
    manifest.arcane_geography_schema_version = checkpoint.schema_version;
    manifest.arcane_geography_algorithm_version = checkpoint.algorithm_version;
    manifest.arcane_geography_dynamic_version = checkpoint.dynamic_version;
    manifest.arcane_geography_genesis_total = checkpoint.genesis_total;
    manifest.arcane_geography_immutable_checksum = checkpoint.immutable_checksum;
    manifest.arcane_geography_dynamic_checksum = checkpoint.dynamic_checksum;
    manifest.arcane_geography_site_catalog_checksum = checkpoint.site_catalog_checksum;
    manifest.arcane_geography_last_authoritative_time = checkpoint.last_authoritative_time;
    let payload = toml::to_string_pretty(&manifest)
        .map_err(|error| AtlasError::Corrupt(format!("manifest encoding failed: {error}")))?;
    if payload.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(AtlasError::Corrupt(
            "manifest exceeds its size limit".into(),
        ));
    }
    Ok(payload.into_bytes())
}

pub(crate) fn verify_arcane_geography_manifest(
    world_dir: &Path,
    checkpoint: &ArcaneGeographyManifestCheckpoint,
) -> Result<(), AtlasError> {
    let planet_dir = PlanetAtlas::planet_dir(world_dir);
    let bytes = read_bounded(&planet_dir.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| AtlasError::Corrupt(format!("manifest is not UTF-8: {error}")))?;
    let manifest: AtlasManifest = toml::from_str(text)
        .map_err(|error| AtlasError::Corrupt(format!("manifest decode failed: {error}")))?;
    let valid = manifest.arcane_geography_schema_version == checkpoint.schema_version
        && manifest.arcane_geography_algorithm_version == checkpoint.algorithm_version
        && manifest.arcane_geography_dynamic_version == checkpoint.dynamic_version
        && manifest.arcane_geography_genesis_total == checkpoint.genesis_total
        && manifest.arcane_geography_immutable_checksum == checkpoint.immutable_checksum
        && manifest.arcane_geography_dynamic_checksum == checkpoint.dynamic_checksum
        && manifest.arcane_geography_site_catalog_checksum == checkpoint.site_catalog_checksum
        && manifest.arcane_geography_last_authoritative_time == checkpoint.last_authoritative_time;
    if !valid {
        return Err(AtlasError::Corrupt(
            "arcane geography checkpoint does not match the planet manifest".into(),
        ));
    }
    Ok(())
}
