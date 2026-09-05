//! Whole-planet static hydrology.
//!
//! This module turns climate-normal water and geological relief into one
//! acyclic drainage graph before any voxel chunk exists.  The dense cell
//! layer owns routing and materialization constraints; this sparse model owns
//! named rivers, reservoir curves, lake budgets, and ocean connections.

use serde::{Deserialize, Serialize};
use crate::chunk::SEA_LEVEL;
use crate::planet::FACE_BLOCKS;
use super::{AtlasError, AtlasGrid, CancellationToken, ClimateCell, GeometryCell, HydrologyCell, TectonicCell, TerrainCell};

mod records;
pub use records::{WaterBodyKind, LakeClass, StoragePoint, OceanBasinRecord, LakeRecord, RiverRecord, WatershedRecord};
mod flood;
mod runoff;
mod flow;
mod oceans;
mod erosion;
mod basin_shape;
mod lake_candidates;
mod lakes;
mod watersheds;
mod rivers;
mod channels;
mod validation;
mod sampling;
pub use sampling::AtlasHydrologySample;
mod placers;
pub(super) use placers::route_placer_deposits;

use basin_shape::shape_supported_basins;
use channels::{ChannelInput, assign_channels};
use erosion::{ErosionResult, erosion_loop};
use flood::{neighbors8_indices, priority_flood};
use flow::accumulate_flow;
use lakes::{LakeInput, solve_lakes};
use oceans::{label_oceans, ocean_records};
use runoff::local_runoff;
use watersheds::{WatershedInput, assign_watersheds};

pub const HYDROLOGY_SCHEMA_VERSION: u32 = 1;
const MAX_EROSION_ITERATIONS: u8 = 5;
const EROSION_CONVERGENCE_BLOCKS: f32 = 0.035;

pub const HYDRO_RIVER: u16 = 1 << 0;
pub const HYDRO_PERENNIAL: u16 = 1 << 1;
pub const HYDRO_INTERMITTENT: u16 = 1 << 2;
pub const HYDRO_FLOODPLAIN: u16 = 1 << 3;
pub const HYDRO_WETLAND: u16 = 1 << 4;
pub const HYDRO_DELTA: u16 = 1 << 5;
pub const HYDRO_ESTUARY: u16 = 1 << 6;
pub const HYDRO_WATERFALL: u16 = 1 << 7;
pub const HYDRO_TERMINAL: u16 = 1 << 8;
pub const HYDRO_PLAYA: u16 = 1 << 9;
pub const HYDRO_KARST_LOSS: u16 = 1 << 10;
pub const HYDRO_OCEAN: u16 = 1 << 11;
pub const HYDRO_LAKE: u16 = 1 << 12;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HydrologyModel {
    pub schema_version: u32,
    pub sea_level: f32,
    pub dominant_ocean_id: u16,
    pub erosion_iterations: u8,
    pub erosion_max_residual: f32,
    pub baseline_surface_water_units: u128,
    pub voxel_volume_residual: i128,
    pub oceans: Vec<OceanBasinRecord>,
    pub lakes: Vec<LakeRecord>,
    pub rivers: Vec<RiverRecord>,
    pub watersheds: Vec<WatershedRecord>,
}

impl Default for HydrologyModel {
    fn default() -> Self {
        Self {
            schema_version: HYDROLOGY_SCHEMA_VERSION,
            sea_level: SEA_LEVEL as f32,
            dominant_ocean_id: 1,
            erosion_iterations: 0,
            erosion_max_residual: 0.0,
            baseline_surface_water_units: 0,
            voxel_volume_residual: 0,
            oceans: Vec::new(),
            lakes: Vec::new(),
            rivers: Vec::new(),
            watersheds: Vec::new(),
        }
    }
}

pub(super) struct HydrologyOutput {
    pub terrain: AtlasGrid<TerrainCell>,
    pub cells: AtlasGrid<HydrologyCell>,
    pub model: HydrologyModel,
}

pub(super) fn generate_hydrology(
    seed: u32,
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    tectonics: &AtlasGrid<TectonicCell>,
    preliminary_terrain: &AtlasGrid<TerrainCell>,
    climate: &AtlasGrid<ClimateCell>,
    cancel: &CancellationToken,
) -> Result<HydrologyOutput, AtlasError> {
    if cancel.is_cancelled() {
        return Err(AtlasError::Cancelled);
    }
    let original: Vec<f32> = preliminary_terrain
        .values()
        .iter()
        .map(|cell| cell.eroded_elevation)
        .collect();
    let ErosionResult {
        mut elevations,
        iterations,
        residual: erosion_residual,
        mut erosion,
        deposition,
    } = erosion_loop(side, geometry, tectonics, climate, &original)?;
    shape_supported_basins(
        seed,
        side,
        geometry,
        tectonics,
        climate,
        &mut elevations,
        &mut erosion,
    )?;
    if cancel.is_cancelled() {
        return Err(AtlasError::Cancelled);
    }
    let mut terrain_values = preliminary_terrain.values().to_vec();
    for (index, elevation) in elevations.iter().copied().enumerate() {
        terrain_values[index].eroded_elevation = elevation;
    }
    let terrain = AtlasGrid::from_values(side, terrain_values)?;
    let (ocean_labels, ocean_count) = label_oceans(side, &elevations)?;
    let (mut oceans, dominant_ocean_id) = ocean_records(
        seed,
        side,
        &elevations,
        geometry,
        &ocean_labels,
        ocean_count,
    );
    let (filled, mut receiver) = priority_flood(side, &elevations);
    // Build the two arrays directly. Keeping an intermediate tuple vector
    // alive beside both outputs cost roughly 15 MiB at production size and
    // pushed creation needlessly over the 512 MiB process envelope.
    let mut runoff = Vec::with_capacity(elevations.len());
    let mut seasonal_runoff = Vec::with_capacity(elevations.len());
    for (climate, tectonics) in climate
        .values()
        .iter()
        .copied()
        .zip(tectonics.values().iter().copied())
    {
        let (mean, seasonal) = local_runoff(climate, tectonics);
        runoff.push(mean);
        seasonal_runoff.push(seasonal);
    }
    let preliminary_flow = accumulate_flow(&receiver, geometry, &runoff, &seasonal_runoff)?;
    let mut lakes = solve_lakes(LakeInput {
        seed,
        side,
        elevations: &elevations,
        filled: &filled,
        receiver: &mut receiver,
        geometry,
        climate,
        tectonics,
        flow: &preliminary_flow,
    });
    let flow = accumulate_flow(&receiver, geometry, &runoff, &seasonal_runoff)?;
    let (watershed, watersheds) = assign_watersheds(WatershedInput {
        side,
        receiver: &receiver,
        ocean: &ocean_labels,
        lake: &lakes.membership,
        terminal_sink: &lakes.terminal_sink,
        geometry,
        runoff: &runoff,
        seed,
    })?;
    let mut cells = vec![HydrologyCell::default(); elevations.len()];
    let cell_columns = u64::from(FACE_BLOCKS / side).pow(2);
    for index in 0..cells.len() {
        cells[index].drainage_receiver = receiver[index];
        cells[index].watershed_id = watershed[index];
        cells[index].ocean_basin_id = ocean_labels[index];
        cells[index].lake_basin_id = lakes.membership[index];
        cells[index].spill_elevation = if lakes.membership[index] != 0 {
            lakes
                .records
                .iter()
                .find(|lake| lake.id == lakes.membership[index])
                .map_or(filled[index], |lake| lake.spill_elevation)
        } else {
            filled[index]
        };
        cells[index].filled_elevation = filled[index];
        cells[index].channel_bed_elevation = elevations[index];
        cells[index].water_surface_elevation = elevations[index];
        cells[index].erosion_centiblocks =
            (erosion[index] * 100.0).round().clamp(0.0, i16::MAX as f32) as i16;
        cells[index].deposition_centiblocks = (deposition[index] * 100.0)
            .round()
            .clamp(0.0, i16::MAX as f32) as i16;
        if ocean_labels[index] != 0 {
            let ocean = oceans
                .iter()
                .find(|ocean| ocean.id == ocean_labels[index])
                .expect("ocean label has a record");
            let depth = (SEA_LEVEL as f32 - elevations[index]).max(0.0);
            let continuous = f64::from(depth) * cell_columns as f64 * 8.0;
            let voxel = f64::from(depth.floor()) * cell_columns as f64 * 8.0;
            cells[index].water_surface_elevation = SEA_LEVEL as f32;
            cells[index].channel_depth_centiblocks =
                (depth * 100.0).round().clamp(0.0, u16::MAX as f32) as u16;
            cells[index].salinity = ocean.salinity;
            cells[index].flags = HYDRO_OCEAN;
            cells[index].water_body = WaterBodyKind::Ocean;
            cells[index].baseline_water_units = continuous.round() as u64;
            cells[index].voxel_volume_residual = (continuous - voxel)
                .round()
                .clamp(i32::MIN as f64, i32::MAX as f64)
                as i32;
        } else if lakes.membership[index] != 0 {
            let record = lakes
                .records
                .iter()
                .find(|record| record.id == lakes.membership[index])
                .expect("lake membership has a record");
            let depth = (lakes.surface[index] - elevations[index]).max(0.0);
            let continuous = f64::from(depth) * cell_columns as f64 * 8.0;
            let voxel = f64::from(depth.floor()) * cell_columns as f64 * 8.0;
            cells[index].water_surface_elevation = lakes.surface[index];
            cells[index].channel_depth_centiblocks =
                (depth * 100.0).round().clamp(0.0, u16::MAX as f32) as u16;
            cells[index].seasonal_level_range_centiblocks =
                (record.seasonal_level_range * 100.0).round() as u16;
            cells[index].salinity = lakes.salinity[index];
            cells[index].flags = HYDRO_LAKE;
            cells[index].water_body = if record.class == LakeClass::SeasonalPlaya {
                cells[index].flags |= HYDRO_PLAYA | HYDRO_TERMINAL;
                WaterBodyKind::Playa
            } else {
                if record.outlet.is_none() {
                    cells[index].flags |= HYDRO_TERMINAL;
                }
                WaterBodyKind::Lake
            };
            cells[index].baseline_water_units = if record.class == LakeClass::SeasonalPlaya {
                0
            } else {
                continuous.round() as u64
            };
            cells[index].voxel_volume_residual = if record.class == LakeClass::SeasonalPlaya {
                0
            } else {
                (continuous - voxel)
                    .round()
                    .clamp(i32::MIN as f64, i32::MAX as f64) as i32
            };
        }
    }
    let rivers = assign_channels(ChannelInput {
        seed,
        side,
        elevations: &elevations,
        receiver: &receiver,
        geometry,
        tectonics,
        climate,
        flow: &flow,
        watershed: &watershed,
        lake: &lakes,
        ocean: &ocean_labels,
        erosion: &erosion,
        deposition: &deposition,
        cells: &mut cells,
        oceans: &oceans,
    })?;

    // Low-gradient margins of lakes and channels are localized habitat
    // overlays, never province-wide biome assignments.
    for index in 0..cells.len() {
        if ocean_labels[index] != 0 || lakes.membership[index] != 0 {
            continue;
        }
        let adjacent_surface = neighbors8_indices(index, side)
            .into_iter()
            .filter(|neighbor| ocean_labels[*neighbor] != 0 || lakes.membership[*neighbor] != 0)
            .map(|neighbor| cells[neighbor].water_surface_elevation)
            .max_by(f32::total_cmp);
        if adjacent_surface.is_some_and(|water| (-0.5..=2.5).contains(&(elevations[index] - water)))
            && climate.values()[index].aridity < 0.95
        {
            cells[index].flags |= HYDRO_WETLAND;
            if cells[index].water_body == WaterBodyKind::Land {
                cells[index].water_body = WaterBodyKind::Wetland;
            }
        }
    }
    let baseline_surface_water_units = cells
        .iter()
        .map(|cell| u128::from(cell.baseline_water_units))
        .sum();
    let voxel_volume_residual = cells
        .iter()
        .map(|cell| i128::from(cell.voxel_volume_residual))
        .sum();
    for ocean in &mut oceans {
        ocean.baseline_volume_units = cells
            .iter()
            .filter(|cell| cell.ocean_basin_id == ocean.id)
            .fold(0u64, |total, cell| {
                total.saturating_add(cell.baseline_water_units)
            });
    }
    for lake in &mut lakes.records {
        lake.baseline_volume_units = cells
            .iter()
            .filter(|cell| cell.lake_basin_id == lake.id)
            .fold(0u64, |total, cell| {
                total.saturating_add(cell.baseline_water_units)
            });
        lake.voxel_volume_residual = cells
            .iter()
            .filter(|cell| cell.lake_basin_id == lake.id)
            .map(|cell| i64::from(cell.voxel_volume_residual))
            .sum();
    }
    let model = HydrologyModel {
        schema_version: HYDROLOGY_SCHEMA_VERSION,
        sea_level: SEA_LEVEL as f32,
        dominant_ocean_id,
        erosion_iterations: iterations,
        erosion_max_residual: erosion_residual,
        baseline_surface_water_units,
        voxel_volume_residual,
        oceans,
        lakes: lakes.records,
        rivers,
        watersheds,
    };
    let cells = AtlasGrid::from_values(side, cells)?;
    model.validate(side, &terrain, &cells)?;
    Ok(HydrologyOutput {
        terrain,
        cells,
        model,
    })
}
