//! Entry for the common spawn contract.

use std::sync::Arc;
use crate::chunk::CHUNK_Y;
use crate::planet::FACE_BLOCKS;
use crate::planet::Face;
use crate::planet::SurfacePos;
use crate::world::WORLD_GENERATOR_VERSION;
use crate::world::WORLD_TOPOLOGY;
use crate::world::World;
use crate::world::preparation::check_cancelled;
use std::fs;
use crate::world::preparation::generate_trial_region;
use super::ENTRY_RADIUS_CHUNKS;
use super::MAX_SPAWN_MANIFEST_BYTES;
use super::SPAWN_MANIFEST_VERSION;
use super::SPAWN_VERIFICATION_VERSION;
use super::SpawnManifest;
use super::entry_chunks;
use super::prepared_chunk_digest;
use super::qualified_atlas_candidates;
use super::qualify_trial_region;
use super::spawn_candidate_portfolio;
use super::validate_spawn_ledgers;

impl World {
    pub fn qualified_spawn_surface(&self) -> Option<SurfacePos> {
        let Some(atlas) = self.planet_atlas.as_deref() else {
            // Atlas-free fixtures retain a deterministic local doorstep.
            return SurfacePos::new(Face::PosZ, FACE_BLOCKS / 2, FACE_BLOCKS / 2).ok();
        };
        let (candidates, _) = qualified_atlas_candidates(atlas);
        candidates.first().map(|candidate| candidate.surface)
    }

    /// Load or create the world's persisted, fully materialized common
    /// doorstep. The manifest is published only after all entry chunks and
    /// conservation ledgers are durable.
    pub fn prepare_common_spawn(
        &mut self,
        progress: impl FnMut(&str, usize, usize),
    ) -> std::io::Result<crate::planet::EntityPos> {
        self.prepare_common_spawn_cancellable(
            &crate::planet_atlas::CancellationToken::default(),
            progress,
        )
    }

    pub(crate) fn prepare_common_spawn_cancellable(
        &mut self,
        cancel: &crate::planet_atlas::CancellationToken,
        mut progress: impl FnMut(&str, usize, usize),
    ) -> std::io::Result<crate::planet::EntityPos> {
        check_cancelled(cancel)?;
        let path = self.save_dir.join("spawn.toml");
        if path.is_file() {
            let metadata = fs::metadata(&path)?;
            if metadata.len() > MAX_SPAWN_MANIFEST_BYTES {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "spawn manifest exceeds its size bound",
                ));
            }
            let text = fs::read_to_string(&path)?;
            let manifest: SpawnManifest = toml::from_str(&text).map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid spawn manifest: {error}"),
                )
            })?;
            let atlas = self
                .planet_atlas
                .clone()
                .ok_or_else(|| std::io::Error::other("planetary spawn requires an atlas"))?;
            if manifest.version != SPAWN_MANIFEST_VERSION
                || manifest.topology != WORLD_TOPOLOGY
                || manifest.face_blocks != FACE_BLOCKS
                || manifest.world_height != CHUNK_Y as u16
                || manifest.seed != self.seed
                || !(crate::world::MIN_SUPPORTED_WORLD_GENERATOR_VERSION..=WORLD_GENERATOR_VERSION)
                    .contains(&manifest.generator_version)
                || manifest.atlas_format_version != crate::planet_atlas::ATLAS_FORMAT_VERSION
                || manifest.atlas_algorithm_version > crate::planet_atlas::ATLAS_ALGORITHM_VERSION
                || manifest.atlas_checksum != atlas.manifest.genesis_checksum
                || manifest.content_hash != atlas.manifest.content_hash
                || manifest.spawn_surface != manifest.spawn.surface()
                || manifest.prepared_radius_chunks != ENTRY_RADIUS_CHUNKS
                || manifest.chunks != entry_chunks(manifest.spawn_surface)
                || manifest.verification.contract_version != SPAWN_VERIFICATION_VERSION
                || manifest.prepared_digest == 0
                || manifest.completed_unix_seconds == 0
                || manifest.verification.walkable_cells < 32
                || !manifest.verification.safe_standing
                || !manifest.verification.reachable_wood
                || !manifest.verification.reachable_fresh_water
                || !manifest.verification.reachable_soil
                || !manifest.verification.reachable_stone
                || !manifest.verification.reachable_plants
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "spawn manifest is incompatible with this planet",
                ));
            }
            // `prepared_digest` is immutable creation evidence, not an
            // eternal checksum: ecology, fluids, fire, and player edits may
            // legitimately change these chunks after publication. Presence,
            // decoding, ledger validation, and a live safe-entry check below
            // distinguish a usable evolved homeland from corruption without
            // rejecting ordinary play.
            for (index, position) in manifest.chunks.iter().copied().enumerate() {
                check_cancelled(cancel)?;
                self.try_ensure_chunk(position)?;
                if !self.has_chunk(position) {
                    return Err(std::io::Error::other(format!(
                        "could not load homeland chunk {position:?}"
                    )));
                }
                progress(
                    "loading prepared homeland",
                    index + 1,
                    manifest.chunks.len(),
                );
            }
            validate_spawn_ledgers(self)?;
            let spawn = self.prepared_spawn_position(manifest.spawn).ok_or_else(|| {
                std::io::Error::other(
                    "prepared homeland no longer contains a dry safe entry; explicit spawn repair is required",
                )
            })?;
            let region = atlas.atlas_pos(spawn.surface());
            if !self
                .arcane_geography
                .as_ref()
                .is_some_and(|geography| geography.early_discovery_reachable(&atlas, region))
            {
                return Err(std::io::Error::other(
                    "prepared homeland has no same-landmass early magical observation site",
                ));
            }
            check_cancelled(cancel)?;
            // Complete any durable retrogen transaction even if cancellation
            // arrives during it; the caller will then discard session entry.
            let changed = self.ensure_spawn_discovery_sites(spawn.surface())?;
            if !changed.is_empty() {
                // A structure origin can write through a neighboring chunk.
                // Persist the complete prepared homeland so retrogen cannot
                // leave half an outpost only resident in memory.
                for position in entry_chunks(spawn.surface()) {
                    self.save_chunk(position)?;
                }
                let save = self.save_modified();
                if !save.is_ok() {
                    return Err(std::io::Error::other(format!(
                        "could not persist discovery-site retrogen: {}",
                        save.summary()
                    )));
                }
            }
            self.common_spawn = Some(spawn);
            return Ok(spawn);
        }

        let atlas = self
            .planet_atlas
            .clone()
            .ok_or_else(|| std::io::Error::other("planetary spawn requires an atlas"))?;
        progress("selecting homeland", 0, 1);
        let (candidates, selection_diagnostics) = qualified_atlas_candidates(&atlas);
        let candidates = spawn_candidate_portfolio(&candidates);
        if candidates.is_empty() {
            return Err(std::io::Error::other(format!(
                "planet has no dry forest homeland with fresh water ({selection_diagnostics})",
            )));
        }

        let mut winner = None;
        // The best safe-but-resource-incomplete homeland seen so far, kept only
        // to keep world creation from dead-ending: if nothing clears the full
        // bar, spawning somewhere lean beats refusing to make the world at all.
        let mut fallback: Option<HomelandTrial> = None;
        let mut fallback_score = (0u32, 0usize);
        let mut rejections = Vec::new();
        for (candidate_index, candidate) in candidates.iter().enumerate() {
            check_cancelled(cancel)?;
            progress("testing homeland", candidate_index, candidates.len());
            let positions = entry_chunks(candidate.surface);
            let generated = generate_trial_region(
                self.seed,
                Arc::clone(&self.reg),
                Arc::clone(&atlas),
                &positions,
                cancel,
                |completed, total| progress("generating homeland", completed, total),
            )?;
            match qualify_trial_region(&self.reg, &atlas, candidate.surface, &generated) {
                Ok(qualification) => {
                    winner = Some((candidate.surface, qualification, generated));
                    break;
                }
                Err(reject) => {
                    rejections.push(format!(
                        "{}:{}:{}: {}",
                        candidate.surface.face().name(),
                        candidate.surface.u(),
                        candidate.surface.v(),
                        reject.reason,
                    ));
                    // Rank incomplete homelands by resources reachable, then by
                    // how much room there is to walk. Only used if no candidate
                    // clears the full bar.
                    if let Some(effort) = reject.fallback {
                        let score = (
                            effort.verification.resource_score(),
                            effort.verification.walkable_cells,
                        );
                        if fallback.is_none() || score > fallback_score {
                            fallback_score = score;
                            fallback = Some((candidate.surface, effort, generated));
                        }
                    }
                }
            }
        }
        let (wanted, qualification, generated) = match winner.or(fallback) {
            Some(chosen) => chosen,
            None => {
                return Err(std::io::Error::other(format!(
                    "no candidate homeland had a safe standing doorstep: {}",
                    rejections.join("; ")
                )));
            }
        };
        if !qualification.verification.is_fully_qualified() {
            let v = &qualification.verification;
            eprintln!(
                "spawn: no homeland cleared the full resource bar for seed {}; \
                 using the best safe doorstep at {}:{}:{} \
                 (walkable={} wood={} water={} soil={} stone={} plants={}); \
                 {} candidate(s) rejected",
                self.seed,
                wanted.face().name(),
                wanted.u(),
                wanted.v(),
                v.walkable_cells,
                v.reachable_wood,
                v.reachable_fresh_water,
                v.reachable_soil,
                v.reachable_stone,
                v.reachable_plants,
                rejections.len(),
            );
        }
        let spawn_surface = qualification.spawn;

        // Once adoption starts, complete the ledger/chunk/manifest publication
        // as one preparation operation; do not abandon partially saved state.
        check_cancelled(cancel)?;
        let total = generated.len();
        for (index, (position, chunk)) in generated.into_iter().enumerate() {
            self.adopt_generated(position, chunk);
            progress("committing homeland", index + 1, total);
        }
        debug_assert_eq!(
            entry_chunks(wanted),
            entry_chunks(spawn_surface),
            "voxel refinement remains in the candidate's center chunk"
        );
        let height = self.surface_height_at(spawn_surface) + 1;
        let spawn = crate::planet::EntityPos::new(
            spawn_surface.face(),
            spawn_surface.u() as f32 + 0.5,
            height as f32 + 0.2,
            spawn_surface.v() as f32 + 0.5,
        )
        .expect("prepared spawn cell is canonical");
        let region = atlas.atlas_pos(spawn.surface());
        if !self
            .arcane_geography
            .as_ref()
            .is_some_and(|geography| geography.early_discovery_reachable(&atlas, region))
        {
            return Err(std::io::Error::other(
                "qualified homeland has no same-landmass early magical observation site",
            ));
        }
        let _ = self.ensure_spawn_discovery_sites(spawn.surface())?;

        let initial_save = self.save_modified();
        if !initial_save.is_ok() {
            return Err(std::io::Error::other(format!(
                "could not persist homeland ledgers: {}",
                initial_save.summary()
            )));
        }
        let chunks = entry_chunks(spawn.surface());
        for (index, position) in chunks.iter().copied().enumerate() {
            self.save_chunk(position)?;
            progress("persisting homeland", index + 1, chunks.len());
        }
        let final_save = self.save_modified();
        if !final_save.is_ok() {
            return Err(std::io::Error::other(format!(
                "could not finalize homeland ledgers: {}",
                final_save.summary()
            )));
        }
        validate_spawn_ledgers(self)?;
        let prepared_digest = prepared_chunk_digest(&self.region_store, &chunks)?;
        let completed_unix_seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let manifest = SpawnManifest {
            version: SPAWN_MANIFEST_VERSION,
            topology: WORLD_TOPOLOGY.to_string(),
            face_blocks: FACE_BLOCKS,
            world_height: CHUNK_Y as u16,
            seed: self.seed,
            generator_version: WORLD_GENERATOR_VERSION,
            atlas_format_version: crate::planet_atlas::ATLAS_FORMAT_VERSION,
            atlas_algorithm_version: crate::planet_atlas::ATLAS_ALGORITHM_VERSION,
            atlas_checksum: atlas.manifest.genesis_checksum,
            content_hash: atlas.manifest.content_hash,
            spawn_surface,
            spawn,
            prepared_radius_chunks: ENTRY_RADIUS_CHUNKS,
            chunks,
            prepared_digest,
            verification: qualification.verification,
            completed_unix_seconds,
        };
        let text = toml::to_string_pretty(&manifest)
            .map_err(|error| std::io::Error::other(format!("spawn manifest: {error}")))?;
        crate::persist::atomic_write(&path, text.as_bytes(), false)?;
        progress("homeland ready", 1, 1);
        self.common_spawn = Some(spawn);
        Ok(spawn)
    }

}
