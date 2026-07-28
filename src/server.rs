//! The authoritative simulation: the world, its clock, and everything
//! that happens in it. Singleplayer is a `Server` with one local player;
//! a multiplayer host is the same struct with remote players attached —
//! one simulation path, forever (the thing Minecraft retrofitted).
//!
//! The client side (rendering, input, UI, sounds) drives this with
//! `advance()` and applies the returned `SimEvent`s as presentation.

use glam::Vec3;

use crate::mobs::MobEvent;
use crate::world::{Weather, World};

/// Fixed simulation rate. Rendering runs faster and interpol- er, copes.
pub const TICK: f32 = 1.0 / 30.0;
pub const DAY_LENGTH: f32 = 1200.0; // seconds per full day/night cycle

/// What the simulation needs to know about a player this tick.
#[derive(Clone, Copy)]
pub struct PlayerCtx {
    /// Stable identity for lead-following: 0 = host, guests their net id.
    pub id: u32,
    pub pos: Vec3,
    pub spawn: Vec3,
    /// False in creative or while dead: the wild can't touch you.
    pub attackable: bool,
    /// Charm of quiet: shrinks warden attention.
    pub aggro_mod: f32,
}

/// Things the simulation did that the client must present or apply.
pub enum SimEvent {
    /// The wild connected: which player, damage, attacker position.
    ///
    /// `who` is the stable [`PlayerCtx::id`] (0 = host, otherwise the guest's
    /// net id), not a position in the players slice. It used to be an index,
    /// and every consumer resolved it back to a guest by re-iterating a
    /// HashMap — correct only while nobody joined or left in between, which
    /// nothing enforced.
    PlayerHit { who: u32, dmg: f32, from: Vec3 },
    /// A warden loosed a bolt (sound cue; the projectile is already live).
    BoltCast,
    /// Wildlife bred.
    Bred,
    /// Day rolled over; offerings worth this much were accepted.
    Dawn { offering_refund: f32 },
    /// The wild's ire crossed a tier boundary.
    IreTier { rose: bool, tier: usize },
    /// The sky changed its mind (ambience/visual transitions).
    WeatherChanged(Weather),
    /// The wild's own hand: a bolt landed here.
    Lightning(Vec3),
    /// The year stopped turning, or started again.
    LongWinter(bool),
}

pub struct Server {
    pub world: World,
    pub time_of_day: f32,
    /// Hold `time_of_day` still. Set for headless captures, where the sun
    /// drifting by however long the machine took to reach the capture frame
    /// showed up as a small global brightness difference between runs.
    pub freeze_clock: bool,
    /// Simulation randomness — separate from client/UI randomness.
    pub rng: u32,
    accum: f32,
    water_timer: f32,
    lava_timer: f32,
    fire_timer: f32,
    random_timer: f32,
    snow_timer: f32,
    bolt_timer: f32,
    prev_tier: usize,
}

impl Server {
    pub fn new(world: World, time_of_day: f32, rng: u32) -> Server {
        let prev_tier = world.ire_tier();
        let mut world = world;
        world.clock = Server::clock_of(world.day, time_of_day);
        Server {
            world,
            time_of_day,
            freeze_clock: false,
            rng,
            accum: 0.0,
            water_timer: 0.0,
            lava_timer: 0.0,
            fire_timer: 0.0,
            random_timer: 0.0,
            snow_timer: 0.0,
            bolt_timer: 24.0,
            prev_tier,
        }
    }

    /// Absolute sim-time in seconds: whole days plus the time of day.
    fn clock_of(day: u32, time_of_day: f32) -> f64 {
        (day as f64 + time_of_day.rem_euclid(1.0) as f64) * DAY_LENGTH as f64
    }

    /// Current daylight factor (0.12 night floor .. 1.0 noon).
    pub fn daylight(&self) -> f32 {
        let sun = (self.time_of_day * std::f32::consts::TAU).sin();
        (sun * 2.5 + 0.5).clamp(0.12, 1.0)
    }

    /// Run the simulation forward by wall-clock `dt`, stepping at the
    /// fixed tick. Events accumulate across however many ticks ran.
    pub fn advance(&mut self, dt: f32, players: &[PlayerCtx], events: &mut Vec<SimEvent>) {
        // A hitch (or debugger pause) must not spiral the sim.
        self.accum = (self.accum + dt).min(0.25);
        while self.accum >= TICK {
            self.accum -= TICK;
            self.step(TICK, players, events);
        }
    }

    fn step(&mut self, dt: f32, players: &[PlayerCtx], events: &mut Vec<SimEvent>) {
        // The clock, the wild's ire, and dawn. A frozen clock holds the sun
        // still (headless capture); everything else still ticks.
        if !self.freeze_clock {
            let before = self.time_of_day;
            self.time_of_day = (self.time_of_day + dt / DAY_LENGTH) % 1.0;
            if self.time_of_day < before {
                self.world.day = self.world.day.wrapping_add(1);
            }
            self.world.clock = Server::clock_of(self.world.day, self.time_of_day);
        }
        self.step_weather(dt, events);
        let winter_before = self.world.long_winter;
        if self.world.tick_ire(dt / DAY_LENGTH) {
            let refund = self.world.accept_offerings();
            events.push(SimEvent::Dawn {
                offering_refund: refund,
            });
        }
        if self.world.long_winter != winter_before {
            events.push(SimEvent::LongWinter(self.world.long_winter));
        }
        let tier = self.world.ire_tier();
        if tier != self.prev_tier {
            events.push(SimEvent::IreTier {
                rose: tier > self.prev_tier,
                tier,
            });
            self.prev_tier = tier;
        }

        // Fluids at 5 Hz, like classic water.
        self.water_timer += dt;
        while self.water_timer >= 0.2 {
            self.water_timer -= 0.2;
            self.world.tick_water(512);
        }
        // Lava creeps at a quarter of that pace.
        self.lava_timer += dt;
        while self.lava_timer >= 0.8 {
            self.lava_timer -= 0.8;
            self.world.tick_lava(256);
        }
        // Fire moves faster than either: a burn you can outrun but
        // not ignore.
        self.fire_timer += dt;
        while self.fire_timer >= 0.35 {
            self.fire_timer -= 0.35;
            self.world.tick_fire(256, &mut self.rng);
        }

        // Machines and gravity.
        self.world.tick_entities(dt);
        self.world.tick_falling(dt);

        // Creatures: wildlife, wardens, spawning, projectiles.
        let dl = self.daylight();
        let mut rng = self.rng;
        let mob_events = self.world.tick_mobs(players, dl, dt, &mut rng);
        // Spawning pressure rings a random player each cycle.
        if !players.is_empty() {
            rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            let p = &players[(rng >> 8) as usize % players.len()];
            self.world
                .tick_hostile_spawns(p.pos, p.spawn, dl, dt, &mut rng);
        }
        self.rng = rng;
        for ev in mob_events {
            match ev {
                MobEvent::HitPlayer(who, dmg, from) => {
                    if let Some(p) = players.get(who) {
                        events.push(SimEvent::PlayerHit {
                            who: p.id,
                            dmg,
                            from,
                        });
                    }
                }
                MobEvent::Cast(proj) => {
                    self.world.spawn_projectile(proj);
                    events.push(SimEvent::BoltCast);
                }
                MobEvent::Bred => events.push(SimEvent::Bred),
                // Kills are settled inside tick_mobs; none escape.
                MobEvent::Killed(_) => {}
                MobEvent::Ate(pos) => self.world.apply_bite(pos),
                MobEvent::Dung(at, guano) => {
                    let item = if guano { "base:guano" } else { "base:dung" };
                    if let Some(dung) = self.world.reg.item_id(item) {
                        let reg = self.world.reg.clone();
                        let stack = crate::inventory::ItemStack::new(&reg, dung, 1);
                        self.world.push_drop(
                            (
                                at.x.floor() as i32,
                                at.y.floor() as i32,
                                at.z.floor() as i32,
                            ),
                            stack,
                        );
                    }
                }
                MobEvent::LeadSnapped(at) => {
                    if let Some(lead) = self.world.reg.item_id("base:lead") {
                        let reg = self.world.reg.clone();
                        let stack = crate::inventory::ItemStack::new(&reg, lead, 1);
                        self.world.push_drop(
                            (
                                at.x.floor() as i32,
                                at.y.floor() as i32,
                                at.z.floor() as i32,
                            ),
                            stack,
                        );
                    }
                }
            }
        }
        for (who, dmg) in self.world.tick_projectiles(players, dt) {
            if let Some(p) = players.get(who).filter(|p| p.attackable) {
                events.push(SimEvent::PlayerHit {
                    who: p.id,
                    dmg,
                    from: p.pos,
                });
            }
        }

        // Precipitation lands near players while it lasts: snow
        // sprinkles layers onto exposed cold ground, rain tops up
        // whatever surface water it finds.
        if self.world.weather.precipitating() && !players.is_empty() {
            self.snow_timer += dt;
            if self.snow_timer >= 0.25 {
                self.snow_timer = 0.0;
                let mut rng = self.rng;
                for _ in 0..4 {
                    rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    let p = players[(rng >> 8) as usize % players.len()].pos;
                    rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    let dx = ((rng >> 8) % 49) as i32 - 24;
                    rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    let dz = ((rng >> 8) % 49) as i32 - 24;
                    let (x, z) = (p.x.floor() as i32 + dx, p.z.floor() as i32 + dz);
                    self.world.settle_snow(x, z);
                    self.world.rain_fill(x, z);
                }
                self.rng = rng;
            }
        }

        // Ire storms strike: every so often a bolt hunts natural
        // ground near a player, chars it fertile, and banks a bloom
        // — the wrath and the gift are the same event.
        if self.world.weather == Weather::Storm && !players.is_empty() {
            self.bolt_timer -= dt;
            if self.bolt_timer <= 0.0 {
                let mut rng = self.rng;
                rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                self.bolt_timer = 16.0 + ((rng >> 8) % 24) as f32;
                let p = players[(rng >> 6) as usize % players.len()].pos;
                rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                let dx = ((rng >> 8) % 81) as i32 - 40;
                rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                let dz = ((rng >> 8) % 81) as i32 - 40;
                self.rng = rng;
                let (sx, sz) = (p.x.floor() as i32 + dx, p.z.floor() as i32 + dz);
                if let Some((bx, by, bz)) = self.world.lightning_strike(sx, sz) {
                    events.push(SimEvent::Lightning(Vec3::new(
                        bx as f32 + 0.5,
                        by as f32 + 1.0,
                        bz as f32 + 0.5,
                    )));
                }
            }
        } else {
            self.bolt_timer = self.bolt_timer.max(6.0);
        }

        // Random ticks (crops, saplings) every half second.
        self.random_timer += dt;
        if self.random_timer >= 0.5 {
            self.random_timer = 0.0;
            let mut rng = self.rng;
            self.world.random_tick(&mut rng);
            self.rng = rng;
        }
    }

    /// The weather front machine: mostly random, but storm odds lean on
    /// the wild's ire. Durations are rolled per state, in day fractions.
    fn step_weather(&mut self, dt: f32, events: &mut Vec<SimEvent>) {
        self.world.weather_timer -= dt;
        if self.world.weather_timer > 0.0 {
            return;
        }
        let mut r01 = || {
            self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
            (self.rng >> 8) as f32 / (1 << 24) as f32
        };
        let (next, dur_days) = match self.world.weather {
            Weather::Clear => (Weather::Overcast, 0.2 + r01() * 0.3),
            Weather::Overcast => {
                let storm_p = 0.1 + 0.5 * (self.world.ire / 100.0);
                let roll = r01();
                if roll < storm_p {
                    (Weather::Storm, 0.1 + r01() * 0.2)
                } else if roll < storm_p + (1.0 - storm_p) * 0.6 {
                    (Weather::Precip, 0.2 + r01() * 0.6)
                } else {
                    (Weather::Clear, 0.5 + r01() * 1.5)
                }
            }
            Weather::Precip => (Weather::Clear, 0.5 + r01() * 1.5),
            Weather::Storm => (Weather::Overcast, 0.2 + r01() * 0.3),
        };
        self.world.weather = next;
        self.world.weather_timer = dur_days * DAY_LENGTH;
        events.push(SimEvent::WeatherChanged(next));
    }

    /// The night was slept through: the front moved on with it.
    pub fn sleep_to_dawn(&mut self) {
        self.time_of_day = 0.3;
        self.world.day = self.world.day.wrapping_add(1);
        self.world.weather_timer = 0.0;
        self.world.clock = Server::clock_of(self.world.day, self.time_of_day);
    }

    /// Sync the tier tracker (world load / forced ire) so the next tick
    /// doesn't toast a spurious change.
    pub fn sync_tier(&mut self) {
        self.prev_tier = self.world.ire_tier();
    }
}
