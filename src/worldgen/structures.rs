//! Province hearts and edifices stamp only the current chunk's columns.

use super::Generator;
use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos, SEA_LEVEL};
use super::Biome;
use crate::registry::BlockId;
use crate::planet::geodesic_distance;

impl Generator {
    /// The stone and trim a country builds with.
    pub(super) fn edifice_materials(&self, biome: Biome) -> crate::edifice::Materials {
        self.edifice_mats
            .get(biome as usize)
            .copied()
            .unwrap_or(crate::edifice::Materials {
                shell: self.stone,
                crown: self.stone,
            })
    }

    /// The living heart block a country raises.
    pub(crate) fn heart_block(&self, biome: Biome) -> BlockId {
        self.hearts
            .get(biome as usize)
            .copied()
            .unwrap_or(self.stone)
    }


    pub(super) fn plant_province_structures(&self, c: &mut Chunk, pos: ChunkPos, heights: &[[i32; CHUNK_Z]; CHUNK_X]) {
        // The heart of a country stands at its province's center. A
        // site sits inside its own province by construction, so at
        // most a few keys can land in any one chunk.
        {
            let center = Self::surface_in_chunk(pos, CHUNK_X as i32 / 2, CHUNK_Z as i32 / 2);
            for key in self.province_keys_near(center, 24.0) {
                let site = self.province_center_at(key);
                if ChunkPos::from_surface(site) != pos {
                    continue;
                }
                let lx = usize::from(site.u() % CHUNK_X as u16);
                let lz = usize::from(site.v() % CHUNK_Z as u16);
                let ground = heights[lx][lz];
                // A site wants dry, standable ground; a country
                // whose center drowns keeps its heart unbuilt, and
                // the world reads such country as living.
                if ground <= SEA_LEVEL || ground + 8 >= CHUNK_Y as i32 {
                    continue;
                }
                let biome = self.province_at(site).biome;
                if biome == Biome::Ocean {
                    continue;
                }
                let form = super::heart_form(biome);
                let block = self.heart_block(biome);
                let tall = super::heart_height(form);
                for dy in 1..=tall {
                    c.set(lx, (ground + dy) as usize, lz, block);
                }
            }
        }

        // And the edifice over it. A monument spans several chunks and
        // a chunk cannot write into its neighbours, so this is not one
        // chunk stamping a shape: every chunk asks what belongs in its
        // OWN columns and they agree at the seams by construction.
        // The site's ground therefore has to come from a function of
        // position alone, not from this chunk's carved heightmap.
        {
            let center = Self::surface_in_chunk(pos, CHUNK_X as i32 / 2, CHUNK_Z as i32 / 2);
            for key in self.province_keys_near(center, 96.0) {
                let site = self.province_center_at(key);
                let biome = self.province_at(site).biome;
                if biome == Biome::Ocean {
                    continue;
                }
                let ed = crate::edifice::edifice_of(biome);
                if geodesic_distance(center.center(), site.center()) > f64::from(ed.reach) + 24.0 {
                    continue;
                }
                let base = self.surface_estimate_at(site);
                if base <= SEA_LEVEL || base + ed.rise + 4 >= CHUNK_Y as i32 {
                    continue;
                }
                let mats = self.edifice_materials(biome);
                let site_entity = crate::planet::EntityPos::new(
                    site.face(),
                    f32::from(site.u()) + 0.5,
                    0.0,
                    f32::from(site.v()) + 0.5,
                )
                .expect("province site is canonical");
                for lx in 0..CHUNK_X as i32 {
                    for lz in 0..CHUNK_Z as i32 {
                        let column = Self::surface_in_chunk(pos, lx, lz);
                        let column_entity = crate::planet::EntityPos::new(
                            column.face(),
                            f32::from(column.u()) + 0.5,
                            0.0,
                            f32::from(column.v()) + 0.5,
                        )
                        .expect("chunk column is canonical");
                        let delta = site_entity.local_delta_to(column_entity);
                        let (dx, dz) = (delta.x.round() as i32, delta.z.round() as i32);
                        if dx.abs() > ed.reach || dz.abs() > ed.reach {
                            continue;
                        }
                        // Footings run a little below the estimate so
                        // the mass never floats over carved ground.
                        for dy in -4..=ed.rise {
                            let y = base + dy;
                            if !(1..CHUNK_Y as i32).contains(&y) {
                                continue;
                            }
                            if let Some(b) = crate::edifice::block_at(&ed, &mats, dx, dy, dz) {
                                c.set(lx as usize, y as usize, lz as usize, b);
                            }
                        }
                    }
                }
            }
        }
    }
}
