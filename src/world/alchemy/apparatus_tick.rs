//! Apparatus tick alchemy transaction coordination.

use crate::alchemy::AlchemyCue;
use crate::alchemy::AlchemyCueKind;
use crate::alchemy::AlchemyRequest;
use crate::alchemy::ApparatusAction;
use crate::alchemy::ApparatusKind;
use crate::arcane::ArcaneOwner;
use std::collections::BTreeMap;
use crate::alchemy::BatchOutcome;
use crate::planet::BlockPos;
use crate::arcane::Current;
use crate::alchemy::DisposalRoute;
use crate::inventory::Inventory;
use crate::world::World;
use super::add_current_map;
use super::next_block_key;
use super::next_u64_key;

impl World {
    /// Bounded round-robin background maintenance: spoilage, deterministic
    /// cracked/overheated apparatus leaks, root-treatment expiry, and coating
    /// Current settlement. Persisted cursors ensure a large population cannot
    /// starve entries that sort after the first budget-sized prefix.
    pub fn tick_alchemy(&mut self, budget: usize) -> Result<Vec<AlchemyCue>, String> {
        if self.alchemy_state.is_none() && self.planet_atlas.is_none() {
            return Ok(Vec::new());
        }
        let now = self.alchemy_tick();
        let state = self
            .alchemy_state
            .as_mut()
            .ok_or("The authoritative alchemy state is unavailable.")?;
        let mut cues = Vec::new();
        let budget = budget.max(1);
        let phase = state.maintenance_phase;
        state.maintenance_phase = (phase + 1) % 4;
        let mut leaks = Vec::<(BlockPos, DisposalRoute)>::new();
        let mut expired = Vec::new();

        match phase {
            0 => {
                let visits = budget.min(state.apparatus.len());
                for _ in 0..visits {
                    let Some(pos) =
                        next_block_key(&state.apparatus, state.maintenance_apparatus_cursor)
                    else {
                        break;
                    };
                    state.maintenance_apparatus_cursor = Some(pos);
                    let apparatus = state
                        .apparatus
                        .get_mut(&pos)
                        .expect("round-robin apparatus key remained present");
                    let mut leak_route = None;
                    if let Some(batch) = apparatus.batch.as_mut() {
                        if now >= batch.expires_tick
                            && matches!(
                                batch.outcome,
                                BatchOutcome::Processing | BatchOutcome::Ready
                            )
                        {
                            batch.outcome = BatchOutcome::Spoiled;
                            batch.revision = batch.revision.saturating_add(1);
                            apparatus.revision = apparatus.revision.saturating_add(1);
                            cues.push(AlchemyCue {
                                pos: apparatus.pos,
                                installation_id: apparatus.installation_id,
                                batch_id: batch.id,
                                revision: apparatus.revision,
                                kind: AlchemyCueKind::Spoil,
                                intensity: 120,
                                color: [120, 90, 130],
                                message: "A preparation ages into its named spent-liquor state; nothing vanishes.".into(),
                            });
                        }
                        let excessive_heat = self
                            .reg
                            .preparations
                            .get(&batch.preparation_id)
                            .is_some_and(|definition| {
                                apparatus.temperature_millic
                                    > definition.temperature_millic[1].saturating_add(20_000)
                            });
                        if apparatus.integrity_permille <= 250 || excessive_heat {
                            leak_route = Some(
                                if apparatus.kind == ApparatusKind::Alembic || excessive_heat {
                                    DisposalRoute::Air
                                } else {
                                    DisposalRoute::Runoff
                                },
                            );
                        }
                    }
                    if let Some(route) = leak_route {
                        leaks.push((pos, route));
                    }
                }
            }
            1 => {
                let visits = budget.min(state.containers.len());
                for _ in 0..visits {
                    let Some(id) =
                        next_u64_key(&state.containers, state.maintenance_container_cursor)
                    else {
                        break;
                    };
                    state.maintenance_container_cursor = id;
                    let dose = state
                        .containers
                        .get_mut(&id)
                        .expect("round-robin container key remained present");
                    if now >= dose.expires_tick && dose.outcome == BatchOutcome::Ready {
                        dose.outcome = BatchOutcome::Spoiled;
                    }
                }
            }
            2 => {
                let visits = budget.min(state.root_treatments.len());
                for _ in 0..visits {
                    let Some(pos) =
                        next_block_key(&state.root_treatments, state.maintenance_root_cursor)
                    else {
                        break;
                    };
                    state.maintenance_root_cursor = Some(pos);
                    if state
                        .root_treatments
                        .get(&pos)
                        .is_some_and(|treatment| now >= treatment.expires_tick)
                    {
                        state.root_treatments.remove(&pos);
                    }
                }
            }
            3 => {
                let visits = budget.min(state.coatings.len());
                for _ in 0..visits {
                    let Some(item_id) =
                        next_u64_key(&state.coatings, state.maintenance_coating_cursor)
                    else {
                        break;
                    };
                    state.maintenance_coating_cursor = item_id;
                    if let Some(coating) = state
                        .coatings
                        .get(&item_id)
                        .filter(|coating| now >= coating.expires_tick)
                        .cloned()
                    {
                        expired.push((item_id, coating));
                    }
                }
            }
            _ => unreachable!("validated alchemy maintenance phase"),
        }

        for (pos, route) in leaks {
            let Some(apparatus) = self
                .alchemy_state
                .as_ref()
                .and_then(|state| state.apparatus.get(&pos))
                .cloned()
            else {
                continue;
            };
            let mut empty_inventory = Inventory::new();
            let result = self.operate_alchemy(
                pos,
                &mut empty_inventory,
                AlchemyRequest {
                    actor: apparatus.last_operator,
                    actor_label: "apparatus leak".into(),
                    expected_revision: Some(apparatus.revision),
                    action: ApparatusAction::Drain { route },
                },
            )?;
            let mut cue = result.cue;
            cue.message = "A cracked or dangerously overheated apparatus leaks through an ordinary environmental disposal path; its contents remain accounted.".into();
            cues.push(cue);
        }

        if expired.is_empty() {
            return Ok(cues);
        }
        // Coating expiry is its own phase, so no leak operation can have
        // replaced this snapshot while its linked Current settlement is built.
        let mut state = self
            .alchemy_state
            .clone()
            .ok_or("The authoritative alchemy state disappeared during maintenance.")?;
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let mut debits = Vec::new();
        let mut credit_map = BTreeMap::<ArcaneOwner, Current>::new();
        for (item_id, coating) in expired {
            state.coatings.remove(&item_id);
            let Some(atlas) = self.planet_atlas.as_ref() else {
                return Err("Coating settlement needs the authoritative atlas.".into());
            };
            let region = atlas.atlas_pos(coating.applied_pos.surface());
            let owner_id = crate::alchemy::status_owner_id(coating.status_id);
            for (owner, destination) in [
                (ArcaneOwner::Alchemy(owner_id), ArcaneOwner::Ambient(region)),
                (
                    ArcaneOwner::AlchemyDross(owner_id),
                    ArcaneOwner::Dross {
                        region,
                        medium: crate::arcane::DrossMedium::Soil,
                    },
                ),
            ] {
                if let Some(current) = self
                    .arcane_ledger
                    .as_ref()
                    .and_then(|ledger| ledger.account(&owner))
                    .map(|account| account.current.clone())
                {
                    debits.push((owner, current.clone()));
                    add_current_map(&mut credit_map, destination, &current)?;
                }
            }
        }
        self.commit_alchemy_current(
            state,
            operation_id,
            "base:frostlace_suspension",
            "expired specimen coatings settled their finite Current",
            debits,
            credit_map
                .into_iter()
                .map(|(owner, current)| (owner, current, None))
                .collect(),
        )?;
        Ok(cues)
    }
}
