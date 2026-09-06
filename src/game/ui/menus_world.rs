//! Menus world layout and UI composition.

use crate::game::widgets;
use crate::ui::UiBatch;
use crate::game::Game;
use super::{wrap_ui_status};

impl Game {
    pub(in crate::game) fn draw_title_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32) {

        ui.rect(0.0, 0.0, w, h, [0.05, 0.08, 0.15, 0.55]);
        let tw = UiBatch::text_width(8.0, "WILDFORGE");
        ui.text_shadow(
            (w - tw) / 2.0,
            h * 0.10,
            8.0,
            "WILDFORGE",
            [1.0, 0.95, 0.7, 1.0],
        );
        let (active_name, social_name) = self.selected_multiplayer_name();
        let identity_line = format!(
            "PLAYING AS {active_name}{}",
            if self.atproto_account.is_some() {
                if social_name {
                    "  [ATPROTO PROFILE]"
                } else {
                    "  [ATPROTO LINKED / LOCAL NAME]"
                }
            } else {
                ""
            }
        );
        let iw = UiBatch::text_width(1.5, &identity_line);
        ui.text_shadow(
            (w - iw) / 2.0,
            h * 0.205,
            1.5,
            &identity_line,
            [0.65, 1.0, 0.72, 1.0],
        );
        if self.worlds.is_empty() {
            let msg = "NO WORLDS YET - CREATE ONE";
            let mw = UiBatch::text_width(2.0, msg);
            ui.text_shadow(
                (w - mw) / 2.0,
                self.title_row_y(0) + 12.0,
                2.0,
                msg,
                [0.8, 0.8, 0.8, 1.0],
            );
        }
        for (i, (name, seed)) in self.worlds.iter().take(6).enumerate() {
            let y = self.title_row_y(i);
            let label = format!("{}  SEED {}", name.to_uppercase(), seed);
            ui.text_shadow(w / 2.0 - 310.0, y + 7.0, 1.7, &label, [1.0; 4]);
            if let Some(detail) = self.world_details.get(name) {
                ui.text_shadow(
                    w / 2.0 - 310.0,
                    y + 27.0,
                    1.0,
                    detail,
                    [0.58, 0.86, 0.67, 1.0],
                );
            }
            let pr = self.title_play_rect(i);
            widgets::button(&mut *ui, pr, "PLAY", self.hit(pr));
            let dr = self.title_delete_rect(i);
            widgets::button(&mut *ui, dr, "X", self.hit(dr));
        }
        if let Some((name, problem)) = self.world_problems.first() {
            let warning = format!("{}: {}", name.to_uppercase(), problem);
            ui.text_shadow(
                w / 2.0 - 310.0,
                h * 0.235,
                1.0,
                &warning,
                [1.0, 0.52, 0.42, 1.0],
            );
        }
        for (j, label) in [
            "NEW SURVIVAL WORLD",
            "NEW CREATIVE WORLD",
            "JOIN GAME",
            "ACCOUNTS",
            "APPEARANCE",
            "MODS",
            "TEXTURE PACKS",
            "SETTINGS",
            "QUIT",
        ]
        .iter()
        .enumerate()
        {
            let r = self.title_action_rect(j);
            widgets::button(&mut *ui, r, label, self.hit(r));
        }
    }
    pub(in crate::game) fn draw_new_world_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32) {

        ui.rect(0.0, 0.0, w, h, [0.02, 0.04, 0.08, 0.88]);
        let title = format!("NEW {} PLANET", self.ui_state.new_world_mode.to_uppercase());
        let title_width = UiBatch::text_width(4.0, &title);
        ui.text_shadow(
            (w - title_width) / 2.0,
            h * 0.20,
            4.0,
            &title,
            [1.0, 0.95, 0.72, 1.0],
        );
        ui.text_shadow(
            w / 2.0 - 220.0,
            h * 0.34,
            1.5,
            "PLANET SEED",
            [0.72, 0.82, 0.76, 1.0],
        );
        ui.rect(
            w / 2.0 - 220.0,
            h * 0.38,
            440.0,
            42.0,
            [0.04, 0.06, 0.08, 0.98],
        );
        let seed = format!("{}_", self.ui_state.new_world_seed);
        ui.text_shadow(
            w / 2.0 - 205.0,
            h * 0.39,
            2.0,
            &seed,
            [0.92, 0.96, 0.88, 1.0],
        );
        let hint = "ENTER A SEED OR ROLL ONE. THE SAME SEED + CONTENT + VERSION MAKES THE SAME PLANET.";
        let hint_width = UiBatch::text_width(1.1, hint);
        ui.text_shadow(
            (w - hint_width) / 2.0,
            h * 0.47,
            1.1,
            hint,
            [0.66, 0.72, 0.68, 1.0],
        );
        if !self.ui_state.new_world_status.is_empty() {
            for (line, status) in
                wrap_ui_status(&self.ui_state.new_world_status, w - 80.0, 1.2, 4)
                    .iter()
                    .enumerate()
            {
                let status_width = UiBatch::text_width(1.2, status);
                ui.text_shadow(
                    (w - status_width) / 2.0,
                    h * 0.52 + line as f32 * 13.0,
                    1.2,
                    status,
                    [1.0, 0.52, 0.42, 1.0],
                );
            }
        }
        // Mode selector (capability E1): cycle through all
        // declared modes from base + mods.
        let modes = self.available_new_world_modes();
        let current_idx = modes
            .iter()
            .position(|m| m == &self.ui_state.new_world_mode)
            .unwrap_or(0);
        let mode_y = h * 0.56;
        let mode_label = format!("MODE: {}", modes[current_idx].to_uppercase());
        let mlw = UiBatch::text_width(2.0, &mode_label);
        ui.text_shadow(
            (w - mlw) / 2.0,
            mode_y,
            2.0,
            &mode_label,
            [0.85, 0.95, 0.85, 1.0],
        );
        // Left/right arrows as clickable rects.
        let arrow_w = 36.0;
        let left_r = (w / 2.0 - 200.0, mode_y - 6.0, arrow_w, 30.0);
        let right_r = (w / 2.0 + 164.0, mode_y - 6.0, arrow_w, 30.0);
        let lc = if self.hit(left_r) {
            [1.0; 4]
        } else {
            [0.5, 0.6, 0.5, 1.0]
        };
        let rc = if self.hit(right_r) {
            [1.0; 4]
        } else {
            [0.5, 0.6, 0.5, 1.0]
        };
        ui.text_shadow(left_r.0 + 8.0, left_r.1 + 4.0, 2.5, "<", lc);
        ui.text_shadow(right_r.0 + 8.0, right_r.1 + 4.0, 2.5, ">", rc);

        for (index, label) in ["CREATE PLANET", "ROLL SEED", "BACK"].iter().enumerate() {
            let rect = self.new_world_button_rect(index);
            widgets::button(&mut *ui, rect, label, self.hit(rect));
        }
    }
    pub(in crate::game) fn draw_creating_world_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32) {

        ui.rect(0.0, 0.0, w, h, [0.02, 0.04, 0.08, 0.88]);
        let title = "A WORLD IS BECOMING";
        let title_width = UiBatch::text_width(4.0, title);
        ui.text_shadow(
            (w - title_width) / 2.0,
            h * 0.28,
            4.0,
            title,
            [1.0, 0.95, 0.72, 1.0],
        );
        let status_width = UiBatch::text_width(2.5, &self.ui_state.creation_status);
        ui.text_shadow(
            (w - status_width) / 2.0,
            h * 0.43,
            2.5,
            &self.ui_state.creation_status,
            [0.75, 0.95, 0.82, 1.0],
        );
        let (completed, total) = self.ui_state.creation_progress;
        let fraction = completed as f32 / total.max(1) as f32;
        ui.rect(
            w / 2.0 - 220.0,
            h * 0.52,
            440.0,
            16.0,
            [0.08, 0.1, 0.12, 0.95],
        );
        ui.rect(
            w / 2.0 - 218.0,
            h * 0.52 + 2.0,
            436.0 * fraction,
            12.0,
            [0.25, 0.72, 0.45, 1.0],
        );
        let cancel = self.world_creation_cancel_rect();
        widgets::button(&mut *ui, cancel, "CANCEL", self.hit(cancel));
    }
    pub(in crate::game) fn draw_confirm_delete_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32) {

        ui.rect(0.0, 0.0, w, h, [0.1, 0.02, 0.02, 0.7]);
        let name = self
            .ui_state
            .pending_delete
            .and_then(|i| self.worlds.get(i))
            .map(|(n, _)| n.to_uppercase())
            .unwrap_or_default();
        let msg = format!("DELETE {name}?");
        let tw = UiBatch::text_width(4.0, &msg);
        ui.text_shadow((w - tw) / 2.0, h * 0.25, 4.0, &msg, [1.0, 0.8, 0.8, 1.0]);
        let sub = "THIS CANNOT BE UNDONE";
        let sw = UiBatch::text_width(2.0, sub);
        ui.text_shadow(
            (w - sw) / 2.0,
            h * 0.25 + 50.0,
            2.0,
            sub,
            [0.9, 0.7, 0.7, 1.0],
        );
        for (j, label) in ["DELETE", "CANCEL"].iter().enumerate() {
            let r = self.menu_button_rect(j);
            widgets::button(&mut *ui, r, label, self.hit(r));
        }
    }
}
