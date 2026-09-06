//! Feedback in the graphical frame pipeline.

use crate::world::TerrainRead;
use crate::audio;
use crate::audio::Sfx;
use glam::Vec3;
use crate::game::Game;
use crate::game::navigation::Screen;

impl Game {

    pub(in crate::game) fn advance_feedback(&mut self, dt: f32, paused: bool) {
        // The juice layer's clock: particles, pulses, streaks, motion.
        self.presentation.pool.tick(dt);
        // Ambience nobody profits from: a fluttering speck near the
        // canopy by day, a dragonfly skimming the water. Client-side
        // only — some of the world is just living here.
        self.presentation.ambient_timer -= dt;
        if !paused && self.presentation.ambient_timer <= 0.0 && self.presentation.juice {
            self.presentation.ambient_timer = 1.6 + self.presentation.vary() * 2.4;
            let day = self.runtime.view().daylight_at_surface(self.player.pos.surface())
                > 0.5;
            let r1 = self.presentation.vary();
            let r2 = self.presentation.vary();
            let r3 = self.presentation.vary();
            let Ok(sample) = self.player.eye().translated(Vec3::new(
                (r1 - 0.5) * 24.0,
                (r2 - 0.3) * 8.0,
                (r3 - 0.5) * 24.0,
            )) else {
                return;
            };
            let sample = sample.pos;
            let p = sample.render_pos();
            let Some(center) = sample.block() else {
                return;
            };
            let reg = self.content.reg.clone();
            let near = |pred: &dyn Fn(&str) -> bool| -> bool {
                (-2..=2i32).any(|dx| {
                    (-2..=2i32).any(|dy| {
                        (-2..=2i32).any(|dz| {
                            center.offset(dx, dy, dz).is_some_and(|at| {
                                pred(&reg.block(self.runtime.view().get_block_at(at)).name)
                            })
                        })
                    })
                })
            };
            if day && near(&|n: &str| n.contains("leaves")) {
                // A songbird-or-butterfly speck breaking from the canopy.
                if let Some(b) = reg.block_id("base:berry_bush") {
                    self.presentation.puff(p, reg.block(b).tiles[0], 1);
                }
            } else if day && near(&|n: &str| n.contains("water")) {
                // A dragonfly working the surface.
                if let Some(b) = reg.block_id("base:kelp_frond") {
                    self.presentation.puff(p, reg.block(b).tiles[0], 1);
                }
            }
        }
        self.presentation.screen_age = (self.presentation.screen_age + dt / 0.14).min(1.0);
        self.presentation.sel_bounce = (self.presentation.sel_bounce + dt / 0.12).min(1.0);
        self.presentation.press_dip = (self.presentation.press_dip - dt).max(0.0);
        self.presentation.nudge.1 = (self.presentation.nudge.1 - dt).max(0.0);
        for p in self.presentation.slot_pulse.iter_mut() {
            *p = (*p - dt).max(0.0);
        }
        self.presentation.pickup_streak.1 = (self.presentation.pickup_streak.1 - dt).max(0.0);
        if self.presentation.pickup_streak.1 <= 0.0 {
            self.presentation.pickup_streak.0 = 0;
        }
        for f in self.presentation.ui_flies.iter_mut() {
            f.3 += dt;
        }
        self.presentation.ui_flies.retain(|f| f.3 < 0.22);
        // A hostile you haven't met yet announces itself now and then:
        // hearing the threat before seeing it is the point.
        self.presentation.presence_timer -= dt;
        if self.presentation.presence_timer <= 0.0
            && self.in_world
            && self.presentation.juice
            && self.ui_state.screen == Screen::Playing
        {
            self.presentation.presence_timer = 6.0 + (self.presentation.vary() - 0.9) * 20.0;
            let reg = self.content.reg.clone();
            let lurker = self.runtime.view().mobs()
                .iter()
                .filter(|m| {
                    reg.animals[m.species].hostile && m.state != crate::mobs::MobState::Hunt
                })
                .map(|m| ((m.pos - self.player.pos).length(), m.species))
                .filter(|(d, _)| *d < 20.0)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((d, sp)) = lurker {
                let vol = (1.0 - d / 24.0).max(0.2);
                self.sfx_vol(Sfx::Presence(reg.animals[sp].sound_pitch), vol);
            }
        }
        // The stomach speaks before the bar empties.
        self.presentation.hunger_timer -= dt;
        if self.presentation.hunger_timer <= 0.0 {
            let gap = if self.survival.hunger < 2.0 {
                10.0
            } else {
                20.0
            };
            self.presentation.hunger_timer = gap;
            if self.survival.hunger < 5.0
                && self.in_world
                && self.presentation.juice
                && self.ui_state.screen == Screen::Playing
            {
                self.sfx_vol(Sfx::Rumble, 0.7);
            }
        }

        if let Some((at, tile)) = self.presentation.demo_burst
            && self.total_frames.is_multiple_of(10)
        {
            self.presentation.burst(at, tile, 10, 2.2);
            self.survival.damage_flash = 0.4;
        }

        // Footprints in snow: not juice — the trail is world state.
        if self.in_world
            && !paused
            && self.multiplayer.remote.is_none()
            && self.player.on_ground
            && let Some(at) = self.player.pos.block()
        {
            self.runtime.local_mut().world.tread_at(at);
        }

        // Footsteps: mine, my fellow players', and the creatures'.
        if self.in_world && !paused && self.presentation.juice {
            let hv = Vec3::new(self.player.vel.x, 0.0, self.player.vel.z).length();
            if self.player.on_ground && hv > 0.5 {
                self.presentation.step_accum += hv * dt;
                if self.presentation.step_accum >= 2.2 {
                    self.presentation.step_accum = 0.0;
                    let m = self.step_mat_at(self.player.pos);
                    let pitch = self.presentation.vary();
                    self.sfx(Sfx::Step(m, pitch));
                }
            } else if hv <= 0.5 {
                self.presentation.step_accum = 0.0;
            }
            // Mobs step when their stride phase crosses a beat.
            let cam = self.camera.pos;
            let mut steps: Vec<(audio::StepMat, f32, f32)> = Vec::new();
            for m in self.runtime.view().mobs() {
                let d = (m.pos.render_pos() - cam).length();
                if d > 16.0 || m.id == 0 {
                    continue;
                }
                let beat = (m.anim_phase / std::f32::consts::PI).floor();
                let last = self.presentation.mob_strides.insert(m.id, beat);
                if last.is_some_and(|l| beat > l) {
                    let mat = self.step_mat_at(m.pos);
                    let pitch = self.content.reg.animals[m.species].sound_pitch;
                    steps.push((mat, pitch, 1.0 - d / 18.0));
                }
            }
            // Remote players step by distance walked, like we do.
            let remote_players: Vec<(u32, crate::planet::EntityPos)> = self
                .multiplayer
                .remote
                .as_ref()
                .map(|r| {
                    r.player_positions
                        .iter()
                        .map(|(id, pos)| (*id, *pos))
                        .collect()
                })
                .unwrap_or_default();
            for (id, pos) in remote_players {
                let (last, mut accum) = self
                    .presentation
                    .remote_strides
                    .get(&id)
                    .copied()
                    .unwrap_or((pos, 0.0));
                let moved = last.horizontal_distance_to(pos);
                accum += moved;
                if accum >= 2.2 {
                    accum = 0.0;
                    let d = (pos.render_pos() - cam).length();
                    if d < 16.0 {
                        let mat = self.step_mat_at(pos);
                        steps.push((mat, 1.0, 1.0 - d / 18.0));
                    }
                }
                self.presentation.remote_strides.insert(id, (pos, accum));
            }
            for (mat, pitch, vol) in steps {
                let p = pitch * self.presentation.vary();
                self.sfx_vol(Sfx::Step(mat, p), vol * 0.6);
            }
        }
    }
}
