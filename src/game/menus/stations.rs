//! Stations menu actions.

use crate::audio::Sfx;
use crate::inventory::TOTAL_SLOTS;
use crate::game::Game;

impl Game {
    pub(in crate::game) fn click_furnace_menu(&mut self , right: bool, pos: crate::planet::BlockPos) {
                if self.browser_click(right) {
                    return;
                }
                for i in 0..3 {
                    if self.hit(self.furnace_slot_rect(i)) {
                        self.furnace_click(pos, i, right);
                        return;
                    }
                }
                for i in 0..TOTAL_SLOTS {
                    if self.hit(self.inventory_layout().slot_rect(i)) {
                        self.inventory_click(false, i, right);
                        return;
                    }
                }
                }

    pub(in crate::game) fn click_bloomery_menu(&mut self , right: bool, pos: crate::planet::BlockPos) {
                if self.browser_click(right) {
                    return;
                }
                if self.hit(self.bloomery_light_rect()) {
                    self.sfx(Sfx::Click);
                    self.light_bloomery_action(pos);
                    return;
                }
                for i in 0..8 {
                    if self.hit(self.bloomery_slot_rect(i)) {
                        self.bloomery_click(pos, i, right);
                        return;
                    }
                }
                for i in 0..TOTAL_SLOTS {
                    if self.hit(self.inventory_layout().slot_rect(i)) {
                        self.inventory_click(false, i, right);
                        return;
                    }
                }
                }

    pub(in crate::game) fn click_kiln_menu(&mut self , right: bool, pos: crate::planet::BlockPos) {
                if self.browser_click(right) {
                    return;
                }
                if self.hit(self.bloomery_light_rect()) {
                    self.sfx(Sfx::Click);
                    self.light_bloomery_action(pos);
                    return;
                }
                for i in 0..9 {
                    if self.hit(self.kiln_slot_rect(i)) {
                        self.kiln_click(pos, i, right);
                        return;
                    }
                }
                for i in 0..TOTAL_SLOTS {
                    if self.hit(self.inventory_layout().slot_rect(i)) {
                        self.inventory_click(false, i, right);
                        return;
                    }
                }
                }

    pub(in crate::game) fn click_workbench_menu(&mut self , right: bool, pos: crate::planet::BlockPos) {
                if self.browser_click(right) {
                    return;
                }
                for i in 0..20 {
                    if self.hit(self.workbench_recipe_rect(i)) {
                        self.workbench_craft(pos, i);
                        return;
                    }
                }
                for i in 0..TOTAL_SLOTS {
                    if self.hit(self.inventory_layout().slot_rect(i)) {
                        self.inventory_click(false, i, right);
                        return;
                    }
                }
                }

    pub(in crate::game) fn click_mod_menu(&mut self , right: bool) {
                // Capability E11: widget rows first, then the shared
                // inventory grid beneath.
                if self.browser_click(right) {
                    return;
                }
                self.mod_screen_click();
                }
}
