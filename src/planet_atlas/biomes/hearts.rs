//! Habitat-aware selection of accessible country heart sites.

use crate::chunk::{SEA_LEVEL};
use crate::planet::{geodesic_distance};
use crate::planet_atlas::{AtlasGrid, AtlasPos, BiomeCell, GeometryCell, GroundCell, HydrologyCell, TerrainCell};
use super::{BIOME_ARCTIC, BIOME_BADLANDS, BIOME_DESERT, BIOME_FOREST, BIOME_JUNGLE, BIOME_MOUNTAINS, BIOME_PLAINS, BIOME_SAVANNA, BIOME_SCRUBLAND, BIOME_TAIGA, BIOME_TUNDRA, HABITAT_AQUATIC_BRACKISH, HABITAT_AQUATIC_FRESH, HABITAT_AQUATIC_SALT, HABITAT_FLOODPLAIN, HABITAT_OASIS, HABITAT_PERMAFROST, HABITAT_RIPARIAN, HABITAT_SPRING, HABITAT_WETLAND};

#[allow(clippy::too_many_arguments)]
pub(super) fn heart_score(
    index: usize,
    dominant: u8,
    wetland_country: bool,
    side: u16,
    seed_index: usize,
    geometry: &AtlasGrid<GeometryCell>,
    terrain: &AtlasGrid<TerrainCell>,
    hydrology: &AtlasGrid<HydrologyCell>,
    ground: &AtlasGrid<GroundCell>,
    biomes: &AtlasGrid<BiomeCell>,
) -> f32 {
    let cell = biomes.values()[index];
    let habitat = cell.habitat_flags;
    if habitat & (HABITAT_AQUATIC_FRESH | HABITAT_AQUATIC_BRACKISH | HABITAT_AQUATIC_SALT) != 0 {
        return f32::NEG_INFINITY;
    }
    let distance = geodesic_distance(
        AtlasPos::from_index(index, side)
            .expect("candidate")
            .center(side),
        AtlasPos::from_index(seed_index, side)
            .expect("seed")
            .center(side),
    ) as f32;
    let mut score = 180.0 - distance * 0.12 + f32::from(cell.vegetation_potential) * 0.28;
    match dominant {
        BIOME_FOREST | BIOME_JUNGLE | BIOME_TAIGA => {
            score += f32::from(ground.values()[index].organic) * 0.42;
            score += if habitat & HABITAT_RIPARIAN != 0 {
                32.0
            } else {
                0.0
            };
        }
        BIOME_DESERT | BIOME_SCRUBLAND | BIOME_BADLANDS => {
            score += if habitat & HABITAT_OASIS != 0 {
                260.0
            } else {
                0.0
            };
            score += if habitat & HABITAT_SPRING != 0 {
                130.0
            } else {
                0.0
            };
            score += if habitat & HABITAT_RIPARIAN != 0 {
                74.0
            } else {
                0.0
            };
            score += f32::from(ground.values()[index].soil_depth_decimeters) * -0.8;
        }
        BIOME_PLAINS | BIOME_SAVANNA => {
            score += if habitat & (HABITAT_RIPARIAN | HABITAT_FLOODPLAIN) != 0 {
                66.0
            } else {
                0.0
            };
        }
        BIOME_MOUNTAINS => {
            score += (terrain.values()[index].eroded_elevation - SEA_LEVEL as f32) * 0.7;
            score +=
                if hydrology.values()[index].stream_order <= 1 && habitat & HABITAT_RIPARIAN != 0 {
                    72.0
                } else {
                    0.0
                };
        }
        BIOME_TUNDRA | BIOME_ARCTIC => {
            score += (terrain.values()[index].eroded_elevation - SEA_LEVEL as f32) * 0.25;
            score += if habitat & HABITAT_PERMAFROST != 0 {
                28.0
            } else {
                0.0
            };
        }
        _ => {}
    }
    if wetland_country {
        score += if habitat & HABITAT_WETLAND == 0 {
            72.0
        } else {
            24.0
        };
        score += if habitat & HABITAT_RIPARIAN != 0 {
            24.0
        } else {
            0.0
        };
    }
    score + geometry.values()[index].latitude_radians.cos().abs() * 0.001
}
