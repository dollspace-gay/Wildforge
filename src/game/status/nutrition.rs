//! Nutrition graphical status adapter.

use crate::audio::Sfx;
use crate::inventory::ItemStack;
use crate::net;
use crate::physics;
use crate::world;
use crate::game::Game;
use crate::game::navigation::Screen;
use super::dross_warning_text;

impl Game {
    pub(in crate::game) fn update_food(&mut self, dt: f32, input: &physics::Input) {
        if self.creative || !self.runtime.view().ruleset().hunger {
            return;
        }
        // The hunger charm prepays one fixed five-second interval. If the
        // debit fails or the charm depletes, no fraction of the benefit is
        // applied and ordinary starvation resumes immediately.
        if self.survival.hunger_charm_credit <= 0.0
            && !self.runtime.is_guest()
            && let Some(mut charm) = self.survival.armor[4]
            && self.content.reg.item(charm.item).charm.as_deref() == Some("hunger")
            && let Some(pos) = self.player.pos.block()
            && self.runtime.local_mut().world.debit_charm_at(
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
                    &self.runtime.local().world.save_dir_for_saving(),
                    self.identity.device_id(),
                )
                .unwrap_or(crate::identity::PlayerId([0; 16]))
                .0;
                let pack_temperature_millic = (self.runtime.view().weather_at_surface(self.player.pos.surface())
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
                            self.runtime.local_mut().world.holdfast_age_step(
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
                        match self.runtime.local_mut().world.age_preparation_storage(
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
                            self.runtime.local_mut().world.holdfast_age_step(
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
                            self.runtime.local_mut().world.coated_specimen_age_advance(
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
                            && let Err(error) = self.runtime.local_mut().world.leak_fragile_item_charge(
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
                if let Err(error) = self.runtime.local_mut().world.record_consumed_stacks(consumed) {
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
                        &self.runtime.local().world.save_dir_for_saving(),
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
                    match self.runtime.local_mut().world.tick_preparation_statuses(actor.0, actor_pos, physiology)
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
            let want = crate::player_ops::nutrition::wants_food(
                self.survival.hunger, &self.survival.nutrition, &f,
            );
            if want {
                self.survival.eating += dt;
                if self.survival.eating >= f.eat_time {
                    self.survival.eating = 0.0;
                    if let Some(remote) = &self.multiplayer.remote {
                        remote.session.send(&net::C2S::EatSelected);
                    }
                    crate::player_ops::nutrition::eat(
                        &mut self.survival.hunger, &mut self.survival.nutrition, &f,
                    );
                    let consumed = self.inventory.slots[self.input.hotbar_sel]
                        .map(|stack| ItemStack::new(&self.content.reg, stack.item, 1));
                    if self.multiplayer.remote.is_none()
                        && let Some(stack) = consumed
                        && let Err(error) = self.runtime.local_mut().world.record_consumed_stacks([stack])
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
}
