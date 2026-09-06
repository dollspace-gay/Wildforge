//! Preferences menu actions.

use crate::audio::Sfx;
use crate::style;
use crate::game::Game;
use crate::game::navigation::Screen;

impl Game {
    pub(in crate::game) fn click_mods_menu(&mut self) {
                if self.hit(self.menu_button_rect(4)) {
                    self.sfx(Sfx::Click);
                    self.set_screen(Screen::Title);
                }
                }

    pub(in crate::game) fn click_packs_menu(&mut self) {
                if self.hit(self.pack_back_rect()) {
                    self.sfx(Sfx::Click);
                    self.set_screen(Screen::Title);
                    return;
                }
                for i in 0..=self.content.packs.len().min(7) {
                    if self.hit(self.pack_row_rect(i)) {
                        let sel = if i == 0 {
                            String::new()
                        } else {
                            self.content.packs[i - 1].id.clone()
                        };
                        self.sfx(Sfx::Click);
                        if sel != self.active_pack_id() {
                            self.content.pack_override = None;
                            self.config.pack = sel;
                            self.apply_pack();
                        }
                        return;
                    }
                }
                }

    pub(in crate::game) fn click_settings_menu(&mut self) {
                for i in 0..Self::SLIDERS.len() {
                    let (bx, by, bw, bh) = self.slider_bar_rect(i);
                    let (cx, cy) = self.input.ui_cursor;
                    if cx >= bx && cx < bx + bw && cy >= by && cy < by + bh {
                        self.ui_state.dragging_slider = Some(i);
                        self.set_slider(i, (cx - bx - 2.0) / (bw - 4.0));
                        return;
                    }
                }
                if self.hit(self.settings_toggle_rect(0)) {
                    self.sfx(Sfx::Click);
                    self.config.lights = (self.config.lights + 1) % 3;
                    return;
                }
                if self.hit(self.settings_toggle_rect(1)) {
                    self.sfx(Sfx::Click);
                    self.config.point_grid = !self.config.point_grid;
                    return;
                }
                if self.hit(self.settings_toggle_rect(2)) {
                    self.sfx(Sfx::Click);
                    self.config.stark = !self.config.stark;
                    return;
                }
                if self.hit(self.settings_toggle_rect(3)) {
                    self.sfx(Sfx::Click);
                    self.config.outline = !self.config.outline;
                    return;
                }
                if self.hit(self.settings_toggle_rect(4)) {
                    self.sfx(Sfx::Click);
                    self.config.bloom = !self.config.bloom;
                    return;
                }
                if self.hit(self.settings_back_rect()) {
                    self.sfx(Sfx::Click);
                    self.config.save();
                    self.set_screen(if self.ui_state.settings_from_pause {
                        Screen::Paused
                    } else {
                        Screen::Title
                    });
                }
                }

    pub(in crate::game) fn click_appearance_menu(&mut self , right: bool) {
                let lens: [usize; 8] = [
                    style::SKIN_TONES.len(),
                    style::HAIR_COLORS.len(),
                    style::HAIR_STYLE_NAMES.len(),
                    style::BEARD_NAMES.len(),
                    style::BUILD_NAMES.len(),
                    style::SHIRT_COLORS.len(),
                    style::LEGWEAR_NAMES.len(),
                    style::TROUSER_COLORS.len(),
                ];
                for (i, len) in lens.iter().enumerate() {
                    if self.hit(self.appearance_row_rect(i)) {
                        self.sfx(Sfx::Click);
                        let n = *len as i32;
                        let cur = match i {
                            0 => &mut self.style.skin,
                            1 => &mut self.style.hair,
                            2 => &mut self.style.hair_style,
                            3 => &mut self.style.beard,
                            4 => &mut self.style.build,
                            5 => &mut self.style.shirt,
                            6 => &mut self.style.legwear,
                            _ => &mut self.style.trousers,
                        };
                        let step = if right { -1 } else { 1 };
                        *cur = ((*cur as i32 + step).rem_euclid(n)) as u8;
                        self.config.appearance = self.style.pack();
                        return;
                    }
                }
                if self.hit(self.appearance_back_rect()) {
                    self.sfx(Sfx::Click);
                    self.config.save();
                    self.set_screen(if self.ui_state.appearance_from_pause {
                        Screen::Paused
                    } else {
                        Screen::Title
                    });
                }
                }
}
