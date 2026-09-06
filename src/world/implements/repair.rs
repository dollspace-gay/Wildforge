//! Repair implements transaction coordination.

use super::add_current;
use super::transaction_from_maps;
use crate::arcane::ArcaneOwner;
use crate::arcane::LinkedFileReplacement;
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
    pub(super) fn repair_at_frame(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
        selected_slot: usize,
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
                    .ok_or("Fit a worn implement in the finished cradle.")?,
                frame.revision,
            ),
            _ => return Err("The frame has no mounts.".into()),
        };
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let instance = next_state
            .instance(output.arcane_id)
            .cloned()
            .ok_or("The fitted implement has no construction record.")?;
        if instance.wear == 0 && instance.strain == 0 {
            return Err("The fitted implement is already sound.".into());
        }
        let mut repair_materials = Vec::new();
        match &instance.kind {
            ImplementKind::Wand { parts, .. } => {
                for id in [&parts.body, &parts.binding] {
                    if let Some(component) = self
                        .reg
                        .item_id(id)
                        .and_then(|item| self.reg.item(item).wand_component.as_ref())
                        .or_else(|| next_state.component_manifests.get(id))
                    {
                        repair_materials.push(component.repair_material.clone());
                    }
                }
            }
            ImplementKind::Charm { .. } => repair_materials.push("base:leather_strip".into()),
            ImplementKind::Vessel { .. } => repair_materials.push("base:glass".into()),
            ImplementKind::Fragments { .. } => {
                return Err("Sort the conserved fragment bundle with safe disassembly before attempting repair.".into());
            }
        }
        let hammer_slot = (0..inventory.slots.len())
            .find(|&slot| {
                inventory.slots[slot].is_some_and(|stack| self.reg.item(stack.item).hammer)
            })
            .ok_or("Repair requires a hammer in the pack.")?;
        let material_slot = if inventory.slots[selected_slot]
            .is_some_and(|stack| repair_materials.contains(&self.reg.item(stack.item).name))
        {
            Some(selected_slot)
        } else {
            (0..inventory.slots.len()).find(|&slot| {
                inventory.slots[slot]
                    .is_some_and(|stack| repair_materials.contains(&self.reg.item(stack.item).name))
            })
        }
        .ok_or_else(|| format!("Repair needs one of: {}.", repair_materials.join(", ")))?;
        let repair_stack = inventory.slots[material_slot]
            .ok_or("The matching repair material moved before use.")?;
        let repair_matter = crate::materials::stack_materials(
            &self.reg,
            ItemStack {
                count: 1,
                ..repair_stack
            },
        );
        let staged_material = if repair_matter.is_empty() {
            None
        } else {
            self.material_ledger
                .as_ref()
                .ok_or("The world has no finite material ledger.")?
                .stage_linked_recipe_loss(&repair_matter)
                .map_err(|error| error.to_string())?
        };
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let target_owner = ArcaneOwner::Item(output.arcane_id);
        let mut debits = BTreeMap::new();
        let mut credits = BTreeMap::new();
        let mut routed = 0u64;
        let mut retained_dross = 0u64;
        if repair_stack.arcane_id != 0 {
            let target_free = instance.usable_capacity().saturating_sub(
                ledger
                    .item_clean_total(output.arcane_id)
                    .map(crate::implements::usable_charge)
                    .unwrap_or(0),
            );
            if let Some(account) = ledger.account(&ArcaneOwner::Item(repair_stack.arcane_id)) {
                let mut incoming = account.current.clone();
                add_current(
                    &mut debits,
                    ArcaneOwner::Item(repair_stack.arcane_id),
                    &incoming,
                )
                .map_err(|error| error.to_string())?;
                let into_target = incoming
                    .take_units(target_free.min(incoming.total()), std::iter::empty())
                    .map_err(|error| error.to_string())?;
                routed = into_target.total();
                if !into_target.is_empty() {
                    add_current(&mut credits, target_owner.clone(), &into_target)
                        .map_err(|error| error.to_string())?;
                }
                if !incoming.is_empty() {
                    add_current(&mut credits, ArcaneOwner::Ambient(region), &incoming)
                        .map_err(|error| error.to_string())?;
                }
            }
            if let Some(account) = ledger.account(&ArcaneOwner::ItemDross(repair_stack.arcane_id)) {
                retained_dross = account.current.total();
                add_current(
                    &mut debits,
                    ArcaneOwner::ItemDross(repair_stack.arcane_id),
                    &account.current,
                )
                .map_err(|error| error.to_string())?;
                add_current(
                    &mut credits,
                    ArcaneOwner::ItemDross(output.arcane_id),
                    &account.current,
                )
                .map_err(|error| error.to_string())?;
            }
        }
        if debits.is_empty() {
            let account = ledger
                .account(&target_owner)
                .ok_or("The fitted implement has no structural spark.")?;
            let mut pulse = account.current.clone();
            let pulse = pulse
                .take_units(STRUCTURAL_SPARK_UNITS, std::iter::empty())
                .map_err(|error| error.to_string())?;
            add_current(&mut debits, target_owner.clone(), &pulse)
                .map_err(|error| error.to_string())?;
            add_current(&mut credits, target_owner.clone(), &pulse)
                .map_err(|error| error.to_string())?;
        }
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        let record = next_state
            .instances
            .get_mut(&output.arcane_id)
            .ok_or("The fitted implement vanished from state.")?;
        let wear_before = record.wear;
        let strain_before = record.strain;
        record.wear = record.wear.saturating_sub(250);
        record.strain = record.strain.saturating_sub(1_000);
        let wear_after = record.wear;
        let strain_after = record.strain;
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "repair".into(),
            instance_id: output.arcane_id,
            units: routed,
            dross: retained_dross,
            actor: actor.into(),
            note: format!(
                "{}; wear {wear_before}->{}, strain {strain_before}->{}; frame revision {frame_revision}",
                self.reg.item(repair_stack.item).name,
                wear_after,
                strain_after
            ),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &instance.content_id,
            "binding-frame matching-material repair",
        )?;
        let replacement = LinkedFileReplacement {
            subsystem: "implements".into(),
            operation_id,
            relative_path: crate::implements::IMPLEMENTS_FILE.into(),
            after: Some(next_state.encode().map_err(|error| error.to_string())?),
        };
        let mut replacements = vec![replacement];
        if let Some((_, bytes)) = &staged_material {
            replacements.push(LinkedFileReplacement {
                subsystem: "materials".into(),
                operation_id,
                relative_path: crate::materials::MaterialLedger::linked_delta_path().into(),
                after: Some(bytes.clone()),
            });
        }
        ledger
            .commit_linked_files(transaction, replacements)
            .map_err(|error| error.to_string())?;
        self.implements_state = Some(next_state);
        if let Some((next_material, _)) = staged_material {
            self.material_ledger = Some(next_material);
        }
        inventory.take_one_stack(material_slot);
        inventory.wear_tool(&self.reg, hammer_slot);
        let revision =
            if let Some(BlockEntity::BindingFrame(frame)) = self.installations.get_mut(&pos) {
                frame.revision = frame.revision.saturating_add(1);
                frame.revision
            } else {
                frame_revision
            };
        self.save_entities().map_err(|error| error.to_string())?;
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Strain,
            message: format!(
                "Repaired the implement with {}.",
                self.reg.item(repair_stack.item).label
            ),
            preview: self.frame_preview(pos).ok(),
            lines: vec![format!(
                "Routed {routed} carried Current; retained {retained_dross} dross."
            )],
        })
    }
}
