use crate::chunk::SEA_LEVEL;
use crate::planet::{Direction4, Face, SurfacePos};
use crate::planet_atlas::{
    AXIAL_TILT_DEGREES, AtlasPos, CLIMATE_CONVERGENCE_TOLERANCE, CancellationToken, LocalWeather,
    PlanetAtlas, PlanetaryWeather, PrecipitationForm, ReservoirMass, YEAR_DAYS,
    daily_mean_insolation, day_length_hours, dynamic_water_total, generate_climate, local_season,
    solar_direction,
};
use crate::world::World;
use crate::world::{ReplicaWorld, ReplicationTarget};
use std::sync::Arc;

use super::{base_reg, tmp_dir};

fn climate(seed: u32, side: u16) -> PlanetAtlas {
    PlanetAtlas::fixture(seed, side).expect("climate fixture")
}

fn average(values: impl Iterator<Item = f64>) -> f64 {
    let values: Vec<f64> = values.collect();
    values.iter().sum::<f64>() / values.len().max(1) as f64
}

mod astronomy;
mod normals;
mod persistence;
mod replication;
mod weather;
