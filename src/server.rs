//! The authoritative simulation: the world, its clock, and everything
//! that happens in it. Singleplayer is a `Server` with one local player;
//! a multiplayer host is the same struct with remote players attached —
//! one simulation path, forever (the thing Minecraft retrofitted).
//!
//! The client side (rendering, input, UI, sounds) drives this with
//! `advance()` and applies the returned `SimEvent`s as presentation.

use glam::Vec3;

use crate::mobs::MobEvent;
use crate::world::World;

/// Fixed simulation rate. Rendering runs faster and interpol- er, copes.
pub const TICK: f32 = 1.0 / 30.0;
pub const DAY_LENGTH: f32 = 600.0; // seconds per full day/night cycle

/// What the simulation needs to know about a player this tick.
#[derive(Clone, Copy)]
pub struct PlayerCtx {
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
    PlayerHit { who: usize, dmg: f32, from: Vec3 },
    /// A warden loosed a bolt (sound cue; the projectile is already live).
    BoltCast,
    /// Wildlife bred.
    Bred,
    /// Day rolled over; offerings worth this much were accepted.
    Dawn { offering_refund: f32 },
    /// The wild's ire crossed a tier boundary.
    IreTier { rose: bool, tier: usize },
}

pub struct Server {
    pub world: World,
    pub time_of_day: f32,
    /// Simulation randomness — separate from client/UI randomness.
    pub rng: u32,
    accum: f32,
    water_timer: f32,
    random_timer: f32,
    prev_tier: usize,
}

impl Server {
    pub fn new(world: World, time_of_day: f32, rng: u32) -> Server {
        let prev_tier = world.ire_tier();
        Server {
            world,
            time_of_day,
            rng,
            accum: 0.0,
            water_timer: 0.0,
            random_timer: 0.0,
            prev_tier,
        }
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
        // The clock, the wild's ire, and dawn.
        self.time_of_day = (self.time_of_day + dt / DAY_LENGTH) % 1.0;
        if self.world.tick_ire(dt / DAY_LENGTH) {
            let refund = self.world.accept_offerings();
            events.push(SimEvent::Dawn {
                offering_refund: refund,
            });
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

        // Machines.
        self.world.tick_entities(dt);

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
                    events.push(SimEvent::PlayerHit { who, dmg, from })
                }
                MobEvent::Cast(proj) => {
                    self.world.projectiles.push(proj);
                    events.push(SimEvent::BoltCast);
                }
                MobEvent::Bred(_) => events.push(SimEvent::Bred),
            }
        }
        for (who, dmg) in self.world.tick_projectiles(players, dt) {
            if players.get(who).is_some_and(|p| p.attackable) {
                events.push(SimEvent::PlayerHit {
                    who,
                    dmg,
                    from: players[who].pos,
                });
            }
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

    /// Sync the tier tracker (world load / forced ire) so the next tick
    /// doesn't toast a spurious change.
    pub fn sync_tier(&mut self) {
        self.prev_tier = self.world.ire_tier();
    }
}
