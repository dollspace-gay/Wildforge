//! Deterministic geological and climatic basin shaping.

use super::flood::{neighbors8_indices, priority_flood};
use super::flow::{accumulate_flow, edge_distance};
use super::runoff::local_runoff;
use crate::chunk::SEA_LEVEL;
use crate::planet::geodesic_distance;
use crate::planet_atlas::{
    AtlasError, AtlasGrid, AtlasPos, BasinKind, CLIMATE_SEASONS, ClimateCell, GeometryCell,
    TectonicCell, cell_hash,
};

pub(super) fn shape_supported_basins(
    seed: u32,
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    tectonics: &AtlasGrid<TectonicCell>,
    climate: &AtlasGrid<ClimateCell>,
    elevations: &mut [f32],
    erosion: &mut [f32],
) -> Result<(), AtlasError> {
    let (filled, receiver) = priority_flood(side, elevations);
    let runoff_data: Vec<_> = climate
        .values()
        .iter()
        .copied()
        .zip(tectonics.values().iter().copied())
        .map(|(climate, tectonics)| local_runoff(climate, tectonics))
        .collect();
    let runoff: Vec<f32> = runoff_data.iter().map(|value| value.0).collect();
    let seasonal: Vec<[f64; CLIMATE_SEASONS]> = runoff_data.iter().map(|value| value.1).collect();
    let flow = accumulate_flow(&receiver, geometry, &runoff, &seasonal)?;
    let mean_area = geometry
        .values()
        .iter()
        .map(|cell| f64::from(cell.physical_area))
        .sum::<f64>()
        / geometry.len() as f64;
    // category: rift, crater, alpine/glacial, closed/arid, wet through-flow.
    let mut candidates = Vec::<(u8, f32, u64, usize)>::new();
    for index in 0..elevations.len() {
        if elevations[index] <= SEA_LEVEL as f32 + 5.0 || receiver[index] == u32::MAX {
            continue;
        }
        let tectonic = tectonics.values()[index];
        let local = climate.values()[index];
        let downstream = receiver[index] as usize;
        let slope = ((filled[index] - filled[downstream])
            / edge_distance(side, index, downstream) as f32)
            .max(0.0);
        let discharge = (flow.annual[index] / mean_area) as f32;
        let category = if tectonic.sediment_basin == BasinKind::Rift {
            Some((0, 8.0 + discharge.ln_1p()))
        } else if tectonic.volcanic_history != 0 && tectonic.boundary_distance > 1 {
            Some((1, 7.0 + tectonic.boundary_strength * 2.0))
        } else if elevations[index] > 112.0 && local.mean_temperature < 4.0 {
            Some((2, elevations[index] / 24.0 - local.mean_temperature * 0.1))
        } else if tectonic.sediment_basin == BasinKind::Closed
            || (local.aridity > 1.25 && discharge < 1.2)
        {
            Some((3, local.aridity * 2.0 + (1.2 - discharge).max(0.0)))
        } else if local.aridity < 0.82 && discharge > 2.0 && slope < 0.012 {
            Some((4, discharge.ln_1p() * 2.0 + (0.012 - slope) * 80.0))
        } else {
            None
        };
        if let Some((category, score)) = category {
            let pos = AtlasPos::from_index(index, side).expect("basin candidate");
            candidates.push((
                category,
                score,
                cell_hash(seed, pos, 0x6261_7369_6e5f_7368),
                index,
            ));
        }
    }
    candidates.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| b.1.total_cmp(&a.1))
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.3.cmp(&b.3))
    });
    let targets = [5usize, 5, 6, 8, 12];
    let mut accepted = Vec::<usize>::new();
    let mut counts = [0usize; 5];
    for (category, _, hash, index) in candidates {
        if counts[category as usize] >= targets[category as usize] {
            continue;
        }
        let pos = AtlasPos::from_index(index, side).expect("basin candidate");
        if accepted.iter().any(|other| {
            geodesic_distance(
                pos.center(side),
                AtlasPos::from_index(*other, side)
                    .expect("accepted basin")
                    .center(side),
            ) < 230.0
        }) {
            continue;
        }
        let depth = match category {
            0 => 5.5,
            1 => 4.2,
            2 => 3.2,
            3 => 3.8,
            _ => 2.8,
        } + (hash & 255) as f32 / 255.0 * 1.4;
        let floor = (elevations[index] - depth).max(SEA_LEVEL as f32 + 1.2);
        erosion[index] += elevations[index] - floor;
        elevations[index] = floor;
        for neighbor in neighbors8_indices(index, side) {
            if elevations[neighbor] <= SEA_LEVEL as f32 {
                continue;
            }
            let shoulder_depth = depth * 0.32;
            let shoulder = (elevations[neighbor] - shoulder_depth)
                .max(floor + depth * 0.16)
                .max(SEA_LEVEL as f32 + 1.2);
            erosion[neighbor] += (elevations[neighbor] - shoulder).max(0.0);
            elevations[neighbor] = elevations[neighbor].min(shoulder);
        }
        accepted.push(index);
        counts[category as usize] += 1;
    }
    Ok(())
}
