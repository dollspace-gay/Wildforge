//! Water-backed habitat overlays, edaphic flags, and vegetation potential.

use super::classification::{classify_zonal, slope_at};
use super::{
    BIOME_MOUNTAINS, EDAPHIC_ALLUVIAL, EDAPHIC_DUNE, EDAPHIC_FIRE_FAVORED, EDAPHIC_LIMESTONE,
    EDAPHIC_PERMAFROST, EDAPHIC_SHALLOW_ROCK, EDAPHIC_STEEP, EDAPHIC_VOLCANIC, FREEZE_PERMAFROST,
    HABITAT_ALPINE, HABITAT_AQUATIC_BRACKISH, HABITAT_AQUATIC_FRESH, HABITAT_AQUATIC_SALT,
    HABITAT_BEACH_DUNE, HABITAT_CAVE_OUTLET, HABITAT_ESTUARY_DELTA, HABITAT_FLOODPLAIN,
    HABITAT_LAKESHORE, HABITAT_OASIS, HABITAT_PERMAFROST, HABITAT_RIPARIAN, HABITAT_SALT_MARSH,
    HABITAT_SPRING, HABITAT_VOLCANIC_SOIL, HABITAT_WETLAND,
};
use crate::planet::surface_to_unit;
use crate::planet_atlas::{
    AtlasGrid, AtlasPos, BedrockFamily, BiomeCell, ClimateCell, GroundCell, HYDRO_DELTA,
    HYDRO_ESTUARY, HYDRO_FLOODPLAIN, HYDRO_KARST_LOSS, HYDRO_LAKE, HYDRO_OCEAN, HYDRO_PERENNIAL,
    HYDRO_RIVER, HYDRO_WETLAND, HydrologyCell, TectonicCell, TerrainCell, WaterBodyKind,
};
fn water_neighbor_flags(
    index: usize,
    side: u16,
    hydrology: &AtlasGrid<HydrologyCell>,
) -> (bool, bool) {
    let pos = AtlasPos::from_index(index, side).expect("atlas index");
    let mut ocean = false;
    let mut lake = false;
    for neighbor in pos.neighbors4(side) {
        let cell = hydrology.values()[neighbor.index(side)];
        ocean |= cell.flags & HYDRO_OCEAN != 0;
        lake |= cell.flags & HYDRO_LAKE != 0;
    }
    (ocean, lake)
}

pub(super) fn derive_biome_cell(
    index: usize,
    side: u16,
    terrain_grid: &AtlasGrid<TerrainCell>,
    tectonics: &AtlasGrid<TectonicCell>,
    climate_grid: &AtlasGrid<ClimateCell>,
    hydrology_grid: &AtlasGrid<HydrologyCell>,
    ground_grid: &AtlasGrid<GroundCell>,
) -> BiomeCell {
    let terrain = terrain_grid.values()[index];
    let tectonic = tectonics.values()[index];
    let climate = climate_grid.values()[index];
    let hydro = hydrology_grid.values()[index];
    let ground = ground_grid.values()[index];
    let latitude = AtlasPos::from_index(index, side)
        .expect("atlas index")
        .center(side);
    let latitude = surface_to_unit(latitude).y.asin().abs() as f32;
    let slope = slope_at(index, side, terrain_grid);
    let warmest = climate
        .seasonal_temperature
        .into_iter()
        .fold(f32::NEG_INFINITY, f32::max);
    let tree_line = (178.0 - latitude.to_degrees() * 0.78 + warmest * 0.72).clamp(72.0, 218.0);
    let zonal = classify_zonal(climate, terrain.eroded_elevation, slope, tree_line);
    let (ocean_neighbor, lake_neighbor) = water_neighbor_flags(index, side, hydrology_grid);
    let aquatic =
        hydro.flags & (HYDRO_OCEAN | HYDRO_LAKE) != 0 || hydro.water_body != WaterBodyKind::Land;
    let fresh = hydro.salinity < 64;
    let brackish = (64..192).contains(&hydro.salinity);
    let near_fresh_groundwater = ground.baseline_groundwater_head >= terrain.eroded_elevation - 2.0
        && ground.soil_salinity < 64;
    let arid = climate.aridity >= 1.02 || climate.mean_precipitation < 620.0;
    let spring = !aquatic
        && near_fresh_groundwater
        && (ground.aquifer_permeability > 12_000
            || slope > 1.5
            || hydro.flags & HYDRO_KARST_LOSS != 0
            || hydro.flags & HYDRO_PERENNIAL != 0);

    let mut habitat = 0u32;
    // Discharge exists in every drainage cell, including dry slopes with no
    // channel. Treating a tiny accumulated value as a river painted whole
    // desert watersheds riparian even though hydrology had assigned them no
    // river, water body, width, or depth.
    if hydro.flags & HYDRO_RIVER != 0 {
        habitat |= HABITAT_RIPARIAN;
    }
    if hydro.flags & HYDRO_FLOODPLAIN != 0 {
        habitat |= HABITAT_FLOODPLAIN | HABITAT_RIPARIAN;
    }
    if hydro.flags & HYDRO_WETLAND != 0
        || (!aquatic && near_fresh_groundwater && ground.drainage < 92)
    {
        habitat |= HABITAT_WETLAND;
    }
    if spring {
        habitat |= HABITAT_SPRING;
        if arid {
            habitat |= HABITAT_OASIS;
        }
    }
    if !aquatic && lake_neighbor {
        habitat |= HABITAT_LAKESHORE;
    }
    if hydro.flags & (HYDRO_ESTUARY | HYDRO_DELTA) != 0 {
        habitat |= HABITAT_ESTUARY_DELTA;
    }
    if !aquatic && ocean_neighbor {
        habitat |= HABITAT_BEACH_DUNE;
        if ground.drainage < 105 && ground.soil_salinity >= 64 {
            habitat |= HABITAT_SALT_MARSH;
        }
    }
    if aquatic {
        habitat |= if fresh {
            HABITAT_AQUATIC_FRESH
        } else if brackish {
            HABITAT_AQUATIC_BRACKISH
        } else {
            HABITAT_AQUATIC_SALT
        };
    }
    if hydro.flags & HYDRO_KARST_LOSS != 0
        || (BedrockFamily::from_id(tectonic.bedrock_family) == BedrockFamily::Limestone && spring)
    {
        habitat |= HABITAT_CAVE_OUTLET;
    }
    if zonal == BIOME_MOUNTAINS || terrain.eroded_elevation > tree_line {
        habitat |= HABITAT_ALPINE;
    }
    if tectonic.volcanic_history != 0 || terrain.volcanic_contribution > 1.5 {
        habitat |= HABITAT_VOLCANIC_SOIL;
    }
    if ground.freeze_flags & FREEZE_PERMAFROST != 0 {
        habitat |= HABITAT_PERMAFROST;
    }

    let mut edaphic = 0u16;
    if ground.soil_depth_decimeters < 5 {
        edaphic |= EDAPHIC_SHALLOW_ROCK;
    }
    if BedrockFamily::from_id(tectonic.bedrock_family) == BedrockFamily::Limestone {
        edaphic |= EDAPHIC_LIMESTONE;
    }
    if habitat & HABITAT_BEACH_DUNE != 0 || (ground.sand > 180 && arid) {
        edaphic |= EDAPHIC_DUNE;
    }
    if habitat & HABITAT_VOLCANIC_SOIL != 0 {
        edaphic |= EDAPHIC_VOLCANIC;
    }
    if hydro.flags & (HYDRO_FLOODPLAIN | HYDRO_DELTA) != 0 || hydro.deposition_centiblocks > 50 {
        edaphic |= EDAPHIC_ALLUVIAL;
    }
    if slope > 11.0 {
        edaphic |= EDAPHIC_STEEP;
    }
    if arid && warmest > 12.0 && climate.precipitation_seasonality > 0.42 {
        edaphic |= EDAPHIC_FIRE_FAVORED;
    }
    if habitat & HABITAT_PERMAFROST != 0 {
        edaphic |= EDAPHIC_PERMAFROST;
    }

    let water_factor = (climate.mean_precipitation
        / climate.potential_evapotranspiration.max(220.0))
    .clamp(0.0, 1.6);
    let energy = ((warmest + 8.0) / 30.0).clamp(0.0, 1.0);
    let fertility = f32::from(ground.baseline_fertility) / 255.0;
    let salinity_penalty = 1.0 - f32::from(ground.soil_salinity) / 300.0;
    let habitat_bonus = if habitat & (HABITAT_RIPARIAN | HABITAT_OASIS | HABITAT_WETLAND) != 0 {
        0.28
    } else {
        0.0
    };
    let vegetation =
        ((water_factor.min(1.0) * 0.44 + energy * 0.30 + fertility * 0.26 + habitat_bonus)
            * salinity_penalty.clamp(0.0, 1.0)
            * 255.0)
            .clamp(0.0, 255.0) as u8;
    let succession = (f32::from(vegetation) * 0.65
        + f32::from(ground.organic) * 0.22
        + if edaphic & EDAPHIC_FIRE_FAVORED != 0 {
            -28.0
        } else {
            18.0
        })
    .clamp(0.0, 255.0) as u8;

    BiomeCell {
        baseline_biome: zonal,
        edaphic_flags: edaphic,
        habitat_flags: habitat,
        vegetation_potential: vegetation,
        tree_line_y: tree_line as u8,
        succession_potential: succession,
        country_id: 0,
        heart_assignment: 0,
    }
}
