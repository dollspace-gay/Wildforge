//! Depression candidates and geological lake classifications.

use std::collections::VecDeque;
use crate::chunk::SEA_LEVEL;
use crate::planet_atlas::{AtlasGrid, BasinKind, ClimateCell, TectonicCell};
use super::LakeClass;
use super::flood::neighbors8_indices;

#[derive(Clone, Debug)]
pub(super) struct LakeCandidate {
    pub(super) members: Vec<usize>,
    pub(super) sink: usize,
    pub(super) spill: f32,
    pub(super) depth: f32,
    pub(super) score: f32,
}

pub(super) fn lake_candidates(
    side: u16,
    elevations: &[f32],
    filled: &[f32],
    tectonics: &AtlasGrid<TectonicCell>,
) -> Vec<LakeCandidate> {
    let mut visited = vec![false; elevations.len()];
    let mut candidates = Vec::new();
    for start in 0..elevations.len() {
        if visited[start]
            || elevations[start] <= SEA_LEVEL as f32
            || filled[start] - elevations[start] <= 0.65
        {
            continue;
        }
        let target_fill = filled[start];
        let mut queue = VecDeque::from([start]);
        let mut members = Vec::new();
        visited[start] = true;
        while let Some(index) = queue.pop_front() {
            members.push(index);
            for neighbor in neighbors8_indices(index, side) {
                if !visited[neighbor]
                    && elevations[neighbor] > SEA_LEVEL as f32
                    && filled[neighbor] - elevations[neighbor] > 0.65
                    && (filled[neighbor] - target_fill).abs() <= 0.55
                {
                    visited[neighbor] = true;
                    queue.push_back(neighbor);
                }
            }
        }
        let sink = *members
            .iter()
            .min_by(|a, b| {
                elevations[**a]
                    .total_cmp(&elevations[**b])
                    .then_with(|| a.cmp(b))
            })
            .expect("depression has a member");
        let member_set: std::collections::BTreeSet<_> = members.iter().copied().collect();
        let mut outlet = sink;
        let mut spill = f32::INFINITY;
        for &index in &members {
            for neighbor in neighbors8_indices(index, side) {
                if member_set.contains(&neighbor) {
                    continue;
                }
                let candidate = filled[index]
                    .max(elevations[index])
                    .max(elevations[neighbor]);
                if candidate < spill || (candidate == spill && neighbor < outlet) {
                    spill = candidate;
                    outlet = neighbor;
                }
            }
        }
        if !spill.is_finite() {
            continue;
        }
        let depth = (spill - elevations[sink]).max(0.0);
        let tectonic_bonus = members
            .iter()
            .map(|index| match tectonics.values()[*index].sediment_basin {
                BasinKind::Rift | BasinKind::Closed => 2.0,
                BasinKind::Foreland => 0.8,
                _ => 0.0,
            })
            .fold(0.0f32, f32::max);
        let volcanic_bonus = members
            .iter()
            .any(|index| tectonics.values()[*index].volcanic_history != 0)
            as u8 as f32
            * 1.2;
        let score = depth + (members.len() as f32).ln_1p() * 0.7 + tectonic_bonus + volcanic_bonus;
        candidates.push(LakeCandidate {
            members,
            sink,
            spill,
            depth,
            score,
        });
    }
    candidates.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.sink.cmp(&b.sink))
    });
    candidates
}

pub(super) fn lake_class(
    candidate: &LakeCandidate,
    terminal: bool,
    playa: bool,
    elevations: &[f32],
    climate: &AtlasGrid<ClimateCell>,
    tectonics: &AtlasGrid<TectonicCell>,
) -> LakeClass {
    if playa {
        return LakeClass::SeasonalPlaya;
    }
    if candidate
        .members
        .iter()
        .any(|index| tectonics.values()[*index].volcanic_history != 0)
        && candidate.members.len() <= 12
    {
        return LakeClass::VolcanicCrater;
    }
    if candidate
        .members
        .iter()
        .any(|index| tectonics.values()[*index].sediment_basin == BasinKind::Rift)
    {
        return LakeClass::Rift;
    }
    let sink_climate = climate.values()[candidate.sink];
    if elevations[candidate.sink] > 105.0 && sink_climate.mean_temperature < 5.0 {
        return LakeClass::GlacialAlpine;
    }
    if terminal {
        if sink_climate.aridity > 1.05 {
            LakeClass::SalineTerminal
        } else {
            LakeClass::TerminalFresh
        }
    } else {
        LakeClass::ThroughFlowFresh
    }
}
