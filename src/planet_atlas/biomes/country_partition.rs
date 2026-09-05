//! Country border costs and ordered traversal across landmasses.

use crate::chunk::{SEA_LEVEL};
use crate::planet::{geodesic_distance};
use crate::planet_atlas::{AtlasGrid, AtlasPos, GroundCell, HYDRO_RIVER, HydrologyCell, TerrainCell};
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::cmp::Ordering as CmpOrdering;
#[derive(Clone, Copy, Debug)]
struct QueueEntry {
    cost: f32,
    index: usize,
    country: u16,
}

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.cost.to_bits() == other.cost.to_bits()
            && self.index == other.index
            && self.country == other.country
    }
}

impl Eq for QueueEntry {}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.index.cmp(&self.index))
            .then_with(|| other.country.cmp(&self.country))
    }
}

pub(super) fn crossing_cost(
    from: usize,
    to: usize,
    side: u16,
    terrain: &AtlasGrid<TerrainCell>,
    hydrology: &AtlasGrid<HydrologyCell>,
    ground: &AtlasGrid<GroundCell>,
) -> f32 {
    let a = terrain.values()[from];
    let b = terrain.values()[to];
    let ah = hydrology.values()[from];
    let bh = hydrology.values()[to];
    let physical = geodesic_distance(
        AtlasPos::from_index(from, side)
            .expect("from index")
            .center(side),
        AtlasPos::from_index(to, side)
            .expect("to index")
            .center(side),
    ) as f32;
    let watershed =
        if ah.watershed_id != 0 && bh.watershed_id != 0 && ah.watershed_id != bh.watershed_id {
            92.0
        } else {
            0.0
        };
    let crest = (a.eroded_elevation - b.eroded_elevation).abs() * 0.9
        + (f32::from(ground.values()[from].erosion_susceptibility)
            + f32::from(ground.values()[to].erosion_susceptibility))
            * 0.045;
    let river = if (ah.flags | bh.flags) & HYDRO_RIVER != 0
        && (ah.stream_order.max(bh.stream_order) >= 2
            || ah.mean_discharge.max(bh.mean_discharge) > 45.0)
    {
        36.0
    } else {
        0.0
    };
    physical + watershed + crest + river
}

pub(super) fn partition_countries(
    side: u16,
    seeds: &[usize],
    terrain: &AtlasGrid<TerrainCell>,
    hydrology: &AtlasGrid<HydrologyCell>,
    ground: &AtlasGrid<GroundCell>,
) -> (Vec<u16>, Vec<f32>) {
    let mut owner = vec![0u16; terrain.len()];
    let mut distance = vec![f32::INFINITY; terrain.len()];
    let mut queue = BinaryHeap::new();
    let mut watershed_nuclei = BTreeMap::<u32, BTreeSet<u16>>::new();
    for (offset, &index) in seeds.iter().enumerate() {
        let country = (offset + 1) as u16;
        let watershed = hydrology.values()[index].watershed_id;
        if watershed != 0 {
            watershed_nuclei
                .entry(watershed)
                .or_default()
                .insert(country);
        }
        owner[index] = country;
        distance[index] = 0.0;
        queue.push(QueueEntry {
            cost: 0.0,
            index,
            country,
        });
    }
    while let Some(entry) = queue.pop() {
        if entry.cost > distance[entry.index] + 0.001 || owner[entry.index] != entry.country {
            continue;
        }
        let pos = AtlasPos::from_index(entry.index, side).expect("atlas index");
        for neighbor in pos.neighbors4(side) {
            let next = neighbor.index(side);
            if terrain.values()[next].eroded_elevation <= SEA_LEVEL as f32
                || terrain.values()[next].landmass_id != terrain.values()[entry.index].landmass_id
            {
                continue;
            }
            let next_watershed = hydrology.values()[next].watershed_id;
            if next_watershed != 0
                && watershed_nuclei
                    .get(&next_watershed)
                    .is_some_and(|countries| !countries.contains(&entry.country))
            {
                // A drainage basin with its own country nucleus cannot be
                // swallowed from across its divide. Multiple nuclei inside
                // a very large basin may still partition it normally.
                continue;
            }
            let candidate =
                entry.cost + crossing_cost(entry.index, next, side, terrain, hydrology, ground);
            if candidate + 0.001 < distance[next]
                || ((candidate - distance[next]).abs() <= 0.001 && entry.country < owner[next])
            {
                distance[next] = candidate;
                owner[next] = entry.country;
                queue.push(QueueEntry {
                    cost: candidate,
                    index: next,
                    country: entry.country,
                });
            }
        }
    }
    // Flow routing may join a watershed diagonally at a cube vertex while
    // countries deliberately use four-neighbor travel. Such a one-cell
    // fragment has no path to its own protected nucleus. Fill only cells the
    // protected pass could not reach, preserving every established border
    // and exact land coverage.
    if owner.iter().copied().enumerate().any(|(index, country)| {
        country == 0 && terrain.values()[index].eroded_elevation > SEA_LEVEL as f32
    }) {
        let mut fallback = std::collections::VecDeque::new();
        for (index, country) in owner.iter().copied().enumerate() {
            if country != 0 {
                fallback.push_back(index);
            }
        }
        while let Some(index) = fallback.pop_front() {
            let pos = AtlasPos::from_index(index, side).expect("atlas fallback index");
            for neighbor in pos.neighbors4(side) {
                let next = neighbor.index(side);
                if owner[next] != 0
                    || terrain.values()[next].eroded_elevation <= SEA_LEVEL as f32
                    || terrain.values()[next].landmass_id != terrain.values()[index].landmass_id
                {
                    continue;
                }
                owner[next] = owner[index];
                distance[next] =
                    distance[index] + crossing_cost(index, next, side, terrain, hydrology, ground);
                fallback.push_back(next);
            }
        }
    }
    (owner, distance)
}
