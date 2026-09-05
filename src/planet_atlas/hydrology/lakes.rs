//! Finite lake budgets and acyclic reservoir outlet resolution.

use std::collections::VecDeque;
use crate::planet::FACE_BLOCKS;
use crate::planet_atlas::{AtlasGrid, AtlasPos, BasinKind, ClimateCell, GeometryCell, TectonicCell, mix64};
use super::LakeRecord;
use super::flood::neighbors8_indices;
use super::flow::{FlowAccumulation, receiver_path_reaches};
use super::lake_candidates::{lake_candidates, lake_class};
use super::records::{generated_word, storage_curve};
use super::runoff::infiltration_fraction;

pub(super) struct LakeSolution {
    pub(super) records: Vec<LakeRecord>,
    pub(super) membership: Vec<u32>,
    pub(super) surface: Vec<f32>,
    pub(super) salinity: Vec<u8>,
    pub(super) terminal_sink: Vec<bool>,
}

pub(super) struct LakeInput<'a> {
    pub(super) seed: u32,
    pub(super) side: u16,
    pub(super) elevations: &'a [f32],
    pub(super) filled: &'a [f32],
    pub(super) receiver: &'a mut [u32],
    pub(super) geometry: &'a AtlasGrid<GeometryCell>,
    pub(super) climate: &'a AtlasGrid<ClimateCell>,
    pub(super) tectonics: &'a AtlasGrid<TectonicCell>,
    pub(super) flow: &'a FlowAccumulation,
}

pub(super) fn solve_lakes(input: LakeInput<'_>) -> LakeSolution {
    let LakeInput {
        seed,
        side,
        elevations,
        filled,
        receiver,
        geometry,
        climate,
        tectonics,
        flow,
    } = input;
    let mut candidates = lake_candidates(side, elevations, filled, tectonics);
    // A real finite planet should expose multiple basins.  Tiny fixtures may
    // have only one; keeping the deepest is enough to exercise the contract.
    candidates.retain(|candidate| {
        candidate.depth >= 1.15
            && (candidate.members.len() >= 2
                || candidate.score >= 3.0
                || tectonics.values()[candidate.sink].sediment_basin != BasinKind::None)
    });
    candidates.truncate(4_096);
    let mut membership = vec![0u32; elevations.len()];
    let mut surface = vec![f32::NAN; elevations.len()];
    let mut salinity = vec![0u8; elevations.len()];
    let mut terminal_sink = vec![false; elevations.len()];
    let mut records = Vec::new();
    for candidate in candidates {
        if candidate
            .members
            .iter()
            .any(|index| membership[*index] != 0)
        {
            continue;
        }
        let potential_evaporation = candidate
            .members
            .iter()
            .map(|index| {
                f64::from(
                    climate.values()[*index]
                        .potential_evapotranspiration
                        .max(0.0),
                ) * f64::from(geometry.values()[*index].physical_area)
                    / 1000.0
            })
            .sum::<f64>();
        let inflow = flow.annual[candidate.sink].max(0.001);
        let aridity = candidate
            .members
            .iter()
            .map(|index| climate.values()[*index].aridity)
            .sum::<f32>()
            / candidate.members.len() as f32;
        let tectonically_closed = candidate
            .members
            .iter()
            .any(|index| tectonics.values()[*index].sediment_basin == BasinKind::Closed);
        let balance = (inflow / potential_evaporation.max(0.001)) as f32;
        let terminal = tectonically_closed || (aridity > 0.95 && balance < 1.08);
        let fill_fraction = if terminal {
            balance.clamp(0.03, 0.92).sqrt()
        } else {
            1.0
        };
        let lake_surface = elevations[candidate.sink]
            + (candidate.spill - elevations[candidate.sink]) * fill_fraction;
        let playa =
            terminal && (fill_fraction < 0.28 || lake_surface - elevations[candidate.sink] < 0.7);
        let class = lake_class(&candidate, terminal, playa, elevations, climate, tectonics);
        let id = records.len() as u32 + 1;
        let mut wet_members = Vec::new();
        for &index in &candidate.members {
            if elevations[index] < lake_surface - 0.04 || (playa && index == candidate.sink) {
                membership[index] = id;
                surface[index] = lake_surface;
                wet_members.push(index);
            }
        }
        if wet_members.is_empty() {
            continue;
        }
        let wet_set: std::collections::BTreeSet<_> = wet_members.iter().copied().collect();
        let root_and_outlet = if terminal {
            Some((candidate.sink, u32::MAX))
        } else {
            // Preserve a real edge from the acyclic priority-flood graph.
            // Merely choosing the geometrically lowest boundary neighbor can
            // choose a cell whose own receiver points back into the lake and
            // create a seed-dependent two-cycle.
            wet_members
                .iter()
                .copied()
                .filter_map(|index| {
                    let next = receiver[index];
                    (next != u32::MAX
                        && !wet_set.contains(&(next as usize))
                        && !receiver_path_reaches(next, receiver, &wet_set))
                    .then_some((index, next))
                })
                .min_by(|(a_index, a_next), (b_index, b_next)| {
                    filled[*a_index]
                        .total_cmp(&filled[*b_index])
                        .then_with(|| a_next.cmp(b_next))
                        .then_with(|| a_index.cmp(b_index))
                })
        };
        let Some((root, resolved_outlet)) = root_and_outlet else {
            for index in wet_members {
                membership[index] = 0;
                surface[index] = f32::NAN;
            }
            continue;
        };
        let mut routed = std::collections::BTreeSet::from([root]);
        let mut queue = VecDeque::from([root]);
        receiver[root] = resolved_outlet;
        while let Some(index) = queue.pop_front() {
            for neighbor in neighbors8_indices(index, side) {
                if wet_set.contains(&neighbor) && routed.insert(neighbor) {
                    receiver[neighbor] = index as u32;
                    queue.push_back(neighbor);
                }
            }
        }
        // A thresholded nested depression can leave a disconnected wet cell.
        // It is a separate pond, not part of this reservoir record.
        for index in wet_members.extract_if(.., |index| !routed.contains(index)) {
            membership[index] = 0;
            surface[index] = f32::NAN;
        }
        if terminal {
            terminal_sink[root] = true;
        }
        let concentration = if terminal {
            (64.0 + aridity.max(0.0) * 84.0 + (1.0 - fill_fraction) * 96.0)
                .round()
                .clamp(8.0, 255.0) as u8
        } else {
            (4.0 + aridity.max(0.0) * 12.0).round().clamp(0.0, 63.0) as u8
        };
        for &index in &wet_members {
            salinity[index] = concentration;
        }
        let curve = storage_curve(side, &candidate.members, elevations, candidate.spill);
        let column_area = f64::from(FACE_BLOCKS / side).powi(2);
        let volume = wet_members
            .iter()
            .map(|index| {
                f64::from((lake_surface - elevations[*index]).max(0.0)) * column_area * 8.0
            })
            .sum::<f64>()
            .round()
            .clamp(0.0, u64::MAX as f64) as u64;
        let seasonal_range = (climate.values()[candidate.sink]
            .precipitation_seasonality
            .abs()
            * 1.8
            / candidate.depth.max(0.5))
        .clamp(0.05, 3.5);
        let evaporation = if terminal {
            inflow
        } else {
            potential_evaporation.min(inflow * 0.85)
        };
        let outflow = if terminal {
            0.0
        } else {
            (inflow - evaporation).max(0.0)
        };
        let hash = mix64(u64::from(seed) ^ u64::from(id) ^ 0x006c_616b_6573);
        records.push(LakeRecord {
            id,
            name: if playa {
                format!("{} Playa", generated_word(hash))
            } else {
                format!("Lake {}", generated_word(hash))
            },
            class,
            sink: AtlasPos::from_index(candidate.sink, side).expect("lake sink"),
            outlet: (!terminal)
                .then(|| AtlasPos::from_index(resolved_outlet as usize, side))
                .flatten(),
            cell_count: wet_members.len().try_into().unwrap_or(u32::MAX),
            catchment_area: flow.area[candidate.sink],
            surface_elevation: lake_surface,
            spill_elevation: candidate.spill,
            baseline_inflow: inflow,
            baseline_evaporation: evaporation,
            baseline_outflow: outflow,
            groundwater_exchange_coefficient: (infiltration_fraction(
                tectonics.values()[candidate.sink],
            ) * 0.18)
                .clamp(0.01, 0.12),
            salinity: concentration,
            seasonal_level_range: seasonal_range,
            // A playa's curve records how much a wet season can hold, but its
            // genesis state is the dry salt flat materialized by the dense
            // atlas cells. Capacity is not baseline water mass.
            baseline_volume_units: if playa { 0 } else { volume },
            voxel_volume_residual: 0,
            volume_elevation_curve: curve,
        });
    }
    LakeSolution {
        records,
        membership,
        surface,
        salinity,
        terminal_sink,
    }
}
