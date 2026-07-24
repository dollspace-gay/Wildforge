//! UI layout, drawing, and screen composition.

use super::*;
use glam::Mat4;

fn project_world_label(
    view_proj: Mat4,
    point: Vec3,
    width: f32,
    height: f32,
) -> Option<(f32, f32)> {
    let clip = view_proj * point.extend(1.0);
    if clip.w <= 0.05 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    if ndc.x.abs() > 1.0 || ndc.y.abs() > 1.0 || !ndc.is_finite() {
        return None;
    }
    Some(((ndc.x * 0.5 + 0.5) * width, (0.5 - ndc.y * 0.5) * height))
}

impl Game {
    pub(super) fn hotbar_origin(&self) -> (f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        ((w - 9.0 * Self::SLOT) / 2.0, h - Self::SLOT - 8.0)
    }

    pub(super) fn hotbar_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let (x0, y0) = self.hotbar_origin();
        (x0 + i as f32 * Self::SLOT, y0, Self::SLOT, Self::SLOT)
    }

    /// Slot rects for the inventory screen: 0..9 hotbar row, 9..36 storage grid.
    pub(super) fn inv_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let panel_w = 9.0 * Self::SLOT;
        let x0 = (w - panel_w) / 2.0;
        // The complete inventory card (equipment above, storage below) is
        // vertically centered. The old offset centered only the storage rows,
        // leaving the paper doll stranded near the corner of the screen.
        let grid_y = h / 2.0 + 16.0;
        if i < HOTBAR_SLOTS {
            (
                x0 + i as f32 * Self::SLOT,
                grid_y + 3.0 * Self::SLOT + 14.0,
                Self::SLOT,
                Self::SLOT,
            )
        } else {
            let j = i - HOTBAR_SLOTS;
            (
                x0 + (j % 9) as f32 * Self::SLOT,
                grid_y + (j / 9) as f32 * Self::SLOT,
                Self::SLOT,
                Self::SLOT,
            )
        }
    }

    /// Unified inventory card containing identity, gear, crafting and storage.
    pub(super) fn inventory_panel_rect(&self) -> (f32, f32, f32, f32) {
        let (grid_x, grid_y, _, _) = self.inv_slot_rect(HOTBAR_SLOTS);
        (
            grid_x - 134.0,
            grid_y - 248.0,
            9.0 * Self::SLOT + 268.0,
            464.0,
        )
    }

    pub(super) fn inventory_avatar_rect(&self) -> (f32, f32, f32, f32) {
        let (panel_x, panel_y, _, _) = self.inventory_panel_rect();
        (panel_x + 70.0, panel_y + 48.0, 170.0, 184.0)
    }

    pub(super) fn inventory_avatar_center(&self) -> (f32, f32) {
        let (x, y, width, _) = self.inventory_avatar_rect();
        (x + width * 0.5, y + 82.0)
    }

    /// Header controls use one source of geometry for drawing and hit-testing.
    pub(super) fn inventory_tab_rect(&self, tab: usize) -> (f32, f32, f32, f32) {
        let (x, y, width, _) = self.inventory_panel_rect();
        let (button_width, right_pad) = match tab {
            0 => (74.0, 228.0),
            1 => (98.0, 124.0),
            _ => (112.0, 8.0),
        };
        (
            x + width - right_pad - button_width,
            y + 9.0,
            button_width,
            28.0,
        )
    }

    pub(super) fn menu_button_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 - 150.0,
            h / 2.0 - 40.0 + i as f32 * 56.0,
            300.0,
            40.0,
        )
    }

    /// Kick button beside the pause menu, one row per connected guest.
    pub(super) fn kick_rect(&self, row: usize) -> (f32, f32, f32, f32) {
        let (bx, by, bw, _) = self.menu_button_rect(2);
        (bx + bw + 16.0, by + row as f32 * 36.0, 70.0, 28.0)
    }

    /// Guest (id, name) rows in a stable order for the pause menu.
    pub(super) fn guest_rows(&self) -> Vec<(u32, String)> {
        let mut rows: Vec<(u32, String)> = if let Some(h) = &self.multiplayer.host {
            h.guests
                .iter()
                .map(|(id, g)| (*id, g.public_label()))
                .collect()
        } else if let Some(r) = &self.multiplayer.remote
            && r.role.can_moderate()
        {
            r.names
                .iter()
                .filter(|(id, _)| **id != 0 && **id != r.my_id)
                .map(|(id, name)| (*id, name.clone()))
                .collect()
        } else {
            Vec::new()
        };
        rows.sort_by_key(|(id, _)| *id);
        rows
    }

    fn draw_world_nameplate(
        &self,
        ui: &mut UiBatch,
        name: &str,
        feet: Vec3,
        width: f32,
        height: f32,
    ) {
        let head = feet + Vec3::new(0.0, 2.12, 0.0);
        let sight = head - self.camera.pos;
        let distance = sight.length();
        if !(1.0..=64.0).contains(&distance)
            || raycast::raycast(
                &self.server.world,
                self.camera.pos,
                sight,
                (distance - 0.3).max(0.0),
            )
            .is_some()
        {
            return;
        }
        let Some((sx, sy)) = project_world_label(self.camera.view_proj(), head, width, height)
        else {
            return;
        };
        // Keep world labels compact even when the roster includes an opted-in
        // handle. The roster is the detail surface; the world needs a readable
        // display name and a small verification signal.
        let label = if let Some((identity, verification)) = name.split_once(" [") {
            let display_name = identity
                .split_once(" @")
                .map_or(identity, |(display_name, _)| display_name);
            let badge = if verification.starts_with("VERIFIED/CACHED") {
                "V*"
            } else {
                "V"
            };
            format!("{display_name} [{badge}]")
        } else {
            name.to_owned()
        }
        .to_uppercase();
        let scale = if distance < 24.0 { 1.5 } else { 1.25 };
        let alpha = ((64.0 - distance) / 16.0).clamp(0.35, 1.0);
        let text_width = UiBatch::text_width(scale, &label);
        let text_y = sy - 9.0 * scale;
        ui.rect(
            sx - text_width * 0.5 - 4.0,
            text_y - 3.0,
            text_width + 8.0,
            7.0 * scale + 6.0,
            [0.01, 0.01, 0.015, 0.52 * alpha],
        );
        ui.text_shadow(
            sx - text_width * 0.5,
            text_y,
            scale,
            &label,
            [1.0, 1.0, 1.0, alpha],
        );
    }

    // ---- title screen layout ----

    pub(super) fn title_row_y(&self, i: usize) -> f32 {
        self.renderer.config.height as f32 * 0.28 + i as f32 * 54.0
    }

    pub(super) fn title_play_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        (w / 2.0 + 60.0, self.title_row_y(i), 100.0, 42.0)
    }

    pub(super) fn title_delete_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        (w / 2.0 + 172.0, self.title_row_y(i), 46.0, 42.0)
    }

    /// 0 = new world, 1 = settings, 2 = quit.
    /// Two columns of four: left = play, right = meta.
    pub(super) fn title_action_rect(&self, j: usize) -> (f32, f32, f32, f32) {
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

    pub(super) fn account_field_rect(&self, row: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 - 40.0, h * 0.20 + row as f32 * 64.0, 340.0, 38.0)
    }

    pub(super) fn account_button_rect(&self, button: usize) -> (f32, f32, f32, f32) {
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

    pub(super) fn appearance_row_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 + 40.0, h * 0.18 + i as f32 * 58.0, 260.0, 42.0)
    }

    pub(super) fn appearance_back_rect(&self) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 + 40.0, h * 0.18 + 8.0 * 58.0 + 12.0, 260.0, 42.0)
    }

    // ---- texture packs screen layout ----

    /// Row 0 is "NONE", rows 1.. are discovered packs.
    pub(super) fn pack_row_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 - 220.0, h * 0.20 + i as f32 * 68.0, 440.0, 42.0)
    }

    pub(super) fn pack_back_rect(&self) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 - 150.0, h - 80.0, 300.0, 42.0)
    }

    // ---- settings screen layout ----

    pub(super) const SLIDERS: [&'static str; 4] = ["VOLUME", "SENSITIVITY", "RENDER DIST", "FOV"];

    pub(super) fn slider_bar_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 - 20.0, h * 0.30 + i as f32 * 64.0, 300.0, 30.0)
    }

    /// The toggle rows under the sliders (lights, darkness, outline).
    /// Top of the toggle stack: directly under the four sliders.
    fn settings_toggles_top(h: f32) -> f32 {
        h * 0.30 + 4.0 * 64.0
    }

    /// Vertical step between settings toggles. The designed 56 wherever there's
    /// room; on a short window it compresses (down to a 2px gap) so five rows
    /// plus BACK still land on screen instead of running off the bottom.
    fn settings_step(h: f32) -> f32 {
        const ROWS: f32 = 5.0;
        let avail = h - Self::settings_toggles_top(h) - 50.0;
        (avail / (ROWS + 1.0)).clamp(44.0, 56.0)
    }

    pub(super) fn settings_toggle_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 - 20.0,
            Self::settings_toggles_top(h) - 8.0 + i as f32 * Self::settings_step(h),
            300.0,
            42.0,
        )
    }

    pub(super) fn settings_back_rect(&self) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 - 150.0,
            Self::settings_toggles_top(h) + 5.0 * Self::settings_step(h) + 8.0,
            300.0,
            42.0,
        )
    }

    pub(super) fn slider_frac(&self, i: usize) -> f32 {
        match i {
            0 => self.config.volume,
            1 => (self.config.sensitivity - 0.1) / 2.9,
            2 => (self.config.view_dist - 4) as f32 / 8.0,
            _ => (self.config.fov - 50.0) / 60.0,
        }
    }

    pub(super) fn slider_label(&self, i: usize) -> String {
        match i {
            0 => format!("{:.0}", self.config.volume * 100.0),
            1 => format!("{:.2}", self.config.sensitivity),
            2 => format!("{}", self.config.view_dist),
            _ => format!("{:.0}", self.config.fov),
        }
    }

    pub(super) fn set_slider(&mut self, i: usize, frac: f32) {
        let f = frac.clamp(0.0, 1.0);
        match i {
            0 => self.config.volume = (f * 20.0).round() / 20.0,
            1 => self.config.sensitivity = ((0.1 + f * 2.9) * 20.0).round() / 20.0,
            2 => self.config.view_dist = 4 + (f * 8.0).round() as i32,
            _ => self.config.fov = 50.0 + (f * 60.0).round(),
        }
        self.apply_config();
    }

    pub(super) fn draw_button(ui: &mut UiBatch, r: (f32, f32, f32, f32), label: &str, hover: bool) {
        // Hover grows the plate 4% and brightens; a press dips it 2% —
        // the down-up is what makes a click feel mechanical.
        let mut r = r;
        if hover {
            let sc = if ui.press_dip { 0.98 } else { 1.04 };
            let (cx, cy) = (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0);
            r = (cx - r.2 / 2.0 * sc, cy - r.3 / 2.0 * sc, r.2 * sc, r.3 * sc);
        }
        let bg = if hover {
            [0.55, 0.55, 0.55, 0.95]
        } else {
            [0.25, 0.25, 0.25, 0.95]
        };
        ui.rect(r.0, r.1, r.2, r.3, [0.1, 0.1, 0.1, 0.95]);
        ui.rect(r.0 + 2.0, r.1 + 2.0, r.2 - 4.0, r.3 - 4.0, bg);
        let lw = UiBatch::text_width(2.0, label);
        ui.text_shadow(
            r.0 + (r.2 - lw) / 2.0,
            r.1 + (r.3 - 14.0) / 2.0,
            2.0,
            label,
            [1.0; 4],
        );
    }

    pub(super) fn hit(&self, r: (f32, f32, f32, f32)) -> bool {
        let (x, y) = self.input.ui_cursor;
        x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3
    }

    pub(super) fn draw_slot(
        reg: &Registry,
        ui: &mut UiBatch,
        r: (f32, f32, f32, f32),
        stack: Option<ItemStack>,
        selected: bool,
        hover: bool,
    ) {
        let (x, y, w, h) = r;
        let border = if selected {
            [1.0, 1.0, 1.0, 0.9]
        } else {
            [0.35, 0.35, 0.35, 0.9]
        };
        ui.rect(x + 1.0, y + 1.0, w - 2.0, h - 2.0, border);
        let bg = if hover {
            [0.45, 0.45, 0.45, 0.92]
        } else {
            [0.18, 0.18, 0.18, 0.92]
        };
        ui.rect(x + 3.0, y + 3.0, w - 6.0, h - 6.0, bg);
        if let Some(s) = stack {
            let pad = 8.0;
            let icon = reg.item(s.item).icon;
            let tile = icon;
            ui.tile(
                x + pad,
                y + pad,
                w - 2.0 * pad,
                h - 2.0 * pad,
                tile,
                [1.0; 4],
            );
            if s.count > 1 {
                let txt = format!("{}", s.count);
                let tw = UiBatch::text_width(2.0, &txt);
                ui.text_shadow(x + w - tw - 4.0, y + h - 18.0, 2.0, &txt, [1.0; 4]);
            }
            // Durability bar for worn tools.
            let max = reg.item(s.item).durability;
            if max > 0 && s.durability < max {
                let frac = s.durability as f32 / max as f32;
                ui.rect(x + 6.0, y + h - 9.0, w - 12.0, 4.0, [0.05, 0.05, 0.05, 0.9]);
                ui.rect(
                    x + 6.0,
                    y + h - 9.0,
                    (w - 12.0) * frac,
                    4.0,
                    [1.0 - frac, frac, 0.1, 1.0],
                );
            }
        }
    }

    /// Craft grid layout: grid slots then the result slot to their right.
    pub(super) fn craft_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let n = self.interaction.craft_size;
        let (sx, sy, _, _) = self.inv_slot_rect(HOTBAR_SLOTS); // storage top-left
        let y0 = sy - (n as f32) * Self::SLOT - 26.0;
        let x0 = sx + 4.25 * Self::SLOT;
        (
            x0 + (i % n) as f32 * Self::SLOT,
            y0 + (i / n) as f32 * Self::SLOT,
            Self::SLOT,
            Self::SLOT,
        )
    }

    pub(super) fn result_slot_rect(&self) -> (f32, f32, f32, f32) {
        let n = self.interaction.craft_size;
        let (gx, gy, _, _) = self.craft_slot_rect(0);
        (
            gx + n as f32 * Self::SLOT + Self::SLOT,
            gy + ((n as f32) - 1.0) * Self::SLOT / 2.0,
            Self::SLOT,
            Self::SLOT,
        )
    }

    pub(super) fn build_ui(&mut self) {
        self.poll_account_task();
        let mut ui = std::mem::replace(&mut self.ui, UiBatch::new());
        ui.clear();
        ui.press_dip = self.presentation.juice && self.presentation.press_dip > 0.0;
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;

        // Menu-only screens draw over the sky and skip the HUD entirely.
        match self.ui_state.screen {
            Screen::Title => {
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
                    ui.text_shadow(w / 2.0 - 310.0, y + 13.0, 2.0, &label, [1.0; 4]);
                    let pr = self.title_play_rect(i);
                    Self::draw_button(&mut ui, pr, "PLAY", self.hit(pr));
                    let dr = self.title_delete_rect(i);
                    Self::draw_button(&mut ui, dr, "X", self.hit(dr));
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
                    Self::draw_button(&mut ui, r, label, self.hit(r));
                }
                self.ui = ui;
                return;
            }
            Screen::Accounts => {
                ui.rect(0.0, 0.0, w, h, [0.02, 0.05, 0.1, 0.82]);
                let title = if self.config.profile_complete {
                    "ACCOUNTS"
                } else {
                    "CREATE LOCAL PROFILE"
                };
                let tw = UiBatch::text_width(4.0, title);
                ui.text_shadow((w - tw) / 2.0, h * 0.07, 4.0, title, [1.0; 4]);
                ui.text_shadow(
                    w / 2.0 - 310.0,
                    h * 0.14,
                    1.5,
                    "LOCAL PLAY NEVER REQUIRES AN ONLINE ACCOUNT. NAMES ARE LABELS, NOT SAVE KEYS.",
                    [0.75, 0.85, 0.75, 1.0],
                );
                for (row, label, value) in [
                    (
                        0usize,
                        "WILDFORGE NAME",
                        self.ui_state.account_name.as_str(),
                    ),
                    (
                        1usize,
                        "ATPROTO HANDLE OR DID",
                        self.ui_state.account_handle.as_str(),
                    ),
                ] {
                    let r = self.account_field_rect(row);
                    ui.text_shadow(w / 2.0 - 310.0, r.1 + 8.0, 1.5, label, [1.0; 4]);
                    ui.rect(r.0, r.1, r.2, r.3, [0.08, 0.08, 0.08, 0.98]);
                    ui.text_shadow(r.0 + 8.0, r.1 + 8.0, 2.0, &value.to_uppercase(), [1.0; 4]);
                    if self.ui_state.account_focus as usize == row {
                        ui.rect(r.0, r.1 + r.3 - 3.0, r.2, 3.0, [0.6, 1.0, 0.6, 1.0]);
                    }
                }
                let device = format!(
                    "LOCAL PROFILE: {}  /  DEVICE {}",
                    self.config.display_name,
                    self.identity.device_id().short()
                );
                ui.text_shadow(w / 2.0 - 310.0, h * 0.35, 1.5, &device, [0.7; 4]);
                let linked = self.atproto_account.as_ref();
                let labels = [
                    "SAVE LOCAL NAME".to_string(),
                    if linked.is_some() {
                        "REFRESH / RELINK ATPROTO"
                    } else {
                        "LINK ATPROTO"
                    }
                    .to_string(),
                    format!(
                        "SOCIAL DISPLAY NAME: {}",
                        if linked.is_some_and(|a| a.use_social_display_name) {
                            "ON"
                        } else {
                            "OFF"
                        }
                    ),
                    format!(
                        "SOCIAL AVATAR: {}",
                        if linked.is_some_and(|a| a.use_social_avatar) {
                            "ON"
                        } else {
                            "OFF"
                        }
                    ),
                    format!(
                        "SHARE ATPROTO HANDLE: {}",
                        if linked.is_some_and(|a| a.share_social_handle) {
                            "ON"
                        } else {
                            "OFF"
                        }
                    ),
                    "REVOKE THIS DEVICE".to_string(),
                    "UNLINK LOCALLY".to_string(),
                    if self.config.profile_complete {
                        "BACK"
                    } else {
                        "SAVE A NAME TO CONTINUE"
                    }
                    .to_string(),
                ];
                for (i, label) in labels.iter().enumerate() {
                    let r = self.account_button_rect(i);
                    Self::draw_button(&mut ui, r, label, self.hit(r));
                }
                if let Some(account) = linked {
                    let (active_name, social_name) = self.selected_multiplayer_name();
                    let join_line = format!(
                        "MULTIPLAYER NAME: {active_name}  ({})",
                        if social_name {
                            "ATPROTO PROFILE"
                        } else {
                            "LOCAL PROFILE"
                        }
                    );
                    ui.text_shadow(
                        w / 2.0 - 310.0,
                        h * 0.77,
                        1.5,
                        &join_line,
                        [1.0, 0.95, 0.72, 1.0],
                    );
                    let profile_line = format!(
                        "ATPROTO PROFILE: {}{}",
                        account
                            .profile_display_name
                            .as_deref()
                            .unwrap_or("NO DISPLAY NAME PUBLISHED"),
                        account
                            .handle
                            .as_deref()
                            .map(|handle| format!("  (@{handle})"))
                            .unwrap_or_default()
                    );
                    ui.text_shadow(
                        w / 2.0 - 310.0,
                        h * 0.81,
                        1.25,
                        &profile_line.to_uppercase(),
                        [0.6, 1.0, 0.7, 1.0],
                    );
                    let disclosure = if account.share_social_handle {
                        "OTHER PLAYERS SEE YOUR VERIFIED BADGE AND PUBLIC HANDLE"
                    } else {
                        "OTHER PLAYERS SEE ONLY A VERIFIED BADGE"
                    };
                    let did_line = format!("LINKED ID: {}  ({disclosure})", account.did.short());
                    ui.text_shadow(
                        w / 2.0 - 310.0,
                        h * 0.85,
                        1.25,
                        &did_line.to_uppercase(),
                        [0.6, 1.0, 0.7, 1.0],
                    );
                }
                if !self.ui_state.account_status.is_empty() {
                    ui.text_shadow(
                        w / 2.0 - 310.0,
                        h * 0.91,
                        1.25,
                        &self.ui_state.account_status,
                        [1.0, 0.75, 0.5, 1.0],
                    );
                }
                ui.text_shadow(
                    w / 2.0 - 310.0,
                    h * 0.95,
                    1.0,
                    "LINKING WRITES A PUBLIC DEVICE RECORD. A VERIFIED SERVER CAN RESOLVE YOUR DID AND PUBLIC PROFILE.",
                    [0.65, 0.65, 0.65, 1.0],
                );
                self.ui = ui;
                return;
            }
            Screen::Moderation(id) => {
                ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.78]);
                let title = "PLAYER MODERATION";
                let tw = UiBatch::text_width(4.0, title);
                ui.text_shadow((w - tw) / 2.0, h * 0.08, 4.0, title, [1.0; 4]);
                let summary = self
                    .multiplayer
                    .host
                    .as_ref()
                    .and_then(|host| host.guest_identity_summary(id))
                    .or_else(|| {
                        let remote = self.multiplayer.remote.as_ref()?;
                        remote
                            .names
                            .get(&id)
                            .map(|name| format!("{name} | YOUR ROLE {:?}", remote.role))
                    })
                    .unwrap_or_else(|| "PLAYER DISCONNECTED".into());
                ui.text_shadow(
                    w / 2.0 - 360.0,
                    h * 0.18,
                    1.5,
                    &summary.to_uppercase(),
                    [0.8; 4],
                );
                let labels = [
                    "KICK",
                    "MUTE 10 MINUTES",
                    "BAN 1 HOUR",
                    "BAN PERMANENTLY",
                    "ADD TO ALLOWLIST",
                    "CYCLE ROLE",
                    "BACK",
                ];
                for (i, label) in labels.iter().enumerate() {
                    let r = self.menu_button_rect(i);
                    let label = if self.ui_state.moderation_confirm == Some(i as u8) {
                        format!("CONFIRM {label}")
                    } else {
                        (*label).to_string()
                    };
                    Self::draw_button(&mut ui, r, &label, self.hit(r));
                }
                self.ui = ui;
                return;
            }
            Screen::Mods => {
                ui.rect(0.0, 0.0, w, h, [0.02, 0.05, 0.1, 0.75]);
                let tw = UiBatch::text_width(4.0, "MODS");
                ui.text_shadow((w - tw) / 2.0, h * 0.10, 4.0, "MODS", [1.0; 4]);
                let mut y = h * 0.24;
                for m in self.content.reg.mods.iter().take(10) {
                    let script = if m.has_script { " +SCRIPT" } else { "" };
                    let line = format!("{} {}{}", m.name.to_uppercase(), m.version, script);
                    ui.text_shadow(w / 2.0 - 300.0, y, 2.0, &line, [1.0; 4]);
                    match &m.error {
                        Some(e) => {
                            let msg: String = e.chars().take(60).collect();
                            ui.text_shadow(
                                w / 2.0 - 300.0,
                                y + 20.0,
                                1.5,
                                &msg.to_uppercase(),
                                [1.0, 0.5, 0.5, 1.0],
                            );
                            y += 44.0;
                        }
                        None => {
                            ui.text_shadow(w / 2.0 + 200.0, y, 2.0, "OK", [0.5, 1.0, 0.5, 1.0]);
                            y += 30.0;
                        }
                    }
                }
                let hint = "EDIT MODS/ WHILE PLAYING - CHANGES HOT RELOAD. F5 FORCES.";
                ui.text_shadow(w / 2.0 - 300.0, y + 16.0, 1.5, hint, [0.7, 0.7, 0.7, 1.0]);
                let br = self.menu_button_rect(4);
                Self::draw_button(&mut ui, br, "BACK", self.hit(br));
                self.ui = ui;
                return;
            }
            Screen::Packs => {
                ui.rect(0.0, 0.0, w, h, [0.02, 0.05, 0.1, 0.75]);
                let tw = UiBatch::text_width(4.0, "TEXTURE PACKS");
                ui.text_shadow((w - tw) / 2.0, h * 0.08, 4.0, "TEXTURE PACKS", [1.0; 4]);
                for i in 0..=self.content.packs.len().min(7) {
                    let r = self.pack_row_rect(i);
                    let label = if i == 0 {
                        "NONE - PROCEDURAL".to_string()
                    } else {
                        self.content.packs[i - 1].name.to_uppercase()
                    };
                    Self::draw_button(&mut ui, r, &label, self.hit(r));
                    let cur = self.active_pack_id();
                    let active = if i == 0 {
                        cur.is_empty() || pack_source_of(&cur).is_none()
                    } else {
                        self.content.packs[i - 1].id == cur
                    };
                    if active {
                        ui.text_shadow(
                            r.0 + r.2 + 18.0,
                            r.1 + 12.0,
                            2.0,
                            "ACTIVE",
                            [0.5, 1.0, 0.5, 1.0],
                        );
                    }
                    if i > 0 && !self.content.packs[i - 1].description.is_empty() {
                        let d: String = self.content.packs[i - 1]
                            .description
                            .chars()
                            .take(64)
                            .collect();
                        ui.text_shadow(
                            r.0 + 8.0,
                            r.1 + r.3 + 4.0,
                            1.5,
                            &d.to_uppercase(),
                            [0.7, 0.7, 0.7, 1.0],
                        );
                    }
                }
                let mut y = self.pack_row_rect(self.content.packs.len().min(7)).1 + 60.0;
                for warn in self.content.pack_warnings.iter().take(3) {
                    let msg: String = warn.chars().take(70).collect();
                    ui.text_shadow(
                        w / 2.0 - 300.0,
                        y,
                        1.5,
                        &msg.to_uppercase(),
                        [1.0, 0.5, 0.5, 1.0],
                    );
                    y += 20.0;
                }
                let hint = "DROP PACKS IN PACKS/ - PNG EDITS HOT RELOAD LIVE.";
                ui.text_shadow(w / 2.0 - 300.0, y + 4.0, 1.5, hint, [0.7, 0.7, 0.7, 1.0]);
                let br = self.pack_back_rect();
                Self::draw_button(&mut ui, br, "BACK", self.hit(br));
                self.ui = ui;
                return;
            }
            Screen::Join => {
                ui.rect(0.0, 0.0, w, h, [0.02, 0.05, 0.1, 0.75]);
                let tw = UiBatch::text_width(4.0, "JOIN GAME");
                ui.text_shadow((w - tw) / 2.0, h * 0.08, 4.0, "JOIN GAME", [1.0; 4]);
                if let Some(d) = &mut self.multiplayer.discovery {
                    d.poll();
                }
                let found: Vec<net::DiscoveredServer> = self
                    .multiplayer
                    .discovery
                    .as_ref()
                    .map(|d| d.found.clone())
                    .unwrap_or_default();
                if found.is_empty() {
                    ui.text_shadow(
                        w / 2.0 - 220.0,
                        h * 0.20 + 10.0,
                        2.0,
                        "SEARCHING THE LAN...",
                        [0.7, 0.7, 0.7, 1.0],
                    );
                }
                for (i, found) in found.iter().take(5).enumerate() {
                    let r = (w / 2.0 - 220.0, h * 0.20 + i as f32 * 56.0, 440.0, 42.0);
                    Self::draw_button(
                        &mut ui,
                        r,
                        &format!("{} - {}", found.name.to_uppercase(), found.addr),
                        self.hit(r),
                    );
                    let policy = match found.identity {
                        identity::IdentityPolicy::AtprotoRequired => "VERIFIED ATPROTO REQUIRED",
                        identity::IdentityPolicy::AtprotoOptional => "ATPROTO OPTIONAL",
                        identity::IdentityPolicy::Local => "LOCAL IDENTITIES ACCEPTED",
                    };
                    ui.text_shadow(r.0 + 8.0, r.1 + 29.0, 1.0, policy, [0.65, 0.85, 0.65, 1.0]);
                }
                // The searching line occupies one row when the list is
                // empty; the click handler mirrors this formula.
                let y = h * 0.20 + found.len().clamp(1, 5) as f32 * 56.0 + 26.0;
                ui.text_shadow(w / 2.0 - 220.0, y, 2.0, "DIRECT IP:", [1.0; 4]);
                ui.rect(w / 2.0 - 80.0, y - 6.0, 300.0, 34.0, [0.1, 0.1, 0.1, 0.95]);
                let shown = if self.multiplayer.join_ip.is_empty() {
                    "TYPE ADDRESS"
                } else {
                    &self.multiplayer.join_ip
                };
                let col = if self.multiplayer.join_ip.is_empty() {
                    [0.5, 0.5, 0.5, 1.0]
                } else {
                    [1.0; 4]
                };
                ui.text_shadow(w / 2.0 - 72.0, y, 2.0, &shown.to_uppercase(), col);
                let cr = (w / 2.0 + 240.0, y - 6.0, 160.0, 34.0);
                Self::draw_button(&mut ui, cr, "CONNECT", self.hit(cr));
                if !self.multiplayer.join_status.is_empty() {
                    ui.text_shadow(
                        w / 2.0 - 220.0,
                        y + 46.0,
                        2.0,
                        &self.multiplayer.join_status,
                        [1.0, 0.6, 0.5, 1.0],
                    );
                }
                let br = self.pack_back_rect();
                Self::draw_button(&mut ui, br, "BACK", self.hit(br));
                self.ui = ui;
                return;
            }
            Screen::Settings => {
                ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.6]);
                let tw = UiBatch::text_width(4.0, "SETTINGS");
                ui.text_shadow((w - tw) / 2.0, h * 0.12, 4.0, "SETTINGS", [1.0; 4]);
                for i in 0..Self::SLIDERS.len() {
                    let (bx, by, bw, bh) = self.slider_bar_rect(i);
                    ui.text_shadow(w / 2.0 - 300.0, by + 8.0, 2.0, Self::SLIDERS[i], [1.0; 4]);
                    ui.rect(bx, by, bw, bh, [0.1, 0.1, 0.1, 0.95]);
                    let frac = self.slider_frac(i);
                    ui.rect(
                        bx + 2.0,
                        by + 2.0,
                        (bw - 4.0) * frac,
                        bh - 4.0,
                        [0.35, 0.65, 0.35, 0.95],
                    );
                    // Handle notch.
                    let hx = bx + 2.0 + (bw - 8.0) * frac;
                    ui.rect(hx, by, 4.0, bh, [0.9, 0.9, 0.9, 1.0]);
                    ui.text_shadow(
                        bx + bw + 14.0,
                        by + 8.0,
                        2.0,
                        &self.slider_label(i),
                        [1.0; 4],
                    );
                }
                for (i, (name, value)) in [
                    (
                        "DYNAMIC LIGHTS",
                        ["OFF", "ON", "+SHADOWS"][self.config.lights.min(2) as usize],
                    ),
                    (
                        "TORCH SHADOWS",
                        if self.config.point_grid { "GRID" } else { "CUBE" },
                    ),
                    ("DARKNESS", if self.config.stark { "STARK" } else { "SOFT" }),
                    (
                        "BLOCK OUTLINE",
                        if self.config.outline { "ON" } else { "OFF" },
                    ),
                    ("BLOOM", if self.config.bloom { "ON" } else { "OFF" }),
                ]
                .iter()
                .enumerate()
                {
                    let r = self.settings_toggle_rect(i);
                    ui.text_shadow(w / 2.0 - 300.0, r.1 + 12.0, 2.0, name, [1.0; 4]);
                    Self::draw_button(&mut ui, r, value, self.hit(r));
                }
                let br = self.settings_back_rect();
                Self::draw_button(&mut ui, br, "BACK", self.hit(br));
                self.ui = ui;
                return;
            }
            Screen::Appearance => {
                ui.rect(0.0, 0.0, w, h, [0.02, 0.04, 0.08, 0.72]);
                let tw = UiBatch::text_width(4.0, "APPEARANCE");
                ui.text_shadow((w - tw) / 2.0, h * 0.10, 4.0, "APPEARANCE", [1.0; 4]);
                let st = self.style;
                // (label, swatch color if a color row, value name)
                let rows: [(&str, Option<[f32; 3]>, String); 8] = [
                    (
                        "SKIN",
                        Some(style::SKIN_TONES[st.skin as usize]),
                        format!("TONE {}", st.skin + 1),
                    ),
                    (
                        "HAIR",
                        Some(style::HAIR_COLORS[st.hair as usize]),
                        style::HAIR_NAMES[st.hair as usize].to_string(),
                    ),
                    (
                        "LENGTH",
                        None,
                        style::HAIR_STYLE_NAMES[st.hair_style as usize].to_string(),
                    ),
                    (
                        "FACIAL HAIR",
                        None,
                        style::BEARD_NAMES[st.beard as usize].to_string(),
                    ),
                    (
                        "BUILD",
                        None,
                        style::BUILD_NAMES[st.build as usize].to_string(),
                    ),
                    (
                        "SHIRT",
                        Some(style::SHIRT_COLORS[st.shirt as usize]),
                        style::SHIRT_NAMES[st.shirt as usize].to_string(),
                    ),
                    (
                        "LEGWEAR",
                        None,
                        style::LEGWEAR_NAMES[st.legwear as usize].to_string(),
                    ),
                    (
                        "LEG COLOR",
                        Some(style::TROUSER_COLORS[st.trousers as usize]),
                        style::TROUSER_NAMES[st.trousers as usize].to_string(),
                    ),
                ];
                for (i, (label, c, name)) in rows.iter().enumerate() {
                    let r = self.appearance_row_rect(i);
                    ui.text_shadow(r.0 - 190.0, r.1 + 12.0, 2.0, label, [1.0; 4]);
                    if let Some(c) = c {
                        ui.rect(r.0 - 56.0, r.1 + 2.0, 38.0, 38.0, [0.1, 0.1, 0.1, 1.0]);
                        ui.rect(
                            r.0 - 53.0,
                            r.1 + 5.0,
                            32.0,
                            32.0,
                            [c[0].min(1.0), c[1].min(1.0), c[2].min(1.0), 1.0],
                        );
                    }
                    Self::draw_button(&mut ui, r, name, self.hit(r));
                }
                let hint = "CLICK CYCLES - RIGHT-CLICK GOES BACK";
                let hw = UiBatch::text_width(1.5, hint);
                let hr = self.appearance_back_rect();
                ui.text_shadow(
                    hr.0 + (hr.2 - hw) / 2.0,
                    hr.1 - 22.0,
                    1.5,
                    hint,
                    [0.7, 0.7, 0.7, 1.0],
                );
                Self::draw_button(&mut ui, hr, "BACK", self.hit(hr));
                self.ui = ui;
                return;
            }
            Screen::ConfirmDelete => {
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
                    Self::draw_button(&mut ui, r, label, self.hit(r));
                }
                self.ui = ui;
                return;
            }
            _ => {}
        }

        // Damage flash vignette.
        if self.survival.damage_flash > 0.0 {
            if self.presentation.juice {
                let a = self.survival.damage_flash * 0.5;
                for (frac, aa) in [(1.0, 0.5), (0.66, 0.35), (0.33, 0.25)] {
                    let bw = w * 0.14 * frac;
                    let bh = h * 0.18 * frac;
                    let col = [0.8, 0.1, 0.1, a * aa];
                    ui.rect(0.0, 0.0, bw, h, col);
                    ui.rect(w - bw, 0.0, bw, h, col);
                    ui.rect(0.0, 0.0, w, bh, col);
                    ui.rect(0.0, h - bh, w, bh, col);
                }
            } else {
                ui.rect(
                    0.0,
                    0.0,
                    w,
                    h,
                    [0.8, 0.1, 0.1, self.survival.damage_flash * 0.55],
                );
            }
        }

        // Mod/system toasts, top center (hidden during screenshot
        // sessions — they'd sit over every captured frame).
        for (i, (msg, ttl)) in self
            .presentation
            .toasts
            .iter()
            .filter(|_| self.auto_shot.is_none())
            .enumerate()
        {
            let a = ttl.min(1.0);
            let m = msg.to_uppercase();
            let tw = UiBatch::text_width(2.0, &m);
            ui.text_shadow(
                (w - tw) / 2.0,
                16.0 + i as f32 * 22.0,
                2.0,
                &m,
                [1.0, 1.0, 0.6, a],
            );
        }

        // Keep gameplay instrumentation in gameplay. Inventory and container
        // screens already present those objects directly; repeating the
        // hotbar, vitals, chat, and nameplates behind them creates two
        // competing visual hierarchies.
        if self.ui_state.screen == Screen::Playing {
            // Hotbar.
            for i in 0..HOTBAR_SLOTS {
                let mut r = self.hotbar_rect(i);
                // Selection bounce: 1.0 -> 1.12 -> 1.0 over ~120ms.
                if i == self.input.hotbar_sel && self.presentation.sel_bounce < 1.0 {
                    let t = self.presentation.sel_bounce;
                    let sc = 1.0 + 0.12 * (t * std::f32::consts::PI).sin();
                    let (cx, cy) = (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0);
                    r = (cx - r.2 / 2.0 * sc, cy - r.3 / 2.0 * sc, r.2 * sc, r.3 * sc);
                }
                Self::draw_slot(
                    &self.content.reg,
                    &mut ui,
                    r,
                    self.inventory.slots[i],
                    i == self.input.hotbar_sel,
                    false,
                );
                // Pickup pulse: one bright cycle over the receiving slot.
                let p = self.presentation.slot_pulse[i];
                if p > 0.0 {
                    let a = (p / 0.18) * 0.35;
                    ui.rect(
                        r.0 + 1.0,
                        r.1 + 1.0,
                        r.2 - 2.0,
                        r.3 - 2.0,
                        [1.0, 1.0, 0.9, a],
                    );
                }
            }
            // Ghost icons fly from the pickup point to their slot.
            for &(icon, (fx, fy), slot, age) in &self.presentation.ui_flies {
                let t = (age / 0.22).min(1.0);
                let t = t * t; // ease-in quad
                let r = self.hotbar_rect(slot);
                let (tx, ty) = (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0);
                let x = fx + (tx - fx) * t;
                let y = fy + (ty - fy) * t;
                let sz = 28.0 * (1.0 - 0.4 * t);
                ui.tile(
                    x - sz / 2.0,
                    y - sz / 2.0,
                    sz,
                    sz,
                    icon,
                    [1.0, 1.0, 1.0, 0.9 * (1.0 - t * 0.5)],
                );
            }
            // Selected item name above the hotbar.
            if let Some(s) = self.inventory.slots[self.input.hotbar_sel] {
                let name = &self.content.reg.item(s.item).label.to_uppercase();
                let tw = UiBatch::text_width(2.0, name);
                let (hx0, hy0) = self.hotbar_origin();
                ui.text_shadow(
                    hx0 + (9.0 * Self::SLOT - tw) / 2.0,
                    hy0 - 56.0,
                    2.0,
                    name,
                    [1.0; 4],
                );
            }

            // Hearts above the hotbar (count follows max health).
            let (hx, hy) = self.hotbar_origin();
            let hs = 2.6;
            let hearts = if self.creative {
                0
            } else {
                (self.max_health() / 2.0).ceil() as i32
            };
            let clock = self.total_frames as f32 / 60.0;
            for i in 0..hearts {
                let kind = if self.survival.health >= (i * 2 + 2) as f32 {
                    2
                } else if self.survival.health >= (i * 2 + 1) as f32 {
                    1
                } else {
                    0
                };
                let wobble = if self.presentation.juice && self.survival.health <= 6.0 && kind > 0 {
                    (clock * 9.0 + i as f32 * 1.7).sin() * 2.0
                } else {
                    0.0
                };
                ui.heart(hx + i as f32 * 8.0 * hs, hy - 24.0 + wobble, hs, kind);
            }
            // Armor pips above the hearts, only while wearing any.
            let ap = if self.creative {
                0
            } else {
                self.armor_points()
            };
            for i in 0..ap.min(15) {
                let x = hx + i as f32 * 6.0 * hs * 0.8;
                ui.rect(x, hy - 48.0, 4.0 * hs, 4.0 * hs, [0.75, 0.72, 0.6, 0.95]);
            }
            // Hunger pips, right-aligned above the hotbar.
            let pips = (self.survival.hunger / 2.0).ceil() as i32;
            for i in 0..if self.creative { 0 } else { 10 } {
                let x = hx + 9.0 * Self::SLOT - (i + 1) as f32 * 8.0 * hs;
                let a = if i < pips { 1.0 } else { 0.25 };
                ui.rect(
                    x,
                    hy - 24.0 + 4.0,
                    6.0 * hs * 0.7,
                    5.0 * hs * 0.7,
                    [0.85, 0.55, 0.2, a],
                );
            }
            // Bow draw near the crosshair (red until min draw, then filling).
            if self.interaction.bow_draw > 0.0 {
                let t = ((self.interaction.bow_draw - 0.25) / 0.75).clamp(0.0, 1.0);
                ui.rect(
                    w / 2.0 - 30.0,
                    h / 2.0 + 24.0,
                    60.0,
                    6.0,
                    [0.1, 0.1, 0.1, 0.8],
                );
                let col = if self.interaction.bow_draw < 0.25 {
                    [0.7, 0.3, 0.2, 0.95]
                } else {
                    [0.75, 0.9, 0.5, 0.95]
                };
                ui.rect(w / 2.0 - 30.0, h / 2.0 + 24.0, 60.0 * t.max(0.06), 6.0, col);
            }
            // Chat entry line.
            if self.multiplayer.chat_open {
                ui.rect(12.0, h - 46.0, w * 0.5, 30.0, [0.0, 0.0, 0.0, 0.7]);
                let line = format!("SAY: {}_", self.multiplayer.chat_text.to_uppercase());
                ui.text_shadow(18.0, h - 40.0, 2.0, &line, [1.0; 4]);
            }
            // Other players: world-space identity labels with distance fading,
            // screen clipping, and terrain occlusion.
            if let Some(r) = &self.multiplayer.remote {
                for (name, pos, _) in r.players.values() {
                    self.draw_world_nameplate(&mut ui, name, *pos, w, h);
                }
            }
            if let Some(hst) = &self.multiplayer.host {
                for g in hst.guests.values() {
                    self.draw_world_nameplate(&mut ui, &g.public_label(), g.render_pos().0, w, h);
                }
            }
            if std::env::var("WILDFORGE_DEMO_PLAYER").is_ok() && self.in_world {
                for (i, name) in ["ROWAN", "MICA"].iter().enumerate() {
                    let px = self.player.pos.x.floor() + 0.5 + (i as f32 * 2.0 - 1.0);
                    let pz = self.player.pos.z.floor() + 3.5;
                    let py = self.server.world.surface_height(px as i32, pz as i32) as f32 + 1.0;
                    self.draw_world_nameplate(&mut ui, name, Vec3::new(px, py, pz), w, h);
                }
            }
            // Brushing progress near the crosshair.
            if self.interaction.anvil_work > 0.0 {
                let t = (self.interaction.anvil_work / 2.0).min(1.0);
                ui.rect(
                    w / 2.0 - 30.0,
                    h / 2.0 + 24.0,
                    60.0,
                    6.0,
                    [0.1, 0.1, 0.1, 0.8],
                );
                ui.rect(
                    w / 2.0 - 30.0,
                    h / 2.0 + 24.0,
                    60.0 * t,
                    6.0,
                    [0.85, 0.85, 0.9, 0.95],
                );
            }
            if self.interaction.brushing > 0.0 {
                let t = (self.interaction.brushing / 1.5).min(1.0);
                ui.rect(
                    w / 2.0 - 30.0,
                    h / 2.0 + 24.0,
                    60.0,
                    6.0,
                    [0.1, 0.1, 0.1, 0.8],
                );
                ui.rect(
                    w / 2.0 - 30.0,
                    h / 2.0 + 24.0,
                    60.0 * t,
                    6.0,
                    [0.75, 0.7, 0.5, 0.95],
                );
            }
            // Eat progress near the crosshair.
            if self.survival.eating > 0.0
                && let Some(f) = self.inventory.slots[self.input.hotbar_sel]
                    .and_then(|s| self.content.reg.item(s.item).food.clone())
            {
                let t = (self.survival.eating / f.eat_time).min(1.0);
                ui.rect(
                    w / 2.0 - 30.0,
                    h / 2.0 + 24.0,
                    60.0,
                    6.0,
                    [0.1, 0.1, 0.1, 0.8],
                );
                ui.rect(
                    w / 2.0 - 30.0,
                    h / 2.0 + 24.0,
                    60.0 * t,
                    6.0,
                    [0.9, 0.8, 0.3, 0.95],
                );
            }

            // Air bubbles (right-aligned above hotbar) when submerged.
            if self.survival.air < MAX_AIR && !self.creative {
                let n = (self.survival.air / MAX_AIR * 10.0).ceil() as usize;
                for i in 0..n {
                    let x = hx + 9.0 * Self::SLOT - (i + 1) as f32 * 8.0 * hs;
                    ui.bubble(x, hy - 16.0 * hs - 8.0, hs);
                }
            }
            self.draw_roster_overlay(&mut ui, w);
        }

        match self.ui_state.screen {
            Screen::Playing
            | Screen::Title
            | Screen::Accounts
            | Screen::Moderation(_)
            | Screen::Mods
            | Screen::Packs
            | Screen::Join
            | Screen::Settings
            | Screen::Appearance
            | Screen::ConfirmDelete => {}
            Screen::Furnace(pos) => {
                ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
                let title = "FURNACE";
                let tw = UiBatch::text_width(3.0, title);
                ui.text_shadow((w - tw) / 2.0, h / 2.0 - 285.0, 3.0, title, [1.0; 4]);
                let (inp, fuel, out, prog, burn) = self.furnace_view(pos);
                let ir = self.furnace_slot_rect(0);
                let fr = self.furnace_slot_rect(1);
                let orr = self.furnace_slot_rect(2);
                Self::draw_slot(&self.content.reg, &mut ui, ir, inp, false, self.hit(ir));
                Self::draw_slot(&self.content.reg, &mut ui, fr, fuel, false, self.hit(fr));
                Self::draw_slot(&self.content.reg, &mut ui, orr, out, false, self.hit(orr));
                // Flame between input and fuel, arrow toward the output.
                let flame_h = 24.0 * burn;
                ui.rect(
                    ir.0 + 12.0,
                    fr.1 - 4.0 - flame_h,
                    22.0,
                    flame_h,
                    [1.0, 0.55, 0.1, 0.95],
                );
                let ay = ir.1 + Self::SLOT + 14.0;
                ui.rect(ir.0 + 64.0, ay, 100.0, 8.0, [0.15, 0.15, 0.15, 0.9]);
                ui.rect(ir.0 + 64.0, ay, 100.0 * prog, 8.0, [1.0, 1.0, 1.0, 0.95]);
                // Player inventory below for restocking.
                for i in 0..TOTAL_SLOTS {
                    let r = self.inv_slot_rect(i);
                    Self::draw_slot(
                        &self.content.reg,
                        &mut ui,
                        r,
                        self.inventory.slots[i],
                        i == self.input.hotbar_sel,
                        self.hit(r),
                    );
                }
                self.draw_browser(&mut ui);
                if let Some(s) = self.ui_state.held_stack {
                    let (cx, cy) = self.input.ui_cursor;
                    let icon = self.content.reg.item(s.item).icon;
                    ui.tile(cx - 16.0, cy - 16.0, 32.0, 32.0, icon, [1.0; 4]);
                    if s.count > 1 {
                        ui.text_shadow(cx + 6.0, cy + 4.0, 2.0, &format!("{}", s.count), [1.0; 4]);
                    }
                }
                self.ui = ui;
                return;
            }
            Screen::Bloomery(pos) => {
                ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
                // The forge rides the bloomery screen: same slots,
                // its own shell check and firing clock.
                let forge = matches!(
                    self.server.world.block_entity(&pos),
                    Some(world::BlockEntity::Forge(_))
                );
                let title = if forge { "FORGE" } else { "BLOOMERY" };
                let tw = UiBatch::text_width(3.0, title);
                ui.text_shadow((w - tw) / 2.0, h / 2.0 - 300.0, 3.0, title, [1.0; 4]);
                let (slots, lit, progress, breached) = {
                    let breached = if forge {
                        self.server.world.check_forge(pos.0, pos.1, pos.2).is_none()
                    } else {
                        self.server
                            .world
                            .check_bloomery(pos.0, pos.1, pos.2)
                            .is_none()
                    };
                    match self.server.world.block_entity(&pos) {
                        Some(world::BlockEntity::Bloomery(b))
                        | Some(world::BlockEntity::Forge(b)) => {
                            let mut v = [None; 8];
                            v[..4].copy_from_slice(&b.charge);
                            v[4..].copy_from_slice(&b.fuel);
                            let secs = if forge {
                                world::FORGE_FIRE_SECS
                            } else {
                                world::BLOOMERY_FIRE_SECS
                            };
                            (v, b.lit, b.progress / secs, breached)
                        }
                        _ => ([None; 8], false, 0.0, breached),
                    }
                };
                ui.text_shadow(w / 2.0 - 150.0, h / 2.0 - 268.0, 1.5, "CHARGE", [1.0; 4]);
                let fuel_label = if forge { "FUEL" } else { "CHARCOAL" };
                ui.text_shadow(w / 2.0 - 150.0, h / 2.0 - 186.0, 1.5, fuel_label, [1.0; 4]);
                for (i, s) in slots.iter().enumerate() {
                    let r = self.bloomery_slot_rect(i);
                    Self::draw_slot(&self.content.reg, &mut ui, r, *s, false, self.hit(r));
                }
                let lr = self.bloomery_light_rect();
                if lit {
                    let br = (
                        w / 2.0 - 2.0 * (Self::SLOT + 10.0) + 5.0,
                        h / 2.0 - 120.0,
                        4.0 * (Self::SLOT + 10.0) - 10.0,
                        10.0,
                    );
                    ui.rect(br.0, br.1, br.2, br.3, [0.15, 0.15, 0.15, 0.9]);
                    ui.rect(br.0, br.1, br.2 * progress, br.3, [1.0, 0.55, 0.1, 0.95]);
                    ui.text_shadow(
                        br.0,
                        br.1 + 16.0,
                        1.5,
                        "FIRING - SEALED",
                        [1.0, 0.8, 0.5, 1.0],
                    );
                } else if breached {
                    ui.text_shadow(
                        lr.0,
                        lr.1 + 44.0,
                        1.5,
                        if forge {
                            "WANTS STACK, CHIMNEY, ANVIL"
                        } else {
                            "THE STACK IS BREACHED"
                        },
                        [1.0, 0.5, 0.4, 1.0],
                    );
                    Self::draw_button(&mut ui, lr, "LIGHT", false);
                } else {
                    Self::draw_button(&mut ui, lr, "LIGHT", self.hit(lr));
                }
                for i in 0..TOTAL_SLOTS {
                    let r = self.inv_slot_rect(i);
                    Self::draw_slot(
                        &self.content.reg,
                        &mut ui,
                        r,
                        self.inventory.slots[i],
                        i == self.input.hotbar_sel,
                        self.hit(r),
                    );
                }
                self.draw_browser(&mut ui);
                if let Some(s) = self.ui_state.held_stack {
                    let (cx, cy) = self.input.ui_cursor;
                    let icon = self.content.reg.item(s.item).icon;
                    ui.tile(cx - 16.0, cy - 16.0, 32.0, 32.0, icon, [1.0; 4]);
                    if s.count > 1 {
                        ui.text_shadow(cx + 6.0, cy + 4.0, 2.0, &format!("{}", s.count), [1.0; 4]);
                    }
                }
                self.ui = ui;
                return;
            }
            Screen::Kiln(pos) => {
                ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
                let title = if self
                    .server
                    .world
                    .check_glassworks(pos.0, pos.1, pos.2)
                    .is_some()
                {
                    "GLASSWORKS"
                } else {
                    "GLASS KILN"
                };
                let tw = UiBatch::text_width(3.0, title);
                ui.text_shadow((w - tw) / 2.0, h / 2.0 - 310.0, 3.0, title, [1.0; 4]);
                let (slots, lit, progress, breached) = {
                    let breached = self.server.world.check_kiln(pos.0, pos.1, pos.2).is_none();
                    match self.server.world.block_entity(&pos) {
                        Some(world::BlockEntity::Kiln(k)) => {
                            let mut v = [None; 9];
                            v[..4].copy_from_slice(&k.sand);
                            v[4] = k.powder;
                            v[5..].copy_from_slice(&k.fuel);
                            (v, k.lit, k.progress / world::KILN_FIRE_SECS, breached)
                        }
                        _ => ([None; 9], false, 0.0, breached),
                    }
                };
                ui.text_shadow(w / 2.0 - 150.0, h / 2.0 - 288.0, 1.5, "SAND", [1.0; 4]);
                ui.text_shadow(w / 2.0 - 150.0, h / 2.0 - 210.0, 1.5, "PIGMENT", [1.0; 4]);
                ui.text_shadow(w / 2.0 - 150.0, h / 2.0 - 132.0, 1.5, "CHARCOAL", [1.0; 4]);
                for (i, sl) in slots.iter().enumerate() {
                    let r = self.kiln_slot_rect(i);
                    Self::draw_slot(&self.content.reg, &mut ui, r, *sl, false, self.hit(r));
                }
                let lr = self.bloomery_light_rect();
                if lit {
                    let br = (
                        w / 2.0 - 2.0 * (Self::SLOT + 10.0) + 5.0,
                        h / 2.0 - 60.0,
                        4.0 * (Self::SLOT + 10.0) - 10.0,
                        10.0,
                    );
                    ui.rect(br.0, br.1, br.2, br.3, [0.15, 0.15, 0.15, 0.9]);
                    ui.rect(br.0, br.1, br.2 * progress, br.3, [1.0, 0.9, 0.5, 0.95]);
                    ui.text_shadow(
                        br.0,
                        br.1 + 16.0,
                        1.5,
                        "FIRING - SEALED",
                        [1.0, 0.9, 0.6, 1.0],
                    );
                } else if breached {
                    ui.text_shadow(
                        lr.0,
                        lr.1 + 44.0,
                        1.5,
                        "THE STACK IS BREACHED",
                        [1.0, 0.5, 0.4, 1.0],
                    );
                    Self::draw_button(&mut ui, lr, "LIGHT", false);
                } else {
                    Self::draw_button(&mut ui, lr, "LIGHT", self.hit(lr));
                }
                for i in 0..TOTAL_SLOTS {
                    let r = self.inv_slot_rect(i);
                    Self::draw_slot(
                        &self.content.reg,
                        &mut ui,
                        r,
                        self.inventory.slots[i],
                        i == self.input.hotbar_sel,
                        self.hit(r),
                    );
                }
                self.draw_browser(&mut ui);
                if let Some(s) = self.ui_state.held_stack {
                    let (cx, cy) = self.input.ui_cursor;
                    let icon = self.content.reg.item(s.item).icon;
                    ui.tile(cx - 16.0, cy - 16.0, 32.0, 32.0, icon, [1.0; 4]);
                    if s.count > 1 {
                        ui.text_shadow(cx + 6.0, cy + 4.0, 2.0, &format!("{}", s.count), [1.0; 4]);
                    }
                }
                self.ui = ui;
                return;
            }
            Screen::Chest(pos) => {
                ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
                let title = "CHEST";
                let tw = UiBatch::text_width(3.0, title);
                ui.text_shadow((w - tw) / 2.0, h / 2.0 - 340.0, 3.0, title, [1.0; 4]);
                let slots = match self.server.world.block_entity(&pos) {
                    Some(world::BlockEntity::Chest(c)) => c.slots,
                    _ => [None; world::CHEST_SLOTS],
                };
                for (i, st) in slots.iter().enumerate() {
                    let r = self.chest_slot_rect(i);
                    Self::draw_slot(&self.content.reg, &mut ui, r, *st, false, self.hit(r));
                }
                for i in 0..TOTAL_SLOTS {
                    let r = self.inv_slot_rect(i);
                    Self::draw_slot(
                        &self.content.reg,
                        &mut ui,
                        r,
                        self.inventory.slots[i],
                        i == self.input.hotbar_sel,
                        self.hit(r),
                    );
                }
                self.draw_browser(&mut ui);
                if let Some(s) = self.ui_state.held_stack {
                    let (cx, cy) = self.input.ui_cursor;
                    let icon = self.content.reg.item(s.item).icon;
                    ui.tile(cx - 16.0, cy - 16.0, 32.0, 32.0, icon, [1.0; 4]);
                    if s.count > 1 {
                        ui.text_shadow(cx + 6.0, cy + 4.0, 2.0, &format!("{}", s.count), [1.0; 4]);
                    }
                }
                self.ui = ui;
                return;
            }
            Screen::Offering(pos) => {
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
                let slots = match self.server.world.block_entity(&pos) {
                    Some(world::BlockEntity::Offering(o)) => o.slots,
                    _ => [None; 3],
                };
                for (i, st) in slots.iter().enumerate() {
                    let r = self.offering_slot_rect(i);
                    Self::draw_slot(&self.content.reg, &mut ui, r, *st, false, self.hit(r));
                }
                for i in 0..TOTAL_SLOTS {
                    let r = self.inv_slot_rect(i);
                    Self::draw_slot(
                        &self.content.reg,
                        &mut ui,
                        r,
                        self.inventory.slots[i],
                        i == self.input.hotbar_sel,
                        self.hit(r),
                    );
                }
                self.draw_browser(&mut ui);
                if let Some(s) = self.ui_state.held_stack {
                    let (cx, cy) = self.input.ui_cursor;
                    let icon = self.content.reg.item(s.item).icon;
                    ui.tile(cx - 16.0, cy - 16.0, 32.0, 32.0, icon, [1.0; 4]);
                    if s.count > 1 {
                        ui.text_shadow(cx + 6.0, cy + 4.0, 2.0, &format!("{}", s.count), [1.0; 4]);
                    }
                }
                self.ui = ui;
                return;
            }
            Screen::Inventory => {
                self.draw_inventory_screen(&mut ui);
            }
            Screen::Paused => {
                ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.6]);
                let title = "GAME PAUSED";
                let tw = UiBatch::text_width(4.0, title);
                ui.text_shadow((w - tw) / 2.0, h / 2.0 - 130.0, 4.0, title, [1.0; 4]);
                let mode = if self.creative {
                    "MODE: CREATIVE"
                } else {
                    "MODE: SURVIVAL"
                };
                let friends = match &self.multiplayer.host {
                    Some(h) => format!(
                        "FRIENDS: {} CONNECTED ({})",
                        h.guests.len(),
                        h.identity_policy.as_str().to_uppercase()
                    ),
                    None if self.multiplayer.remote.is_some() => "CONNECTED AS GUEST".to_string(),
                    None => "OPEN TO FRIENDS".to_string(),
                };
                for (i, label) in [
                    "RESUME",
                    mode,
                    &friends,
                    "SETTINGS",
                    "APPEARANCE",
                    "SAVE AND QUIT TO TITLE",
                ]
                .iter()
                .enumerate()
                {
                    let r = self.menu_button_rect(i);
                    Self::draw_button(&mut ui, r, label, self.hit(r));
                }
                // Hosting: each guest gets a name row and a KICK button.
                for (row, (_, name)) in self.guest_rows().iter().enumerate() {
                    let r = self.kick_rect(row);
                    ui.text_shadow(
                        r.0,
                        r.1 - 16.0,
                        1.5,
                        &name.to_uppercase(),
                        [0.9, 0.9, 0.9, 1.0],
                    );
                    Self::draw_button(&mut ui, r, "MANAGE", self.hit(r));
                }
            }
            Screen::Dead => {
                ui.rect(0.0, 0.0, w, h, [0.5, 0.0, 0.0, 0.5]);
                let title = "YOU DIED";
                let tw = UiBatch::text_width(5.0, title);
                ui.text_shadow(
                    (w - tw) / 2.0,
                    h / 2.0 - 120.0,
                    5.0,
                    title,
                    [1.0, 0.85, 0.85, 1.0],
                );
                if self.survival.killed_by_wild {
                    let sub = "RECLAIMED BY THE WILD";
                    let sw = UiBatch::text_width(2.0, sub);
                    ui.text_shadow(
                        (w - sw) / 2.0,
                        h / 2.0 - 60.0,
                        2.0,
                        sub,
                        [0.8, 0.95, 0.75, 1.0],
                    );
                }
                let r = self.menu_button_rect(0);
                let hover = self.hit(r);
                let bg = if hover {
                    [0.5, 0.5, 0.5, 0.95]
                } else {
                    [0.25, 0.25, 0.25, 0.95]
                };
                ui.rect(r.0, r.1, r.2, r.3, [0.1, 0.1, 0.1, 0.95]);
                ui.rect(r.0 + 2.0, r.1 + 2.0, r.2 - 4.0, r.3 - 4.0, bg);
                let lw = UiBatch::text_width(2.0, "RESPAWN");
                ui.text_shadow(
                    r.0 + (r.2 - lw) / 2.0,
                    r.1 + (r.3 - 14.0) / 2.0,
                    2.0,
                    "RESPAWN",
                    [1.0; 4],
                );
            }
        }
        self.ui = ui;
    }

    // ---------- Menu / inventory clicks ----------
}

#[cfg(test)]
mod characterization {
    use super::project_world_label;
    use glam::{Mat4, Vec3};

    #[test]
    fn world_labels_project_to_pixels_and_clip_offscreen_points() {
        assert_eq!(
            project_world_label(Mat4::IDENTITY, Vec3::ZERO, 800.0, 600.0),
            Some((400.0, 300.0))
        );
        assert_eq!(
            project_world_label(Mat4::IDENTITY, Vec3::new(2.0, 0.0, 0.0), 800.0, 600.0),
            None
        );
    }
}
