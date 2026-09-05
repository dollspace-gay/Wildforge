//! Stable watershed labels and catchment records.

use std::collections::BTreeMap;
use crate::planet_atlas::{AtlasError, AtlasGrid, AtlasPos, GeometryCell, mix64};
use super::WatershedRecord;
use super::flow::topological_order;
use super::records::generated_word;

pub(super) struct WatershedInput<'a> {
    pub(super) side: u16,
    pub(super) receiver: &'a [u32],
    pub(super) ocean: &'a [u16],
    pub(super) lake: &'a [u32],
    pub(super) terminal_sink: &'a [bool],
    pub(super) geometry: &'a AtlasGrid<GeometryCell>,
    pub(super) runoff: &'a [f32],
    pub(super) seed: u32,
}

pub(super) fn assign_watersheds(
    input: WatershedInput<'_>,
) -> Result<(Vec<u32>, Vec<WatershedRecord>), AtlasError> {
    let WatershedInput {
        side,
        receiver,
        ocean,
        lake,
        terminal_sink,
        geometry,
        runoff,
        seed,
    } = input;
    let order = topological_order(receiver)?;
    let mut root = vec![u32::MAX; receiver.len()];
    for &index in order.iter().rev() {
        if ocean[index] != 0 || receiver[index] == u32::MAX {
            root[index] = index as u32;
        } else {
            root[index] = root[receiver[index] as usize];
        }
    }
    let mut ids = BTreeMap::<u32, u32>::new();
    let mut watershed = vec![0u32; receiver.len()];
    for index in 0..receiver.len() {
        if ocean[index] != 0 {
            continue;
        }
        let next = ids.len() as u32 + 1;
        let id = *ids.entry(root[index]).or_insert(next);
        watershed[index] = id;
    }
    let mut area = vec![0.0f64; ids.len() + 1];
    let mut weighted_runoff = vec![0.0f64; ids.len() + 1];
    for index in 0..receiver.len() {
        let id = watershed[index] as usize;
        if id == 0 {
            continue;
        }
        let cell_area = f64::from(geometry.values()[index].physical_area);
        area[id] += cell_area;
        weighted_runoff[id] += f64::from(runoff[index]) * cell_area;
    }
    let mut records = Vec::with_capacity(ids.len());
    for (outlet, id) in ids {
        let outlet_index = outlet as usize;
        let hash = mix64(u64::from(seed) ^ u64::from(id) ^ 0x7761_7465_7273_6864);
        records.push(WatershedRecord {
            id,
            name: format!("{} Watershed", generated_word(hash)),
            outlet: AtlasPos::from_index(outlet_index, side).expect("watershed outlet"),
            terminal: terminal_sink[outlet_index] || lake[outlet_index] != 0,
            area: area[id as usize],
            mean_runoff: if area[id as usize] > 0.0 {
                weighted_runoff[id as usize] / area[id as usize]
            } else {
                0.0
            },
        });
    }
    records.sort_by_key(|record| record.id);
    Ok((watershed, records))
}
