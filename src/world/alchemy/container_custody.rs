//! Container custody alchemy transaction coordination.

use crate::alchemy::AlchemyAuditEvent;
use crate::arcane::ArcaneOwner;
use crate::alchemy::BatchOutcome;
use crate::world::BlockPos;
use crate::arcane::Current;
use crate::inventory::ItemStack;
use crate::world::World;
use super::add_materials;

impl World {
    /// Apply one non-duplicable carried-storage assessment. Absolute world
    /// time supplies ordinary aging; Holdfast may reduce `ordinary_age_ticks`
    /// and therefore extends only the interval it actually protected, while
    /// temperatures outside the declared band accelerate the remainder.
    pub fn age_preparation_storage(
        &mut self,
        stack: ItemStack,
        temperature_millic: i32,
        ordinary_age_ticks: u64,
    ) -> Result<Option<bool>, String> {
        if stack.arcane_id == 0 {
            return Ok(None);
        }
        let now = self.alchemy_tick();
        let (preparation_id, item_name) = match self
            .alchemy_state
            .as_ref()
            .and_then(|state| state.containers.get(&stack.arcane_id))
        {
            Some(dose) => (dose.preparation_id.clone(), dose.item_name.clone()),
            None => return Ok(None),
        };
        if stack.count != 1 || self.reg.item(stack.item).name != item_name {
            return Err("A carried preparation disagrees with its stable storage state.".into());
        }
        let storage_band = self
            .reg
            .preparations
            .get(&preparation_id)
            .ok_or("The stored preparation definition is unavailable.")?
            .storage_temperature_millic;
        let dose = self
            .alchemy_state
            .as_mut()
            .and_then(|state| state.containers.get_mut(&stack.arcane_id))
            .ok_or("The preparation moved during its storage assessment.")?;
        let last = dose.last_storage_tick.max(dose.born_tick).min(now);
        let actual_elapsed = now.saturating_sub(last);
        dose.last_storage_tick = now;
        if actual_elapsed == 0 {
            return Ok(Some(false));
        }
        let effective_elapsed = ordinary_age_ticks.min(actual_elapsed);
        let protected = actual_elapsed - effective_elapsed;
        dose.expires_tick = dose.expires_tick.saturating_add(protected);
        let outside_by = if temperature_millic < storage_band[0] {
            storage_band[0].saturating_sub(temperature_millic)
        } else if temperature_millic > storage_band[1] {
            temperature_millic.saturating_sub(storage_band[1])
        } else {
            0
        };
        let extra_multiplier = u64::try_from(outside_by)
            .unwrap_or(u64::MAX)
            .div_ceil(10_000)
            .min(3);
        let extra_age = effective_elapsed.saturating_mul(extra_multiplier);
        dose.expires_tick = dose
            .expires_tick
            .saturating_sub(extra_age)
            .max(dose.born_tick.saturating_add(1));
        let newly_spoiled = now >= dose.expires_tick && dose.outcome != BatchOutcome::Spoiled;
        if newly_spoiled {
            dose.outcome = BatchOutcome::Spoiled;
        }
        Ok(Some(newly_spoiled))
    }

    /// Settle a filled preparation that is physically broken, burned, or
    /// otherwise lost. The liquid enters local runoff, ingredient matter and
    /// glass follow explicit material paths, and both clean charge and dross
    /// become attributed environmental dross. Returning `false` means the
    /// stable item was not an alchemy container and the generic destruction
    /// path should continue.
    pub(crate) fn destroy_preparation_container_at(
        &mut self,
        at: BlockPos,
        stack: ItemStack,
        reason: &str,
    ) -> Result<bool, String> {
        if stack.arcane_id == 0 {
            return Ok(false);
        }
        let mut state = self
            .alchemy_state
            .clone()
            .ok_or("The authoritative alchemy state is unavailable.")?;
        let Some(dose) = state.containers.get(&stack.arcane_id).cloned() else {
            return Ok(false);
        };
        if stack.count != 1 || self.reg.item(stack.item).name != dose.item_name {
            return Err(
                "A destroyed preparation's physical item disagrees with its stable sidecar.".into(),
            );
        }
        let atlas = self
            .planet_atlas
            .as_ref()
            .ok_or("Preparation breakage needs the authoritative planet atlas.")?;
        let region = atlas.atlas_pos(at.surface());
        let clean_owner = ArcaneOwner::Item(dose.container_id);
        let dross_owner = ArcaneOwner::ItemDross(dose.container_id);
        let clean = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&clean_owner))
            .map_or_else(Current::default, |account| account.current.clone());
        let dross = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&dross_owner))
            .map_or_else(Current::default, |account| account.current.clone());
        if clean.total() != dose.current_units || dross.total() != dose.dross_units {
            return Err("Destroyed preparation state and Current custody disagree.".into());
        }
        let mut environmental = clean.clone();
        environmental
            .checked_add(&dross)
            .map_err(|error| error.to_string())?;

        self.preflight_industrial_water_return(at, dose.liquid.water, true)?;

        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        state.containers.remove(&dose.container_id);
        let pollution = state.pollution.entry(at).or_default();
        pollution.water = pollution
            .water
            .checked_add(dose.liquid.water)
            .ok_or("Broken preparation water custody overflowed.")?;
        if let Some(carrier) = dose.liquid.carrier {
            let units = pollution.carrier_units.entry(carrier).or_default();
            *units = units
                .checked_add(dose.liquid.volume_units)
                .ok_or("Broken preparation carrier custody overflowed.")?;
        }
        add_materials(&mut pollution.materials, &dose.materials)?;
        add_materials(&mut pollution.solutes, &dose.liquid.solutes)?;
        pollution
            .dross
            .checked_add(&environmental)
            .map_err(|error| error.to_string())?;
        pollution.last_actor = [255; 16];
        pollution.last_operation_id = operation_id;
        pollution.updated_tick = self.alchemy_tick();
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id: dose.source_installation,
            batch_id: dose.source_batch,
            actor: [255; 16],
            action: "break_container".into(),
            preparation_id: dose.preparation_id.clone(),
            volume_units: dose.liquid.volume_units,
            current_units: clean.total(),
            dross_units: dross.total(),
            tick: self.alchemy_tick(),
            note: format!("stable container {} destroyed: {reason}", dose.container_id),
        });
        let mut debits = Vec::new();
        if !clean.is_empty() {
            debits.push((clean_owner, clean));
        }
        if !dross.is_empty() {
            debits.push((dross_owner, dross));
        }
        self.commit_alchemy_current(
            state,
            operation_id,
            &dose.preparation_id,
            "broken preparation transferred all contents into local pollution",
            debits,
            if environmental.is_empty() {
                Vec::new()
            } else {
                vec![(
                    ArcaneOwner::Dross {
                        region,
                        medium: crate::arcane::DrossMedium::Water,
                    },
                    environmental,
                    None,
                )]
            },
        )?;
        self.apply_industrial_water_return(at, dose.liquid.water, true)?;
        if let Some(ledger) = &mut self.material_ledger {
            ledger
                .record_consumption(&dose.materials)
                .map_err(|error| error.to_string())?;
            ledger
                .bury_materials(at, &dose.vessel_materials, "broken reusable alchemy vessel")
                .map_err(|error| error.to_string())?;
        }
        Ok(true)
    }
}
