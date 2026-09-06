//! Worlds menu actions.

use crate::atlas;
use crate::audio::Sfx;
use crate::game::Game;
use crate::game::navigation::Screen;
use crate::net;
use std::path::PathBuf;
use winit::event_loop::ActiveEventLoop;

impl Game {
    pub(in crate::game) fn click_title_menu(&mut self, event_loop: &ActiveEventLoop) {
        for i in 0..self.worlds.len().min(6) {
            if self.hit(self.title_play_rect(i)) {
                self.sfx(Sfx::Click);
                let name = self.worlds[i].0.clone();
                self.start_world(&name);
                return;
            }
            if self.hit(self.title_delete_rect(i)) {
                self.sfx(Sfx::Click);
                self.ui_state.pending_delete = Some(i);
                self.set_screen(Screen::ConfirmDelete);
                return;
            }
        }
        if self.hit(self.title_action_rect(0)) {
            self.sfx(Sfx::Click);
            self.open_new_world("survival");
        } else if self.hit(self.title_action_rect(1)) {
            self.sfx(Sfx::Click);
            self.open_new_world("creative");
        } else if self.hit(self.title_action_rect(2)) {
            self.sfx(Sfx::Click);
            self.multiplayer.discovery = net::Discovery::start().ok();
            self.multiplayer.join_status.clear();
            self.set_screen(Screen::Join);
        } else if self.hit(self.title_action_rect(3)) {
            self.sfx(Sfx::Click);
            self.ui_state.account_name = self.config.display_name.clone();
            self.set_screen(Screen::Accounts);
        } else if self.hit(self.title_action_rect(4)) {
            self.sfx(Sfx::Click);
            self.ui_state.appearance_from_pause = false;
            self.set_screen(Screen::Appearance);
        } else if self.hit(self.title_action_rect(5)) {
            self.sfx(Sfx::Click);
            self.set_screen(Screen::Mods);
        } else if self.hit(self.title_action_rect(6)) {
            self.sfx(Sfx::Click);
            self.content.packs = atlas::discover_packs();
            self.set_screen(Screen::Packs);
        } else if self.hit(self.title_action_rect(7)) {
            self.sfx(Sfx::Click);
            self.ui_state.settings_from_pause = false;
            self.set_screen(Screen::Settings);
        } else if self.hit(self.title_action_rect(8)) {
            event_loop.exit();
        }
    }

    pub(in crate::game) fn click_new_world_menu(&mut self) {
        if self.hit(self.new_world_button_rect(0)) {
            self.sfx(Sfx::Click);
            self.create_new_world();
        } else if self.hit(self.new_world_button_rect(1)) {
            self.sfx(Sfx::Click);
            self.roll_new_world_seed();
            self.ui_state.new_world_status.clear();
        } else if self.hit(self.new_world_button_rect(2)) {
            self.sfx(Sfx::Click);
            self.set_screen(Screen::Title);
        }
        // Mode cycler arrows (capability E1).
        let modes = self.available_new_world_modes();
        if modes.len() > 1 {
            let w = self.renderer.config.width as f32;
            let h = self.renderer.config.height as f32;
            let mode_y = h * 0.56;
            let left_r = (w / 2.0 - 200.0, mode_y - 6.0, 36.0, 30.0);
            let right_r = (w / 2.0 + 164.0, mode_y - 6.0, 36.0, 30.0);
            let cur = modes
                .iter()
                .position(|m| m == &self.ui_state.new_world_mode)
                .unwrap_or(0);
            if self.hit(left_r) {
                self.sfx(Sfx::Click);
                let prev = if cur == 0 { modes.len() - 1 } else { cur - 1 };
                self.ui_state.new_world_mode = modes[prev].clone();
            } else if self.hit(right_r) {
                self.sfx(Sfx::Click);
                let next = (cur + 1) % modes.len();
                self.ui_state.new_world_mode = modes[next].clone();
            }
        }
    }

    pub(in crate::game) fn click_creating_world_menu(&mut self) {
        if self.hit(self.world_creation_cancel_rect()) {
            self.sfx(Sfx::Click);
            self.cancel_world_creation();
        }
    }

    pub(in crate::game) fn click_confirm_delete_menu(&mut self) {
        if self.hit(self.menu_button_rect(0)) {
            self.sfx(Sfx::Click);
            if let Some(i) = self.ui_state.pending_delete.take() {
                if let Some((name, _)) = self.worlds.get(i) {
                    let _ = std::fs::remove_dir_all(PathBuf::from("saves").join(name));
                }
                self.refresh_worlds();
            }
            self.set_screen(Screen::Title);
        } else if self.hit(self.menu_button_rect(1)) {
            self.sfx(Sfx::Click);
            self.ui_state.pending_delete = None;
            self.set_screen(Screen::Title);
        }
    }
}
