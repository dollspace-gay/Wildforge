//! Lifecycle workings transaction coordination.

use crate::workings::DeliveryMode;
use crate::workings::WorkingCue;
use crate::workings::WorkingCueKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingHandler;
use crate::workings::WorkingPhase;
use crate::workings::WorkingResult;
use crate::world::World;
use super::Settlement;
use super::effect_path;

impl World {
    /// Transition a settled channel from its charge-up presentation into an
    /// active host-owned process. This changes no Current custody.
    pub fn activate_working(&mut self, id: u64) -> Result<WorkingResult, String> {
        let before = self
            .workings_state
            .as_ref()
            .and_then(|state| state.active.get(&id))
            .ok_or_else(|| format!("Working {id} is not active."))?;
        if before.definition.mode == DeliveryMode::Wand
            && before.phase == WorkingPhase::Charging
            && self.working_tick().saturating_sub(before.started_tick)
                < crate::workings::MIN_WAND_SETTLE_TICKS
        {
            return Err(format!(
                "{} is still settling; its visible channel has not reached safe commitment.",
                before.definition.label
            ));
        }
        let transaction = {
            let state = self
                .workings_state
                .as_mut()
                .ok_or("The world has no workings authority.")?;
            state
                .phase(id, WorkingPhase::Active)
                .map_err(|error| error.to_string())?;
            state.save().map_err(|error| error.to_string())?;
            state
                .active
                .get(&id)
                .expect("phase checked active id")
                .clone()
        };
        let message = if transaction.definition.handler == WorkingHandler::Trace {
            self.trace_report(&transaction)?
        } else {
            format!(
                "{} settles into a stable working.",
                transaction.definition.label
            )
        };
        Ok(WorkingResult {
            success: true,
            stable_id: id,
            phase: Some(WorkingPhase::Active),
            cue: WorkingCueKind::Active,
            warning_band: transaction.strain.warning_band,
            message,
        })
    }

    pub fn complete_working(&mut self, id: u64) -> Result<WorkingResult, String> {
        self.settle_working(id, Settlement::Complete, None)
    }

    pub fn cancel_working(&mut self, id: u64) -> Result<WorkingResult, String> {
        self.settle_working(id, Settlement::Cancel, None)
    }

    pub fn interrupt_working(&mut self, id: u64) -> Result<WorkingResult, String> {
        self.settle_working(id, Settlement::Interrupt, None)
    }

    /// Settle every non-replay transaction still owned by one authenticated
    /// actor. Hosted death and disconnect both use this authority boundary so
    /// a stale connection-side channel pointer cannot strand a reservation.
    pub fn interrupt_actor_workings(
        &mut self,
        actor: [u8; 16],
    ) -> Result<Vec<WorkingResult>, String> {
        let ids = self
            .workings_state
            .as_ref()
            .into_iter()
            .flat_map(|state| state.active.values())
            .filter(|transaction| {
                transaction.actor == actor && transaction.phase != WorkingPhase::PendingApply
            })
            .map(|transaction| transaction.id)
            .collect::<Vec<_>>();
        ids.into_iter()
            .map(|id| self.interrupt_working(id))
            .collect()
    }

    /// Ritual release leaves the constructed process running; continuous and
    /// one-shot wand workings use release as their normal settlement edge.
    pub fn release_working(&mut self, id: u64) -> Result<WorkingResult, String> {
        let transaction = self
            .workings_state
            .as_ref()
            .and_then(|state| state.active.get(&id))
            .cloned()
            .ok_or_else(|| format!("Working {id} is not active."))?;
        if transaction.definition.mode == DeliveryMode::Ritual {
            if transaction.phase == WorkingPhase::Charging {
                return self.activate_working(id);
            }
            return Ok(WorkingResult {
                success: true,
                stable_id: id,
                phase: Some(transaction.phase),
                cue: WorkingCueKind::Active,
                warning_band: transaction.strain.warning_band,
                message: format!(
                    "{} remains embodied and continues on its physical schedule.",
                    transaction.definition.label
                ),
            });
        }
        if transaction.phase == WorkingPhase::Charging {
            if self.working_tick().saturating_sub(transaction.started_tick)
                < crate::workings::MIN_WAND_SETTLE_TICKS
            {
                return self.cancel_working(id);
            }
            self.activate_working(id)?;
        }
        self.complete_working(id)
    }

    /// Settle bounded-duration workings on the simulation clock. Rituals use
    /// this path while unloaded; a transient target that disappeared is
    /// interrupted deterministically instead of leaving orphan Current.
    pub fn tick_workings(&mut self) -> Vec<(WorkingResult, WorkingCue)> {
        let tick = self.working_tick();
        let active = self
            .workings_state
            .as_ref()
            .map(|state| {
                state
                    .active
                    .values()
                    .filter(|transaction| transaction.phase != WorkingPhase::PendingApply)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut outcomes = Vec::new();
        for transaction in active {
            let path = effect_path(&transaction.effect);
            let missing = path.iter().any(|pos| self.chunk(pos.chunk()).is_none());
            let due =
                transaction.due_tick > transaction.started_tick && transaction.due_tick <= tick;
            let broken_loaded_ritual = !missing
                && transaction.phase == WorkingPhase::Active
                && transaction.definition.mode == DeliveryMode::Ritual
                && self.validate_ritual_apparatus(&transaction).is_err();
            let should_interrupt = broken_loaded_ritual
                || (missing
                    && transaction.interruption
                        != crate::workings::InterruptionPolicy::ContinueUnloaded);
            if !due && !should_interrupt {
                continue;
            }
            if due
                && transaction.interruption == crate::workings::InterruptionPolicy::ContinueUnloaded
            {
                for pos in &path {
                    self.ensure_chunk(pos.chunk());
                }
            }
            let mut cue = self
                .working_cues()
                .into_iter()
                .find(|cue| cue.stable_id == transaction.id)
                .unwrap_or(WorkingCue {
                    stable_id: transaction.id,
                    working_id: transaction.definition.id.clone(),
                    handler: transaction.definition.handler,
                    source: transaction.source,
                    path: transaction.path.clone(),
                    kind: WorkingCueKind::Strain,
                    warning_band: transaction.strain.warning_band,
                    completion_permille: 0,
                });
            let result = if should_interrupt {
                self.interrupt_working(transaction.id)
            } else {
                match self.complete_working(transaction.id) {
                    Ok(result) => Ok(result),
                    Err(completion_error) => {
                        self.interrupt_working(transaction.id).map(|mut result| {
                            result.message = format!(
                                "{} could not complete: {completion_error} It interrupts with accounted dross.",
                                transaction.definition.label
                            );
                            result
                        })
                    }
                }
            };
            if let Ok(result) = result {
                cue.kind = result.cue;
                cue.warning_band = result.warning_band;
                cue.completion_permille = 1_000;
                outcomes.push((result, cue));
            }
        }
        outcomes
    }

    /// Resume the second half of a crash-safe completion. The Current was
    /// already settled atomically with `PendingApply`; typed world mutation is
    /// idempotent and the durable transaction is removed only after its chunks
    /// have landed.
    pub(in crate::world) fn replay_pending_workings(&mut self) -> Result<usize, String> {
        let pending = self
            .workings_state
            .as_ref()
            .map(|state| {
                state
                    .active
                    .values()
                    .filter(|transaction| transaction.phase == WorkingPhase::PendingApply)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut replayed = 0usize;
        for transaction in pending {
            if matches!(transaction.effect, WorkingEffect::RepairItem { .. }) {
                // Player profiles are loaded by their authenticated session
                // owner. The pending transaction is the write-ahead proof;
                // that adapter applies/recognises the exact inventory after
                // state and then calls `finish_inventory_working`.
                continue;
            }
            for pos in effect_path(&transaction.effect) {
                self.ensure_chunk(pos.chunk());
            }
            self.apply_working_effect(&transaction.effect, &transaction.targets)?;
            self.save_effect_chunks(&transaction.effect)?;
            self.save_effect_sidecars(&transaction.effect)?;
            let tick = self.working_tick();
            let state = self
                .workings_state
                .as_mut()
                .ok_or("Pending working lost its authority state.")?;
            state
                .settle(transaction.id, "crash_replay_complete", tick)
                .map_err(|error| error.to_string())?;
            state.save().map_err(|error| error.to_string())?;
            replayed += 1;
        }
        Ok(replayed)
    }

    /// A held wand channel has no owner after a process restart: mouse/key
    /// state and authenticated session custody are intentionally transient.
    /// Settle those channels through their declared interruption rule while
    /// leaving embodied rituals and write-ahead PendingApply work untouched.
    pub(in crate::world) fn interrupt_loaded_wand_workings(&mut self) -> Result<usize, String> {
        let ids = self
            .workings_state
            .as_ref()
            .into_iter()
            .flat_map(|state| state.active.values())
            .filter(|transaction| {
                transaction.definition.mode == DeliveryMode::Wand
                    && transaction.phase != WorkingPhase::PendingApply
            })
            .map(|transaction| transaction.id)
            .collect::<Vec<_>>();
        for id in &ids {
            self.interrupt_working(*id)?;
        }
        Ok(ids.len())
    }
}
