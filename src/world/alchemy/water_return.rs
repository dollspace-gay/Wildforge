//! Water return alchemy transaction coordination.

use crate::planet::BlockPos;
use crate::planet_atlas::ReservoirMass;
use crate::world::World;

impl World {
    pub(super) fn return_preparation_water(
        &mut self,
        pos: BlockPos,
        dose: &crate::alchemy::PreparationDose,
        runoff: bool,
    ) -> Result<(), String> {
        self.apply_industrial_water_return(pos, dose.liquid.water, runoff)
    }

    pub(super) fn preflight_preparation_water_return(
        &self,
        pos: BlockPos,
        dose: &crate::alchemy::PreparationDose,
        runoff: bool,
    ) -> Result<(), String> {
        self.preflight_industrial_water_return(pos, dose.liquid.water, runoff)
    }

    pub(super) fn preflight_industrial_water_return(
        &self,
        pos: BlockPos,
        mass: ReservoirMass,
        runoff: bool,
    ) -> Result<(), String> {
        if mass.water_hu == 0 && mass.salt_mass == 0 {
            return Ok(());
        }
        let (Some(atlas), Some(weather)) = (&self.planet_atlas, self.weather_state.live()) else {
            return Err("Alchemy water needs the authoritative atlas and water cycle.".into());
        };
        let region = atlas.atlas_pos(pos.surface());
        let possible = if runoff {
            weather.can_return_industrial_exact_to_runoff(region, mass)
        } else {
            weather.can_return_industrial_exact_to_soil(region, mass)
        };
        possible.then_some(()).ok_or_else(|| {
            "The exact alchemy water parcel cannot enter its local reservoir.".into()
        })
    }

    pub(super) fn apply_industrial_water_return(
        &mut self,
        pos: BlockPos,
        mass: ReservoirMass,
        runoff: bool,
    ) -> Result<(), String> {
        if mass.water_hu == 0 && mass.salt_mass == 0 {
            return Ok(());
        }
        let (Some(atlas), Some(weather)) = (&self.planet_atlas, self.weather_state.live_mut())
        else {
            return Err("Alchemy water needs the authoritative atlas and water cycle.".into());
        };
        let region = atlas.atlas_pos(pos.surface());
        let returned = if runoff {
            weather.return_industrial_exact_to_runoff(region, mass)
        } else {
            weather.return_industrial_exact_to_soil(region, mass)
        };
        returned
            .then_some(())
            .ok_or_else(|| "Preflighted alchemy water settlement unexpectedly failed.".into())
    }
}
