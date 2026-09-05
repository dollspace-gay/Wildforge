//! Seasonal interpolation and read-only weather observations.

use crate::planet_atlas::{AtlasPos, ClimateCell, DynamicCell, DynamicLayers, PlanetAtlas};
use super::{CLIMATE_SEASONS, YEAR_DAYS, LocalWeather, PrecipitationForm, LocalWeatherSample};
use super::transport::chart_vector;
use super::circulation::downstream_neighbor;

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
