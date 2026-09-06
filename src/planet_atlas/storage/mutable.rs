//! Mutable atmosphere, water, and history checkpoint writes.

use crate::planet_atlas::codec::container::{
    container_bytes_owned, variable_container_bytes_owned,
};
use crate::planet_atlas::codec::dynamic::encode_dynamic;
use crate::planet_atlas::codec::models::encode_history;
use crate::planet_atlas::codec::{DYNAMIC_MAGIC, DYNAMIC_RECORD_BYTES, WATER_CYCLE_MAGIC};
use crate::planet_atlas::identity::stable_hash;
use crate::planet_atlas::storage::{
    DYNAMIC_BACKUP_FILE, DYNAMIC_FILE, HISTORY_FILE, MAX_DYNAMIC_BYTES, MAX_HISTORY_BYTES,
    MAX_WATER_CYCLE_BYTES, WATER_CYCLE_BACKUP_FILE, WATER_CYCLE_FILE, read_bounded, write_manifest,
};
use crate::planet_atlas::{
    ATLAS_DYNAMIC_VERSION, AtlasError, DynamicLayers, PlanetAtlas, WATER_CYCLE_SCHEMA_VERSION,
    WaterCycleState, encode_water_cycle,
};
use std::path::Path;

impl PlanetAtlas {
    /// Atomically replace mutable hydrology/climate state while preserving a
    /// last-known file for interrupted-write recovery.
    pub fn save_dynamic(&mut self, world_dir: &Path) -> Result<(), AtlasError> {
        let planet_dir = Self::planet_dir(world_dir);
        let path = planet_dir.join(DYNAMIC_FILE);
        let water_path = planet_dir.join(WATER_CYCLE_FILE);
        if let Ok(previous) = read_bounded(&path, MAX_DYNAMIC_BYTES) {
            crate::persist::atomic_write(&planet_dir.join(DYNAMIC_BACKUP_FILE), &previous, false)?;
        }
        if let Ok(previous) = read_bounded(&water_path, MAX_WATER_CYCLE_BYTES) {
            crate::persist::atomic_write(
                &planet_dir.join(WATER_CYCLE_BACKUP_FILE),
                &previous,
                false,
            )?;
        }
        let payload = encode_dynamic(&self.dynamic)?;
        let water_payload = encode_water_cycle(&self.water_cycle)?;
        self.manifest.dynamic_checksum = stable_hash(&payload);
        self.manifest.water_cycle_checksum = stable_hash(&water_payload);
        let container = container_bytes_owned(
            DYNAMIC_MAGIC,
            ATLAS_DYNAMIC_VERSION,
            self.side(),
            DYNAMIC_RECORD_BYTES,
            payload,
        )?;
        self.manifest.dynamic_bytes = container.len() as u64;
        let water_container = variable_container_bytes_owned(
            WATER_CYCLE_MAGIC,
            WATER_CYCLE_SCHEMA_VERSION,
            self.side(),
            water_payload,
        )?;
        self.manifest.water_cycle_bytes = water_container.len() as u64;
        crate::persist::atomic_write(&path, &container, false)?;
        crate::persist::atomic_write(&water_path, &water_container, false)?;
        write_manifest(&planet_dir, &self.manifest)
    }

    /// Persist authoritative mutable state owned by the running world while
    /// immutable genesis remains shared with chunk workers.
    pub fn save_dynamic_snapshot(
        &self,
        world_dir: &Path,
        dynamic: &DynamicLayers,
        water_cycle: &WaterCycleState,
    ) -> Result<(), AtlasError> {
        if dynamic.cells.side() != self.side() || dynamic.cells.len() != self.dynamic.cells.len() {
            return Err(AtlasError::InvalidDimensions {
                side: dynamic.cells.side(),
                count: dynamic.cells.len(),
            });
        }
        let planet_dir = Self::planet_dir(world_dir);
        let path = planet_dir.join(DYNAMIC_FILE);
        let water_path = planet_dir.join(WATER_CYCLE_FILE);
        if let Ok(previous) = read_bounded(&path, MAX_DYNAMIC_BYTES) {
            crate::persist::atomic_write(&planet_dir.join(DYNAMIC_BACKUP_FILE), &previous, false)?;
        }
        if let Ok(previous) = read_bounded(&water_path, MAX_WATER_CYCLE_BYTES) {
            crate::persist::atomic_write(
                &planet_dir.join(WATER_CYCLE_BACKUP_FILE),
                &previous,
                false,
            )?;
        }
        let payload = encode_dynamic(dynamic)?;
        let water_payload = encode_water_cycle(water_cycle)?;
        let dynamic_checksum = stable_hash(&payload);
        let water_cycle_checksum = stable_hash(&water_payload);
        let container = container_bytes_owned(
            DYNAMIC_MAGIC,
            ATLAS_DYNAMIC_VERSION,
            self.side(),
            DYNAMIC_RECORD_BYTES,
            payload,
        )?;
        let mut manifest = self.manifest.clone();
        manifest.dynamic_checksum = dynamic_checksum;
        manifest.dynamic_bytes = container.len() as u64;
        let water_container = variable_container_bytes_owned(
            WATER_CYCLE_MAGIC,
            WATER_CYCLE_SCHEMA_VERSION,
            self.side(),
            water_payload,
        )?;
        manifest.water_cycle_checksum = water_cycle_checksum;
        manifest.water_cycle_bytes = water_container.len() as u64;
        crate::persist::atomic_write(&path, &container, false)?;
        crate::persist::atomic_write(&water_path, &water_container, false)?;
        write_manifest(&planet_dir, &manifest)
    }

    pub fn save_history(&mut self, world_dir: &Path) -> Result<(), AtlasError> {
        let planet_dir = Self::planet_dir(world_dir);
        let payload = encode_history(&self.history)?;
        if payload.len() as u64 > MAX_HISTORY_BYTES {
            return Err(AtlasError::Corrupt(format!(
                "history is {} bytes; limit is {MAX_HISTORY_BYTES}",
                payload.len()
            )));
        }
        self.manifest.history_checksum = stable_hash(payload.as_bytes());
        crate::persist::atomic_write(&planet_dir.join(HISTORY_FILE), payload.as_bytes(), false)?;
        write_manifest(&planet_dir, &self.manifest)
    }
}
