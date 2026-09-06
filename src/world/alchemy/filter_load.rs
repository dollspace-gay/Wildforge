//! Filter load alchemy transaction coordination.

use crate::alchemy::AlchemyAuditEvent;
use crate::alchemy::AlchemyCueKind;
use crate::alchemy::AlchemyRequest;
use crate::alchemy::AlchemyResult;
use crate::alchemy::ApparatusKind;
use crate::arcane::ArcaneOwner;
use crate::world::BlockPos;
use crate::arcane::Current;
use crate::inventory::Inventory;
use crate::alchemy::ProcessStep;
use crate::world::World;
use super::result_for;
use super::take_exact_slot;

impl World {
    pub(super) fn alchemy_load_filter(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        slot: usize,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        let stack = inventory
            .slots
            .get(slot)
            .copied()
            .flatten()
            .ok_or("That authoritative filter-media slot is empty.")?;
        let item_name = self.reg.item(stack.item).name.clone();
        if !matches!(
            item_name.as_str(),
            "base:ashlace_tissue" | "base:filter_cloth" | "base:charcoal" | "base:still_salt"
        ) {
            return Err(
                "The filter stand accepts Ashlace, cloth, charcoal, or still salt as physical media."
                    .into(),
            );
        }
        if stack.arcane_id != 0 && item_name != "base:ashlace_tissue" {
            return Err(
                "Only Ashlace may carry a stable magical identity into disposable filter media."
                    .into(),
            );
        }
        let (installation_id, batch_id, preparation_id) = {
            let apparatus = state
                .apparatus
                .get(&pos)
                .ok_or("The filter stand is not installed.")?;
            if apparatus.kind != ApparatusKind::FilterStand {
                return Err("Disposable filter media mounts only in a filter stand.".into());
            }
            if apparatus.filter_medium.is_some() || apparatus.filter_burden != 0 {
                return Err(
                    "Clean or recover the previous filter burden before mounting fresh media."
                        .into(),
                );
            }
            let batch = apparatus
                .batch
                .as_ref()
                .ok_or("There is no batch to filter.")?;
            let definition = self
                .reg
                .preparations
                .get(&batch.preparation_id)
                .ok_or("The saved preparation definition is unavailable.")?;
            if definition.steps.get(usize::from(batch.step_index)) != Some(&ProcessStep::Filter) {
                return Err("Filtering is not the current visible recipe step.".into());
            }
            (
                apparatus.installation_id,
                batch.id,
                batch.preparation_id.clone(),
            )
        };
        let taken = take_exact_slot(inventory, slot, stack.item)?;
        let materials = crate::materials::stack_materials(&self.reg, taken);
        let clean = if taken.arcane_id == 0 {
            Current::default()
        } else {
            self.arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.account(&ArcaneOwner::Item(taken.arcane_id)))
                .map_or_else(Current::default, |account| account.current.clone())
        };
        let dross = if taken.arcane_id == 0 {
            Current::default()
        } else {
            self.arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.account(&ArcaneOwner::ItemDross(taken.arcane_id)))
                .map_or_else(Current::default, |account| account.current.clone())
        };
        if taken.arcane_id != 0 && clean.is_empty() && dross.is_empty() {
            return Err("That stable Ashlace identity has no finite Current custody.".into());
        }
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let filter_owner_id = crate::alchemy::status_owner_id(
            state
                .allocate_status_id()
                .map_err(|error| error.to_string())?,
        );
        let now = self.alchemy_tick();
        let apparatus = state
            .apparatus
            .get_mut(&pos)
            .ok_or("The filter stand disappeared.")?;
        apparatus.filter_medium = Some(item_name.clone());
        apparatus.filter_owner_id = filter_owner_id;
        apparatus.filter_medium_materials = materials;
        let batch = apparatus
            .batch
            .as_mut()
            .ok_or("The filter batch disappeared.")?;
        batch.current_units = batch
            .current_units
            .checked_add(clean.total())
            .ok_or("Filter Current custody overflowed.")?;
        batch.charge_input_units = batch
            .charge_input_units
            .checked_add(clean.total())
            .ok_or("Filter admitted-charge counter overflowed.")?;
        batch.dross_units = batch
            .dross_units
            .checked_add(dross.total())
            .ok_or("Filter dross custody overflowed.")?;
        batch.revision = batch.revision.saturating_add(1);
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id,
            actor: request.actor,
            action: "load_filter".into(),
            preparation_id: preparation_id.clone(),
            volume_units: 0,
            current_units: clean.total(),
            dross_units: dross.total(),
            tick: now,
            note: format!("mounted one physical {item_name} filter medium"),
        });
        if !clean.is_empty() || !dross.is_empty() {
            let mut debits = Vec::new();
            let mut credits = Vec::new();
            if !clean.is_empty() {
                debits.push((ArcaneOwner::Item(taken.arcane_id), clean.clone()));
                credits.push((
                    ArcaneOwner::Alchemy(crate::alchemy::batch_owner_id(batch_id)),
                    clean,
                    Some(preparation_id.clone()),
                ));
            }
            if !dross.is_empty() {
                debits.push((ArcaneOwner::ItemDross(taken.arcane_id), dross.clone()));
                credits.push((
                    ArcaneOwner::AlchemyDross(crate::alchemy::batch_owner_id(batch_id)),
                    dross,
                    Some(preparation_id.clone()),
                ));
            }
            self.commit_alchemy_current(
                state.clone(),
                operation_id,
                &preparation_id,
                "Ashlace filter media transferred its finite charge and burden into the batch",
                debits,
                credits,
            )?;
        }
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Filter,
            "Fresh physical media is mounted; the next filter step will leave a hazardous burden.",
            None,
        )
    }
}
