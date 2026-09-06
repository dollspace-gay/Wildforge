//! Storage menu actions.

use crate::audio::Sfx;
use crate::inventory::TOTAL_SLOTS;
use crate::net;
use crate::world;
use crate::game::Game;

impl Game {
    pub(in crate::game) fn click_stall_menu(&mut self , right: bool, pos: crate::planet::BlockPos) {
                if self.hit(self.stall_buy_rect()) {
                    self.sfx(Sfx::Click);
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.session.send(&net::C2S::StallBuy { pos });
                    } else {
                        self.stall_buy_local(pos);
                    }
                    return;
                }
                for i in 0..13 {
                    if self.hit(self.stall_slot_rect(i)) {
                        self.stall_click(pos, i, right);
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

    pub(in crate::game) fn click_mob_cargo_menu(&mut self , right: bool, id: u32) {
                for i in 0..12 {
                    if self.hit(self.mob_cargo_slot_rect(i)) {
                        self.mob_cargo_click(id, i, right);
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

    pub(in crate::game) fn click_chest_menu(&mut self , right: bool, pos: crate::planet::BlockPos) {
                if self.browser_click(right) {
                    return;
                }
                for i in 0..world::CHEST_SLOTS {
                    if self.hit(self.chest_slot_rect(i)) {
                        self.chest_click(pos, i, right);
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

    pub(in crate::game) fn click_offering_menu(&mut self , right: bool, pos: crate::planet::BlockPos) {
                if self.browser_click(right) {
                    return;
                }
                for i in 0..3 {
                    if self.hit(self.offering_slot_rect(i)) {
                        self.offering_click(pos, i, right);
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
}
