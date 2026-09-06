//! Deterministic seam-aware priority flooding and neighbor order.

use crate::chunk::SEA_LEVEL;
use crate::planet_atlas::AtlasPos;
use std::cmp::Ordering as CmpOrdering;
use std::collections::BinaryHeap;

#[derive(Clone, Copy, Debug)]
pub(super) struct FloodEntry {
    pub(super) elevation: f32,
    pub(super) index: usize,
}

impl PartialEq for FloodEntry {
    fn eq(&self, other: &Self) -> bool {
        self.elevation.to_bits() == other.elevation.to_bits() && self.index == other.index
    }
}

impl Eq for FloodEntry {}

impl PartialOrd for FloodEntry {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for FloodEntry {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        // Reverse the natural comparison: BinaryHeap then pops the lowest
        // elevation, with the lowest global index breaking flats.
        other
            .elevation
            .total_cmp(&self.elevation)
            .then_with(|| other.index.cmp(&self.index))
    }
}

pub(super) fn neighbors8_indices(index: usize, side: u16) -> Vec<usize> {
    let pos = AtlasPos::from_index(index, side).expect("atlas index");
    let mut out = Vec::with_capacity(8);
    for neighbor in pos.neighbors8(side) {
        let candidate = neighbor.index(side);
        if candidate != index && !out.contains(&candidate) {
            out.push(candidate);
        }
    }
    out.sort_unstable();
    out
}

pub(super) fn priority_flood(side: u16, elevations: &[f32]) -> (Vec<f32>, Vec<u32>) {
    let count = elevations.len();
    let mut filled = elevations.to_vec();
    let mut receiver = vec![u32::MAX; count];
    let mut visited = vec![false; count];
    let mut heap = BinaryHeap::new();
    for (index, elevation) in elevations.iter().copied().enumerate() {
        if elevation <= SEA_LEVEL as f32 {
            visited[index] = true;
            heap.push(FloodEntry { elevation, index });
        }
    }
    // Geology validation guarantees ocean, but retaining this deterministic
    // fallback makes tiny synthetic fixtures fail usefully rather than loop.
    if heap.is_empty()
        && let Some((index, elevation)) = elevations
            .iter()
            .copied()
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
    {
        visited[index] = true;
        heap.push(FloodEntry { elevation, index });
    }
    while let Some(entry) = heap.pop() {
        for neighbor in neighbors8_indices(entry.index, side) {
            if visited[neighbor] {
                continue;
            }
            visited[neighbor] = true;
            filled[neighbor] = elevations[neighbor].max(entry.elevation);
            receiver[neighbor] = entry.index as u32;
            heap.push(FloodEntry {
                elevation: filled[neighbor],
                index: neighbor,
            });
        }
    }
    (filled, receiver)
}
