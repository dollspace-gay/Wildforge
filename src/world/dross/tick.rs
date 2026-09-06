//! Tick dross transaction coordination.

use crate::dross::DrossAdvance;
use crate::dross::DrossConditions;
use crate::world::World;
use std::collections::BTreeSet;

impl World {
    /// Slice one planetary dross hour. Weather routing, heart mortality, and
    /// ecology are sampled from host-owned state; chunk residency never
    /// determines transport or manifestations.
    pub fn tick_dross(&mut self, budget: usize) -> std::io::Result<DrossAdvance> {
        self.import_pending_environmental_dross()
            .map_err(std::io::Error::other)?;
        let Some(atlas) = self.planet_atlas.as_ref().cloned() else {
            return Ok(DrossAdvance::default());
        };
        let target_hour = self
            .weather_state
            .live()
            .map_or(0, |weather| weather.completed_hours);
        let runoff_routes = self
            .weather_state
            .live()
            .map_or_else(Vec::new, |weather| weather.last_runoff_routes().to_vec());
        let mut living_hearts = atlas
            .biomes
            .countries
            .iter()
            .map(|country| country.id)
            .collect::<BTreeSet<_>>();
        for heart in self.hearts.values().filter(|heart| heart.stage == 0) {
            if let Some(country) = atlas.country_at(heart.pos.surface()) {
                living_hearts.remove(&country.id);
            }
        }
        let industrial_regions = self
            .alchemy_state
            .as_ref()
            .into_iter()
            .flat_map(|state| state.apparatus.keys())
            .map(|pos| atlas.atlas_pos(pos.surface()))
            .collect::<BTreeSet<_>>();
        let cave_regions = BTreeSet::new();
        let conditions = DrossConditions {
            runoff_routes: &runoff_routes,
            living_hearts: &living_hearts,
            long_winter: self.calendar_state.long_winter(),
        };
        let report =
            self.arcane_geography
                .as_mut()
                .map_or(Ok(DrossAdvance::default()), |geography| {
                    geography
                        .advance_dross_toward(&atlas, &self.reg, target_hour, budget, conditions)
                        .map_err(std::io::Error::other)
                })?;
        if let Some(hour) = report.completed_hour {
            self.manifest_one_dross_scar(&atlas, hour, &industrial_regions, &cave_regions)
                .map_err(std::io::Error::other)?;
            self.refresh_loaded_dross_scars();
        }
        Ok(report)
    }
}
