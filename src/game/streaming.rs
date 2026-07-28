//! Client chunk generation, unloading, remeshing, and GPU upload budgets.
//!
//! Terrain math is pure, so generation runs on background workers; the
//! main thread only adopts finished chunks (light, seams, reconcile)
//! and meshes, each on a per-frame time budget.

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

/// A pool of background chunk generators for the current world.
pub(super) struct GenPool {
    pub(super) req: Sender<ChunkPos>,
    pub(super) done: Receiver<(ChunkPos, crate::chunk::Chunk)>,
    pub(super) in_flight: HashSet<ChunkPos>,
}

impl GenPool {
    pub(super) fn new(seed: u32, reg: Arc<Registry>) -> GenPool {
        let (req, req_rx) = channel::<ChunkPos>();
        let (done_tx, done) = channel();
        let req_rx = Arc::new(Mutex::new(req_rx));
        // Leave a couple of cores for the render thread and the OS,
        // and take the rest. This was capped at four however many the
        // machine had, which is what made a large view distance take
        // thousands of frames to fill rather than seconds.
        let workers = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(2).clamp(2, 24))
            .unwrap_or(4);
        for _ in 0..workers {
            let rx = Arc::clone(&req_rx);
            let tx = done_tx.clone();
            let reg = reg.clone();
            std::thread::spawn(move || {
                let generator = crate::worldgen::Generator::new(seed, &reg);
                loop {
                    let pos = {
                        let Ok(guard) = rx.lock() else { return };
                        let Ok(pos) = guard.recv() else { return };
                        pos
                    };
                    if tx.send((pos, generator.generate(pos, &reg))).is_err() {
                        return; // the world moved on
                    }
                }
            });
        }
        GenPool {
            req,
            done,
            in_flight: HashSet::new(),
        }
    }
}

use super::*;

impl Game {
    /// Outstanding generation/remesh work used to settle deterministic
    /// headless captures before the screenshot fires.
    pub(super) fn chunk_work_pending(&self) -> usize {
        if !self.in_world {
            return 0;
        }
        let pcx = (self.player.pos.x.floor() as i32).div_euclid(CHUNK_X as i32);
        let pcz = (self.player.pos.z.floor() as i32).div_euclid(CHUNK_X as i32);
        let vd = self.config.view_dist;
        let mut pending = 0;
        for dx in -vd..=vd {
            for dz in -vd..=vd {
                if !self.server.world.has_chunk(ChunkPos {
                    x: pcx + dx,
                    z: pcz + dz,
                }) {
                    pending += 1;
                }
            }
        }
        pending
            + self
                .server
                .world
                .dirty_chunks()
                .into_iter()
                .filter(|pos| {
                    [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().all(|(dx, dz)| {
                        self.server.world.has_chunk(ChunkPos {
                            x: pos.x + dx,
                            z: pos.z + dz,
                        })
                    })
                })
                .count()
    }

    pub(super) fn stream_chunks(&mut self) {
        self.stream_t0 = std::time::Instant::now();
        let pcx = (self.player.pos.x.floor() as i32).div_euclid(CHUNK_X as i32);
        let pcz = (self.player.pos.z.floor() as i32).div_euclid(CHUNK_X as i32);

        // Generate missing chunks, nearest first.
        let mut wanted: Vec<(i32, ChunkPos)> = Vec::new();
        let vd = self.config.view_dist;
        for dx in -vd..=vd {
            for dz in -vd..=vd {
                let pos = ChunkPos {
                    x: pcx + dx,
                    z: pcz + dz,
                };
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
        // Adoption and meshing share one time budget so they cannot
        // stack; it grows with the view because there is simply more to
        // bring in, and a bigger view is a deliberate choice to spend
        // frame time on distance.
        let adopt_ms = (vd_us as u128 / 2).clamp(3, 12);
        let stream_ms = (vd_us as u128).clamp(5, 28);
        if let Some(pool) = &mut self.gen_pool {
            // Keep the workers fed a nearest-first pipeline.
            for (_, pos) in wanted.iter().take(ask) {
                if pool.in_flight.len() >= flight {
                    break;
                }
                if pool.in_flight.insert(*pos) {
                    let _ = pool.req.send(*pos);
                }
            }
            // Adopt what's ready, on a time budget — adoption still
            // pays light and seams on this thread. This budget and the
            // mesh budget below are ONE 5ms pool: they used to stack
            // (6ms + 6ms) and could eat 12ms of every streaming frame
            // by themselves.
            let t0 = self.stream_t0;
            while let Ok((pos, chunk)) = pool.done.try_recv() {
                pool.in_flight.remove(&pos);
                if self.server.world.adopt_generated(pos, chunk) {
                    // New terrain changes neighbors' faces at the border.
                    for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                        self.server.world.mark_chunk_dirty(ChunkPos {
                            x: pos.x + dx,
                            z: pos.z + dz,
                        });
                    }
                }
                if t0.elapsed().as_millis() >= adopt_ms {
                    break;
                }
            }
        } else if !self.server.world.is_remote() {
            // No pool (a fresh session mid-setup): the synchronous
            // path stays correct, just slower.
            for (_, pos) in wanted.into_iter().take(GEN_BUDGET) {
                self.server.world.ensure_chunk(pos);
                for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    self.server.world.mark_chunk_dirty(ChunkPos {
                        x: pos.x + dx,
                        z: pos.z + dz,
                    });
                }
            }
        }

        // Unload chunks far outside the view radius. The same residency rule
        // the dedicated server runs (World::retain_chunks); here it is spelled
        // out because the renderer and the light cache have to let go too.
        let limit = vd + 2;
        let far = self
            .server
            .world
            .chunks_outside_all(&[ChunkPos { x: pcx, z: pcz }], limit);
        if !far.is_empty() {
            self.server.world.settle_falling();
            // Save only what leaves; a full save_modified here wrote
            // the whole world (palette, entities, mobs, stamps, every
            // modified chunk) to disk on the main thread every time a
            // single chunk crossed the border — a walking-speed
            // stutter machine. This IS the incremental save now: the
            // timer is gone, and a chunk is written as it leaves the
            // view rather than the whole world on a clock.
            for pos in far {
                self.server.world.save_chunk_if_modified(pos);
                self.server.world.unload_chunk(pos);
                self.renderer.drop_chunk(pos);
                self.presentation.lights.chunk_dropped(pos);
            }
        }

        // Remesh dirty chunks (only those whose 4 neighbors exist), nearest first.
        let mut dirty: Vec<(i32, ChunkPos)> = self
            .server
            .world
            .dirty_chunks()
            .into_iter()
            .map(|p| ((p.x - pcx).pow(2) + (p.z - pcz).pow(2), p))
            .collect();
        dirty.retain(|(_, p)| {
            [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().all(|(dx, dz)| {
                self.server.world.has_chunk(ChunkPos {
                    x: p.x + dx,
                    z: p.z + dz,
                })
            })
        });
        dirty.sort_by_key(|(d, _)| *d);
        // Meshing spends whatever the shared 5ms streaming pool has
        // left after adoption.
        let mesh_cap = (vd_us).clamp(MESH_BUDGET, 64);
        for (_, pos) in dirty.into_iter().take(mesh_cap) {
            let mesh = mesher::mesh_chunk(&self.server.world, pos, &self.content.tile_variants);
            self.renderer.upload_chunk(pos, &mesh);
            self.presentation.lights.chunk_meshed(pos, mesh.emitters);
            self.server.world.mark_chunk_meshed(pos);
            // A remesh within the DDA occupancy grid's reach means occluder
            // blocks changed — mark the grid stale so shadows track the edit.
            let (cx, cz) = (pos.x as f32 * 16.0 + 8.0, pos.z as f32 * 16.0 + 8.0);
            if (cx - self.camera.pos.x).hypot(cz - self.camera.pos.z)
                < crate::renderer::OCC_GRID as f32 / 2.0 + 16.0
            {
                self.occ_dirty = true;
            }
            if self.stream_t0.elapsed().as_millis() >= stream_ms {
                break;
            }
        }
    }
}
