//! Surface materials produce post-carve heights and biome labels for planting.

use super::Generator;
use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos, SEA_LEVEL};
use crate::registry::AIR;
use super::Biome;
use super::shape::ShapeColumns;
use crate::registry::BlockId;

pub(super) struct SurfaceColumns {
    pub(super) heights: [[i32; CHUNK_Z]; CHUNK_X],
    pub(super) biomes: [[Biome; CHUNK_Z]; CHUNK_X],
}

impl Generator {
    pub(super) fn apply_surface(&self, pos: ChunkPos, c: &mut Chunk, shape: &ShapeColumns) -> SurfaceColumns {
        let shape_top = &shape.top;
        let fills = &shape.fills;
        let armors = &shape.armors;
        // Stage 3: surface rules.
        let mut heights = [[0i32; CHUNK_Z]; CHUNK_X];
        let mut biomes = [[Biome::Plains; CHUNK_Z]; CHUNK_X];
        for lx in 0..CHUNK_X {
            for lz in 0..CHUNK_Z {
                let surface = Self::surface_in_chunk(pos, lx as i32, lz as i32);
                let biome = self.biome_at(surface);
                let hydrology = self
                    .atlas
                    .as_ref()
                    .map(|atlas| atlas.hydrology_sample(surface.center()));
                let ecology = self.atlas.as_ref().map(|atlas| atlas.biome_sample(surface));
                biomes[lx][lz] = biome;

                // Post-carve top solid (an armored bank can stand
                // above the density top — start the scan at its crest).
                let mut top = shape_top[lx + 1][lz + 1].max(armors[lx][lz]);
                while top > 0 && !self.is_rock(c.get(lx, top as usize, lz)) {
                    top -= 1;
                }
                heights[lx][lz] = top;

                // Steepness from the pre-carve heightmap ring (consistent
                // across chunk borders by construction).
                let h0 = shape_top[lx + 1][lz + 1];
                let mut slope = 0;
                for (dx, dz) in [(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
                    let n = shape_top[(lx as i32 + 1 + dx) as usize][(lz as i32 + 1 + dz) as usize];
                    slope = slope.max((h0 - n).abs());
                }
                let steep = slope >= 3;
                // Standing water needs somewhere to stand: a column
                // whose neighbors all sit at or above it. On a
                // shoulder a "pool" is just a spring, and it pours
                // downhill forever — which is exactly what a whole
                // swamp province turned into before this rule.
                let basin = [(0i32, 1i32), (0, -1), (1, 0), (-1, 0)]
                    .iter()
                    .all(|&(dx, dz)| {
                        shape_top[(lx as i32 + 1 + dx) as usize][(lz as i32 + 1 + dz) as usize]
                            >= h0
                    });
                let underwater = top < fills[lx][lz].max(SEA_LEVEL) - 1;
                let snowcap = ecology.map_or(top >= 170, |sample| {
                    top > i32::from(sample.tree_line_y)
                        && sample.habitat_flags
                            & (crate::planet_atlas::HABITAT_ALPINE
                                | crate::planet_atlas::HABITAT_PERMAFROST)
                            != 0
                }) || (ecology.is_none()
                    && biome == Biome::Mountains
                    && top >= 150
                    && Self::noise_at(&self.detail, surface, 9.0, [0.0; 3]) > -0.2);

                let scrub_sandy = Self::noise_at(&self.detail, surface, 33.0, [0.0; 3]) > 0.15;
                // None = leave the natural rock exposed (bare mountains,
                // steep faces — the strata read in the cliffs).
                let (top_b, under_b): (Option<BlockId>, Option<BlockId>) = if underwater {
                    if hydrology.is_some_and(|sample| {
                        sample.flags
                            & (crate::planet_atlas::HYDRO_DELTA
                                | crate::planet_atlas::HYDRO_FLOODPLAIN)
                            != 0
                            || sample.sediment_energy < 0.28
                    }) {
                        (Some(self.clay), Some(self.clay))
                    } else if hydrology.is_some_and(|sample| sample.sediment_energy > 0.68)
                        || top < SEA_LEVEL - 14
                    {
                        (Some(self.gravel), Some(self.gravel))
                    } else if Self::noise_at(&self.detail, surface, 23.0, [0.0; 3]) > 0.34 {
                        // Clay beds: patches where still shallows let
                        // the fine sediment settle (wild arc, stage 5
                        // — the crock starts here).
                        (Some(self.clay), Some(self.clay))
                    } else {
                        (Some(self.sand), Some(self.sand))
                    }
                } else if snowcap {
                    (Some(self.snow), None)
                } else if ecology.is_some_and(|sample| {
                    sample.edaphic_flags
                        & (crate::planet_atlas::EDAPHIC_STEEP
                            | crate::planet_atlas::EDAPHIC_SHALLOW_ROCK)
                        != 0
                        && sample.soil_depth_decimeters < 5
                }) || biome == Biome::Mountains
                    || steep
                {
                    // Bare rock: mountains, cliffs, volcano flanks.
                    (None, None)
                } else {
                    let beach = top <= SEA_LEVEL + 1;
                    let patch = Self::noise_at(&self.detail, surface, 9.0, [0.0; 3]);
                    if let Some(sample) = ecology {
                        if sample.habitat_flags & crate::planet_atlas::HABITAT_SALT_MARSH != 0 {
                            if sample.salinity >= 150 && patch > 0.48 {
                                (Some(self.halite), Some(self.sand))
                            } else if patch < 0.30 {
                                (Some(self.mud), Some(self.mud))
                            } else {
                                // Salt-tolerant turf occupies the raised
                                // marsh; ordinary forest remains excluded by
                                // the salinity/tree rules below.
                                (Some(self.grass), Some(self.mud))
                            }
                        } else if sample.salinity >= 150 && patch > 0.48 {
                            (Some(self.halite), Some(self.sand))
                        } else if sample.habitat_flags & crate::planet_atlas::HABITAT_BEACH_DUNE
                            != 0
                            || sample.edaphic_flags & crate::planet_atlas::EDAPHIC_DUNE != 0
                        {
                            (Some(self.sand), Some(self.sand))
                        } else if sample.habitat_flags & crate::planet_atlas::HABITAT_WETLAND != 0
                            && patch < 0.32
                        {
                            (Some(self.mud), Some(self.mud))
                        } else if sample.edaphic_flags & crate::planet_atlas::EDAPHIC_ALLUVIAL != 0
                            && patch < -0.18
                        {
                            (Some(self.clay), Some(self.mud))
                        } else {
                            let watered = sample.habitat_flags
                                & (crate::planet_atlas::HABITAT_RIPARIAN
                                    | crate::planet_atlas::HABITAT_OASIS
                                    | crate::planet_atlas::HABITAT_WETLAND)
                                != 0;
                            match biome {
                                // Zonal aridity still owns ordinary ground.
                                // Productivity alone cannot turn a hot desert
                                // into turf: the energy term keeps even truly
                                // dry cells well above the old threshold.
                                Biome::Desert if !watered => (Some(self.sand), Some(self.sand)),
                                Biome::Badlands if !watered => (None, None),
                                Biome::Scrubland
                                    if !watered
                                        && (sample.vegetation_potential < 96 || scrub_sandy) =>
                                {
                                    (Some(self.sand), Some(self.dirt))
                                }
                                Biome::Arctic => (Some(self.snow), Some(self.dirt)),
                                Biome::Tundra if patch > 0.22 => (Some(self.snow), Some(self.dirt)),
                                Biome::Tundra if patch < -0.3 => {
                                    (Some(self.gravel), Some(self.gravel))
                                }
                                Biome::Tundra => (Some(self.dirt), Some(self.dirt)),
                                _ if sample.vegetation_potential < 48 => {
                                    (Some(self.dirt), Some(self.dirt))
                                }
                                _ => (Some(self.grass), Some(self.dirt)),
                            }
                        }
                    } else {
                        match biome {
                            Biome::Desert => (Some(self.sand), Some(self.sand)),
                            Biome::Scrubland if scrub_sandy => (Some(self.sand), Some(self.sand)),
                            Biome::Arctic => (Some(self.snow), Some(self.dirt)),
                            // Mesa country bares its sandstone bones.
                            Biome::Badlands => (None, None),
                            // Frozen barrens: snow, dirt, and gravel patches.
                            Biome::Tundra if patch > 0.22 => (Some(self.snow), Some(self.dirt)),
                            Biome::Tundra if patch < -0.3 => (Some(self.gravel), Some(self.gravel)),
                            Biome::Tundra => (Some(self.dirt), Some(self.dirt)),
                            _ if hydrology.is_some_and(|sample| {
                                sample.flags & crate::planet_atlas::HYDRO_WETLAND != 0
                                    && patch < 0.2
                            }) =>
                            {
                                (Some(self.mud), Some(self.mud))
                            }
                            // Wetlands: standing pools and mud between grass.
                            Biome::Swamp
                                if self.atlas.is_none() && patch > 0.34 && !beach && basin =>
                            {
                                (Some(self.water), Some(self.mud))
                            }
                            Biome::Swamp if patch < -0.22 => (Some(self.mud), Some(self.mud)),
                            _ if beach => (Some(self.sand), Some(self.sand)),
                            _ => (Some(self.grass), Some(self.dirt)),
                        }
                    }
                };

                // Apply to the consecutive solid run from the top.
                if top > 0
                    && let Some(tb) = top_b
                {
                    c.set(lx, top as usize, lz, tb);
                    if let Some(ub) = under_b {
                        for d in 1..=3i32 {
                            let y = top - d;
                            if y <= 0 || !self.is_rock(c.get(lx, y as usize, lz)) {
                                break;
                            }
                            c.set(lx, y as usize, lz, ub);
                        }
                    }
                }

                // Mountain springs: rare seeps on high steep ground,
                // a still pool the size of a footprint.
                if self.atlas.is_none()
                    && top > 110
                    && steep
                    && top + 1 < CHUNK_Y as i32 - 1
                    && self.hash_surface(0x59a1, surface).is_multiple_of(211)
                    && c.get(lx, (top + 1) as usize, lz) == AIR
                {
                    c.set(lx, top as usize, lz, self.water);
                }

                // Frozen ocean surface.
                if biome == Biome::Arctic && c.get(lx, SEA_LEVEL as usize, lz) == self.water {
                    c.set(lx, SEA_LEVEL as usize, lz, self.ice);
                }
            }
        }

        SurfaceColumns { heights, biomes }
    }
}
