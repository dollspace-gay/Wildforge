//! Status tick alchemy transaction coordination.

use crate::alchemy::AlchemyCue;
use crate::alchemy::AlchemyCueKind;
use crate::arcane::ArcaneOwner;
use std::collections::BTreeMap;
use crate::world::BlockPos;
use crate::arcane::Current;
use crate::alchemy::PreparationHandler;
use crate::alchemy::PreparationModifiers;
use crate::alchemy::PreparationPhysiology;
use crate::alchemy::PreparationTickResult;
use crate::world::World;
use super::add_current_map;
use super::debit_nutrition;

impl World {
    /// Advance one actor's authoritative timed effects. Local and remote
    /// callers pass the same physiological snapshot and receive the same
    /// bounded result; no client computes healing, vision, or wand bonuses.
    pub fn tick_preparation_statuses(
        &mut self,
        actor: [u8; 16],
        actor_pos: BlockPos,
        mut physiology: PreparationPhysiology,
    ) -> Result<PreparationTickResult, String> {
        // Atlas-free fixture/remote worlds do not own planetary water,
        // Current, or an alchemy sidecar. Their ordinary survival tick is a
        // legitimate no-op here, not a once-per-second error. A qualified
        // atlas-backed world missing its sidecar still fails closed below.
        if self.alchemy_state.is_none() && self.planet_atlas.is_none() {
            return Ok(PreparationTickResult {
                physiology,
                modifiers: PreparationModifiers::default(),
                cues: Vec::new(),
            });
        }
        let now = self.alchemy_tick();
        let environmental_dross = self.environmental_dross_band_at(actor_pos);
        // Validate every definition reference before touching the first
        // status. A corrupt later entry must not leave earlier physiology or
        // progress partially advanced on the direct (non-settlement) path.
        let authoritative_state = self
            .alchemy_state
            .as_ref()
            .ok_or("The authoritative alchemy state is unavailable.")?;
        if let Some(statuses) = authoritative_state.statuses.get(&actor) {
            for status in statuses {
                let definition = self
                    .reg
                    .preparations
                    .get(&status.preparation_id)
                    .ok_or("An active preparation definition is unavailable.")?;
                if definition.version != status.definition_version {
                    return Err(
                        "An active preparation definition version changed underneath its status."
                            .into(),
                    );
                }
            }
        }
        // Most actor ticks only advance bounded physiology and modifiers. Do
        // that in place so one active draught does not clone the entire world
        // census every second. Expiry is rarer and moves Current through the
        // linked ledger, so retain the old copy-on-commit transaction for that
        // path: a failed ledger write then leaves both custody records intact.
        let needs_staged_settlement = self
            .alchemy_state
            .as_ref()
            .and_then(|state| state.statuses.get(&actor))
            .is_some_and(|statuses| {
                statuses.iter().any(|status| {
                    now >= status.due_tick
                        && (!status.active_current.is_empty() || !status.dross_current.is_empty())
                })
            });
        let mut staged_state = needs_staged_settlement.then(|| {
            self.alchemy_state
                .clone()
                .expect("settlement preflight observed authoritative alchemy state")
        });
        let state = if let Some(state) = staged_state.as_mut() {
            state
        } else {
            self.alchemy_state
                .as_mut()
                .ok_or("The authoritative alchemy state is unavailable.")?
        };
        let mut modifiers = PreparationModifiers::default();
        let mut cues = Vec::new();
        let mut settlements = Vec::<(ArcaneOwner, Current, ArcaneOwner, Current, String)>::new();
        if let Some(statuses) = state.statuses.get_mut(&actor) {
            for status in statuses.iter_mut() {
                let definition = self
                    .reg
                    .preparations
                    .get(&status.preparation_id)
                    .expect("status definitions were preflight before mutation");
                debug_assert_eq!(definition.version, status.definition_version);
                let from_tick = status.last_tick.max(status.started_tick).min(now);
                let active_until = now.min(status.due_tick);
                if now < status.due_tick {
                    match definition.handler {
                        PreparationHandler::TraceSight => {
                            modifiers.trace_sight = modifiers
                                .trace_sight
                                .max(definition.effect.strength.min(u32::from(u16::MAX)) as u16);
                        }
                        PreparationHandler::NaturalRecovery => {
                            let cap_milli = u64::from(definition.effect.strength) * 1_000;
                            let span = status.due_tick.saturating_sub(status.started_tick).max(1);
                            let elapsed = active_until.saturating_sub(status.started_tick);
                            let desired = u64::try_from(
                                u128::from(cap_milli) * u128::from(elapsed) / u128::from(span),
                            )
                            .unwrap_or(cap_milli)
                            .min(cap_milli);
                            let wanted = desired.saturating_sub(status.completed_units);
                            let health_room_milli =
                                ((physiology.max_health - physiology.health).max(0.0) * 1_000.0)
                                    .floor() as u64;
                            let starving_room = if physiology.hunger <= 0.01 {
                                ((2.0 - physiology.health).max(0.0) * 1_000.0).floor() as u64
                            } else {
                                u64::MAX
                            };
                            let hunger_per_heal = if cap_milli == 0 {
                                0.0
                            } else {
                                f64::from(definition.effect.hunger_cost_milli)
                                    / cap_milli as f64
                                    / 1_000.0
                            };
                            let hunger_room = if hunger_per_heal == 0.0 {
                                u64::MAX
                            } else {
                                (f64::from(physiology.hunger.max(0.0)) / hunger_per_heal).floor()
                                    as u64
                            };
                            let nutrition_total = physiology
                                .nutrition
                                .iter()
                                .copied()
                                .map(|value| value.max(0.0))
                                .sum::<f32>();
                            let nutrient_per_heal = if cap_milli == 0 {
                                0.0
                            } else {
                                f64::from(definition.effect.nutrient_cost) / cap_milli as f64
                            };
                            let nutrient_room = if nutrient_per_heal == 0.0 {
                                u64::MAX
                            } else {
                                (f64::from(nutrition_total) / nutrient_per_heal).floor() as u64
                            };
                            let healed_milli = wanted
                                .min(health_room_milli)
                                .min(starving_room)
                                .min(hunger_room)
                                .min(nutrient_room);
                            if healed_milli != 0 {
                                physiology.health = (physiology.health
                                    + healed_milli as f32 / 1_000.0)
                                    .min(physiology.max_health);
                                physiology.hunger = (physiology.hunger
                                    - (healed_milli as f64 * hunger_per_heal) as f32)
                                    .max(0.0);
                                debit_nutrition(
                                    &mut physiology.nutrition,
                                    (healed_milli as f64 * nutrient_per_heal) as f32,
                                );
                                status.completed_units = status
                                    .completed_units
                                    .saturating_add(healed_milli)
                                    .min(cap_milli);
                            }
                            let sickness_until = active_until.min(status.overdose_until_tick);
                            let sickness_from = from_tick.min(sickness_until);
                            let sickness_elapsed = sickness_until.saturating_sub(sickness_from);
                            if sickness_elapsed != 0 {
                                physiology.hunger = (physiology.hunger
                                    - sickness_elapsed.min(600) as f32 / 600.0 * 0.5)
                                    .max(0.0);
                            }
                        }
                        PreparationHandler::StrainRelief => {
                            modifiers.strain_permille = modifiers.strain_permille.min(
                                1_000u16.saturating_sub(definition.effect.strength.min(750) as u16),
                            );
                            modifiers.throughput_permille = modifiers
                                .throughput_permille
                                .min(definition.effect.throughput_permille);
                        }
                        PreparationHandler::ThroughputSurge => {
                            modifiers.throughput_permille = modifiers
                                .throughput_permille
                                .max(definition.effect.throughput_permille);
                            modifiers.drain_permille = modifiers
                                .drain_permille
                                .max(definition.effect.drain_permille);
                            modifiers.overdraw_permille = modifiers
                                .overdraw_permille
                                .max(definition.effect.overdraw_permille);
                            modifiers.storm_warning = true;
                            if from_tick / 40 != active_until / 40 {
                                cues.push(AlchemyCue {
                                    pos: actor_pos,
                                    installation_id: 0,
                                    batch_id: status.source_batch,
                                    revision: status.status_id,
                                    kind: AlchemyCueKind::Pulse,
                                    intensity: 210,
                                    color: [110, 180, 255],
                                    message: "Storm cordial pulses: throughput, drain, and overdraw are all elevated.".into(),
                                });
                            }
                        }
                        PreparationHandler::DrossAntidote => {
                            let capacity = u64::from(definition.effect.dross_capacity);
                            let span = status.due_tick.saturating_sub(status.started_tick).max(1);
                            let elapsed = active_until.saturating_sub(status.started_tick);
                            let desired = u64::try_from(
                                u128::from(capacity) * u128::from(elapsed) / u128::from(span),
                            )
                            .unwrap_or(capacity)
                            .min(capacity);
                            let moved = desired
                                .saturating_sub(status.completed_units)
                                .min(physiology.bodily_dross);
                            physiology.bodily_dross -= moved;
                            status.completed_units = status.completed_units.saturating_add(moved);
                        }
                        PreparationHandler::RootUptake
                        | PreparationHandler::DrossWash
                        | PreparationHandler::PreserveSpecimen => {}
                    }
                }
                status.last_tick = now.min(status.recovery_until_tick);
                if now >= status.due_tick
                    && (!status.active_current.is_empty() || !status.dross_current.is_empty())
                {
                    let Some(atlas) = self.planet_atlas.as_ref() else {
                        return Err(
                            "Status settlement needs the authoritative planet atlas.".into()
                        );
                    };
                    let region = atlas.atlas_pos(actor_pos.surface());
                    let owner_id = crate::alchemy::status_owner_id(status.status_id);
                    settlements.push((
                        ArcaneOwner::Alchemy(owner_id),
                        std::mem::take(&mut status.active_current),
                        ArcaneOwner::AlchemyDross(owner_id),
                        std::mem::take(&mut status.dross_current),
                        status.preparation_id.clone(),
                    ));
                    let _ = region;
                }
            }
            statuses.retain(|status| {
                now < status.recovery_until_tick
                    || !status.active_current.is_empty()
                    || !status.dross_current.is_empty()
            });
        }
        let operation_id = if settlements.is_empty() {
            None
        } else {
            Some(
                state
                    .allocate_operation_id()
                    .map_err(|error| error.to_string())?,
            )
        };
        self.apply_environmental_dross_exposure(
            actor,
            actor_pos,
            now,
            environmental_dross,
            &mut physiology,
            &mut modifiers,
        );
        // Trace evidence is earned through a lens/preparation; unaided
        // players first receive a direct categorical warning at strained.
        if modifiers.dross_band == crate::dross::DrossBand::Trace.ordinal()
            && modifiers.trace_sight == 0
        {
            modifiers.dross_band = 0;
            modifiers.dross_pattern = 0;
        }
        let result = PreparationTickResult {
            physiology,
            modifiers,
            cues,
        };
        if let Some(operation_id) = operation_id {
            let atlas = self
                .planet_atlas
                .as_ref()
                .ok_or("Status settlement needs the authoritative planet atlas.")?;
            let region = atlas.atlas_pos(actor_pos.surface());
            let mut debits = Vec::new();
            let mut credit_map = BTreeMap::<ArcaneOwner, Current>::new();
            for (clean_owner, clean, dross_owner, dross, _) in settlements {
                if !clean.is_empty() {
                    debits.push((clean_owner, clean.clone()));
                    add_current_map(&mut credit_map, ArcaneOwner::Ambient(region), &clean)?;
                }
                if !dross.is_empty() {
                    debits.push((dross_owner, dross.clone()));
                    add_current_map(
                        &mut credit_map,
                        ArcaneOwner::Dross {
                            region,
                            medium: crate::arcane::DrossMedium::Water,
                        },
                        &dross,
                    )?;
                }
            }
            self.commit_alchemy_current(
                staged_state
                    .take()
                    .expect("Current settlement was staged on a private state copy"),
                operation_id,
                "base:preparation_status_settlement",
                "completed preparation statuses returned clean Current and retained dross",
                debits,
                credit_map
                    .into_iter()
                    .map(|(owner, current)| (owner, current, None))
                    .collect(),
            )?;
        }
        Ok(result)
    }

    pub fn settle_preparations_on_death(
        &mut self,
        actor: [u8; 16],
        actor_pos: BlockPos,
    ) -> Result<(), String> {
        let now = self.alchemy_tick();
        if let Some(state) = &mut self.alchemy_state
            && let Some(statuses) = state.statuses.get_mut(&actor)
        {
            for status in statuses {
                status.due_tick = status.due_tick.min(now);
                status.recovery_until_tick = status.recovery_until_tick.min(now);
            }
        }
        let _ =
            self.tick_preparation_statuses(actor, actor_pos, PreparationPhysiology::default())?;
        Ok(())
    }
}
