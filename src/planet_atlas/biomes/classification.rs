//! Zonal climate classifications and terrain slope interpretation.

use crate::chunk::{SEA_LEVEL};
use crate::planet_atlas::{AtlasGrid, AtlasPos, ClimateCell, TerrainCell};
use super::{BIOME_ARCTIC, BIOME_BADLANDS, BIOME_DESERT, BIOME_FOREST, BIOME_JUNGLE, BIOME_MOUNTAINS, BIOME_OCEAN, BIOME_PLAINS, BIOME_SAVANNA, BIOME_SCRUBLAND, BIOME_SWAMP, BIOME_TAIGA, BIOME_TUNDRA};

pub(super) fn slope_at(index: usize, side: u16, terrain: &AtlasGrid<TerrainCell>) -> f32 {
    let pos = AtlasPos::from_index(index, side).expect("atlas index");
    let elevation = terrain.values()[index].eroded_elevation;
    pos.neighbors4(side)
        .into_iter()
        .map(|neighbor| (terrain.values()[neighbor.index(side)].eroded_elevation - elevation).abs())
        .fold(0.0, f32::max)
}

pub(super) fn classify_zonal(climate: ClimateCell, elevation: f32, slope: f32, tree_line: f32) -> u8 {
    if elevation <= SEA_LEVEL as f32 {
        return BIOME_OCEAN;
    }
    let warmest = climate
        .seasonal_temperature
        .into_iter()
        .fold(f32::NEG_INFINITY, f32::max);
    if elevation > tree_line + 7.0 || (elevation > 112.0 && slope > 17.0) {
        BIOME_MOUNTAINS
    } else if warmest < 0.0 || (climate.mean_temperature < -12.0 && climate.snow_persistence > 0.65)
    {
        BIOME_ARCTIC
    } else if warmest < 8.0 || climate.mean_temperature < -5.0 {
        BIOME_TUNDRA
    } else if climate.mean_temperature < 6.5 {
        BIOME_TAIGA
    } else if climate.mean_temperature >= 21.0
        && climate.mean_precipitation >= 1_550.0
        && climate.aridity < 0.82
        && climate.precipitation_seasonality < 0.75
    {
        BIOME_JUNGLE
    } else if climate.mean_temperature >= 18.0
        && climate.mean_precipitation >= 560.0
        && climate.aridity < 1.65
    {
        BIOME_SAVANNA
    } else if climate.mean_temperature >= 13.0
        && (climate.aridity >= 1.55 || climate.mean_precipitation < 310.0)
    {
        BIOME_DESERT
    } else if climate.aridity >= 1.02 || climate.mean_precipitation < 620.0 {
        if slope > 7.0 || climate.aridity > 1.38 {
            BIOME_BADLANDS
        } else {
            BIOME_SCRUBLAND
        }
    } else if climate.mean_precipitation >= 820.0 && climate.aridity < 1.0 {
        BIOME_FOREST
    } else {
        BIOME_PLAINS
    }
}

pub(super) fn biome_name(id: u8) -> &'static str {
    match id {
        BIOME_FOREST => "forest",
        BIOME_PLAINS => "plains",
        BIOME_DESERT => "desert",
        BIOME_JUNGLE => "jungle",
        BIOME_SCRUBLAND => "scrubland",
        BIOME_TAIGA => "taiga",
        BIOME_ARCTIC => "arctic",
        BIOME_MOUNTAINS => "mountains",
        BIOME_SWAMP => "swamp",
        BIOME_SAVANNA => "savanna",
        BIOME_TUNDRA => "tundra",
        BIOME_BADLANDS => "badlands",
        BIOME_OCEAN => "ocean",
        _ => "unknown",
    }
}
