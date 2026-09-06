//! Inventory completion workings transaction coordination.

use crate::workings::DeliveryMode;
use crate::workings::WorkingCueKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingPhase;
use crate::workings::WorkingResult;
use crate::workings::WorkingTransaction;
use crate::world::World;
use super::Settlement;
use super::repair_inventory_slots;

impl World {
    /// Fieldmend's target lives in a player profile rather than a chunk. Its
    /// Current and material loss are committed first as a write-ahead
    /// `PendingApply`; the inventory mutation is then idempotent. The caller
    /// must durably save the authoritative profile and call
    /// [`World::finish_inventory_working`] before acknowledging completion.
    pub fn complete_inventory_working(
        &mut self,
        id: u64,
        inventory: &mut crate::inventory::Inventory,
    ) -> Result<WorkingResult, String> {
        let transaction = self
            .workings_state
            .as_ref()
            .and_then(|state| state.active.get(&id))
            .cloned()
            .ok_or_else(|| format!("Working {id} is not active."))?;
        if transaction.definition.mode == DeliveryMode::Wand
            && transaction.phase == WorkingPhase::Charging
        {
            if self.working_tick().saturating_sub(transaction.started_tick)
                < crate::workings::MIN_WAND_SETTLE_TICKS
            {
                return self.cancel_working(id);
            }
            self.activate_working(id)?;
        }
        self.settle_working(id, Settlement::Complete, Some(inventory))
    }

    pub fn finish_inventory_working(&mut self, id: u64) -> Result<WorkingResult, String> {
        let transaction = self
            .workings_state
            .as_ref()
            .and_then(|state| state.active.get(&id))
            .cloned()
            .ok_or_else(|| format!("Working {id} is not pending."))?;
        if transaction.phase != WorkingPhase::PendingApply
            || !matches!(transaction.effect, WorkingEffect::RepairItem { .. })
        {
            return Err("Only a profile-checkpointed inventory working may finish here.".into());
        }
        let tick = self.working_tick();
        let state = self
            .workings_state
            .as_mut()
            .ok_or("The world has no workings authority.")?;
        state
            .settle(id, "completed_profile_checkpoint", tick)
            .map_err(|error| error.to_string())?;
        state.save().map_err(|error| error.to_string())?;
        Ok(WorkingResult {
            success: true,
            stable_id: id,
            phase: None,
            cue: WorkingCueKind::Complete,
            warning_band: transaction.strain.warning_band,
            message: format!("{} completes exactly once.", transaction.definition.label),
        })
    }

    /// Reconcile write-ahead Fieldmend effects after the authenticated player
    /// profile has loaded. Applying the saved after-state is idempotent; the
    /// caller must checkpoint that profile and only then call
    /// `finish_inventory_working` for each returned id.
    pub fn resume_pending_inventory_workings(
        &mut self,
        actor: [u8; 16],
        inventory: &mut crate::inventory::Inventory,
    ) -> Result<Vec<u64>, String> {
        let ids = self
            .workings_state
            .as_ref()
            .into_iter()
            .flat_map(|state| state.active.values())
            .filter(|transaction| {
                transaction.actor == actor
                    && transaction.phase == WorkingPhase::PendingApply
                    && matches!(transaction.effect, WorkingEffect::RepairItem { .. })
            })
            .map(|transaction| transaction.id)
            .collect::<Vec<_>>();
        for id in &ids {
            self.complete_inventory_working(*id, inventory)?;
        }
        Ok(ids)
    }

    pub(super) fn validate_inventory_effect(
        &self,
        transaction: &WorkingTransaction,
        inventory: &crate::inventory::Inventory,
    ) -> Result<(), String> {
        let WorkingEffect::RepairItem {
            item_name,
            before_durability,
            after_durability,
            repair_material,
            residue_item,
            ..
        } = &transaction.effect
        else {
            return Err("That effect has no inventory adapter.".into());
        };
        let (target_slot, material_slot) = repair_inventory_slots(transaction)?;
        let target = inventory.slots[target_slot];
        let material = inventory.slots[material_slot];
        let target_item = self
            .reg
            .item_id(item_name)
            .ok_or("Fieldmend's saved target content is unavailable.")?;
        let repair_item = self
            .reg
            .item_id(repair_material)
            .ok_or("Fieldmend's matching material content is unavailable.")?;
        let residue = self
            .reg
            .item_id(residue_item)
            .ok_or("Fieldmend's ordinary residue content is unavailable.")?;
        let before = target.is_some_and(|stack| {
            stack.item == target_item
                && stack.count == 1
                && stack.arcane_id == 0
                && stack.durability == *before_durability
        }) && material.is_some_and(|stack| {
            stack.item == repair_item && stack.count == 1 && stack.arcane_id == 0
        });
        let after = target.is_some_and(|stack| {
            stack.item == target_item
                && stack.count == 1
                && stack.arcane_id == 0
                && stack.durability == *after_durability
        }) && material
            .is_some_and(|stack| stack.item == residue && stack.count == 1 && stack.arcane_id == 0);
        if before || after {
            Ok(())
        } else {
            Err("Fieldmend found neither its exact before nor after inventory state.".into())
        }
    }

    pub(super) fn apply_inventory_effect(
        &self,
        transaction: &WorkingTransaction,
        inventory: &mut crate::inventory::Inventory,
    ) -> Result<(), String> {
        self.validate_inventory_effect(transaction, inventory)?;
        let WorkingEffect::RepairItem {
            item_name,
            before_durability,
            after_durability,
            repair_material,
            residue_item,
            residue_units,
            ..
        } = &transaction.effect
        else {
            return Err("That effect has no inventory adapter.".into());
        };
        let (target_slot, material_slot) = repair_inventory_slots(transaction)?;
        let target_item = self
            .reg
            .item_id(item_name)
            .ok_or("Fieldmend's saved target content is unavailable.")?;
        let repair_item = self
            .reg
            .item_id(repair_material)
            .ok_or("Fieldmend's matching material content is unavailable.")?;
        let residue = self
            .reg
            .item_id(residue_item)
            .ok_or("Fieldmend's ordinary residue content is unavailable.")?;
        let already_after = inventory.slots[target_slot].is_some_and(|stack| {
            stack.item == target_item && stack.durability == *after_durability
        }) && inventory.slots[material_slot]
            .is_some_and(|stack| stack.item == residue && stack.count == *residue_units);
        if already_after {
            return Ok(());
        }
        if !inventory.slots[target_slot].is_some_and(|stack| {
            stack.item == target_item && stack.durability == *before_durability
        }) || !inventory.slots[material_slot]
            .is_some_and(|stack| stack.item == repair_item && stack.count == 1)
        {
            return Err("Fieldmend target changed during its write-ahead apply.".into());
        }
        inventory.slots[target_slot]
            .as_mut()
            .expect("validated target slot")
            .durability = *after_durability;
        let mut residue_stack =
            crate::inventory::ItemStack::new(&self.reg, residue, *residue_units);
        residue_stack.count = *residue_units;
        inventory.slots[material_slot] = Some(residue_stack);
        Ok(())
    }

    pub(super) fn stage_fieldmend_material_loss(
        &self,
        effect: &WorkingEffect,
    ) -> Result<Option<(crate::materials::MaterialLedger, Vec<u8>)>, String> {
        let WorkingEffect::RepairItem {
            repair_material,
            residue_item,
            residue_units,
            ..
        } = effect
        else {
            return Ok(None);
        };
        let repair = self
            .reg
            .item_id(repair_material)
            .ok_or("Fieldmend matching material disappeared.")?;
        let residue = self
            .reg
            .item_id(residue_item)
            .ok_or("Fieldmend residue disappeared.")?;
        let input = &self.reg.item(repair).materials;
        let output = &self.reg.item(residue).materials;
        let mut loss = crate::registry::MaterialVector::new();
        for (material, output_units) in output {
            let available = input.get(material).copied().unwrap_or_default();
            let required = output_units.saturating_mul(u64::from(*residue_units));
            if required > available {
                return Err("Fieldmend residue would transmute or create ordinary matter.".into());
            }
        }
        for (material, input_units) in input {
            let output_units = output
                .get(material)
                .copied()
                .unwrap_or_default()
                .saturating_mul(u64::from(*residue_units));
            if *input_units > output_units {
                loss.insert(material.clone(), input_units - output_units);
            }
        }
        self.material_ledger
            .as_ref()
            .ok_or("The finite material ledger is unavailable.")?
            .stage_linked_recipe_loss(&loss)
            .map_err(|error| error.to_string())
    }
}
