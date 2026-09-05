//! Cave carving consumes the pre-carve top map without changing it.

use super::Generator;
use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos, SEA_LEVEL};
use crate::registry::AIR;
use super::{ATLAS_CAVE_ROOF, RING};

impl Generator {
    pub(super) fn carve(&self, pos: ChunkPos, c: &mut Chunk, shape_top: &[[i32; RING]; RING]) {
        // Stage 2: carve caves (stone only, never the bedrock rows).
        for lx in 0..CHUNK_X as i32 {
            for lz in 0..CHUNK_Z as i32 {
                let surface = Self::surface_in_chunk(pos, lx, lz);
                let top = shape_top[(lx + 1) as usize][(lz + 1) as usize];
                let magma_interval = self
                    .atlas
                    .as_ref()
                    .and_then(|atlas| atlas.magma_interval(surface.center()));
                for y in 5..top.min(CHUNK_Y as i32 - 1) {
                    if !self.is_rock(c.get(lx as usize, y as usize, lz as usize)) {
                        continue;
                    }
                    let depth = (top - y).max(0) as f32;
                    if self.atlas.is_some() && depth < ATLAS_CAVE_ROOF {
                        continue;
                    }
                    let yf = y as f64;
                    // Cheese: big voids, more common deeper down.
                    let ch = Self::radial_noise_at(&self.cheese, surface, yf, 120.0, [0.0; 3]);
                    // Large chambers should read as underground geology, not
                    // hollow out the visible shell of every highland and
                    // volcanic cone.  Fade them out through the upper 32
                    // blocks while leaving the deeper cave field unchanged.
                    let near_surface = (1.0 - depth / 32.0).clamp(0.0, 1.0);
                    let cheese_thr = 0.74 - (SEA_LEVEL as f32 - y as f32).clamp(0.0, 50.0) * 0.004
                        + near_surface * 0.20;
                    // Spaghetti: two noises near zero = a winding tunnel.
                    // Width tapers near the surface so entrances are rare.
                    let taper = (depth / 12.0).min(1.0);
                    let w = (0.055 + depth * 0.0003) * taper;
                    let s1 = Self::radial_noise_at(&self.spag1, surface, yf, 70.0, [0.0; 3]);
                    let s2 =
                        Self::radial_noise_at(&self.spag2, surface, yf, 70.0, [41.0, 0.0, -13.0]);
                    let manifest_magma = magma_interval.is_some_and(|(minimum, maximum)| {
                        y as f32 >= minimum && y as f32 <= maximum
                    });
                    if manifest_magma || (self.atlas.is_none() && y < 11 && ch > 0.32) {
                        // Deep magma pockets: where the cheese noise
                        // merely swells, the rock holds lava instead
                        // of opening — sealed chambers you mine into.
                        // Settled full cells, never queued, until
                        // something breaks the crust.
                        c.set(lx as usize, y as usize, lz as usize, self.lava);
                    } else if ch > cheese_thr || (s1.abs() < w && s2.abs() < w) {
                        c.set(lx as usize, y as usize, lz as usize, AIR);
                    }
                }
            }
        }

    }
}
