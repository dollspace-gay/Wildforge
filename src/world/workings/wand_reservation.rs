//! Wand reservation workings transaction coordination.

use crate::arcane::ArcaneOwner;
use std::collections::BTreeMap;
use crate::planet::BlockPos;
use crate::arcane::Current;
use crate::workings::CurrentDebit;
use crate::workings::DeliveryMode;
use crate::implements::ImplementKind;
use crate::arcane::LinkedFileReplacement;
use crate::workings::PhysicalDebit;
use crate::implements::STRUCTURAL_SPARK_UNITS;
use crate::workings::StrainInputs;
use crate::workings::WorkingApparatus;
use crate::workings::WorkingCueKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingPhase;
use crate::workings::WorkingResult;
use crate::workings::WorkingTargetSnapshot;
use crate::workings::WorkingTransaction;
use crate::world::World;
use super::AMBIENT_SAFE_FLOOR;
use super::effect_path;

impl World {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn reserve_wand_effect(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        targets: Vec<WorkingTargetSnapshot>,
        physical_debits: Vec<PhysicalDebit>,
        effect: WorkingEffect,
        magnitude: u32,
        distance: u16,
        duration_ticks: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        let definition = self
            .reg
            .workings
            .get(working_id)
            .cloned()
            .ok_or_else(|| format!("Unknown working {working_id}."))?;
        if definition.mode != DeliveryMode::Wand
            || definition.handler.effect_kind() != effect.kind()
        {
            return Err("The selected content does not own that native wand capability.".into());
        }
        self.validate_wand_line_of_sight(source, &effect, definition.range)?;
        let quote = definition
            .quote(magnitude, distance, duration_ticks)
            .map_err(|error| error.to_string())?;
        let instance = self
            .implements_state
            .as_ref()
            .and_then(|state| state.instance(wand_id))
            .cloned()
            .ok_or("The held wand has no authoritative physical instance.")?;
        let ImplementKind::Wand { resolved, .. } = &instance.kind else {
            return Err("That implement is not a wand.".into());
        };
        if instance.wear >= crate::implements::MAX_WAND_WEAR
            || instance.strain >= crate::implements::MAX_WAND_STRAIN
        {
            return Err("The wand is visibly too damaged or strained to channel safely.".into());
        }
        let preparation_modifiers = self.preparation_modifiers(actor);
        let charge_required = quote
            .charge
            .saturating_mul(u64::from(preparation_modifiers.drain_permille))
            .div_ceil(1_000);
        let prepared_safe_throughput = definition
            .safe_throughput
            .min(resolved.safe_transfer)
            .saturating_mul(u64::from(preparation_modifiers.throughput_permille))
            .div_ceil(1_000)
            .max(1);
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(source.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let wand_owner = ArcaneOwner::Item(wand_id);
        let wand_usable = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&wand_owner))
            .map_or(0, |account| {
                account
                    .current
                    .total()
                    .saturating_sub(STRUCTURAL_SPARK_UNITS)
            });
        if wand_usable < charge_required && definition.ambient {
            // A normal draw exports enough local custody to leave the
            // measured floor intact. Explicit forced draw exports only the
            // missing effect charge, so crossing the floor is real, finite,
            // legible, and subsequently priced by deterministic strain.
            self.ensure_regional_ambient_units(
                region,
                charge_required
                    .saturating_sub(wand_usable)
                    .saturating_add(if forced { 0 } else { AMBIENT_SAFE_FLOOR }),
                "working drew measured local Ambient Current",
            )?;
        }
        let local_capacity_permille = self.local_capacity_permille(region);
        let environmental_instability = self.arcane_geography.as_ref().map_or(0, |geography| {
            geography.dross_band_at(region).stability_penalty_permille()
        });
        let tick = self.working_tick();
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no finite Current ledger.")?;
        let mut remaining = charge_required;
        let mut current_debits = Vec::new();
        let mut reserved = Current::default();
        if let Some(account) = ledger.account(&wand_owner).cloned() {
            let amount = remaining.min(
                account
                    .current
                    .total()
                    .saturating_sub(STRUCTURAL_SPARK_UNITS),
            );
            if amount != 0 {
                let mut current = account.current.clone();
                let selected = current
                    .take_units(amount, [definition.focus.clone()])
                    .map_err(|error| error.to_string())?;
                reserved
                    .checked_add(&selected)
                    .map_err(|error| error.to_string())?;
                current_debits.push(CurrentDebit {
                    owner: wand_owner.clone(),
                    expected_version: account.version,
                    current: selected,
                });
                remaining -= amount;
            }
        }
        let ambient_owner = ArcaneOwner::Ambient(region);
        if remaining != 0 && definition.ambient {
            let account = ledger
                .account(&ambient_owner)
                .cloned()
                .ok_or("No measured Ambient Current is available here.")?;
            let safe_available = account.current.total().saturating_sub(AMBIENT_SAFE_FLOOR);
            let amount = if forced {
                remaining.min(account.current.total())
            } else {
                remaining.min(safe_available)
            };
            if amount != 0 {
                let mut current = account.current.clone();
                let selected = current
                    .take_units(amount, [definition.focus.clone()])
                    .map_err(|error| error.to_string())?;
                reserved
                    .checked_add(&selected)
                    .map_err(|error| error.to_string())?;
                current_debits.push(CurrentDebit {
                    owner: ambient_owner.clone(),
                    expected_version: account.version,
                    current: selected,
                });
                remaining -= amount;
            }
        }
        if remaining != 0 {
            return Err(format!(
                "The wand and measured local Current are {remaining} units short."
            ));
        }
        let preferred_weight = resolved
            .resonance
            .get(&definition.focus)
            .copied()
            .unwrap_or_default();
        let total_weight = resolved.resonance.values().copied().max().unwrap_or(1);
        let mismatch = 1_000u16.saturating_sub(
            u16::try_from(
                u32::from(preferred_weight)
                    .saturating_mul(1_000)
                    .checked_div(u32::from(total_weight.max(1)))
                    .unwrap_or_default(),
            )
            .unwrap_or(1_000),
        );
        let over_safe = charge_required.saturating_sub(prepared_safe_throughput);
        let below_floor = ledger.account(&ambient_owner).map_or(0, |account| {
            AMBIENT_SAFE_FLOOR.saturating_sub(account.current.total())
        });
        let strain = crate::workings::deterministic_strain(StrainInputs {
            resonance_mismatch_permille: mismatch,
            component_instability_permille: 1_000u16.saturating_sub(resolved.stability),
            throughput: charge_required,
            safe_throughput: prepared_safe_throughput,
            local_capacity_permille,
            below_safe_floor_units: below_floor,
            apparatus_damage_permille: ((u64::from(instance.wear) * 1_000)
                / u64::from(crate::implements::MAX_WAND_WEAR))
                as u16,
            contamination_permille: ((instance.strain / 10).min(1_000) as u16)
                .saturating_add(environmental_instability)
                .min(1_000),
            interruption: false,
            forced_overdraw_units: u64::from(forced)
                .saturating_mul(over_safe.max(below_floor))
                .saturating_mul(u64::from(preparation_modifiers.overdraw_permille))
                .div_ceil(1_000),
            personal_strain_permille: preparation_modifiers.strain_permille,
        })
        .map_err(|error| error.to_string())?;
        if strain.refuses {
            return Err("The forced overdraw exceeds this visibly damaged apparatus.".into());
        }
        let apparatus_dross = charge_required
            .saturating_mul(u64::from(resolved.dross_per_thousand))
            .div_ceil(1_000);
        let environmental_dross = charge_required
            .saturating_mul(u64::from(environmental_instability))
            .div_ceil(1_000);
        let dross_units = quote
            .base_dross
            .saturating_add(apparatus_dross)
            .saturating_add(strain.extra_dross)
            .saturating_add(environmental_dross)
            .min(reserved.total());
        let mut return_current = reserved.clone();
        let dross_current = return_current
            .take_units(dross_units, std::iter::empty())
            .map_err(|error| error.to_string())?;
        let id = ledger
            .allocate_working_id()
            .map_err(|error| error.to_string())?;
        let transaction = WorkingTransaction {
            id,
            definition: definition.clone(),
            actor,
            actor_label: actor_label.into(),
            source,
            path: {
                let mut path = effect_path(&effect);
                if path.is_empty() {
                    path.push(source);
                    for target in &targets {
                        let pos = match target {
                            WorkingTargetSnapshot::Block { pos, .. }
                            | WorkingTargetSnapshot::Reservoir { pos, .. } => Some(*pos),
                            WorkingTargetSnapshot::Area { controller, .. } => Some(*controller),
                            WorkingTargetSnapshot::Item { .. }
                            | WorkingTargetSnapshot::Entity { .. } => None,
                        };
                        if let Some(pos) = pos
                            && !path.contains(&pos)
                        {
                            path.push(pos);
                        }
                    }
                }
                path
            },
            apparatus: WorkingApparatus::Wand {
                instance_id: wand_id,
                expected_revision: u64::from(instance.wear) << 32 | u64::from(instance.strain),
            },
            targets,
            current_debits: current_debits.clone(),
            reserved_current: reserved.clone(),
            physical_debits,
            effect,
            return_current,
            dross_current,
            phase: WorkingPhase::Charging,
            started_tick: tick,
            due_tick: tick.saturating_add(duration_ticks),
            interruption: definition.interruption,
            strain,
            forced,
            trace: format!("{working_id} by {actor_label} from {source:?}"),
            completion_nonce: id,
        };
        transaction.validate().map_err(|error| error.to_string())?;
        let mut next_state = self
            .workings_state
            .clone()
            .ok_or("The world has no workings authority.")?;
        next_state
            .start(transaction)
            .map_err(|error| error.to_string())?;
        let mut debits = BTreeMap::new();
        for debit in current_debits {
            crate::world::implements::add_current(&mut debits, debit.owner, &debit.current)
                .map_err(|error| error.to_string())?;
        }
        let mut credits = BTreeMap::new();
        crate::world::implements::add_current(&mut credits, ArcaneOwner::Working(id), &reserved)
            .map_err(|error| error.to_string())?;
        let arcane = crate::world::implements::transaction_from_maps(
            ledger,
            debits,
            credits,
            working_id,
            "working reserved exact Current and targets",
        )?;
        let replacement = LinkedFileReplacement {
            subsystem: "workings".into(),
            operation_id: id,
            relative_path: crate::workings::WORKINGS_FILE.into(),
            after: Some(next_state.encode().map_err(|error| error.to_string())?),
        };
        ledger
            .commit_linked_files(arcane, vec![replacement])
            .map_err(|error| error.to_string())?;
        self.workings_state = Some(next_state);
        Ok(WorkingResult {
            success: true,
            stable_id: id,
            phase: Some(WorkingPhase::Charging),
            cue: if strain.warning_band >= 2 {
                WorkingCueKind::Strain
            } else {
                WorkingCueKind::Settle
            },
            warning_band: strain.warning_band,
            message: format!("{} begins to settle.", definition.label),
        })
    }
}
