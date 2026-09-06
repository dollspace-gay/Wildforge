//! Ritual reservation workings transaction coordination.

use crate::arcane::ArcaneOwner;
use std::collections::BTreeMap;
use crate::world::BlockPos;
use crate::arcane::Current;
use crate::workings::CurrentDebit;
use crate::workings::DeliveryMode;
use crate::arcane::LinkedFileReplacement;
use crate::workings::PhysicalDebit;
use crate::workings::StrainInputs;
use crate::workings::WorkingApparatus;
use crate::workings::WorkingCueKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingPhase;
use crate::workings::WorkingResult;
use crate::workings::WorkingTargetSnapshot;
use crate::workings::WorkingTransaction;
use crate::world::World;
use super::effect_path;

impl World {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn reserve_ritual_effect(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        controller: BlockPos,
        source_vessel_id: u64,
        controller_revision: u64,
        apparatus_damage: u16,
        working_id: &str,
        targets: Vec<WorkingTargetSnapshot>,
        physical_debits: Vec<PhysicalDebit>,
        effect: WorkingEffect,
        magnitude: u32,
        distance: u16,
        duration_ticks: u64,
        payload: Current,
    ) -> Result<WorkingResult, String> {
        let definition = self
            .reg
            .workings
            .get(working_id)
            .cloned()
            .ok_or_else(|| format!("Unknown ritual {working_id}."))?;
        if definition.mode != DeliveryMode::Ritual
            || definition.handler.effect_kind() != effect.kind()
        {
            return Err("The selected content does not own that native ritual capability.".into());
        }
        let quote = definition
            .quote(magnitude, distance, duration_ticks)
            .map_err(|error| error.to_string())?;
        let preparation_modifiers = self.preparation_modifiers(actor);
        let source_owner = ArcaneOwner::Item(source_vessel_id);
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(controller.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let local_capacity_permille = self.local_capacity_permille(region);
        let environmental_instability = self.arcane_geography.as_ref().map_or(0, |geography| {
            geography.dross_band_at(region).stability_penalty_permille()
        });
        let tick = self.working_tick();
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no finite Current ledger.")?;
        let source_account = ledger
            .account(&source_owner)
            .cloned()
            .ok_or("The ritual's mounted source vessel is empty.")?;
        let mut available = source_account.current.clone();
        available
            .checked_sub(&payload)
            .map_err(|_| "The ritual payload is not present in its named source vessel.")?;
        let cost = available
            .take_units(quote.charge, [definition.focus.clone()])
            .map_err(|error| error.to_string())?;
        let mut reserved = payload.clone();
        reserved
            .checked_add(&cost)
            .map_err(|error| error.to_string())?;
        let strain = crate::workings::deterministic_strain(StrainInputs {
            resonance_mismatch_permille: 0,
            component_instability_permille: 0,
            throughput: quote.charge,
            safe_throughput: definition.safe_throughput.max(1),
            local_capacity_permille,
            apparatus_damage_permille: apparatus_damage.min(1_000),
            contamination_permille: environmental_instability,
            personal_strain_permille: preparation_modifiers.strain_permille,
            ..StrainInputs::default()
        })
        .map_err(|error| error.to_string())?;
        if strain.refuses {
            return Err("The visibly damaged ritual apparatus refuses this load.".into());
        }
        let environmental_dross = cost
            .total()
            .saturating_mul(u64::from(environmental_instability))
            .div_ceil(1_000);
        let dross_units = quote
            .base_dross
            .saturating_add(strain.extra_dross)
            .saturating_add(environmental_dross)
            .min(cost.total());
        let mut clean_cost = cost;
        let dross_current = clean_cost
            .take_units(dross_units, std::iter::empty())
            .map_err(|error| error.to_string())?;
        let mut return_current = payload;
        return_current
            .checked_add(&clean_cost)
            .map_err(|error| error.to_string())?;
        let id = ledger
            .allocate_working_id()
            .map_err(|error| error.to_string())?;
        let transaction = WorkingTransaction {
            id,
            definition: definition.clone(),
            actor,
            actor_label: actor_label.into(),
            source: controller,
            path: {
                let path = effect_path(&effect);
                if path.is_empty() {
                    vec![controller]
                } else {
                    path
                }
            },
            apparatus: WorkingApparatus::Ritual {
                controller,
                expected_revision: controller_revision,
            },
            targets,
            current_debits: vec![CurrentDebit {
                owner: source_owner.clone(),
                expected_version: source_account.version,
                current: reserved.clone(),
            }],
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
            forced: false,
            trace: format!("{working_id} by {actor_label} at {controller:?}"),
            completion_nonce: id,
        };
        transaction.validate().map_err(|error| error.to_string())?;
        let mut next_state = self
            .workings_state
            .clone()
            .ok_or("The world has no workings authority.")?;
        if let WorkingEffect::Settle { process_id, .. } = &transaction.effect {
            let process = next_state
                .active
                .get_mut(process_id)
                .ok_or("The adjacent process ended before the settling rite reserved.")?;
            let remaining = process.due_tick.saturating_sub(tick).max(1);
            let latest = process
                .started_tick
                .saturating_add(process.definition.max_duration_ticks);
            process.due_tick = process
                .due_tick
                .saturating_add(remaining.div_ceil(2))
                .min(latest);
        }
        next_state
            .start(transaction)
            .map_err(|error| error.to_string())?;
        let arcane = crate::world::implements::transaction_from_maps(
            ledger,
            BTreeMap::from([(source_owner, reserved.clone())]),
            BTreeMap::from([(ArcaneOwner::Working(id), reserved)]),
            working_id,
            "constructed ritual reserved exact cost, payload, apparatus, and targets",
        )?;
        ledger
            .commit_linked_files(
                arcane,
                vec![LinkedFileReplacement {
                    subsystem: "workings".into(),
                    operation_id: id,
                    relative_path: crate::workings::WORKINGS_FILE.into(),
                    after: Some(next_state.encode().map_err(|error| error.to_string())?),
                }],
            )
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
            message: format!("{} begins through its physical circle.", definition.label),
        })
    }
}
