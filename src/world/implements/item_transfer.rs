//! Item transfer implements transaction coordination.

use super::add_current;
use super::implement_transfer_properties;
use super::transaction_from_maps;
use crate::arcane::ArcaneOwner;
use crate::arcane::Current;
use crate::arcane::LinkedFileReplacement;
use crate::implements::FrameResult;
use crate::implements::ImplementAuditEvent;
use crate::implements::ImplementCue;
use crate::implements::ImplementKind;
use crate::implements::STRUCTURAL_SPARK_UNITS;
use crate::planet::BlockPos;
use crate::world::BlockEntity;
use crate::world::World;
use std::collections::BTreeMap;

impl World {
    pub(super) fn transfer_at_frame_with_limit(
        &mut self,
        pos: BlockPos,
        actor: &str,
        forced_limit: Option<u64>,
        calibration: bool,
    ) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        let (output, frame_revision) = match self.installations.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => (
                frame
                    .output
                    .ok_or("Fit an implement in the finished-object cradle.")?,
                frame.revision,
            ),
            _ => return Err("The frame has no physical mounts.".into()),
        };
        if output.arcane_id == 0 {
            return Err("The fitted object has not been bound to finite Current.".into());
        }
        let (vessel_pos, vessel_stack, vessel_revision) = self.adjacent_vessel(pos)?;
        if vessel_stack.arcane_id == 0 {
            return Err("Calibrate the adjacent vessel before transferring.".into());
        }
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let output_instance = next_state
            .instance(output.arcane_id)
            .cloned()
            .ok_or("The fitted implement has no stable construction record.")?;
        let vessel_instance = next_state
            .instance(vessel_stack.arcane_id)
            .cloned()
            .ok_or("The vessel has no stable construction record.")?;
        let environmental_instability = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .and_then(|region| {
                self.arcane_geography
                    .as_ref()
                    .map(|geography| geography.dross_band_at(region))
            })
            .map_or(0, crate::dross::DrossBand::stability_penalty_permille);
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let output_total = ledger.item_clean_total(output.arcane_id).unwrap_or(0);
        let vessel_total = ledger.item_clean_total(vessel_stack.arcane_id).unwrap_or(0);
        let output_usable = crate::implements::usable_charge(output_total);
        let vessel_usable = crate::implements::usable_charge(vessel_total);
        let output_free = output_instance
            .usable_capacity()
            .saturating_sub(output_usable);
        let vessel_free = vessel_instance
            .usable_capacity()
            .saturating_sub(vessel_usable);
        let to_output = calibration || (output_free != 0 && vessel_usable != 0);
        let (source_id, target_id, source_instance, target_instance, source_usable, target_free) =
            if to_output {
                (
                    vessel_stack.arcane_id,
                    output.arcane_id,
                    &vessel_instance,
                    &output_instance,
                    vessel_usable,
                    output_free,
                )
            } else {
                (
                    output.arcane_id,
                    vessel_stack.arcane_id,
                    &output_instance,
                    &vessel_instance,
                    output_usable,
                    vessel_free,
                )
            };
        if source_usable == 0 {
            return Err("The source implement is dormant.".into());
        }
        if target_free == 0 {
            return Err("The receiving implement is visibly full.".into());
        }
        let (_, source_rate, _) = implement_transfer_properties(&source_instance.kind);
        let (_, target_rate, target_dross_rate) =
            implement_transfer_properties(&target_instance.kind);
        let conductor_rate = u64::from(layout.network_size.max(1)) * 32;
        let amount = source_usable
            .min(target_free)
            .min(source_rate)
            .min(target_rate)
            .min(conductor_rate)
            .min(forced_limit.unwrap_or(u64::MAX));
        if amount == 0 || calibration && amount < 2 {
            return Err("There is not enough free, usable Current for that pulse.".into());
        }
        let source_owner = ArcaneOwner::Item(source_id);
        let target_owner = ArcaneOwner::Item(target_id);
        let dross_owner = ArcaneOwner::ItemDross(target_id);
        let source_account = ledger
            .account(&source_owner)
            .cloned()
            .ok_or("The source Current account is missing.")?;
        let mut available = source_account.current.clone();
        let mut selected = available
            .take_units(amount, std::iter::empty())
            .map_err(|error| error.to_string())?;
        if available.total() < STRUCTURAL_SPARK_UNITS {
            return Err("The transfer would tear out the source's structural spark.".into());
        }
        let network_dross = u64::from(layout.network_size.saturating_sub(1)) * 2;
        let dross_permille = u64::from(target_dross_rate)
            .saturating_add(network_dross)
            .saturating_add(u64::from(environmental_instability))
            .clamp(1, 900);
        let dross_units = amount
            .saturating_mul(dross_permille)
            .div_ceil(1_000)
            .min(amount.saturating_sub(1));
        let dross = if dross_units == 0 {
            Current::default()
        } else {
            selected
                .take_units(dross_units, std::iter::empty())
                .map_err(|error| error.to_string())?
        };
        let mut debits = BTreeMap::new();
        add_current(&mut debits, source_owner, &{
            let mut total = selected.clone();
            total
                .checked_add(&dross)
                .map_err(|error| error.to_string())?;
            total
        })
        .map_err(|error| error.to_string())?;
        let mut credits = BTreeMap::new();
        add_current(&mut credits, target_owner, &selected).map_err(|error| error.to_string())?;
        if !dross.is_empty() {
            add_current(&mut credits, dross_owner, &dross).map_err(|error| error.to_string())?;
        }
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        for (id, receiving) in [(source_id, false), (target_id, true)] {
            let Some(record) = next_state.instances.get_mut(&id) else {
                continue;
            };
            let load = if receiving {
                amount
            } else {
                amount.div_ceil(2)
            };
            let (strain_delta, wear_delta) = match &record.kind {
                ImplementKind::Wand { resolved, .. } => {
                    let instability = u64::from(1_000u16.saturating_sub(resolved.stability))
                        .saturating_add(u64::from(resolved.saturation_instability))
                        .clamp(25, 1_500);
                    (
                        load.saturating_mul(instability).div_ceil(250).max(1) as u32,
                        load.saturating_mul(instability).div_ceil(50_000).max(1) as u32,
                    )
                }
                ImplementKind::Charm { stability, .. } => {
                    let instability = u64::from(1_000u16.saturating_sub(*stability)).max(25);
                    (
                        load.saturating_mul(instability).div_ceil(500).max(1) as u32,
                        load.saturating_mul(instability).div_ceil(75_000).max(1) as u32,
                    )
                }
                ImplementKind::Vessel { containment, .. } => {
                    let exposure = u64::from(1_000u16.saturating_sub(*containment)).max(20);
                    (load.saturating_mul(exposure).div_ceil(400).max(1) as u32, 0)
                }
                ImplementKind::Fragments { .. } => (crate::implements::MAX_WAND_STRAIN, 0),
            };
            record.strain = record
                .strain
                .saturating_add(strain_delta)
                .min(crate::implements::MAX_WAND_STRAIN);
            record.wear = record
                .wear
                .saturating_add(wear_delta)
                .min(crate::implements::MAX_WAND_WEAR);
        }
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: if calibration {
                "calibration_pulse"
            } else {
                "transfer"
            }
            .into(),
            instance_id: target_id,
            units: selected.total(),
            dross: dross.total(),
            actor: actor.into(),
            note: format!(
                "{} -> {}; frame revision {frame_revision}; vessel revision {vessel_revision}",
                source_id, target_id
            ),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &target_instance.content_id,
            if calibration {
                "binding-frame calibration pulse"
            } else {
                "local conductor transfer"
            },
        )?;
        let replacement = LinkedFileReplacement {
            subsystem: "implements".into(),
            operation_id,
            relative_path: crate::implements::IMPLEMENTS_FILE.into(),
            after: Some(next_state.encode().map_err(|error| error.to_string())?),
        };
        ledger
            .commit_linked_files(transaction, vec![replacement])
            .map_err(|error| error.to_string())?;
        self.implements_state = Some(next_state);
        if let Some(BlockEntity::BindingFrame(frame)) = self.installations.get_mut(&pos) {
            frame.revision = frame.revision.saturating_add(1);
        }
        if let Some(BlockEntity::ChargeVessel(vessel)) = self.installations.get_mut(&vessel_pos) {
            let after_usable = if target_id == vessel_stack.arcane_id {
                vessel_usable.saturating_add(selected.total())
            } else {
                vessel_usable.saturating_sub(amount)
            };
            let pressure = after_usable
                .saturating_mul(1_000)
                .checked_div(vessel_instance.usable_capacity().max(1))
                .unwrap_or(1_000)
                .min(2_000);
            let pressure_damage = pressure
                .saturating_sub(700)
                .saturating_mul(amount)
                .div_ceil(200_000) as u16;
            vessel.damage = vessel
                .damage
                .saturating_add(pressure_damage)
                .saturating_add(dross.total().min(4) as u16)
                .min(1_000);
            vessel.revision = vessel.revision.saturating_add(1);
        }
        self.save_entities().map_err(|error| error.to_string())?;
        let output_failed = self
            .implements_state
            .as_ref()
            .and_then(|state| state.instance(output.arcane_id))
            .is_some_and(|instance| {
                instance.wear >= crate::implements::MAX_WAND_WEAR
                    || instance.strain >= crate::implements::MAX_WAND_STRAIN
            });
        if output_failed {
            return self.fail_frame_output(
                pos,
                "transfer strain exceeded the implement's physical limit",
            );
        }
        let vessel_failed = self.installations.get(&vessel_pos).is_some_and(
            |entity| matches!(entity, BlockEntity::ChargeVessel(vessel) if vessel.damage >= 1_000),
        );
        if vessel_failed {
            self.fail_placed_vessel(
                vessel_pos,
                vessel_stack,
                "transfer pressure fractured the charge vessel",
            )?;
            self.save_entities().map_err(|error| error.to_string())?;
            return Ok(FrameResult {
                success: false,
                revision: self
                    .installations
                    .get(&pos)
                    .and_then(|entity| match entity {
                        BlockEntity::BindingFrame(frame) => Some(frame.revision),
                        _ => None,
                    })
                    .unwrap_or(frame_revision),
                cue: ImplementCue::Failure,
                message: "The overstrained vessel fractures visibly; recoverable fragments remain."
                    .into(),
                preview: self.frame_preview(pos).ok(),
                lines: vec![
                    "Released Current and retained dross were settled in the local ledger.".into(),
                ],
            });
        }
        let revision = match self.installations.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => frame.revision,
            _ => frame_revision,
        };
        Ok(FrameResult {
            success: true,
            revision,
            cue: if calibration {
                ImplementCue::Strain
            } else {
                ImplementCue::Transfer
            },
            message: if calibration {
                format!("A {amount}-unit pulse settles through the fitted implement.")
            } else if to_output {
                format!("The vessel routes {amount} units into the fitted implement.")
            } else {
                format!("The fitted implement routes {amount} units back into the vessel.")
            },
            preview: self.frame_preview(pos).ok(),
            lines: vec![format!(
                "{} units settled as retained dross.",
                dross.total()
            )],
        })
    }
}
