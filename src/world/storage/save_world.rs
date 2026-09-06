//! Save world storage transaction coordination.

use crate::chunk::ChunkPos;
use crate::world::SaveFailure;
use crate::world::SaveReport;
use crate::world::World;
use std::fs;
use crate::world::write_world_meta_full;

impl World {
    pub fn save_modified(&mut self) -> SaveReport {
        let mut report = SaveReport::default();
        report.record(
            "save directory",
            self.save_dir.clone(),
            fs::create_dir_all(&self.save_dir),
        );
        let meta_path = self.save_dir.join("world.toml");
        report.record(
            "world metadata",
            meta_path,
            write_world_meta_full(
                &self.save_dir,
                self.seed,
                &self.mode,
                self.ire,
                self.calendar_state.day(),
                &self.camera,
            ),
        );
        if let (Some(atlas), Some(weather)) = (&self.planet_atlas, self.weather_state.live()) {
            report.record(
                "planetary weather",
                crate::planet_atlas::PlanetAtlas::planet_dir(&self.save_dir).join("dynamic.wfd"),
                atlas
                    .save_dynamic_snapshot(&self.save_dir, &weather.cells, &weather.water)
                    .map_err(std::io::Error::other),
            );
        }
        if let Some(ledger) = &self.material_ledger {
            report.record(
                "finite-material ledger",
                self.save_dir.join("materials.wfm"),
                ledger.save(),
            );
        }
        if let Some(ledger) = &self.arcane_ledger {
            report.record(
                "finite-Current ledger",
                self.save_dir.join("arcane.wfc"),
                ledger.save().map_err(std::io::Error::other),
            );
        }
        if let Some(geography) = &mut self.arcane_geography {
            report.record(
                "planetary arcane geography",
                crate::planet_atlas::PlanetAtlas::planet_dir(&self.save_dir)
                    .join("arcane-geography.wad"),
                geography
                    .save_dynamic(&self.save_dir)
                    .map_err(std::io::Error::other),
            );
        }
        // One owner publishes extensions for both full saves and direct chunk
        // writes. Stored names are never renumbered when content changes.
        let palette_ready = report.record(
            "block palette",
            self.save_dir.join("palette"),
            self.palette.publish(&self.reg).map(|_| ()),
        );
        let path = self.entities_path();
        report.record("block entities", path, self.save_entities());
        let path = self.save_dir.join("discovery.toml");
        let discovery_result = self
            .discovery_state
            .as_mut()
            .map_or(Ok(()), |state| state.save().map_err(std::io::Error::other));
        report.record("discovery state", path, discovery_result);
        let path = self.save_dir.join(crate::implements::IMPLEMENTS_FILE);
        let implements_result = self
            .implements_state
            .as_ref()
            .map_or(Ok(()), |state| state.save().map_err(std::io::Error::other));
        report.record("implement state", path, implements_result);
        let path = self.save_dir.join(crate::workings::WORKINGS_FILE);
        let workings_result = self
            .workings_state
            .as_ref()
            .map_or(Ok(()), |state| state.save().map_err(std::io::Error::other));
        report.record("working transaction state", path, workings_result);
        let path = self.save_dir.join(crate::alchemy::ALCHEMY_FILE);
        let alchemy_result = self
            .alchemy_state
            .as_ref()
            .map_or(Ok(()), |state| state.save().map_err(std::io::Error::other));
        report.record("alchemy state", path, alchemy_result);
        let path = self.save_dir.join(crate::workings::WATER_CARRIERS_FILE);
        let carrier_result = self
            .water_carriers
            .as_ref()
            .map_or(Ok(()), |state| state.save().map_err(std::io::Error::other));
        report.record("detailed water carriers", path, carrier_result);
        report.record(
            "host-owned loose items",
            self.save_dir.join("loose-items.toml"),
            self.save_loose_items(),
        );
        report.extend(self.save_mobs());
        let path = self.save_dir.join("stamps");
        report.record("random-tick stamps", path, self.save_stamps());
        report.record(
            "template library",
            self.save_dir.join("templates.toml"),
            self.save_templates(),
        );
        report.record(
            "local structures",
            self.save_dir.join("local_structures.toml"),
            self.save_local_structures(),
        );
        let dirty: Vec<ChunkPos> = self
            .chunks
            .iter()
            .filter(|(_, chunk)| chunk.modified)
            .map(|(pos, _)| *pos)
            .collect();
        for pos in dirty {
            // New stored IDs cannot reach disk before their names. Existing
            // IDs keep their meaning even when unloaded chunks are untouched.
            if !palette_ready {
                continue;
            }
            // Clear only on a write that landed: a chunk whose file could
            // not be written stays queued for the next save.
            match self.save_chunk(pos) {
                Ok(()) => {
                    report.chunks_saved += 1;
                    if let Some(chunk) = self.chunks.get_mut(&pos) {
                        chunk.modified = false;
                    }
                }
                Err(error) => report.failures.push(SaveFailure::new(
                    format!("chunk {pos:?}"),
                    crate::world::region::region_path(&self.save_dir, pos),
                    error,
                )),
            }
        }
        report
    }
}
