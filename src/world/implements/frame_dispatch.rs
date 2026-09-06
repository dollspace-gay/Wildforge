//! Frame dispatch implements transaction coordination.

use crate::world::BlockEntity;
use crate::world::BlockPos;
use crate::implements::ComponentRole;
use crate::implements::FrameAction;
use crate::implements::FrameResult;
use crate::implements::ImplementCue;
use crate::implements::ImplementKind;
use crate::world::ItemStack;
use crate::world::World;

impl World {
    pub fn operate_binding_frame(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
        selected_slot: usize,
        action: FrameAction,
        expected_revision: Option<u64>,
        actor: &str,
    ) -> Result<FrameResult, String> {
        if selected_slot >= inventory.slots.len() {
            return Err("That inventory slot does not exist.".into());
        }
        if self
            .reg
            .block(self.get_block_at(pos))
            .interaction
            .as_deref()
            != Some("binding_frame")
        {
            return Err("There is no binding frame there.".into());
        }
        let current_revision = match self.installations.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => frame.revision,
            Some(_) => return Err("Another block entity occupies the frame.".into()),
            None => 0,
        };
        if action != FrameAction::Inspect && expected_revision.is_none() && current_revision != 0 {
            let mut result = self.inspect_binding_frame(pos)?;
            result.success = false;
            result.cue = ImplementCue::Strain;
            result.message =
                "The frame has prior changes; its authoritative state was inspected without mutating it. Repeat the operation.".into();
            return Ok(result);
        }
        if action != FrameAction::Inspect
            && expected_revision.is_some_and(|revision| revision != current_revision)
        {
            let mut result = self.inspect_binding_frame(pos)?;
            result.success = false;
            result.cue = ImplementCue::Strain;
            result.message =
                "The frame changed before that operation arrived; its current state was returned without mutating it. Repeat the operation.".into();
            return Ok(result);
        }
        let action = if action == FrameAction::Contextual {
            self.contextual_frame_action(pos, inventory.slots[selected_slot])
        } else {
            action
        };
        match action {
            FrameAction::Contextual => unreachable!("contextual action resolves above"),
            FrameAction::ExchangeSelected => {
                self.exchange_frame_item(pos, inventory, selected_slot)
            }
            FrameAction::Assemble => self.assemble_wand(pos, actor),
            FrameAction::Calibrate => self.calibrate_frame_or_vessel(pos, actor),
            FrameAction::Inspect => self.inspect_binding_frame(pos),
            FrameAction::BindCharm => self.bind_charm_at_frame(pos, actor),
            FrameAction::Transfer => {
                self.transfer_at_frame(pos, inventory.slots[selected_slot], actor)
            }
            FrameAction::SafeDischarge => self.discharge_at_frame(pos, actor),
            FrameAction::Disassemble => {
                self.disassemble_at_frame(pos, inventory, inventory.slots[selected_slot], actor)
            }
            FrameAction::SwapFocus => self.swap_focus_at_frame(pos, inventory, actor),
            FrameAction::Repair => self.repair_at_frame(pos, inventory, selected_slot, actor),
        }
    }

    pub(super) fn contextual_frame_action(&self, pos: BlockPos, held: Option<ItemStack>) -> FrameAction {
        if let Some(stack) = held {
            let definition = self.reg.item(stack.item);
            if definition
                .discovery
                .as_ref()
                .is_some_and(|definition| definition.kind == "tuning_lens")
            {
                return FrameAction::Inspect;
            }
            if definition.shears {
                return FrameAction::Disassemble;
            }
            if definition.hammer {
                return FrameAction::Repair;
            }
            if stack.arcane_id != 0 && definition.arcane.is_some() {
                return FrameAction::Transfer;
            }
            if definition.name == "base:still_salt" {
                return FrameAction::SafeDischarge;
            }
            return FrameAction::ExchangeSelected;
        }
        let Some(BlockEntity::BindingFrame(frame)) = self.installations.get(&pos) else {
            return FrameAction::Calibrate;
        };
        if let Some(output) = frame.output {
            let name = self.reg.item(output.item).name.as_str();
            if matches!(
                name,
                "base:quiet_charm_blank" | "base:bark_charm_blank" | "base:hunger_charm_blank"
            ) && frame.mounts().into_iter().flatten().next().is_some()
            {
                return FrameAction::BindCharm;
            }
            if self
                .implements_state
                .as_ref()
                .and_then(|state| state.instance(output.arcane_id))
                .is_some_and(|instance| matches!(instance.kind, ImplementKind::Wand { .. }))
                && frame.focus.is_some()
            {
                return FrameAction::SwapFocus;
            }
            return FrameAction::ExchangeSelected;
        }
        if frame.mounts().into_iter().all(|mount| mount.is_some()) {
            FrameAction::Assemble
        } else if frame.is_empty() {
            FrameAction::Calibrate
        } else {
            FrameAction::ExchangeSelected
        }
    }

    pub(super) fn exchange_frame_item(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
        selected_slot: usize,
    ) -> Result<FrameResult, String> {
        let selected = inventory.slots[selected_slot];
        self.installations
            .entry(pos)
            .or_insert_with(|| BlockEntity::BindingFrame(Default::default()));
        let Some(BlockEntity::BindingFrame(frame)) = self.installations.get_mut(&pos) else {
            return Err("Another block entity occupies the frame.".into());
        };
        let message = if let Some(stack) = selected {
            let definition = self.reg.item(stack.item);
            if let Some(component) = &definition.wand_component {
                let mount = match component.role {
                    ComponentRole::Body => &mut frame.body,
                    ComponentRole::Reservoir => &mut frame.reservoir,
                    ComponentRole::Focus => &mut frame.focus,
                    ComponentRole::Binding => &mut frame.binding,
                };
                if mount.is_some() {
                    return Err(format!(
                        "The {} mount is already occupied.",
                        component.role.label()
                    ));
                }
                *mount = inventory.take_one_stack(selected_slot);
                format!(
                    "Mounted {} as the {}.",
                    definition.label,
                    component.role.label()
                )
            } else if definition.implement.is_some()
                || matches!(
                    definition.name.as_str(),
                    "base:quiet_charm_blank" | "base:bark_charm_blank" | "base:hunger_charm_blank"
                )
            {
                if frame.output.is_some() {
                    return Err("The finished-object cradle is occupied.".into());
                }
                frame.output = inventory.take_one_stack(selected_slot);
                format!("Placed {} in the finished-object cradle.", definition.label)
            } else {
                return Err("That item fits none of the frame's physical mounts.".into());
            }
        } else {
            let retrieved = frame
                .output
                .take()
                .or_else(|| frame.binding.take())
                .or_else(|| frame.focus.take())
                .or_else(|| frame.reservoir.take())
                .or_else(|| frame.body.take())
                .ok_or("Every frame mount is empty.")?;
            inventory.slots[selected_slot] = Some(retrieved);
            format!(
                "Retrieved {} from the frame.",
                self.reg.item(retrieved.item).label
            )
        };
        frame.revision = frame.revision.saturating_add(1);
        let revision = frame.revision;
        self.save_entities().map_err(|error| error.to_string())?;
        let preview = self.frame_preview(pos).ok();
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Use,
            message,
            preview,
            lines: Vec::new(),
        })
    }

    pub(super) fn frame_preview(&self, pos: BlockPos) -> Result<crate::implements::ResolvedWand, String> {
        let Some(BlockEntity::BindingFrame(frame)) = self.installations.get(&pos) else {
            return Err("The frame has no mounts.".into());
        };
        let stacks = frame.mounts();
        let mut parts: Vec<(String, crate::implements::WandComponentDef)> = Vec::new();
        for stack in stacks {
            let stack = stack.ok_or("Install one component in every role mount.")?;
            let definition = self.reg.item(stack.item);
            let component = definition
                .wand_component
                .clone()
                .ok_or("A mounted item is not a wand component.")?;
            parts.push((definition.name.clone(), component));
        }
        let parts: [(String, crate::implements::WandComponentDef); 4] = parts
            .try_into()
            .map_err(|_| "The frame must hold exactly four parts.".to_string())?;
        crate::implements::resolve_wand(&parts)
            .map(|(_, resolved)| resolved)
            .map_err(|error| error.to_string())
    }

    pub(super) fn inspect_binding_frame(&self, pos: BlockPos) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        let revision = match self.installations.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => frame.revision,
            _ => 0,
        };
        let preview = self.frame_preview(pos).ok();
        let mut lines = if layout.valid {
            vec![format!(
                "Frame embodied: {} conductors, containment {} permille.",
                layout.network_size, layout.containment
            )]
        } else {
            layout.problems.clone()
        };
        if let Some(BlockEntity::BindingFrame(frame)) = self.installations.get(&pos)
            && let Some(output) = frame.output
            && output.arcane_id != 0
            && let (Some(state), Some(ledger)) = (&self.implements_state, &self.arcane_ledger)
            && let Some(instance) = state.instance(output.arcane_id)
        {
            let clean = ledger.item_clean_total(output.arcane_id).unwrap_or(0);
            let dross = ledger.item_dross_total(output.arcane_id);
            lines.extend(crate::implements::tooltip(instance, clean, dross, false));
        }
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Use,
            message: if layout.valid {
                "The binding frame is physically complete.".into()
            } else {
                "The binding frame is incomplete.".into()
            },
            preview,
            lines,
        })
    }
}
