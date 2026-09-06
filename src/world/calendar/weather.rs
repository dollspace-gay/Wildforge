//! Weather calendar transaction coordination.

use crate::world::RegionCell;
use crate::world::World;

impl World {
    /// Claim landed precipitation for materialization by the voxel water
    /// cycle. Atlas worlds debit the exact coarse runoff/snow reserve; small
    /// atlas-free fixtures retain their synthetic precipitation behavior.
    pub(in crate::world) fn claim_precipitation_transfer(
        &mut self,
        pos: crate::planet::SurfacePos,
        form: crate::planet_atlas::PrecipitationForm,
        requested: u32,
    ) -> u32 {
        if let Some(atlas) = &self.planet_atlas {
            let atlas_pos = atlas.atlas_pos(pos);
            return self.weather_state.live_mut().map_or(0, |weather| {
                weather.withdraw_water_cycle_transfer(atlas_pos, form, requested)
            });
        }
        let sample = self.weather_at_surface(pos);
        if sample.precipitation == form {
            requested
        } else {
            0
        }
    }

    /// Development/capture override. Water is only moved between vapor and
    /// cloud; even a forced storm cannot mint atmospheric mass.
    pub fn force_local_weather(&mut self, requested: &str) {
        self.weather_state.force_local_weather(requested);
    }

    /// Slice one authoritative climate-hour update. A production pass is
    /// spread over roughly three seconds of ordinary 30 Hz server ticks.
    pub fn tick_planetary_weather(
        &mut self,
        budget: usize,
    ) -> Result<Option<crate::planet_atlas::WeatherStepReport>, crate::planet_atlas::AtlasError>
    {
        let Some(atlas) = self.planet_atlas.clone() else {
            return Ok(None);
        };
        let dross_completed = self
            .arcane_geography
            .as_ref()
            .map(|geography| geography.dynamic.dross_state.completed_steps);
        let day = self.calendar_state.clock() / f64::from(crate::server::DAY_LENGTH);
        let global_ire = self.ire;
        let regional_ire = &self.regional_ire;
        let report = self
            .weather_state
            .advance(&atlas, day, budget, dross_completed, |pos| {
                let center = pos.center(atlas.side());
                let surface = crate::planet::SurfacePos::new(
                    center.face,
                    center
                        .u
                        .floor()
                        .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1))
                        as u16,
                    center
                        .v
                        .floor()
                        .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1))
                        as u16,
                )
                .expect("atlas center is a canonical surface position");
                (global_ire
                    + regional_ire
                        .get(&RegionCell::from_surface(surface))
                        .copied()
                        .unwrap_or(0.0)
                        * 3.0)
                    .clamp(0.0, 100.0)
            });
        let report = report?;
        if report.is_some() {
            self.apply_loaded_water_inboxes();
            // Springs and changing shorelines can touch hundreds of loaded
            // water cells in one climate hour. Relighting after every cell
            // made the window appear permanently frozen while the same
            // connected chunks were rebuilt over and over. Preserve all
            // ordinary mutation behavior, but settle their shared light field
            // once after the complete water-cycle transaction.
            self.edit_batch(|world| {
                world.reconcile_loaded_springs();
                world.reconcile_loaded_shores();
            });
        }
        Ok(report)
    }

    pub(in crate::world) fn apply_loaded_water_inboxes(&mut self) {
        let (Some(atlas), Some(weather)) = (&self.planet_atlas, self.weather_state.live_mut())
        else {
            return;
        };
        // A sliced climate hour owns a second water-cell grid. Moving a flux
        // inbox into the live grid after that cell was processed would be
        // discarded by the final swap (the production cold-streaming loss).
        // Keep the parcel in its explicit inbox until the transaction
        // completes; `tick_planetary_weather` calls us immediately afterward.
        if weather.is_updating() {
            return;
        }
        let loaded = self
            .chunks
            .keys()
            .map(|chunk| {
                crate::planet_atlas::AtlasPos::from_surface(chunk.block_origin(), atlas.side())
            })
            .collect::<std::collections::BTreeSet<_>>();
        let mut index = 0;
        while index < weather.water.inboxes.len() {
            if !loaded.contains(&weather.water.inboxes[index].pos) {
                index += 1;
                continue;
            }
            let inbox = weather.water.inboxes.remove(index);
            let cell_index = inbox.pos.index(weather.water.cells.side());
            weather.water.cells.values_mut()[cell_index]
                .runoff
                .add_assign(inbox.mass)
                .expect("loaded flux inbox fits runoff reservoir");
        }
    }
}
