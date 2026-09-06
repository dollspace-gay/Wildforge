//! Failure implements transaction coordination.

use crate::world::AIR;
use crate::arcane::ArcaneOwner;
use std::collections::BTreeMap;
use crate::world::BlockEntity;
use crate::world::BlockPos;
use crate::arcane::Current;
use crate::arcane::DrossMedium;
use crate::implements::FrameResult;
use crate::implements::ImplementAuditEvent;
use crate::implements::ImplementCue;
use crate::implements::ImplementKind;
use crate::world::ItemStack;
use crate::arcane::LinkedFileReplacement;
use crate::implements::STRUCTURAL_SPARK_UNITS;
use crate::world::World;
use super::add_current;
use super::transaction_from_maps;

impl World {
    pub(super) fn fail_frame_output(&mut self, pos: BlockPos, reason: &str) -> Result<FrameResult, String> {
        let output = match self.installations.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => frame
                .output
                .ok_or("The critically strained implement is no longer in the frame.")?,
            _ => return Err("The binding frame vanished before failure settled.".into()),
        };
        let fragments = self.fracture_implement_at(pos, output, reason)?;
        let revision = match self.installations.get_mut(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => {
                frame.output = None;
                frame.revision = frame.revision.saturating_add(1);
                frame.revision
            }
            _ => 0,
        };
        self.push_drop_at(pos, fragments);
        self.save_entities().map_err(|error| error.to_string())?;
        Ok(FrameResult {
            success: false,
            revision,
            cue: ImplementCue::Failure,
            message: "The implement fractures under visible strain; recoverable fragments fall clear.".into(),
            preview: self.frame_preview(pos).ok(),
            lines: vec!["Its remaining Current and dross were released through explicit ledger dispositions.".into()],
        })
    }

    pub(super) fn fail_placed_vessel(
        &mut self,
        pos: BlockPos,
        stack: ItemStack,
        reason: &str,
    ) -> Result<(), String> {
        let fragments = self.fracture_implement_at(pos, stack, reason)?;
        self.installations.remove(&pos);
        self.set_block_at(pos, AIR);
        self.push_drop_at(pos, fragments);
        Ok(())
    }

    /// Turn a failed implement into one stable, non-stackable bundle whose
    /// sidecar still contains the exact original construction bill. Only the
    /// structural spark remains bound; useful charge and retained dross are
    /// released through the same durable transaction.
    pub(super) fn fracture_implement_at(
        &mut self,
        pos: BlockPos,
        stack: ItemStack,
        reason: &str,
    ) -> Result<ItemStack, String> {
        let fragment_item = self
            .reg
            .item_id("base:implement_fragment")
            .ok_or("The conserved fragment-bundle content definition is missing.")?;
        let fragment_content = self.reg.item(fragment_item).name.clone();
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let original = next_state
            .instance(stack.arcane_id)
            .cloned()
            .ok_or("The failing implement has no construction record.")?;
        if matches!(original.kind, ImplementKind::Fragments { .. }) {
            return Err("A fragment bundle cannot recursively fracture.".into());
        }
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let clean_owner = ArcaneOwner::Item(stack.arcane_id);
        let dross_owner = ArcaneOwner::ItemDross(stack.arcane_id);
        let clean = ledger
            .account(&clean_owner)
            .map(|account| account.current.clone())
            .ok_or("The failing implement has no structural Current custody.")?;
        if clean.total() < STRUCTURAL_SPARK_UNITS {
            return Err("The failing implement has lost its structural spark.".into());
        }
        let retained = ledger
            .account(&dross_owner)
            .map(|account| account.current.clone())
            .unwrap_or_default();
        let mut released = clean.clone();
        let spark = released
            .take_units(STRUCTURAL_SPARK_UNITS, std::iter::empty())
            .map_err(|error| error.to_string())?;
        let destabilized = released
            .take_units(released.total().div_ceil(2), std::iter::empty())
            .map_err(|error| error.to_string())?;
        let mut all_dross = destabilized;
        all_dross
            .checked_add(&retained)
            .map_err(|error| error.to_string())?;

        let mut debits = BTreeMap::new();
        add_current(&mut debits, clean_owner.clone(), &clean).map_err(|error| error.to_string())?;
        if !retained.is_empty() {
            add_current(&mut debits, dross_owner, &retained).map_err(|error| error.to_string())?;
        }
        let mut credits = BTreeMap::new();
        add_current(&mut credits, clean_owner, &spark).map_err(|error| error.to_string())?;
        if !released.is_empty() {
            add_current(&mut credits, ArcaneOwner::Ambient(region), &released)
                .map_err(|error| error.to_string())?;
        }
        if !all_dross.is_empty() {
            add_current(
                &mut credits,
                ArcaneOwner::Dross {
                    region,
                    medium: DrossMedium::Air,
                },
                &all_dross,
            )
            .map_err(|error| error.to_string())?;
        }

        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        let record = next_state
            .instances
            .get_mut(&stack.arcane_id)
            .ok_or("The failing implement vanished from state.")?;
        let pieces = record.construction.len().clamp(1, u8::MAX as usize) as u8;
        record.kind = ImplementKind::Fragments {
            source: Box::new(original.kind),
            pieces,
        };
        record.content_id = fragment_content.clone();
        record.wear = crate::implements::MAX_WAND_WEAR;
        record.strain = crate::implements::MAX_WAND_STRAIN;
        record.provenance = format!("fractured:{}", original.instance_id);
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "catastrophic_fragmentation".into(),
            instance_id: stack.arcane_id,
            units: released.total(),
            dross: all_dross.total(),
            actor: "world".into(),
            note: reason.into(),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &fragment_content,
            "visible conserved implement fragmentation",
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
        Ok(ItemStack {
            item: fragment_item,
            count: 1,
            durability: self.reg.item(fragment_item).durability,
            arcane_id: stack.arcane_id,
        })
    }
}
