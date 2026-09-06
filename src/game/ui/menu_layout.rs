//! Menu layout layout and UI composition.

use crate::game::Game;

impl Game {
    pub(in crate::game) fn menu_button_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 - 150.0,
            h / 2.0 - 40.0 + i as f32 * 56.0,
            300.0,
            40.0,
        )
    }

    pub(in crate::game) fn new_world_button_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 - 150.0, h * 0.62 + i as f32 * 52.0, 300.0, 40.0)
    }

    pub(in crate::game) fn world_creation_cancel_rect(&self) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 - 150.0, (h - 72.0).max(8.0), 300.0, 40.0)
    }

    /// Kick button beside the pause menu, one row per connected guest.
    pub(in crate::game) fn kick_rect(&self, row: usize) -> (f32, f32, f32, f32) {
        let (bx, by, bw, _) = self.menu_button_rect(2);
        (bx + bw + 16.0, by + row as f32 * 36.0, 70.0, 28.0)
    }

    /// Guest (id, name) rows in a stable order for the pause menu.
    pub(in crate::game) fn guest_rows(&self) -> Vec<(u32, String)> {
        let mut rows: Vec<(u32, String)> = if let Some(h) = &self.multiplayer.host {
            h.guests
                .iter()
                .filter(|(_, guest)| guest.is_active())
                .map(|(id, g)| (*id, g.public_label()))
                .collect()
        } else if let Some(r) = &self.multiplayer.remote
            && r.role.can_moderate()
        {
            r.session.roster()
                .iter()
                .filter(|(id, _)| **id != 0 && **id != r.my_id)
                .map(|(id, presence)| (*id, crate::game::remote::presence_label(presence)))
                .collect()
        } else {
            Vec::new()
        };
        rows.sort_by_key(|(id, _)| *id);
        rows
    }

    // ---- title screen layout ----

    pub(in crate::game) fn title_row_y(&self, i: usize) -> f32 {
        self.renderer.config.height as f32 * 0.28 + i as f32 * 54.0
    }

    pub(in crate::game) fn title_play_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        (w / 2.0 + 60.0, self.title_row_y(i), 100.0, 42.0)
    }

    pub(in crate::game) fn title_delete_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        (w / 2.0 + 172.0, self.title_row_y(i), 46.0, 42.0)
    }

    /// 0 = new world, 1 = settings, 2 = quit.
    /// Two columns of four: left = play, right = meta.
    pub(in crate::game) fn title_action_rect(&self, j: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let base = self.title_row_y(self.worlds.len().min(6)) + 26.0;
        let x = if j < 5 {
            w / 2.0 - 310.0
        } else {
            w / 2.0 + 10.0
        };
        let row = if j < 5 { j } else { j - 5 };
        (x, base + row as f32 * 56.0, 300.0, 42.0)
    }

    pub(in crate::game) fn account_field_rect(&self, row: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 - 40.0, h * 0.20 + row as f32 * 64.0, 340.0, 38.0)
    }

    pub(in crate::game) fn account_button_rect(&self, button: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let column = button / 4;
        let row = button % 4;
        (
            w / 2.0 - 310.0 + column as f32 * 320.0,
            h * 0.42 + row as f32 * 56.0,
            300.0,
            42.0,
        )
    }

    pub(in crate::game) fn appearance_row_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 + 40.0, h * 0.18 + i as f32 * 58.0, 260.0, 42.0)
    }

    pub(in crate::game) fn appearance_back_rect(&self) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 + 40.0, h * 0.18 + 8.0 * 58.0 + 12.0, 260.0, 42.0)
    }

    // ---- texture packs screen layout ----

    /// Row 0 is "NONE", rows 1.. are discovered packs.
    pub(in crate::game) fn pack_row_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 - 220.0, h * 0.20 + i as f32 * 68.0, 440.0, 42.0)
    }

    pub(in crate::game) fn pack_back_rect(&self) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 - 150.0, h - 80.0, 300.0, 42.0)
    }

    // ---- settings screen layout ----

    pub(super) const SLIDERS: [&'static str; 4] = ["VOLUME", "SENSITIVITY", "RENDER DIST", "FOV"];
}
