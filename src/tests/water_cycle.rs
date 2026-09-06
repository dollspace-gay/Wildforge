use crate::planet::{Direction4, Face};
use crate::planet_atlas::{
    AquiferLayer, AtlasPos, ChunkWaterCommitment, HydrologyCell, PlanetAtlas, PlanetaryWeather,
    PrecipitationForm, ReservoirMass, SpringState, SurfaceReservoirKind, WaterCell, WaterClass,
    YEAR_DAYS, dynamic_water_total, surface_reservoir_parts, take_river_baseflow,
};

use super::{base_reg, tmp_dir};

fn atmosphere(weather: &PlanetaryWeather) -> ReservoirMass {
    ReservoirMass::fresh(dynamic_water_total(&weather.cells) as u64)
}

fn process_peak_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmHWM:")?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    })
}

mod basins;
mod groundwater;
mod persistence;
mod qualification;
mod transfers;
mod voxel_custody;
mod weather;
