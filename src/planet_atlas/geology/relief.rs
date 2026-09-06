//! Tectonic and volcanic relief, smoothing, sea-level quantiles, and connected land.

use super::{DetailedBoundary, VolcanoRecord};
use crate::chunk::SEA_LEVEL;
use crate::planet::{FACE_BLOCKS, geodesic_distance};
use crate::planet_atlas::{AtlasGrid, AtlasPos, GeometryCell, TectonicCell, TerrainCell};
use std::cmp::Ordering as CmpOrdering;
use std::collections::{BTreeSet, VecDeque};

pub(super) fn boundary_relief(cell: TectonicCell, cell_blocks: u16) -> f32 {
    let distance = f32::from(cell.boundary_distance) * f32::from(cell_blocks);
    let gaussian = |center: f32, width: f32| (-((distance - center) / width).powi(2)).exp();
    let strength = cell.boundary_strength.clamp(0.18, 1.4);
    let continental = f32::from(cell.continental_crust) / 65_535.0;
    match cell.boundary_detail {
        DetailedBoundary::ContinentalCollision => 46.0 * strength * gaussian(70.0, 190.0),
        DetailedBoundary::OceanContinentSubduction if continental >= 0.5 => {
            34.0 * strength * gaussian(105.0, 110.0) - 2.0 * strength * gaussian(0.0, 65.0)
        }
        DetailedBoundary::OceanContinentSubduction => -24.0 * strength * gaussian(0.0, 58.0),
        DetailedBoundary::OceanOceanSubduction => {
            if cell.plate_id < cell.neighbor_plate {
                25.0 * strength * gaussian(90.0, 95.0)
            } else {
                -23.0 * strength * gaussian(0.0, 55.0)
            }
        }
        DetailedBoundary::ContinentalRift => {
            -17.0 * strength * gaussian(0.0, 75.0) + 8.0 * gaussian(125.0, 65.0)
        }
        DetailedBoundary::OceanRidge => 17.0 * strength * gaussian(0.0, 110.0),
        DetailedBoundary::Transform => -6.0 * strength * gaussian(0.0, 52.0),
        DetailedBoundary::PassiveWeak | DetailedBoundary::Interior => 0.0,
    }
}

pub(super) fn stamp_volcanic_relief(
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    volcanoes: &[VolcanoRecord],
) -> Vec<f32> {
    let mut relief = vec![0.0f32; geometry.len()];
    for volcano in volcanoes {
        let max_steps = (volcano.edifice_radius_blocks / (FACE_BLOCKS / side)).saturating_add(3);
        let mut queue = VecDeque::from([(volcano.pos, 0u16)]);
        let mut seen = BTreeSet::new();
        while let Some((pos, steps)) = queue.pop_front() {
            if !seen.insert(pos) || steps > max_steps {
                continue;
            }
            let distance = geodesic_distance(pos.center(side), volcano.pos.center(side)) as f32;
            let radius = f32::from(volcano.edifice_radius_blocks);
            if distance <= radius {
                let t = 1.0 - distance / radius.max(1.0);
                let erosion = 1.0 - f32::from(volcano.erosion) / 65_535.0;
                let mut height = f32::from(volcano.edifice_height_blocks) * t.powf(1.45) * erosion;
                let crater = f32::from(volcano.crater_radius_blocks);
                if distance < crater {
                    height -= f32::from(volcano.crater_depth_blocks) * (1.0 - distance / crater);
                }
                relief[pos.index(side)] += height;
            }
            for neighbor in pos.neighbors4(side) {
                queue.push_back((neighbor, steps.saturating_add(1)));
            }
        }
    }
    relief
}

pub(super) fn smooth_scalar(side: u16, values: &[f32], passes: usize) -> Vec<f32> {
    let mut current = values.to_vec();
    for _ in 0..passes {
        let previous = current.clone();
        for (index, target) in current.iter_mut().enumerate() {
            let pos = AtlasPos::from_index(index, side).unwrap();
            let sum: f32 = pos
                .neighbors4(side)
                .iter()
                .map(|neighbor| previous[neighbor.index(side)])
                .sum();
            *target = previous[index] * 0.58 + sum * 0.105;
        }
    }
    current
}

pub(super) fn weighted_sea_level(
    raw: &[f32],
    geometry: &AtlasGrid<GeometryCell>,
    target_ocean: f64,
) -> f32 {
    let mut order: Vec<usize> = (0..raw.len()).collect();
    order.sort_unstable_by(|a, b| raw[*a].partial_cmp(&raw[*b]).unwrap_or(CmpOrdering::Equal));
    let total: f64 = geometry
        .values()
        .iter()
        .map(|cell| f64::from(cell.physical_area))
        .sum();
    let wanted = total * target_ocean;
    let mut accumulated = 0.0;
    for index in order {
        accumulated += f64::from(geometry.values()[index].physical_area);
        if accumulated >= wanted {
            return raw[index];
        }
    }
    raw.last().copied().unwrap_or(0.0)
}

pub(super) fn label_components(
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    terrain: &[TerrainCell],
    land: bool,
) -> (Vec<u16>, Vec<(u16, u32, f64)>) {
    let mut labels = vec![0u16; terrain.len()];
    let mut records = Vec::new();
    let mut next = 0u16;
    for index in 0..terrain.len() {
        let included = (terrain[index].eroded_elevation > SEA_LEVEL as f32) == land;
        if !included || labels[index] != 0 {
            continue;
        }
        next = next.saturating_add(1);
        let mut queue = VecDeque::from([index]);
        let mut cells = 0u32;
        let mut area = 0.0f64;
        while let Some(at) = queue.pop_front() {
            if labels[at] != 0 || ((terrain[at].eroded_elevation > SEA_LEVEL as f32) != land) {
                continue;
            }
            labels[at] = next;
            cells += 1;
            area += f64::from(geometry.values()[at].physical_area);
            let pos = AtlasPos::from_index(at, side).unwrap();
            for neighbor in pos.neighbors4(side) {
                let neighbor = neighbor.index(side);
                if labels[neighbor] == 0 {
                    queue.push_back(neighbor);
                }
            }
        }
        records.push((next, cells, area));
    }
    (labels, records)
}
