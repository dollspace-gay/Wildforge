//! Live authoritative atmosphere and climate update lifetime.
//!
//! PlanetaryWeather remains the owner of its conserved water. This component
//! owns its optional atlas-world lifetime, update admission/abort, and overrides.
//! World coordinates water-inbox materialization after a completed climate hour.

use crate::planet_atlas::{AtlasError, AtlasPos, LocalWeatherSample, PlanetAtlas, PlanetaryWeather, WeatherStepReport};

pub(super) struct WeatherState {
    live: Option<PlanetaryWeather>,
    override_sample: Option<LocalWeatherSample>,
}

impl WeatherState {
    pub(super) fn new(atlas: Option<&PlanetAtlas>) -> Self {
        Self { live: atlas.map(|atlas| PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone())), override_sample: None }
    }
    pub(super) fn live(&self) -> Option<&PlanetaryWeather> { self.live.as_ref() }
    pub(super) fn live_mut(&mut self) -> Option<&mut PlanetaryWeather> { self.live.as_mut() }
    pub(super) fn override_sample(&self) -> Option<LocalWeatherSample> { self.override_sample }

    pub(super) fn force_local_weather(&mut self, requested: &str) {
        let forced = super::calendar_view::forced_weather(requested);
        let requested = forced.kind;
        let Some(weather) = &mut self.live else {
            self.override_sample = Some(forced);
            return;
        };
        self.override_sample = None;
        for cell in weather.cells.cells.values_mut() {
            let total = cell.atmospheric_vapor.saturating_add(cell.cloud_water);
            match requested {
                crate::planet_atlas::LocalWeather::Clear => {
                    cell.atmospheric_vapor = total;
                    cell.cloud_water = 0;
                    cell.storm_energy = 0;
                    cell.precipitation_rate = 0;
                }
                crate::planet_atlas::LocalWeather::Overcast => {
                    cell.cloud_water = total / 3;
                    cell.atmospheric_vapor = total - cell.cloud_water;
                    cell.storm_energy = 8_000;
                    cell.precipitation_rate = 0;
                }
                crate::planet_atlas::LocalWeather::Precipitation => {
                    cell.cloud_water = total / 2;
                    cell.atmospheric_vapor = total - cell.cloud_water;
                    cell.storm_energy = 20_000;
                    cell.precipitation_rate = 1;
                }
                crate::planet_atlas::LocalWeather::Storm => {
                    cell.cloud_water = total * 2 / 3;
                    cell.atmospheric_vapor = total - cell.cloud_water;
                    cell.storm_energy = 52_000;
                    cell.precipitation_rate = 1;
                }
            }
        }
    }

    pub(super) fn advance(
        &mut self, atlas: &PlanetAtlas, day: f64, budget: usize,
        dross_completed: Option<u64>, ire_at: impl Fn(AtlasPos) -> f32,
    ) -> Result<Option<WeatherStepReport>, AtlasError> {
        let Some(weather) = self.live.as_mut() else {
            return Ok(None);
        };
        let target_hour = (day * 24.0).floor().max(0.0) as u64;
        let dross_needs_previous_routes = dross_completed
            .is_some_and(|completed| weather.completed_hours > completed.saturating_add(1));
        if !weather.is_updating()
            && !dross_needs_previous_routes
            && weather.completed_hours <= target_hour
        {
            weather.begin_hour(weather.completed_hours);
        }
        let report = weather.advance_slice(atlas, day, budget, ire_at);
        let report = match report {
            Ok(report) => report,
            Err(error) => {
                weather.abort_failed_hour();
                return Err(error);
            }
        };
        Ok(report)
    }
}
