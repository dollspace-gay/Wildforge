//! Keyboard mapping and high-level key actions.

use crate::audio::Sfx;
use glam::Vec3;
use super::Game;
use super::navigation::Screen;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::KeyCode;
use winit::window::Fullscreen;

impl Game {
    pub(super) fn key(&mut self, code: KeyCode, pressed: bool, _event_loop: &ActiveEventLoop) {
        if std::env::var_os("WILDFORGE_INPUT_TRACE").is_some()
            && matches!(
                code,
                KeyCode::KeyW | KeyCode::KeyA | KeyCode::KeyS | KeyCode::KeyD | KeyCode::F2
            )
        {
            eprintln!(
                "input: {code:?} {}",
                if pressed { "pressed" } else { "released" }
            );
        }
        match code {
            KeyCode::KeyW | KeyCode::ArrowUp => self.input.keys.w = pressed,
            KeyCode::KeyA | KeyCode::ArrowLeft => self.input.keys.a = pressed,
            KeyCode::KeyS | KeyCode::ArrowDown => self.input.keys.s = pressed,
            KeyCode::KeyD | KeyCode::ArrowRight => self.input.keys.d = pressed,
            KeyCode::Space => {
                if pressed && self.creative && self.ui_state.screen == Screen::Playing {
                    if self.time_abs - self.last_space < 0.3 {
                        self.flying = !self.flying;
                        self.player.vel = Vec3::ZERO;
                    }
                    self.last_space = self.time_abs;
                }
                self.input.keys.space = pressed;
            }
            KeyCode::ControlLeft | KeyCode::ControlRight => self.input.keys.sprint = pressed,
            KeyCode::AltLeft | KeyCode::AltRight
                if pressed && self.ui_state.screen == Screen::Playing =>
            {
                self.input.dodge_pressed = true;
            }
            KeyCode::KeyF => self.input.keys.block = pressed,
            KeyCode::Escape if pressed => match self.ui_state.screen {
                Screen::Playing => self.set_screen(Screen::Paused),
                Screen::CreatingWorld => self.cancel_world_creation(),
                Screen::NewWorld => self.set_screen(Screen::Title),
                Screen::Inventory
                | Screen::Furnace(_)
                | Screen::Chest(_)
                | Screen::Offering(_)
                | Screen::Bloomery(_)
                | Screen::Kiln(_)
                | Screen::Workbench(_)
                | Screen::Mod(_)
                | Screen::MobCargo(_)
                | Screen::Stall(_)
                | Screen::Dialog { .. }
                | Screen::Journal
                | Screen::Skills
                | Screen::Loadout => self.set_screen(Screen::Playing),
                Screen::SignEdit(pos) => self.commit_sign(pos),
                Screen::Paused => self.set_screen(Screen::Playing),
                Screen::Settings => {
                    self.config.save();
                    self.set_screen(if self.ui_state.settings_from_pause {
                        Screen::Paused
                    } else {
                        Screen::Title
                    });
                }
                Screen::Appearance => {
                    self.config.save();
                    self.set_screen(if self.ui_state.appearance_from_pause {
                        Screen::Paused
                    } else {
                        Screen::Title
                    });
                }
                Screen::ConfirmDelete => {
                    self.ui_state.pending_delete = None;
                    self.set_screen(Screen::Title);
                }
                Screen::Mods | Screen::Packs => self.set_screen(Screen::Title),
                Screen::Join => {
                    self.multiplayer.discovery = None;
                    self.set_screen(Screen::Title);
                }
                Screen::Accounts if self.config.profile_complete => self.set_screen(Screen::Title),
                Screen::Accounts => {}
                Screen::Moderation(_) => {
                    self.ui_state.moderation_confirm = None;
                    self.set_screen(Screen::Paused);
                }
                Screen::Title | Screen::Dead => {}
            },
            KeyCode::KeyE if pressed && self.in_world => match self.ui_state.screen {
                Screen::Playing => {
                    self.interaction.craft_size = 2;
                    self.set_screen(Screen::Inventory);
                }
                Screen::Inventory
                | Screen::Furnace(_)
                | Screen::Chest(_)
                | Screen::Offering(_)
                | Screen::Bloomery(_)
                | Screen::Kiln(_)
                | Screen::Workbench(_)
                | Screen::Mod(_)
                | Screen::MobCargo(_)
                | Screen::Stall(_)
                | Screen::Dialog { .. }
                | Screen::Journal
                | Screen::Skills
                | Screen::Loadout => self.set_screen(Screen::Playing),
                _ => {}
            },
            // Quest journal (spec 3.3): read-only progress overview.
            KeyCode::KeyJ if pressed && self.in_world => match self.ui_state.screen {
                Screen::Playing => self.set_screen(Screen::Journal),
                Screen::Journal => self.set_screen(Screen::Playing),
                _ => {}
            },
            // Skill tree (capability E5): allocate mode-gated points.
            KeyCode::KeyK if pressed && self.in_world => match self.ui_state.screen {
                Screen::Playing => self.set_screen(Screen::Skills),
                Screen::Skills => self.set_screen(Screen::Playing),
                _ => {}
            },
            // Loadout (capability E6): slot components, repair, presets.
            KeyCode::KeyL if pressed && self.in_world => match self.ui_state.screen {
                Screen::Playing => self.set_screen(Screen::Loadout),
                Screen::Loadout => self.set_screen(Screen::Playing),
                _ => {}
            },
            KeyCode::KeyT
                if pressed
                    && self.ui_state.screen == Screen::Playing
                    && (self.multiplayer.host.is_some() || self.multiplayer.remote.is_some()) =>
            {
                self.multiplayer.chat_open = true;
                self.multiplayer.chat_text.clear();
            }
            KeyCode::Tab
                if pressed
                    && self.ui_state.screen == Screen::Playing
                    && self.multiplayer.host.is_none()
                    && self.multiplayer.remote.is_none() =>
            {
                // Single-player: Tab steps first → chase → orbit → first.
                // (In multiplayer Tab opens the roster, below.)
                use crate::camera::CameraMode;
                match self.camera.mode {
                    CameraMode::First | CameraMode::Third => {
                        if self.camera.mode == CameraMode::First {
                            // Enter the orbit view behind where the player is
                            // aiming, at a slight three-quarter angle.
                            self.camera.orbit_yaw = self.camera.yaw + std::f32::consts::PI + 0.4;
                        }
                        self.camera.mode = match self.camera.mode {
                            CameraMode::First => CameraMode::Third,
                            _ => CameraMode::Orbit,
                        };
                    }
                    CameraMode::Orbit => self.camera.mode = CameraMode::First,
                }
                if let Err(error) = self.server.world.set_camera(self.camera.mode.key()) {
                    eprintln!("camera: could not persist mode: {error}");
                }
                self.sfx(Sfx::Click);
            }
            KeyCode::Tab => {
                self.multiplayer.roster_open = pressed
                    && self.ui_state.screen == Screen::Playing
                    && (self.multiplayer.host.is_some() || self.multiplayer.remote.is_some());
            }
            KeyCode::F5 if pressed => {
                self.reload_mods(true);
            }
            KeyCode::F2 if pressed => {
                let name = format!(
                    "screenshot-{}.ppm",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0)
                );
                self.renderer.pending_screenshot = Some(name);
            }
            KeyCode::F11 if pressed => {
                let fs = self.window.fullscreen();
                self.window.set_fullscreen(if fs.is_some() {
                    None
                } else {
                    Some(Fullscreen::Borderless(None))
                });
            }
            _ => {
                if pressed && self.ui_state.screen == Screen::Playing {
                    let digit = match code {
                        KeyCode::Digit1 => Some(0),
                        KeyCode::Digit2 => Some(1),
                        KeyCode::Digit3 => Some(2),
                        KeyCode::Digit4 => Some(3),
                        KeyCode::Digit5 => Some(4),
                        KeyCode::Digit6 => Some(5),
                        KeyCode::Digit7 => Some(6),
                        KeyCode::Digit8 => Some(7),
                        KeyCode::Digit9 => Some(8),
                        _ => None,
                    };
                    if let Some(d) = digit {
                        if self.input.hotbar_sel != d {
                            self.presentation.sel_bounce = 0.0;
                        }
                        self.input.hotbar_sel = d;
                    }
                }
            }
        }
    }
}
