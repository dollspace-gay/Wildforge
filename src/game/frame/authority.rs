//! Authority in the graphical frame pipeline.

use super::local_sim_should_advance;
use crate::atlas;
use crate::audio::Sfx;
use crate::entity::ItemEntity;
use crate::game::Game;
use crate::game::navigation::Screen;
use crate::mp;
use crate::server;
use crate::world;
use glam::Vec3;

impl Game {
    pub(in crate::game) fn advance_session_authority(&mut self, dt: f32, paused: bool) {
        let t0 = std::time::Instant::now();
        self.advance_session_authority_inner(dt, paused);
        let ms = t0.elapsed().as_secs_f32() * 1000.0;
        self.frame_ms.0 = self.frame_ms.0 * 0.95 + ms * 0.05;
    }

    pub(in crate::game) fn advance_session_authority_inner(&mut self, dt: f32, paused: bool) {
        // Saving occurs at window/title exit, sleep, and dirty-chunk unload.
        // A periodic sweep scales with residency and stalls the simulation;
        // an unclean exit can lose changes since those explicit save points.
        if !self.in_world && self.multiplayer.remote.is_some() {
            self.remote_pump(dt);
        }
        if self.in_world {
            let profile_frame = self.total_frames.is_multiple_of(60)
                && std::env::var_os("WILDFORGE_PROFILE").is_some();
            let stream_started = std::time::Instant::now();
            self.stream_chunks();
            if profile_frame {
                eprintln!(
                    "profile: stream {:.2}ms, resident {}, dirty {}",
                    stream_started.elapsed().as_secs_f64() * 1_000.0,
                    self.runtime.view().chunk_count(),
                    self.runtime.view().dirty_chunks().len(),
                );
            }
            // The authoritative simulation steps at its fixed tick; the
            // client applies the results as presentation.
            if self.multiplayer.remote.is_some() {
                self.remote_pump(dt);
            } else if local_sim_should_advance(paused, self.multiplayer.host.is_some()) {
                let ctx = server::PlayerCtx {
                    id: 0,
                    pos: self.player.pos,
                    spawn: self.survival.spawn_point,
                    attackable: self.survival.attackable(self.creative),
                    aggro_mod: if self.charm("quiet") {
                        -crate::implements::QUIET_CHARM_AGGRO_REDUCTION
                    } else {
                        0.0
                    },
                    quiet_charm: self.survival.armor[4]
                        .filter(|stack| self.runtime.view().charm_can_pay(*stack, "quiet")),
                };
                // Hosting: guests are simulated players too, and their
                // requests apply before the tick.
                let players = if let Some(mut sess) = self.multiplayer.host.take() {
                    self.runtime.local_mut().world.set_edit_logging(true);
                    let held = self.inventory.slots[self.input.hotbar_sel];
                    let fx = sess.pump_with_host_stack(
                        self.runtime.local_mut(),
                        Some((
                            self.player.pos,
                            self.camera.yaw,
                            self.multiplayer.host_sleeping,
                            held,
                            self.style.pack(),
                        )),
                        dt,
                    );
                    for f in fx {
                        match f {
                            mp::HostFx::Chat { from, msg } => {
                                self.toast(format!("{from}: {msg}"));
                            }
                            mp::HostFx::Joined(n) => self.toast(format!("{n} joined.")),
                            mp::HostFx::Left(n) => self.toast(format!("{n} left.")),
                            mp::HostFx::ImplementActivation { pos, cue, visual } => {
                                self.present_implement_activation(pos, cue, visual, None);
                            }
                            mp::HostFx::WorkingEvent(cue) => self.present_working_cue(cue),
                            mp::HostFx::AlchemyEvent(cue) => self.present_alchemy_cue(cue),
                            mp::HostFx::AllSlept => {
                                self.multiplayer.host_sleeping = false;
                                self.survival.spawn_point = self.player.pos;
                                self.toast("Dawn. The camp wakes.".to_string());
                            }
                            mp::HostFx::ScreenClick { screen, action } => {
                                // Capability E11: a guest's mod-screen
                                // button. Ids were validated host-side;
                                // dispatch the mod's hook here where the
                                // scripts live.
                                if self.content.scripts.wants("on_screen_click") {
                                    self.content.scripts.dispatch_view(
                                        &self.runtime.view(),
                                        "on_screen_click",
                                        (screen, action),
                                    );
                                    self.apply_script_cmds();
                                }
                            }
                        }
                    }
                    let players =
                        sess.authoritative_player_ctxs(&self.runtime.local().world, Some(ctx));
                    self.multiplayer.host = Some(sess);
                    players
                } else {
                    vec![ctx]
                };
                let mut evs = Vec::new();
                let server_started = std::time::Instant::now();
                self.runtime.local_mut().advance(dt, &players, &mut evs);
                if profile_frame {
                    eprintln!(
                        "profile: server {:.2}ms, mobs {}, projectiles {}",
                        server_started.elapsed().as_secs_f64() * 1_000.0,
                        self.runtime.view().mobs().len(),
                        self.runtime.view().projectiles().len(),
                    );
                }
                for ev in evs {
                    match ev {
                        server::SimEvent::PlayerHit {
                            who,
                            dmg,
                            dmg_type,
                            attack,
                            from,
                        } => {
                            if who == 0 && self.multiplayer.remote.is_none() {
                                if self.content.scripts.wants("on_attack") {
                                    self.content.scripts.dispatch_view(
                                        &self.runtime.view(),
                                        "on_attack",
                                        (
                                            attack.clone(),
                                            dmg as f64,
                                            dmg_type.clone().unwrap_or_default(),
                                        ),
                                    );
                                    self.apply_script_cmds();
                                }
                                self.hurt_player_from_wild(dmg, from, dmg_type.as_deref());
                            } else if let Some(sess) = &mut self.multiplayer.host {
                                // `who` is that guest's own net id.
                                sess.hurt_guest(self.runtime.local_mut(), who, dmg, from);
                            }
                        }
                        server::SimEvent::BoltCast => self.sfx(Sfx::Bolt(1.2)),
                        server::SimEvent::Lightning(at) => {
                            // A LANDED bolt: a longer flash, thunder
                            // timed by distance, and a white column
                            // standing on the strike for a beat.
                            self.presentation.lightning = 0.3;
                            let at = at.render_pos();
                            let dist = (at - self.camera.pos).length();
                            self.presentation.thunder_delay = (dist / 110.0).clamp(0.1, 2.0);
                            let white = *atlas::builtin_slots().get("snow").unwrap_or(&39);
                            for dy in 0..26 {
                                self.presentation.burst(
                                    at + Vec3::new(0.0, dy as f32 * 1.1, 0.0),
                                    white,
                                    2,
                                    0.5,
                                );
                            }
                        }
                        server::SimEvent::Bred => {
                            self.sfx(Sfx::Pickup);
                            self.toast("New life stirs in the wild.".to_string());
                        }
                        server::SimEvent::QuietSheltered { who } => {
                            if who == 0 {
                                self.sfx(Sfx::Click);
                            }
                        }
                        server::SimEvent::MobDied(death) => {
                            self.present_settled_mob_death(death);
                            self.grant_xp("kill");
                        }
                        server::SimEvent::Dawn { offering_refund } => {
                            if offering_refund > 0.0 {
                                self.sfx(Sfx::Pickup);
                                self.toast("The wild has accepted your offering.".to_string());
                            }
                        }
                        server::SimEvent::LongWinter(fell) => {
                            self.toast(if fell {
                                "The year has stopped turning. Spring does not come.".to_string()
                            } else {
                                "The year turns again.".to_string()
                            });
                        }
                        server::SimEvent::IreTier { rose, tier } => {
                            let name = world::IRE_TIERS[tier.min(world::IRE_TIERS.len() - 1)];
                            self.toast(if rose {
                                format!("The wild stirs against you - {name}.")
                            } else {
                                format!("The wild settles - {name}.")
                            });
                        }
                        server::SimEvent::Working(result, cue) => {
                            if let Some(session) = &self.multiplayer.host {
                                session.broadcast_working_cue(cue.clone());
                            }
                            self.present_working_cue(cue);
                            self.toast(result.message);
                        }
                        server::SimEvent::Alchemy(cue) => {
                            if let Some(session) = &self.multiplayer.host {
                                session.broadcast_alchemy_cue(cue.clone());
                            }
                            self.present_alchemy_cue(cue);
                        }
                        server::SimEvent::Dross(cue) => {
                            if let Some(session) = &self.multiplayer.host {
                                session.broadcast_dross_cue(&self.runtime.local().world, cue);
                            }
                            let local_region = self
                                .runtime
                                .view()
                                .planet_atlas()
                                .map(|atlas| atlas.atlas_pos(self.player.pos.surface()));
                            if local_region == Some(cue.region) {
                                self.present_dross_cue(cue);
                            }
                        }
                    }
                }
                for (pos, s) in self.runtime.local_mut().world.take_pending_drops() {
                    let center = pos.entity_center();
                    let a = self.rand01() * std::f32::consts::TAU;
                    let v = Vec3::new(a.cos() * 1.5, 2.5, a.sin() * 1.5);
                    let mut entity = ItemEntity::new(center, v, s.item, s.count);
                    entity.durability = s.durability;
                    entity.arcane_id = s.arcane_id;
                    self.runtime.local_mut().world.spawn_loose_item(entity);
                }
                // The wild's whispers reach the ear as toasts.
                for line in std::mem::take(&mut self.runtime.local_mut().world.whispers) {
                    self.toast(line);
                }
                // Crossing into marked country: one line per region
                // per session, hostile or blessed.
                let surface = self.player.pos.surface();
                let cell = world::RegionCell::from_surface(surface);
                if self.presentation.last_ire_cell != Some(cell) {
                    self.presentation.last_ire_cell = Some(cell);
                    let standing = self.runtime.view().regional_ire_at_surface(surface);
                    if standing.abs() >= 8.0 && self.presentation.whispered_cells.insert(cell) {
                        self.toast(
                            if standing > 0.0 {
                                "The trees here remember the axe."
                            } else {
                                "This ground knows you."
                            }
                            .to_string(),
                        );
                    }
                }
                // Before a tuning lens exists, magical geography is learned
                // through signs rather than a debug number or raw atlas map.
                // Guests use only the host's coarse local bands; solo/host
                // players may ask their authoritative atlas for the same
                // unaided qualitative vocabulary.
                let arcane_sign = if self.multiplayer.remote.is_some() {
                    Some(crate::arcane_geography::coarse_sensory_cue(
                        self.runtime.view().remote_arcane_cue(),
                        self.runtime.view().remote_arcane_dominant(),
                    ))
                } else {
                    self.runtime.view().planet_atlas().and_then(|atlas| {
                        self.runtime
                            .local()
                            .world
                            .arcane_survey_at(atlas.atlas_pos(surface), false)
                            .map(|survey| survey.sensory_cue())
                    })
                };
                if let Some(sign) = arcane_sign
                    && self.presentation.arcane_signs.insert(sign.clone())
                {
                    self.toast(sign);
                }
                if let Some(observation) = self
                    .runtime
                    .view()
                    .perceived_arcane_ecology_at(surface, self.scan_range())
                    && self
                        .presentation
                        .arcane_signs
                        .insert(observation.text.clone())
                {
                    self.toast(observation.text);
                }
                // Close container screens if their block vanished.
                if let Screen::Furnace(pos)
                | Screen::Chest(pos)
                | Screen::Offering(pos)
                | Screen::Bloomery(pos) = self.ui_state.screen
                    && self.runtime.view().block_entity_at(&pos).is_none()
                {
                    self.set_screen(Screen::Playing);
                }
                // Mod tick at 10 Hz.
                if self.content.scripts.wants("on_tick") {
                    self.multiplayer.tick_accum += dt;
                    if self.multiplayer.tick_accum >= 0.1 {
                        let t = self.multiplayer.tick_accum;
                        self.multiplayer.tick_accum = 0.0;
                        self.content.scripts.dispatch_view(
                            &self.runtime.view(),
                            "on_tick",
                            (t as f64,),
                        );
                        self.apply_script_cmds();
                    }
                }
            }
        }
    }
}
