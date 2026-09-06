//! Survival graphical status adapter.

use crate::audio::Sfx;
use glam::Vec3;
use crate::game::Game;
use crate::game::MAX_AIR;

impl Game {
    pub(in crate::game) fn update_survival(&mut self, dt: f32) {
        let ruleset = self.runtime.view().ruleset();
        // Fall damage: measure from the apex of the fall.
        if self.player.in_water || self.player.on_ground {
            if let (Some(start), true) = (self.survival.fall_start, self.player.on_ground) {
                let fall = start - self.player.pos.y;
                if fall >= 2.0 && self.presentation.juice {
                    let under = self
                        .player
                        .pos
                        .translated(Vec3::new(0.0, -0.6, 0.0))
                        .ok()
                        .and_then(|canonical| canonical.pos.block())
                        .map(|block| self.runtime.view().get_block_at(block))
                        .unwrap_or(crate::registry::AIR);
                    let tile = self.content.reg.block(under).tiles[2];
                    self.presentation.puff(self.player.pos.render_pos(), tile, 5);
                    if fall > 3.0 {
                        self.sfx(Sfx::Thud);
                    } else {
                        let m = self.step_mat_at(self.player.pos);
                        let p = self.presentation.vary() * 0.8;
                        self.sfx(Sfx::Step(m, p));
                    }
                }
                if ruleset.fall_damage {
                    self.damage((fall - 3.0).floor());
                }
            }
            self.survival.fall_start = None;
        } else if self.player.vel.y < 0.0 {
            self.survival.fall_start = Some(
                self.survival
                    .fall_start
                    .unwrap_or(self.player.pos.y)
                    .max(self.player.pos.y),
            );
        } else {
            self.survival.fall_start = None;
        }

        // Lava burns fast — you can struggle (fluids are swimmable),
        // but every half-second in the fire costs dearly.
        let feet = self
            .player
            .pos
            .translated(Vec3::new(0.0, 0.4, 0.0))
            .ok()
            .and_then(|canonical| canonical.pos.block())
            .map(|block| self.runtime.view().get_block_at(block))
            .unwrap_or(crate::registry::AIR);
        if self.content.reg.is_lava(feet) {
            self.survival.burn_timer += dt;
            if self.survival.burn_timer >= 0.5 {
                self.survival.burn_timer = 0.0;
                self.survival.killed_by_wild = false;
                if ruleset.lava_burn {
                    self.damage(3.0);
                }
            }
        } else {
            self.survival.burn_timer = 0.0;
        }

        // Drowning.
        if self.player.head_underwater(&self.runtime.view()) {
            self.survival.air -= dt;
            if self.survival.air <= 0.0 {
                self.survival.air = 0.0;
                self.survival.drown_timer += dt;
                if self.survival.drown_timer >= 1.0 {
                    self.survival.drown_timer = 0.0;
                    if ruleset.drowning {
                        self.damage(2.0);
                    }
                }
            }
        } else {
            self.survival.air = (self.survival.air + dt * 4.0).min(MAX_AIR);
            self.survival.drown_timer = 0.0;
        }

        self.survival.since_damage += dt;
        self.survival.damage_flash = (self.survival.damage_flash - dt).max(0.0);
    }
}
