//! Initial atlas bundle writes with the complete manifest committed last.

use crate::planet_atlas::codec::container::{
    container_bytes_owned, variable_container_bytes_owned,
};
use crate::planet_atlas::codec::dynamic::encode_dynamic;
use crate::planet_atlas::codec::genesis::encode_genesis;
use crate::planet_atlas::codec::models::{
    encode_biomes, encode_geology, encode_history, encode_hydrology,
};
use crate::planet_atlas::codec::{
    DYNAMIC_MAGIC, DYNAMIC_RECORD_BYTES, GENESIS_MAGIC, GENESIS_RECORD_BYTES, WATER_CYCLE_MAGIC,
};
use crate::planet_atlas::storage::{
    BIOMES_FILE, DYNAMIC_FILE, GENESIS_FILE, GEOLOGY_FILE, HISTORY_FILE, HYDROLOGY_FILE,
    MAX_BIOMES_BYTES, MAX_DYNAMIC_BYTES, MAX_GENESIS_BYTES, MAX_GEOLOGY_BYTES, MAX_HISTORY_BYTES,
    MAX_HYDROLOGY_BYTES, MAX_WATER_CYCLE_BYTES, WATER_CYCLE_FILE, write_manifest,
};
use crate::planet_atlas::{
    ATLAS_DYNAMIC_VERSION, ATLAS_FORMAT_VERSION, AtlasError, PlanetAtlas,
    WATER_CYCLE_SCHEMA_VERSION, encode_water_cycle,
};
use std::path::Path;

impl PlanetAtlas {
    pub(super) fn write_bundle(&self, planet_dir: &Path) -> Result<(), AtlasError> {
        {
            let payload = encode_genesis(&self.genesis)?;
            let container = container_bytes_owned(
                GENESIS_MAGIC,
                ATLAS_FORMAT_VERSION,
                self.side(),
                GENESIS_RECORD_BYTES,
                payload,
            )?;
            if container.len() as u64 > MAX_GENESIS_BYTES {
                return Err(AtlasError::Corrupt(
                    "genesis exceeds its file budget".into(),
                ));
            }
            crate::persist::atomic_write(&planet_dir.join(GENESIS_FILE), &container, false)?;
        }
        {
            let payload = encode_dynamic(&self.dynamic)?;
            let container = container_bytes_owned(
                DYNAMIC_MAGIC,
                ATLAS_DYNAMIC_VERSION,
                self.side(),
                DYNAMIC_RECORD_BYTES,
                payload,
            )?;
            if container.len() as u64 > MAX_DYNAMIC_BYTES {
                return Err(AtlasError::Corrupt(
                    "dynamic atmosphere exceeds its file budget".into(),
                ));
            }
            crate::persist::atomic_write(&planet_dir.join(DYNAMIC_FILE), &container, false)?;
        }
        {
            let payload = encode_water_cycle(&self.water_cycle)?;
            let container = variable_container_bytes_owned(
                WATER_CYCLE_MAGIC,
                WATER_CYCLE_SCHEMA_VERSION,
                self.side(),
                payload,
            )?;
            if container.len() as u64 > MAX_WATER_CYCLE_BYTES {
                return Err(AtlasError::Corrupt(
                    "water cycle exceeds its file budget".into(),
                ));
            }
            crate::persist::atomic_write(&planet_dir.join(WATER_CYCLE_FILE), &container, false)?;
        }
        for (file, payload, limit) in [
            (
                HISTORY_FILE,
                encode_history(&self.history)?,
                MAX_HISTORY_BYTES,
            ),
            (
                GEOLOGY_FILE,
                encode_geology(&self.geology)?,
                MAX_GEOLOGY_BYTES,
            ),
            (
                HYDROLOGY_FILE,
                encode_hydrology(&self.hydrology)?,
                MAX_HYDROLOGY_BYTES,
            ),
            (BIOMES_FILE, encode_biomes(&self.biomes)?, MAX_BIOMES_BYTES),
        ] {
            if payload.len() as u64 > limit {
                return Err(AtlasError::Corrupt(format!(
                    "{file} exceeds its file budget"
                )));
            }
            crate::persist::atomic_write(&planet_dir.join(file), payload.as_bytes(), false)?;
        }
        // The complete manifest is the commit marker and is deliberately last.
        write_manifest(planet_dir, &self.manifest)
    }
}
