//! Seasonal moisture relaxation and its explicit convergence/budget result.

use super::transport::{chart_vector, transport_stencil};
use super::{CLIMATE_CONVERGENCE_TOLERANCE, CLIMATE_MAX_ITERATIONS, CLIMATE_SEASONS};
use crate::chunk::SEA_LEVEL;
use crate::planet_atlas::{
    AtlasError, AtlasGrid, AtlasPos, CancellationToken, GeometryCell, TerrainCell,
};

#[derive(Clone, Copy)]
pub(super) struct SeasonalMoistureInputs<'a> {
    pub(super) side: u16,
    pub(super) geometry: &'a AtlasGrid<GeometryCell>,
    pub(super) terrain: &'a AtlasGrid<TerrainCell>,
    pub(super) winds: &'a [[[f32; 2]; CLIMATE_SEASONS]],
    pub(super) continentality: &'a [f32],
    pub(super) cancel: &'a CancellationToken,
}

pub(super) struct SeasonalMoistureSolution {
    pub(super) moisture: Vec<f32>,
    pub(super) precipitation: Vec<f32>,
    pub(super) iterations: u16,
    pub(super) residual: f64,
    pub(super) budget_error: f64,
}

pub(super) fn solve_seasonal_moisture(
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
