//! Named river paths and records assembled in watershed order.

use super::flow::edge_distance;
use super::records::generated_word;
use super::{LakeRecord, OceanBasinRecord, RiverRecord};
use crate::planet::geodesic_distance;
use crate::planet_atlas::{AtlasPos, HydrologyCell, mix64};
use std::collections::BTreeMap;

pub(super) struct RiverInput<'a> {
    pub(super) seed: u32,
    pub(super) side: u16,
    pub(super) receiver: &'a [u32],
    pub(super) watershed: &'a [u32],
    pub(super) channel: &'a [bool],
    pub(super) cells: &'a mut [HydrologyCell],
    pub(super) model_lakes: &'a [LakeRecord],
    pub(super) model_oceans: &'a [OceanBasinRecord],
}

pub(super) fn river_records(input: RiverInput<'_>) -> Vec<RiverRecord> {
    let RiverInput {
        seed,
        side,
        receiver,
        watershed,
        channel,
        cells,
        model_lakes,
        model_oceans,
    } = input;
    let mut by_watershed = BTreeMap::<u32, Vec<usize>>::new();
    for (index, present) in channel.iter().copied().enumerate() {
        if present {
            by_watershed
                .entry(watershed[index])
                .or_default()
                .push(index);
        }
    }
    let mut records = Vec::new();
    for (watershed_id, members) in by_watershed {
        if watershed_id == 0 || members.len() < 2 {
            continue;
        }
        let member_set: std::collections::BTreeSet<_> = members.iter().copied().collect();
        let mouth = *members
            .iter()
            .filter(|index| {
                receiver[**index] == u32::MAX || !member_set.contains(&(receiver[**index] as usize))
            })
            .max_by(|a, b| {
                cells[**a]
                    .mean_discharge
                    .total_cmp(&cells[**b].mean_discharge)
                    .then_with(|| b.cmp(a))
            })
            .unwrap_or_else(|| {
                members
                    .iter()
                    .max_by(|a, b| {
                        cells[**a]
                            .mean_discharge
                            .total_cmp(&cells[**b].mean_discharge)
                    })
                    .expect("river has members")
            });
        let mut distance_to_mouth = BTreeMap::<usize, f64>::new();
        distance_to_mouth.insert(mouth, 0.0);
        let mut changed = true;
        while changed {
            changed = false;
            for &index in &members {
                let next = receiver[index];
                if next == u32::MAX {
                    continue;
                }
                let next = next as usize;
                if let Some(downstream) = distance_to_mouth.get(&next).copied() {
                    let candidate = downstream + edge_distance(side, index, next);
                    if distance_to_mouth
                        .get(&index)
                        .is_none_or(|old| candidate > *old)
                    {
                        distance_to_mouth.insert(index, candidate);
                        changed = true;
                    }
                }
            }
        }
        let source = members
            .iter()
            .copied()
            .max_by(|a, b| {
                distance_to_mouth
                    .get(a)
                    .copied()
                    .unwrap_or(0.0)
                    .total_cmp(&distance_to_mouth.get(b).copied().unwrap_or(0.0))
                    .then_with(|| b.cmp(a))
            })
            .unwrap_or(mouth);
        let mut path = Vec::new();
        let mut at = source;
        let mut seen = std::collections::BTreeSet::new();
        while seen.insert(at) {
            path.push(AtlasPos::from_index(at, side).expect("river path"));
            if at == mouth || receiver[at] == u32::MAX {
                break;
            }
            at = receiver[at] as usize;
        }
        let id = records.len() as u32 + 1;
        for &index in &members {
            cells[index].river_id = id;
        }
        let sink_index = receiver[mouth];
        let sink_name = if sink_index == u32::MAX {
            "an inland basin".to_string()
        } else {
            let sink = &cells[sink_index as usize];
            if sink.lake_basin_id != 0 {
                model_lakes
                    .iter()
                    .find(|lake| lake.id == sink.lake_basin_id)
                    .map_or_else(|| "a lake".to_string(), |lake| lake.name.clone())
            } else if sink.ocean_basin_id != 0 {
                model_oceans
                    .iter()
                    .find(|ocean| ocean.id == sink.ocean_basin_id)
                    .map_or_else(|| "the sea".to_string(), |ocean| ocean.name.clone())
            } else {
                "a downstream river".to_string()
            }
        };
        let hash = mix64(u64::from(seed) ^ u64::from(id) ^ 0x0000_7269_7665_7273);
        records.push(RiverRecord {
            id,
            name: format!("{} River", generated_word(hash)),
            watershed_id,
            source: AtlasPos::from_index(source, side).expect("river source"),
            mouth: AtlasPos::from_index(mouth, side).expect("river mouth"),
            sink_name,
            length_blocks: path
                .windows(2)
                .map(|edge| geodesic_distance(edge[0].center(side), edge[1].center(side)))
                .sum(),
            maximum_discharge: members
                .iter()
                .map(|index| cells[*index].mean_discharge)
                .fold(0.0, f32::max),
            maximum_width_blocks: members
                .iter()
                .map(|index| f32::from(cells[*index].channel_width_centiblocks) / 100.0)
                .fold(0.0, f32::max),
            stream_order: members
                .iter()
                .map(|index| cells[*index].stream_order)
                .max()
                .unwrap_or(1),
            path,
        });
    }
    records
}
