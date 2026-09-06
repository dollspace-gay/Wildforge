//! Bounded erosion iterations over the ordered drainage graph.

use super::flood::priority_flood;
use super::flow::{accumulate_flow, edge_distance};
use super::runoff::{bedrock_resistance, local_runoff};
use super::{EROSION_CONVERGENCE_BLOCKS, MAX_EROSION_ITERATIONS};
use crate::chunk::SEA_LEVEL;
use crate::planet_atlas::{AtlasError, AtlasGrid, ClimateCell, GeometryCell, TectonicCell};

pub(super) struct ErosionResult {
    pub(super) elevations: Vec<f32>,
    pub(super) iterations: u8,
    pub(super) residual: f32,
    pub(super) erosion: Vec<f32>,
    pub(super) deposition: Vec<f32>,
}

pub(super) fn erosion_loop(
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    tectonics: &AtlasGrid<TectonicCell>,
    climate: &AtlasGrid<ClimateCell>,
    original: &[f32],
) -> Result<ErosionResult, AtlasError> {
    let mut elevations = original.to_vec();
    let mut total_erosion = vec![0.0f32; elevations.len()];
    let mut total_deposition = vec![0.0f32; elevations.len()];
    let runoff_data: Vec<_> = climate
        .values()
        .iter()
        .copied()
        .zip(tectonics.values().iter().copied())
        .map(|(climate, tectonics)| local_runoff(climate, tectonics))
        .collect();
    let runoff: Vec<f32> = runoff_data.iter().map(|value| value.0).collect();
    let seasonal: Vec<[f64; 4]> = runoff_data.iter().map(|value| value.1).collect();
    let mean_area = geometry
        .values()
        .iter()
        .map(|cell| f64::from(cell.physical_area))
        .sum::<f64>()
        / geometry.len() as f64;
    let mut iterations = 0;
    let mut residual = 0.0f32;
    for iteration in 0..MAX_EROSION_ITERATIONS {
        let (filled, receiver) = priority_flood(side, &elevations);
        let flow = accumulate_flow(&receiver, geometry, &runoff, &seasonal)?;
        let mut next = elevations.clone();
        residual = 0.0;
        let damping = 0.72f32.powi(i32::from(iteration));
        for index in 0..elevations.len() {
            let ocean = original[index] <= SEA_LEVEL as f32;
            if ocean {
                continue;
            }
            let depression = (filled[index] - elevations[index]).max(0.0);
            let receiver_index = receiver[index];
            let mut erosion = 0.0f32;
            let mut deposition = 0.0f32;
            if receiver_index != u32::MAX {
                let receiver_index = receiver_index as usize;
                let slope = ((filled[index] - filled[receiver_index])
                    / edge_distance(side, index, receiver_index) as f32)
                    .max(0.0);
                let discharge = (flow.annual[index] / mean_area).max(0.0) as f32;
                let power = discharge.ln_1p() * (slope * 48.0).sqrt();
                erosion = (power * 0.22 / bedrock_resistance(tectonics.values()[index])).min(0.72)
                    * damping;
                if slope < 0.0025 && discharge > 1.0 {
                    deposition = ((0.0025 - slope) * discharge.sqrt() * 12.0).min(0.22) * damping;
                }
            }
            if depression > 0.2 {
                deposition += depression.min(1.0) * 0.16 * damping;
            }
            let minimum = SEA_LEVEL as f32 + 0.35;
            let changed = (elevations[index] - erosion + deposition)
                .max(minimum)
                .min(CHANNEL_CEILING);
            let delta = changed - elevations[index];
            next[index] = changed;
            residual = residual.max(delta.abs());
            if delta < 0.0 {
                total_erosion[index] -= delta;
            } else {
                total_deposition[index] += delta;
            }
        }
        elevations = next;
        iterations = iteration + 1;
        if residual <= EROSION_CONVERGENCE_BLOCKS {
            break;
        }
    }
    Ok(ErosionResult {
        elevations,
        iterations,
        residual,
        erosion: total_erosion,
        deposition: total_deposition,
    })
}

const CHANNEL_CEILING: f32 = crate::chunk::CHUNK_Y as f32 - 2.0;
