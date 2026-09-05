//! Build the tectonic cell layer from plate and craton fields.

use std::collections::{BTreeMap, VecDeque};
use glam::DVec3;
use crate::planet_atlas::{AtlasGrid, AtlasPos, GeometryCell, TectonicCell, cell_hash};
use super::{PlateRecord, DetailedBoundary, BedrockFamily, BasinKind};
use super::geometry::dvec;
use super::plates::classify_pair;

pub(super) fn build_tectonics(
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    plates: &[PlateRecord],
    plate_ids: &[u16],
    continental: &[u16],
    craton_ids: &[u16],
    crust_ages: &[u16],
) -> Vec<TectonicCell> {
    let count = geometry.len();
    let mut detail = vec![DetailedBoundary::Interior; count];
    let mut neighbor_plate = plate_ids.to_vec();
    let mut strength = vec![0.0f32; count];
    let mut strike = vec![DVec3::ZERO; count];

    for index in 0..count {
        let pos = AtlasPos::from_index(index, side).expect("geology index");
        let own = plate_ids[index];
        let point = dvec(geometry.values()[index].unit_direction);
        let mut best: Option<(f32, u16, DetailedBoundary, DVec3)> = None;
        for neighbor in pos.neighbors4(side) {
            let other_index = neighbor.index(side);
            let other = plate_ids[other_index];
            if other == own {
                continue;
            }
            let (class, motion, run_strike) = classify_pair(
                own,
                other,
                continental[index] >= 32_768,
                continental[other_index] >= 32_768,
                point,
                plates,
            );
            if best.is_none_or(|candidate| motion > candidate.0) {
                best = Some((motion, other, class, run_strike));
            }
        }
        if let Some((motion, other, class, run_strike)) = best {
            detail[index] = class;
            neighbor_plate[index] = other;
            strength[index] = motion;
            strike[index] = run_strike;
        }
    }

    // Two categorical majority passes remove one-cell class flicker without
    // blurring junctions into the plate interiors.
    for _ in 0..2 {
        let previous = detail.clone();
        for index in 0..count {
            if previous[index] == DetailedBoundary::Interior {
                continue;
            }
            let pos = AtlasPos::from_index(index, side).expect("geology index");
            let mut counts = BTreeMap::<DetailedBoundary, u8>::new();
            *counts.entry(previous[index]).or_default() += 2;
            for neighbor in pos.neighbors8(side) {
                let class = previous[neighbor.index(side)];
                if class != DetailedBoundary::Interior {
                    *counts.entry(class).or_default() += 1;
                }
            }
            if let Some((class, _)) = counts.into_iter().max_by_key(|(_, count)| *count) {
                detail[index] = class;
            }
        }
    }

    let mut nearest_source = vec![u32::MAX; count];
    let mut distance = vec![u16::MAX; count];
    let mut queue = VecDeque::new();
    for index in 0..count {
        if detail[index] != DetailedBoundary::Interior {
            nearest_source[index] = index as u32;
            distance[index] = 0;
            queue.push_back(index);
        }
    }
    while let Some(index) = queue.pop_front() {
        let pos = AtlasPos::from_index(index, side).expect("geology index");
        let next = distance[index].saturating_add(1);
        if next > 32 {
            continue;
        }
        for neighbor in pos.neighbors4(side) {
            let target = neighbor.index(side);
            if next < distance[target]
                || (next == distance[target] && nearest_source[index] < nearest_source[target])
            {
                distance[target] = next;
                nearest_source[target] = nearest_source[index];
                queue.push_back(target);
            }
        }
    }

    // Ocean crust grows older away from spreading ridges.
    let mut ridge_age = vec![u16::MAX; count];
    let mut ridge_queue = VecDeque::new();
    for index in 0..count {
        if detail[index] == DetailedBoundary::OceanRidge && continental[index] < 32_768 {
            ridge_age[index] = 0;
            ridge_queue.push_back(index);
        }
    }
    while let Some(index) = ridge_queue.pop_front() {
        let pos = AtlasPos::from_index(index, side).expect("geology index");
        let next = ridge_age[index].saturating_add(1);
        if next > 40 {
            continue;
        }
        for neighbor in pos.neighbors4(side) {
            let target = neighbor.index(side);
            if continental[target] < 32_768 && next < ridge_age[target] {
                ridge_age[target] = next;
                ridge_queue.push_back(target);
            }
        }
    }

    (0..count)
        .map(|index| {
            let source = usize::try_from(nearest_source[index])
                .ok()
                .filter(|source| *source < count)
                .unwrap_or(index);
            let run_strike = strike[source];
            let quantized = [
                (run_strike.x * 32_767.0).round() as i16,
                (run_strike.y * 32_767.0).round() as i16,
                (run_strike.z * 32_767.0).round() as i16,
            ];
            let oceanic_age = if continental[index] >= 32_768 {
                0
            } else if ridge_age[index] == u16::MAX {
                160 + (cell_hash(0, AtlasPos::from_index(index, side).unwrap(), 0x6f636561) % 61)
                    as u16
            } else {
                ridge_age[index].saturating_mul(6).min(220)
            };
            let cont = f32::from(continental[index]) / 65_535.0;
            TectonicCell {
                plate_id: plate_ids[index],
                boundary: detail[source].summary(),
                boundary_detail: detail[source],
                neighbor_plate: neighbor_plate[source],
                boundary_strength: strength[source],
                boundary_distance: distance[index],
                boundary_strike: quantized,
                continental_crust: continental[index],
                craton_id: craton_ids[index],
                crust_age: if cont >= 0.5 {
                    crust_ages[index]
                } else {
                    oceanic_age
                },
                oceanic_age,
                crust_thickness: (70.0 + cont * 330.0) as u16,
                bedrock_family: BedrockFamily::MixedBasement as u16,
                geological_province: 0,
                stratigraphic_stack: 0,
                metamorphic_grade: 0,
                fault_intensity: ((strength[source] * 38_000.0)
                    / (1.0 + f32::from(distance[index]) * 0.24))
                    .clamp(0.0, 65_535.0) as u16,
                volcanic_history: u8::from(detail[source].is_volcanic()),
                sediment_basin: BasinKind::None,
            }
        })
        .collect()
}
