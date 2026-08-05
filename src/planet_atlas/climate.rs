//! Causal spherical climate normals and shared astronomical geometry.
//!
//! The model is intentionally coarse. It resolves the first-order causes a
//! player can read in the landscape—latitude, seasons, oceans, circulation,
//! continental fetch and mountain lift—without pretending to be a fluid
//! dynamics solver.

use std::collections::{BTreeMap, VecDeque};

use glam::DVec3;
use noise::Perlin;

use super::*;
use crate::chunk::ChunkPos;

pub const CLIMATE_SEASONS: usize = 4;
pub const YEAR_DAYS: u32 = 144;
pub const AXIAL_TILT_DEGREES: f64 = 23.5;
pub const ROTATION_AXIS: [f64; 3] = [0.0, 1.0, 0.0];
pub const PRIME_MERIDIAN: [f64; 3] = [0.0, 0.0, 1.0];
/// Long advective loops on the 256×256 production faces need substantially
/// more relaxation than tiny fixtures. Keep the physical tolerance fixed and
/// allow the full planet to prove convergence rather than accepting a looser
/// answer merely because the grid is larger.
pub const CLIMATE_MAX_ITERATIONS: u16 = 256;
pub const CLIMATE_CONVERGENCE_TOLERANCE: f64 = 0.02;

const SEASON_MID_DAYS: [f64; CLIMATE_SEASONS] = [18.0, 54.0, 90.0, 126.0];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClimateSolveReport {
    pub iterations: [u16; CLIMATE_SEASONS],
    pub max_residual: f32,
    pub max_moisture_budget_error: f64,
}

#[inline]
pub fn rotation_axis() -> DVec3 {
    DVec3::from_array(ROTATION_AXIS)
}

#[inline]
pub fn prime_meridian() -> DVec3 {
    DVec3::from_array(PRIME_MERIDIAN)
}

/// Solar declination in radians. Day zero is the northern vernal equinox.
pub fn solar_declination(day: f64) -> f64 {
    AXIAL_TILT_DEGREES.to_radians() * (std::f64::consts::TAU * day / f64::from(YEAR_DAYS)).sin()
}

/// Planet-space direction from the planet toward the sun.
pub fn solar_direction(day: f64, time_of_day: f64) -> DVec3 {
    let axis = rotation_axis();
    let prime = prime_meridian();
    let east = axis.cross(prime).normalize();
    let declination = solar_declination(day);
    let hour = std::f64::consts::TAU * (time_of_day - 0.25);
    (axis * declination.sin() + (prime * hour.cos() + east * hour.sin()) * declination.cos())
        .normalize()
}

/// Astronomical daylight duration, including polar day and night.
pub fn day_length_hours(latitude_radians: f64, day: f64) -> f64 {
    let declination = solar_declination(day);
    let cos_hour = -latitude_radians.tan() * declination.tan();
    if cos_hour <= -1.0 {
        24.0
    } else if cos_hour >= 1.0 {
        0.0
    } else {
        24.0 * cos_hour.acos() / std::f64::consts::PI
    }
}

/// Daily-mean top-of-atmosphere solar factor, normalized to 0..=1.
pub fn daily_mean_insolation(latitude_radians: f64, day: f64) -> f64 {
    let declination = solar_declination(day);
    let sin_product = latitude_radians.sin() * declination.sin();
    let cos_product = latitude_radians.cos() * declination.cos();
    let cos_hour = if cos_product.abs() < 1.0e-12 {
        if sin_product > 0.0 { -2.0 } else { 2.0 }
    } else {
        -sin_product / cos_product
    };
    let hour = if cos_hour <= -1.0 {
        std::f64::consts::PI
    } else if cos_hour >= 1.0 {
        0.0
    } else {
        cos_hour.acos()
    };
    ((hour * sin_product + cos_product * hour.sin()) / std::f64::consts::PI).clamp(0.0, 1.0)
}

/// Local astronomical season: spring, summer, autumn, winter.
pub fn local_season(day: u32, latitude_radians: f64) -> usize {
    let northern = ((day / 36) % 4) as usize;
    if latitude_radians < -1.0e-6 {
        (northern + 2) % 4
    } else {
        northern
    }
}

/// Signed latitude and longitude about the manifest axes.
pub fn latitude_longitude(unit: DVec3) -> (f64, f64) {
    let axis = rotation_axis();
    let prime = prime_meridian();
    let east = axis.cross(prime).normalize();
    let latitude = unit.dot(axis).clamp(-1.0, 1.0).asin();
    let planar = (unit - axis * unit.dot(axis)).normalize_or_zero();
    let longitude = planar.dot(east).atan2(planar.dot(prime));
    (latitude, longitude)
}

fn geographic_basis(unit: DVec3) -> (DVec3, DVec3) {
    let axis = rotation_axis();
    let mut east = axis.cross(unit);
    if east.length_squared() < 1.0e-12 {
        east = prime_meridian().cross(unit);
    }
    east = east.normalize();
    let north = unit.cross(east).normalize();
    (east, north)
}

fn chart_components(pos: AtlasPos, side: u16, vector: DVec3) -> [f32; 2] {
    let frame = crate::planet::local_frame(pos.center(side));
    [
        vector.dot(frame.east) as f32,
        vector.dot(frame.north) as f32,
    ]
}

pub(crate) fn chart_vector(pos: AtlasPos, side: u16, components: [f32; 2]) -> DVec3 {
    let frame = crate::planet::local_frame(pos.center(side));
    (frame.east * f64::from(components[0]) + frame.north * f64::from(components[1]))
        .normalize_or_zero()
}

fn ocean_distances(side: u16, terrain: &AtlasGrid<TerrainCell>) -> Vec<u16> {
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

fn direction_to_lower_distance(pos: AtlasPos, side: u16, distances: &[u16]) -> Option<DVec3> {
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

fn circulation_wind(
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

fn downstream_neighbor(pos: AtlasPos, side: u16, wind: DVec3) -> AtlasPos {
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

#[derive(Clone, Copy, Debug, Default)]
struct TransportTarget {
    index: u32,
    weight: f32,
}

#[derive(Clone, Copy, Debug, Default)]
struct TransportStencil {
    targets: [TransportTarget; 4],
    len: u8,
}

/// Conservative semi-Lagrangian stencil over the forward half-plane. A
/// single winning neighbor turns convergent flow into pixel-width moisture
/// rails; sharing flux across the 3–4 forward neighbors gives fronts and rain
/// shadows an atmospheric width while preserving every unit.
fn transport_stencil(pos: AtlasPos, side: u16, wind: DVec3) -> TransportStencil {
    let here = surface_to_unit(pos.center(side));
    let mut candidates: Vec<(f64, usize)> = pos
        .neighbors8(side)
        .into_iter()
        .filter_map(|candidate| {
            let unit = surface_to_unit(candidate.center(side));
            let tangent = (unit - here * unit.dot(here)).normalize_or_zero();
            let score = wind.dot(tangent);
            (score > 1.0e-6).then_some((score * score, candidate.index(side)))
        })
        .collect();
    candidates.sort_by(|(a_score, a_index), (b_score, b_index)| {
        b_score
            .total_cmp(a_score)
            .then_with(|| a_index.cmp(b_index))
    });
    candidates.truncate(4);
    if candidates.is_empty() {
        return TransportStencil {
            targets: [TransportTarget {
                index: pos.index(side) as u32,
                weight: 1.0,
            }; 4],
            len: 1,
        };
    }
    let total = candidates.iter().map(|(score, _)| *score).sum::<f64>();
    let mut stencil = TransportStencil {
        len: candidates.len() as u8,
        ..TransportStencil::default()
    };
    for (slot, (score, index)) in candidates.into_iter().enumerate() {
        stencil.targets[slot] = TransportTarget {
            index: index as u32,
            weight: (score / total) as f32,
        };
    }
    stencil
}

fn distribute_u32(amount: u32, stencil: TransportStencil, mut add: impl FnMut(usize, u32)) {
    let len = usize::from(stencil.len);
    let mut remaining = amount;
    for (slot, target) in stencil.targets[..len].iter().enumerate() {
        let share = if slot + 1 == len {
            remaining
        } else {
            ((amount as f64 * f64::from(target.weight)).floor() as u32).min(remaining)
        };
        remaining -= share;
        add(target.index as usize, share);
    }
}

fn spill_vapor(cells: &mut [DynamicCell], start: usize, mut amount: u32) -> Result<(), AtlasError> {
    for offset in 0..cells.len() {
        let index = (start + offset) % cells.len();
        let room = u32::MAX - cells[index].atmospheric_vapor;
        let accepted = amount.min(room);
        cells[index].atmospheric_vapor += accepted;
        amount -= accepted;
        if amount == 0 {
            return Ok(());
        }
    }
    Err(AtlasError::Corrupt(
        "whole-planet vapor storage capacity exhausted".into(),
    ))
}

fn spill_cloud(cells: &mut [DynamicCell], start: usize, mut amount: u32) -> Result<(), AtlasError> {
    for offset in 0..cells.len() {
        let index = (start + offset) % cells.len();
        let room = u32::MAX - cells[index].cloud_water;
        let accepted = amount.min(room);
        cells[index].cloud_water += accepted;
        amount -= accepted;
        if amount == 0 {
            return Ok(());
        }
    }
    Err(AtlasError::Corrupt(
        "whole-planet cloud storage capacity exhausted".into(),
    ))
}

#[derive(Clone, Copy)]
struct SeasonalMoistureInputs<'a> {
    side: u16,
    geometry: &'a AtlasGrid<GeometryCell>,
    terrain: &'a AtlasGrid<TerrainCell>,
    winds: &'a [[[f32; 2]; CLIMATE_SEASONS]],
    continentality: &'a [f32],
    cancel: &'a CancellationToken,
}

struct SeasonalMoistureSolution {
    moisture: Vec<f32>,
    precipitation: Vec<f32>,
    iterations: u16,
    residual: f64,
    budget_error: f64,
}

fn solve_seasonal_moisture(
    season: usize,
    temperatures: &[f32],
    inputs: &SeasonalMoistureInputs<'_>,
) -> Result<SeasonalMoistureSolution, AtlasError> {
    let SeasonalMoistureInputs {
        side,
        geometry,
        terrain,
        winds,
        continentality,
        cancel,
    } = *inputs;
    let count = terrain.len();
    let mut transport = Vec::with_capacity(count);
    let mut convergence = vec![0.0f64; count];
    for (index, seasonal_winds) in winds.iter().enumerate().take(count) {
        let pos = AtlasPos::from_index(index, side).expect("atlas index");
        let wind = chart_vector(pos, side, seasonal_winds[season]);
        let stencil = transport_stencil(pos, side, wind);
        for target in &stencil.targets[..usize::from(stencil.len)] {
            convergence[target.index as usize] += f64::from(target.weight);
        }
        transport.push(stencil);
    }

    let mut source = vec![0.0f64; count];
    for index in 0..count {
        let terrain_cell = terrain.values()[index];
        let latitude = f64::from(geometry.values()[index].latitude_radians);
        let temp = f64::from(temperatures[index]);
        let ocean = terrain_cell.eroded_elevation <= SEA_LEVEL as f32;
        let equatorial = (-((latitude.to_degrees().abs() / 13.0).powi(2))).exp();
        let subtropical = (-(((latitude.to_degrees().abs() - 27.0) / 8.0).powi(2))).exp();
        let thermal = ((temp + 18.0) / 42.0).clamp(0.04, 1.25);
        source[index] = if ocean {
            (8.0 + 18.0 * thermal) * (1.0 + 0.28 * equatorial) * (1.0 - 0.18 * subtropical)
        } else {
            let provisional_cover = (1.0 - f64::from(continentality[index]) * 0.65)
                * ((temp + 12.0) / 35.0).clamp(0.0, 1.0);
            0.20 + provisional_cover * 2.8
        };
    }

    let mut moisture = source.clone();
    let mut next = vec![0.0f64; count];
    let mut precipitation = vec![0.0f64; count];
    let mut last_budget_error = 0.0f64;
    let mut last_residual = f64::INFINITY;
    let mut completed = CLIMATE_MAX_ITERATIONS;
    for iteration in 1..=CLIMATE_MAX_ITERATIONS {
        if iteration % 8 == 0 && cancel.is_cancelled() {
            return Err(AtlasError::Cancelled);
        }
        next.copy_from_slice(&source);
        precipitation.fill(0.0);
        let before = moisture.iter().sum::<f64>() + source.iter().sum::<f64>();
        for index in 0..count {
            let from_elevation = terrain.values()[index].eroded_elevation;
            let lift = transport[index].targets[..usize::from(transport[index].len)]
                .iter()
                .map(|target| {
                    f64::from(
                        (terrain.values()[target.index as usize].eroded_elevation - from_elevation)
                            .max(0.0)
                            * target.weight,
                    )
                })
                .sum::<f64>();
            let latitude = f64::from(geometry.values()[index].latitude_radians);
            let degrees = latitude.to_degrees().abs();
            let equatorial = (-(degrees / 12.0).powi(2)).exp();
            let subtropical = (-((degrees - 27.0) / 7.5).powi(2)).exp();
            let storm_track = (-((degrees - 50.0) / 12.0).powi(2)).exp();
            let cold = ((5.0 - f64::from(temperatures[index])) / 32.0).clamp(0.0, 0.55);
            let convergence_lift = (convergence[index] - 1.0).max(0.0) * 0.018;
            let condensation = (0.075
                + lift * 0.0065
                + equatorial * 0.16
                + storm_track * 0.08
                + convergence_lift
                + cold * 0.08
                - subtropical * 0.052)
                .clamp(0.025, 0.82);
            let rain = moisture[index] * condensation;
            precipitation[index] += rain;
            let residual = moisture[index] - rain;
            let retained = residual * 0.12;
            next[index] += retained;
            let targets = &transport[index].targets[..usize::from(transport[index].len)];
            let moving = residual - retained;
            let mut remaining = moving;
            for (slot, target) in targets.iter().enumerate() {
                let share = if slot + 1 == targets.len() {
                    remaining
                } else {
                    (moving * f64::from(target.weight)).min(remaining)
                };
                remaining -= share;
                next[target.index as usize] += share;
            }
        }
        let after = next.iter().sum::<f64>() + precipitation.iter().sum::<f64>();
        last_budget_error = (before - after).abs();
        last_residual = next
            .iter()
            .zip(&moisture)
            .map(|(a, b)| (a - b).abs() / (1.0 + b.abs()))
            .fold(0.0f64, f64::max);
        std::mem::swap(&mut moisture, &mut next);
        if iteration >= 24 && last_residual <= CLIMATE_CONVERGENCE_TOLERANCE {
            completed = iteration;
            break;
        }
    }
    if last_residual > CLIMATE_CONVERGENCE_TOLERANCE {
        return Err(AtlasError::Incomplete(format!(
            "seasonal climate moisture solver did not converge: season {season}, residual {last_residual:.6} after {CLIMATE_MAX_ITERATIONS} iterations"
        )));
    }

    // Re-evaluate the equilibrium precipitation represented by the accepted
    // moisture field. Scaling converts the model's hourly flux into a stable,
    // legible annual millimetre range without changing spatial causality.
    for index in 0..count {
        let from_elevation = terrain.values()[index].eroded_elevation;
        let lift = transport[index].targets[..usize::from(transport[index].len)]
            .iter()
            .map(|target| {
                f64::from(
                    (terrain.values()[target.index as usize].eroded_elevation - from_elevation)
                        .max(0.0)
                        * target.weight,
                )
            })
            .sum::<f64>();
        let degrees = f64::from(geometry.values()[index].latitude_radians)
            .to_degrees()
            .abs();
        let equatorial = (-(degrees / 12.0).powi(2)).exp();
        let subtropical = (-((degrees - 27.0) / 7.5).powi(2)).exp();
        let storm_track = (-((degrees - 50.0) / 12.0).powi(2)).exp();
        let cold = ((5.0 - f64::from(temperatures[index])) / 32.0).clamp(0.0, 0.55);
        let condensation = (0.075
            + lift * 0.0065
            + equatorial * 0.16
            + storm_track * 0.08
            + (convergence[index] - 1.0).max(0.0) * 0.018
            + cold * 0.08
            - subtropical * 0.052)
            .clamp(0.025, 0.82);
        precipitation[index] = moisture[index] * condensation;
    }
    Ok(SeasonalMoistureSolution {
        moisture: moisture.into_iter().map(|value| value as f32).collect(),
        precipitation: precipitation
            .into_iter()
            .map(|value| {
                // Convert equilibrium flux into millimetres for one season.
                // Extreme convergence grows almost linearly without a
                // convective-depth ceiling; compress that tail smoothly so
                // four wet seasons asymptote below 12 m/year while ordinary
                // climates retain their ordering and rain-shadow contrast.
                let linear_mm = value * 24.0;
                (3_000.0 * (1.0 - (-linear_mm / 3_000.0).exp())).max(0.75) as f32
            })
            .collect(),
        iterations: completed,
        residual: last_residual,
        budget_error: last_budget_error,
    })
}

pub(crate) fn generate_climate(
    seed: u32,
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    terrain: &AtlasGrid<TerrainCell>,
    cancel: &CancellationToken,
) -> Result<(AtlasGrid<ClimateCell>, ClimateSolveReport), AtlasError> {
    let count = terrain.len();
    let ocean_distance = ocean_distances(side, terrain);
    let continental_scale = (side / 12).max(3);
    let continentality: Vec<f32> = terrain
        .values()
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            if cell.eroded_elevation <= SEA_LEVEL as f32 {
                0.0
            } else {
                (f32::from(ocean_distance[index]) / f32::from(continental_scale)).clamp(0.0, 1.0)
            }
        })
        .collect();

    let variation = Perlin::new(seed ^ 0x434c_494d);
    let mut ocean_current = vec![[0.0f32; 2]; count];
    let mut ocean_anomaly = vec![0.0f32; count];
    let mut seasonal_wind = vec![[[0.0f32; 2]; CLIMATE_SEASONS]; count];
    for index in 0..count {
        let pos = AtlasPos::from_index(index, side).expect("atlas index");
        let unit = DVec3::from_array(geometry.values()[index].unit_direction.map(f64::from));
        let latitude = f64::from(geometry.values()[index].latitude_radians);
        let oceanward = direction_to_lower_distance(pos, side, &ocean_distance);
        for season in 0..CLIMATE_SEASONS {
            let declination = solar_declination(SEASON_MID_DAYS[season]);
            let wind = circulation_wind(
                unit,
                latitude,
                declination,
                f64::from(continentality[index]),
                oceanward,
            );
            seasonal_wind[index][season] = chart_components(pos, side, wind);
        }
        if terrain.values()[index].eroded_elevation <= SEA_LEVEL as f32 {
            let (east, north) = geographic_basis(unit);
            let annual = (0..CLIMATE_SEASONS)
                .map(|season| chart_vector(pos, side, seasonal_wind[index][season]))
                .fold(DVec3::ZERO, |sum, wind| sum + wind)
                .normalize_or_zero();
            let coriolis = if latitude >= 0.0 { -1.0 } else { 1.0 };
            let current =
                (annual * 0.82 + unit.cross(annual) * coriolis * 0.36).normalize_or_zero();
            ocean_current[index] = chart_components(pos, side, current);
            let poleward = current.dot(north) * latitude.signum();
            let zonal = current.dot(east);
            ocean_anomaly[index] = (poleward * 4.8 + zonal * latitude.sin() * 1.2) as f32;
        }
    }

    // Carry the coastal current signal a few cells inland, decaying with
    // distance. This is a heat anomaly, not moving voxel ocean water.
    for _ in 0..4 {
        let previous = ocean_anomaly.clone();
        for (index, anomaly) in ocean_anomaly.iter_mut().enumerate() {
            if terrain.values()[index].eroded_elevation <= SEA_LEVEL as f32 {
                continue;
            }
            let pos = AtlasPos::from_index(index, side).expect("atlas index");
            let (sum, samples) = pos
                .neighbors8(side)
                .into_iter()
                .fold((0.0f32, 0u8), |(sum, n), neighbor| {
                    (sum + previous[neighbor.index(side)], n + 1)
                });
            *anomaly = sum / f32::from(samples) * 0.62;
        }
    }

    let mut seasonal_temperature = vec![[0.0f32; CLIMATE_SEASONS]; count];
    for index in 0..count {
        let pos = AtlasPos::from_index(index, side).expect("atlas index");
        let latitude = f64::from(geometry.values()[index].latitude_radians);
        let abs_sine = latitude.sin().abs();
        let ocean = terrain.values()[index].eroded_elevation <= SEA_LEVEL as f32;
        // A spherical planet has much more area in the middle latitudes than
        // at its endpoints. A shallow exponent made those broad belts nearly
        // polar and drove the area-mean surface below 8 C; this curve keeps
        // genuinely cold poles while producing an Earthlike temperate mean.
        let base = 29.5 - 48.5 * abs_sine.powf(2.2);
        let elevation =
            f64::from((terrain.values()[index].eroded_elevation - SEA_LEVEL as f32).max(0.0));
        let lapse = elevation * 0.009;
        let noise = f64::from(unit_noise(
            &variation,
            pos.center(side),
            5.5,
            [3.2, -7.1, 11.4],
        )) * 1.4;
        let maritime = if ocean {
            0.0
        } else {
            f64::from(continentality[index])
        };
        let amplitude =
            (3.2 + 18.0 * abs_sine * maritime + 4.0 * abs_sine) * if ocean { 0.58 } else { 1.0 };
        for season in 0..CLIMATE_SEASONS {
            let phase = solar_declination(SEASON_MID_DAYS[season]).sin()
                / AXIAL_TILT_DEGREES.to_radians().sin();
            let seasonal = phase * latitude.signum() * amplitude;
            let insolation = daily_mean_insolation(latitude, SEASON_MID_DAYS[season]);
            let polar_day_correction = (insolation - 0.22) * 5.0 * abs_sine;
            seasonal_temperature[index][season] = (base + seasonal + polar_day_correction - lapse
                + f64::from(ocean_anomaly[index])
                + noise) as f32;
        }
    }

    let mut seasonal_moisture = vec![[0.0f32; CLIMATE_SEASONS]; count];
    let mut seasonal_precipitation = vec![[0.0f32; CLIMATE_SEASONS]; count];
    let mut report = ClimateSolveReport::default();
    let moisture_inputs = SeasonalMoistureInputs {
        side,
        geometry,
        terrain,
        winds: &seasonal_wind,
        continentality: &continentality,
        cancel,
    };
    for season in 0..CLIMATE_SEASONS {
        let temperatures: Vec<f32> = seasonal_temperature
            .iter()
            .map(|values| values[season])
            .collect();
        let solution = solve_seasonal_moisture(season, &temperatures, &moisture_inputs)?;
        report.iterations[season] = solution.iterations;
        report.max_residual = report.max_residual.max(solution.residual as f32);
        report.max_moisture_budget_error =
            report.max_moisture_budget_error.max(solution.budget_error);
        for index in 0..count {
            seasonal_moisture[index][season] = solution.moisture[index];
            seasonal_precipitation[index][season] = solution.precipitation[index];
        }
    }

    let mut values = Vec::with_capacity(count);
    for index in 0..count {
        let mean_temperature = seasonal_temperature[index].iter().sum::<f32>() / 4.0;
        let low_temperature = seasonal_temperature[index]
            .iter()
            .copied()
            .fold(f32::INFINITY, f32::min);
        let high_temperature = seasonal_temperature[index]
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        let mean_precipitation = seasonal_precipitation[index].iter().sum::<f32>();
        let mean_atmospheric_moisture = seasonal_moisture[index].iter().sum::<f32>() / 4.0;
        let max_precip = seasonal_precipitation[index]
            .iter()
            .copied()
            .fold(0.0f32, f32::max);
        let min_precip = seasonal_precipitation[index]
            .iter()
            .copied()
            .fold(f32::INFINITY, f32::min);
        let wind_speed = seasonal_wind[index]
            .iter()
            .map(|wind| wind[0].hypot(wind[1]))
            .sum::<f32>()
            / 4.0;
        let potential_evapotranspiration =
            ((mean_temperature + 10.0).max(0.0) * 25.0 + wind_speed * 90.0).max(25.0);
        let aridity = (potential_evapotranspiration / mean_precipitation.max(1.0)).clamp(0.0, 8.0);
        let snow_persistence = seasonal_temperature[index]
            .iter()
            .zip(seasonal_precipitation[index])
            .map(|(temperature, precip)| {
                if *temperature < 0.0 {
                    (precip / mean_precipitation.max(1.0)).min(1.0)
                } else {
                    0.0
                }
            })
            .sum::<f32>()
            .clamp(0.0, 1.0);
        let prevailing_wind = [
            seasonal_wind[index].iter().map(|wind| wind[0]).sum::<f32>() / 4.0,
            seasonal_wind[index].iter().map(|wind| wind[1]).sum::<f32>() / 4.0,
        ];
        values.push(ClimateCell {
            mean_temperature,
            seasonality: high_temperature - low_temperature,
            ocean_temperature_anomaly: ocean_anomaly[index],
            continentality: continentality[index],
            prevailing_wind,
            ocean_current: ocean_current[index],
            mean_atmospheric_moisture,
            mean_precipitation,
            precipitation_seasonality: (max_precip - min_precip) / mean_precipitation.max(1.0),
            potential_evapotranspiration,
            aridity,
            snow_persistence,
            seasonal_temperature: seasonal_temperature[index],
            seasonal_precipitation: seasonal_precipitation[index],
            seasonal_wind: seasonal_wind[index],
        });
    }
    Ok((AtlasGrid::from_values(side, values)?, report))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[repr(u8)]
pub enum LocalWeather {
    Clear = 0,
    Overcast = 1,
    Precipitation = 2,
    Storm = 3,
}

impl LocalWeather {
    pub const fn precipitating(self) -> bool {
        matches!(self, Self::Precipitation | Self::Storm)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Overcast => "overcast",
            Self::Precipitation => "precipitation",
            Self::Storm => "storm",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[repr(u8)]
pub enum PrecipitationForm {
    None = 0,
    Rain = 1,
    Snow = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct LocalWeatherSample {
    pub kind: LocalWeather,
    pub precipitation: PrecipitationForm,
    pub temperature_c: f32,
    pub pressure_anomaly: i16,
    pub vapor: u32,
    pub cloud_water: u32,
    pub storm_energy: u16,
    pub precipitation_units: u16,
    pub wind: [f32; 2],
}

impl Default for LocalWeatherSample {
    fn default() -> Self {
        Self {
            kind: LocalWeather::Clear,
            precipitation: PrecipitationForm::None,
            temperature_c: 15.0,
            pressure_anomaly: 0,
            vapor: 0,
            cloud_water: 0,
            storm_energy: 0,
            precipitation_units: 0,
            wind: [0.0; 2],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WeatherStepReport {
    pub climate_hour: u64,
    pub processed_cells: usize,
    pub evaporation_units: u64,
    pub condensation_units: u64,
    pub precipitation_units: u64,
    /// Water removed from the coarse snow/runoff stores through the explicit
    /// voxel water-cycle handoff since the previous completed weather step.
    pub water_cycle_outflow_units: u64,
    pub atmospheric_water_before: i128,
    pub atmospheric_water_after: i128,
    pub unexplained_water_drift: i128,
}

/// Exact coarse runoff movement accepted by one weather hour. Environmental
/// solutes use the same source fraction and seam-aware receiver; evaporation
/// is absent because nonvolatile dross remains behind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct RunoffTransport {
    pub from: AtlasPos,
    pub to: AtlasPos,
    pub water_hu: u64,
    pub source_water_before_hu: u64,
}

#[derive(Clone, Debug)]
struct WeatherCheckpoint {
    aquifers: Vec<SparseAquiferState>,
    reservoirs: Vec<SurfaceReservoirState>,
    springs: Vec<SpringState>,
    inboxes: Vec<FluxInbox>,
    ledger: WaterLedger,
    completed_surface_hours: u64,
    completed_groundwater_days: u64,
}

/// Mutable whole-planet weather state. A climate-hour pass is built into a
/// second grid and sliced across ordinary server ticks; the accepted state is
/// swapped atomically only after every cell has contributed.
#[derive(Clone, Debug)]
pub struct PlanetaryWeather {
    pub cells: DynamicLayers,
    pub water: WaterCycleState,
    scratch: Vec<DynamicCell>,
    water_scratch: Vec<WaterCell>,
    water_inbound: Vec<ReservoirMass>,
    surface_fluxes: BTreeMap<u64, (ReservoirMass, ReservoirMass)>,
    cursor: usize,
    active_hour: Option<u64>,
    start_total: i128,
    evaporation_units: u64,
    condensation_units: u64,
    precipitation_units: u64,
    pending_water_cycle_outflow: u64,
    active_water_cycle_outflow: u64,
    pub completed_hours: u64,
    pub last_report: WeatherStepReport,
    active_runoff_routes: Vec<RunoffTransport>,
    last_runoff_routes: Vec<RunoffTransport>,
    checkpoint: Option<WeatherCheckpoint>,
    cells_swapped: bool,
    failed_hour: Option<u64>,
}

impl PlanetaryWeather {
    pub fn new(cells: DynamicLayers, water: WaterCycleState) -> Self {
        let scratch = vec![DynamicCell::default(); cells.cells.len()];
        let water_scratch = water.cells.values().to_vec();
        let water_inbound = vec![ReservoirMass::default(); cells.cells.len()];
        let completed_hours = cells.completed_climate_hours;
        Self {
            cells,
            water,
            scratch,
            water_scratch,
            water_inbound,
            surface_fluxes: BTreeMap::new(),
            cursor: 0,
            active_hour: None,
            start_total: 0,
            evaporation_units: 0,
            condensation_units: 0,
            precipitation_units: 0,
            pending_water_cycle_outflow: 0,
            active_water_cycle_outflow: 0,
            completed_hours,
            last_report: WeatherStepReport::default(),
            active_runoff_routes: Vec::new(),
            last_runoff_routes: Vec::new(),
            checkpoint: None,
            cells_swapped: false,
            failed_hour: None,
        }
    }

    pub fn water_audit(&self) -> crate::planet_atlas::WaterAudit {
        let atmospheric = self.cells.cells.values().iter().fold(0u64, |sum, cell| {
            sum.saturating_add(u64::from(cell.atmospheric_vapor))
        });
        self.water
            .audit(crate::planet_atlas::ReservoirMass::fresh(atmospheric))
    }

    /// Soil water visible to an ecology update even while a sliced climate
    /// hour is in flight. Cells already visited by the weather pass live in
    /// scratch; later cells still live in the committed arrays.
    pub fn ecology_soil_water_hu(&self, pos: AtlasPos) -> u64 {
        let index = pos.index(self.water.cells.side());
        if self.active_hour.is_some() && index < self.cursor {
            self.water_scratch[index].soil.water_hu
        } else {
            self.water.cells.values()[index].soil.water_hu
        }
    }

    /// Move real fresh soil water into atmospheric vapor for magical plant
    /// growth. This is transpiration, not deletion. The same in-flight rule
    /// as `ecology_soil_water_hu` prevents a later sliced-weather commit from
    /// overwriting the ecological withdrawal.
    pub fn transpire_ecology(&mut self, pos: AtlasPos, requested_hu: u64) -> u64 {
        let index = pos.index(self.water.cells.side());
        let in_scratch = self.active_hour.is_some() && index < self.cursor;
        let (water, atmosphere) = if in_scratch {
            (&mut self.water_scratch[index], &mut self.scratch[index])
        } else {
            (
                &mut self.water.cells.values_mut()[index],
                &mut self.cells.cells.values_mut()[index],
            )
        };
        let room = u64::from(u32::MAX - atmosphere.atmospheric_vapor);
        let moved = water.soil.take_fresh_water(requested_hu.min(room));
        atmosphere.atmospheric_vapor += moved.water_hu as u32;
        self.evaporation_units = self.evaporation_units.saturating_add(moved.water_hu);
        moved.water_hu
    }

    /// Move liquid embodied in a harvested ecological product (currently
    /// rainbell dew) from real soil water into the detailed industrial/
    /// circulating reservoir. It remains inside the finite water audit until
    /// a later use returns it to soil, vapor, or another declared reservoir.
    pub fn harvest_ecology_water(&mut self, pos: AtlasPos, requested_hu: u64) -> u64 {
        let index = pos.index(self.water.cells.side());
        let in_scratch = self.active_hour.is_some() && index < self.cursor;
        let water = if in_scratch {
            &mut self.water_scratch[index]
        } else {
            &mut self.water.cells.values_mut()[index]
        };
        let moved = water.soil.take_fresh_water(requested_hu);
        if self.water.ledger.industrial.add_assign(moved).is_err() {
            water
                .soil
                .add_assign(moved)
                .expect("rolling back ecological water harvest fits");
            return 0;
        }
        moved.water_hu
    }

    /// Return water embodied in a planted ecological item to local soil.
    /// The item identity is handled by the arcane ledger; this moves only the
    /// physical fresh-water parcel previously held in circulating custody.
    pub fn return_ecology_water_to_soil(&mut self, pos: AtlasPos, requested_hu: u64) -> u64 {
        let moved = self.water.ledger.industrial.take_fresh_water(requested_hu);
        if moved.water_hu == 0 {
            return 0;
        }
        let index = pos.index(self.water.cells.side());
        let in_scratch = self.active_hour.is_some() && index < self.cursor;
        let water = if in_scratch {
            &mut self.water_scratch[index]
        } else {
            &mut self.water.cells.values_mut()[index]
        };
        if water.soil.add_assign(moved).is_err() {
            self.water
                .ledger
                .industrial
                .add_assign(moved)
                .expect("rolling back planted ecological water fits");
            return 0;
        }
        moved.water_hu
    }

    pub fn is_updating(&self) -> bool {
        self.active_hour.is_some()
    }

    pub fn last_runoff_routes(&self) -> &[RunoffTransport] {
        &self.last_runoff_routes
    }

    pub fn begin_hour(&mut self, climate_hour: u64) {
        if self.active_hour.is_some() || self.failed_hour.is_some() {
            return;
        }
        self.checkpoint = Some(WeatherCheckpoint {
            aquifers: self.water.aquifers.clone(),
            reservoirs: self.water.reservoirs.clone(),
            springs: self.water.springs.clone(),
            inboxes: self.water.inboxes.clone(),
            ledger: self.water.ledger,
            completed_surface_hours: self.water.completed_surface_hours,
            completed_groundwater_days: self.water.completed_groundwater_days,
        });
        self.cells_swapped = false;
        self.scratch.fill(DynamicCell::default());
        self.water_scratch
            .clone_from_slice(self.water.cells.values());
        self.water_inbound.fill(ReservoirMass::default());
        self.surface_fluxes.clear();
        self.active_runoff_routes.clear();
        self.cursor = 0;
        self.active_hour = Some(climate_hour);
        self.start_total = self
            .water
            .audit(ReservoirMass::fresh(dynamic_water_total(&self.cells) as u64))
            .current_water_hu as i128;
        self.evaporation_units = 0;
        self.condensation_units = 0;
        self.precipitation_units = 0;
        self.active_water_cycle_outflow = 0;
    }

    /// Roll a failed sliced hour back to its exact pre-hour state and latch
    /// the failure so the server does not retry the same transaction every
    /// tick. The next process restart may retry after code/operator repair;
    /// no corrupt dynamic state is persisted in the meantime.
    pub fn abort_failed_hour(&mut self) {
        let Some(hour) = self.active_hour.take() else {
            return;
        };
        if self.cells_swapped {
            std::mem::swap(&mut self.cells.cells.values, &mut self.scratch);
            std::mem::swap(&mut self.water.cells.values, &mut self.water_scratch);
        }
        if let Some(checkpoint) = self.checkpoint.take() {
            self.water.aquifers = checkpoint.aquifers;
            self.water.reservoirs = checkpoint.reservoirs;
            self.water.springs = checkpoint.springs;
            self.water.inboxes = checkpoint.inboxes;
            self.water.ledger = checkpoint.ledger;
            self.water.completed_surface_hours = checkpoint.completed_surface_hours;
            self.water.completed_groundwater_days = checkpoint.completed_groundwater_days;
        }
        self.cursor = 0;
        self.surface_fluxes.clear();
        self.active_runoff_routes.clear();
        self.water_inbound.fill(ReservoirMass::default());
        self.pending_water_cycle_outflow = 0;
        self.active_water_cycle_outflow = 0;
        self.cells_swapped = false;
        self.failed_hour = Some(hour);
    }

    /// Move already-landed precipitation out of the coarse climate reserve
    /// and into the voxel water cycle. Rain may only draw from runoff (water
    /// accepted by soil remains in soil); snow draws from snowpack. The
    /// returned amount is the only amount the caller is allowed to
    /// materialize, which prevents precipitation from being counted twice.
    pub fn withdraw_water_cycle_transfer(
        &mut self,
        pos: AtlasPos,
        form: PrecipitationForm,
        requested: u32,
    ) -> u32 {
        self.withdraw_water_cycle_mass(pos, form, requested)
            .water_hu
            .min(u64::from(u32::MAX)) as u32
    }

    pub fn withdraw_water_cycle_mass(
        &mut self,
        pos: AtlasPos,
        form: PrecipitationForm,
        requested: u32,
    ) -> ReservoirMass {
        let side = self.cells.cells.side();
        if requested == 0 || pos.u >= side || pos.v >= side {
            return ReservoirMass::default();
        }
        let index = pos.index(side);
        let processed = self.active_hour.is_some() && index < self.cursor;
        let source = match form {
            PrecipitationForm::Rain => &mut self.water.cells.values[index].runoff,
            PrecipitationForm::Snow => &mut self.water.cells.values[index].snow,
            PrecipitationForm::None => return ReservoirMass::default(),
        };
        let transferred_mass = source.take(u64::from(requested));
        if processed {
            let scratch_source = match form {
                PrecipitationForm::Rain => &mut self.water_scratch[index].runoff,
                PrecipitationForm::Snow => &mut self.water_scratch[index].snow,
                PrecipitationForm::None => unreachable!(),
            };
            let _ = scratch_source.take(transferred_mass.water_hu);
        }
        self.water
            .credit_detailed(transferred_mass)
            .expect("water commitment total fits u64");
        let transferred = transferred_mass.water_hu as u32;
        self.pending_water_cycle_outflow = self
            .pending_water_cycle_outflow
            .saturating_add(u64::from(transferred));
        if self.active_hour.is_some() {
            self.active_water_cycle_outflow = self
                .active_water_cycle_outflow
                .saturating_add(u64::from(transferred));
        }
        transferred_mass
    }

    pub fn credit_detailed_vapor(&mut self, pos: AtlasPos, mass: ReservoirMass) -> bool {
        self.credit_detailed_vapor_from(pos, None, mass)
    }

    pub fn credit_detailed_vapor_from(
        &mut self,
        pos: AtlasPos,
        preferred: Option<u64>,
        mass: ReservoirMass,
    ) -> bool {
        if mass.water_hu == 0 {
            return false;
        }
        let index = pos.index(self.cells.cells.side());
        let Ok(water_hu) = u32::try_from(mass.water_hu) else {
            return false;
        };
        let Some(next) = self.cells.cells.values()[index]
            .atmospheric_vapor
            .checked_add(water_hu)
        else {
            return false;
        };
        if self.active_hour.is_some()
            && index < self.cursor
            && self.scratch[index]
                .atmospheric_vapor
                .checked_add(water_hu)
                .is_none()
        {
            return false;
        }
        if !self.water.debit_detailed_exact_from(preferred, mass) {
            return false;
        }
        self.cells.cells.values_mut()[index].atmospheric_vapor = next;
        if self.active_hour.is_some() && index < self.cursor {
            let Some(next) = self.scratch[index].atmospheric_vapor.checked_add(water_hu) else {
                return false;
            };
            self.scratch[index].atmospheric_vapor = next;
        }
        self.water.ledger.precipitated_salt_mass = self
            .water
            .ledger
            .precipitated_salt_mass
            .saturating_add(mass.salt_mass);
        true
    }

    pub fn return_detailed_to_runoff(&mut self, pos: AtlasPos, mass: ReservoirMass) -> bool {
        if !self.water.debit_detailed_exact(mass) {
            return false;
        }
        let index = pos.index(self.water.cells.side());
        self.water.cells.values_mut()[index]
            .runoff
            .add_assign(mass)
            .is_ok()
    }

    pub fn reject_detailed_salt_to_runoff(&mut self, pos: AtlasPos, salt_mass: u64) -> bool {
        self.reject_detailed_salt_to_runoff_from(pos, None, salt_mass)
    }

    pub fn reject_detailed_salt_to_runoff_from(
        &mut self,
        pos: AtlasPos,
        preferred: Option<u64>,
        salt_mass: u64,
    ) -> bool {
        let mass = ReservoirMass {
            water_hu: 0,
            salt_mass,
        };
        if !self.water.debit_detailed_exact_from(preferred, mass) {
            return false;
        }
        let index = pos.index(self.water.cells.side());
        self.water.cells.values_mut()[index]
            .runoff
            .add_assign(mass)
            .is_ok()
    }

    pub fn move_detailed_to_industrial_from(
        &mut self,
        preferred: Option<u64>,
        mass: ReservoirMass,
    ) -> bool {
        if !self.water.debit_detailed_exact_from(preferred, mass) {
            return false;
        }
        if self.water.ledger.industrial.add_assign(mass).is_err() {
            self.water
                .credit_detailed_to(preferred, mass)
                .expect("industrial rollback fits");
            return false;
        }
        true
    }

    pub fn move_detailed_to_portable_from(
        &mut self,
        preferred: Option<u64>,
        mass: ReservoirMass,
    ) -> Option<WaterClass> {
        let class = mass.water_class();
        if !self.water.debit_detailed_exact_from(preferred, mass) {
            return None;
        }
        if self.water.ledger.portable[class as usize]
            .add_assign(mass)
            .is_err()
        {
            self.water
                .credit_detailed_to(preferred, mass)
                .expect("portable rollback fits");
            return None;
        }
        Some(class)
    }

    pub fn pump_groundwater(&mut self, pos: AtlasPos, requested_hu: u64) -> ReservoirMass {
        let index = pos.index(self.water.cells.side());
        let parcel = self.water.cells.values_mut()[index]
            .groundwater
            .take(requested_hu);
        if parcel.water_hu == 0 {
            return parcel;
        }
        if self.water.credit_detailed(parcel).is_err() {
            self.water.cells.values_mut()[index]
                .groundwater
                .add_assign(parcel)
                .expect("groundwater rollback fits");
            return ReservoirMass::default();
        }
        let drawdown = parcel.water_hu.min(i32::MAX as u64) as i32;
        self.water.cells.values_mut()[index].groundwater_head_milliblocks =
            self.water.cells.values()[index]
                .groundwater_head_milliblocks
                .saturating_sub((drawdown / 8).max(1));
        parcel
    }

    pub fn move_detailed_to_industrial(&mut self, mass: ReservoirMass) -> bool {
        self.move_detailed_to_industrial_from(None, mass)
    }

    pub fn move_detailed_to_portable(&mut self, mass: ReservoirMass) -> Option<WaterClass> {
        self.move_detailed_to_portable_from(None, mass)
    }

    pub fn move_portable_to_detailed(&mut self, class: WaterClass) -> Option<ReservoirMass> {
        let parcel = self.water.ledger.portable[class as usize].take(HYDRO_UNITS_PER_BLOCK);
        if parcel.water_hu != HYDRO_UNITS_PER_BLOCK {
            self.water.ledger.portable[class as usize]
                .add_assign(parcel)
                .expect("portable rollback fits");
            return None;
        }
        if self.water.credit_detailed(parcel).is_err() {
            self.water.ledger.portable[class as usize]
                .add_assign(parcel)
                .expect("portable rollback fits");
            return None;
        }
        Some(parcel)
    }

    /// Move one exact portable vessel into a host-owned industrial
    /// subdivision such as an alchemy batch. The returned mass is the
    /// subdivision's custody record; the planetary aggregate remains in the
    /// industrial ledger until use, spill, or disposal returns it.
    pub fn move_portable_to_industrial(
        &mut self,
        class: WaterClass,
        requested_hu: u64,
    ) -> Option<ReservoirMass> {
        let expected = self.preview_move_portable_to_industrial(class, requested_hu)?;
        let parcel = self.water.ledger.portable[class as usize].take(requested_hu);
        debug_assert_eq!(parcel, expected);
        if self.water.ledger.industrial.add_assign(parcel).is_err() {
            self.water.ledger.portable[class as usize]
                .add_assign(parcel)
                .expect("portable alchemy rollback fits");
            return None;
        }
        Some(parcel)
    }

    pub fn preview_move_portable_to_industrial(
        &self,
        class: WaterClass,
        requested_hu: u64,
    ) -> Option<ReservoirMass> {
        if requested_hu == 0 {
            return None;
        }
        let mut portable = self.water.ledger.portable[class as usize];
        let parcel = portable.take(requested_hu);
        (parcel.water_hu == requested_hu
            && self.water.ledger.industrial.checked_add(parcel).is_some())
        .then_some(parcel)
    }

    /// Preflight and perform the common cleaning exchange without cloning
    /// whole-planet weather: one portable vessel enters industrial custody,
    /// then that cleaning water plus an existing exact residue parcel enter
    /// the local runoff cell together.
    pub fn preview_portable_exchange_to_runoff(
        &self,
        pos: AtlasPos,
        class: WaterClass,
        requested_hu: u64,
        existing_industrial: ReservoirMass,
    ) -> Option<ReservoirMass> {
        let parcel = self.preview_move_portable_to_industrial(class, requested_hu)?;
        let industrial_after = self.water.ledger.industrial.checked_add(parcel)?;
        let runoff_parcel = parcel.checked_add(existing_industrial)?;
        if industrial_after.water_hu < runoff_parcel.water_hu
            || industrial_after.salt_mass < runoff_parcel.salt_mass
        {
            return None;
        }
        self.water
            .cells
            .values()
            .get(pos.index(self.water.cells.side()))?
            .runoff
            .checked_add(runoff_parcel)?;
        Some(parcel)
    }

    pub fn portable_exchange_to_runoff(
        &mut self,
        pos: AtlasPos,
        class: WaterClass,
        requested_hu: u64,
        existing_industrial: ReservoirMass,
    ) -> Option<ReservoirMass> {
        let expected = self.preview_portable_exchange_to_runoff(
            pos,
            class,
            requested_hu,
            existing_industrial,
        )?;
        let parcel = self.move_portable_to_industrial(class, requested_hu)?;
        debug_assert_eq!(parcel, expected);
        let runoff_parcel = parcel.checked_add(existing_industrial)?;
        let returned = self.return_industrial_exact_to_runoff(pos, runoff_parcel);
        debug_assert!(returned, "preflighted cleaning-water exchange failed");
        returned.then_some(parcel)
    }

    /// Settle an exact industrial subdivision back into local soil. This is
    /// used for drinking, plot application, and responsible liquid disposal;
    /// salt travels with the same parcel instead of being relabelled fresh.
    pub fn can_return_industrial_exact_to_soil(&self, pos: AtlasPos, mass: ReservoirMass) -> bool {
        if self.water.ledger.industrial.water_hu < mass.water_hu
            || self.water.ledger.industrial.salt_mass < mass.salt_mass
        {
            return false;
        }
        let index = pos.index(self.water.cells.side());
        let in_scratch = self.active_hour.is_some() && index < self.cursor;
        let soil = if in_scratch {
            self.water_scratch.get(index).map(|cell| cell.soil)
        } else {
            self.water.cells.values().get(index).map(|cell| cell.soil)
        };
        soil.and_then(|soil| soil.checked_add(mass)).is_some()
    }

    pub fn return_industrial_exact_to_soil(&mut self, pos: AtlasPos, mass: ReservoirMass) -> bool {
        if self.water.ledger.industrial.take_exact(mass).is_none() {
            return false;
        }
        let index = pos.index(self.water.cells.side());
        let in_scratch = self.active_hour.is_some() && index < self.cursor;
        let water = if in_scratch {
            &mut self.water_scratch[index]
        } else {
            &mut self.water.cells.values_mut()[index]
        };
        if water.soil.add_assign(mass).is_err() {
            self.water
                .ledger
                .industrial
                .add_assign(mass)
                .expect("industrial soil rollback fits");
            return false;
        }
        true
    }

    pub fn return_industrial_exact_to_runoff(
        &mut self,
        pos: AtlasPos,
        mass: ReservoirMass,
    ) -> bool {
        if self.water.ledger.industrial.take_exact(mass).is_none() {
            return false;
        }
        let index = pos.index(self.water.cells.side());
        if self.water.cells.values_mut()[index]
            .runoff
            .add_assign(mass)
            .is_err()
        {
            self.water
                .ledger
                .industrial
                .add_assign(mass)
                .expect("industrial runoff rollback fits");
            return false;
        }
        true
    }

    pub fn can_return_industrial_exact_to_runoff(
        &self,
        pos: AtlasPos,
        mass: ReservoirMass,
    ) -> bool {
        self.water.ledger.industrial.water_hu >= mass.water_hu
            && self.water.ledger.industrial.salt_mass >= mass.salt_mass
            && self
                .water
                .cells
                .values()
                .get(pos.index(self.water.cells.side()))
                .and_then(|cell| cell.runoff.checked_add(mass))
                .is_some()
    }

    pub fn materialize_surface_water(
        &mut self,
        reservoir_id: u64,
        requested_hu: u64,
    ) -> ReservoirMass {
        let Some(reservoir) = self.water.reservoir_mut(reservoir_id) else {
            return ReservoirMass::default();
        };
        let parcel = reservoir.coarse.take(requested_hu);
        let reservoir = self
            .water
            .reservoir_mut(reservoir_id)
            .expect("source surface reservoir still exists");
        if reservoir.committed.add_assign(parcel).is_err() {
            reservoir
                .coarse
                .add_assign(parcel)
                .expect("surface rollback fits");
            return ReservoirMass::default();
        }
        parcel
    }

    pub fn dematerialize_surface_water(&mut self, reservoir_id: u64, mass: ReservoirMass) -> bool {
        if !self
            .water
            .debit_detailed_exact_from(Some(reservoir_id), mass)
        {
            return false;
        }
        let Some(reservoir) = self.water.reservoir_mut(reservoir_id) else {
            self.water
                .credit_detailed(mass)
                .expect("dematerialization rollback fits");
            return false;
        };
        reservoir.coarse.add_assign(mass).is_ok()
    }

    pub fn register_dynamic_basin(
        &mut self,
        chunk: ChunkPos,
        level_milliblocks: i32,
        mass: ReservoirMass,
    ) -> Option<u64> {
        let id = self.ensure_dynamic_basin(chunk, level_milliblocks);
        self.water.reservoir_mut(id)?.committed.checked_add(mass)?;
        if self
            .water
            .commitments
            .iter()
            .find(|commitment| commitment.chunk == chunk && commitment.reservoir == id)
            .is_some_and(|commitment| commitment.mass.checked_add(mass).is_none())
        {
            return None;
        }
        if !self.water.debit_detailed_exact(mass) {
            return None;
        }
        self.water
            .reservoir_mut(id)
            .expect("dynamic basin exists")
            .committed
            .add_assign(mass)
            .expect("dynamic basin addition was preflighted");
        if let Some(commitment) = self
            .water
            .commitments
            .iter_mut()
            .find(|commitment| commitment.chunk == chunk && commitment.reservoir == id)
        {
            commitment
                .mass
                .add_assign(mass)
                .expect("dynamic commitment addition was preflighted");
        } else if mass.water_hu != 0 || mass.salt_mass != 0 {
            self.water.commitments.push(ChunkWaterCommitment {
                chunk,
                reservoir: id,
                mass,
            });
            self.water
                .commitments
                .sort_by_key(|commitment| (commitment.chunk, commitment.reservoir));
        }
        Some(id)
    }

    /// Register the topological fact that player terrain can hold a local
    /// basin even before it contains a visible HU. Waterfront masonry and
    /// excavations call this; later bucket/flux transfers attach exact mass
    /// to the same stable chunk-derived id.
    pub fn ensure_dynamic_basin(&mut self, chunk: ChunkPos, level_milliblocks: i32) -> u64 {
        let local_id = (chunk.face() as u32)
            .saturating_mul(u32::from(crate::planet::FACE_CHUNKS).pow(2))
            .saturating_add(u32::from(chunk.v()) * u32::from(crate::planet::FACE_CHUNKS))
            .saturating_add(u32::from(chunk.u()))
            .saturating_add(1);
        let id = surface_reservoir_id(SurfaceReservoirKind::Dynamic, local_id);
        if self.water.reservoir_mut(id).is_none() {
            let insertion = self
                .water
                .reservoirs
                .binary_search_by_key(&id, |reservoir| reservoir.id)
                .unwrap_err();
            self.water.reservoirs.insert(
                insertion,
                SurfaceReservoirState {
                    id,
                    name: format!(
                        "player basin {}:{},{}",
                        chunk.face().name(),
                        chunk.u(),
                        chunk.v()
                    ),
                    coarse: ReservoirMass::default(),
                    committed: ReservoirMass::default(),
                    initial_total_hu: 0,
                    level_milliblocks,
                },
            );
        }
        id
    }

    pub fn breach_surface_reservoir(
        &mut self,
        source: u64,
        destination: u64,
        requested_hu: u64,
    ) -> ReservoirMass {
        let Some(source_index) = self
            .water
            .reservoirs
            .binary_search_by_key(&source, |reservoir| reservoir.id)
            .ok()
        else {
            return ReservoirMass::default();
        };
        let parcel = self.water.reservoirs[source_index]
            .coarse
            .take(requested_hu);
        let Some(destination) = self.water.reservoir_mut(destination) else {
            self.water.reservoirs[source_index]
                .coarse
                .add_assign(parcel)
                .expect("breach rollback fits");
            return ReservoirMass::default();
        };
        if destination.coarse.add_assign(parcel).is_err() {
            self.water.reservoirs[source_index]
                .coarse
                .add_assign(parcel)
                .expect("breach rollback fits");
            return ReservoirMass::default();
        }
        parcel
    }

    /// Move process water into local atmospheric vapor. Existing ordinary
    /// machines use this unrestricted form; subsystems with water already in
    /// flight use `exhaust_industrial_vapor_excluding` so another machine
    /// cannot spend their custody.
    pub fn exhaust_industrial_vapor(&mut self, pos: AtlasPos, requested_hu: u64) -> u64 {
        self.exhaust_industrial_vapor_excluding(pos, requested_hu, 0)
    }

    pub fn exhaust_industrial_vapor_excluding(
        &mut self,
        pos: AtlasPos,
        requested_hu: u64,
        reserved_hu: u64,
    ) -> u64 {
        let available = self
            .water
            .ledger
            .industrial
            .water_hu
            .saturating_sub(reserved_hu);
        let water_hu = requested_hu.min(available);
        if water_hu == 0 || water_hu > u64::from(u32::MAX) {
            return 0;
        }
        let amount = water_hu as u32;
        let index = pos.index(self.cells.cells.side());
        let Some(next) = self.cells.cells.values()[index]
            .atmospheric_vapor
            .checked_add(amount)
        else {
            return 0;
        };
        if self.active_hour.is_some()
            && index < self.cursor
            && self.scratch[index]
                .atmospheric_vapor
                .checked_add(amount)
                .is_none()
        {
            return 0;
        }
        self.water.ledger.industrial.water_hu -= water_hu;
        self.cells.cells.values_mut()[index].atmospheric_vapor = next;
        if self.active_hour.is_some() && index < self.cursor {
            self.scratch[index].atmospheric_vapor += amount;
        }
        water_hu
    }

    fn plan_surface_evaporation(&mut self, id: u64, requested_hu: u64) -> ReservoirMass {
        let already = self
            .surface_fluxes
            .get(&id)
            .map_or(0, |(debit, _)| debit.water_hu);
        let available = self
            .water
            .reservoirs
            .binary_search_by_key(&id, |reservoir| reservoir.id)
            .ok()
            .map_or(0, |index| {
                self.water.reservoirs[index]
                    .coarse
                    .water_hu
                    .saturating_sub(already)
            });
        let mass = ReservoirMass::fresh(requested_hu.min(available));
        let entry = self.surface_fluxes.entry(id).or_default();
        entry.0.water_hu = entry.0.water_hu.saturating_add(mass.water_hu);
        mass
    }

    fn plan_surface_credit(&mut self, id: u64, mass: ReservoirMass) {
        let entry = self.surface_fluxes.entry(id).or_default();
        entry.1.add_assign(mass).expect("surface flux fits u64");
    }

    /// Advance at most `budget` cells. The closure returns local Ire on the
    /// familiar 0..=100 scale; it can strengthen instability but never enters
    /// any water transfer equation.
    pub fn advance_slice(
        &mut self,
        atlas: &PlanetAtlas,
        day: f64,
        budget: usize,
        mut local_ire: impl FnMut(AtlasPos) -> f32,
    ) -> Result<Option<WeatherStepReport>, AtlasError> {
        let Some(climate_hour) = self.active_hour else {
            return Ok(None);
        };
        if self.cells.cells.side() != atlas.side()
            || self.cells.cells.len() != atlas.dynamic.cells.len()
        {
            return Err(AtlasError::Corrupt(
                "weather state dimensions do not match immutable climate".into(),
            ));
        }
        let side = atlas.side();
        let end = self
            .cursor
            .saturating_add(budget)
            .min(self.cells.cells.len());
        for index in self.cursor..end {
            let pos = AtlasPos::from_index(index, side).expect("weather index");
            let climate = atlas.genesis.climate.values()[index];
            let terrain = atlas.genesis.terrain.values()[index];
            let ground = atlas.genesis.ground.values()[index];
            let hydro = atlas.genesis.hydrology.values()[index];
            let mut source = self.cells.cells.values()[index];
            let mut water = self.water.cells.values()[index];
            let temperature = seasonal_scalar(climate.seasonal_temperature, day)
                + f32::from(source.weather_temperature_anomaly) / 100.0;
            let target_vapor = ((climate.mean_atmospheric_moisture * 180.0).max(24.0) as u32)
                .saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL as u32);

            let evaporation = if source.atmospheric_vapor < target_vapor {
                ((target_vapor - source.atmospheric_vapor) / 12
                    + HYDRO_UNITS_PER_VISIBLE_LEVEL as u32)
                    .min(96 * HYDRO_UNITS_PER_VISIBLE_LEVEL as u32)
            } else {
                0
            };
            let surface_id = hydrology_surface_id(hydro);
            let evaporation_mass = if let Some(id) = surface_id {
                self.plan_surface_evaporation(id, u64::from(evaporation))
            } else {
                water.soil.take_fresh_water(u64::from(evaporation))
            };
            let transpiration_request = if terrain.eroded_elevation > SEA_LEVEL as f32 {
                u64::from(evaporation / 4).saturating_mul(u64::from(
                    atlas.genesis.biomes.values()[index].baseline_biome != 2,
                ))
            } else {
                0
            };
            let transpired = water.soil.take_fresh_water(transpiration_request);
            let actual_evaporation = evaporation_mass
                .water_hu
                .saturating_add(transpired.water_hu)
                .min(u64::from(u32::MAX)) as u32;
            source.atmospheric_vapor = source
                .atmospheric_vapor
                .checked_add(actual_evaporation)
                .ok_or_else(|| AtlasError::Corrupt("atmospheric vapor overflow".into()))?;
            water.last_evaporation_hu = actual_evaporation;
            self.evaporation_units += u64::from(actual_evaporation);

            let saturation = ((target_vapor as f32)
                * (1.0 + ((temperature - climate.mean_temperature) * 0.025).clamp(-0.35, 0.45)))
            .max(12.0) as u32;
            let condensation = source
                .atmospheric_vapor
                .saturating_sub(saturation)
                .saturating_div(3)
                .min(512 * HYDRO_UNITS_PER_VISIBLE_LEVEL as u32)
                .min(u32::MAX - source.cloud_water);
            source.atmospheric_vapor -= condensation;
            source.cloud_water += condensation;
            self.condensation_units += u64::from(condensation);

            let east = pos.step(Direction4::East, side).pos.index(side);
            let west = pos.step(Direction4::West, side).pos.index(side);
            let north = pos.step(Direction4::North, side).pos.index(side);
            let south = pos.step(Direction4::South, side).pos.index(side);
            let pressure_gradient = i32::from(self.cells.cells.values()[west].pressure_anomaly)
                - i32::from(self.cells.cells.values()[east].pressure_anomaly)
                + i32::from(self.cells.cells.values()[south].pressure_anomaly)
                - i32::from(self.cells.cells.values()[north].pressure_anomaly);
            let unit = DVec3::from_array(
                atlas.genesis.geometry.values()[index]
                    .unit_direction
                    .map(f64::from),
            );
            let wave_axis = DVec3::new(
                (climate_hour as f64 * 0.071).cos(),
                0.37,
                (climate_hour as f64 * 0.071).sin(),
            )
            .normalize();
            let wave = (unit.dot(wave_axis) * 9.0 + climate_hour as f64 * 0.31).sin();
            let pressure = (i32::from(source.pressure_anomaly) * 3 / 4 + (wave * 420.0) as i32)
                .clamp(i32::from(i16::MIN), i32::from(i16::MAX));
            let temperature_anomaly = (i32::from(source.weather_temperature_anomaly) * 4 / 5
                - pressure / 18
                + (wave * 32.0) as i32)
                .clamp(-2400, 2400);
            let ire = local_ire(pos).clamp(0.0, 100.0);
            let cloud_fraction =
                (source.cloud_water as f32 / (target_vapor as f32 + 1.0)).clamp(0.0, 2.0);
            let instability = (pressure_gradient.unsigned_abs() as f32 / 1200.0).clamp(0.0, 1.0);
            let storm_energy = ((cloud_fraction * 18_000.0 + instability * 18_000.0 + ire * 180.0)
                .clamp(0.0, 65_535.0)) as u16;
            let seasonal_precip = seasonal_scalar(climate.seasonal_precipitation, day);
            let precipitation_fraction = (0.012
                + seasonal_precip / climate.mean_precipitation.max(1.0) * 0.028
                + f32::from(storm_energy) / 65_535.0 * 0.12)
                .clamp(0.006, 0.22);
            let precipitation_threshold =
                (target_vapor * 7 / 100).max(24 * HYDRO_UNITS_PER_VISIBLE_LEVEL as u32);
            let precipitable = source.cloud_water.saturating_sub(precipitation_threshold);
            let precipitation =
                ((precipitable as f32 * precipitation_fraction) as u32).min(precipitable);
            source.cloud_water -= precipitation;
            if temperature <= 0.0 {
                water
                    .snow
                    .add_assign(ReservoirMass::fresh(u64::from(precipitation)))?;
            } else {
                let soil_capacity = u64::from(
                    ground
                        .aquifer_capacity
                        .saturating_div(8)
                        .max((climate.mean_precipitation * 3.0) as u32)
                        .max(256),
                )
                .saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL);
                let accepted =
                    u64::from(precipitation).min(soil_capacity.saturating_sub(water.soil.water_hu));
                water.soil.add_assign(ReservoirMass::fresh(accepted))?;
                water.runoff.add_assign(ReservoirMass::fresh(
                    u64::from(precipitation).saturating_sub(accepted),
                ))?;
            }
            self.precipitation_units += u64::from(precipitation);

            // Hourly infiltration/recharge is bounded by permeability and
            // storage. Frozen ground slows but never disables the path.
            let groundwater_capacity =
                u64::from(ground.aquifer_capacity).saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL);
            let permeability_rate = (u64::from(ground.aquifer_permeability) / 2048).max(1);
            let frozen_divisor = if temperature <= 0.0 { 8 } else { 1 };
            let recharge_request = permeability_rate / frozen_divisor;
            let recharge = water.soil.take(
                recharge_request
                    .min(groundwater_capacity.saturating_sub(water.groundwater.water_hu)),
            );
            water.groundwater.add_assign(recharge)?;
            water.last_recharge_hu = recharge.water_hu.min(u64::from(u32::MAX)) as u32;

            // Springs and perennial river baseflow are pressure-dependent
            // transfers from the shallow aquifer, never perpetual sources.
            let spring_index = self
                .water
                .springs
                .binary_search_by_key(&pos, |spring| spring.pos)
                .ok();
            let spring_active = spring_index.and_then(|spring_index| {
                let spring = self.water.springs[spring_index];
                (water.groundwater_head_milliblocks > spring.outlet_milliblocks)
                    .then_some((spring_index, spring))
            });
            if let Some((spring_index, spring)) = spring_active {
                let pressure = water
                    .groundwater_head_milliblocks
                    .saturating_sub(spring.outlet_milliblocks)
                    as u64;
                let discharge = water.groundwater.take((pressure / 250).clamp(1, 256));
                water.runoff.add_assign(discharge)?;
                water.last_spring_hu = discharge.water_hu.min(u64::from(u32::MAX)) as u32;
                self.water.springs[spring_index].last_discharge_hu = water.last_spring_hu;
                self.water.springs[spring_index].active = discharge.water_hu != 0;
            } else {
                water.last_spring_hu = 0;
                if let Some(spring_index) = spring_index {
                    self.water.springs[spring_index].last_discharge_hu = 0;
                    self.water.springs[spring_index].active = false;
                }
            }
            if let Some((river_id, baseflow)) = take_river_baseflow(hydro, &mut water) {
                self.plan_surface_credit(
                    surface_reservoir_id(SurfaceReservoirKind::River, river_id),
                    baseflow,
                );
            }

            // Route a bounded parcel of standing runoff through the immutable
            // seam-aware drainage graph. Incoming parcels are applied after
            // the complete old-state pass.
            let runoff_before = water.runoff.water_hu;
            let routed = water.runoff.take((water.runoff.water_hu / 4).max(u64::from(
                water.runoff.water_hu >= HYDRO_UNITS_PER_VISIBLE_LEVEL,
            )));
            if routed.water_hu != 0 {
                if let Some(id) = surface_id {
                    self.plan_surface_credit(id, routed);
                } else if hydro.drainage_receiver != u32::MAX {
                    let receiver = hydro.drainage_receiver as usize;
                    let receiver_pos = AtlasPos::from_index(receiver, side)
                        .expect("drainage receiver is validated");
                    self.active_runoff_routes.push(RunoffTransport {
                        from: pos,
                        to: receiver_pos,
                        water_hu: routed.water_hu,
                        source_water_before_hu: runoff_before,
                    });
                    let materialized = self.water.commitments.iter().any(|commitment| {
                        AtlasPos::from_surface(commitment.chunk.block_origin(), side)
                            == receiver_pos
                    });
                    if materialized {
                        if let Some(inbox) = self
                            .water
                            .inboxes
                            .iter_mut()
                            .find(|inbox| inbox.pos == receiver_pos && inbox.reservoir == 0)
                        {
                            inbox.mass.add_assign(routed)?;
                        } else {
                            self.water.inboxes.push(FluxInbox {
                                pos: receiver_pos,
                                reservoir: 0,
                                mass: routed,
                            });
                        }
                    } else {
                        self.water_inbound[receiver].add_assign(routed)?;
                    }
                } else {
                    water.runoff.add_assign(routed)?;
                }
            }

            let season_wind = seasonal_vector(climate.seasonal_wind, day);
            let wind_anomaly = [
                (-pressure_gradient / 5).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
                ((i32::from(self.cells.cells.values()[south].pressure_anomaly)
                    - i32::from(self.cells.cells.values()[north].pressure_anomaly))
                    / 3)
                .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
            ];
            let wind = [
                season_wind[0] + f32::from(wind_anomaly[0]) / 16_384.0,
                season_wind[1] + f32::from(wind_anomaly[1]) / 16_384.0,
            ];
            let wind_vector = chart_vector(pos, side, wind);
            let receiver = downstream_neighbor(pos, side, wind_vector);
            let receiver_index = receiver.index(side);
            let stencil = transport_stencil(pos, side, wind_vector);

            let vapor_transport = source.atmospheric_vapor / 4;
            let cloud_transport = source.cloud_water * 3 / 10;
            source.atmospheric_vapor -= vapor_transport;
            source.cloud_water -= cloud_transport;
            let vapor_room = u32::MAX - self.scratch[index].atmospheric_vapor;
            let local_vapor = source.atmospheric_vapor.min(vapor_room);
            self.scratch[index].atmospheric_vapor += local_vapor;
            let mut rejected_vapor = source.atmospheric_vapor - local_vapor;
            let cloud_room = u32::MAX - self.scratch[index].cloud_water;
            let local_cloud = source.cloud_water.min(cloud_room);
            self.scratch[index].cloud_water += local_cloud;
            let mut rejected_cloud = source.cloud_water - local_cloud;
            distribute_u32(vapor_transport, stencil, |target, share| {
                let room = u32::MAX - self.scratch[target].atmospheric_vapor;
                let accepted = share.min(room);
                self.scratch[target].atmospheric_vapor += accepted;
                rejected_vapor += share - accepted;
            });
            spill_vapor(&mut self.scratch, index, rejected_vapor)?;
            distribute_u32(cloud_transport, stencil, |target, share| {
                let room = u32::MAX - self.scratch[target].cloud_water;
                let accepted = share.min(room);
                self.scratch[target].cloud_water += accepted;
                rejected_cloud += share - accepted;
            });
            spill_cloud(&mut self.scratch, index, rejected_cloud)?;

            let temperature_transport = temperature_anomaly * 3 / 10;
            add_i16(
                &mut self.scratch[index].weather_temperature_anomaly,
                temperature_anomaly - temperature_transport,
            );
            add_i16(
                &mut self.scratch[receiver_index].weather_temperature_anomaly,
                temperature_transport,
            );
            let pressure_transport = pressure * 3 / 10;
            add_i16(
                &mut self.scratch[index].pressure_anomaly,
                pressure - pressure_transport,
            );
            add_i16(
                &mut self.scratch[receiver_index].pressure_anomaly,
                pressure_transport,
            );

            let destination = &mut self.scratch[index];
            self.water_scratch[index] = water;
            destination.local_weather_anomaly = temperature_anomaly;
            destination.storm_energy = storm_energy;
            destination.precipitation_rate = precipitation.min(u32::from(u16::MAX)) as u16;
            destination.wind_anomaly = wind_anomaly;
            destination.fire_moisture_anomaly =
                (source.fire_moisture_anomaly + precipitation.min(i32::MAX as u32) as i32 - 2)
                    .clamp(-20_000, 20_000);
            destination.vegetation_moisture_anomaly = (source.vegetation_moisture_anomaly
                + precipitation.min(i32::MAX as u32) as i32
                - actual_evaporation.min(i32::MAX as u32) as i32)
                .clamp(-20_000, 20_000);
        }
        self.cursor = end;
        if self.cursor < self.cells.cells.len() {
            return Ok(None);
        }

        for (cell, inbound) in self.water_scratch.iter_mut().zip(&self.water_inbound) {
            cell.runoff.add_assign(*inbound)?;
        }
        for (id, (debit, credit)) in std::mem::take(&mut self.surface_fluxes) {
            if self.water.reservoir_mut(id).is_none() {
                let insertion = self
                    .water
                    .reservoirs
                    .binary_search_by_key(&id, |reservoir| reservoir.id)
                    .unwrap_err();
                self.water.reservoirs.insert(
                    insertion,
                    SurfaceReservoirState {
                        id,
                        name: format!("dynamic surface reservoir {id}"),
                        coarse: ReservoirMass::default(),
                        committed: ReservoirMass::default(),
                        initial_total_hu: 0,
                        level_milliblocks: 0,
                    },
                );
            }
            let reservoir = self
                .water
                .reservoir_mut(id)
                .expect("surface reservoir exists");
            let removed = reservoir.coarse.take_fresh_water(debit.water_hu);
            if removed.water_hu != debit.water_hu {
                return Err(AtlasError::Corrupt(format!(
                    "surface reservoir {id} could not honor a planned debit"
                )));
            }
            reservoir.coarse.add_assign(credit)?;
        }
        std::mem::swap(&mut self.cells.cells.values, &mut self.scratch);
        std::mem::swap(&mut self.water.cells.values, &mut self.water_scratch);
        self.cells_swapped = true;
        self.precipitate_terminal_lake_salt(atlas);
        self.water.completed_surface_hours = self.water.completed_surface_hours.saturating_add(1);
        if self.water.completed_surface_hours.is_multiple_of(24) {
            self.advance_groundwater_day(atlas)?;
        }
        self.reconcile_basin_levels(atlas);
        let end_audit = self
            .water
            .audit(ReservoirMass::fresh(dynamic_water_total(&self.cells) as u64));
        let end_total = i128::from(end_audit.current_water_hu);
        let report = WeatherStepReport {
            climate_hour,
            processed_cells: self.cells.cells.len(),
            evaporation_units: self.evaporation_units,
            condensation_units: self.condensation_units,
            precipitation_units: self.precipitation_units,
            water_cycle_outflow_units: self.pending_water_cycle_outflow,
            atmospheric_water_before: self.start_total,
            atmospheric_water_after: end_total,
            unexplained_water_drift: end_total - self.start_total,
        };
        if report.unexplained_water_drift != 0
            || end_audit.unexplained_water_delta_hu != 0
            || end_audit.unexplained_salt_delta != 0
        {
            return Err(AtlasError::Corrupt(format!(
                "planetary water drift in hour {climate_hour}: step {}, ledger {} HU / {} salt",
                report.unexplained_water_drift,
                end_audit.unexplained_water_delta_hu,
                end_audit.unexplained_salt_delta,
            )));
        }
        self.active_hour = None;
        self.checkpoint = None;
        self.cells_swapped = false;
        self.cursor = 0;
        self.pending_water_cycle_outflow = 0;
        self.active_water_cycle_outflow = 0;
        self.completed_hours = self.completed_hours.saturating_add(1);
        self.cells.completed_climate_hours = self.completed_hours;
        self.last_runoff_routes = std::mem::take(&mut self.active_runoff_routes);
        self.last_report = report;
        Ok(Some(report))
    }

    fn advance_groundwater_day(&mut self, atlas: &PlanetAtlas) -> Result<(), AtlasError> {
        let side = atlas.side();
        let count = self.water.cells.len();
        let mut available = self
            .water
            .cells
            .values()
            .iter()
            .map(|cell| cell.groundwater)
            .collect::<Vec<_>>();
        let mut inbound = vec![ReservoirMass::default(); count];
        let mut edges = std::collections::BTreeSet::new();
        for index in 0..count {
            let pos = AtlasPos::from_index(index, side).expect("groundwater index");
            for neighbor in pos.neighbors4(side) {
                let neighbor = neighbor.index(side);
                if neighbor != index {
                    edges.insert((index.min(neighbor), index.max(neighbor)));
                }
            }
        }
        for (index, neighbor) in edges {
            let a_head = self.water.cells.values()[index].groundwater_head_milliblocks;
            let b_head = self.water.cells.values()[neighbor].groundwater_head_milliblocks;
            let (source, destination, gradient) = if a_head > b_head {
                (index, neighbor, u64::from((a_head - b_head) as u32))
            } else {
                (neighbor, index, u64::from((b_head - a_head) as u32))
            };
            if gradient < 2 {
                continue;
            }
            let permeability = u64::from(
                atlas.genesis.ground.values()[source]
                    .aquifer_permeability
                    .min(atlas.genesis.ground.values()[destination].aquifer_permeability),
            );
            let capacity = u64::from(atlas.genesis.ground.values()[destination].aquifer_capacity)
                .saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL);
            let room = capacity.saturating_sub(
                available[destination]
                    .water_hu
                    .saturating_add(inbound[destination].water_hu),
            );
            let requested = gradient
                .saturating_mul(permeability)
                .saturating_div(65_535 * 512)
                .max(1)
                .min(room)
                .min(available[source].water_hu / 128 + 1);
            let parcel = available[source].take(requested);
            inbound[destination].add_assign(parcel)?;
        }
        for index in 0..count {
            available[index].add_assign(inbound[index])?;
            let cell = &mut self.water.cells.values[index];
            cell.groundwater = available[index];
            let ground = atlas.genesis.ground.values()[index];
            let capacity = u64::from(ground.aquifer_capacity)
                .saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL)
                .max(1);
            let saturation_milli = cell
                .groundwater
                .water_hu
                .saturating_mul(4_000)
                .saturating_div(capacity)
                .min(4_000) as i32;
            cell.groundwater_head_milliblocks =
                (ground.baseline_groundwater_head * 1000.0).round() as i32 - 4_000
                    + saturation_milli;
        }
        // Perched and confined stores exchange slowly with the shallow cell.
        // This keeps their pressure response visible without voxelizing pores.
        for aquifer in &mut self.water.aquifers {
            let index = aquifer.pos.index(side);
            let shallow = &mut self.water.cells.values[index];
            if aquifer.head_milliblocks > shallow.groundwater_head_milliblocks {
                let parcel = aquifer.mass.take((aquifer.mass.water_hu / 2048).max(1));
                shallow.groundwater.add_assign(parcel)?;
            } else {
                let room = aquifer.capacity_hu.saturating_sub(aquifer.mass.water_hu);
                let parcel = shallow
                    .groundwater
                    .take((shallow.groundwater.water_hu / 4096).min(room));
                aquifer.mass.add_assign(parcel)?;
            }
            let fullness = aquifer
                .mass
                .water_hu
                .saturating_mul(4_000)
                .saturating_div(aquifer.capacity_hu.max(1)) as i32;
            aquifer.head_milliblocks = aquifer.head_milliblocks.saturating_sub(2_000) + fullness;
        }
        self.water.completed_groundwater_days =
            self.water.completed_groundwater_days.saturating_add(1);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn advance_groundwater_day_for_test(
        &mut self,
        atlas: &PlanetAtlas,
    ) -> Result<(), AtlasError> {
        self.advance_groundwater_day(atlas)
    }

    fn reconcile_basin_levels(&mut self, atlas: &PlanetAtlas) {
        for reservoir in &mut self.water.reservoirs {
            let (kind, id) = surface_reservoir_parts(reservoir.id);
            let total_hu = reservoir
                .coarse
                .water_hu
                .saturating_add(reservoir.committed.water_hu);
            let volume_units = total_hu / HYDRO_UNITS_PER_VISIBLE_LEVEL;
            let curve = match kind {
                SurfaceReservoirKind::Ocean => atlas
                    .hydrology
                    .oceans
                    .iter()
                    .find(|record| u32::from(record.id) == id)
                    .map(|record| record.volume_elevation_curve.as_slice()),
                SurfaceReservoirKind::Lake => atlas
                    .hydrology
                    .lakes
                    .iter()
                    .find(|record| record.id == id)
                    .map(|record| record.volume_elevation_curve.as_slice()),
                _ => None,
            };
            if let Some(curve) = curve {
                reservoir.level_milliblocks = storage_level_milliblocks(curve, volume_units);
            }
        }
    }

    fn precipitate_terminal_lake_salt(&mut self, atlas: &PlanetAtlas) {
        let mut total_precipitated = 0u64;
        for lake in &atlas.hydrology.lakes {
            if !matches!(
                lake.class,
                LakeClass::TerminalFresh | LakeClass::SalineTerminal | LakeClass::SeasonalPlaya
            ) {
                continue;
            }
            let id = surface_reservoir_id(SurfaceReservoirKind::Lake, lake.id);
            let Some(reservoir) = self.water.reservoir_mut(id) else {
                continue;
            };
            let saturation = reservoir.coarse.water_hu.saturating_mul(240);
            let excess = reservoir.coarse.salt_mass.saturating_sub(saturation);
            if excess == 0 {
                continue;
            }
            let precipitated = (excess / 64).max(1);
            reservoir.coarse.salt_mass -= precipitated;
            total_precipitated = total_precipitated.saturating_add(precipitated);
        }
        self.water.ledger.precipitated_salt_mass = self
            .water
            .ledger
            .precipitated_salt_mass
            .saturating_add(total_precipitated);
    }

    pub fn complete_hour(
        &mut self,
        atlas: &PlanetAtlas,
        day: f64,
        climate_hour: u64,
        ire: f32,
    ) -> Result<WeatherStepReport, AtlasError> {
        if let Some(failed) = self.failed_hour {
            return Err(AtlasError::Corrupt(format!(
                "weather hour {failed} is latched failed"
            )));
        }
        self.begin_hour(climate_hour);
        loop {
            match self.advance_slice(atlas, day, self.cells.cells.len(), |_| ire) {
                Ok(Some(report)) => return Ok(report),
                Ok(None) => {}
                Err(error) => {
                    self.abort_failed_hour();
                    return Err(error);
                }
            }
        }
    }

    pub fn sample(
        &self,
        atlas: &PlanetAtlas,
        surface: SurfacePos,
        day: f64,
        long_winter: bool,
    ) -> LocalWeatherSample {
        let pos = atlas.atlas_pos(surface);
        let index = pos.index(atlas.side());
        weather_sample(
            atlas.genesis.climate.values()[index],
            self.cells.cells.values()[index],
            day,
            long_winter,
        )
    }
}

fn hydrology_surface_id(cell: HydrologyCell) -> Option<u64> {
    if cell.ocean_basin_id != 0 {
        Some(surface_reservoir_id(
            SurfaceReservoirKind::Ocean,
            u32::from(cell.ocean_basin_id),
        ))
    } else if cell.lake_basin_id != 0 {
        Some(surface_reservoir_id(
            SurfaceReservoirKind::Lake,
            cell.lake_basin_id,
        ))
    } else if cell.river_id != 0 {
        Some(surface_reservoir_id(
            SurfaceReservoirKind::River,
            cell.river_id,
        ))
    } else {
        None
    }
}

/// Pressure-independent minimum river support from the shallow aquifer.
/// Stream order is the immutable hydrology distinction between perennial
/// channels and intermittent drainage lines; the transfer itself is bounded
/// by available groundwater and therefore stops under sustained drought.
pub(crate) fn take_river_baseflow(
    hydro: HydrologyCell,
    water: &mut WaterCell,
) -> Option<(u32, ReservoirMass)> {
    if hydro.river_id == 0 || hydro.stream_order < 3 {
        return None;
    }
    let parcel = water
        .groundwater
        .take((water.groundwater.water_hu / 4096).min(64));
    (parcel.water_hu != 0).then_some((hydro.river_id, parcel))
}

fn storage_level_milliblocks(curve: &[StoragePoint], volume_units: u64) -> i32 {
    let Some(first) = curve.first() else { return 0 };
    if volume_units <= first.volume_units {
        return (first.elevation * 1000.0).round() as i32;
    }
    for pair in curve.windows(2) {
        let [lower, upper] = pair else { unreachable!() };
        if volume_units <= upper.volume_units {
            let span = upper.volume_units.saturating_sub(lower.volume_units).max(1);
            let offset = volume_units.saturating_sub(lower.volume_units);
            let fraction = offset as f64 / span as f64;
            return ((f64::from(lower.elevation)
                + f64::from(upper.elevation - lower.elevation) * fraction)
                * 1000.0)
                .round() as i32;
        }
    }
    (curve.last().expect("curve is nonempty").elevation * 1000.0).round() as i32
}

fn add_i16(destination: &mut i16, amount: i32) {
    *destination =
        (i32::from(*destination) + amount).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
}

pub fn seasonal_scalar(values: [f32; CLIMATE_SEASONS], day: f64) -> f32 {
    let position = (day.rem_euclid(f64::from(YEAR_DAYS)) - 18.0) / 36.0;
    let lower = position.floor();
    let fraction = (position - lower) as f32;
    let a = lower.rem_euclid(4.0) as usize;
    let b = (a + 1) % CLIMATE_SEASONS;
    values[a] + (values[b] - values[a]) * fraction
}

pub fn seasonal_vector(values: [[f32; 2]; CLIMATE_SEASONS], day: f64) -> [f32; 2] {
    [
        seasonal_scalar(values.map(|value| value[0]), day),
        seasonal_scalar(values.map(|value| value[1]), day),
    ]
}

pub fn weather_sample(
    climate: ClimateCell,
    dynamic: DynamicCell,
    day: f64,
    long_winter: bool,
) -> LocalWeatherSample {
    let long_winter_anomaly = if long_winter { -18.0 } else { 0.0 };
    let temperature = seasonal_scalar(climate.seasonal_temperature, day)
        + f32::from(dynamic.weather_temperature_anomaly) / 100.0
        + long_winter_anomaly;
    let kind = if dynamic.precipitation_rate > 0 && dynamic.storm_energy > 24_000 {
        LocalWeather::Storm
    } else if dynamic.precipitation_rate > 0 {
        LocalWeather::Precipitation
    } else if dynamic.cloud_water > (climate.mean_atmospheric_moisture * 10.0).max(24.0) as u32 {
        LocalWeather::Overcast
    } else {
        LocalWeather::Clear
    };
    let precipitation = if !kind.precipitating() {
        PrecipitationForm::None
    } else if temperature <= 0.0 {
        PrecipitationForm::Snow
    } else {
        PrecipitationForm::Rain
    };
    let normal_wind = seasonal_vector(climate.seasonal_wind, day);
    LocalWeatherSample {
        kind,
        precipitation,
        temperature_c: temperature,
        pressure_anomaly: dynamic.pressure_anomaly,
        vapor: dynamic.atmospheric_vapor,
        cloud_water: dynamic.cloud_water,
        storm_energy: dynamic.storm_energy,
        precipitation_units: dynamic.precipitation_rate,
        wind: [
            normal_wind[0] + f32::from(dynamic.wind_anomaly[0]) / 16_384.0,
            normal_wind[1] + f32::from(dynamic.wind_anomaly[1]) / 16_384.0,
        ],
    }
}

pub fn dynamic_water_total(dynamic: &DynamicLayers) -> i128 {
    dynamic.cells.values().iter().fold(0i128, |total, cell| {
        total + i128::from(cell.atmospheric_vapor) + i128::from(cell.cloud_water)
    })
}

impl PlanetAtlas {
    pub fn climate_downstream(&self, pos: AtlasPos, day: f64) -> AtlasPos {
        let climate = self
            .genesis
            .climate
            .get(pos)
            .expect("validated atlas climate query");
        downstream_neighbor(
            pos,
            self.side(),
            chart_vector(
                pos,
                self.side(),
                seasonal_vector(climate.seasonal_wind, day),
            ),
        )
    }
}
