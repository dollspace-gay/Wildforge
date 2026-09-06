//! Storage panels layout and UI composition.

use crate::game::widgets;
use crate::inventory::ItemStack;
use crate::ui::UiBatch;
use crate::world;
use crate::game::Game;

impl Game {
    pub(in crate::game) fn draw_stall_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32, pos: crate::planet::BlockPos) {

        ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
        let mine = self.multiplayer.remote.is_none() && self.stall_is_mine(pos);
        let (slots, owner_name, remote_mine) = {
            match self.runtime.view().block_entity_at(&pos) {
                Some(world::BlockEntity::Stall(st)) => {
                    let mut v: Vec<Option<ItemStack>> = st.goods.to_vec();
                    v.push(st.price);
                    v.extend(st.till.iter().copied());
                    (v, st.owner_name.clone(), st.owner == [1; 16])
                }
                _ => (vec![None; 13], String::new(), false),
            }
        };
        let mine = mine || remote_mine;
        let title = if owner_name.is_empty() {
            "MARKET STALL".to_string()
        } else {
            format!("{}'S STALL", owner_name.to_uppercase())
        };
        let tw = UiBatch::text_width(3.0, &title);
        ui.text_shadow((w - tw) / 2.0, h / 2.0 - 300.0, 3.0, &title, [1.0; 4]);
        ui.text_shadow(w / 2.0 - 150.0, h / 2.0 - 278.0, 1.5, "GOODS", [1.0; 4]);
        ui.text_shadow(
            w / 2.0 - 150.0,
            h / 2.0 - 196.0,
            1.5,
            "PRICE EACH",
            [1.0; 4],
        );
        if mine {
            ui.text_shadow(w / 2.0 - 150.0, h / 2.0 - 114.0, 1.5, "TILL", [1.0; 4]);
        }
        for (i, s) in slots.iter().enumerate() {
            if !mine && i >= 7 {
                break; // the till is the owner's business
            }
            let r = self.stall_slot_rect(i);
            widgets::slot(&self.content.reg, &mut *ui, r, *s, false, self.hit(r));
        }
        if !mine {
            let br = self.stall_buy_rect();
            widgets::button(&mut *ui, br, "BUY", self.hit(br));
        }
        self.draw_player_inventory(&mut *ui);
        widgets::held_stack(&self.content.reg, &mut *ui, self.input.ui_cursor, self.ui_state.held_stack);
    }
    pub(in crate::game) fn draw_mob_cargo_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32, id: u32) {

        ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
        let title = "SADDLEBAGS";
        let tw = UiBatch::text_width(3.0, title);
        ui.text_shadow((w - tw) / 2.0, h / 2.0 - 300.0, 3.0, title, [1.0; 4]);
        let slots: [Option<ItemStack>; 12] = self.runtime.view().mob_by_id(id)
            .and_then(|m| m.cargo.as_deref().copied())
            .unwrap_or_default();
        for (i, s) in slots.iter().enumerate() {
            let r = self.mob_cargo_slot_rect(i);
            widgets::slot(&self.content.reg, &mut *ui, r, *s, false, self.hit(r));
        }
        self.draw_player_inventory(&mut *ui);
        widgets::held_stack(&self.content.reg, &mut *ui, self.input.ui_cursor, self.ui_state.held_stack);
    }
    pub(in crate::game) fn draw_chest_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32, pos: crate::planet::BlockPos) {

        ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
        let title = "CHEST";
        let tw = UiBatch::text_width(3.0, title);
        ui.text_shadow((w - tw) / 2.0, h / 2.0 - 340.0, 3.0, title, [1.0; 4]);
        let slots = match self.runtime.view().block_entity_at(&pos) {
            Some(world::BlockEntity::Chest(c)) => c.slots,
            _ => [None; world::CHEST_SLOTS],
        };
        for (i, st) in slots.iter().enumerate() {
            let r = self.chest_slot_rect(i);
            widgets::slot(&self.content.reg, &mut *ui, r, *st, false, self.hit(r));
        }
        self.draw_player_inventory(&mut *ui);
        self.draw_browser(&mut *ui);
        widgets::held_stack(&self.content.reg, &mut *ui, self.input.ui_cursor, self.ui_state.held_stack);
    }
    pub(in crate::game) fn draw_offering_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32, pos: crate::planet::BlockPos) {

        ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
        let title = "OFFERING STONE";
        let tw = UiBatch::text_width(3.0, title);
        ui.text_shadow((w - tw) / 2.0, h / 2.0 - 260.0, 3.0, title, [1.0; 4]);
        let hint = "LEFT AT DUSK, TAKEN BY DAWN";
        let hw2 = UiBatch::text_width(1.5, hint);
        ui.text_shadow(
            (w - hw2) / 2.0,
            h / 2.0 - 232.0,
            1.5,
            hint,
            [0.7, 0.85, 0.65, 1.0],
        );
        // The stone states the season's appetite plainly.
        let (_, want_line) = self.runtime.view().season_want_at_surface(self.player.pos.surface());
        let want_line = want_line.to_uppercase();
        let ww = UiBatch::text_width(1.5, &want_line);
        ui.text_shadow(
            (w - ww) / 2.0,
            h / 2.0 - 208.0,
            1.5,
            &want_line,
            [0.85, 0.8, 0.55, 1.0],
        );
        let slots = match self.runtime.view().block_entity_at(&pos) {
            Some(world::BlockEntity::Offering(o)) => o.slots,
            _ => [None; 3],
        };
        for (i, st) in slots.iter().enumerate() {
            let r = self.offering_slot_rect(i);
            widgets::slot(&self.content.reg, &mut *ui, r, *st, false, self.hit(r));
        }
        self.draw_player_inventory(&mut *ui);
        self.draw_browser(&mut *ui);
        widgets::held_stack(&self.content.reg, &mut *ui, self.input.ui_cursor, self.ui_state.held_stack);
    }
}
