//! Charge alchemy transaction coordination.

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
use crate::alchemy::ProcessObservation;
use crate::alchemy::ProcessStep;
use crate::world::World;
use super::result_for;

impl World {
    pub(super) fn alchemy_sample(
        &self,
        pos: BlockPos,
        request: &AlchemyRequest,
        state: &mut crate::alchemy::AlchemyState,
    ) -> Result<AlchemyResult, String> {
        let apparatus = state
            .apparatus
            .get_mut(&pos)
            .ok_or("The apparatus is not installed.")?;
        let batch = apparatus
            .batch
            .as_mut()
            .ok_or("There is no batch to sample.")?;
        batch.revision = batch.revision.saturating_add(1);
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Drip,
            "A non-consuming tuning sample reports only this batch's temperature, step, fill, condition, and coarse charge cue.",
            None,
        )
    }

    pub(super) fn alchemy_charge(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        source_slot: Option<usize>,
        units: u64,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &Inventory,
    ) -> Result<AlchemyResult, String> {
        let (batch_id, preparation_id, installation_id, prior_input, prior_clean) = {
            let apparatus = state
                .apparatus
                .get(&pos)
                .ok_or("The apparatus is not installed.")?;
            let batch = apparatus.batch.as_ref().ok_or("There is no batch here.")?;
            (
                batch.id,
                batch.preparation_id.clone(),
                apparatus.installation_id,
                batch.charge_input_units,
                batch.current_units,
            )
        };
        let definition = self
            .reg
            .preparations
            .get(&preparation_id)
            .ok_or("The saved preparation definition is unavailable.")?
            .clone();
        let step = state
            .apparatus
            .get(&pos)
            .and_then(|apparatus| apparatus.batch.as_ref())
            .and_then(|batch| definition.steps.get(usize::from(batch.step_index)))
            .copied();
        if step != Some(ProcessStep::Charge) {
            return Err("Charging is not the current visible recipe step.".into());
        }
        if units > crate::alchemy::MAX_PREPARATION_CHARGE {
            return Err("That charge request exceeds the bounded apparatus capacity.".into());
        }
        if units != 0 && !self.has_adjacent_alchemy_conductor(pos) {
            return Err(
                "The vessel needs an adjacent ordinary arcane conductor for charge transfer."
                    .into(),
            );
        }
        let target_owner = ArcaneOwner::Alchemy(crate::alchemy::batch_owner_id(batch_id));
        let dross_owner = ArcaneOwner::AlchemyDross(crate::alchemy::batch_owner_id(batch_id));
        let (source_owner, moved) = if units == 0 {
            (None, Current::default())
        } else if let Some(slot) = source_slot {
            let stack = inventory
                .slots
                .get(slot)
                .copied()
                .flatten()
                .ok_or("That authoritative charge-source slot is empty.")?;
            if stack.arcane_id == 0 {
                return Err("That physical item has no bound Current account.".into());
            }
            if !self
                .reg
                .item(stack.item)
                .implement
                .as_ref()
                .is_some_and(|implement| {
                    implement.kind == crate::implements::ImplementItemKind::ChargeVessel
                })
                || !self
                    .implements_state
                    .as_ref()
                    .and_then(|state| state.instance(stack.arcane_id))
                    .is_some_and(|instance| {
                        matches!(
                            instance.kind,
                            crate::implements::ImplementKind::Vessel { .. }
                        )
                    })
            {
                return Err(
                    "Only a live, embodied charge vessel can fund an alchemy transfer.".into(),
                );
            }
            let owner = ArcaneOwner::Item(stack.arcane_id);
            let account = self
                .arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.account(&owner))
                .ok_or("That charge source has no live clean Current custody.")?;
            if account.current.units_of(&definition.resonance) < units {
                return Err("That vessel cannot fund the requested resonance and amount.".into());
            }
            (
                Some(owner),
                Current::single(definition.resonance.clone(), units),
            )
        } else {
            let atlas = self
                .planet_atlas
                .as_ref()
                .ok_or("Ambient charging needs the authoritative planet atlas.")?;
            let owner = ArcaneOwner::Ambient(atlas.atlas_pos(pos.surface()));
            let account = self
                .arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.account(&owner))
                .ok_or("No local Ambient Current account can fund this transfer.")?;
            if account.current.units_of(&definition.resonance) < units {
                return Err("The local ambient resonance cannot fund that measured charge.".into());
            }
            (
                Some(owner),
                Current::single(definition.resonance.clone(), units),
            )
        };
        let next_input = prior_input
            .checked_add(moved.total())
            .ok_or("Batch charge input overflowed.")?;
        let rate_failure = if units != 0 && units < u64::from(definition.charge_rate[0]) {
            Some(BatchFailure::SpentLiquor)
        } else if units > u64::from(definition.charge_rate[1]) {
            Some(BatchFailure::OverchargedBatch)
        } else {
            None
        };
        let finish = units == 0 || next_input >= definition.charge_units || rate_failure.is_some();
        let mut failure = rate_failure;
        if finish && failure.is_none() {
            failure = if next_input < definition.charge_units {
                Some(BatchFailure::SpentLiquor)
            } else if next_input > definition.charge_units {
                Some(BatchFailure::OverchargedBatch)
            } else {
                None
            };
        }
        let declared_dross = if finish {
            definition
                .dross_units
                .min(prior_clean.saturating_add(moved.total()))
        } else {
            0
        };
        let from_new = declared_dross.min(moved.units_of(&definition.resonance));
        let from_existing = declared_dross.saturating_sub(from_new);
        let new_clean_credit = moved
            .units_of(&definition.resonance)
            .saturating_sub(from_new);
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let now = self.alchemy_tick();
        {
            let apparatus = state
                .apparatus
                .get_mut(&pos)
                .ok_or("The apparatus disappeared.")?;
            let batch = apparatus.batch.as_mut().ok_or("The batch disappeared.")?;
            batch.charge_input_units = next_input;
            batch.current_units = prior_clean
                .checked_add(moved.total())
                .and_then(|value| value.checked_sub(declared_dross))
                .ok_or("Batch clean Current settlement underflowed.")?;
            batch.dross_units = batch
                .dross_units
                .checked_add(declared_dross)
                .ok_or("Batch dross settlement overflowed.")?;
            batch.observations.push(ProcessObservation {
                step: ProcessStep::Charge,
                tick: now,
                temperature_millic: apparatus.temperature_millic,
                agitation: apparatus.agitation,
                cleanliness_permille: apparatus.cleanliness_permille,
                charge_delta: moved.total(),
            });
            if finish {
                batch.step_index = batch.step_index.saturating_add(1);
                if let Some(failure) = failure {
                    batch.outcome = BatchOutcome::Failed(failure);
                }
            }
            batch.revision = batch.revision.saturating_add(1);
            apparatus.revision = apparatus.revision.saturating_add(1);
            apparatus.last_operator = request.actor;
        }
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id,
            actor: request.actor,
            action: "charge".into(),
            preparation_id: preparation_id.clone(),
            volume_units: 0,
            current_units: moved.total().saturating_sub(declared_dross),
            dross_units: declared_dross,
            tick: now,
            note: failure.map_or_else(
                || {
                    if finish {
                        "measured charge step complete".into()
                    } else {
                        "bounded charge increment admitted".into()
                    }
                },
                |failure| format!("deterministic named charge failure: {failure:?}"),
            ),
        });
        let mut debits = Vec::new();
        let mut credits = Vec::new();
        if let Some(source_owner) = source_owner
            && !moved.is_empty()
        {
            debits.push((source_owner, moved.clone()));
        }
        if new_clean_credit != 0 {
            credits.push((
                target_owner.clone(),
                Current::single(definition.resonance.clone(), new_clean_credit),
                Some(preparation_id.clone()),
            ));
        }
        if from_existing != 0 {
            debits.push((
                target_owner,
                Current::single(definition.resonance.clone(), from_existing),
            ));
        }
        if declared_dross != 0 {
            credits.push((
                dross_owner,
                Current::single(definition.resonance.clone(), declared_dross),
                Some(preparation_id.clone()),
            ));
        }
        if !debits.is_empty() || !credits.is_empty() {
            self.commit_alchemy_current(
                state.clone(),
                operation_id,
                &preparation_id,
                "measured preparation charge and dross settlement",
                debits,
                credits,
            )?;
        }
        result_for(
            &self.reg,
            state,
            pos,
            failure.map_or(AlchemyCueKind::Pulse, |_| AlchemyCueKind::Overcharge),
            failure.map_or(
                if finish {
                    "The charge step settles its declared clean and dross custody."
                } else {
                    "A bounded charge increment enters the batch."
                },
                |_| "The measured rate or amount produces its deterministic named failure.",
            ),
            None,
        )
    }

    pub(super) fn has_adjacent_alchemy_conductor(&self, pos: BlockPos) -> bool {
        [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0)]
            .into_iter()
            .filter_map(|(du, dy, dv)| pos.offset(du, dy, dv))
            .any(|at| self.reg.block(self.get_block_at(at)).name == "base:arcane_conductor")
    }
}
