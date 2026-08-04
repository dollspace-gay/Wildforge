//! Survival ticking, item pickup, and player-facing status messages.

use super::*;

pub(super) fn dross_warning_text(band: u8) -> (&'static str, &'static str) {
    match band {
        1 => ("TRACE", "GLASS HAZE"),
        2 => ("STRAINED", "TWO-PULSE HUM"),
        3 => ("SEEP", "BRANCHING SIGN"),
        4 => ("SCAR", "BROKEN RING"),
        5 => ("BREACH RISK", "REPEATING SHEAR"),
        _ => ("CLEAR", "EVEN FIELD"),
    }
}

impl Game {
    pub(super) fn present_dross_cue(&mut self, cue: crate::dross::DrossCue) {
        match cue.kind {
            crate::dross::DrossCueKind::BreachForecast => self.sfx(Sfx::DrossWarning(5)),
            crate::dross::DrossCueKind::Breach { activity } => self.sfx(Sfx::DrossBreach(
                activity.unwrap_or(crate::dross::ScarActivityHandler::Shear),
            )),
        }
        self.toast(cue.accessible_text().to_string());
    }

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
        // The hunger charm prepays one fixed five-second interval. If the
        // debit fails or the charm depletes, no fraction of the benefit is
        // applied and ordinary starvation resumes immediately.
        if self.survival.hunger_charm_credit <= 0.0
            && let Some(mut charm) = self.survival.armor[4]
            && self.content.reg.item(charm.item).charm.as_deref() == Some("hunger")
            && let Some(pos) = self.player.pos.block()
            && self.server.world.debit_charm_at(
                pos,
                &mut charm,
                "hunger",
                "slow-hunger charm prepaid an active interval",
            )
        {
            self.survival.armor[4] = Some(charm);
            self.survival.hunger_charm_credit = crate::implements::HUNGER_CHARM_INTERVAL_SECS;
        }
        let charm_mult = if self.survival.hunger_charm_credit > 0.0 {
            self.survival.hunger_charm_credit = (self.survival.hunger_charm_credit - dt).max(0.0);
            crate::implements::HUNGER_CHARM_MULTIPLIER
        } else {
            1.0
        };
        let mut drain = 0.01 * charm_mult;
        if input.sprint && (input.forward != 0.0 || input.strafe != 0.0) {
            drain += 0.02;
        }
        self.survival.hunger = (self.survival.hunger - drain * dt).max(0.0);
        // Food carried in the pack ages (economy plan, leg 3): every
        // 20 s each perishable stack loses that much freshness — no
        // cellar in a backpack — rotting to mush at zero. A legacy
        // stack from before freshness initializes instead of rotting.
        const SWEEP: f32 = 20.0;
        let step = (SWEEP * world::FRESHNESS_PER_SEC) as u32;
        self.survival.perish_accum += dt;
        if self.survival.perish_accum >= SWEEP {
            self.survival.perish_accum -= SWEEP;
            if self.multiplayer.remote.is_none() {
                let reg = self.content.reg.clone();
                let mush = reg.item_id("base:spoiled_mush");
                let mut consumed = Vec::new();
                let actor = crate::identity::local_player_id(
                    &self.server.world.save_dir_for_saving(),
                    self.identity.device_id(),
                )
                .unwrap_or(crate::identity::PlayerId([0; 16]))
                .0;
                let pack_temperature_millic = (self
                    .server
                    .world
                    .weather_at_surface(self.player.pos.surface())
                    .temperature_c
                    * 1_000.0)
                    .round()
                    .clamp(i32::MIN as f32, i32::MAX as f32)
                    as i32;
                let sweep_ticks = (SWEEP * 20.0).round() as u64;
                let mut age = |slot: Option<usize>, s: &mut Option<ItemStack>| {
                    let Some(st) = s else { return };
                    if st.arcane_id != 0 {
                        let holdfast_step = slot.map_or(step, |slot| {
                            self.server.world.holdfast_age_step(
                                actor,
                                slot,
                                *st,
                                step,
                                SWEEP as u32,
                            )
                        });
                        let ordinary_age_ticks = sweep_ticks
                            .saturating_mul(u64::from(holdfast_step))
                            .div_ceil(u64::from(step.max(1)));
                        match self.server.world.age_preparation_storage(
                            *st,
                            pack_temperature_millic,
                            ordinary_age_ticks,
                        ) {
                            Ok(Some(_)) => return,
                            Ok(None) => {}
                            Err(error) => {
                                eprintln!("alchemy: carried storage aging failed: {error}");
                                return;
                            }
                        }
                    }
                    let full = reg.item(st.item).durability;
                    let food = reg.item(st.item).food.is_some();
                    let viable_seed = reg.item(st.item).name.ends_with("_seed");
                    if (!food && !viable_seed) || full == 0 {
                        return;
                    }
                    if st.durability == 0 {
                        st.durability = full;
                    } else {
                        let holdfast_step = slot.map_or(step, |slot| {
                            self.server.world.holdfast_age_step(
                                actor,
                                slot,
                                *st,
                                step,
                                SWEEP as u32,
                            )
                        });
                        let actual_step = if st.arcane_id == 0 {
                            holdfast_step
                        } else {
                            self.server.world.coated_specimen_age_advance(
                                st.arcane_id,
                                u64::from(holdfast_step),
                                pack_temperature_millic,
                            ) as u32
                        };
                        if st.durability > actual_step {
                            st.durability -= actual_step;
                            return;
                        }
                        if food {
                            consumed.push(*st);
                            *s = mush.map(|m| {
                                let mut sp = ItemStack::new(&reg, m, 1);
                                sp.count = st.count;
                                sp
                            });
                        } else {
                            st.durability = 0;
                        }
                    }
                };
                for (slot, s) in self.inventory.slots.iter_mut().enumerate() {
                    age(Some(slot), s);
                }
                age(None, &mut self.ui_state.held_stack);
                if let Some(at) = self.player.pos.block() {
                    for slot in 0..self.inventory.slots.len() {
                        if let Some(stack) = self.inventory.slots[slot]
                            && let Err(error) = self.server.world.leak_fragile_item_charge(
                                actor,
                                slot,
                                stack,
                                at,
                                SWEEP as u32,
                            )
                        {
                            eprintln!("arcane specimen leakage failed: {error}");
                        }
                    }
                }
                if let Err(error) = self.server.world.record_consumed_stacks(consumed) {
                    eprintln!("materials: spoiled carried food accounting failed: {error}");
                }
            }
        }
        // Nutrition decays slowly (~full to empty over long play).
        for n in self.survival.nutrition.iter_mut() {
            *n = (*n - dt * 0.01).max(0.0);
        }
        let maxh = self.max_health();
        self.survival.health = self.survival.health.min(maxh);
        if self.multiplayer.remote.is_none() {
            self.survival.alchemy_accum += dt;
            if self.survival.alchemy_accum >= 1.0 {
                self.survival.alchemy_accum %= 1.0;
                if let Some(actor_pos) = self.player.pos.block() {
                    let actor = crate::identity::local_player_id(
                        &self.server.world.save_dir_for_saving(),
                        self.identity.device_id(),
                    )
                    .unwrap_or(crate::identity::PlayerId([0; 16]));
                    let physiology = crate::alchemy::PreparationPhysiology {
                        health: self.survival.health,
                        max_health: maxh,
                        hunger: self.survival.hunger,
                        nutrition: self.survival.nutrition,
                        strain: 0.0,
                        bodily_dross: self.survival.bodily_dross,
                    };
                    match self
                        .server
                        .world
                        .tick_preparation_statuses(actor.0, actor_pos, physiology)
                    {
                        Ok(result) => {
                            let old_dross_band = self.survival.preparation_modifiers.dross_band;
                            self.survival.health = result.physiology.health;
                            self.survival.hunger = result.physiology.hunger;
                            self.survival.nutrition = result.physiology.nutrition;
                            self.survival.bodily_dross = result.physiology.bodily_dross;
                            self.survival.preparation_modifiers = result.modifiers;
                            if result.modifiers.dross_band > old_dross_band
                                && result.modifiers.dross_band != 0
                            {
                                self.sfx(Sfx::DrossWarning(result.modifiers.dross_band));
                                let (band, pattern) =
                                    dross_warning_text(result.modifiers.dross_band);
                                self.toast(format!("DROSS {band} — {pattern}"));
                            }
                            for cue in result.cues {
                                if let Some(session) = &self.multiplayer.host {
                                    session.broadcast_alchemy_cue(cue.clone());
                                }
                                self.present_alchemy_cue(cue);
                            }
                        }
                        Err(error) => eprintln!("alchemy status update failed: {error}"),
                    }
                }
            }
        }
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
            self.survival.exhaustion_regen +=
                dt * f32::from(self.survival.preparation_modifiers.recovery_permille) / 1_000.0;
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
                    let consumed = self.inventory.slots[self.input.hotbar_sel]
                        .map(|stack| ItemStack::new(&self.content.reg, stack.item, 1));
                    if self.multiplayer.remote.is_none()
                        && let Some(stack) = consumed
                        && let Err(error) = self.server.world.record_consumed_stacks([stack])
                    {
                        eprintln!("materials: eaten food accounting failed: {error}");
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
                    let under = self
                        .player
                        .pos
                        .translated(Vec3::new(0.0, -0.6, 0.0))
                        .ok()
                        .and_then(|canonical| canonical.pos.block())
                        .map(|block| self.server.world.get_block_at(block))
                        .unwrap_or(crate::registry::AIR);
                    let tile = self.content.reg.block(under).tiles[2];
                    self.juice_puff(self.player.pos.render_pos(), tile, 5);
                    if fall > 3.0 {
                        self.sfx(Sfx::Thud);
                    } else {
                        let m = self.step_mat_at(self.player.pos);
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

        // Lava burns fast — you can struggle (fluids are swimmable),
        // but every half-second in the fire costs dearly.
        let feet = self
            .player
            .pos
            .translated(Vec3::new(0.0, 0.4, 0.0))
            .ok()
            .and_then(|canonical| canonical.pos.block())
            .map(|block| self.server.world.get_block_at(block))
            .unwrap_or(crate::registry::AIR);
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

    pub(super) fn update_items(&mut self, _dt: f32) {
        // Physics, lifetime, collision, and loss accounting are host-owned.
        // A guest only renders snapshots and receives authoritative Give
        // messages; it never predicts an inventory pickup.
        if self.multiplayer.remote.is_some() || self.ui_state.screen == Screen::Dead {
            return;
        }
        let mut items = self.server.world.take_loose_items();
        // Pickup: magnetize into the inventory.
        let target = self
            .player
            .pos
            .translated(Vec3::new(0.0, 0.9, 0.0))
            .expect("pickup target stays beside the player")
            .pos;
        let mut i = 0;
        while i < items.len() {
            let it = &items[i];
            let d = it.pos.distance_to(target);
            if it.age > entity::PICKUP_DELAY && d < 1.4 {
                let it_pos = it.pos;
                let (item, count, dur) = (items[i].item, items[i].count, items[i].durability);
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
                        let clip = self.camera.view_proj() * it_pos.render_pos().extend(1.0);
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
                    items.swap_remove(i);
                    continue;
                } else {
                    items[i].count = left;
                }
            }
            i += 1;
        }
        self.server.world.replace_loose_items(items);
    }
}
