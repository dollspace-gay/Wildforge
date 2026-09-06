//! Ecological water exchanges respect the in-flight weather cursor.

use super::PlanetaryWeather;
use crate::planet_atlas::AtlasPos;

impl PlanetaryWeather {
    /// Soil water visible to an ecology update even while a sliced climate
    /// hour is in flight. Cells already visited by the weather pass live in
    /// scratch; later cells still live in the committed arrays.
    pub fn ecology_soil_water_hu(&self, pos: AtlasPos) -> u64 {
        let index = pos.index(self.water.cells.side());
        if self.active_hour.is_some() && index < self.cursor {
            self.water_scratch.values()[index].soil.water_hu
        } else {
            self.water.cells.values()[index].soil.water_hu
        }
    }

    /// Move real fresh soil water into atmospheric vapor for magical plant
    /// growth. This is transpiration, not deletion. The same in-flight rule
    /// as `ecology_soil_water_hu` prevents a later sliced-weather commit from
    /// overwriting the ecological withdrawal.
    pub fn transpire_ecology(&mut self, pos: AtlasPos, requested_hu: u64) -> u64 {
        let index = pos.index(self.water.cells.side());
        let in_scratch = self.active_hour.is_some() && index < self.cursor;
        let (water, atmosphere) = if in_scratch {
            (
                &mut self.water_scratch.values_mut()[index],
                &mut self.scratch.values_mut()[index],
            )
        } else {
            (
                &mut self.water.cells.values_mut()[index],
                &mut self.cells.cells.values_mut()[index],
            )
        };
        let room = u64::from(u32::MAX - atmosphere.atmospheric_vapor);
        let moved = water.soil.take_fresh_water(requested_hu.min(room));
        atmosphere.atmospheric_vapor += moved.water_hu as u32;
        self.evaporation_units = self.evaporation_units.saturating_add(moved.water_hu);
        moved.water_hu
    }

    /// Move liquid embodied in a harvested ecological product (currently
    /// rainbell dew) from real soil water into the detailed industrial/
    /// circulating reservoir. It remains inside the finite water audit until
    /// a later use returns it to soil, vapor, or another declared reservoir.
    pub fn harvest_ecology_water(&mut self, pos: AtlasPos, requested_hu: u64) -> u64 {
        let index = pos.index(self.water.cells.side());
        let in_scratch = self.active_hour.is_some() && index < self.cursor;
        let water = if in_scratch {
            &mut self.water_scratch.values_mut()[index]
        } else {
            &mut self.water.cells.values_mut()[index]
        };
        let moved = water.soil.take_fresh_water(requested_hu);
        if self.water.ledger.industrial.add_assign(moved).is_err() {
            water
                .soil
                .add_assign(moved)
                .expect("rolling back ecological water harvest fits");
            return 0;
        }
        moved.water_hu
    }

    /// Return water embodied in a planted ecological item to local soil.
    /// The item identity is handled by the arcane ledger; this moves only the
    /// physical fresh-water parcel previously held in circulating custody.
    pub fn return_ecology_water_to_soil(&mut self, pos: AtlasPos, requested_hu: u64) -> u64 {
        let moved = self.water.ledger.industrial.take_fresh_water(requested_hu);
        if moved.water_hu == 0 {
            return 0;
        }
        let index = pos.index(self.water.cells.side());
        let in_scratch = self.active_hour.is_some() && index < self.cursor;
        let water = if in_scratch {
            &mut self.water_scratch.values_mut()[index]
        } else {
            &mut self.water.cells.values_mut()[index]
        };
        if water.soil.add_assign(moved).is_err() {
            self.water
                .ledger
                .industrial
                .add_assign(moved)
                .expect("rolling back planted ecological water fits");
            return 0;
        }
        moved.water_hu
    }
}
