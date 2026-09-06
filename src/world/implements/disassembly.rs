//! Disassembly implements transaction coordination.

use crate::arcane::ArcaneOwner;
use std::collections::BTreeMap;
use crate::world::BlockEntity;
use crate::planet::BlockPos;
use crate::arcane::Current;
use crate::arcane::DrossMedium;
use crate::implements::FrameResult;
use crate::implements::ImplementAuditEvent;
use crate::implements::ImplementCue;
use crate::implements::ImplementKind;
use crate::inventory::ItemStack;
use crate::arcane::LinkedFileReplacement;
use crate::implements::STRUCTURAL_SPARK_UNITS;
use crate::world::World;
use super::add_current;
use super::transaction_from_maps;

impl World {
    pub(super) fn disassemble_at_frame(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
        selected: Option<ItemStack>,
        actor: &str,
    ) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        if !selected.is_some_and(|stack| self.reg.item(stack.item).shears) {
            return Err("Safe disassembly requires shears in the selected hand.".into());
        }
        let (output, frame_revision) = match self.installations.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => (
                frame
                    .output
                    .ok_or("Fit a bound implement in the finished cradle.")?,
                frame.revision,
            ),
            _ => return Err("The frame has no mounts.".into()),
        };
        if output.arcane_id == 0 {
            return Err("That object has no stable implement identity to disassemble.".into());
        }
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let instance = next_state
            .instances
            .get(&output.arcane_id)
            .cloned()
            .ok_or("The fitted implement has no construction record.")?;
        let source_kind = match &instance.kind {
            ImplementKind::Fragments { source, .. } => source.as_ref().clone(),
            kind => kind.clone(),
        };
        let mut recovered_names = Vec::new();
        let mut unavailable = Vec::new();
        for component in &instance.construction {
            let compatible = self
                .reg
                .item_id(&component.content_id)
                .is_some_and(|item| self.reg.item(item).materials == component.materials);
            if compatible {
                recovered_names.push(component.content_id.clone());
            } else {
                unavailable.push(component.clone());
            }
        }
        if recovered_names.is_empty()
            && !unavailable.is_empty()
            && matches!(instance.kind, ImplementKind::Fragments { .. })
        {
            return Err(
                "No saved fragment component has a compatible registered item; restore its content pack or keep the conserved bundle."
                    .into(),
            );
        }
        let leaves_fragment = !unavailable.is_empty();
        let fragment_content = self
            .reg
            .item_id("base:implement_fragment")
            .map(|item| self.reg.item(item).name.clone())
            .ok_or("The conserved fragment-bundle content definition is missing.")?;
        if leaves_fragment {
            let record = next_state
                .instances
                .get_mut(&output.arcane_id)
                .ok_or("The fitted implement vanished during disassembly.")?;
            let pieces = unavailable.len().clamp(1, u8::MAX as usize) as u8;
            record.kind = ImplementKind::Fragments {
                source: Box::new(source_kind),
                pieces,
            };
            record.construction = unavailable;
            record.content_id = fragment_content.clone();
            record.wear = crate::implements::MAX_WAND_WEAR;
            record.strain = crate::implements::MAX_WAND_STRAIN;
        } else {
            next_state.instances.remove(&output.arcane_id);
        }
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let adjacent_vessel = self.adjacent_vessel(pos).ok();
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let clean_owner = ArcaneOwner::Item(output.arcane_id);
        let dross_owner = ArcaneOwner::ItemDross(output.arcane_id);
        let clean = ledger
            .account(&clean_owner)
            .map(|account| account.current.clone())
            .unwrap_or_default();
        let dross = ledger
            .account(&dross_owner)
            .map(|account| account.current.clone())
            .unwrap_or_default();
        if clean.is_empty() && dross.is_empty() {
            return Err("The fitted identity names no finite Current custody.".into());
        }
        let mut debits = BTreeMap::new();
        if !clean.is_empty() {
            add_current(&mut debits, clean_owner, &clean).map_err(|error| error.to_string())?;
        }
        if !dross.is_empty() {
            add_current(&mut debits, dross_owner, &dross).map_err(|error| error.to_string())?;
        }
        let mut credits = BTreeMap::new();
        let mut clean_remaining = clean.clone();
        if leaves_fragment {
            let spark = clean_remaining
                .take_units(STRUCTURAL_SPARK_UNITS, std::iter::empty())
                .map_err(|error| error.to_string())?;
            add_current(&mut credits, ArcaneOwner::Item(output.arcane_id), &spark)
                .map_err(|error| error.to_string())?;
        }
        if let Some((_, vessel, _)) = adjacent_vessel
            && vessel.arcane_id != 0
            && let Some(vessel_instance) = next_state.instance(vessel.arcane_id)
        {
            let vessel_usable = ledger
                .item_clean_total(vessel.arcane_id)
                .map(crate::implements::usable_charge)
                .unwrap_or(0);
            let free = vessel_instance
                .usable_capacity()
                .saturating_sub(vessel_usable);
            let take = free.min(clean_remaining.total());
            if take != 0 {
                let into_vessel = clean_remaining
                    .take_units(take, std::iter::empty())
                    .map_err(|error| error.to_string())?;
                add_current(
                    &mut credits,
                    ArcaneOwner::Item(vessel.arcane_id),
                    &into_vessel,
                )
                .map_err(|error| error.to_string())?;
            }
        }
        if !clean_remaining.is_empty() {
            add_current(&mut credits, ArcaneOwner::Ambient(region), &clean_remaining)
                .map_err(|error| error.to_string())?;
        }
        if !dross.is_empty() {
            add_current(
                &mut credits,
                ArcaneOwner::Dross {
                    region,
                    medium: DrossMedium::Soil,
                },
                &dross,
            )
            .map_err(|error| error.to_string())?;
        }
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "safe_disassembly".into(),
            instance_id: output.arcane_id,
            units: clean.total(),
            dross: dross.total(),
            actor: actor.into(),
            note: format!(
                "frame revision {frame_revision}; {} compatible component(s) recovered, {} retained in conserved bundle",
                recovered_names.len(),
                usize::from(leaves_fragment)
            ),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            if leaves_fragment {
                &fragment_content
            } else {
                &instance.content_id
            },
            "binding-frame safe implement disassembly",
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
        let Some(BlockEntity::BindingFrame(frame)) = self.installations.get_mut(&pos) else {
            return Err("The frame vanished after disassembly committed.".into());
        };
        frame.output = None;
        frame.revision = frame.revision.saturating_add(1);
        let revision = frame.revision;
        for name in &recovered_names {
            let Some(item) = self.reg.item_id(name) else {
                let Some(fragment) = self.reg.item_id("base:implement_fragment") else {
                    continue;
                };
                self.push_drop_at(pos, ItemStack::new(&self.reg, fragment, 1));
                continue;
            };
            let stack = ItemStack::new(&self.reg, item, 1);
            let left = inventory.add_stack(&self.reg, stack);
            if left != 0 {
                self.push_drop_at(
                    pos,
                    ItemStack {
                        count: left,
                        ..stack
                    },
                );
            }
        }
        if leaves_fragment && let Some(fragment) = self.reg.item_id("base:implement_fragment") {
            self.push_drop_at(
                pos,
                ItemStack {
                    item: fragment,
                    count: 1,
                    durability: self.reg.item(fragment).durability,
                    arcane_id: output.arcane_id,
                },
            );
        }
        self.save_entities().map_err(|error| error.to_string())?;
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Use,
            message: format!(
                "Safely disassembled the implement into {} recoverable part(s){}.",
                recovered_names.len(),
                if leaves_fragment {
                    " and one conserved unavailable-component bundle"
                } else {
                    ""
                }
            ),
            preview: self.frame_preview(pos).ok(),
            lines: vec![format!(
                "Returned {} Current units and {} dross units without duplication.",
                clean.total(),
                dross.total()
            )],
        })
    }
}
