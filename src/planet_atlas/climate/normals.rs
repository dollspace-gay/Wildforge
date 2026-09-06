//! Generate immutable seasonal climate normals before dynamic weather exists.

use super::circulation::{circulation_wind, direction_to_lower_distance, ocean_distances};
use super::moisture::{SeasonalMoistureInputs, solve_seasonal_moisture};
use super::solar::{daily_mean_insolation, geographic_basis, solar_declination};
use super::transport::{chart_components, chart_vector};
use super::{AXIAL_TILT_DEGREES, CLIMATE_SEASONS, ClimateSolveReport, SEASON_MID_DAYS};
use crate::chunk::SEA_LEVEL;
use crate::planet_atlas::{
    AtlasError, AtlasGrid, AtlasPos, CancellationToken, ClimateCell, GeometryCell, TerrainCell,
    unit_noise,
};
use glam::DVec3;
use noise::Perlin;

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
