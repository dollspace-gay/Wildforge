//! Channel geometry, habitats, sediment, and downstream salinity.

use crate::chunk::SEA_LEVEL;
use crate::planet_atlas::{AtlasError, AtlasGrid, BedrockFamily, ClimateCell, GeometryCell, HydrologyCell, TectonicCell};
use super::{OceanBasinRecord, RiverRecord, WaterBodyKind, HYDRO_DELTA, HYDRO_ESTUARY, HYDRO_FLOODPLAIN, HYDRO_INTERMITTENT, HYDRO_KARST_LOSS, HYDRO_PERENNIAL, HYDRO_RIVER, HYDRO_WATERFALL};
use super::flow::{FlowAccumulation, edge_distance, stream_orders};
use super::lakes::LakeSolution;
use super::rivers::{RiverInput, river_records};
use super::runoff::{bedrock_resistance, local_runoff, normalized_fractions};

pub(super) struct ChannelInput<'a> {
    pub(super) seed: u32,
    pub(super) side: u16,
    pub(super) elevations: &'a [f32],
    pub(super) receiver: &'a [u32],
    pub(super) geometry: &'a AtlasGrid<GeometryCell>,
    pub(super) tectonics: &'a AtlasGrid<TectonicCell>,
    pub(super) climate: &'a AtlasGrid<ClimateCell>,
    pub(super) flow: &'a FlowAccumulation,
    pub(super) watershed: &'a [u32],
    pub(super) lake: &'a LakeSolution,
    pub(super) ocean: &'a [u16],
    pub(super) erosion: &'a [f32],
    pub(super) deposition: &'a [f32],
    pub(super) cells: &'a mut [HydrologyCell],
    pub(super) oceans: &'a [OceanBasinRecord],
}

pub(super) fn assign_channels(input: ChannelInput<'_>) -> Result<Vec<RiverRecord>, AtlasError> {
    let ChannelInput {
        seed,
        side,
        elevations,
        receiver,
        geometry,
        tectonics,
        climate,
        flow,
        watershed,
        lake,
        ocean,
        erosion,
        deposition,
        cells,
        oceans,
    } = input;
    let mean_area = geometry
        .values()
        .iter()
        .map(|cell| f64::from(cell.physical_area))
        .sum::<f64>()
        / geometry.len() as f64;
    let threshold = mean_area * 1.15;
    let mut channel = vec![false; cells.len()];
    for index in 0..cells.len() {
        if ocean[index] != 0 || lake.membership[index] != 0 {
            continue;
        }
        let runoff_equivalent = flow.annual[index] / threshold;
        channel[index] = runoff_equivalent >= 1.0 && receiver[index] != u32::MAX;
    }
    let orders = stream_orders(receiver, &channel)?;
    for index in 0..cells.len() {
        let runoff_data = local_runoff(climate.values()[index], tectonics.values()[index]);
        cells[index].drainage_receiver = receiver[index];
        cells[index].watershed_id = watershed[index];
        cells[index].mean_runoff = runoff_data.0;
        cells[index].seasonal_runoff_fraction = normalized_fractions(runoff_data.1);
        cells[index].mean_discharge = flow.annual[index].min(f32::MAX as f64) as f32;
        cells[index].seasonal_discharge_fraction = normalized_fractions(flow.seasonal[index]);
        cells[index].catchment_area = flow.area[index].min(f32::MAX as f64) as f32;
        cells[index].erosion_centiblocks =
            (erosion[index] * 100.0).round().clamp(0.0, i16::MAX as f32) as i16;
        cells[index].deposition_centiblocks = (deposition[index] * 100.0)
            .round()
            .clamp(0.0, i16::MAX as f32) as i16;
        if !channel[index] {
            continue;
        }
        let equivalent = (flow.annual[index] / mean_area).max(0.01) as f32;
        let resistance = bedrock_resistance(tectonics.values()[index]);
        let mut width = ((0.9 + equivalent.sqrt() * 1.45) / resistance.sqrt()).clamp(1.2, 28.0);
        let mut depth = ((0.75 + equivalent.powf(0.31) * 0.82) * resistance.sqrt()).clamp(1.0, 8.0);
        let next = receiver[index] as usize;
        let slope = ((cells[index].filled_elevation - cells[next].filled_elevation)
            / edge_distance(side, index, next) as f32)
            .max(0.0);
        // The receiver of a mouth is an ocean-floor cell. Measuring the
        // land-to-seafloor drop classified every coast as steep and made
        // deltas impossible even on broad low coastal plains. Delta relief
        // is the height of the last alluvial land above sea level; discharge
        // supplies the sediment. Higher/smaller mouths remain estuaries.
        let coastal_relief = (elevations[index] - SEA_LEVEL as f32).max(0.0);
        let delta_mouth = ocean[next] != 0
            && equivalent > 3.0
            && (coastal_relief <= 4.0 || slope < 0.006)
            && resistance <= 1.25;
        let estuary_mouth = ocean[next] != 0 && !delta_mouth;
        if delta_mouth {
            width = (width * 1.75).min(40.0);
            depth = (depth * 0.72).max(1.0);
            cells[index].deposition_centiblocks = cells[index]
                .deposition_centiblocks
                .saturating_add((equivalent.sqrt() * 18.0).round() as i16);
        } else if estuary_mouth {
            width = (width * 1.25).min(34.0);
        }
        let minimum_surface = if ocean[next] == 0 {
            SEA_LEVEL as f32 + 1.0
        } else {
            SEA_LEVEL as f32
        };
        let water_surface = (elevations[index] - 0.35).floor().max(minimum_surface);
        cells[index].channel_bed_elevation = (water_surface - depth).max(2.0);
        cells[index].water_surface_elevation = water_surface;
        cells[index].channel_width_centiblocks = (width * 100.0).round() as u16;
        cells[index].channel_depth_centiblocks = (depth * 100.0).round() as u16;
        cells[index].sediment_energy =
            ((slope * 3600.0 + equivalent.sqrt() * 900.0).clamp(0.0, 65_535.0)) as u16;
        cells[index].stream_order = orders[index];
        cells[index].flags |= HYDRO_RIVER;
        let min_share = cells[index]
            .seasonal_discharge_fraction
            .iter()
            .copied()
            .min()
            .unwrap_or(0);
        if min_share >= 3_000 || climate.values()[index].mean_precipitation > 1_000.0 {
            cells[index].flags |= HYDRO_PERENNIAL;
        } else {
            cells[index].flags |= HYDRO_INTERMITTENT;
        }
        if slope < 0.0045 && equivalent > 2.0 {
            cells[index].flags |= HYDRO_FLOODPLAIN;
        }
        if slope > 0.075 || elevations[index] - elevations[next] > 5.0 {
            cells[index].flags |= HYDRO_WATERFALL;
        }
        if BedrockFamily::from_id(tectonics.values()[index].bedrock_family)
            == BedrockFamily::Limestone
            && cells[index].flags & HYDRO_INTERMITTENT != 0
        {
            cells[index].flags |= HYDRO_KARST_LOSS;
        }
        let dissolved = match BedrockFamily::from_id(tectonics.values()[index].bedrock_family) {
            BedrockFamily::Limestone | BedrockFamily::Evaporite => 18.0,
            BedrockFamily::Shale | BedrockFamily::Sandstone => 8.0,
            _ => 3.0,
        };
        cells[index].salinity = (dissolved + climate.values()[index].aridity * 6.0)
            .round()
            .clamp(0.0, 63.0) as u8;
        cells[index].water_body = WaterBodyKind::River;
        if delta_mouth {
            cells[index].flags |= HYDRO_DELTA;
            cells[index].water_body = WaterBodyKind::Delta;
        } else if estuary_mouth {
            cells[index].flags |= HYDRO_ESTUARY;
            cells[index].water_body = WaterBodyKind::Estuary;
            cells[index].salinity = 96;
        }
        let length = edge_distance(side, index, next) as f32;
        let continuous = f64::from(length * width * depth * 8.0);
        let voxel = f64::from(length.floor() * width.floor().max(1.0) * depth.floor() * 8.0);
        let baseline_wet = cells[index].flags & HYDRO_PERENNIAL != 0 || equivalent >= 4.0;
        if baseline_wet {
            cells[index].baseline_water_units = continuous.round().max(0.0) as u64;
            cells[index].voxel_volume_residual = (continuous - voxel)
                .round()
                .clamp(i32::MIN as f64, i32::MAX as f64)
                as i32;
        }
    }
    // Dissolved load can accumulate or mix but cannot spontaneously vanish
    // at a lithological boundary. Estuaries are the explicit salt-mixing
    // exception and already carry their brackish genesis concentration.
    for &index in &flow.order {
        if !channel[index] || receiver[index] == u32::MAX {
            continue;
        }
        let next = receiver[index] as usize;
        if channel[next] && cells[next].flags & HYDRO_ESTUARY == 0 {
            cells[next].salinity = cells[next].salinity.max(cells[index].salinity).min(63);
        }
    }
    // Priority-flood routing is acyclic, so a source-to-sink pass can lower
    // any resistant flat step without ever revisiting an upstream bed.
    for &index in &flow.order {
        if !channel[index] || receiver[index] == u32::MAX {
            continue;
        }
        let next = receiver[index] as usize;
        if !channel[next] {
            continue;
        }
        cells[next].channel_bed_elevation = cells[next]
            .channel_bed_elevation
            .min((cells[index].channel_bed_elevation - 0.002).max(2.0));
        cells[next].water_surface_elevation = cells[next]
            .water_surface_elevation
            .min(cells[index].water_surface_elevation);
    }
    Ok(river_records(RiverInput {
        seed,
        side,
        receiver,
        watershed,
        channel: &channel,
        cells,
        model_lakes: &lake.records,
        model_oceans: oceans,
    }))
}
