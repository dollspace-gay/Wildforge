//! Pause menu actions.

use crate::audio::Sfx;
use crate::identity;
use crate::mp;
use crate::net;
use crate::world;
use crate::game::Game;
use crate::game::navigation::Screen;

impl Game {
    pub(in crate::game) fn click_paused_menu(&mut self) {
                if self.hit(self.menu_button_rect(0)) {
                    self.sfx(Sfx::Click);
                    self.set_screen(Screen::Playing);
                } else if self.hit(self.menu_button_rect(1)) {
                    self.sfx(Sfx::Click);
                    let next_creative = !self.creative;
                    let mode = if next_creative {
                        "creative"
                    } else {
                        "survival"
                    };
                    let saved = world::write_world_meta_full(
                        &self.runtime.local().world.save_dir_for_saving(),
                        self.runtime.view().seed(),
                        mode,
                        self.runtime.view().ire(),
                        self.runtime.view().day(),
                        &self.runtime.local().world.camera,
                    );
                    match saved {
                        Ok(()) => {
                            self.creative = next_creative;
                            self.flying = false;
                            self.runtime.local_mut().world.mode = mode.to_string();
                            if self.content.scripts.wants("on_mode_change") {
                                self.content.scripts.dispatch_view(
                                    &self.runtime.view(),
                                    "on_mode_change",
                                    (mode.to_string(),),
                                );
                                self.apply_script_cmds();
                            }
                        }
                        Err(error) => {
                            self.toast(format!("Could not change mode: {error}"));
                        }
                    }
                } else if self.hit(self.menu_button_rect(2)) {
                    self.sfx(Sfx::Click);
                    if self.multiplayer.host.is_none() && self.multiplayer.remote.is_none() {
                        let wname = self.runtime.local().world.save_dir_for_saving()
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "world".into());
                        let host_name = identity::DisplayName::parse(&self.config.display_name)
                            .expect("configured display name is valid");
                        match mp::HostSession::start_windowed(wname, host_name) {
                            Ok(mut sess) => {
                                sess.fresh_spawn = self.runtime.local().world.common_spawn();
                                self.runtime.local_mut().world.set_edit_logging(true);
                                self.toast(format!(
                                    "Open to friends on port {} (LAN + direct IP).",
                                    sess.net.port
                                ));
                                self.multiplayer.host = Some(sess);
                            }
                            Err(e) => self.toast(format!("Could not host: {e}")),
                        }
                    }
                } else if self.hit(self.menu_button_rect(3)) {
                    self.sfx(Sfx::Click);
                    self.ui_state.settings_from_pause = true;
                    self.set_screen(Screen::Settings);
                } else if self.hit(self.menu_button_rect(4)) {
                    self.sfx(Sfx::Click);
                    self.ui_state.appearance_from_pause = true;
                    self.set_screen(Screen::Appearance);
                } else if self.hit(self.menu_button_rect(5)) {
                    self.sfx(Sfx::Click);
                    self.quit_to_title();
                } else {
                    for (row, (id, _)) in self.guest_rows().iter().enumerate() {
                        if self.hit(self.kick_rect(row)) {
                            self.sfx(Sfx::Click);
                            self.ui_state.moderation_confirm = None;
                            self.set_screen(Screen::Moderation(*id));
                            break;
                        }
                    }
                }
                }

    pub(in crate::game) fn click_dead_menu(&mut self) {
                if self.hit(self.menu_button_rect(0)) {
                    self.sfx(Sfx::Click);
                    self.respawn();
                }
                }
}
