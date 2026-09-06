//! Continental fetch and deterministic spherical circulation directions.

use super::solar::geographic_basis;
use crate::chunk::SEA_LEVEL;
use crate::planet::surface_to_unit;
use crate::planet_atlas::{AtlasGrid, AtlasPos, TerrainCell};
use glam::DVec3;
use std::collections::VecDeque;

pub(super) fn ocean_distances(side: u16, terrain: &AtlasGrid<TerrainCell>) -> Vec<u16> {
    let mut distance = vec![u16::MAX; terrain.len()];
    let mut queue = VecDeque::new();
    for (pos, cell) in terrain.iter() {
        if cell.eroded_elevation <= SEA_LEVEL as f32 {
            distance[pos.index(side)] = 0;
            queue.push_back(pos);
        }
    }
    while let Some(pos) = queue.pop_front() {
        let next = distance[pos.index(side)].saturating_add(1);
        for neighbor in pos.neighbors4(side) {
            let index = neighbor.index(side);
            if next < distance[index] {
                distance[index] = next;
                queue.push_back(neighbor);
            }
        }
    }
    distance
}

pub(super) fn direction_to_lower_distance(
    pos: AtlasPos,
    side: u16,
    distances: &[u16],
) -> Option<DVec3> {
    let mine = distances[pos.index(side)];
    pos.neighbors8(side)
        .into_iter()
        .filter(|neighbor| distances[neighbor.index(side)] < mine)
        .min_by_key(|neighbor| (distances[neighbor.index(side)], neighbor.index(side)))
        .map(|neighbor| {
            let here = surface_to_unit(pos.center(side));
            let there = surface_to_unit(neighbor.center(side));
            (there - here * there.dot(here)).normalize_or_zero()
        })
}

pub(super) fn circulation_wind(
    unit: DVec3,
    latitude: f64,
    declination: f64,
    continentality: f64,
    oceanward: Option<DVec3>,
) -> DVec3 {
    let (east, north) = geographic_basis(unit);
    let degrees = latitude.to_degrees();
    let abs = degrees.abs();
    let hemisphere = latitude.signum();
    // Do not flip the trade-wind meridional component in a single atlas row.
    // A smooth cross-equatorial transition makes the ITCZ a migrating belt,
    // not a one-cell moisture rail along mathematical latitude zero.
    let tropical_hemisphere = (latitude / 8f64.to_radians()).tanh();
    let (zonal, meridional, speed) = if abs < 30.0 {
        (-1.0, -tropical_hemisphere * 0.22, 1.0)
    } else if abs < 62.0 {
        (1.0, hemisphere * 0.10, 1.15)
    } else {
        (-1.0, -hemisphere * 0.08, 0.72)
    };
    let seasonal_belt = (declination * 1.6 - latitude).clamp(-0.7, 0.7);
    let mut wind = east * zonal + north * (meridional + seasonal_belt * 0.16);
    if abs < 38.0
        && continentality > 0.25
        && let Some(toward_ocean) = oceanward
    {
        // Hot-season flow is inland; cold-season flow is seaward.
        let heating = declination.sin() * hemisphere;
        wind += -toward_ocean * heating * continentality * 1.15;
    }
    (wind.normalize_or_zero() * speed).normalize_or_zero()
}

pub(super) fn downstream_neighbor(pos: AtlasPos, side: u16, wind: DVec3) -> AtlasPos {
    let here = surface_to_unit(pos.center(side));
    pos.neighbors8(side)
        .into_iter()
        .max_by(|a, b| {
            let score = |candidate: AtlasPos| {
                let unit = surface_to_unit(candidate.center(side));
                let tangent = (unit - here * unit.dot(here)).normalize_or_zero();
                wind.dot(tangent)
            };
            score(*a)
                .partial_cmp(&score(*b))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.index(side).cmp(&a.index(side)))
        })
        .unwrap_or(pos)
}
