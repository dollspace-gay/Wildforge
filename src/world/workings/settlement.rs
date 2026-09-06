//! Settlement workings transaction coordination.

use crate::arcane::ArcaneOwner;
use std::collections::BTreeMap;
use crate::arcane::Current;
use crate::workings::DeliveryMode;
use crate::implements::ImplementAuditEvent;
use crate::arcane::LinkedFileReplacement;
use crate::workings::NudgeEntityKind;
use crate::workings::StrainInputs;
use crate::workings::WorkingApparatus;
use crate::workings::WorkingCueKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingHandler;
use crate::workings::WorkingPhase;
use crate::workings::WorkingResult;
use crate::workings::WorkingTargetSnapshot;
use crate::world::World;
use super::Settlement;
use super::dross_medium;

impl World {
    pub(super) fn settle_working(
        &mut self,
        id: u64,
        settlement: Settlement,
        mut inventory: Option<&mut crate::inventory::Inventory>,
    ) -> Result<WorkingResult, String> {
        if settlement == Settlement::Complete {
            self.refresh_nudge_velocity(id)?;
        }
        let transaction = self
            .workings_state
            .as_ref()
            .and_then(|state| state.active.get(&id))
            .cloned()
            .ok_or_else(|| format!("Working {id} is not active."))?;
        if transaction.phase == WorkingPhase::PendingApply {
            if matches!(transaction.effect, WorkingEffect::RepairItem { .. }) {
                let inventory = inventory
                    .as_deref_mut()
                    .ok_or("Fieldmend is waiting for its authoritative player profile.")?;
                self.apply_inventory_effect(&transaction, inventory)?;
                return Ok(WorkingResult {
                    success: true,
                    stable_id: id,
                    phase: Some(WorkingPhase::PendingApply),
                    cue: WorkingCueKind::Complete,
                    warning_band: transaction.strain.warning_band,
                    message: format!(
                        "{} has landed; its profile checkpoint is pending.",
                        transaction.definition.label
                    ),
                });
            }
            self.apply_working_effect(&transaction.effect, &transaction.targets)?;
            self.save_effect_chunks(&transaction.effect)?;
            self.save_effect_sidecars(&transaction.effect)?;
            let tick = self.working_tick();
            let state = self
                .workings_state
                .as_mut()
                .expect("transaction came from state");
            state
                .settle(id, "completion_retried", tick)
                .map_err(|error| error.to_string())?;
            state.save().map_err(|error| error.to_string())?;
            return Ok(WorkingResult {
                success: true,
                stable_id: id,
                phase: None,
                cue: WorkingCueKind::Complete,
                warning_band: transaction.strain.warning_band,
                message: format!("{} completes exactly once.", transaction.definition.label),
            });
        }
        if settlement == Settlement::Complete {
            if transaction.definition.mode == DeliveryMode::Ritual {
                self.validate_ritual_apparatus(&transaction)?;
            }
            if matches!(transaction.effect, WorkingEffect::RepairItem { .. }) {
                let inventory = inventory
                    .as_deref_mut()
                    .ok_or("Fieldmend must complete through an authoritative inventory adapter.")?;
                self.validate_inventory_effect(&transaction, inventory)?;
            } else {
                self.validate_working_effect_before(&transaction.effect)?;
            }
        }
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(transaction.source.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let mut next_state = self
            .workings_state
            .clone()
            .ok_or("The world has no workings authority.")?;
        let mut settled = next_state
            .active
            .get(&id)
            .cloned()
            .expect("cloned active transaction");
        if settlement != Settlement::Complete {
            let extra = crate::workings::deterministic_strain(StrainInputs {
                safe_throughput: settled.definition.safe_throughput.max(1),
                local_capacity_permille: 1_000,
                interruption: true,
                ..StrainInputs::default()
            })
            .map_err(|error| error.to_string())?;
            let convert = extra.extra_dross.min(settled.return_current.total());
            if convert != 0 {
                let moved = settled
                    .return_current
                    .take_units(convert, std::iter::empty())
                    .map_err(|error| error.to_string())?;
                settled
                    .dross_current
                    .checked_add(&moved)
                    .map_err(|error| error.to_string())?;
            }
            settled.strain.strain = settled.strain.strain.saturating_add(extra.strain);
            settled.strain.extra_dross =
                settled.strain.extra_dross.saturating_add(extra.extra_dross);
            settled.strain.warning_band = settled.strain.warning_band.max(extra.warning_band);
            next_state.active.insert(id, settled.clone());
        }
        let settlement_tick = self.working_tick();
        let staged_material = if settlement == Settlement::Complete
            && matches!(settled.effect, WorkingEffect::RepairItem { .. })
        {
            self.stage_fieldmend_material_loss(&settled.effect)?
        } else {
            None
        };
        let mut settling_destination = None;
        let mut settling_wear = None;
        if settlement == Settlement::Complete
            && settled.definition.handler != WorkingHandler::SettlingRite
        {
            let rite = next_state
                .active
                .values()
                .filter(|candidate| {
                    candidate.phase == WorkingPhase::Active
                        && matches!(
                            candidate.effect,
                            WorkingEffect::Settle { process_id, .. } if process_id == id
                        )
                        && candidate.targets.iter().any(|target| {
                            matches!(
                                target,
                                WorkingTargetSnapshot::Entity {
                                    stable_id,
                                    kind,
                                    version,
                                } if *stable_id == id
                                    && kind == "magical_process"
                                    && *version == settled.completion_nonce
                            )
                        })
                })
                .min_by_key(|candidate| candidate.id)
                .cloned();
            if let Some(rite) = rite {
                let WorkingEffect::Settle {
                    dross_vessel_id, ..
                } = &rite.effect
                else {
                    unreachable!("settling candidate was matched above")
                };
                let dross_vessel_id = *dross_vessel_id;
                let vessel_capacity = self
                    .implements_state
                    .as_ref()
                    .and_then(|state| state.instance(dross_vessel_id))
                    .map(|instance| instance.kind.capacity())
                    .unwrap_or_default();
                let vessel_load = self
                    .arcane_ledger
                    .as_ref()
                    .and_then(|ledger| ledger.account(&ArcaneOwner::Item(dross_vessel_id)))
                    .map_or(0, |account| account.current.total())
                    .saturating_add(
                        self.arcane_ledger
                            .as_ref()
                            .and_then(|ledger| {
                                ledger.account(&ArcaneOwner::ItemDross(dross_vessel_id))
                            })
                            .map_or(0, |account| account.current.total()),
                    );
                if self.validate_ritual_apparatus(&rite).is_ok()
                    && vessel_load.saturating_add(settled.dross_current.total()) <= vessel_capacity
                {
                    // Stabilization trades throughput for a deterministic 25%
                    // reduction in disordered output. The recovered mixture
                    // remains clean return Current; every remaining unit is
                    // credited to the real mounted dross vessel below.
                    let recovered_units = settled.dross_current.total() / 4;
                    if recovered_units != 0 {
                        let recovered = settled
                            .dross_current
                            .take_units(recovered_units, std::iter::empty())
                            .map_err(|error| error.to_string())?;
                        settled
                            .return_current
                            .checked_add(&recovered)
                            .map_err(|error| error.to_string())?;
                    }
                    let routed = settled.dross_current.total();
                    settling_destination = Some(ArcaneOwner::ItemDross(dross_vessel_id));
                    let wear = u16::try_from(routed.div_ceil(16)).unwrap_or(u16::MAX);
                    settling_wear = Some((dross_vessel_id, u32::from(wear)));
                    if let Some(active_rite) = next_state.active.get_mut(&rite.id)
                        && let WorkingEffect::Settle {
                            dross_routed,
                            stabilizer_wear,
                            ..
                        } = &mut active_rite.effect
                    {
                        *dross_routed = dross_routed.saturating_add(routed);
                        *stabilizer_wear = stabilizer_wear.saturating_add(wear);
                    }
                    next_state.active.insert(id, settled.clone());
                } else if let Some(active_rite) = next_state.active.get_mut(&rite.id) {
                    active_rite.strain.warning_band = 3;
                    active_rite.strain.strain = active_rite.strain.strain.saturating_add(250);
                }
            }
        }
        let staged_loose_items = if settlement == Settlement::Complete
            && matches!(
                settled.effect,
                WorkingEffect::Impulse {
                    entity_kind: NudgeEntityKind::DroppedItem,
                    ..
                }
            ) {
            let after = self
                .encode_loose_items()
                .map_err(|error| error.to_string())?;
            let path = self.save_dir.join("loose-items.toml");
            let before = match std::fs::read(path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                Err(error) => return Err(error.to_string()),
            };
            (before != after).then_some(after)
        } else {
            None
        };
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no finite Current ledger.")?;
        let mut debits = BTreeMap::new();
        crate::world::implements::add_current(
            &mut debits,
            ArcaneOwner::Working(id),
            &settled.reserved_current,
        )
        .map_err(|error| error.to_string())?;
        let mut credits = BTreeMap::new();
        let mut ordinary_return = settled.return_current.clone();
        let preserve_spent_units = match settled.effect {
            WorkingEffect::Preserve {
                charge_spent_units, ..
            } => charge_spent_units.min(ordinary_return.total()),
            _ => 0,
        };
        if preserve_spent_units != 0 {
            let spent = ordinary_return
                .take_units(preserve_spent_units, [settled.definition.focus.clone()])
                .map_err(|error| error.to_string())?;
            crate::world::implements::add_current(&mut credits, ArcaneOwner::Ambient(region), &spent)
                .map_err(|error| error.to_string())?;
        }
        if settlement == Settlement::Complete
            && let WorkingEffect::TransferCurrent { to, current, .. } = &settled.effect
        {
            ordinary_return
                .checked_sub(current)
                .map_err(|error| error.to_string())?;
            crate::world::implements::add_current(&mut credits, to.clone(), current)
                .map_err(|error| error.to_string())?;
        }
        if !ordinary_return.is_empty() {
            let refunds_original_sources = settlement != Settlement::Complete
                || matches!(settled.effect, WorkingEffect::Preserve { .. });
            if refunds_original_sources {
                // Refund only into the accounts that actually supplied the
                // reservation, bounded by each exact debit. This prevents an
                // interrupted ambient-assisted channel from overfilling the
                // wand while preserving the conserved resonance mixture.
                for debit in &settled.current_debits {
                    let amount = ordinary_return.total().min(debit.current.total());
                    if amount == 0 {
                        break;
                    }
                    let refunded = ordinary_return
                        .take_units(amount, debit.current.parts().keys().cloned())
                        .map_err(|error| error.to_string())?;
                    crate::world::implements::add_current(&mut credits, debit.owner.clone(), &refunded)
                        .map_err(|error| error.to_string())?;
                }
            } else {
                crate::world::implements::add_current(
                    &mut credits,
                    ArcaneOwner::Ambient(region),
                    &ordinary_return,
                )
                .map_err(|error| error.to_string())?;
                ordinary_return = Current::default();
            }
            if !ordinary_return.is_empty() {
                crate::world::implements::add_current(
                    &mut credits,
                    ArcaneOwner::Ambient(region),
                    &ordinary_return,
                )
                .map_err(|error| error.to_string())?;
            }
        }
        if !settled.dross_current.is_empty() {
            crate::world::implements::add_current(
                &mut credits,
                settling_destination.unwrap_or(ArcaneOwner::Dross {
                    region,
                    medium: dross_medium(settled.definition.handler),
                }),
                &settled.dross_current,
            )
            .map_err(|error| error.to_string())?;
        }
        let mut next_implements = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let operation_id = next_implements
            .operation_id()
            .map_err(|error| error.to_string())?;
        if let WorkingApparatus::Wand { instance_id, .. } = settled.apparatus
            && let Some(instance) = next_implements.instances.get_mut(&instance_id)
        {
            instance.wear = instance
                .wear
                .saturating_add(u32::from(settled.definition.wear))
                .min(crate::implements::MAX_WAND_WEAR);
            instance.strain = instance
                .strain
                .saturating_add(settled.strain.strain)
                .min(crate::implements::MAX_WAND_STRAIN);
        }
        let ritual_instance_id = if matches!(settled.apparatus, WorkingApparatus::Ritual { .. }) {
            settled
                .current_debits
                .iter()
                .find_map(|debit| match debit.owner {
                    ArcaneOwner::Item(instance_id) => Some(instance_id),
                    _ => None,
                })
        } else {
            None
        };
        if let Some(instance_id) = ritual_instance_id
            && let Some(instance) = next_implements.instances.get_mut(&instance_id)
        {
            instance.wear = instance
                .wear
                .saturating_add(u32::from(settled.definition.wear))
                .min(crate::implements::MAX_WAND_WEAR);
            instance.strain = instance
                .strain
                .saturating_add(settled.strain.strain / 2)
                .min(crate::implements::MAX_WAND_STRAIN);
        }
        if let Some((vessel_id, wear)) = settling_wear
            && let Some(instance) = next_implements.instances.get_mut(&vessel_id)
        {
            // The dross vessel is the rite's real stabilizer. Its embodied
            // implement record wears in the same linked commit that granted
            // the target process its reduced/routed dross, so a crash cannot
            // keep the benefit while losing the physical cost.
            instance.wear = instance
                .wear
                .saturating_add(wear)
                .min(crate::implements::MAX_WAND_WEAR);
            instance.strain = instance
                .strain
                .saturating_add(wear.saturating_mul(8))
                .min(crate::implements::MAX_WAND_STRAIN);
        }
        next_implements.record(ImplementAuditEvent {
            operation_id,
            kind: format!("working_{settlement:?}").to_ascii_lowercase(),
            instance_id: match settled.apparatus {
                WorkingApparatus::Wand { instance_id, .. } => instance_id,
                WorkingApparatus::Ritual { .. } => ritual_instance_id.unwrap_or_default(),
            },
            units: settled.reserved_current.total(),
            dross: settled.dross_current.total(),
            actor: settled.actor_label.clone(),
            note: settled.trace.clone(),
        });
        if settlement == Settlement::Complete {
            next_state
                .phase(id, WorkingPhase::PendingApply)
                .map_err(|error| error.to_string())?;
        } else {
            next_state
                .settle(
                    id,
                    if settlement == Settlement::Cancel {
                        "cancelled"
                    } else {
                        "interrupted"
                    },
                    settlement_tick,
                )
                .map_err(|error| error.to_string())?;
        }
        let arcane = crate::world::implements::transaction_from_maps(
            ledger,
            debits,
            credits,
            &settled.definition.id,
            if settlement == Settlement::Complete {
                "working settled Current before typed effect replay"
            } else {
                "working interruption settled its declared refund and dross"
            },
        )?;
        let mut replacements = vec![
            LinkedFileReplacement {
                subsystem: "workings".into(),
                operation_id,
                relative_path: crate::workings::WORKINGS_FILE.into(),
                after: Some(next_state.encode().map_err(|error| error.to_string())?),
            },
            LinkedFileReplacement {
                subsystem: "implements".into(),
                operation_id,
                relative_path: crate::implements::IMPLEMENTS_FILE.into(),
                after: Some(
                    next_implements
                        .encode()
                        .map_err(|error| error.to_string())?,
                ),
            },
        ];
        if let Some((_, bytes)) = &staged_material {
            replacements.push(LinkedFileReplacement {
                subsystem: "materials".into(),
                operation_id,
                relative_path: crate::materials::MaterialLedger::linked_delta_path().into(),
                after: Some(bytes.clone()),
            });
        }
        if let Some(staged_loose_items) = staged_loose_items {
            // The PendingApply journal and the exact loose-entity before
            // state land together. A crash can therefore replay the impulse
            // once instead of losing the transient target or guessing.
            replacements.push(LinkedFileReplacement {
                subsystem: "loose_items".into(),
                operation_id,
                relative_path: "loose-items.toml".into(),
                after: Some(staged_loose_items),
            });
        }
        ledger
            .commit_linked_files(arcane, replacements)
            .map_err(|error| error.to_string())?;
        self.workings_state = Some(next_state);
        self.implements_state = Some(next_implements);
        if let Some((next_material, _)) = staged_material {
            self.material_ledger = Some(next_material);
        }
        let inventory_pending = settlement == Settlement::Complete
            && matches!(settled.effect, WorkingEffect::RepairItem { .. });
        if inventory_pending {
            let inventory = inventory.expect("external working was validated with an inventory");
            self.apply_inventory_effect(&settled, inventory)?;
        } else if settlement == Settlement::Complete {
            self.apply_working_effect(&settled.effect, &settled.targets)?;
            self.save_effect_chunks(&settled.effect)?;
            self.save_effect_sidecars(&settled.effect)?;
            let tick = self.working_tick();
            let state = self
                .workings_state
                .as_mut()
                .expect("pending state was installed");
            state
                .settle(id, "completed", tick)
                .map_err(|error| error.to_string())?;
            state.save().map_err(|error| error.to_string())?;
        }
        Ok(WorkingResult {
            success: true,
            stable_id: id,
            phase: inventory_pending.then_some(WorkingPhase::PendingApply),
            cue: match settlement {
                Settlement::Complete => WorkingCueKind::Complete,
                Settlement::Cancel => WorkingCueKind::Cancel,
                Settlement::Interrupt => WorkingCueKind::Strain,
            },
            warning_band: settled.strain.warning_band,
            message: match settlement {
                Settlement::Complete if inventory_pending => format!(
                    "{} has landed; saving the authoritative profile finishes it.",
                    settled.definition.label
                ),
                Settlement::Complete => format!("{} completes.", settled.definition.label),
                Settlement::Cancel => format!(
                    "{} is cancelled and settles visibly.",
                    settled.definition.label
                ),
                Settlement::Interrupt => {
                    format!("{} breaks with accounted dross.", settled.definition.label)
                }
            },
        })
    }
}
