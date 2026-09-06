//! Focus swap implements transaction coordination.

use super::add_current;
use super::physical_component;
use super::transaction_from_maps;
use crate::arcane::ArcaneOwner;
use crate::arcane::Current;
use crate::arcane::LinkedFileReplacement;
use crate::implements::ComponentRole;
use crate::implements::FrameResult;
use crate::implements::ImplementAuditEvent;
use crate::implements::ImplementCue;
use crate::implements::ImplementKind;
use crate::implements::STRUCTURAL_SPARK_UNITS;
use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::world::BlockEntity;
use crate::world::World;
use std::collections::BTreeMap;

impl World {
    pub(super) fn swap_focus_at_frame(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
        actor: &str,
    ) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        let (output, replacement_focus, frame_revision) = match self.installations.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => (
                frame
                    .output
                    .ok_or("Fit a bound wand in the finished cradle.")?,
                frame.focus.ok_or("Mount the replacement focus first.")?,
                frame.revision,
            ),
            _ => return Err("The frame has no mounts.".into()),
        };
        let replacement_definition = self
            .reg
            .item(replacement_focus.item)
            .wand_component
            .clone()
            .filter(|definition| definition.role == ComponentRole::Focus)
            .ok_or("The focus mount does not contain a declared focus.")?;
        let replacement_id = self.reg.item(replacement_focus.item).name.clone();
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let instance = next_state
            .instance(output.arcane_id)
            .cloned()
            .ok_or("The fitted wand has no construction record.")?;
        let ImplementKind::Wand { parts, .. } = instance.kind else {
            return Err("Only a bound wand has a replaceable focus.".into());
        };
        let old_focus = parts.focus.clone();
        let old_focus_item = self.reg.item_id(&old_focus).ok_or(
            "The installed focus's content pack is unavailable; safely disassemble the wand so its exact matter remains in a conserved fragment bundle.",
        )?;
        let component = |id: &str| {
            self.reg
                .item_id(id)
                .and_then(|item| self.reg.item(item).wand_component.clone())
                .or_else(|| next_state.component_manifests.get(id).cloned())
                .ok_or_else(|| format!("The saved component definition for {id} is unavailable."))
        };
        let declared = [
            (parts.body.clone(), component(&parts.body)?),
            (parts.reservoir.clone(), component(&parts.reservoir)?),
            (replacement_id.clone(), replacement_definition.clone()),
            (parts.binding.clone(), component(&parts.binding)?),
        ];
        let (new_parts, resolved) =
            crate::implements::resolve_wand(&declared).map_err(|error| error.to_string())?;
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let wand_owner = ArcaneOwner::Item(output.arcane_id);
        let wand_total = ledger
            .item_clean_total(output.arcane_id)
            .ok_or("The wand's Current account is missing.")?;
        let wand_usable = crate::implements::usable_charge(wand_total);
        let mut debits = BTreeMap::new();
        let mut credits = BTreeMap::new();
        let mut incoming_clean = Current::default();
        let mut incoming_dross = Current::default();
        if replacement_focus.arcane_id != 0 {
            for (owner, dross) in [
                (ArcaneOwner::Item(replacement_focus.arcane_id), false),
                (ArcaneOwner::ItemDross(replacement_focus.arcane_id), true),
            ] {
                if let Some(account) = ledger.account(&owner) {
                    add_current(&mut debits, owner, &account.current)
                        .map_err(|error| error.to_string())?;
                    if dross {
                        incoming_dross
                            .checked_add(&account.current)
                            .map_err(|error| error.to_string())?;
                    } else {
                        incoming_clean
                            .checked_add(&account.current)
                            .map_err(|error| error.to_string())?;
                    }
                }
            }
        }
        let free = resolved.capacity.saturating_sub(wand_usable);
        let into_wand_units = free.min(incoming_clean.total());
        let into_wand = if into_wand_units == 0 {
            Current::default()
        } else {
            incoming_clean
                .take_units(into_wand_units, resolved.resonance.keys().cloned())
                .map_err(|error| error.to_string())?
        };
        if !into_wand.is_empty() {
            add_current(&mut credits, wand_owner.clone(), &into_wand)
                .map_err(|error| error.to_string())?;
        }
        if !incoming_dross.is_empty() {
            add_current(
                &mut credits,
                ArcaneOwner::ItemDross(output.arcane_id),
                &incoming_dross,
            )
            .map_err(|error| error.to_string())?;
        }
        if !incoming_clean.is_empty() {
            add_current(&mut credits, ArcaneOwner::Ambient(region), &incoming_clean)
                .map_err(|error| error.to_string())?;
        }
        if debits.is_empty() {
            let account = ledger
                .account(&wand_owner)
                .ok_or("The wand has no structural spark.")?;
            let mut one = account.current.clone();
            let one = one
                .take_units(STRUCTURAL_SPARK_UNITS, std::iter::empty())
                .map_err(|error| error.to_string())?;
            add_current(&mut debits, wand_owner.clone(), &one)
                .map_err(|error| error.to_string())?;
            add_current(&mut credits, wand_owner.clone(), &one)
                .map_err(|error| error.to_string())?;
        }
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        next_state
            .component_manifests
            .insert(replacement_id.clone(), replacement_definition);
        let Some(record) = next_state.instances.get_mut(&output.arcane_id) else {
            return Err("The fitted wand vanished from implement state.".into());
        };
        record.kind = ImplementKind::Wand {
            parts: new_parts,
            resolved: resolved.clone(),
        };
        let saved_focus = record
            .construction
            .iter_mut()
            .find(|component| component.content_id == old_focus)
            .ok_or("The wand's saved physical focus bill is missing.")?;
        if self.reg.item(old_focus_item).materials != saved_focus.materials {
            return Err("The installed focus's material identity changed; an explicit content migration is required.".into());
        }
        *saved_focus = physical_component(&self.reg, replacement_focus);
        record.strain = record
            .strain
            .saturating_add(20)
            .min(crate::implements::MAX_WAND_STRAIN);
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "swap_focus".into(),
            instance_id: output.arcane_id,
            units: into_wand.total(),
            dross: incoming_dross.total(),
            actor: actor.into(),
            note: format!("{old_focus} -> {replacement_id}; frame revision {frame_revision}"),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &instance.content_id,
            "binding-frame focus swap",
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
            return Err("The frame vanished after focus swap committed.".into());
        };
        frame.focus = None;
        frame.revision = frame.revision.saturating_add(1);
        let revision = frame.revision;
        let old_stack = ItemStack::new(&self.reg, old_focus_item, 1);
        let left = inventory.add_stack(&self.reg, old_stack);
        if left != 0 {
            self.push_drop_at(
                pos,
                ItemStack {
                    count: left,
                    ..old_stack
                },
            );
        }
        self.save_entities().map_err(|error| error.to_string())?;
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Use,
            message: format!(
                "Rebound the wand around {}.",
                self.reg.item(replacement_focus.item).label
            ),
            preview: Some(resolved),
            lines: vec![format!(
                "Recovered {}; routed {} Current and {} retained dross from the replacement.",
                old_focus,
                into_wand.total(),
                incoming_dross.total()
            )],
        })
    }
}
