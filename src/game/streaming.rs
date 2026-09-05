//! Client chunk generation, unloading, remeshing, and GPU upload budgets.
//!
//! Terrain math is pure, so generation runs on background workers; the
//! main thread only adopts finished chunks (light, seams, reconcile), captures
//! immutable mesh inputs, and uploads completed meshes on a per-frame budget.

use super::*;
use crate::terrain_jobs::Priority;

impl Game {
    fn effective_view_dist(&self) -> i32 {
        self.multiplayer
            .remote
            .as_ref()
            .map_or(self.config.view_dist, |remote| {
                remote.granted_view_dist.min(self.config.view_dist)
            })
    }

    fn chunk_mesh_ready(&self, position: ChunkPos, center: ChunkPos, view_dist: i32) -> bool {
        [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().all(|(du, dv)| {
            let neighbor = position.offset(*du, *dv);
            let expected = neighbor.distance(center) <= f64::from(view_dist * CHUNK_X as i32) + 1.0;
            !expected || self.server.world.has_chunk(neighbor)
        })
    }

    /// Outstanding generation/remesh work used to settle deterministic
    /// headless captures before the screenshot fires.
    pub(super) fn chunk_work_pending(&self) -> usize {
        if !self.in_world {
            return 0;
        }
        let Some(center) = self.player.pos.chunk() else {
            return 0;
        };
        let vd = self.effective_view_dist();
        let mut pending = 0;
        for dx in -vd..=vd {
            for dz in -vd..=vd {
                let pos = center.offset(dx, dz);
                if pos.distance(center) <= f64::from(vd * CHUNK_X as i32) + 1.0
                    && !self.server.world.has_chunk(pos)
                {
                    pending += 1;
                }
            }
        }
        // A capture needs every requested chunk to have reached the GPU once.
        // Live water, weather, ecology, and footsteps can immediately dirty an
        // already-visible chunk again; waiting for that churn to reach a
        // simultaneous zero made otherwise-ready captures hit the 3,000-frame
        // timeout on healthy worlds.
        let initial_mesh_in_flight = self.mesh_pool.as_ref().map_or(0, |pool| {
            pool.positions()
                .filter(|position| !self.renderer.has_chunk(*position))
                .count()
        });
        pending
            + self
                .server
                .world
                .dirty_chunks()
                .into_iter()
                .filter(|position| {
                    self.chunk_mesh_ready(*position, center, vd)
                        && !self.renderer.has_chunk(*position)
                })
                .count()
            + initial_mesh_in_flight
    }

    pub(super) fn stream_chunks(&mut self) {
        self.stream_t0 = std::time::Instant::now();
        let Some(center) = self.player.pos.chunk() else {
            return;
        };

        // Generate missing chunks, nearest first.
        let mut wanted: Vec<(i32, ChunkPos)> = Vec::new();
        let vd = self.effective_view_dist();
        for dx in -vd..=vd {
            for dz in -vd..=vd {
                let pos = center.offset(dx, dz);
                if pos.distance(center) > f64::from(vd * CHUNK_X as i32) + 1.0 {
                    continue;
                }
                if !self.server.world.has_chunk(pos) {
                    wanted.push((dx * dx + dz * dz, pos));
                }
            }
        }
        wanted.sort_by_key(|(d, _)| *d);
        // The budgets below were tuned for a seven-chunk view, where the
        // whole loaded set is 225 chunks. A sixty-four chunk view wants
        // 16600 of them, so everything that fills the world scales with
        // the setting or the far half never arrives.
        let vd_us = vd.max(1) as usize;
        let flight = (vd_us * 3).clamp(12, 192);
        let ask = (vd_us * 6).clamp(24, 384);
        // Adoption keeps a wall-clock budget; meshing below has a fixed
        // two-job concurrency budget because a single job can itself exceed a
        // small millisecond allowance.
        const ADOPT_BUDGET_MS: u128 = 3;
        let mut terrain_error = None;
        if let Some(pool) = &mut self.gen_pool {
            pool.reconfigure(
                crate::terrain_jobs::TerrainContext::new(
                    self.server.world.seed,
                    self.server.world.planet_atlas(),
                    self.server.world.chunk_loader(),
                ),
                crate::terrain_jobs::WorkerPolicy::Interactive,
            );
            if let Some(error) = pool.take_failure_notification() {
                terrain_error = Some(format!("Terrain streaming stopped: {error}"));
            }
            pool.cancel_queued(&|position| {
                position.distance(center) <= f64::from(vd * CHUNK_X as i32) + 1.0
            });
            // Keep the workers fed a nearest-first pipeline.
            for (_, pos) in wanted.iter().take(ask) {
                if pool.pending_count() >= flight {
                    break;
                }
                pool.request(*pos, Priority::Ordinary, flight);
            }
            // Adopt what's ready on a time budget. Lighting and seam repair
            // still belong to the authoritative world on this thread, while
            // pure mesh construction runs in the bounded pool below.
            let t0 = self.stream_t0;
            while let Some(result) = pool.try_ready() {
                match result {
                    Ok(prepared) => {
                        let pos = prepared.position;
                        let fresh = prepared.is_fresh();
                        if self.server.world.adopt_prepared_at_revision(
                            pos,
                            prepared.chunk,
                            fresh,
                            &prepared.revision,
                        ) {
                            for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                                self.server.world.mark_chunk_dirty(pos.offset(dx, dz));
                            }
                        }
                    }
                    Err(failure) => {
                        let message = format!(
                            "Could not load terrain {:?}: {}",
                            failure.position, failure.source
                        );
                        eprintln!("{message}");
                        terrain_error = Some(message);
                    }
                }
                if t0.elapsed().as_millis() >= ADOPT_BUDGET_MS {
                    break;
                }
            }
        } else if !self.server.world.is_remote() {
            // No pool (a fresh session mid-setup): the synchronous
            // path stays correct, just slower.
            for (_, pos) in wanted.into_iter().take(GEN_BUDGET) {
                self.server.world.ensure_chunk(pos);
                for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    self.server.world.mark_chunk_dirty(pos.offset(dx, dz));
                }
            }
        }

        if let Some(message) = terrain_error {
            self.set_screen(Screen::Paused);
            self.toast(message);
        }

        // Unload chunks far outside the view radius. The same residency rule
        // the dedicated server runs (World::retain_chunks); here it is spelled
        // out because the renderer and the light cache have to let go too.
        let limit = vd + 2;
        let far = self.server.world.chunks_outside_all(&[center], limit);
        if !far.is_empty() {
            // Save only what leaves; a full save_modified here wrote
            // the whole world (palette, entities, mobs, stamps, every
            // modified chunk) to disk on the main thread every time a
            // single chunk crossed the border — a walking-speed
            // stutter machine. This IS the incremental save now: the
            // timer is gone, and a chunk is written as it leaves the
            // view rather than the whole world on a clock.
            let (report, released) = self.server.world.evict_chunks(far);
            for pos in released {
                self.renderer.drop_chunk(pos);
                self.presentation.lights.chunk_dropped(pos);
            }
            if !report.is_ok() {
                let message = format!("Could not save departing ground: {}", report.summary());
                eprintln!("world: {message}");
                self.toast(message);
            }
        }

        if let Some(error) = self
            .mesh_pool
            .as_mut()
            .and_then(|pool| pool.take_failure_notification())
        {
            eprintln!("mesh workers stopped: {error}");
            self.set_screen(Screen::Paused);
            self.toast(format!("Terrain drawing stopped: {error}"));
        }

        let current_variant_signature = self.content.tile_variants.signature();
        let completed_meshes = if let Some(pool) = &mut self.mesh_pool {
            let mut completed = Vec::new();
            while let Some(result) =
                pool.try_ready(&self.server.world.reg, &current_variant_signature)
            {
                completed.push(result);
            }
            completed
        } else {
            Vec::new()
        };
        for (position, mesh) in completed_meshes {
            let still_current = self
                .server
                .world
                .chunk(position)
                .is_some_and(|chunk| !chunk.dirty);
            if !still_current {
                continue;
            }
            self.renderer.upload_chunk(position, &mesh);
            self.presentation
                .lights
                .chunk_meshed(position, mesh.emitters);
            if position.distance(center) < f64::from(crate::renderer::OCC_GRID as u32) / 2.0 + 16.0
            {
                self.occ_dirty = true;
            }
        }

        // Remesh dirty chunks nearest first, once every neighbor expected in
        // this view has arrived. That avoids repeatedly rebuilding frontier
        // chunks as an eight-chunk network batch expands around them. A
        // circular residency set still has an outer boundary; neighbors beyond
        // the negotiated radius are deliberately sampled as air, so the ring
        // that used to remain invisible forever is meshed exactly once.
        let mut dirty: Vec<(i32, ChunkPos)> = self
            .server
            .world
            .dirty_chunks()
            .into_iter()
            .map(|p| (p.distance(center) as i32, p))
            .collect();
        dirty.retain(|(_, position)| self.chunk_mesh_ready(*position, center, vd));
        dirty.retain(|(_, position)| {
            !self
                .mesh_pool
                .as_ref()
                .is_some_and(|pool| pool.contains(*position))
        });
        dirty.sort_by_key(|(d, _)| *d);
        // Snapshot at most one job per frame and keep two total outstanding.
        // The old scoped threads were joined immediately, putting 40–60 ms of
        // supposedly background work straight back onto every render frame.
        let available = self
            .mesh_pool
            .as_ref()
            .map_or(0, |pool| 2usize.saturating_sub(pool.pending_count()));
        let jobs = dirty
            .into_iter()
            .take(available.min(1))
            .filter_map(|(_, position)| {
                mesher::ChunkMeshInput::capture(&self.server.world, position).map(|input| {
                    (
                        input,
                        self.content.tile_variants.clone(),
                        current_variant_signature.clone(),
                    )
                })
            })
            .collect::<Vec<_>>();
        for (input, variants, signature) in jobs {
            let position = input.position();
            let sent = self
                .mesh_pool
                .as_mut()
                .is_some_and(|pool| pool.request(input, variants, signature));
            if sent {
                // This snapshot owns the current dirty state. Any subsequent
                // edit flips it dirty again while the worker is running.
                self.server.world.mark_chunk_meshed(position);
            }
        }
    }
}
