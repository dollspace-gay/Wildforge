//! Pump guest state for the authoritative host session.

use super::{HostFx, HostSession, S2C, Server};

impl HostSession {
    pub(super) fn pump_guest_state(&mut self, server: &mut Server, dt: f32, fx: &mut Vec<HostFx>) {
        // Rate-limit windows + movement interpolation clocks.
        let creative = server.world.mode == "creative";
        let mut survival_changed = Vec::new();
        for g in self.guests.values_mut() {
            if !g.entry_ready {
                continue;
            }
            g.edit_window += dt;
            if g.edit_window >= 1.0 {
                g.edit_window = 0.0;
                g.edits = 0;
            }
            g.net_age += dt;
            g.action_cooldown = (g.action_cooldown - dt).max(0.0);
            g.since_damage += dt;
            g.chat_window += dt;
            if g.chat_window >= 10.0 {
                g.chat_window = 0.0;
                g.chat_count = 0;
            }
            g.command_window += dt;
            if g.command_window >= 1.0 {
                g.command_window = 0.0;
                g.command_count = 0;
            }
            g.chunk_window += dt;
            if g.chunk_window >= 1.0 {
                g.chunk_window = 0.0;
                g.chunk_requests = 0;
            }
            if !creative {
                let old = (g.health, g.hunger, g.nutrition);
                if g.hunger_charm_credit <= 0.0
                    && let Some(mut charm) = g.armor[4]
                    && server.world.reg.item(charm.item).charm.as_deref() == Some("hunger")
                    && let Some(pos) = g.pos.block()
                    && server.world.debit_charm_at(
                        pos,
                        &mut charm,
                        "hunger",
                        "guest slow-hunger charm prepaid an active interval",
                    )
                {
                    g.armor[4] = Some(charm);
                    g.hunger_charm_credit = crate::implements::HUNGER_CHARM_INTERVAL_SECS;
                }
                let hunger_charm = g.hunger_charm_credit > 0.0;
                g.hunger_charm_credit = (g.hunger_charm_credit - dt).max(0.0);
                let drain = (0.01 + if g.sprinting { 0.02 } else { 0.0 })
                    * if hunger_charm {
                        crate::implements::HUNGER_CHARM_MULTIPLIER
                    } else {
                        1.0
                    };
                g.hunger = (g.hunger - drain * dt).max(0.0);
                for value in &mut g.nutrition {
                    *value = (*value - dt * 0.01).max(0.0);
                }
                let max_health =
                    14.0 + g.nutrition.iter().filter(|&&value| value >= 40.0).count() as f32 * 2.0;
                g.health = g.health.min(max_health);
                if g.hunger >= 17.0 && g.health < max_health && g.since_damage > 4.0 {
                    g.regen_timer += dt;
                    if g.regen_timer >= 3.0 {
                        g.regen_timer = 0.0;
                        g.health = (g.health + 1.0).min(max_health);
                        g.hunger = (g.hunger - 0.5).max(0.0);
                    }
                }
                if g.hunger <= 0.0 {
                    g.starve_timer += dt;
                    if g.starve_timer >= 4.0 {
                        g.starve_timer = 0.0;
                        if g.health > 2.0 {
                            g.health -= 1.0;
                        }
                    }
                } else {
                    g.starve_timer = 0.0;
                }
                if old != (g.health, g.hunger, g.nutrition) {
                    survival_changed.push(g.player_id);
                }
            }
        }
        if self.state_timer + dt >= 1.0 {
            let mut status_cues = Vec::new();
            for (id, guest) in &mut self.guests {
                if !guest.entry_ready {
                    continue;
                }
                let Some(actor_pos) = guest.pos.block() else {
                    continue;
                };
                let max_health = 14.0
                    + guest
                        .nutrition
                        .iter()
                        .filter(|&&value| value >= 40.0)
                        .count() as f32
                        * 2.0;
                let physiology = crate::alchemy::PreparationPhysiology {
                    health: guest.health,
                    max_health,
                    hunger: guest.hunger,
                    nutrition: guest.nutrition,
                    strain: 0.0,
                    bodily_dross: guest.bodily_dross,
                };
                match server.world.tick_preparation_statuses(
                    guest.player_id.0,
                    actor_pos,
                    physiology,
                ) {
                    Ok(result) => {
                        guest.health = result.physiology.health;
                        guest.hunger = result.physiology.hunger;
                        guest.nutrition = result.physiology.nutrition;
                        guest.bodily_dross = result.physiology.bodily_dross;
                        self.net.send(
                            *id,
                            &S2C::PreparationState {
                                modifiers: result.modifiers,
                                bodily_dross: guest.bodily_dross,
                            },
                        );
                        status_cues.extend(result.cues);
                    }
                    Err(error) => eprintln!(
                        "alchemy: status update for {} failed: {error}",
                        guest.player_id
                    ),
                }
            }
            for cue in status_cues {
                for (observer, guest) in &self.guests {
                    if guest.entry_ready
                        && guest.pos.horizontal_distance_to(cue.pos.entity_center()) <= 96.0
                    {
                        self.net.send(*observer, &S2C::AlchemyEvent(cue.clone()));
                    }
                }
                fx.push(HostFx::AlchemyEvent(cue));
            }
        }
        let out_of_range = self
            .guests
            .iter()
            .filter_map(|(id, guest)| {
                let active = guest.active_working?;
                let source = guest.pos.block()?;
                (!server.world.wand_working_reachable_from(active, source)).then_some((*id, active))
            })
            .collect::<Vec<_>>();
        for (id, active) in out_of_range {
            let mut cue = server
                .world
                .working_cues()
                .into_iter()
                .find(|cue| cue.stable_id == active);
            if let Some(guest) = self.guests.get_mut(&id) {
                guest.active_working = None;
            }
            if let Ok(mut result) = server.world.interrupt_working(active) {
                result.message =
                    "The wand path leaves its bounded reach and breaks cleanly.".into();
                if let Some(cue) = cue.as_mut() {
                    cue.kind = result.cue;
                    cue.warning_band = result.warning_band;
                    cue.completion_permille = 1_000;
                }
                self.net.send(id, &S2C::WorkingResult(result));
                if let Some(cue) = cue {
                    self.broadcast_ready(&S2C::WorkingEvent(cue.clone()));
                    fx.push(HostFx::WorkingEvent(cue));
                }
            }
        }
        if self.state_timer + dt >= 1.0 && !survival_changed.is_empty() {
            let ids: Vec<u32> = self
                .guests
                .iter()
                .filter(|(_, guest)| guest.entry_ready)
                .map(|(id, _)| *id)
                .collect();
            for id in ids {
                self.send_player_state(id);
            }
        }

    }
}
