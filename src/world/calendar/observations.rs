//! Observations calendar transaction coordination.

use crate::world::World;

impl World {
    pub(in crate::world) fn calendar_view(&self) -> crate::world::calendar_view::CalendarView {
        self.calendar_state.view()
    }

    pub(super) fn orbital_day(&self) -> f64 {
        self.calendar_state.clock() / f64::from(crate::server::DAY_LENGTH)
    }

    #[cfg(test)]
    pub fn sun_direction(&self) -> glam::DVec3 {
        self.calendar_view().sun_direction()
    }

    pub fn latitude_at_surface(&self, pos: crate::planet::SurfacePos) -> f64 {
        crate::world::calendar_view::CalendarView::latitude(pos)
    }

    /// The local temperature for a particular orbital day. Offline crop
    /// reconciliation uses this instead of applying today's weather to every
    /// season the unloaded field missed.
    pub(crate) fn temperature_at_surface_on_day(
        &self,
        pos: crate::planet::SurfacePos,
        day: f64,
    ) -> f32 {
        if let (Some(atlas), Some(weather)) = (&self.planet_atlas, self.weather_state.live()) {
            return weather
                .sample(atlas, pos, day, self.calendar_state.long_winter())
                .temperature_c;
        }
        crate::climate::seasonal_temperature(self.generator.climate_at(pos).t, pos, day)
    }

    /// Local astronomical season. The Long Winter is a supernatural thermal
    /// anomaly, so it suppresses growth everywhere without freezing the orbit.
    pub fn season_at_surface(&self, pos: crate::planet::SurfacePos) -> usize {
        self.calendar_view().season_at(pos)
    }

    pub fn daylight_at_surface(&self, pos: crate::planet::SurfacePos) -> f32 {
        self.calendar_view().daylight_at(pos)
    }

    pub fn weather_at_surface(
        &self,
        pos: crate::planet::SurfacePos,
    ) -> crate::planet_atlas::LocalWeatherSample {
        if let (Some(atlas), Some(weather)) = (&self.planet_atlas, self.weather_state.live()) {
            return weather.sample(
                atlas,
                pos,
                self.orbital_day(),
                self.calendar_state.long_winter(),
            );
        }
        // Atlas-free fixtures and development worlds still need a physically
        // sane local temperature. The old fixed +14 C spring/summer offset
        // overheated the equator above crop tolerance and applied the same
        // seasonal swing at every latitude. Preserve the generator's broad
        // latitude field as the annual mean, then scale the orbital anomaly
        // by signed latitude: no equatorial season spike, opposite
        // hemispheres, strongest variation toward the poles.
        let temperature_c =
            self.temperature_at_surface_on_day(pos, f64::from(self.calendar_state.day()));
        crate::world::calendar_view::fallback_weather(
            temperature_c,
            self.weather_state.override_sample(),
        )
    }

    pub fn soil_moisture_at_surface(&self, pos: crate::planet::SurfacePos) -> f32 {
        let (Some(atlas), Some(weather)) = (&self.planet_atlas, self.weather_state.live()) else {
            return 1.0;
        };
        let atlas_pos = atlas.atlas_pos(pos);
        let index = atlas_pos.index(atlas.side());
        let baseline = (atlas.genesis.climate.values()[index].mean_precipitation * 4.0).max(256.0)
            * crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL as f32;
        (weather.water.cells.values()[index].soil.water_hu as f32 / baseline).clamp(0.0, 1.5)
    }
}
