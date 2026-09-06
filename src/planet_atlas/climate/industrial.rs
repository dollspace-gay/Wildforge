//! Preflighted industrial water exchanges with soil, runoff, and atmosphere.

use super::PlanetaryWeather;
use crate::planet_atlas::{AtlasPos, ReservoirMass, WaterClass};

impl PlanetaryWeather {
    /// Preflight and perform the common cleaning exchange without cloning
    /// whole-planet weather: one portable vessel enters industrial custody,
    /// then that cleaning water plus an existing exact residue parcel enter
    /// the local runoff cell together.
    pub fn preview_portable_exchange_to_runoff(
        &self,
        pos: AtlasPos,
        class: WaterClass,
        requested_hu: u64,
        existing_industrial: ReservoirMass,
    ) -> Option<ReservoirMass> {
        let parcel = self.preview_move_portable_to_industrial(class, requested_hu)?;
        let industrial_after = self.water.ledger.industrial.checked_add(parcel)?;
        let runoff_parcel = parcel.checked_add(existing_industrial)?;
        if industrial_after.water_hu < runoff_parcel.water_hu
            || industrial_after.salt_mass < runoff_parcel.salt_mass
        {
            return None;
        }
        self.water
            .cells
            .values()
            .get(pos.index(self.water.cells.side()))?
            .runoff
            .checked_add(runoff_parcel)?;
        Some(parcel)
    }

    pub fn portable_exchange_to_runoff(
        &mut self,
        pos: AtlasPos,
        class: WaterClass,
        requested_hu: u64,
        existing_industrial: ReservoirMass,
    ) -> Option<ReservoirMass> {
        let expected = self.preview_portable_exchange_to_runoff(
            pos,
            class,
            requested_hu,
            existing_industrial,
        )?;
        let parcel = self.move_portable_to_industrial(class, requested_hu)?;
        debug_assert_eq!(parcel, expected);
        let runoff_parcel = parcel.checked_add(existing_industrial)?;
        let returned = self.return_industrial_exact_to_runoff(pos, runoff_parcel);
        debug_assert!(returned, "preflighted cleaning-water exchange failed");
        returned.then_some(parcel)
    }

    /// Settle an exact industrial subdivision back into local soil. This is
    /// used for drinking, plot application, and responsible liquid disposal;
    /// salt travels with the same parcel instead of being relabelled fresh.
    pub fn can_return_industrial_exact_to_soil(&self, pos: AtlasPos, mass: ReservoirMass) -> bool {
        if self.water.ledger.industrial.water_hu < mass.water_hu
            || self.water.ledger.industrial.salt_mass < mass.salt_mass
        {
            return false;
        }
        let index = pos.index(self.water.cells.side());
        let in_scratch = self.active_hour.is_some() && index < self.cursor;
        let soil = if in_scratch {
            self.water_scratch.values().get(index).map(|cell| cell.soil)
        } else {
            self.water.cells.values().get(index).map(|cell| cell.soil)
        };
        soil.and_then(|soil| soil.checked_add(mass)).is_some()
    }

    pub fn return_industrial_exact_to_soil(&mut self, pos: AtlasPos, mass: ReservoirMass) -> bool {
        if self.water.ledger.industrial.take_exact(mass).is_none() {
            return false;
        }
        let index = pos.index(self.water.cells.side());
        let in_scratch = self.active_hour.is_some() && index < self.cursor;
        let water = if in_scratch {
            &mut self.water_scratch.values_mut()[index]
        } else {
            &mut self.water.cells.values_mut()[index]
        };
        if water.soil.add_assign(mass).is_err() {
            self.water
                .ledger
                .industrial
                .add_assign(mass)
                .expect("industrial soil rollback fits");
            return false;
        }
        true
    }

    pub fn return_industrial_exact_to_runoff(
        &mut self,
        pos: AtlasPos,
        mass: ReservoirMass,
    ) -> bool {
        if self.water.ledger.industrial.take_exact(mass).is_none() {
            return false;
        }
        let index = pos.index(self.water.cells.side());
        if self.water.cells.values_mut()[index]
            .runoff
            .add_assign(mass)
            .is_err()
        {
            self.water
                .ledger
                .industrial
                .add_assign(mass)
                .expect("industrial runoff rollback fits");
            return false;
        }
        true
    }

    pub fn can_return_industrial_exact_to_runoff(
        &self,
        pos: AtlasPos,
        mass: ReservoirMass,
    ) -> bool {
        self.water.ledger.industrial.water_hu >= mass.water_hu
            && self.water.ledger.industrial.salt_mass >= mass.salt_mass
            && self
                .water
                .cells
                .values()
                .get(pos.index(self.water.cells.side()))
                .and_then(|cell| cell.runoff.checked_add(mass))
                .is_some()
    }

    /// Move process water into local atmospheric vapor. Existing ordinary
    /// machines use this unrestricted form; subsystems with water already in
    /// flight use `exhaust_industrial_vapor_excluding` so another machine
    /// cannot spend their custody.
    pub fn exhaust_industrial_vapor(&mut self, pos: AtlasPos, requested_hu: u64) -> u64 {
        self.exhaust_industrial_vapor_excluding(pos, requested_hu, 0)
    }

    pub fn exhaust_industrial_vapor_excluding(
        &mut self,
        pos: AtlasPos,
        requested_hu: u64,
        reserved_hu: u64,
    ) -> u64 {
        let available = self
            .water
            .ledger
            .industrial
            .water_hu
            .saturating_sub(reserved_hu);
        let water_hu = requested_hu.min(available);
        if water_hu == 0 || water_hu > u64::from(u32::MAX) {
            return 0;
        }
        let amount = water_hu as u32;
        let index = pos.index(self.cells.cells.side());
        let Some(next) = self.cells.cells.values()[index]
            .atmospheric_vapor
            .checked_add(amount)
        else {
            return 0;
        };
        if self.active_hour.is_some()
            && index < self.cursor
            && self.scratch.values()[index]
                .atmospheric_vapor
                .checked_add(amount)
                .is_none()
        {
            return 0;
        }
        self.water.ledger.industrial.water_hu -= water_hu;
        self.cells.cells.values_mut()[index].atmospheric_vapor = next;
        if self.active_hour.is_some() && index < self.cursor {
            self.scratch.values_mut()[index].atmospheric_vapor += amount;
        }
        water_hu
    }
}
