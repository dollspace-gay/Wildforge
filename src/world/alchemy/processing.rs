//! Processing alchemy transaction coordination.

use crate::alchemy::AgitationKind;
use crate::alchemy::AlchemyAuditEvent;
use crate::alchemy::AlchemyCueKind;
use crate::alchemy::AlchemyRequest;
use crate::alchemy::AlchemyResult;
use crate::alchemy::ApparatusKind;
use crate::arcane::ArcaneOwner;
use crate::alchemy::BatchFailure;
use crate::alchemy::BatchOutcome;
use crate::world::BlockEntity;
use crate::planet::BlockPos;
use crate::alchemy::ProcessObservation;
use crate::alchemy::ProcessStep;
use crate::world::World;
use super::process_failure;
use super::result_for;
use super::separate_brine_distillate;

impl World {
    pub(super) fn alchemy_set_heat(
        &self,
        pos: BlockPos,
        request: &AlchemyRequest,
        temperature_millic: i32,
        state: &mut crate::alchemy::AlchemyState,
    ) -> Result<AlchemyResult, String> {
        if !(-50_000..=250_000).contains(&temperature_millic) {
            return Err(
                "That requested temperature is outside the physical apparatus range.".into(),
            );
        }
        let ambient_millic = (self.weather_at_surface(pos.surface()).temperature_c * 1_000.0)
            .round()
            .clamp(-50_000.0, 60_000.0) as i32;
        let mut minimum_millic = ambient_millic;
        let mut maximum_millic = ambient_millic;
        for at in (-1..=1)
            .flat_map(|du| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dv| (du, dy, dv))))
            .filter(|offset| *offset != (0, 0, 0))
            .filter_map(|(du, dy, dv)| pos.offset(du, dy, dv))
        {
            let block = self.get_block_at(at);
            let name = self.reg.block(block).name.as_str();
            if self.reg.is_lava(block) {
                maximum_millic = maximum_millic.max(180_000);
            } else if name == "base:firebox_lit" {
                maximum_millic = maximum_millic.max(150_000);
            } else if name == "base:fire" {
                maximum_millic = maximum_millic.max(105_000);
            } else if name == "base:furnace"
                && self
                    .block_entity_at(&at)
                    .is_some_and(|entity| matches!(entity, BlockEntity::Furnace(furnace) if furnace.burn_left > 0.0))
            {
                maximum_millic = maximum_millic.max(125_000);
            }
            if name == "base:ice" || name.starts_with("base:snow") {
                minimum_millic = minimum_millic.min(-8_000);
            }
        }
        if !(minimum_millic..=maximum_millic).contains(&temperature_millic) {
            return Err(format!(
                "The nearby ordinary heat/cooling arrangement can hold only {minimum_millic}..={maximum_millic} m°C; it cannot assert the requested temperature."
            ));
        }
        let apparatus = state
            .apparatus
            .get_mut(&pos)
            .ok_or("The apparatus is not installed.")?;
        if apparatus.batch.is_none() {
            return Err("There is no batch here to heat or cool.".into());
        }
        let delta = temperature_millic.saturating_sub(apparatus.temperature_millic);
        apparatus.temperature_millic = apparatus
            .temperature_millic
            .saturating_add(delta.clamp(-20_000, 20_000));
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Bubble,
            "The vessel temperature moves one bounded interval toward the ordinary heat control.",
            None,
        )
    }

    pub(super) fn alchemy_set_agitation(
        &self,
        pos: BlockPos,
        request: &AlchemyRequest,
        agitation: AgitationKind,
        state: &mut crate::alchemy::AlchemyState,
    ) -> Result<AlchemyResult, String> {
        let apparatus = state
            .apparatus
            .get_mut(&pos)
            .ok_or("The apparatus is not installed.")?;
        if apparatus.batch.is_none() {
            return Err("There is no batch here to stir or settle.".into());
        }
        apparatus.agitation = agitation;
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Bubble,
            "The mechanical agitation setting is changed.",
            None,
        )
    }

    pub(super) fn alchemy_advance(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        step: ProcessStep,
        state: &mut crate::alchemy::AlchemyState,
    ) -> Result<AlchemyResult, String> {
        let (batch_id, preparation_id, installation_id, step_index) = {
            let apparatus = state
                .apparatus
                .get(&pos)
                .ok_or("The apparatus is not installed.")?;
            let batch = apparatus.batch.as_ref().ok_or("There is no batch here.")?;
            (
                batch.id,
                batch.preparation_id.clone(),
                apparatus.installation_id,
                batch.step_index,
            )
        };
        let definition = self
            .reg
            .preparations
            .get(&preparation_id)
            .ok_or("The saved preparation definition is unavailable.")?
            .clone();
        if definition.steps.get(usize::from(step_index)) != Some(&step) {
            return Err("The requested operation is out of the visible recipe order.".into());
        }
        if matches!(
            step,
            ProcessStep::Grind | ProcessStep::Load | ProcessStep::Charge
        ) {
            return Err("That process step has its own embodied station action.".into());
        }
        let apparatus_kind = state
            .apparatus
            .get(&pos)
            .map(|apparatus| apparatus.kind)
            .ok_or("The apparatus is not installed.")?;
        let required_kind = match step {
            ProcessStep::Filter => ApparatusKind::FilterStand,
            ProcessStep::Distill => ApparatusKind::Alembic,
            _ => definition.process.apparatus(),
        };
        if apparatus_kind != required_kind {
            return Err(format!(
                "The visible {step:?} step needs its {required_kind:?}; transfer the batch physically."
            ));
        }
        if step == ProcessStep::Filter
            && state
                .apparatus
                .get(&pos)
                .is_none_or(|apparatus| apparatus.filter_medium.is_none())
        {
            return Err(
                "Mount one physical cloth, charcoal, or still-salt medium before filtering.".into(),
            );
        }
        let (filter_owner_id, filter_capacity) = if step == ProcessStep::Filter {
            let apparatus = state
                .apparatus
                .get(&pos)
                .ok_or("The filter stand disappeared.")?;
            let medium = apparatus
                .filter_medium
                .as_deref()
                .ok_or("The filter medium disappeared.")?;
            let capacity = match medium {
                "base:filter_cloth" => 4,
                "base:charcoal" => 8,
                "base:still_salt" => 16,
                "base:ashlace_tissue" => 32,
                _ => return Err("The mounted filter medium is no longer approved.".into()),
            };
            (apparatus.filter_owner_id, capacity)
        } else {
            (0, 0)
        };
        let now = self.alchemy_tick();
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let (failure, final_step, captured_dross) = {
            let apparatus = state
                .apparatus
                .get_mut(&pos)
                .ok_or("The apparatus disappeared.")?;
            let temperature_millic = apparatus.temperature_millic;
            let agitation = apparatus.agitation;
            let cleanliness_permille = apparatus.cleanliness_permille;
            let batch = apparatus.batch.as_mut().ok_or("The batch disappeared.")?;
            let failure = process_failure(
                &definition,
                temperature_millic,
                agitation,
                cleanliness_permille,
                batch,
                step,
                now,
            );
            batch.observations.push(ProcessObservation {
                step,
                tick: now,
                temperature_millic: apparatus.temperature_millic,
                agitation: apparatus.agitation,
                cleanliness_permille: apparatus.cleanliness_permille,
                charge_delta: 0,
            });
            batch.step_index = batch.step_index.saturating_add(1);
            if let Some(failure) = failure {
                batch.outcome = BatchOutcome::Failed(failure);
            } else if step == ProcessStep::Distill {
                separate_brine_distillate(batch)?;
            }
            let final_step = usize::from(batch.step_index) == definition.steps.len();
            if final_step && failure.is_none() {
                batch.outcome = BatchOutcome::Ready;
            }
            batch.revision = batch.revision.saturating_add(1);
            apparatus.cleanliness_permille = apparatus.cleanliness_permille.saturating_sub(
                if matches!(step, ProcessStep::Distill | ProcessStep::Filter) {
                    18
                } else {
                    4
                },
            );
            let captured_dross = if step == ProcessStep::Filter {
                let captured = batch.dross_units.min(filter_capacity);
                batch.dross_units -= captured;
                apparatus.filter_burden = apparatus
                    .filter_burden
                    .checked_add(captured)
                    .ok_or("Filter burden overflowed.")?;
                captured
            } else {
                0
            };
            if matches!(step, ProcessStep::Heat | ProcessStep::Distill) {
                apparatus.integrity_permille = apparatus.integrity_permille.saturating_sub(1);
            }
            apparatus.revision = apparatus.revision.saturating_add(1);
            apparatus.last_operator = request.actor;
            (failure, final_step, captured_dross)
        };
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id,
            actor: request.actor,
            action: format!("advance_{step:?}").to_lowercase(),
            preparation_id: preparation_id.clone(),
            volume_units: 0,
            current_units: 0,
            dross_units: captured_dross,
            tick: now,
            note: failure.map_or_else(
                || {
                    if final_step {
                        "declared batch ready".into()
                    } else {
                        "declared control step accepted".into()
                    }
                },
                |failure| format!("deterministic named failure: {failure:?}"),
            ),
        });
        if captured_dross != 0 {
            let source_owner = ArcaneOwner::AlchemyDross(crate::alchemy::batch_owner_id(batch_id));
            let mut available = self
                .arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.account(&source_owner))
                .map(|account| account.current.clone())
                .ok_or("The batch's filterable dross account is unavailable.")?;
            let captured = available
                .take_units(captured_dross, [definition.resonance.clone()])
                .map_err(|error| error.to_string())?;
            self.commit_alchemy_current(
                state.clone(),
                operation_id,
                &preparation_id,
                "physical filter media captured exact batch dross",
                vec![(source_owner, captured.clone())],
                vec![(
                    ArcaneOwner::AlchemyDross(filter_owner_id),
                    captured,
                    Some("base:spent_filter".into()),
                )],
            )?;
        }
        result_for(
            &self.reg,
            state,
            pos,
            failure.map_or(AlchemyCueKind::Bubble, |failure| {
                if failure == BatchFailure::OverchargedBatch {
                    AlchemyCueKind::Overcharge
                } else {
                    AlchemyCueKind::Leak
                }
            }),
            failure.map_or(
                if final_step {
                    "The preparation reaches its declared stable batch state."
                } else {
                    "The measured process step completes."
                },
                |_| "The controls produce a deterministic named failure; every input remains in custody.",
            ),
            None,
        )
    }
}
