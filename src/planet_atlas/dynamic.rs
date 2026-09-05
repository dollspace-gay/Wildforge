//! Mutable atmosphere cells and bounded update cursors.

use crate::planet_atlas::{AtlasGrid, AtlasPos, PlanetAtlas};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicCell {
    pub atmospheric_vapor: u32,
    pub cloud_water: u32,
    pub local_weather_anomaly: i32,
    /// Near-surface temperature anomaly in centi-degrees Celsius.
    pub weather_temperature_anomaly: i16,
    /// Pressure anomaly in compact arbitrary pascal-like units.
    pub pressure_anomaly: i16,
    /// Convective/storm energy, normalized 0..=65535.
    pub storm_energy: u16,
    /// Water transferred out of cloud in the most recent climate hour.
    pub precipitation_rate: u16,
    /// East/north wind anomaly in signed fixed point.
    pub wind_anomaly: [i16; 2],
    pub fire_moisture_anomaly: i32,
    pub vegetation_moisture_anomaly: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DynamicLayers {
    /// Last fully accepted whole-planet climate hour. In-progress sliced
    /// passes are deliberately not checkpointed; they restart deterministically.
    pub completed_climate_hours: u64,
    pub cells: AtlasGrid<DynamicCell>,
}

/// Cursor used to slice one dynamic pass without ever scanning the atlas in a
/// single ordinary server tick.
#[derive(Clone, Debug, Default)]
pub struct DynamicScan {
    cursor: usize,
}

impl DynamicScan {
    pub fn step(
        &mut self,
        atlas: &mut PlanetAtlas,
        budget: usize,
        mut update: impl FnMut(AtlasPos, &mut DynamicCell),
    ) -> bool {
        let side = atlas.side();
        let end = self
            .cursor
            .saturating_add(budget)
            .min(atlas.dynamic.cells.len());
        for index in self.cursor..end {
            let pos = AtlasPos::from_index(index, side).expect("dynamic index is valid");
            update(pos, &mut atlas.dynamic.cells.values_mut()[index]);
        }
        self.cursor = end;
        if self.cursor == atlas.dynamic.cells.len() {
            self.cursor = 0;
            true
        } else {
            false
        }
    }
}
