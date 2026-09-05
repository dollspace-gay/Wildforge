//! Pure astronomical observations shared by authoritative and replica clocks.

use crate::planet::{SurfacePos, surface_to_unit};
use super::{LUNAR_DAYS, SEASON_DAYS};

#[derive(Clone, Copy)]
pub(crate) struct CalendarView {
    day: u32,
    clock: f64,
    long_winter: bool,
}

impl CalendarView {
    pub(crate) fn new(day: u32, clock: f64, long_winter: bool) -> Self {
        Self { day, clock, long_winter }
    }

    pub(crate) fn sun_direction(self) -> glam::DVec3 {
        let day = self.clock / f64::from(crate::server::DAY_LENGTH);
        crate::planet_atlas::solar_direction(day, day.fract())
    }

    pub(crate) fn latitude(position: SurfacePos) -> f64 {
        crate::planet_atlas::latitude_longitude(surface_to_unit(position.center())).0
    }

    pub(crate) fn season_at(self, position: SurfacePos) -> usize {
        if self.long_winter { return 3; }
        crate::planet_atlas::local_season(self.day, Self::latitude(position))
    }

    pub(crate) fn daylight_at(self, position: SurfacePos) -> f32 {
        let elevation = self.sun_direction().dot(surface_to_unit(position.center())) as f32;
        (elevation * 2.5 + 0.5).clamp(0.12, 1.0)
    }

    pub(crate) fn season_progress(self) -> f32 {
        (self.day % SEASON_DAYS) as f32 / SEASON_DAYS as f32
    }

    pub(crate) fn moon_cycle(self) -> f32 { (self.day % LUNAR_DAYS) as f32 / LUNAR_DAYS as f32 }

    pub(crate) fn moon_illumination(self) -> f32 {
        0.5 * (1.0 - (self.moon_cycle() * std::f32::consts::TAU).cos())
    }
}

pub(super) fn ire_tier(ire: f32) -> usize {
    match ire {
        value if value < 20.0 => 0,
        value if value < 50.0 => 1,
        value if value < 80.0 => 2,
        _ => 3,
    }
}

pub(super) fn seasonal_want(season: usize) -> (usize, &'static str) {
    match season {
        0 => (0, "The wild stirs. Seeds and saplings are welcome."),
        1 => (1, "The wild thirsts. Carried water is welcome."),
        2 => (2, "The wild gathers. First fruits are welcome."),
        _ => (3, "The wild hungers. Food is welcome."),
    }
}
