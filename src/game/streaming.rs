//! Client chunk generation, unloading, remeshing, and GPU upload budgets.

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
        for (_, pos) in wanted.into_iter().take(GEN_BUDGET) {
            self.server.world.ensure_chunk(pos);
            // New terrain changes neighbors' visible faces at the border.
            for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                self.server.world.mark_chunk_dirty(ChunkPos {
                    x: pos.x + dx,
                    z: pos.z + dz,
                });
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
            self.server.world.save_modified();
            for pos in far {
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
        for (_, pos) in dirty.into_iter().take(MESH_BUDGET) {
            let mesh = mesher::mesh_chunk(&self.server.world, pos);
            self.renderer.upload_chunk(pos, &mesh);
            self.presentation.lights.chunk_meshed(pos, mesh.emitters);
            self.server.world.mark_chunk_meshed(pos);
        }
    }
}
