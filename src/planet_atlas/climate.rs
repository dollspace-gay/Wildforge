//! Climate normals and one owned, transactional whole-planet weather state.

use super::{
    AtlasError, AtlasGrid, AtlasPos, DynamicCell, DynamicLayers, FluxInbox, PlanetAtlas,
    ReservoirMass, SparseAquiferState, SpringState, SurfaceReservoirState, WaterCell,
    WaterCycleState, WaterLedger,
};
use crate::planet::SurfacePos;
#[cfg(test)]
pub(crate) use basins::take_river_baseflow;
use std::collections::BTreeMap;
#[cfg(test)]
pub(crate) use transport::chart_vector;

mod solar;
pub use solar::{
    daily_mean_insolation, day_length_hours, latitude_longitude, local_season, prime_meridian,
    rotation_axis, solar_declination, solar_direction,
};
mod circulation;
mod moisture;
mod normals;
mod transport;
pub(crate) use normals::generate_climate;
mod weather_types;
pub use weather_types::{
    LocalWeather, LocalWeatherSample, PrecipitationForm, RunoffTransport, WeatherStepReport,
};
mod basins;
mod custody;
mod ecology;
mod groundwater;
mod hour_cell;
mod hour_commit;
mod industrial;
mod sampling;
pub use sampling::{dynamic_water_total, seasonal_scalar, seasonal_vector, weather_sample};

pub const CLIMATE_SEASONS: usize = 4;
pub const YEAR_DAYS: u32 = 144;
pub const AXIAL_TILT_DEGREES: f64 = 23.5;
pub const ROTATION_AXIS: [f64; 3] = [0.0, 1.0, 0.0];
pub const PRIME_MERIDIAN: [f64; 3] = [0.0, 0.0, 1.0];
/// Long advective loops on the 256×256 production faces need substantially
/// more relaxation than tiny fixtures. Keep the physical tolerance fixed and
/// allow the full planet to prove convergence rather than accepting a looser
/// answer merely because the grid is larger.
pub const CLIMATE_MAX_ITERATIONS: u16 = 256;
pub const CLIMATE_CONVERGENCE_TOLERANCE: f64 = 0.02;

const SEASON_MID_DAYS: [f64; CLIMATE_SEASONS] = [18.0, 54.0, 90.0, 126.0];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClimateSolveReport {
    pub iterations: [u16; CLIMATE_SEASONS],
    pub max_residual: f32,
    pub max_moisture_budget_error: f64,
}

#[derive(Clone, Debug)]
struct WeatherCheckpoint {
    aquifers: Vec<SparseAquiferState>,
    reservoirs: Vec<SurfaceReservoirState>,
    springs: Vec<SpringState>,
    inboxes: Vec<FluxInbox>,
    ledger: WaterLedger,
    completed_surface_hours: u64,
    completed_groundwater_days: u64,
}

/// Mutable whole-planet weather state. A climate-hour pass is built into a
/// second grid and sliced across ordinary server ticks; the accepted state is
/// swapped atomically only after every cell has contributed.
#[derive(Clone, Debug)]
pub struct PlanetaryWeather {
    pub cells: DynamicLayers,
    pub water: WaterCycleState,
    scratch: AtlasGrid<DynamicCell>,
    water_scratch: AtlasGrid<WaterCell>,
    water_inbound: Vec<ReservoirMass>,
    surface_fluxes: BTreeMap<u64, (ReservoirMass, ReservoirMass)>,
    cursor: usize,
    active_hour: Option<u64>,
    start_total: i128,
    evaporation_units: u64,
    condensation_units: u64,
    precipitation_units: u64,
    pending_water_cycle_outflow: u64,
    active_water_cycle_outflow: u64,
    pub completed_hours: u64,
    pub last_report: WeatherStepReport,
    active_runoff_routes: Vec<RunoffTransport>,
    last_runoff_routes: Vec<RunoffTransport>,
    checkpoint: Option<WeatherCheckpoint>,
    cells_swapped: bool,
    failed_hour: Option<u64>,
}

impl PlanetaryWeather {
    pub fn new(cells: DynamicLayers, water: WaterCycleState) -> Self {
        let scratch = cells.cells.filled_like(DynamicCell::default());
        let water_scratch = water.cells.clone();
        let water_inbound = vec![ReservoirMass::default(); cells.cells.len()];
        let completed_hours = cells.completed_climate_hours;
        Self {
            cells,
            water,
            scratch,
            water_scratch,
            water_inbound,
            surface_fluxes: BTreeMap::new(),
            cursor: 0,
            active_hour: None,
            start_total: 0,
            evaporation_units: 0,
            condensation_units: 0,
            precipitation_units: 0,
            pending_water_cycle_outflow: 0,
            active_water_cycle_outflow: 0,
            completed_hours,
            last_report: WeatherStepReport::default(),
            active_runoff_routes: Vec::new(),
            last_runoff_routes: Vec::new(),
            checkpoint: None,
            cells_swapped: false,
            failed_hour: None,
        }
    }

    pub fn water_audit(&self) -> crate::planet_atlas::WaterAudit {
        self.water
            .audit(ReservoirMass::fresh(dynamic_water_total(&self.cells) as u64))
    }

    pub fn is_updating(&self) -> bool {
        self.active_hour.is_some()
    }

    pub fn last_runoff_routes(&self) -> &[RunoffTransport] {
        &self.last_runoff_routes
    }

    pub fn begin_hour(&mut self, climate_hour: u64) {
        if self.active_hour.is_some() || self.failed_hour.is_some() {
            return;
        }
        self.checkpoint = Some(WeatherCheckpoint {
            aquifers: self.water.aquifers.clone(),
            reservoirs: self.water.reservoirs.clone(),
            springs: self.water.springs.clone(),
            inboxes: self.water.inboxes.clone(),
            ledger: self.water.ledger,
            completed_surface_hours: self.water.completed_surface_hours,
            completed_groundwater_days: self.water.completed_groundwater_days,
        });
        self.cells_swapped = false;
        self.scratch.values_mut().fill(DynamicCell::default());
        self.water_scratch
            .values_mut()
            .clone_from_slice(self.water.cells.values());
        self.water_inbound.fill(ReservoirMass::default());
        self.surface_fluxes.clear();
        self.active_runoff_routes.clear();
        self.cursor = 0;
        self.active_hour = Some(climate_hour);
        self.start_total = self
            .water
            .audit(ReservoirMass::fresh(dynamic_water_total(&self.cells) as u64))
            .current_water_hu as i128;
        self.evaporation_units = 0;
        self.condensation_units = 0;
        self.precipitation_units = 0;
        self.active_water_cycle_outflow = 0;
    }

    /// Roll a failed sliced hour back to its exact pre-hour state and latch
    /// the failure so the server does not retry the same transaction every
    /// tick. The next process restart may retry after code/operator repair;
    /// no corrupt dynamic state is persisted in the meantime.
    pub fn abort_failed_hour(&mut self) {
        let Some(hour) = self.active_hour.take() else {
            return;
        };
        if self.cells_swapped {
            std::mem::swap(&mut self.cells.cells, &mut self.scratch);
            std::mem::swap(&mut self.water.cells, &mut self.water_scratch);
        }
        if let Some(checkpoint) = self.checkpoint.take() {
            self.water.aquifers = checkpoint.aquifers;
            self.water.reservoirs = checkpoint.reservoirs;
            self.water.springs = checkpoint.springs;
            self.water.inboxes = checkpoint.inboxes;
            self.water.ledger = checkpoint.ledger;
            self.water.completed_surface_hours = checkpoint.completed_surface_hours;
            self.water.completed_groundwater_days = checkpoint.completed_groundwater_days;
        }
        self.cursor = 0;
        self.surface_fluxes.clear();
        self.active_runoff_routes.clear();
        self.water_inbound.fill(ReservoirMass::default());
        self.pending_water_cycle_outflow = 0;
        self.active_water_cycle_outflow = 0;
        self.cells_swapped = false;
        self.failed_hour = Some(hour);
    }

    /// Advance at most `budget` cells. The closure returns local Ire on the
    /// familiar 0..=100 scale; it can strengthen instability but never enters
    /// any water transfer equation.
    pub fn advance_slice(
        &mut self,
        atlas: &PlanetAtlas,
        day: f64,
        budget: usize,
        mut local_ire: impl FnMut(AtlasPos) -> f32,
    ) -> Result<Option<WeatherStepReport>, AtlasError> {
        let Some(climate_hour) = self.active_hour else {
            return Ok(None);
        };
        if self.cells.cells.side() != atlas.side()
            || self.cells.cells.len() != atlas.dynamic.cells.len()
        {
            return Err(AtlasError::Corrupt(
                "weather state dimensions do not match immutable climate".into(),
            ));
        }
        let end = self
            .cursor
            .saturating_add(budget)
            .min(self.cells.cells.len());
        for index in self.cursor..end {
            self.advance_cell(atlas, day, climate_hour, index, &mut local_ire)?;
        }
        self.cursor = end;
        if self.cursor < self.cells.cells.len() {
            return Ok(None);
        }

        self.finish_hour(atlas, climate_hour).map(Some)
    }

    pub fn complete_hour(
        &mut self,
        atlas: &PlanetAtlas,
        day: f64,
        climate_hour: u64,
        ire: f32,
    ) -> Result<WeatherStepReport, AtlasError> {
        if let Some(failed) = self.failed_hour {
            return Err(AtlasError::Corrupt(format!(
                "weather hour {failed} is latched failed"
            )));
        }
        self.begin_hour(climate_hour);
        loop {
            match self.advance_slice(atlas, day, self.cells.cells.len(), |_| ire) {
                Ok(Some(report)) => return Ok(report),
                Ok(None) => {}
                Err(error) => {
                    self.abort_failed_hour();
                    return Err(error);
                }
            }
        }
    }

    pub fn sample(
        &self,
        atlas: &PlanetAtlas,
        surface: SurfacePos,
        day: f64,
        long_winter: bool,
    ) -> LocalWeatherSample {
        let pos = atlas.atlas_pos(surface);
        let index = pos.index(atlas.side());
        weather_sample(
            atlas.genesis.climate.values()[index],
            self.cells.cells.values()[index],
            day,
            long_winter,
        )
    }
}
