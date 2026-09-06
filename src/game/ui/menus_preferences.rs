//! Menus preferences layout and UI composition.

use crate::atlas;
use crate::game::Game;
use crate::game::widgets;
use crate::style;
use crate::ui::UiBatch;

impl Game {
    pub(in crate::game) fn draw_mods_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32) {
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
        widgets::button(&mut *ui, br, "BACK", self.hit(br));
    }
    pub(in crate::game) fn draw_packs_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32) {
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
            widgets::button(&mut *ui, r, &label, self.hit(r));
            let cur = self.active_pack_id();
            let active = if i == 0 {
                cur.is_empty() || atlas::pack_source_of(&cur).is_none()
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
        widgets::button(&mut *ui, br, "BACK", self.hit(br));
    }
    pub(in crate::game) fn draw_settings_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32) {
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
                if self.config.point_grid {
                    "GRID"
                } else {
                    "CUBE"
                },
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
            widgets::button(&mut *ui, r, value, self.hit(r));
        }
        let br = self.settings_back_rect();
        widgets::button(&mut *ui, br, "BACK", self.hit(br));
    }
    pub(in crate::game) fn draw_appearance_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32) {
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
            widgets::button(&mut *ui, r, name, self.hit(r));
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
        widgets::button(&mut *ui, hr, "BACK", self.hit(hr));
    }
}
