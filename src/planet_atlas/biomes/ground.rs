//! Soil texture, fertility, freezing, and groundwater genesis.

use super::classification::slope_at;
use super::{FREEZE_PERMAFROST, FREEZE_SEASONAL};
use crate::chunk::SEA_LEVEL;
use crate::planet_atlas::grid::generate_grid;
use crate::planet_atlas::identity::cell_hash;
use crate::planet_atlas::{
    AtlasError, AtlasGrid, BedrockFamily, ClimateCell, GenerationMode, GroundCell, HYDRO_DELTA,
    HYDRO_FLOODPLAIN, HYDRO_LAKE, HYDRO_OCEAN, HYDRO_PLAYA, HYDRO_TERMINAL, HYDRO_WETLAND,
    HydrologyCell, TectonicCell, TerrainCell,
};
fn parent_texture(parent: BedrockFamily) -> (u8, u8) {
    match parent {
        BedrockFamily::Sandstone => (188, 48),
        BedrockFamily::Limestone => (92, 96),
        BedrockFamily::Shale => (42, 92),
        BedrockFamily::Granite | BedrockFamily::Quartzite => (126, 72),
        BedrockFamily::Basalt | BedrockFamily::Ultramafic => (72, 92),
        BedrockFamily::Evaporite => (202, 35),
        BedrockFamily::Marble | BedrockFamily::Slate => (76, 94),
        BedrockFamily::MixedBasement => (108, 82),
    }
}

pub(in crate::planet_atlas) fn generate_ground_layer(
    seed: u32,
    side: u16,
    mode: GenerationMode,
    tectonics: &AtlasGrid<TectonicCell>,
    terrain: &AtlasGrid<TerrainCell>,
    climate: &AtlasGrid<ClimateCell>,
    hydrology: &AtlasGrid<HydrologyCell>,
) -> Result<AtlasGrid<GroundCell>, AtlasError> {
    let climate_grid = climate;
    generate_grid(side, mode, |pos| {
        let index = pos.index(side);
        let climate = climate.values()[index];
        let terrain_cell = terrain.values()[index];
        let hydro = hydrology.values()[index];
        let tectonic = tectonics.values()[index];
        let parent = BedrockFamily::from_id(tectonic.bedrock_family);
        let slope = slope_at(index, side, terrain);
        let geological_permeability: f32 = match parent {
            BedrockFamily::Limestone => 0.78,
            BedrockFamily::Sandstone => 0.58,
            BedrockFamily::Shale => 0.16,
            BedrockFamily::Granite | BedrockFamily::Quartzite => 0.20,
            BedrockFamily::Basalt | BedrockFamily::Ultramafic => 0.42,
            _ => 0.34,
        };
        let variation = (cell_hash(seed, pos, 0x5045_524d) & 0xffff) as f32 / 65_535.0;
        let permeability = ((geological_permeability * 0.82 + variation * 0.18) * 65_535.0) as u16;
        let alluvial = hydro.flags & (HYDRO_FLOODPLAIN | HYDRO_DELTA) != 0;
        let (mut sand, mut silt) = parent_texture(parent);
        if alluvial {
            sand = sand.saturating_sub(24);
            silt = silt.saturating_add(30).min(220);
        }
        let sum = u16::from(sand) + u16::from(silt);
        if sum > 245 {
            silt = (245 - u16::from(sand).min(245)) as u8;
        }
        let wetness = (climate.mean_precipitation
            / climate.potential_evapotranspiration.max(180.0))
        .clamp(0.0, 2.5);
        let deposition = f32::from(hydro.deposition_centiblocks) / 100.0;
        let depth_dm =
            (4.0 + wetness * 10.0 + deposition * 2.5 - slope * 0.42).clamp(1.0, 80.0) as u8;
        let baseline_groundwater_head = if hydro.flags & (HYDRO_OCEAN | HYDRO_LAKE) != 0 {
            hydro.water_surface_elevation - 0.5
        } else if hydro.flags & (HYDRO_WETLAND | HYDRO_FLOODPLAIN) != 0 {
            terrain_cell.eroded_elevation - 1.0
        } else {
            // A local recharge table sits several blocks down. Lateral
            // hydraulic pressure from a wetter/higher neighboring cell can
            // lift it against a valley wall or footslope, creating a finite
            // spring outlet instead of sprinkling decorative springs by hash.
            let local_head = terrain_cell.eroded_elevation - 4.0 - climate.aridity;
            let lateral_head = pos
                .neighbors4(side)
                .into_iter()
                .filter_map(|neighbor| {
                    let next = neighbor.index(side);
                    let neighbor_terrain = terrain.values()[next];
                    let neighbor_climate = climate_grid.values()[next];
                    if neighbor_terrain.eroded_elevation <= SEA_LEVEL as f32
                        || neighbor_climate.mean_precipitation < 140.0
                    {
                        return None;
                    }
                    let recharge = (neighbor_climate.mean_precipitation
                        / neighbor_climate.potential_evapotranspiration.max(220.0))
                    .clamp(0.0, 1.0);
                    let transmission_loss =
                        1.0 + (1.0 - geological_permeability) * 3.0 + (1.0 - recharge) * 2.0;
                    Some(
                        neighbor_terrain.eroded_elevation
                            - 4.0
                            - neighbor_climate.aridity
                            - transmission_loss,
                    )
                })
                .fold(f32::NEG_INFINITY, f32::max);
            local_head
                .max(lateral_head)
                .min(terrain_cell.eroded_elevation - 0.5)
        };
        let water_table_depth =
            (terrain_cell.eroded_elevation - baseline_groundwater_head).max(0.0);
        let drainage = ((geological_permeability * 120.0
            + f32::from(sand) * 0.52
            + slope * 4.0
            + water_table_depth * 5.0)
            .clamp(0.0, 255.0)) as u8;
        let organic = ((wetness * 72.0 + (18.0 - climate.mean_temperature).max(0.0) * 1.8
            - climate.aridity * 22.0
            - slope * 1.6)
            .clamp(0.0, 255.0)) as u8;
        let salinity = if hydro.salinity > 0 {
            hydro.salinity
        } else if hydro.flags & (HYDRO_TERMINAL | HYDRO_PLAYA) != 0 {
            (climate.aridity * 62.0).clamp(0.0, 190.0) as u8
        } else {
            ((climate.aridity - 1.0).max(0.0) * 22.0).clamp(0.0, 63.0) as u8
        };
        let volcanic = tectonic.volcanic_history != 0 || terrain_cell.volcanic_contribution > 1.5;
        let fertility = (38.0
            + f32::from(organic) * 0.56
            + if volcanic { 45.0 } else { 0.0 }
            + if alluvial { 48.0 } else { 0.0 }
            + if parent == BedrockFamily::Limestone {
                15.0
            } else {
                0.0
            }
            - f32::from(salinity) * 0.52
            - f32::from(sand) * 0.10)
            .clamp(0.0, 255.0) as u8;
        let permafrost = climate.mean_temperature < -5.0 && climate.snow_persistence > 0.45;
        let seasonal_freeze = permafrost
            || climate
                .seasonal_temperature
                .into_iter()
                .any(|temperature| temperature < 0.0);
        let freeze_flags = (u8::from(seasonal_freeze) * FREEZE_SEASONAL)
            | (u8::from(permafrost) * FREEZE_PERMAFROST);
        let erosion = ((slope * 7.0
            + hydro.mean_runoff.sqrt() * 2.2
            + if depth_dm < 5 { 34.0 } else { 0.0 }
            + if matches!(parent, BedrockFamily::Shale | BedrockFamily::Sandstone) {
                22.0
            } else {
                0.0
            })
        .clamp(0.0, 255.0)) as u8;
        GroundCell {
            soil_parent_material: tectonic.bedrock_family,
            aquifer_capacity: ((climate.mean_precipitation + hydro.mean_runoff * 0.65)
                * (24.0 + f32::from(permeability) / 4096.0)) as u32,
            aquifer_permeability: permeability,
            porosity: ((65_535u32 - u32::from(permeability)) / 2 + u32::from(permeability) / 3)
                as u16,
            baseline_groundwater_head,
            soil_depth_decimeters: depth_dm,
            sand,
            silt,
            organic,
            baseline_fertility: fertility,
            drainage,
            soil_salinity: salinity,
            freeze_flags,
            erosion_susceptibility: erosion,
        }
    })
}
