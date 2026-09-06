//! Environmental observations with explicit host-only information boundaries.

use std::sync::Arc;

use super::{Source, WorldView};
use crate::planet::SurfacePos;
use crate::planet_atlas::{LocalWeatherSample, PlanetAtlas, PrecipitationForm};
use crate::world::TerrainRead;
use crate::world::calendar_view::{self, CalendarView};

impl<'a> WorldView<'a> {
    fn calendar(&self) -> CalendarView {
        match self.source {
            Source::Authority(world) => world.calendar_view(),
            Source::Replica(world) => world.calendar_view(),
        }
    }
    pub(crate) fn clock(&self) -> f64 {
        match self.source {
            Source::Authority(world) => world.clock(),
            Source::Replica(world) => world.clock(),
        }
    }
    pub(crate) fn script_arcane_estimate(&self, position: SurfacePos) -> [u8; 2] {
        match self.source {
            Source::Authority(world) => world
                .planet_atlas()
                .map(|atlas| world.arcane_cue_at(atlas.atlas_pos(position)))
                .unwrap_or([0; 2]),
            Source::Replica(_) => [0; 2],
        }
    }
    pub(crate) fn day(&self) -> u32 {
        match self.source {
            Source::Authority(world) => world.day(),
            Source::Replica(world) => world.day(),
        }
    }
    pub(crate) fn seed(&self) -> u32 {
        match self.source {
            Source::Authority(world) => world.seed,
            Source::Replica(world) => world.seed(),
        }
    }
    pub(crate) fn mode(&self) -> &'a str {
        match self.source {
            Source::Authority(world) => &world.mode,
            Source::Replica(world) => world.mode(),
        }
    }
    pub(crate) fn ruleset(&self) -> crate::ruleset::Ruleset {
        self.registry().ruleset_for(self.mode())
    }
    pub(crate) fn ire(&self) -> f32 {
        match self.source {
            Source::Authority(world) => world.ire,
            Source::Replica(world) => world.ire(),
        }
    }
    pub(crate) fn ire_tier(&self) -> usize {
        calendar_view::ire_tier(self.ire())
    }
    pub(crate) fn regional_ire_at_surface(&self, position: SurfacePos) -> f32 {
        match self.source {
            Source::Authority(world) => world.regional_ire_at_surface(position),
            // Regional reciprocity ledgers are not sent to guests.
            Source::Replica(_) => 0.0,
        }
    }
    pub(crate) fn ire_tier_at_surface(&self, position: SurfacePos) -> usize {
        calendar_view::ire_tier(
            (self.ire() + self.regional_ire_at_surface(position) * 3.0).clamp(0.0, 100.0),
        )
    }
    pub(crate) fn sun_direction(&self) -> glam::DVec3 {
        self.calendar().sun_direction()
    }
    pub(crate) fn daylight_at_surface(&self, position: SurfacePos) -> f32 {
        self.calendar().daylight_at(position)
    }
    pub(crate) fn season_at_surface(&self, position: SurfacePos) -> usize {
        self.calendar().season_at(position)
    }
    pub(crate) fn season_progress(&self) -> f32 {
        self.calendar().season_progress()
    }
    pub(crate) fn moon_illumination(&self) -> f32 {
        self.calendar().moon_illumination()
    }
    pub(crate) fn season_want_at_surface(&self, position: SurfacePos) -> (usize, &'static str) {
        calendar_view::seasonal_want(self.season_at_surface(position))
    }
    pub(crate) fn weather_at_surface(&self, position: SurfacePos) -> LocalWeatherSample {
        match self.source {
            Source::Authority(world) => world.weather_at_surface(position),
            Source::Replica(world) => world.weather_at_surface(position),
        }
    }
    pub(crate) fn rains_at_surface(&self, position: SurfacePos) -> bool {
        self.weather_at_surface(position).kind.precipitating()
    }
    pub(crate) fn snows_at_surface(&self, position: SurfacePos) -> bool {
        self.weather_at_surface(position).precipitation == PrecipitationForm::Snow
    }
    pub(crate) fn biome_here_at(&self, position: SurfacePos) -> crate::worldgen::Biome {
        match self.source {
            Source::Authority(world) => world.biome_here_at(position),
            Source::Replica(world) => world.biome_here_at(position),
        }
    }
    pub(crate) fn prospect_at(&self, position: SurfacePos) -> crate::worldgen::ProspectReading {
        match self.source {
            Source::Authority(world) => world.generator.prospect_at(position),
            Source::Replica(world) => world.prospect_at(position),
        }
    }

    pub(crate) fn heart_at_surface(&self, position: SurfacePos) -> Option<crate::world::Heart> {
        match self.source {
            Source::Authority(world) => world.heart_at_surface(position),
            // Living-heart state is not part of the guest protocol.
            Source::Replica(_) => None,
        }
    }

    pub(crate) fn planet_atlas(&self) -> Option<Arc<PlanetAtlas>> {
        match self.source {
            Source::Authority(world) => world.planet_atlas(),
            // Welcome does not contain a planet atlas or hidden geography state.
            Source::Replica(_) => None,
        }
    }
    pub(crate) fn remote_arcane_cue(&self) -> [u8; 2] {
        match self.source {
            Source::Authority(_) => [0; 2],
            Source::Replica(world) => world.remote_arcane_cue(),
        }
    }
    pub(crate) fn remote_arcane_dominant(&self) -> u8 {
        match self.source {
            Source::Authority(_) => 0,
            Source::Replica(world) => world.remote_arcane_dominant(),
        }
    }
    pub(crate) fn perceived_arcane_ecology_at(
        &self,
        surface: SurfacePos,
        radius: f32,
    ) -> Option<crate::arcane_ecology::EcologyObservation> {
        match self.source {
            Source::Authority(world) => world.perceived_arcane_ecology_at(surface, radius),
            Source::Replica(world) => world.arcane_ecology(),
        }
    }
}

impl WorldView<'_> {
    pub(crate) fn seed_bearing_at(&self, from: crate::planet::EntityPos) -> String {
        match self.source {
            Source::Authority(world) => world.seed_bearing_at(from),
            Source::Replica(world) => world.seed_bearing_at(from),
        }
    }
    pub(crate) fn heart_report_at(&self, position: SurfacePos) -> String {
        match self.source {
            Source::Authority(world) => world.heart_report_at(position),
            Source::Replica(world) => world.heart_report_at(position),
        }
    }
    pub(crate) fn soil_failure_at(
        &self,
        position: crate::planet::BlockPos,
    ) -> Option<&'static str> {
        match self.source {
            Source::Authority(world) => world.soil_failure_at(position),
            // No atlas/weather water books are sent. The old guest used the
            // atlas-free moisture baseline of 1.0 and no habitat sample.
            Source::Replica(world) => crate::world::soil::soil_failure(
                world.get_soil_salinity_at(position),
                None,
                false,
                1.0,
            ),
        }
    }
}
