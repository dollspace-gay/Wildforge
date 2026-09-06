//! Conductor transfer implements transaction coordination.

use super::implement_transfer_properties;
use crate::arcane::AccountRead;
use crate::arcane::ArcaneAuthority;
use crate::arcane::ArcaneMove;
use crate::arcane::ArcaneOwner;
use crate::arcane::ArcaneTransaction;
use crate::arcane::Current;
use crate::arcane::LinkedFileReplacement;
use crate::implements::FrameResult;
use crate::implements::ImplementAuditEvent;
use crate::implements::ImplementCue;
use crate::implements::ImplementKind;
use crate::planet::BlockPos;
use crate::world::BlockEntity;
use crate::world::World;

impl World {
    pub(super) fn transfer_conductor_place_at_frame(
        &mut self,
        pos: BlockPos,
        actor: &str,
    ) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        let (source_region, source_kind) = self
            .conductor_place_source(pos)
            .ok_or("The loaded conductor no longer touches a confluence or well.")?;
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
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let target_instance = next_state
            .instance(output.arcane_id)
            .cloned()
            .ok_or("The fitted implement has no stable construction record.")?;
        let target_usable = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.item_clean_total(output.arcane_id))
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
        let environmental_instability = self.arcane_geography.as_ref().map_or(0, |geography| {
            geography
                .dross_band_at(source_region)
                .stability_penalty_permille()
        });
        let requested = target_free
            .min(target_rate)
            .min(u64::from(layout.network_size.max(1)) * 32);
        if requested == 0 {
            return Err("The local conductor has no safe transfer budget.".into());
        }

        let (atlas_index, old_cell, old_sequence, old_exported, old_external_imported) = {
            let geography = self
                .arcane_geography
                .as_ref()
                .ok_or("The finite magical geography is unavailable.")?;
            (
                source_region.index(geography.manifest.side),
                geography.dynamic.cells[source_region.index(geography.manifest.side)],
                geography.dynamic.ecology.event_sequence,
                geography.dynamic.ecology.exported,
                geography.dynamic.dross_state.external_imported,
            )
        };
        let selected = self
            .arcane_geography
            .as_mut()
            .ok_or("The finite magical geography is unavailable.")?
            .export_ambient_for_apparatus(source_region, requested)
            .map_err(|error| error.to_string())?;
        let rollback_geography = |world: &mut World| {
            if let Some(geography) = world.arcane_geography.as_mut() {
                geography.dynamic.cells[atlas_index] = old_cell;
                geography.dynamic.ecology.event_sequence = old_sequence;
                geography.dynamic.ecology.exported = old_exported;
                geography.dynamic.dross_state.external_imported = old_external_imported;
            }
        };

        let amount = selected.total();
        let network_loss = u16::from(layout.network_size.saturating_sub(1)).saturating_mul(2);
        let dross_permille = target_dross_rate
            .saturating_add(network_loss)
            .saturating_add(environmental_instability)
            .clamp(1, 900);
        let dross_units = amount
            .saturating_mul(u64::from(dross_permille))
            .div_ceil(1_000)
            .min(amount.saturating_sub(1));
        let mut clean = selected.clone();
        let dross = if dross_units == 0 {
            Current::default()
        } else {
            match clean.take_units(dross_units, std::iter::empty()) {
                Ok(current) => current,
                Err(error) => {
                    rollback_geography(self);
                    return Err(error.to_string());
                }
            }
        };
        let operation_id = match next_state.operation_id() {
            Ok(id) => id,
            Err(error) => {
                rollback_geography(self);
                return Err(error.to_string());
            }
        };
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
                .saturating_add(amount.saturating_mul(instability).div_ceil(300) as u32)
                .min(crate::implements::MAX_WAND_STRAIN);
            if matches!(
                &record.kind,
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
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "place_conductor_transfer".into(),
            instance_id: output.arcane_id,
            units: clean.total(),
            dross: dross.total(),
            actor: actor.into(),
            note: format!(
                "{} at {:?}; frame revision {frame_revision}; {} loaded segments",
                source_kind.label(),
                source_region,
                layout.network_size
            ),
        });
        let (manifest, mut files) = match self
            .arcane_geography
            .as_ref()
            .expect("geography was checked above")
            .linked_dynamic_replacements(&self.save_dir, operation_id)
        {
            Ok(prepared) => prepared,
            Err(error) => {
                rollback_geography(self);
                return Err(error.to_string());
            }
        };
        let implement_after = match next_state.encode() {
            Ok(bytes) => bytes,
            Err(error) => {
                rollback_geography(self);
                return Err(error.to_string());
            }
        };
        files.push(LinkedFileReplacement {
            subsystem: "implements".into(),
            operation_id,
            relative_path: crate::implements::IMPLEMENTS_FILE.into(),
            after: Some(implement_after),
        });
        let geography_owner = ArcaneOwner::Geography;
        let target_owner = ArcaneOwner::Item(output.arcane_id);
        let dross_owner = ArcaneOwner::ItemDross(output.arcane_id);
        if self.arcane_ledger.is_none() {
            rollback_geography(self);
            return Err("The world has no Current ledger.".into());
        }
        let transaction_id = match self
            .arcane_ledger
            .as_mut()
            .expect("ledger presence was checked")
            .system_transaction_id()
        {
            Ok(id) => id,
            Err(error) => {
                rollback_geography(self);
                return Err(error.to_string());
            }
        };
        let ledger = self
            .arcane_ledger
            .as_mut()
            .expect("ledger presence was checked");
        let mut reads = vec![
            AccountRead {
                owner: geography_owner.clone(),
                expected_version: ledger.version_of(&geography_owner),
            },
            AccountRead {
                owner: target_owner.clone(),
                expected_version: ledger.version_of(&target_owner),
            },
        ];
        let mut credits = vec![ArcaneMove {
            owner: target_owner,
            current: clean.clone(),
            content_id: Some(target_instance.content_id.clone()),
        }];
        if !dross.is_empty() {
            reads.push(AccountRead {
                owner: dross_owner.clone(),
                expected_version: ledger.version_of(&dross_owner),
            });
            credits.push(ArcaneMove {
                owner: dross_owner,
                current: dross.clone(),
                content_id: Some(target_instance.content_id.clone()),
            });
        }
        reads.sort_by(|a, b| a.owner.cmp(&b.owner));
        let transaction = ArcaneTransaction {
            id: transaction_id,
            reads,
            debits: vec![ArcaneMove {
                owner: geography_owner,
                current: selected,
                content_id: None,
            }],
            credits,
            transforms: Vec::new(),
            authority: ArcaneAuthority::System,
            reason: "loaded local conductor extraction".into(),
            content_id: target_instance.content_id.clone(),
            linked: Vec::new(),
        };
        if let Err(error) = ledger.commit_linked_files(transaction, files) {
            rollback_geography(self);
            return Err(error.to_string());
        }
        self.arcane_geography
            .as_mut()
            .expect("geography survived linked commit")
            .accept_linked_manifest(manifest);
        self.implements_state = Some(next_state);
        let revision = match self.installations.get_mut(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => {
                frame.revision = frame.revision.saturating_add(1);
                frame.revision
            }
            _ => frame_revision,
        };
        self.save_entities().map_err(|error| error.to_string())?;
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Transfer,
            message: format!(
                "The loaded conductor draws {} units from the local {} into the fitted implement.",
                amount,
                source_kind.label()
            ),
            preview: self.frame_preview(pos).ok(),
            lines: vec![format!(
                "{} units settled as retained dross.",
                dross.total()
            )],
        })
    }
}
