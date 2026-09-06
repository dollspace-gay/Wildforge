//! Decant alchemy transaction coordination.

use crate::alchemy::AlchemyAuditEvent;
use crate::alchemy::AlchemyCueKind;
use crate::alchemy::AlchemyRequest;
use crate::alchemy::AlchemyResult;
use crate::arcane::ArcaneOwner;
use crate::alchemy::BatchFailure;
use crate::alchemy::BatchOutcome;
use crate::world::BlockPos;
use crate::arcane::Current;
use crate::inventory::Inventory;
use crate::registry::MaterialVector;
use crate::world::World;
use super::add_materials;
use super::produced;
use super::proportional_units;
use super::result_for;
use super::take_exact_slot;
use super::take_material_fraction;

impl World {
    pub(super) fn alchemy_decant(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        vessel_slot: usize,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        let (batch_id, preparation_id, installation_id, outcome, before_volume) = {
            let apparatus = state
                .apparatus
                .get(&pos)
                .ok_or("The apparatus is not installed.")?;
            let batch = apparatus
                .batch
                .as_ref()
                .ok_or("There is no batch to decant.")?;
            (
                batch.id,
                batch.preparation_id.clone(),
                apparatus.installation_id,
                batch.outcome.clone(),
                batch.liquid.volume_units,
            )
        };
        if !matches!(
            outcome,
            BatchOutcome::Ready | BatchOutcome::Failed(_) | BatchOutcome::Spoiled
        ) {
            return Err(
                "The batch is still processing and cannot be bottled as a finished dose.".into(),
            );
        }
        let definition = self
            .reg
            .preparations
            .get(&preparation_id)
            .ok_or("The saved preparation definition is unavailable.")?
            .clone();
        if before_volume < definition.dose_units {
            return Err(
                "Less than one declared minimum dose remains; drain it through a disposal path."
                    .into(),
            );
        }
        let vessel = self
            .reg
            .item_id(&definition.empty_vessel)
            .ok_or("The declared reusable vessel is unavailable.")?;
        let empty_vessel = take_exact_slot(inventory, vessel_slot, vessel)?;
        let vessel_materials = crate::materials::stack_materials(&self.reg, empty_vessel);
        let item_name = match outcome {
            BatchOutcome::Ready => definition.output_item.clone(),
            BatchOutcome::Failed(failure) => failure.item_id().into(),
            BatchOutcome::Spoiled => BatchFailure::SpentLiquor.item_id().into(),
            BatchOutcome::Processing => unreachable!(),
        };
        let clean_owner = ArcaneOwner::Alchemy(crate::alchemy::batch_owner_id(batch_id));
        let dross_owner = ArcaneOwner::AlchemyDross(crate::alchemy::batch_owner_id(batch_id));
        let clean_account = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&clean_owner))
            .map(|account| account.current.clone())
            .unwrap_or_default();
        let dross_account = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&dross_owner))
            .map(|account| account.current.clone())
            .unwrap_or_default();
        let clean_units =
            proportional_units(clean_account.total(), definition.dose_units, before_volume)?;
        let dross_units =
            proportional_units(dross_account.total(), definition.dose_units, before_volume)?;
        let mut clean_work = clean_account;
        let clean = clean_work
            .take_units(clean_units, [definition.resonance.clone()])
            .map_err(|error| error.to_string())?;
        let mut dross_work = dross_account;
        let dross = dross_work
            .take_units(dross_units, [definition.resonance.clone()])
            .map_err(|error| error.to_string())?;
        if clean.is_empty() && dross.is_empty() {
            return Err(
                "A state-bearing alchemy dose has no finite Current identity to carry.".into(),
            );
        }
        let container_id = self
            .arcane_ledger
            .as_mut()
            .ok_or("The finite Current ledger is unavailable.")?
            .allocate_item_id()
            .map_err(|error| error.to_string())?;
        let now = self.alchemy_tick();
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let (liquid, materials, born_tick, expires_tick, source_batch, dose_outcome) = {
            let apparatus = state
                .apparatus
                .get_mut(&pos)
                .ok_or("The apparatus disappeared.")?;
            let batch = apparatus.batch.as_mut().ok_or("The batch disappeared.")?;
            let liquid = batch
                .liquid
                .take(definition.dose_units)
                .map_err(|error| error.to_string())?;
            let mut materials = MaterialVector::new();
            for ingredient in &mut batch.ingredients {
                let parcel = take_material_fraction(
                    &mut ingredient.retained_materials,
                    definition.dose_units,
                    before_volume,
                )?;
                add_materials(&mut materials, &parcel)?;
            }
            batch.current_units = batch
                .current_units
                .checked_sub(clean.total())
                .ok_or("Batch clean Current no longer matches its ledger.")?;
            batch.dross_units = batch
                .dross_units
                .checked_sub(dross.total())
                .ok_or("Batch dross no longer matches its ledger.")?;
            batch.revision = batch.revision.saturating_add(1);
            apparatus.revision = apparatus.revision.saturating_add(1);
            apparatus.last_operator = request.actor;
            (
                liquid,
                materials,
                batch.born_tick,
                batch.expires_tick,
                batch.id,
                batch.outcome.clone(),
            )
        };
        state.containers.insert(
            container_id,
            crate::alchemy::PreparationDose {
                container_id,
                preparation_id: preparation_id.clone(),
                definition_version: definition.version,
                item_name: item_name.clone(),
                liquid,
                vessel_materials,
                materials,
                current_units: clean.total(),
                dross_units: dross.total(),
                born_tick,
                expires_tick,
                last_storage_tick: now,
                outcome: dose_outcome,
                source_installation: installation_id,
                source_batch,
            },
        );
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id,
            actor: request.actor,
            action: "decant".into(),
            preparation_id: preparation_id.clone(),
            volume_units: definition.dose_units,
            current_units: clean.total(),
            dross_units: dross.total(),
            tick: now,
            note: format!("exact dose decanted into stable container {container_id}"),
        });
        let filled = produced(&self.reg, &item_name, 1, container_id)?;
        let filled_stack = filled
            .clone()
            .into_stack(&self.reg)
            .map_err(|error| error.to_string())?;
        if inventory.add_stack(&self.reg, filled_stack) != 0 {
            return Err("The filled vessel could not enter the authoritative inventory.".into());
        }
        let mut debits = Vec::new();
        let mut credits = Vec::new();
        if !clean.is_empty() {
            debits.push((clean_owner, clean.clone()));
            credits.push((
                ArcaneOwner::Item(container_id),
                clean,
                Some(item_name.clone()),
            ));
        }
        if !dross.is_empty() {
            debits.push((dross_owner, dross.clone()));
            credits.push((
                ArcaneOwner::ItemDross(container_id),
                dross,
                Some(item_name.clone()),
            ));
        }
        self.commit_alchemy_current(
            state.clone(),
            operation_id,
            &preparation_id,
            "exact preparation dose decanted into one stable reusable vessel",
            debits,
            credits,
        )?;
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Pour,
            "One exact dose leaves the batch; volume and every fixed-point remainder stay conserved.",
            Some(filled),
        )
    }
}
