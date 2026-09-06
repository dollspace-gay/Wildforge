//! Commit one complete weather grid, reconcile stores, and prove conservation.

use super::{PlanetaryWeather, WeatherStepReport, dynamic_water_total};
use crate::planet_atlas::{AtlasError, PlanetAtlas, ReservoirMass, SurfaceReservoirState};

impl PlanetaryWeather {
    pub(super) fn plan_surface_evaporation(&mut self, id: u64, requested_hu: u64) -> ReservoirMass {
        let already = self
            .surface_fluxes
            .get(&id)
            .map_or(0, |(debit, _)| debit.water_hu);
        let available = self
            .water
            .reservoirs
            .binary_search_by_key(&id, |reservoir| reservoir.id)
            .ok()
            .map_or(0, |index| {
                self.water.reservoirs[index]
                    .coarse
                    .water_hu
                    .saturating_sub(already)
            });
        let mass = ReservoirMass::fresh(requested_hu.min(available));
        let entry = self.surface_fluxes.entry(id).or_default();
        entry.0.water_hu = entry.0.water_hu.saturating_add(mass.water_hu);
        mass
    }

    pub(super) fn plan_surface_credit(&mut self, id: u64, mass: ReservoirMass) {
        let entry = self.surface_fluxes.entry(id).or_default();
        entry.1.add_assign(mass).expect("surface flux fits u64");
    }

    pub(super) fn finish_hour(
        &mut self,
        atlas: &PlanetAtlas,
        climate_hour: u64,
    ) -> Result<WeatherStepReport, AtlasError> {
        for (cell, inbound) in self
            .water_scratch
            .values_mut()
            .iter_mut()
            .zip(&self.water_inbound)
        {
            cell.runoff.add_assign(*inbound)?;
        }
        for (id, (debit, credit)) in std::mem::take(&mut self.surface_fluxes) {
            if self.water.reservoir_mut(id).is_none() {
                let insertion = self
                    .water
                    .reservoirs
                    .binary_search_by_key(&id, |reservoir| reservoir.id)
                    .unwrap_err();
                self.water.reservoirs.insert(
                    insertion,
                    SurfaceReservoirState {
                        id,
                        name: format!("dynamic surface reservoir {id}"),
                        coarse: ReservoirMass::default(),
                        committed: ReservoirMass::default(),
                        initial_total_hu: 0,
                        level_milliblocks: 0,
                    },
                );
            }
            let reservoir = self
                .water
                .reservoir_mut(id)
                .expect("surface reservoir exists");
            let removed = reservoir.coarse.take_fresh_water(debit.water_hu);
            if removed.water_hu != debit.water_hu {
                return Err(AtlasError::Corrupt(format!(
                    "surface reservoir {id} could not honor a planned debit"
                )));
            }
            reservoir.coarse.add_assign(credit)?;
        }
        std::mem::swap(&mut self.cells.cells, &mut self.scratch);
        std::mem::swap(&mut self.water.cells, &mut self.water_scratch);
        self.cells_swapped = true;
        self.precipitate_terminal_lake_salt(atlas);
        self.water.completed_surface_hours = self.water.completed_surface_hours.saturating_add(1);
        if self.water.completed_surface_hours.is_multiple_of(24) {
            self.advance_groundwater_day(atlas)?;
        }
        self.reconcile_basin_levels(atlas);
        let end_audit = self
            .water
            .audit(ReservoirMass::fresh(dynamic_water_total(&self.cells) as u64));
        let end_total = i128::from(end_audit.current_water_hu);
        let report = WeatherStepReport {
            climate_hour,
            processed_cells: self.cells.cells.len(),
            evaporation_units: self.evaporation_units,
            condensation_units: self.condensation_units,
            precipitation_units: self.precipitation_units,
            water_cycle_outflow_units: self.pending_water_cycle_outflow,
            atmospheric_water_before: self.start_total,
            atmospheric_water_after: end_total,
            unexplained_water_drift: end_total - self.start_total,
        };
        if report.unexplained_water_drift != 0
            || end_audit.unexplained_water_delta_hu != 0
            || end_audit.unexplained_salt_delta != 0
        {
            return Err(AtlasError::Corrupt(format!(
                "planetary water drift in hour {climate_hour}: step {}, ledger {} HU / {} salt",
                report.unexplained_water_drift,
                end_audit.unexplained_water_delta_hu,
                end_audit.unexplained_salt_delta,
            )));
        }
        self.active_hour = None;
        self.checkpoint = None;
        self.cells_swapped = false;
        self.cursor = 0;
        self.pending_water_cycle_outflow = 0;
        self.active_water_cycle_outflow = 0;
        self.completed_hours = self.completed_hours.saturating_add(1);
        self.cells.completed_climate_hours = self.completed_hours;
        self.last_runoff_routes = std::mem::take(&mut self.active_runoff_routes);
        self.last_report = report;
        Ok(report)
    }
}
