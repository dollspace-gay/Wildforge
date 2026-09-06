//! Ocean components, inter-basin sills, and marine reservoir records.

use super::OceanBasinRecord;
use super::flood::{FloodEntry, neighbors8_indices};
use super::records::{generated_word, storage_curve};
use crate::chunk::SEA_LEVEL;
use crate::planet_atlas::{AtlasError, AtlasGrid, GeometryCell, mix64};
use std::collections::{BinaryHeap, VecDeque};

pub(super) fn label_oceans(side: u16, elevations: &[f32]) -> Result<(Vec<u16>, u16), AtlasError> {
    let mut labels = vec![0u16; elevations.len()];
    let mut next = 0u32;
    for start in 0..elevations.len() {
        if elevations[start] > SEA_LEVEL as f32 || labels[start] != 0 {
            continue;
        }
        next += 1;
        if next > u16::MAX as u32 {
            return Err(AtlasError::Corrupt(
                "ocean component count exceeds persisted identifier width".into(),
            ));
        }
        let label = next as u16;
        let mut queue = VecDeque::from([start]);
        labels[start] = label;
        while let Some(index) = queue.pop_front() {
            for neighbor in neighbors8_indices(index, side) {
                if labels[neighbor] == 0 && elevations[neighbor] <= SEA_LEVEL as f32 {
                    labels[neighbor] = label;
                    queue.push_back(neighbor);
                }
            }
        }
    }
    Ok((labels, next as u16))
}

fn basin_connection_sills(
    side: u16,
    elevations: &[f32],
    ocean_labels: &[u16],
    basin_count: u16,
) -> Vec<(u16, f32)> {
    let mut owner = ocean_labels.to_vec();
    let mut best = vec![f32::INFINITY; elevations.len()];
    let mut heap = BinaryHeap::new();
    for (index, label) in ocean_labels.iter().copied().enumerate() {
        if label != 0 {
            best[index] = elevations[index];
            heap.push(FloodEntry {
                elevation: elevations[index],
                index,
            });
        }
    }
    let mut connections = vec![(0u16, f32::INFINITY); usize::from(basin_count) + 1];
    while let Some(entry) = heap.pop() {
        if entry.elevation > best[entry.index] {
            continue;
        }
        for neighbor in neighbors8_indices(entry.index, side) {
            let candidate = entry.elevation.max(elevations[neighbor]);
            if owner[neighbor] == 0 || candidate < best[neighbor] {
                owner[neighbor] = owner[entry.index];
                best[neighbor] = candidate;
                heap.push(FloodEntry {
                    elevation: candidate,
                    index: neighbor,
                });
            } else if owner[neighbor] != owner[entry.index] {
                let a = owner[entry.index];
                let b = owner[neighbor];
                if a == 0 || b == 0 {
                    continue;
                }
                let sill = candidate.max(best[neighbor]);
                for (from, to) in [(a, b), (b, a)] {
                    let slot = &mut connections[usize::from(from)];
                    if sill < slot.1 || (sill == slot.1 && to < slot.0) {
                        *slot = (to, sill);
                    }
                }
            }
        }
    }
    connections
}

pub(super) fn ocean_records(
    seed: u32,
    side: u16,
    elevations: &[f32],
    geometry: &AtlasGrid<GeometryCell>,
    labels: &[u16],
    basin_count: u16,
) -> (Vec<OceanBasinRecord>, u16) {
    let mut members = vec![Vec::<usize>::new(); usize::from(basin_count) + 1];
    for (index, label) in labels.iter().copied().enumerate() {
        if label != 0 {
            members[usize::from(label)].push(index);
        }
    }
    let dominant = (1..=basin_count)
        .max_by_key(|id| members[usize::from(*id)].len())
        .unwrap_or(1);
    let connections = basin_connection_sills(side, elevations, labels, basin_count);
    let mut records = Vec::new();
    for id in 1..=basin_count {
        let indices = &members[usize::from(id)];
        let area = indices
            .iter()
            .map(|index| f64::from(geometry.values()[*index].physical_area))
            .sum();
        let curve = storage_curve(side, indices, elevations, SEA_LEVEL as f32);
        let volume = curve.last().map_or(0, |point| point.volume_units);
        let enclosed = id != dominant;
        let hash = mix64(u64::from(seed) ^ u64::from(id) ^ 0x6f63_6561_6e73);
        let name = if id == dominant {
            "The World Ocean".to_string()
        } else {
            format!("{} Sea", generated_word(hash))
        };
        let (connection_basin_id, connection_sill_elevation) = if id == dominant {
            (0, SEA_LEVEL as f32)
        } else {
            connections[usize::from(id)]
        };
        records.push(OceanBasinRecord {
            id,
            name,
            cell_count: indices.len().try_into().unwrap_or(u32::MAX),
            area,
            baseline_volume_units: volume,
            salinity: if enclosed { 232 } else { 220 },
            connection_basin_id,
            connection_sill_elevation,
            volume_elevation_curve: curve,
        });
    }
    (records, dominant)
}
