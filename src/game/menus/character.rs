//! Character menu actions.

use crate::audio::Sfx;
use crate::game::Game;
use crate::game::navigation::Screen;
use crate::inventory::TOTAL_SLOTS;

impl Game {
    pub(in crate::game) fn click_dialog_menu(&mut self) {
        // Clicking a choice row runs its callback and advances.
        let Some(sel) = self.dialog_choice_at_click() else {
            return;
        };
        self.sfx(Sfx::Click);
        let (npc, node_id) = match self.ui_state.screen.clone() {
            Screen::Dialog { npc, node_id, .. } => (npc, node_id),
            _ => (0, String::new()),
        };
        self.dialog_select(npc, &node_id, sel);
    }

    pub(in crate::game) fn click_skills_menu(&mut self) {
        if !self.skills_enabled() {
            self.set_screen(Screen::Playing);
            return;
        }
        let tree = &self.content.reg.skills;
        let branches = &tree.branches;
        if branches.is_empty() {
            return;
        }
        let branch_idx = self.ui_state.skills_branch.min(branches.len() - 1);
        for (i, _) in branches.iter().enumerate() {
            if self.hit(self.skill_branch_tab_rect(i)) {
                self.sfx(Sfx::Click);
                self.ui_state.skills_branch = i;
                return;
            }
        }
        if self.hit(self.skill_respec_rect()) {
            self.respec_skills();
            return;
        }
        let nodes: Vec<_> = tree
            .nodes
            .iter()
            .filter(|n| n.branch == branches[branch_idx].id)
            .collect();
        for (i, node) in nodes.iter().enumerate() {
            if self.hit(self.skill_node_rect(i)) {
                let node_id = node.id.clone();
                if let Err(message) = self.allocate_skill(&node_id) {
                    self.toast(message);
                }
                return;
            }
        }
    }

    pub(in crate::game) fn click_loadout_menu(&mut self) {
        if !crate::game::equipment::equipment_enabled(self) {
            self.set_screen(Screen::Playing);
            return;
        }
        // Frame boxes: swap / take off with the held stack.
        for i in 0..4 {
            if !self.hit(self.loadout_frame_rect(i)) {
                continue;
            }
            self.ui_state.loadout_select = i;
            self.armor_click(i);
            self.sfx(Sfx::Click);
            return;
        }
        // Component sub-slots: slot the held component, or unslot one.
        for i in 0..4 {
            let frame = self.survival.armor[i].as_ref();
            let slot_types: Vec<(usize, String)> = frame
                .and_then(|f| self.content.reg.item(f.item).frame.clone())
                .map(|f| {
                    f.slots
                        .iter()
                        .flat_map(|s| (0..s.max).map(move |_| s.slot_type.clone()))
                        .enumerate()
                        .collect()
                })
                .unwrap_or_default();
            for (index, _) in slot_types {
                if !self.hit(self.loadout_component_rect(i, index)) {
                    continue;
                }
                let held = self.ui_state.held_stack.take();
                if let Some(held) = held {
                    if let Err(message) = self.slot_component(i, held) {
                        self.ui_state.held_stack = Some(held);
                        self.toast(message);
                    }
                } else {
                    let _ = self.unslot_component(i, index);
                }
                return;
            }
        }
        if self.hit(self.loadout_repair_rect()) {
            self.repair_frame(self.ui_state.loadout_select);
            return;
        }
        for index in 0..4 {
            if self.hit(self.loadout_preset_rect(index)) {
                self.ui_state.loadout_preset_sel = index;
                self.sfx(Sfx::Click);
                return;
            }
        }
        for index in 0..4 {
            if self.hit(self.loadout_preset_button_rect(index)) {
                match index {
                    0 => self.save_loadout_preset(self.ui_state.loadout_preset_sel),
                    _ => self.apply_loadout_preset(self.ui_state.loadout_preset_sel),
                }
                return;
            }
        }
        for i in 0..TOTAL_SLOTS {
            if self.hit(self.loadout_inv_rect(i)) {
                self.inventory_click(false, i, false);
                return;
            }
        }
    }
}
