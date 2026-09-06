//! Player motion in the graphical frame pipeline.

use super::movement_axes;
use crate::audio::Sfx;
use crate::game::Game;
use crate::game::combat;
use crate::game::navigation::Screen;
use crate::net;
use crate::physics;
use crate::world::TerrainRead;
use glam::Vec3;

impl Game {
    pub(in crate::game) fn advance_player(&mut self, dt: f32, paused: bool) {
        // Physics — only once the chunk under the player exists.
        let Some(pchunk) = self.player.pos.chunk() else {
            return;
        };
        let can_sim = self.runtime.view().has_chunk(pchunk) && !paused;
        if can_sim && self.ui_state.screen != Screen::Dead {
            self.update_blocking();
            let (forward, strafe) = movement_axes(&self.input.keys);
            let sprinting = self.input.keys.sprint
                && self.survival.hunger >= 6.0
                && self.survival.preparation_modifiers.stamina_permille >= 900
                && (self.creative || self.combat.stamina >= combat::SPRINT_MIN_STAMINA);
            self.stamina_tick(dt, sprinting);
            // Dodge consumes the edge-triggered request at the top of the
            // move step so the burst applies before physics integrates.
            if self.input.dodge_pressed {
                self.input.dodge_pressed = false;
                self.try_dodge();
            }
            let guard = if self.combat.blocking { 0.45 } else { 1.0 };
            let input = physics::Input {
                forward: forward * guard,
                strafe: strafe * guard,
                jump: self.input.keys.space,
                sprint: sprinting,
                speed_mult: self.move_speed(),
            };
            if self.input.keys.space && self.player.on_ground {
                self.survival.hunger = (self.survival.hunger - 0.005).max(0.0);
            }
            // Getting up: any movement withdraws a pending sleep vote.
            if input.forward != 0.0 || input.strafe != 0.0 || input.jump {
                if self.multiplayer.host_sleeping {
                    self.multiplayer.host_sleeping = false;
                    self.toast("You get up.".to_string());
                }
                if self.multiplayer.remote.as_ref().is_some_and(|r| r.sleeping) {
                    let r = self.multiplayer.remote.as_mut().unwrap();
                    r.sleeping = false;
                    r.session.send(&net::C2S::SleepCancel);
                    self.toast("You get up.".to_string());
                }
            }
            self.update_food(dt, &input);
            if self.flying {
                let intent_length = input.forward.hypot(input.strafe).max(1.0);
                let wish = (self.camera.local_flat_forward() * input.forward
                    + self.camera.local_right() * input.strafe)
                    / intent_length;
                let mut v = wish * 9.0;
                if self.input.keys.space {
                    v.y += 8.0;
                }
                if self.input.keys.sprint {
                    v.y -= 8.0;
                }
                self.player.fly(&self.runtime.view(), v, dt);
            }
            let was_in_water = self.player.in_water;
            let fall_speed = self.player.vel.y;
            if !self.flying {
                self.player.update(
                    &self.runtime.view(),
                    &input,
                    self.camera.local_flat_forward(),
                    self.camera.local_right(),
                    dt,
                );
            }
            self.camera.yaw = self.player.frame_rotation.rotate_yaw(self.camera.yaw);
            // Aboard a boat: the hull carries you — float at the
            // surface, glide fast, and the boat glues underneath.
            // Jump steps off.
            if let Some(bid) = self.interaction.riding {
                let gone = self.runtime.view().mob_by_id(bid).is_none();
                if gone || input.jump {
                    if !gone && self.multiplayer.remote.is_some() {
                        if let Some(rc) = &self.multiplayer.remote {
                            rc.session.send(&net::C2S::RideMob {
                                id: bid,
                                mount: false,
                            });
                        }
                    } else if !self.runtime.is_guest()
                        && let Some(m) = self.runtime.local_mut().world.mob_by_id_mut(bid)
                    {
                        m.ridden_by = None;
                    }
                    self.interaction.riding = None;
                    self.player.vel.y = self.player.vel.y.max(4.0);
                } else {
                    if self.player.in_water {
                        // Buoyant hull: ride the surface, shed drag.
                        self.player.vel.y = self.player.vel.y.max(1.2);
                        self.player.vel.x *= 1.9;
                        self.player.vel.z *= 1.9;
                    }
                    let at = self
                        .player
                        .pos
                        .translated(Vec3::new(0.0, -0.35, 0.0))
                        .expect("ridden vehicle stays below its rider")
                        .pos;
                    let yaw = self.camera.yaw;
                    self.runtime.present_ridden_mob(bid, at, yaw);
                }
            }
            if !was_in_water && self.player.in_water && fall_speed < -4.0 {
                self.sfx(Sfx::Splash);
            }
            self.update_survival(dt);
        }
        if can_sim {
            self.update_items(dt);
        }
        self.camera.follow_planet(self.player.eye());
        match self.camera.mode {
            crate::camera::CameraMode::First => {}
            crate::camera::CameraMode::Third => {
                self.camera
                    .place_chase(self.player.eye(), &self.runtime.view());
            }
            crate::camera::CameraMode::Orbit => {
                self.camera.place_orbit(self.player.eye());
            }
        }

        if self.ui_state.screen == Screen::Playing && self.input.captured() {
            self.interact(dt);
        }
    }
}
