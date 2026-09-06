//! Transfers between atmospheric, coarse, detailed, and portable water custody.

use super::{PlanetaryWeather, PrecipitationForm};
use crate::planet_atlas::{AtlasPos, HYDRO_UNITS_PER_BLOCK, ReservoirMass, WaterClass};

impl PlanetaryWeather {
    /// Move already-landed precipitation out of the coarse climate reserve
    /// and into the voxel water cycle. Rain may only draw from runoff (water
    /// accepted by soil remains in soil); snow draws from snowpack. The
    /// returned amount is the only amount the caller is allowed to
    /// materialize, which prevents precipitation from being counted twice.
    pub fn withdraw_water_cycle_transfer(
        &mut self,
        pos: AtlasPos,
        form: PrecipitationForm,
        requested: u32,
    ) -> u32 {
        self.withdraw_water_cycle_mass(pos, form, requested)
            .water_hu
            .min(u64::from(u32::MAX)) as u32
    }

    pub fn withdraw_water_cycle_mass(
        &mut self,
        pos: AtlasPos,
        form: PrecipitationForm,
        requested: u32,
    ) -> ReservoirMass {
        let side = self.cells.cells.side();
        if requested == 0 || pos.u >= side || pos.v >= side {
            return ReservoirMass::default();
        }
        let index = pos.index(side);
        let processed = self.active_hour.is_some() && index < self.cursor;
        let source = match form {
            PrecipitationForm::Rain => &mut self.water.cells.values_mut()[index].runoff,
            PrecipitationForm::Snow => &mut self.water.cells.values_mut()[index].snow,
            PrecipitationForm::None => return ReservoirMass::default(),
        };
        let transferred_mass = source.take(u64::from(requested));
        if processed {
            let scratch_source = match form {
                PrecipitationForm::Rain => &mut self.water_scratch.values_mut()[index].runoff,
                PrecipitationForm::Snow => &mut self.water_scratch.values_mut()[index].snow,
                PrecipitationForm::None => unreachable!(),
            };
            let _ = scratch_source.take(transferred_mass.water_hu);
        }
        self.water
            .credit_detailed(transferred_mass)
            .expect("water commitment total fits u64");
        let transferred = transferred_mass.water_hu as u32;
        self.pending_water_cycle_outflow = self
            .pending_water_cycle_outflow
            .saturating_add(u64::from(transferred));
        if self.active_hour.is_some() {
            self.active_water_cycle_outflow = self
                .active_water_cycle_outflow
                .saturating_add(u64::from(transferred));
        }
        transferred_mass
    }

    pub fn credit_detailed_vapor(&mut self, pos: AtlasPos, mass: ReservoirMass) -> bool {
        self.credit_detailed_vapor_from(pos, None, mass)
    }

    pub fn credit_detailed_vapor_from(
        &mut self,
        pos: AtlasPos,
        preferred: Option<u64>,
        mass: ReservoirMass,
    ) -> bool {
        if mass.water_hu == 0 {
            return false;
        }
        let index = pos.index(self.cells.cells.side());
        let Ok(water_hu) = u32::try_from(mass.water_hu) else {
            return false;
        };
        let Some(next) = self.cells.cells.values()[index]
            .atmospheric_vapor
            .checked_add(water_hu)
        else {
            return false;
        };
        if self.active_hour.is_some()
            && index < self.cursor
            && self.scratch.values()[index]
                .atmospheric_vapor
                .checked_add(water_hu)
                .is_none()
        {
            return false;
        }
        if !self.water.debit_detailed_exact_from(preferred, mass) {
            return false;
        }
        self.cells.cells.values_mut()[index].atmospheric_vapor = next;
        if self.active_hour.is_some() && index < self.cursor {
            let Some(next) = self.scratch.values()[index]
                .atmospheric_vapor
                .checked_add(water_hu)
            else {
                return false;
            };
            self.scratch.values_mut()[index].atmospheric_vapor = next;
        }
        self.water.ledger.precipitated_salt_mass = self
            .water
            .ledger
            .precipitated_salt_mass
            .saturating_add(mass.salt_mass);
        true
    }

    pub fn return_detailed_to_runoff(&mut self, pos: AtlasPos, mass: ReservoirMass) -> bool {
        if !self.water.debit_detailed_exact(mass) {
            return false;
        }
        let index = pos.index(self.water.cells.side());
        self.water.cells.values_mut()[index]
            .runoff
            .add_assign(mass)
            .is_ok()
    }

    pub fn reject_detailed_salt_to_runoff(&mut self, pos: AtlasPos, salt_mass: u64) -> bool {
        self.reject_detailed_salt_to_runoff_from(pos, None, salt_mass)
    }

    pub fn reject_detailed_salt_to_runoff_from(
        &mut self,
        pos: AtlasPos,
        preferred: Option<u64>,
        salt_mass: u64,
    ) -> bool {
        let mass = ReservoirMass {
            water_hu: 0,
            salt_mass,
        };
        if !self.water.debit_detailed_exact_from(preferred, mass) {
            return false;
        }
        let index = pos.index(self.water.cells.side());
        self.water.cells.values_mut()[index]
            .runoff
            .add_assign(mass)
            .is_ok()
    }

    pub fn move_detailed_to_industrial_from(
        &mut self,
        preferred: Option<u64>,
        mass: ReservoirMass,
    ) -> bool {
        if !self.water.debit_detailed_exact_from(preferred, mass) {
            return false;
        }
        if self.water.ledger.industrial.add_assign(mass).is_err() {
            self.water
                .credit_detailed_to(preferred, mass)
                .expect("industrial rollback fits");
            return false;
        }
        true
    }

    pub fn move_detailed_to_portable_from(
        &mut self,
        preferred: Option<u64>,
        mass: ReservoirMass,
    ) -> Option<WaterClass> {
        let class = mass.water_class();
        if !self.water.debit_detailed_exact_from(preferred, mass) {
            return None;
        }
        if self.water.ledger.portable[class as usize]
            .add_assign(mass)
            .is_err()
        {
            self.water
                .credit_detailed_to(preferred, mass)
                .expect("portable rollback fits");
            return None;
        }
        Some(class)
    }

    pub fn pump_groundwater(&mut self, pos: AtlasPos, requested_hu: u64) -> ReservoirMass {
        let index = pos.index(self.water.cells.side());
        let parcel = self.water.cells.values_mut()[index]
            .groundwater
            .take(requested_hu);
        if parcel.water_hu == 0 {
            return parcel;
        }
        if self.water.credit_detailed(parcel).is_err() {
            self.water.cells.values_mut()[index]
                .groundwater
                .add_assign(parcel)
                .expect("groundwater rollback fits");
            return ReservoirMass::default();
        }
        let drawdown = parcel.water_hu.min(i32::MAX as u64) as i32;
        self.water.cells.values_mut()[index].groundwater_head_milliblocks =
            self.water.cells.values()[index]
                .groundwater_head_milliblocks
                .saturating_sub((drawdown / 8).max(1));
        parcel
    }

    pub fn move_detailed_to_industrial(&mut self, mass: ReservoirMass) -> bool {
        self.move_detailed_to_industrial_from(None, mass)
    }

    pub fn move_detailed_to_portable(&mut self, mass: ReservoirMass) -> Option<WaterClass> {
        self.move_detailed_to_portable_from(None, mass)
    }

    pub fn move_portable_to_detailed(&mut self, class: WaterClass) -> Option<ReservoirMass> {
        let parcel = self.water.ledger.portable[class as usize].take(HYDRO_UNITS_PER_BLOCK);
        if parcel.water_hu != HYDRO_UNITS_PER_BLOCK {
            self.water.ledger.portable[class as usize]
                .add_assign(parcel)
                .expect("portable rollback fits");
            return None;
        }
        if self.water.credit_detailed(parcel).is_err() {
            self.water.ledger.portable[class as usize]
                .add_assign(parcel)
                .expect("portable rollback fits");
            return None;
        }
        Some(parcel)
    }

    /// Move one exact portable vessel into a host-owned industrial
    /// subdivision such as an alchemy batch. The returned mass is the
    /// subdivision's custody record; the planetary aggregate remains in the
    /// industrial ledger until use, spill, or disposal returns it.
    pub fn move_portable_to_industrial(
        &mut self,
        class: WaterClass,
        requested_hu: u64,
    ) -> Option<ReservoirMass> {
        let expected = self.preview_move_portable_to_industrial(class, requested_hu)?;
        let parcel = self.water.ledger.portable[class as usize].take(requested_hu);
        debug_assert_eq!(parcel, expected);
        if self.water.ledger.industrial.add_assign(parcel).is_err() {
            self.water.ledger.portable[class as usize]
                .add_assign(parcel)
                .expect("portable alchemy rollback fits");
            return None;
        }
        Some(parcel)
    }

    pub fn preview_move_portable_to_industrial(
        &self,
        class: WaterClass,
        requested_hu: u64,
    ) -> Option<ReservoirMass> {
        if requested_hu == 0 {
            return None;
        }
        let mut portable = self.water.ledger.portable[class as usize];
        let parcel = portable.take(requested_hu);
        (parcel.water_hu == requested_hu
            && self.water.ledger.industrial.checked_add(parcel).is_some())
        .then_some(parcel)
    }
}
