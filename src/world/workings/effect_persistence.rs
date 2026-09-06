//! Effect persistence workings transaction coordination.

use crate::world::BlockPos;
use crate::workings::NudgeEntityKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingTargetSnapshot;
use crate::world::World;
use super::validate_water_carrier;

impl World {
    pub(super) fn save_effect_sidecars(&self, effect: &WorkingEffect) -> Result<(), String> {
        if matches!(effect, WorkingEffect::TransferWater { .. }) {
            self.save_water_carriers()?;
        }
        if matches!(
            effect,
            WorkingEffect::Impulse {
                entity_kind: NudgeEntityKind::DroppedItem,
                ..
            }
        ) {
            self.save_loose_items().map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub(super) fn block_snapshot(&self, pos: BlockPos) -> WorkingTargetSnapshot {
        WorkingTargetSnapshot::Block {
            pos,
            block_name: self.reg.block(self.get_block_at(pos)).name.clone(),
            metadata: self.get_meta_at(pos),
            version: 0,
        }
    }

    pub(super) fn reservoir_snapshot(
        &self,
        pos: BlockPos,
        mass: crate::planet_atlas::ReservoirMass,
    ) -> WorkingTargetSnapshot {
        self.reservoir_snapshot_with_carrier(pos, mass, self.water_carrier_at(pos, mass))
    }

    pub(super) fn reservoir_snapshot_with_carrier(
        &self,
        pos: BlockPos,
        mass: crate::planet_atlas::ReservoirMass,
        carrier: crate::workings::WaterCarrier,
    ) -> WorkingTargetSnapshot {
        WorkingTargetSnapshot::Reservoir {
            pos,
            water_hu: mass.water_hu,
            salt_mass: mass.salt_mass,
            temperature_millic: carrier.temperature_millic(mass.water_hu),
            thermal_millic_hu: carrier.thermal_millic_hu,
            dross_units: carrier.dross_subunits / 256,
            carrier_remainder: carrier.dross_subunits % 256,
            version: 0,
        }
    }

    pub(crate) fn water_carrier_at(
        &self,
        pos: BlockPos,
        mass: crate::planet_atlas::ReservoirMass,
    ) -> crate::workings::WaterCarrier {
        self.water_carriers
            .as_ref()
            .and_then(|state| state.cells.get(&pos).copied())
            .unwrap_or_else(|| {
                let temperature =
                    (self.weather_at_surface(pos.surface()).temperature_c * 1_000.0).round() as i64;
                crate::workings::WaterCarrier {
                    thermal_millic_hu: temperature
                        .saturating_mul(i64::try_from(mass.water_hu).unwrap_or_default()),
                    dross_subunits: 0,
                }
            })
    }

    pub(in crate::world) fn set_water_carrier_at(
        &mut self,
        pos: BlockPos,
        mass: crate::planet_atlas::ReservoirMass,
        carrier: crate::workings::WaterCarrier,
    ) -> Result<(), String> {
        let Some(state) = self.water_carriers.as_mut() else {
            return Ok(());
        };
        if mass.water_hu == 0 {
            state.cells.remove(&pos);
        } else {
            validate_water_carrier(mass, carrier)?;
            if !state.cells.contains_key(&pos)
                && state.cells.len() >= crate::workings::MAX_WATER_CARRIER_CELLS
            {
                return Err("The bounded detailed water-carrier census is full.".into());
            }
            state.cells.insert(pos, carrier);
        }
        Ok(())
    }

    pub(in crate::world) fn move_tracked_water_carrier(
        &mut self,
        from: BlockPos,
        to: BlockPos,
        source_before: crate::planet_atlas::ReservoirMass,
        destination_before: crate::planet_atlas::ReservoirMass,
        water_hu: u64,
    ) -> Result<(), String> {
        let tracked = self
            .water_carriers
            .as_ref()
            .is_some_and(|state| state.cells.contains_key(&from) || state.cells.contains_key(&to));
        if !tracked {
            return Ok(());
        }
        let mut source = self.water_carrier_at(from, source_before);
        let parcel = source
            .take(source_before.water_hu, water_hu)
            .ok_or("ordinary water flow could not split a tracked carrier")?;
        let destination = self
            .water_carrier_at(to, destination_before)
            .checked_add(parcel)
            .ok_or("ordinary water flow overflowed a tracked carrier")?;
        let mut source_after = source_before;
        let moved = source_after.take(water_hu);
        let destination_after = destination_before
            .checked_add(moved)
            .ok_or("ordinary water flow overflowed its destination mass")?;
        validate_water_carrier(source_after, source)?;
        validate_water_carrier(destination_after, destination)?;
        let state = self
            .water_carriers
            .as_mut()
            .expect("tracked carrier test proved authoritative state exists");
        let existing_after =
            usize::from(source_after.water_hu != 0) + usize::from(destination_after.water_hu != 0);
        let existing_before = usize::from(state.cells.contains_key(&from))
            + usize::from(state.cells.contains_key(&to));
        if state
            .cells
            .len()
            .saturating_sub(existing_before)
            .saturating_add(existing_after)
            > crate::workings::MAX_WATER_CARRIER_CELLS
        {
            return Err("The bounded detailed water-carrier census is full.".into());
        }
        if source_after.water_hu == 0 {
            state.cells.remove(&from);
        } else {
            state.cells.insert(from, source);
        }
        if destination_after.water_hu == 0 {
            state.cells.remove(&to);
        } else {
            state.cells.insert(to, destination);
        }
        Ok(())
    }

    pub(super) fn save_water_carriers(&self) -> Result<(), String> {
        self.water_carriers.as_ref().map_or(Ok(()), |state| {
            state.save().map_err(|error| error.to_string())
        })
    }
}
