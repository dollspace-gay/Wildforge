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
        let workers = std::thread::available_parallelism()
            .map(|n| (n.get() / 2).clamp(1, 4))
            .unwrap_or(2);
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
        if let Some(pool) = &mut self.gen_pool {
            // Keep the workers fed a nearest-first pipeline.
            for (_, pos) in wanted.iter().take(24) {
                if pool.in_flight.len() >= 12 {
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
                if t0.elapsed().as_millis() >= 3 {
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

        // Unload chunks far outside the view radius.
        let limit = vd + 2;
        let far = self
            .server
            .world
            .chunks_outside(ChunkPos { x: pcx, z: pcz }, limit);
        if !far.is_empty() {
            self.server.world.settle_falling();
            // Save only what leaves; a full save_modified here wrote
            // the whole world (palette, entities, mobs, stamps, every
            // modified chunk) to disk on the main thread every time a
            // single chunk crossed the border — a walking-speed
            // stutter machine. The autosave timer owns the full save.
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
        for (_, pos) in dirty.into_iter().take(MESH_BUDGET) {
            let mesh = mesher::mesh_chunk(&self.server.world, pos);
            self.renderer.upload_chunk(pos, &mesh);
            self.presentation.lights.chunk_meshed(pos, mesh.emitters);
            self.server.world.mark_chunk_meshed(pos);
            if self.stream_t0.elapsed().as_millis() >= 5 {
                break;
            }
        }
    }
}
