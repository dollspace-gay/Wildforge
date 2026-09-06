//! Carrier load alchemy transaction coordination.

use super::CARRIER_ITEM_UNITS;
use super::add_dissolved_displacement;
use super::result_for;
use super::take_exact_slot;
use crate::alchemy::AlchemyAuditEvent;
use crate::alchemy::AlchemyCueKind;
use crate::alchemy::AlchemyRequest;
use crate::alchemy::AlchemyResult;
use crate::alchemy::CarrierKind;
use crate::alchemy::ExactLiquid;
use crate::alchemy::ProcessObservation;
use crate::alchemy::ProcessStep;
use crate::inventory::Inventory;
use crate::planet::BlockPos;
use crate::planet_atlas::HYDRO_UNITS_PER_BLOCK;
use crate::planet_atlas::ReservoirMass;
use crate::planet_atlas::WaterClass;
use crate::workings::WaterCarrier;
use crate::world::World;
use std::collections::BTreeMap;

impl World {
    pub(super) fn alchemy_load_carrier(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        slot: usize,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        let (preparation_id, batch_id, installation_id, current_volume) = {
            let apparatus = state
                .apparatus
                .get(&pos)
                .ok_or("The vessel is not installed.")?;
            let batch = apparatus
                .batch
                .as_ref()
                .ok_or("The vessel holds no measured batch.")?;
            (
                batch.preparation_id.clone(),
                batch.id,
                apparatus.installation_id,
                batch.liquid.volume_units,
            )
        };
        let definition = self
            .reg
            .preparations
            .get(&preparation_id)
            .ok_or("The saved preparation definition is unavailable.")?
            .clone();
        if state
            .apparatus
            .get(&pos)
            .and_then(|apparatus| apparatus.batch.as_ref())
            .and_then(|batch| definition.steps.get(usize::from(batch.step_index)))
            != Some(&ProcessStep::Load)
        {
            return Err("Loading carrier is not the current declared process step.".into());
        }
        let expected_item = self
            .reg
            .item_id(&definition.solvent_item)
            .ok_or("The declared carrier item is unavailable.")?;
        let stack = take_exact_slot(inventory, slot, expected_item)?;
        let amount = if definition.carrier.is_water() {
            HYDRO_UNITS_PER_BLOCK
        } else {
            CARRIER_ITEM_UNITS
        };
        if current_volume.saturating_add(amount) > definition.solvent_units {
            inventory.slots[slot] = Some(stack);
            return Err("That carrier would overfill the exact recipe volume.".into());
        }
        let temperature = state
            .apparatus
            .get(&pos)
            .map_or(20_000, |apparatus| apparatus.temperature_millic);
        if definition.carrier.is_water() {
            let empty_bucket = self
                .reg
                .item_id("base:bucket")
                .ok_or("The reusable empty bucket is unavailable.")?;
            if inventory.add(&self.reg, empty_bucket, 1) != 0 {
                return Err("Make room for the reusable empty bucket before pouring.".into());
            }
        }
        let water_move = if definition.carrier.is_water() {
            let class = match definition.carrier {
                CarrierKind::FreshWater => WaterClass::Fresh,
                CarrierKind::Brine => WaterClass::Salt,
                _ => unreachable!(),
            };
            let mass = self
                .weather_state
                .live()
                .ok_or("Alchemy water needs the authoritative planetary water cycle.")?
                .preview_move_portable_to_industrial(class, amount)
                .ok_or("The portable-water ledger does not contain that full vessel.")?;
            Some((class, mass))
        } else {
            None
        };
        let water = water_move.map_or_else(ReservoirMass::default, |(_, mass)| mass);
        let parcel = ExactLiquid {
            carrier: Some(definition.carrier),
            volume_units: amount,
            water,
            carrier_state: WaterCarrier {
                thermal_millic_hu: i64::from(temperature)
                    .checked_mul(i64::try_from(amount).map_err(|_| "Carrier volume overflowed.")?)
                    .ok_or("Carrier heat custody overflowed.")?,
                dross_subunits: 0,
            },
            solutes: BTreeMap::from([(definition.solvent_item.clone(), amount)]),
        };
        parcel.validate().map_err(|error| error.to_string())?;
        let now = self.alchemy_tick();
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let complete = {
            let apparatus = state
                .apparatus
                .get_mut(&pos)
                .ok_or("The vessel disappeared.")?;
            let batch = apparatus.batch.as_mut().ok_or("The batch disappeared.")?;
            batch
                .liquid
                .checked_add(parcel)
                .map_err(|error| error.to_string())?;
            let complete = batch.liquid.volume_units == definition.solvent_units;
            if complete {
                add_dissolved_displacement(
                    &mut batch.liquid,
                    &definition,
                    apparatus.temperature_millic,
                )?;
                batch.observations.push(ProcessObservation {
                    step: ProcessStep::Load,
                    tick: now,
                    temperature_millic: apparatus.temperature_millic,
                    agitation: apparatus.agitation,
                    cleanliness_permille: apparatus.cleanliness_permille,
                    charge_delta: 0,
                });
                batch.step_index = batch.step_index.saturating_add(1);
                batch.started_tick = now;
                batch.due_tick = now.saturating_add(definition.process_ticks);
            }
            batch.revision = batch.revision.saturating_add(1);
            apparatus.cleanliness_permille = apparatus.cleanliness_permille.saturating_sub(3);
            apparatus.revision = apparatus.revision.saturating_add(1);
            apparatus.last_operator = request.actor;
            complete
        };
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id,
            actor: request.actor,
            action: "load_carrier".into(),
            preparation_id: preparation_id.clone(),
            volume_units: amount,
            current_units: 0,
            dross_units: 0,
            tick: now,
            note: if complete {
                "exact carrier volume complete".into()
            } else {
                "partial exact carrier volume loaded".into()
            },
        });
        let result = result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Pour,
            if complete {
                "The exact carrier volume is loaded."
            } else {
                "The vessel records the partial fill; more declared carrier is needed."
            },
            None,
        )?;
        state.validate().map_err(|error| error.to_string())?;
        if let Some((class, expected)) = water_move {
            let moved = self
                .weather_state
                .live_mut()
                .and_then(|weather| weather.move_portable_to_industrial(class, amount))
                .ok_or("Preflighted carrier water unexpectedly failed to move.")?;
            debug_assert_eq!(moved, expected);
        }
        Ok(result)
    }
}
