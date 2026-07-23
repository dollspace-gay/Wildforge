//! Survival ticking, item pickup, and player-facing status messages.

use super::*;

impl Game {
    pub(super) fn toast(&mut self, msg: String) {
        self.presentation.toasts.push((msg, 4.0));
        if self.presentation.toasts.len() > 5 {
            self.presentation.toasts.remove(0);
        }
    }

    pub(super) fn max_health(&self) -> f32 {
        MAX_HEALTH
            + self
                .survival
                .nutrition
                .iter()
                .filter(|&&n| n >= 40.0)
                .count() as f32
                * 2.0
    }

    pub(super) fn update_food(&mut self, dt: f32, input: &physics::Input) {
        if self.creative {
            return;
        }
        // Activity-based hunger drain (the hunger charm slows it).
        let charm_mult = if self.charm("hunger") { 0.85 } else { 1.0 };
        let mut drain = 0.01 * charm_mult;
        if input.sprint && (input.forward != 0.0 || input.strafe != 0.0) {
            drain += 0.02;
        }
        self.survival.hunger = (self.survival.hunger - drain * dt).max(0.0);
        // Nutrition decays slowly (~full to empty over long play).
        for n in self.survival.nutrition.iter_mut() {
            *n = (*n - dt * 0.01).max(0.0);
        }
        let maxh = self.max_health();
        self.survival.health = self.survival.health.min(maxh);
        // Food-gated regen (replaces free idle regen). Raw pitchblende
        // in your pack quietly pauses it — a whisper, not a mechanic;
        // ground powder and fired glass are safe.
        let cursed = self
            .content
            .reg
            .item_id("base:raw_pitchblende")
            .is_some_and(|p| self.inventory.slots.iter().flatten().any(|s| s.item == p));
        if !cursed
            && self.survival.hunger >= 17.0
            && self.survival.health < maxh
            && self.survival.since_damage > 4.0
        {
            self.survival.exhaustion_regen += dt;
            if self.survival.exhaustion_regen >= 3.0 {
                self.survival.exhaustion_regen = 0.0;
                self.survival.health = (self.survival.health + 1.0).min(maxh);
                self.survival.hunger = (self.survival.hunger - 0.5).max(0.0);
            }
        }
        // Starvation weakens to 1 heart, never kills.
        if self.survival.hunger <= 0.0 {
            self.survival.starve_timer += dt;
            if self.survival.starve_timer >= 4.0 {
                self.survival.starve_timer = 0.0;
                if self.survival.health > 2.0 {
                    self.survival.health -= 1.0;
                    self.survival.damage_flash = 0.3;
                    self.sfx(Sfx::Hurt);
                }
            }
        }
        // Eating: hold right-click with food selected.
        let food = self.inventory.slots[self.input.hotbar_sel]
            .and_then(|s| self.content.reg.item(s.item).food.clone());
        if self.input.right_held
            && self.ui_state.screen == Screen::Playing
            && let Some(f) = food
        {
            let want = self.survival.hunger < 19.5
                || f.nutrition
                    .iter()
                    .zip(&self.survival.nutrition)
                    .any(|(a, b)| *a > 0.0 && *b < 99.0);
            if want {
                self.survival.eating += dt;
                if self.survival.eating >= f.eat_time {
                    self.survival.eating = 0.0;
                    if let Some(remote) = &self.multiplayer.remote {
                        remote.client.send(&net::C2S::EatSelected);
                    }
                    self.survival.hunger = (self.survival.hunger + f.hunger).min(20.0);
                    for (n, add) in self.survival.nutrition.iter_mut().zip(&f.nutrition) {
                        *n = (*n + add).min(100.0);
                    }
                    self.inventory.take_one(self.input.hotbar_sel);
                    self.sfx(Sfx::Pickup);
                }
                return;
            }
        }
        self.survival.eating = 0.0;
    }

    pub(super) fn update_survival(&mut self, dt: f32) {
        // Fall damage: measure from the apex of the fall.
        if self.player.in_water || self.player.on_ground {
            if let (Some(start), true) = (self.survival.fall_start, self.player.on_ground) {
                let fall = start - self.player.pos.y;
                if fall >= 2.0 && self.presentation.juice {
                    let under = self.server.world.get_block(
                        self.player.pos.x.floor() as i32,
                        (self.player.pos.y - 0.6).floor() as i32,
                        self.player.pos.z.floor() as i32,
                    );
                    let tile = self.content.reg.block(under).tiles[2];
                    self.juice_puff(self.player.pos, tile, 5);
                    if fall > 3.0 {
                        self.sfx(Sfx::Thud);
                    } else {
                        let m = self.step_mat_at(
                            self.player.pos.x,
                            self.player.pos.y,
                            self.player.pos.z,
                        );
                        let p = self.vary() * 0.8;
                        self.sfx(Sfx::Step(m, p));
                    }
                }
                self.damage((fall - 3.0).floor());
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

        // The void below the world's floor: nothing survives long out
        // there (a backstop — worldroot should make this unreachable).
        if self.player.pos.y < -8.0 && self.survival.since_damage >= 0.4 {
            self.survival.killed_by_wild = false;
            self.damage(4.0);
        }

        // Lava burns fast — you can struggle (fluids are swimmable),
        // but every half-second in the fire costs dearly.
        let feet = self.server.world.get_block(
            self.player.pos.x.floor() as i32,
            (self.player.pos.y + 0.4).floor() as i32,
            self.player.pos.z.floor() as i32,
        );
        if self.content.reg.is_lava(feet) {
            self.survival.burn_timer += dt;
            if self.survival.burn_timer >= 0.5 {
                self.survival.burn_timer = 0.0;
                self.survival.killed_by_wild = false;
                self.damage(3.0);
            }
        } else {
            self.survival.burn_timer = 0.0;
        }

        // Drowning.
        if self.player.head_underwater(&self.server.world) {
            self.survival.air -= dt;
            if self.survival.air <= 0.0 {
                self.survival.air = 0.0;
                self.survival.drown_timer += dt;
                if self.survival.drown_timer >= 1.0 {
                    self.survival.drown_timer = 0.0;
                    self.damage(2.0);
                }
            }
        } else {
            self.survival.air = (self.survival.air + dt * 4.0).min(MAX_AIR);
            self.survival.drown_timer = 0.0;
        }

        self.survival.since_damage += dt;
        self.survival.damage_flash = (self.survival.damage_flash - dt).max(0.0);
    }

    pub(super) fn update_items(&mut self, dt: f32) {
        let world = &self.server.world;
        self.interaction.items.retain_mut(|it| it.update(world, dt));
        if self.ui_state.screen == Screen::Dead {
            return;
        }
        // Pickup: magnetize into the inventory.
        let target = self.player.pos + Vec3::new(0.0, 0.9, 0.0);
        let mut i = 0;
        while i < self.interaction.items.len() {
            let it = &self.interaction.items[i];
            let d = it.pos.distance(target);
            if it.age > entity::PICKUP_DELAY && d < 1.4 {
                let it_pos = it.pos;
                let (item, count, dur) = (
                    self.interaction.items[i].item,
                    self.interaction.items[i].count,
                    self.interaction.items[i].durability,
                );
                let reg = self.content.reg.clone();
                let left = if dur > 0 {
                    let mut stack = ItemStack::new(&reg, item, count);
                    stack.durability = dur;
                    self.inventory.add_stack(&reg, stack)
                } else {
                    self.inventory.add(&reg, item, count)
                };
                if left < count {
                    if !self.presentation.juice {
                        self.sfx(Sfx::Pickup);
                    } else {
                        // The collection ramp: each quick pickup chimes
                        // a step higher; the gap resets the melody.
                        self.presentation.pickup_streak.0 =
                            (self.presentation.pickup_streak.0 + 1).min(24);
                        self.presentation.pickup_streak.1 = 1.5;
                        let pitch = audio::pickup_pitch(self.presentation.pickup_streak.0 - 1);
                        self.sfx(Sfx::Pickup2(pitch));
                    }
                    if self.presentation.juice
                        && let Some(slot) = self
                            .inventory
                            .slots
                            .iter()
                            .position(|s| s.is_some_and(|s| s.item == item))
                        && slot < HOTBAR_SLOTS
                    {
                        // A ghost of the icon flies to its new home.
                        let clip = self.camera.view_proj() * it_pos.extend(1.0);
                        if clip.w > 0.3 {
                            let w = self.renderer.config.width as f32;
                            let h = self.renderer.config.height as f32;
                            let sx = (clip.x / clip.w * 0.5 + 0.5) * w;
                            let sy = (0.5 - clip.y / clip.w * 0.5) * h;
                            let icon = self.content.reg.item(item).icon;
                            self.presentation.ui_flies.push((icon, (sx, sy), slot, 0.0));
                        }
                        self.presentation.slot_pulse[slot] = 0.18;
                    }
                }
                if left == 0 {
                    self.interaction.items.swap_remove(i);
                    continue;
                } else {
                    self.interaction.items[i].count = left;
                }
            }
            i += 1;
        }
    }
}
