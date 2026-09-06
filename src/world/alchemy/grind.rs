//! Grind alchemy transaction coordination.

use crate::alchemy::AlchemyAuditEvent;
use crate::alchemy::AlchemyCueKind;
use crate::alchemy::AlchemyRequest;
use crate::alchemy::AlchemyResult;
use crate::alchemy::ApparatusKind;
use crate::arcane::ArcaneOwner;
use crate::alchemy::BatchIngredientState;
use crate::alchemy::BatchOutcome;
use crate::planet::BlockPos;
use crate::arcane::Current;
use crate::inventory::Inventory;
use crate::alchemy::ProcessObservation;
use crate::alchemy::ProcessStep;
use crate::planet_atlas::ReservoirMass;
use crate::world::World;
use super::add_materials;
use super::result_for;
use super::split_materials;
use super::take_exact_slot;

impl World {
    pub(super) fn alchemy_grind(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        slot: usize,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        let (batch_id, preparation_id, installation_id) = {
            let apparatus = state
                .apparatus
                .get(&pos)
                .ok_or("The mortar is not installed.")?;
            if apparatus.kind != ApparatusKind::Mortar {
                return Err("Grinding needs the mortar and slab.".into());
            }
            let batch = apparatus
                .batch
                .as_ref()
                .ok_or("The mortar has no measured batch.")?;
            (
                batch.id,
                batch.preparation_id.clone(),
                apparatus.installation_id,
            )
        };
        let definition = self
            .reg
            .preparations
            .get(&preparation_id)
            .ok_or("The saved preparation definition is unavailable.")?
            .clone();
        let stack = inventory
            .slots
            .get(slot)
            .copied()
            .flatten()
            .ok_or("That authoritative inventory slot is empty.")?;
        let item_name = self.reg.item(stack.item).name.clone();
        let ingredient_definition = definition
            .ingredients
            .iter()
            .find(|ingredient| ingredient.item == item_name)
            .ok_or("That item is not a declared ingredient in this batch.")?
            .clone();
        let loaded = state
            .apparatus
            .get(&pos)
            .and_then(|apparatus| apparatus.batch.as_ref())
            .and_then(|batch| {
                batch
                    .ingredients
                    .iter()
                    .find(|ingredient| ingredient.item == item_name)
            })
            .map_or(0, |ingredient| ingredient.count);
        if loaded >= ingredient_definition.count {
            return Err("That ingredient is already present in its measured quantity.".into());
        }
        let taken = take_exact_slot(inventory, slot, stack.item)?;
        let item_definition = self.reg.item(taken.item);
        let condition_permille = if item_definition.food.is_some()
            && item_definition.durability != 0
            && taken.durability != 0
        {
            u16::try_from(
                u64::from(taken.durability)
                    .saturating_mul(1_000)
                    .checked_div(u64::from(item_definition.durability))
                    .unwrap_or(1_000)
                    .min(1_000),
            )
            .unwrap_or(1_000)
            .max(1)
        } else {
            1_000
        };
        let materials = crate::materials::stack_materials(&self.reg, taken);
        let (retained_materials, residue_materials) =
            split_materials(&materials, ingredient_definition.retention_permille);
        let ingredient_water = if item_name == "base:rainbell_dew" {
            let weather = self.weather_state.live()
                .ok_or("Rainbell Dew needs the authoritative water ledger.")?;
            let mut available = weather.water.ledger.industrial;
            let parcel = available.take(crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL);
            if parcel.water_hu != crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL {
                return Err(
                    "That Rainbell Dew has no complete harvested-water custody behind it.".into(),
                );
            }
            parcel
        } else {
            ReservoirMass::default()
        };

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
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let now = self.alchemy_tick();
        let (revision, all_loaded) = {
            let apparatus = state
                .apparatus
                .get_mut(&pos)
                .ok_or("The mortar disappeared.")?;
            let batch = apparatus.batch.as_mut().ok_or("The batch disappeared.")?;
            if batch.outcome != BatchOutcome::Processing
                || definition.steps.get(usize::from(batch.step_index)) != Some(&ProcessStep::Grind)
            {
                return Err("Grinding is not the current declared process step.".into());
            }
            if let Some(ingredient) = batch
                .ingredients
                .iter_mut()
                .find(|ingredient| ingredient.item == item_name)
            {
                ingredient.count = ingredient.count.saturating_add(1);
                ingredient.condition_permille =
                    ingredient.condition_permille.min(condition_permille);
                add_materials(&mut ingredient.retained_materials, &retained_materials)?;
                add_materials(&mut ingredient.residue_materials, &residue_materials)?;
                if taken.arcane_id != 0 {
                    ingredient.source_arcane_ids.push(taken.arcane_id);
                }
            } else {
                batch.ingredients.push(BatchIngredientState {
                    item: item_name.clone(),
                    count: 1,
                    condition_permille,
                    retained_materials,
                    residue_materials,
                    source_arcane_ids: (taken.arcane_id != 0)
                        .then_some(taken.arcane_id)
                        .into_iter()
                        .collect(),
                });
            }
            batch.current_units = batch
                .current_units
                .checked_add(clean.total())
                .ok_or("Batch Current custody overflowed.")?;
            batch.charge_input_units = batch
                .charge_input_units
                .checked_add(clean.total())
                .ok_or("Batch admitted-charge counter overflowed.")?;
            batch.dross_units = batch
                .dross_units
                .checked_add(dross.total())
                .ok_or("Batch dross custody overflowed.")?;
            batch.residue_water = batch
                .residue_water
                .checked_add(ingredient_water)
                .ok_or("Batch ingredient-water custody overflowed.")?;
            let all_loaded = definition.ingredients.iter().all(|required| {
                batch
                    .ingredients
                    .iter()
                    .find(|ingredient| ingredient.item == required.item)
                    .is_some_and(|ingredient| ingredient.count == required.count)
            });
            if all_loaded {
                batch.observations.push(ProcessObservation {
                    step: ProcessStep::Grind,
                    tick: now,
                    temperature_millic: apparatus.temperature_millic,
                    agitation: apparatus.agitation,
                    cleanliness_permille: apparatus.cleanliness_permille,
                    charge_delta: clean.total(),
                });
                batch.step_index = batch.step_index.saturating_add(1);
            }
            batch.revision = batch.revision.saturating_add(1);
            apparatus.integrity_permille = apparatus.integrity_permille.saturating_sub(2);
            apparatus.cleanliness_permille = apparatus.cleanliness_permille.saturating_sub(8);
            apparatus.revision = apparatus.revision.saturating_add(1);
            apparatus.last_operator = request.actor;
            (apparatus.revision, all_loaded)
        };
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id,
            actor: request.actor,
            action: "grind".into(),
            preparation_id: preparation_id.clone(),
            volume_units: 0,
            current_units: clean.total(),
            dross_units: dross.total(),
            tick: now,
            note: if all_loaded {
                "measured mash complete".into()
            } else {
                "one declared ingredient ground".into()
            },
        });
        if !clean.is_empty() || !dross.is_empty() {
            let mut debits = Vec::new();
            if !clean.is_empty() {
                debits.push((ArcaneOwner::Item(taken.arcane_id), clean.clone()));
            }
            if !dross.is_empty() {
                debits.push((ArcaneOwner::ItemDross(taken.arcane_id), dross.clone()));
            }
            let mut credits = Vec::new();
            if !clean.is_empty() {
                credits.push((
                    ArcaneOwner::Alchemy(crate::alchemy::batch_owner_id(batch_id)),
                    clean,
                    Some(preparation_id.clone()),
                ));
            }
            if !dross.is_empty() {
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
                "ingredient ground into exact alchemy custody",
                debits,
                credits,
            )?;
        }
        let message = if all_loaded {
            "The measured mash is ready to transfer."
        } else {
            "The ingredient is reduced; the recipe still shows missing material."
        };
        let mut result = result_for(&self.reg, state, pos, AlchemyCueKind::Grind, message, None)?;
        result.revision = revision;
        Ok(result)
    }
}
