//! The authoritative simulation: the world, its clock, and everything
//! that happens in it. Singleplayer is a `Server` with one local player;
//! a multiplayer host is the same struct with remote players attached —
//! one simulation path, forever (the thing Minecraft retrofitted).
//!
//! The client side (rendering, input, UI, sounds) drives this with
//! `advance()` and applies the returned `SimEvent`s as presentation.

use glam::Vec3;

use crate::mobs::MobEvent;
use crate::planet::EntityPos;
use crate::world::World;

/// Fixed simulation rate. Rendering runs faster and interpol- er, copes.
pub const TICK: f32 = 1.0 / 30.0;
pub const DAY_LENGTH: f32 = 1200.0; // seconds per full day/night cycle

/// What the simulation needs to know about a player this tick.
#[derive(Clone, Copy)]
pub struct PlayerCtx {
    /// Stable identity for lead-following: 0 = host, guests their net id.
    pub id: u32,
    pub pos: EntityPos,
    pub spawn: EntityPos,
    /// False in creative or while dead: the wild can't touch you.
    pub attackable: bool,
    /// Charm of quiet: shrinks warden attention.
    pub aggro_mod: f32,
    /// The exact physical quiet charm whose prepaid concealment may be
    /// debited if it changes a warden's attention result.
    pub quiet_charm: Option<crate::inventory::ItemStack>,
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
    PlayerHit {
        who: u32,
        dmg: f32,
        dmg_type: Option<String>,
        attack: String,
        from: crate::planet::EntityPos,
    },
    /// A warden loosed a bolt (sound cue; the projectile is already live).
    BoltCast,
    /// Wildlife bred.
    Bred,
    /// A quiet charm changed an attention result and was debited.
    QuietSheltered { who: u32 },
    /// Authoritative death settlement completed; clients only present it.
    MobDied(crate::world::SettledMobDeath),
    /// Day rolled over; offerings worth this much were accepted.
    Dawn { offering_refund: f32 },
    /// The wild's ire crossed a tier boundary.
    IreTier { rose: bool, tier: usize },
    /// The wild's own hand: a bolt landed here.
    Lightning(crate::planet::EntityPos),
    /// The year stopped turning, or started again.
    LongWinter(bool),
    /// A bounded working reached its host-owned completion/interruption edge.
    Working(crate::workings::WorkingResult, crate::workings::WorkingCue),
    /// Bounded alchemy process/spoilage/leak cue; physical state is already
    /// committed by the host before presentation sees it.
    Alchemy(crate::alchemy::AlchemyCue),
    /// Forecast or consequence of an authoritative regional dross breach.
    Dross(crate::dross::DrossCue),
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
    /// Physical implement leakage/failure is sampled at a bounded cadence;
    /// an ordinary no-magic tick never scans every block entity.
    implements_timer: f32,
    implements_cursor: usize,
    /// Alchemy spoilage and leakage are slow processes. Tick them at a
    /// bounded one-second cadence rather than cloning/scanning the alchemy
    /// sidecar on every 20 Hz simulation step.
    alchemy_timer: f32,
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
            implements_timer: 0.0,
            implements_cursor: 0,
            alchemy_timer: 0.0,
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
        let sun = crate::planet_atlas::solar_direction(
            f64::from(self.world.day) + f64::from(self.time_of_day),
            f64::from(self.time_of_day),
        );
        (sun.y as f32 * 2.5 + 0.5).clamp(0.12, 1.0)
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
        // Instanced dungeons (capability E10): when EVERY player stands in
        // the Deep, the overworld holds its breath — the sleep-consensus
        // rule. Industry and dungeon creatures keep ticking; the sun,
        // weather, fluids, ire, and crops do not.
        let players_deep = players.iter().filter(|p| p.pos.face().is_deep()).count();
        let all_deep = !players.is_empty() && players_deep == players.len();
        self.world.tick_dungeon_runs(dt, players_deep);
        // The clock, the wild's ire, and dawn. A frozen clock holds the sun
        // still (headless capture); everything else still ticks.
        if !self.freeze_clock && !all_deep {
            let before = self.time_of_day;
            self.time_of_day = (self.time_of_day + dt / DAY_LENGTH) % 1.0;
            if self.time_of_day < before {
                self.world.day = self.world.day.wrapping_add(1);
            }
            self.world.clock = Server::clock_of(self.world.day, self.time_of_day);
        }
        if !all_deep {
            self.tick_overworld_nature(dt, events);
        }

        events.extend(
            self.world
                .tick_workings()
                .into_iter()
                .map(|(result, cue)| SimEvent::Working(result, cue)),
        );
        self.alchemy_timer += dt;
        if self.alchemy_timer >= 1.0 {
            self.alchemy_timer %= 1.0;
            match self.world.tick_alchemy(128) {
                Ok(cues) => events.extend(cues.into_iter().map(SimEvent::Alchemy)),
                Err(error) => eprintln!("alchemy update failed: {error}"),
            }
        }

        // Machines and gravity.
        self.world.tick_entities(dt);
        self.implements_timer += dt;
        if self.implements_timer >= 5.0 {
            self.implements_timer %= 5.0;
            self.world.tick_implements(&mut self.implements_cursor);
        }
        self.world.tick_falling(dt);

        // Creatures: wildlife, wardens, spawning, projectiles.
        let dl = players.first().map_or_else(
            || self.daylight(),
            |player| self.world.daylight_at_surface(player.pos.surface()),
        );
        let mut rng = self.rng;
        let mob_events = self.world.tick_mobs(players, dl, dt, &mut rng);
        // Spawning pressure rings a random player each cycle.
        if !players.is_empty() {
            rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            let p = &players[(rng >> 8) as usize % players.len()];
            let local_daylight = self.world.daylight_at_surface(p.pos.surface());
            self.world
                .tick_hostile_spawns(p.pos, p.spawn, local_daylight, dt, &mut rng);
            // Nest spawns run on their own toggle and their own clock:
            // silencing the ring must not silence the dens (capability E9).
            self.world
                .tick_nest_spawns(p.pos, local_daylight, dt, &mut rng);
        }
        self.rng = rng;
        for ev in mob_events {
            match ev {
                MobEvent::HitPlayer {
                    who,
                    dmg,
                    dmg_type,
                    from,
                    attack,
                    ..
                } => {
                    if let Some(p) = players.get(who) {
                        events.push(SimEvent::PlayerHit {
                            who: p.id,
                            dmg,
                            dmg_type,
                            attack,
                            from,
                        });
                    }
                }
                MobEvent::Cast(proj) => {
                    self.world.spawn_projectile(proj);
                    events.push(SimEvent::BoltCast);
                }
                MobEvent::HealPulse {
                    origin,
                    radius,
                    heal,
                } => {
                    let reg = self.world.reg.clone();
                    self.world.for_each_mob_mut(|m| {
                        let Some(def) = reg.animals.get(m.species) else {
                            return;
                        };
                        if def.hostile && m.pos.local_delta_to(origin).length() <= radius {
                            m.health = (m.health + heal).min(def.health);
                        }
                    });
                }
                MobEvent::SpawnMinions {
                    pos,
                    species,
                    count,
                } => {
                    if self.world.mobs().len() >= crate::world::MOB_CAP {
                        break;
                    }
                    let Some(def) = self.world.reg.animals.get(species).cloned() else {
                        break;
                    };
                    for _ in 0..count.min(4) {
                        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        let ang = self.rng as f32;
                        let step = ((self.rng >> 8) % 9) as f32;
                        let du = ang.cos() * (step + 2.0);
                        let dv = ang.sin() * (step + 2.0);
                        let Some(moved) = pos
                            .translated(glam::Vec3::new(du, 0.0, dv))
                            .ok()
                            .map(|m| m.pos)
                        else {
                            continue;
                        };
                        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        let mut mob = crate::mobs::Mob::new_at(
                            species,
                            moved,
                            (self.rng % 1024) as f32 / 1024.0 * std::f32::consts::TAU,
                        );
                        mob.health = def.health;
                        self.world.spawn_mob(mob);
                    }
                    events.push(SimEvent::BoltCast);
                }
                MobEvent::HitMob {
                    id,
                    dmg,
                    dmg_type,
                    from,
                } => {
                    let td = self
                        .world
                        .reg
                        .animals
                        .get(self.world.mob_by_id(id).map_or(usize::MAX, |m| m.species))
                        .cloned();
                    if let Some(td) = td
                        && let Some(target) = self.world.mob_by_id_mut(id)
                    {
                        target.hurt(&td, dmg, dmg_type.as_deref(), from);
                    }
                }
                MobEvent::Bred => events.push(SimEvent::Bred),
                MobEvent::Build {
                    template,
                    anchor,
                    rot,
                } => {
                    if let Some(t) = self.world.template(&template).cloned() {
                        self.world.stamp_mob(&t, anchor, rot);
                    }
                }
                MobEvent::QuietSheltered { player: who, mob } => {
                    let mut paid = false;
                    if let Some(player) = players.get(who)
                        && let Some(mut charm) = player.quiet_charm
                        && let Some(pos) = player.pos.block()
                        && self.world.debit_charm_at(
                            pos,
                            &mut charm,
                            "quiet",
                            "quiet charm changed a warden attention result",
                        )
                    {
                        paid = true;
                        events.push(SimEvent::QuietSheltered { who: player.id });
                    }
                    // Perception tentatively identified the exact case where
                    // quiet would matter. If the atomic debit loses a race or
                    // the account is depleted, restore the ordinary attention
                    // result immediately; no unpaid tick of concealment leaks
                    // through the two-phase decision.
                    if !paid
                        && let Some(player) = players.get(who)
                        && let Some(warden) = self.world.mob_by_id_mut(mob)
                    {
                        warden.state = crate::mobs::MobState::Hunt;
                        warden.state_timer = 0.0;
                        warden.target = player.pos;
                    }
                }
                // Kills are settled inside tick_mobs; none escape.
                MobEvent::Killed(_) => {}
                MobEvent::Ate(pos) => self.world.apply_bite_at(pos),
                MobEvent::Dung(at, guano) => {
                    let item = if guano { "base:guano" } else { "base:dung" };
                    if let Some(dung) = self.world.reg.item_id(item) {
                        let reg = self.world.reg.clone();
                        let stack = crate::inventory::ItemStack::new(&reg, dung, 1);
                        if let Some(at) = at.block() {
                            self.world.push_drop_at(at, stack);
                        }
                    }
                }
                MobEvent::LeadSnapped(at) => {
                    if let Some(lead) = self.world.reg.item_id("base:lead") {
                        let reg = self.world.reg.clone();
                        let stack = crate::inventory::ItemStack::new(&reg, lead, 1);
                        if let Some(at) = at.block() {
                            self.world.push_drop_at(at, stack);
                        }
                    }
                }
            }
        }
        let deaths = self.world.settle_dead_mobs(&mut self.rng);
        events.extend(deaths.into_iter().map(SimEvent::MobDied));
        for (who, dmg, dmg_type) in self.world.tick_projectiles(players, dt) {
            if let Some(p) = players.get(who).filter(|p| p.attackable) {
                events.push(SimEvent::PlayerHit {
                    who: p.id,
                    dmg,
                    dmg_type,
                    attack: "bolt".into(),
                    from: p.pos,
                });
            }
        }

        // Precipitation lands near players while it lasts: snow
        // sprinkles layers onto exposed cold ground, rain tops up
        // whatever surface water it finds.
        if players.iter().any(|player| {
            self.world
                .weather_at_surface(player.pos.surface())
                .kind
                .precipitating()
        }) {
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
                    if let Some(surface) = p
                        .block()
                        .and_then(|at| at.offset(dx, 0, dz))
                        .map(|at| at.surface())
                        .filter(|surface| {
                            self.world.weather_at_surface(*surface).kind.precipitating()
                        })
                    {
                        self.world.settle_snow_at(surface);
                        self.world.rain_fill_at(surface);
                    }
                }
                self.rng = rng;
            }
        }

        // Ire storms strike: every so often a bolt hunts natural
        // ground near a player, chars it fertile, and banks a bloom
        // — the wrath and the gift are the same event.
        let storm_players: Vec<&PlayerCtx> = players
            .iter()
            .filter(|player| {
                self.world.weather_at_surface(player.pos.surface()).kind
                    == crate::planet_atlas::LocalWeather::Storm
            })
            .collect();
        if !storm_players.is_empty() {
            self.bolt_timer -= dt;
            if self.bolt_timer <= 0.0 {
                let mut rng = self.rng;
                rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                self.bolt_timer = 16.0 + ((rng >> 8) % 24) as f32;
                let p = storm_players[(rng >> 6) as usize % storm_players.len()].pos;
                rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                let dx = ((rng >> 8) % 81) as i32 - 40;
                rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                let dz = ((rng >> 8) % 81) as i32 - 40;
                self.rng = rng;
                let target = p
                    .translated(Vec3::new(dx as f32, 0.0, dz as f32))
                    .ok()
                    .and_then(|canonical| canonical.pos.block())
                    .map(|block| block.surface());
                if let Some(struck) = target.and_then(|at| self.world.lightning_strike_at(at)) {
                    let landed = struck
                        .entity_center()
                        .translated(Vec3::new(0.0, 0.5, 0.0))
                        .expect("lightning presentation offset stays canonical")
                        .pos;
                    events.push(SimEvent::Lightning(landed));
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

    /// The overworld's natural processes: planetary weather and Current,
    /// the wild's ire and dawn, and fluids. Gated off while every player
    /// stands in the Deep (capability E10) — the world holds its breath —
    /// and behind `freeze_clock` semantics they never had a clock of their
    /// own to pause before now.
    fn tick_overworld_nature(&mut self, dt: f32, events: &mut Vec<SimEvent>) {
        if let Err(error) = self.world.tick_planetary_weather(4_096) {
            eprintln!("planetary weather update failed: {error}");
        }
        if let Err(error) = self.world.tick_arcane_geography(4_096) {
            eprintln!("planetary Current update failed: {error}");
        }
        match self
            .world
            .tick_dross(crate::dross::DROSS_SERVER_SLICE_CELLS)
        {
            Ok(report) => events.extend(report.cues.into_iter().map(SimEvent::Dross)),
            Err(error) => eprintln!("planetary dross update failed: {error}"),
        }
        if let Err(error) = self.world.tick_arcane_ecology(512) {
            eprintln!("planetary magical ecology update failed: {error}");
        }
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
    }

    /// The night was slept through; the local weather atlas keeps evolving on
    /// its own hourly clock rather than being re-rolled globally.
    pub fn sleep_to_dawn(&mut self) {
        self.time_of_day = 0.3;
        self.world.day = self.world.day.wrapping_add(1);
        self.world.clock = Server::clock_of(self.world.day, self.time_of_day);
    }

    /// Sync the tier tracker (world load / forced ire) so the next tick
    /// doesn't toast a spurious change.
    pub fn sync_tier(&mut self) {
        self.prev_tier = self.world.ire_tier();
    }
}
