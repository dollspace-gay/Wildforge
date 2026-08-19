//! Game-side modular-equipment glue (belt-quest capability E6): mode
//! gating, loadout stats folded into the E4 stat surface, component
//! slotting (in and out intact), repairable frame durability, and loadout
//! presets. The data model lives in `crate::equipment` + the item graph.

use super::*;
use crate::equipment::{LoadoutPreset, PresetSlot};
use crate::inventory::ItemStack;
use crate::stats::StatBlock;

/// Whether modular equipment is live: the world's mode opts in (E1
/// `equipment = true`), the pack ships a frame, and we are not creative.
pub(super) fn equipment_enabled(game: &Game) -> bool {
    if game.creative {
        return false;
    }
    if !game.content.reg.items.iter().any(|item| item.frame.is_some()) {
        return false;
    }
    game.server.world.ruleset().equipment
}

impl Game {
    /// Aggregated stat modifiers from worn equipment. Legacy armor
    /// contributes its item stats whenever worn; modular frames contribute
    /// while functional (durability > 0) plus the stats of every slotted
    /// component. Disabled frames (durability 0) contribute nothing.
    pub(super) fn equipment_stats(&self) -> StatBlock {
        let reg = &self.content.reg;
        let enabled = equipment_enabled(self);
        let mut block = StatBlock::default();
        for (i, slot) in self.survival.armor.iter().enumerate() {
            let Some(frame) = slot else { continue };
            let def = reg.item(frame.item);
            let disabled_frame = enabled && def.frame.is_some() && frame.durability == 0;
            if disabled_frame {
                continue;
            }
            block.add_all(def.stats.iter().copied());
            if enabled {
                block.merge(&self.survival.loadouts[i].stats(reg));
            }
        }
        block
    }

    /// The frame a worn slot holds, if it is a modular frame.
    fn worn_frame(&self, slot: usize) -> Option<crate::equipment::FrameDef> {
        let frame = self.survival.armor[slot].as_ref()?;
        self.content.reg.item(frame.item).frame.clone()
    }

    /// Return every slotted component of one armor slot to the inventory
    /// (dropping what cannot fit), then clear the loadout.
    pub(super) fn return_loadout_components(&mut self, slot: usize) {
        if slot >= 4 {
            self.survival.loadouts[slot].clear();
            return;
        }
        let reg = self.content.reg.clone();
        let stacks: Vec<ItemStack> = {
            let loadout = &mut self.survival.loadouts[slot];
            let mut stacks = Vec::with_capacity(loadout.components.len());
            while let Some(stack) = loadout.unslot(0) {
                stacks.push(stack);
            }
            stacks
        };
        for stack in stacks {
            let leftover = self.inventory.add_stack(&reg, stack);
            if leftover > 0 {
                self.drop_stack(ItemStack {
                    count: leftover,
                    ..stack
                });
            }
        }
    }

    /// Slot the held component stack into a worn frame. On success the held
    /// stack is consumed and the component rides the frame.
    pub(super) fn slot_component(&mut self, slot: usize, held: ItemStack) -> Result<(), String> {
        if !equipment_enabled(self) {
            return Err("Modular equipment is disabled in this world.".into());
        }
        if slot >= 4 {
            return Err("The charm slot takes charms, not components.".into());
        }
        let Some(frame) = self.worn_frame(slot) else {
            return Err("Equip a modular frame in that slot first.".into());
        };
        let reg = &self.content.reg;
        let Some(slot_type) = reg.item(held.item).component.clone() else {
            return Err(format!(
                "{} is not a component.",
                reg.item(held.item).name
            ));
        };
        self.survival.loadouts[slot].slot(&frame, &slot_type, held)?;
        self.sfx(Sfx::Click);
        self.toast(format!(
            "Slotted {}.",
            reg.item(held.item).label
        ));
        Ok(())
    }

    /// Remove a slotted component intact, returning it to the inventory.
    pub(super) fn unslot_component(&mut self, slot: usize, index: usize) -> Result<(), String> {
        if !equipment_enabled(self) {
            return Err("Modular equipment is disabled in this world.".into());
        }
        let reg = self.content.reg.clone();
        let stack = {
            let loadout = &mut self.survival.loadouts[slot];
            loadout.unslot(index).ok_or_else(|| "That slot is empty.".to_string())?
        };
        let name = reg.item(stack.item).label.clone();
        let leftover = self.inventory.add_stack(&reg, stack);
        if leftover > 0 {
            self.drop_stack(ItemStack {
                count: leftover,
                ..stack
            });
        }
        self.sfx(Sfx::Click);
        self.toast(format!("Unslotted {name}."));
        Ok(())
    }

    /// Restore a worn frame's durability to full. The crafting repair path
    /// (damaged frame + repair part) is the durable fix; this bounded engine
    /// action exists so a frame disabled at 0 can rejoin play directly.
    pub(super) fn repair_frame(&mut self, slot: usize) {
        if !equipment_enabled(self) {
            self.toast("Modular equipment is disabled in this world.".into());
            return;
        }
        let Some(frame) = &mut self.survival.armor[slot] else {
            return;
        };
        let def = self.content.reg.item(frame.item);
        if def.frame.is_none() {
            self.toast("That is not a modular frame.".into());
            return;
        }
        if frame.durability == def.durability {
            self.toast(format!("{} is already whole.", def.label));
            return;
        }
        frame.durability = def.durability;
        self.sfx(Sfx::Click);
        self.toast(format!("Repaired {}.", def.label));
    }

    /// Capture the current four-slot loadout as preset `index`.
    pub(super) fn save_loadout_preset(&mut self, index: usize) {
        if !equipment_enabled(self) {
            self.toast("Modular equipment is disabled in this world.".into());
            return;
        }
        let reg = &self.content.reg;
        let mut slots: [Option<PresetSlot>; 4] = Default::default();
        for (i, preset_slot) in slots.iter_mut().enumerate() {
            let Some(frame) = &self.survival.armor[i] else {
                continue;
            };
            let components: Vec<String> = self.survival.loadouts[i]
                .components
                .iter()
                .map(|c| reg.item(c.stack.item).name.clone())
                .collect();
            *preset_slot = Some(PresetSlot {
                frame: reg.item(frame.item).name.clone(),
                components,
            });
        }
        let preset = LoadoutPreset {
            name: format!("PRESET {}", index + 1),
            slots,
        };
        while self.survival.loadout_presets.len() <= index {
            self.survival.loadout_presets.push(LoadoutPreset::default());
        }
        self.survival.loadout_presets[index] = preset;
        self.sfx(Sfx::Click);
        self.toast(format!("Loadout saved as preset {}.", index + 1));
    }

    /// Equip the configuration stored in preset `index`, returning any
    /// missing items as a player-facing error.
    pub(super) fn apply_loadout_preset(&mut self, index: usize) {
        if !equipment_enabled(self) {
            self.toast("Modular equipment is disabled in this world.".into());
            return;
        }
        let Some(preset) = self.survival.loadout_presets.get(index).cloned() else {
            self.toast(format!("Preset {} has not been saved.", index + 1));
            return;
        };
        let reg = self.content.reg.clone();
        // Return the current loadout so nothing is stranded.
        for i in 0..4 {
            self.return_loadout_components(i);
        }
        for i in 0..4 {
            let Some(slot) = &preset.slots[i] else {
                // Unequip whatever is there; the preset leaves the slot empty.
                if let Some(frame) = self.survival.armor[i].take() {
                    self.ui_state.held_stack = Some(frame);
                }
                continue;
            };
            let Some(frame_id) = reg.item_id(&slot.frame) else {
                continue;
            };
            let Some(slot_idx) = self
                .inventory
                .slots
                .iter()
                .position(|s| s.as_ref().is_some_and(|s| s.item == frame_id))
            else {
                self.toast(format!(
                    "Preset {} needs {}.",
                    index + 1,
                    reg.item(frame_id).label
                ));
                continue;
            };
            let frame_stack = self.inventory.slots[slot_idx].take().expect("just located");
            let previous = self.survival.armor[i].take();
            self.survival.armor[i] = Some(frame_stack);
            if let Some(previous) = previous {
                self.ui_state.held_stack = Some(previous);
            }
            let frame = reg.item(frame_id).frame.clone().unwrap_or_default();
            for component_name in &slot.components {
                let Some(component_id) = reg.item_id(component_name) else {
                    continue;
                };
                let Some(component_slot) = self
                    .inventory
                    .slots
                    .iter()
                    .position(|s| s.as_ref().is_some_and(|s| s.item == component_id))
                else {
                    continue;
                };
                let component_stack = self.inventory.slots[component_slot].take().expect("just located");
                if let Err(error) = self.survival.loadouts[i].slot(&frame, component_name, component_stack) {
                    self.toast(format!("Preset {}: {error}", index + 1));
                }
            }
        }
        self.sfx(Sfx::Click);
        self.toast(format!("Applied loadout preset {}.", index + 1));
    }

    // ------- UI geometry for the loadout screen -------

    pub(super) fn loadout_frame_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let (w, h) = (
            self.window.inner_size().width as f32,
            self.window.inner_size().height as f32,
        );
        (w / 2.0 - 430.0, h / 2.0 - 220.0 + i as f32 * 116.0, 220.0, 86.0)
    }

    pub(super) fn loadout_component_rect(&self, slot: usize, index: usize) -> (f32, f32, f32, f32) {
        let base = self.loadout_frame_rect(slot);
        let cols = 3usize;
        let (col, row) = (index % cols, index / cols);
        (base.0 + base.2 + 16.0 + col as f32 * 58.0, base.1 + row as f32 * 58.0, 50.0, 50.0)
    }

    pub(super) fn loadout_repair_rect(&self) -> (f32, f32, f32, f32) {
        let (w, h) = (
            self.window.inner_size().width as f32,
            self.window.inner_size().height as f32,
        );
        (w / 2.0 - 430.0, h / 2.0 + 260.0, 130.0, 34.0)
    }

    pub(super) fn loadout_preset_rect(&self, index: usize) -> (f32, f32, f32, f32) {
        let (w, _h) = (
            self.window.inner_size().width as f32,
            self.window.inner_size().height as f32,
        );
        (
            w / 2.0 + 10.0 + index as f32 * 150.0,
            200.0,
            140.0,
            34.0,
        )
    }

    pub(super) fn loadout_preset_button_rect(&self, index: usize) -> (f32, f32, f32, f32) {
        let (w, h) = (
            self.window.inner_size().width as f32,
            self.window.inner_size().height as f32,
        );
        (
            w / 2.0 + 10.0 + index as f32 * 150.0,
            h / 2.0 + 260.0,
            68.0,
            34.0,
        )
    }

    pub(super) fn loadout_inv_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let (w, h) = (
            self.window.inner_size().width as f32,
            self.window.inner_size().height as f32,
        );
        let cols = 9usize;
        let (col, row) = (i % cols, i / cols);
        (
            w / 2.0 + 180.0 + col as f32 * 52.0,
            h / 2.0 - 220.0 + row as f32 * 52.0,
            48.0,
            48.0,
        )
    }
}