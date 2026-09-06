//! Source transfer implements transaction coordination.

use crate::arcane::ArcaneOwner;
use std::collections::BTreeMap;
use crate::world::BlockEntity;
use crate::planet::BlockPos;
use crate::arcane::Current;
use crate::implements::FrameResult;
use crate::implements::ImplementAuditEvent;
use crate::implements::ImplementCue;
use crate::implements::ImplementKind;
use crate::inventory::ItemStack;
use crate::arcane::LinkedFileReplacement;
use crate::implements::STRUCTURAL_SPARK_UNITS;
use crate::implements::VESSEL_SAFE_TRANSFER;
use crate::world::World;
use super::add_current;
use super::implement_transfer_properties;
use super::transaction_from_maps;

impl World {
    pub(super) fn transfer_at_frame(
        &mut self,
        pos: BlockPos,
        selected: Option<ItemStack>,
        actor: &str,
    ) -> Result<FrameResult, String> {
        if let Some(source) = selected
            && source.arcane_id != 0
            && self.reg.item(source.item).arcane.is_some()
        {
            return self.transfer_selected_source_at_frame(pos, source, actor);
        }
        // When both local reservoirs are dormant, an attached loaded
        // conductor at a real confluence/well is the only eligible source.
        // Once either reservoir holds charge, the ordinary vessel direction
        // remains deterministic and players can stage cargo deliberately.
        if self.conductor_place_source(pos).is_some()
            && let Some(BlockEntity::BindingFrame(frame)) = self.installations.get(&pos)
            && let Some(output) = frame.output
            && output.arcane_id != 0
            && let Ok((_, vessel, _)) = self.adjacent_vessel(pos)
            && vessel.arcane_id != 0
            && self.arcane_ledger.as_ref().is_some_and(|ledger| {
                ledger
                    .item_clean_total(output.arcane_id)
                    .map(crate::implements::usable_charge)
                    .unwrap_or(0)
                    == 0
                    && ledger
                        .item_clean_total(vessel.arcane_id)
                        .map(crate::implements::usable_charge)
                        .unwrap_or(0)
                        == 0
            })
        {
            return self.transfer_conductor_place_at_frame(pos, actor);
        }
        self.transfer_at_frame_with_limit(pos, actor, None, false)
    }

    /// Drain a physically selected charged shard, biological harvest, warden
    /// material, or other declaratively arcane reservoir into the implement
    /// fitted to the frame. The registry definition supplies the same bounded
    /// conductivity and stability contract to base and mod content; no item
    /// name or client-authored transfer statistic is trusted here.
    pub(super) fn transfer_selected_source_at_frame(
        &mut self,
        pos: BlockPos,
        source_stack: ItemStack,
        actor: &str,
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
        if output.arcane_id == source_stack.arcane_id {
            return Err("The fitted implement cannot be its own charge source.".into());
        }
        let source_definition = self
            .reg
            .item(source_stack.item)
            .arcane
            .clone()
            .ok_or("The selected item declares no charge-source behavior.")?;
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let target_instance = next_state
            .instance(output.arcane_id)
            .cloned()
            .ok_or("The fitted implement has no stable construction record.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let source_owner = ArcaneOwner::Item(source_stack.arcane_id);
        let target_owner = ArcaneOwner::Item(output.arcane_id);
        let dross_owner = ArcaneOwner::ItemDross(output.arcane_id);
        let source_account = ledger
            .account(&source_owner)
            .cloned()
            .ok_or("The selected source has no finite Current custody.")?;
        let source_usable = crate::implements::usable_charge(source_account.current.total());
        if source_usable == 0 {
            return Err("The selected source is dormant.".into());
        }
        let target_usable = ledger
            .item_clean_total(output.arcane_id)
            .map(crate::implements::usable_charge)
            .unwrap_or(0);
        let target_free = target_instance
            .usable_capacity()
            .saturating_sub(target_usable);
        if target_free == 0 {
            return Err("The receiving implement is visibly full.".into());
        }
        let (_, target_rate, target_dross_rate) =
            implement_transfer_properties(&target_instance.kind);
        let source_rate = u64::from(source_definition.conductivity_permille)
            .saturating_mul(VESSEL_SAFE_TRANSFER)
            .div_ceil(1_000)
            .max(1);
        let conductor_rate = u64::from(layout.network_size.max(1)) * 32;
        let amount = source_usable
            .min(target_free)
            .min(source_rate)
            .min(target_rate)
            .min(conductor_rate);
        if amount == 0 {
            return Err("There is no usable Current within the local transfer bounds.".into());
        }

        let mut remaining = source_account.current.clone();
        let mut selected = remaining
            .take_units(amount, std::iter::empty())
            .map_err(|error| error.to_string())?;
        if remaining.total() < STRUCTURAL_SPARK_UNITS {
            return Err("The transfer would tear out the source's structural spark.".into());
        }
        let source_loss = 1_000u16.saturating_sub(source_definition.stability_permille) / 5;
        let network_loss = u16::from(layout.network_size.saturating_sub(1)).saturating_mul(2);
        let dross_permille = target_dross_rate
            .saturating_add(source_loss)
            .saturating_add(network_loss)
            .clamp(1, 500);
        let dross_units = amount
            .saturating_mul(u64::from(dross_permille))
            .div_ceil(1_000)
            .min(amount.saturating_sub(1));
        let dross = if dross_units == 0 {
            Current::default()
        } else {
            selected
                .take_units(dross_units, std::iter::empty())
                .map_err(|error| error.to_string())?
        };
        let mut debit_current = selected.clone();
        debit_current
            .checked_add(&dross)
            .map_err(|error| error.to_string())?;
        let mut debits = BTreeMap::new();
        add_current(&mut debits, source_owner, &debit_current)
            .map_err(|error| error.to_string())?;
        let mut credits = BTreeMap::new();
        add_current(&mut credits, target_owner, &selected).map_err(|error| error.to_string())?;
        if !dross.is_empty() {
            add_current(&mut credits, dross_owner, &dross).map_err(|error| error.to_string())?;
        }

        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        if let Some(record) = next_state.instances.get_mut(&output.arcane_id) {
            let instability = match &record.kind {
                ImplementKind::Wand { resolved, .. } => u64::from(
                    1_000u16
                        .saturating_sub(resolved.stability)
                        .saturating_add(resolved.saturation_instability),
                ),
                ImplementKind::Charm { stability, .. } => {
                    u64::from(1_000u16.saturating_sub(*stability))
                }
                ImplementKind::Vessel { containment, .. } => {
                    u64::from(1_000u16.saturating_sub(*containment))
                }
                ImplementKind::Fragments { .. } => 1_000,
            }
            .max(25);
            record.strain = record
                .strain
                .saturating_add(amount.saturating_mul(instability).div_ceil(350) as u32)
                .min(crate::implements::MAX_WAND_STRAIN);
            if matches!(
                record.kind,
                ImplementKind::Wand { .. } | ImplementKind::Charm { .. }
            ) {
                record.wear = record
                    .wear
                    .saturating_add(
                        amount.saturating_mul(instability).div_ceil(60_000).max(1) as u32
                    )
                    .min(crate::implements::MAX_WAND_WEAR);
            }
        }
        // A charged implement used as the selected reservoir also experiences
        // outgoing strain. Ordinary shard/biology records remain in the
        // arcane ledger and do not acquire fabricated implement metadata.
        if let Some(record) = next_state.instances.get_mut(&source_stack.arcane_id) {
            record.strain = record
                .strain
                .saturating_add(amount.div_ceil(4).max(1) as u32)
                .min(crate::implements::MAX_WAND_STRAIN);
        }
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "selected_source_transfer".into(),
            instance_id: output.arcane_id,
            units: selected.total(),
            dross: dross.total(),
            actor: actor.into(),
            note: format!(
                "{} ({}) -> {}; frame revision {frame_revision}",
                source_stack.arcane_id,
                self.reg.item(source_stack.item).name,
                output.arcane_id
            ),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &target_instance.content_id,
            "binding-frame selected reservoir transfer",
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
        let revision = match self.installations.get_mut(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => {
                frame.revision = frame.revision.saturating_add(1);
                frame.revision
            }
            _ => frame_revision,
        };
        self.save_entities().map_err(|error| error.to_string())?;
        let failed = self
            .implements_state
            .as_ref()
            .and_then(|state| state.instance(output.arcane_id))
            .is_some_and(|instance| {
                instance.wear >= crate::implements::MAX_WAND_WEAR
                    || instance.strain >= crate::implements::MAX_WAND_STRAIN
            });
        if failed {
            return self.fail_frame_output(
                pos,
                "selected reservoir transfer exceeded the implement's physical limit",
            );
        }
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Transfer,
            message: format!(
                "The selected {} routes {} units into the fitted implement.",
                self.reg.item(source_stack.item).label,
                amount
            ),
            preview: self.frame_preview(pos).ok(),
            lines: vec![format!(
                "{} units settled as retained dross.",
                dross.total()
            )],
        })
    }
}
