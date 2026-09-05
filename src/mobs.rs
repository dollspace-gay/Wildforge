//! Wildlife: boxy passive mobs with wander/flee AI, hunted for food.
//! Species are data (`registry::AnimalDef`); this module is the runtime —
//! movement, steering, rendering, and ray hits.

use glam::{Vec2, Vec3};

use crate::atlas::ATLAS_TILES;
use crate::mesher::{CORNERS, FACE_SHADE, NORMALS, Vertex};
use crate::planet::EntityPos;
#[cfg(test)]
use crate::planet::Face;
use crate::registry::{AnimalDef, AttackKind, BehaviorArchetype, Registry};
use crate::server::PlayerCtx;
use crate::world::World;

const GRAVITY: f32 = 28.0;
const TERMINAL: f32 = 40.0;
const JUMP: f32 = 7.6;
/// How far a hungry predator will notice prey.
/// Feet to crown of a humanoid, in blocks: the head box tops out at
/// 22+7 px and hair sits a shade proud of it (16 px = 1 block). The
/// paper-doll preview frames itself against this.
pub const HUMANOID_HEIGHT: f32 = 1.85;

pub const HUNT_RANGE: f32 = 28.0;
/// This long past empty, a predator gets ideas about the player.
pub const BELLY_DESPERATE: f32 = -240.0;
/// An untouched carcass rots away in this many seconds.
pub const CARCASS_ROT_SECS: f32 = 180.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MobState {
    Idle,
    Wander,
    Flee,
    /// Hostiles only: chase/attack the player.
    Hunt,
    /// Hungry: walking to a plant meal. The raid never climbs — a
    /// one-high wall turns a grazer, and that is the point of fences.
    Graze,
    /// Hungry predator closing on quarry (a mob, not a player).
    Stalk,
}

/// Things a mob did this tick that the game loop must apply.
pub enum MobEvent {
    /// A lead stretched past its limit: drop the strip here.
    LeadSnapped(EntityPos),
    /// Contact damage: (attacker index, half-hearts, damage class, attack
    /// name, attacker position). Untyped melee carries an empty class.
    HitPlayer {
        who: usize,
        dmg: f32,
        dmg_type: Option<String>,
        attack: String,
        from: EntityPos,
    },
    /// A caster fired: projectile spawn.
    Cast(Projectile),
    /// A wildlife pair bred at this position.
    Bred,
    /// A quiet charm kept this mob outside its reduced attention interval.
    QuietSheltered { player: usize, mob: u32 },
    /// A grazer took its bite: the world applies the plant's loss.
    Ate(crate::planet::BlockPos),
    /// A predator's kill landed: the prey (by id) becomes a carcass.
    Killed(u32),
    /// Digestion finished where it always does (true = a bat's
    /// guano, the cave's own fertilizer).
    Dung(EntityPos, bool),
    /// A builder stamped a template: the world places the cells through
    /// the ordinary block path.
    Build {
        template: String,
        anchor: crate::planet::BlockPos,
        rot: crate::world::multiblock::Rotation,
    },
    /// A support healed its allies (capability E9): restore `heal` to every
    /// hostile mob within `radius` of `origin`.
    HealPulse {
        origin: EntityPos,
        radius: f32,
        heal: f32,
    },
    /// A controller summoned its companions (capability E9): the world
    /// spawns `count` of `species` near `pos`.
    SpawnMinions {
        pos: EntityPos,
        species: usize,
        count: u32,
    },
    /// A guard struck a hostile mob (capability E15): the world applies
    /// the damage through `Mob::hurt`, which triggers retaliation.
    HitMob {
        id: u32,
        dmg: f32,
        dmg_type: Option<String>,
        from: EntityPos,
    },
}

/// A bolt in flight: warden thorn/ember/frost, or a player's arrow.
#[derive(Clone, Debug)]
pub struct Projectile {
    /// Session-stable host identity used by bounded entity workings and wire
    /// interpolation. Projectiles are intentionally transient across a world
    /// restart; an active transaction targeting one settles as interrupted.
    pub stable_id: u64,
    pub pos: EntityPos,
    pub vel: Vec3,
    pub tile: u16,
    pub damage: f32,
    /// Damage class this bolt carries ("pierce", "fire"...); the wild's
    /// resistances key on it. None = untyped.
    pub damage_type: Option<String>,
    pub age: f32,
    /// Player arrows seek mobs; warden bolts seek the player.
    pub from_player: bool,
    /// Item recovered when this sticks into a block (arrows).
    pub drop_item: Option<crate::registry::ItemId>,
    /// A state-bearing preparation vessel carried intact until impact. Unlike
    /// an arrow's simple item id, this preserves the stable container id so
    /// breakage can settle its exact liquid, matter, Current, and dross.
    pub preparation_payload: Option<crate::inventory::ItemStack>,
    /// 0 = the host/local player; guests get their arrows back by wire.
    pub owner: u32,
}

pub enum ProjHit {
    /// Still flying.
    None,
    Expired,
    Block,
    /// Index into the player list.
    Player(usize),
    /// Index into world.mobs.
    Mob(usize),
}

impl Projectile {
    pub fn tick(&mut self, world: &World, players: &[PlayerCtx], dt: f32) -> ProjHit {
        self.age += dt;
        if self.age > 8.0 {
            return ProjHit::Expired;
        }
        self.vel.y -= 3.0 * dt; // light arc
        let Ok(moved) = self.pos.translated(self.vel * dt) else {
            return ProjHit::Expired;
        };
        self.pos = moved.pos;
        self.vel = moved.rotation.rotate_vec3(self.vel);
        let Some(cell) = self.pos.block() else {
            return ProjHit::Expired;
        };
        let b = world.get_block_at(cell);
        if world.reg.is_solid(b) {
            return ProjHit::Block;
        }
        if self.from_player {
            for (i, m) in world.mobs().iter().enumerate() {
                let Some(def) = world.reg.animals.get(m.species) else {
                    continue;
                };
                // Measure the bolt from the animal's feet.  The previous
                // ordering made `d.y` point down from the bolt to the mob,
                // so a perfectly ordinary waist-high arrow could never
                // enter the positive-height hit box.
                let d = m.pos.local_delta_to(self.pos);
                if d.x.abs() < def.half_w + 0.2
                    && d.z.abs() < def.half_w + 0.2
                    && d.y > -0.15
                    && d.y < def.height + 0.2
                {
                    return ProjHit::Mob(i);
                }
            }
        } else {
            for (i, p) in players.iter().enumerate() {
                let d = self.pos.local_delta_to(p.pos) + Vec3::new(0.0, 0.9, 0.0);
                if d.x.abs() < 0.5 && d.z.abs() < 0.5 && d.y.abs() < 1.1 {
                    return ProjHit::Player(i);
                }
            }
        }
        ProjHit::None
    }

    /// Small spinning sprite, drawn with the entity pipeline.
    pub fn emit(&self, verts: &mut Vec<Vertex>, idx: &mut Vec<u32>) {
        let (tx, ty) = (
            self.tile as u32 % ATLAS_TILES,
            self.tile as u32 / ATLAS_TILES,
        );
        let ts = 1.0 / ATLAS_TILES as f32;
        let inset = ts / 32.0;
        let center = self.pos.render_pos();
        let frame = crate::planet::local_frame(self.pos.surface_point());
        let east = frame.east.as_vec3();
        let up = frame.up.as_vec3();
        let north = frame.north.as_vec3();
        let ang = self.age * 6.0;
        let (sin, cos) = ang.sin_cos();
        let h = 0.28;
        for (dx, dz) in [(cos, sin), (-sin, cos)] {
            for flip in [false, true] {
                let base = verts.len() as u32;
                let (u0, u1) = if flip {
                    ((tx + 1) as f32 * ts - inset, tx as f32 * ts + inset)
                } else {
                    (tx as f32 * ts + inset, (tx + 1) as f32 * ts - inset)
                };
                let sgn = if flip { -1.0 } else { 1.0 };
                let corners = [
                    (-0.5 * h * sgn, -0.5 * h, u0),
                    (0.5 * h * sgn, -0.5 * h, u1),
                    (0.5 * h * sgn, 0.5 * h, u1),
                    (-0.5 * h * sgn, 0.5 * h, u0),
                ];
                for (o, y, u) in corners {
                    let v = if y < 0.0 {
                        (ty + 1) as f32 * ts - inset
                    } else {
                        ty as f32 * ts + inset
                    };
                    let world = center + east * (dx * o) + up * y + north * (dz * o);
                    verts.push(Vertex {
                        pos: world.to_array(),
                        uv: [u, v],
                        normal: [0.0, 0.0, 0.0],
                        light: [1.0; 3], // bolts glow faintly
                        sky: 1.0,
                        ao: 1.0,
                    });
                }
                idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Mob {
    /// Stable id for the wire (0 until the host's sim assigns one);
    /// guests interpolate and target mobs by it across snapshots.
    pub id: u32,
    pub species: usize,
    /// Feet-center position.
    pub pos: EntityPos,
    pub vel: Vec3,
    /// Facing; forward = (sin yaw, 0, cos yaw).
    pub yaw: f32,
    pub health: f32,
    pub state: MobState,
    pub state_timer: f32,
    /// Wander destination, or the point we're fleeing from.
    pub target: EntityPos,
    pub anim_phase: f32,
    pub hurt_flash: f32,
    pub on_ground: bool,
    hit_wall: bool,
    /// Per-attack cooldown wheel (spec 3.6), one entry per species attack,
    /// sized on first tick. Projectile attacks seed at 1.0s so a fresh
    /// caster waits a beat like the legacy `cast_cd` did.
    pub attack_cds: Vec<f32>,
    /// A charge attack winding up: seconds until the dash starts.
    pub charge_wind_up: f32,
    /// Distance and direction recorded when the charge began.
    charge_range: f32,
    charge_dir: Vec3,
    /// Yaw frozen while a charge winds up and dashes (it does not turn).
    charge_yaw: f32,
    /// Active dash: (blocks left, direction). The mob moves at speed*3
    /// in a straight line until it collides or runs out of distance.
    pub dash: Option<(f32, Vec3)>,
    /// Behavioral archetype (spec 3.6), synced from the def each tick.
    pub archetype: BehaviorArchetype,
    /// A construct disabled by hacking: inert, non-aggressive.
    pub hacked: bool,
    /// A brute's enrage: seconds of doubled attack tempo (set by a hit
    /// from a damage class it is vulnerable to).
    pub rage: f32,
    /// A builder's stamp rhythm.
    pub build_cd: f32,
    pub built_count: u32,
    /// E9 rhythms: a support's heal-pulse cooldown, a controller's minion
    /// cooldown, a phaser's blink cooldown. Host-side transient, like `rage`.
    pub support_cd: f32,
    pub controller_cd: f32,
    pub phaser_cd: f32,
    /// Seconds spent out of aggro range while hunting (drops at 8).
    lose_aggro: f32,
    /// Fed and ready to breed (wildlife husbandry).
    pub fed: bool,
    /// Seconds of not fleeing the player after being fed.
    pub calm: f32,
    /// Cooldown between litters.
    pub breed_cd: f32,
    /// 0 = newborn, 1 = adult; scales the model.
    pub growth: f32,
    /// Guest id that last struck this mob (0 = host); drops route there.
    pub last_hit_by: u32,
    /// Domesticated: never flees players, never counts as the wild's
    /// dead, follows a lead. Earned by repeated feeding.
    pub tamed: bool,
    /// Feedings toward taming, and the rolled requirement (0 = unrolled).
    pub tame_fed: u8,
    pub tame_need: u8,
    /// Saddlebags: a carrier's 12-slot pack, spilled where it dies.
    pub cargo: Option<Box<[Option<crate::inventory::ItemStack>; 12]>>,
    /// Player currently leading this mob (PlayerCtx id; None = loose).
    pub led_by: Option<u32>,
    /// Player riding this vehicle (PlayerCtx id; transient).
    pub ridden_by: Option<u32>,
    /// A warden that only watches (the warning before the hunt).
    pub watcher: bool,
    pub watch_timer: f32,
    /// The cell's grievance when the watching began.
    pub watch_baseline: f32,
    /// The herd's nearby center, written by tick_mobs each tick.
    pub herd_pull: Option<EntityPos>,
    /// Nearest prey in range (id, pos), written by tick_mobs.
    pub quarry: Option<(u32, EntityPos)>,
    /// Desperate enough to size up the player (tick_mobs decides:
    /// deep hunger plus night or winter).
    pub bold: bool,
    /// Carcasses only: seconds until the ground takes the rest.
    pub rot: f32,
    /// A warden whose heart died mid-existence: never recalled, never
    /// dissolved, and still walking.
    pub masterless: bool,
    /// Seconds until the next hunger (counts down; <= 0 is hungry).
    pub belly: f32,
    /// Seconds until digestion finishes (> 0 after any meal).
    pub digest: f32,
    /// Debounces sustained concealment so one warden charges one bounded
    /// interval rather than every simulation tick.
    quiet_notice: f32,
}

fn r01(rng: &mut u32) -> f32 {
    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
    (*rng >> 8) as f32 / (1 << 24) as f32
}

fn block_at_height(world: &World, pos: EntityPos, y: i32) -> crate::registry::BlockId {
    if !(0..crate::chunk::CHUNK_Y as i32).contains(&y) {
        return crate::registry::AIR;
    }
    let surface =
        crate::planet::SurfacePos::new(pos.face(), pos.u().floor() as u16, pos.v().floor() as u16)
            .expect("canonical entity has a valid surface cell");
    let block = crate::planet::BlockPos::new(surface.face(), surface.u(), y as u8, surface.v())
        .expect("height and surface were validated");
    world.get_block_at(block)
}

/// Whether an attacker at `from` is behind the mob's facing, judged within
/// a `cone_deg` cone off the nose (the E3 backstab check, parameterized so
/// a shield-bearer's front cone can mirror it).
fn mob_facing_away(yaw: f32, mob_pos: EntityPos, from: EntityPos, cone_deg: f32) -> bool {
    let delta = mob_pos.local_delta_to(from);
    let to = Vec2::new(delta.x, delta.z);
    let len = to.length();
    if len < 1e-4 {
        return false;
    }
    let to = to / len;
    let forward = Vec2::new(yaw.sin(), yaw.cos());
    forward.dot(to) < cone_deg.to_radians().cos()
}

/// Shortest-arc angle interpolation (snapshot smoothing).
pub fn lerp_yaw(a: f32, b: f32, t: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let mut d = (b - a) % TAU;
    if d > PI {
        d -= TAU;
    } else if d < -PI {
        d += TAU;
    }
    a + d * t
}

impl Mob {
    #[cfg(test)]
    pub fn new(species: usize, pos: Vec3, yaw: f32) -> Mob {
        let pos = EntityPos::from_local(Face::PosZ, pos)
            .expect("legacy mob position is inside the finite PosZ chart");
        Self::new_at(species, pos, yaw)
    }

    pub fn new_at(species: usize, pos: EntityPos, yaw: f32) -> Mob {
        Mob {
            id: 0,
            species,
            pos,
            vel: Vec3::ZERO,
            yaw,
            health: 0.0, // caller sets from the def
            state: MobState::Idle,
            state_timer: 1.0,
            target: pos,
            anim_phase: 0.0,
            hurt_flash: 0.0,
            on_ground: false,
            hit_wall: false,
            attack_cds: Vec::new(),
            charge_wind_up: 0.0,
            charge_range: 0.0,
            charge_dir: Vec3::ZERO,
            charge_yaw: yaw,
            dash: None,
            archetype: crate::registry::BehaviorArchetype::Standard,
            hacked: false,
            rage: 0.0,
            build_cd: 0.0,
            built_count: 0,
            support_cd: 0.0,
            controller_cd: 0.0,
            phaser_cd: 0.0,
            lose_aggro: 0.0,
            fed: false,
            calm: 0.0,
            breed_cd: 0.0,
            growth: 1.0,
            last_hit_by: 0,
            tamed: false,
            tame_fed: 0,
            tame_need: 0,
            cargo: None,
            led_by: None,
            ridden_by: None,
            watcher: false,
            watch_timer: 0.0,
            watch_baseline: 0.0,
            herd_pull: None,
            quarry: None,
            bold: false,
            rot: 0.0,
            masterless: false,
            // A grace period before the first meal matters.
            belly: 240.0,
            digest: 0.0,
            quiet_notice: 0.0,
        }
    }

    /// One taming meal: rolls the requirement on the first feeding
    /// (3-5, deterministic per mob), counts up, and returns true the
    /// moment trust lands.
    pub fn feed_tame(&mut self) -> bool {
        if self.tamed {
            return false;
        }
        if self.tame_need == 0 {
            self.tame_need = 3 + (self.id % 3) as u8;
        }
        self.tame_fed += 1;
        if self.tame_fed >= self.tame_need {
            self.tamed = true;
            return true;
        }
        false
    }

    /// Take damage from an attacker at `from`: knockback, then panic
    /// (wildlife) or retaliation (wardens). `dmg_type` keys the species'
    /// damage-class resistances (None = full).
    pub fn hurt(&mut self, def: &AnimalDef, dmg: f32, dmg_type: Option<&str>, from: EntityPos) {
        let mut mult = def.damage_multiplier(dmg_type.unwrap_or(""));
        // E9: a shield-bearer blocks what it faces — the E3 backstab
        // check, inverted. Front hits land reduced; rear hits land full.
        if def.behavior == BehaviorArchetype::ShieldBearer
            && let Some(shield) = def.archetype.shield.as_ref()
            && !mob_facing_away(self.yaw, self.pos, from, shield.front_deg)
        {
            mult *= shield.front_mult;
        }
        self.health -= dmg * mult;
        self.hurt_flash = 0.35;
        // A brute wounded by a damage class it is vulnerable to (mult > 1)
        // enrages: it swings on double time for a few seconds.
        if def.behavior == BehaviorArchetype::Brute
            && let Some(kind) = dmg_type
            && def.damage_multiplier(kind) > 1.0
        {
            self.rage = 6.0;
        }
        let mut away = -self.pos.local_delta_to(from);
        away.y = 0.0;
        let dir = if away.length_squared() > 0.001 {
            away.normalize()
        } else {
            Vec3::Z
        };
        let mut kb = if def.movement_float { 2.5 } else { 6.0 };
        // E9: a tank holds its ground.
        if def.behavior == BehaviorArchetype::Tank {
            kb *= def
                .archetype
                .tank
                .as_ref()
                .map_or(0.25, |t| t.knockback_mult);
        }
        self.vel += dir * kb + Vec3::new(0.0, if def.movement_float { 1.0 } else { 4.5 }, 0.0);
        if def.hostile || def.fierce {
            // Wardens retaliate; the polar bear was already coming.
            self.state = MobState::Hunt;
            self.state_timer = 10.0;
            self.lose_aggro = 0.0;
        } else {
            // Everything else — desperate wolves included — breaks
            // off when wounded. Hungry, not suicidal.
            self.state = MobState::Flee;
            self.state_timer = 5.0;
        }
        self.target = from;
    }

    /// If embedded in solid blocks (ticked while its chunk was missing in
    /// an older save), pop up to the first free spot instead of staying
    /// wedged belly-deep in the terrain.
    pub fn unstick(&mut self, world: &World, def: &AnimalDef) {
        if !self.collides(world, def, self.pos) {
            return;
        }
        for _ in 0..64 {
            self.pos.y += 0.5;
            if !self.collides(world, def, self.pos) {
                self.vel = Vec3::ZERO;
                return;
            }
        }
    }

    pub fn tick(
        &mut self,
        world: &World,
        def: &AnimalDef,
        players: &[PlayerCtx],
        dt: f32,
        rng: &mut u32,
        events: &mut Vec<MobEvent>,
    ) {
        self.archetype = def.behavior;
        // The mob cares about whoever is closest (and, for hunting,
        // closest *attackable*).
        let nearest = players
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                self.pos
                    .local_delta_to(a.pos)
                    .length_squared()
                    .total_cmp(&self.pos.local_delta_to(b.pos).length_squared())
            })
            .map(|(i, p)| (i, *p));
        let prey = players
            .iter()
            .enumerate()
            .filter(|(_, p)| p.attackable)
            .min_by(|(_, a), (_, b)| {
                self.pos
                    .local_delta_to(a.pos)
                    .length_squared()
                    .total_cmp(&self.pos.local_delta_to(b.pos).length_squared())
            })
            .map(|(i, p)| (i, *p));
        self.state_timer -= dt;
        self.hurt_flash = (self.hurt_flash - dt).max(0.0);
        // A raging brute's wheel drains at double tempo.
        let wheel_rate = if self.archetype == BehaviorArchetype::Brute && self.rage > 0.0 {
            2.0
        } else {
            1.0
        };
        self.rage = (self.rage - dt).max(0.0);
        for cd in self.attack_cds.iter_mut() {
            *cd = (*cd - dt * wheel_rate).max(0.0);
        }
        if self.charge_wind_up > 0.0 {
            self.charge_wind_up -= dt;
            if self.charge_wind_up <= 0.0 {
                self.dash = Some((self.charge_range, self.charge_dir));
                self.charge_wind_up = 0.0;
            }
        }
        self.calm = (self.calm - dt).max(0.0);
        self.breed_cd = (self.breed_cd - dt).max(0.0);
        self.quiet_notice = (self.quiet_notice - dt).max(0.0);
        if self.growth < 1.0 {
            self.growth = (self.growth + dt / 1200.0).min(1.0);
        }
        // The belly: hunger is the clock the whole trophic layer runs
        // on. Only species with one, and only grown animals.
        if def.belly_secs > 0.0 && self.growth >= 1.0 {
            self.belly -= dt;
            // A species that neither grazes nor hunts (bats) finds
            // its own meals — insects, abstracted — and digests them
            // where it roosts.
            if self.belly <= 0.0 && !def.grazes && def.prey.is_empty() {
                self.belly = def.belly_secs;
                self.digest = 30.0 + (self.id % 30) as f32;
            }
            if self.digest > 0.0 {
                self.digest -= dt;
                if self.digest <= 0.0 {
                    let guano = def.name.ends_with(":bat");
                    if self.on_ground || def.movement_float {
                        events.push(MobEvent::Dung(self.pos, guano));
                    } else {
                        self.digest = 0.5; // held (politely) until grounded
                    }
                }
            }
        }

        // Skittish species bolt when anyone closes in — unless recently
        // fed (feeding is taming-lite). The industrial response gradient
        // (capability E12) frays their nerves: in tier-2+ country they
        // startle from three quarters the distance, at tier 3 half.
        if let Some((_, near)) = nearest
            && def.flee_range > 0.0
            && !def.hostile
            && !self.tamed
            && self.calm <= 0.0
            && self.state != MobState::Flee
        {
            let fear = match world.ire_tier() {
                0 | 1 => 1.0,
                2 => 0.75,
                _ => 0.5,
            };
            let range = def.flee_range * fear;
            let mut d = self.pos.local_delta_to(near.pos);
            d.y = 0.0;
            if d.length_squared() < range * range {
                self.state = MobState::Flee;
                self.state_timer = 4.0;
                self.target = near.pos;
            }
        }
        // Wildlife must continue to qualify for a desperation hunt. Food
        // becoming available or hunger ending also ends an existing chase.
        if self.state == MobState::Hunt && !def.hostile && !def.fierce && !self.bold {
            self.state = MobState::Idle;
            self.state_timer = 1.0;
            self.lose_aggro = 0.0;
        }
        // Wardens take notice (the quiet charm shortens their attention).
        if let Some((player_index, p)) = prey
            && (def.hostile
                || def.fierce
                || (self.bold && def.attack > 0.0 && self.state != MobState::Flee))
            && !self.tamed
            && !self.watcher
            && self.state != MobState::Hunt
        {
            let range = (def.aggro_range + p.aggro_mod).max(2.0);
            let distance_sq = self.pos.local_delta_to(p.pos).length_squared();
            if distance_sq < range * range {
                self.state = MobState::Hunt;
                self.lose_aggro = 0.0;
            } else if p.quiet_charm.is_some()
                && p.aggro_mod < 0.0
                && distance_sq < def.aggro_range * def.aggro_range
                && self.quiet_notice <= 0.0
            {
                self.quiet_notice = 5.0;
                events.push(MobEvent::QuietSheltered {
                    player: player_index,
                    mob: self.id,
                });
            }
        }

        // A led animal walks after its handler and does nothing else.
        // Too far and the lead snaps (the strip drops where it broke).
        let mut led_active = false;
        let mut wish = Vec3::ZERO;
        if let Some(pid) = self.led_by {
            match players.iter().find(|p| p.id == pid) {
                Some(p) if self.pos.local_delta_to(p.pos).length_squared() <= 12.0 * 12.0 => {
                    led_active = true;
                    let mut to = self.pos.local_delta_to(p.pos);
                    to.y = 0.0;
                    if to.length_squared() > 3.0 * 3.0 {
                        let dir = to.normalize();
                        self.yaw = dir.x.atan2(dir.z);
                        wish = dir * def.speed * 0.75;
                    }
                    self.state = MobState::Idle;
                    self.state_timer = self.state_timer.max(0.5);
                }
                _ => {
                    self.led_by = None;
                    events.push(MobEvent::LeadSnapped(self.pos));
                }
            }
        }
        // A watcher only watches: it faces whoever is nearest, gives
        // ground if crowded, and counts the seconds. The hunt is
        // someone else's decision (ecology grades the vigil).
        if self.watcher && !led_active {
            self.watch_timer += dt;
            if let Some((_, near)) = nearest {
                let mut to = self.pos.local_delta_to(near.pos);
                to.y = 0.0;
                let d2 = to.length_squared();
                if d2 > 0.01 {
                    let dir = to.normalize();
                    self.yaw = dir.x.atan2(dir.z);
                    if d2 < 8.0 * 8.0 {
                        wish = -dir * def.speed * 0.4;
                    }
                }
            }
            self.state = MobState::Idle;
            self.state_timer = self.state_timer.max(0.5);
            led_active = true; // skip the state machine below
        }
        // A hungry predator with quarry in sight goes stalking (short
        // of fleeing, being led, or already hunting the player).
        if !led_active
            && !def.prey.is_empty()
            && def.belly_secs > 0.0
            && self.belly <= 0.0
            && self.growth >= 1.0
            && self.quarry.is_some()
            && !matches!(
                self.state,
                MobState::Flee | MobState::Stalk | MobState::Hunt
            )
        {
            self.state = MobState::Stalk;
            self.state_timer = 18.0;
        }
        // Capability E15: guards enter Stalk when a hostile is nearby,
        // regardless of hunger or prey lists.
        if !led_active
            && def.guards
            && !def.hostile
            && self.quarry.is_some()
            && !matches!(
                self.state,
                MobState::Stalk | MobState::Hunt | MobState::Flee
            )
        {
            self.state = MobState::Stalk;
            self.state_timer = 30.0;
        }
        // A hungry grazer drops what it was doing (short of fleeing or
        // being led) and walks to the nearest richest plant — which,
        // beside a farm, will tend to be the farm.
        if !led_active
            && def.grazes
            && def.belly_secs > 0.0
            && self.belly <= 0.0
            && self.growth >= 1.0
            && !matches!(self.state, MobState::Flee | MobState::Graze)
        {
            match self.scan_food(world) {
                Some(meal) => {
                    self.target = EntityPos::new(
                        meal.face(),
                        meal.u() as f32 + 0.5,
                        meal.y() as f32,
                        meal.v() as f32 + 0.5,
                    )
                    .expect("meal cell center is canonical");
                    self.state = MobState::Graze;
                    self.state_timer = 12.0;
                }
                // Nothing to eat in sight: try again in a while.
                None => self.belly = 45.0,
            }
        }
        // Spec 3.6: an archetype may fully handle the tick (a hacked
        // construct freezes) or add its own rhythm (a builder stamps)
        // before the Standard state machine runs.
        let archetype_handled = self.archetype_tick(world, def, dt, rng, events);
        // State transitions + wish velocity.
        if !led_active && !archetype_handled {
            match self.state {
                MobState::Idle => {
                    if self.state_timer <= 0.0 {
                        // Wings don't loiter. A bird that stops to stand
                        // about in mid-air is the tell that it is a box
                        // on a spring — so a flier always takes another
                        // leg, and a long one.
                        if def.movement_float || r01(rng) < 0.6 {
                            let ang = r01(rng) * std::f32::consts::TAU;
                            let dist = if def.movement_float {
                                14.0 + r01(rng) * 26.0
                            } else {
                                4.0 + r01(rng) * 6.0
                            };
                            let dy = if def.movement_swim {
                                (r01(rng) - 0.5) * 2.0
                            } else {
                                0.0
                            };
                            let mut delta = Vec3::new(ang.sin() * dist, dy, ang.cos() * dist);
                            // Herd animals lean homeward: wander picks
                            // drift toward the group's center when it
                            // has drifted away (no flocking math).
                            if let Some(h) = self.herd_pull {
                                let mut to = self.pos.local_delta_to(h);
                                to.y = 0.0;
                                if to.length_squared() > 36.0 {
                                    delta += to * 0.6;
                                }
                            }
                            self.target = self
                                .pos
                                .translated(delta)
                                .map(|moved| moved.pos)
                                .unwrap_or(self.pos);
                            self.state = MobState::Wander;
                            // Long enough to actually arrive: a 40-block
                            // crossing at cruise takes more than six.
                            self.state_timer = if def.movement_float { 16.0 } else { 6.0 };
                        } else {
                            self.state_timer = 1.5 + r01(rng) * 3.0;
                            self.yaw += (r01(rng) - 0.5) * 1.2;
                        }
                    }
                }
                MobState::Wander => {
                    let mut to = self.pos.local_delta_to(self.target);
                    to.y = 0.0;
                    if to.length_squared() < 0.6 || self.state_timer <= 0.0 {
                        self.state = MobState::Idle;
                        self.state_timer = if def.movement_float {
                            0.0 // straight into the next leg
                        } else {
                            2.0 + r01(rng) * 3.0
                        };
                    } else {
                        let dir = to.normalize();
                        self.yaw = dir.x.atan2(dir.z);
                        // Landfolk don't wander into deep water;
                        // swimmers don't wander OUT of it. Wings mind
                        // neither — a gull turned back at the shoreline
                        // because it read the sea as a landfolk's wall.
                        let probe = self
                            .pos
                            .translated(dir * 1.2)
                            .map(|moved| moved.pos)
                            .unwrap_or(self.pos);
                        let py = self.pos.y.floor() as i32;
                        let blocked = if def.movement_float {
                            false
                        } else if def.movement_swim {
                            !world.reg.is_water(block_at_height(world, probe, py))
                        } else {
                            world.reg.is_water(block_at_height(world, probe, py - 1))
                                && world.reg.is_water(block_at_height(world, probe, py - 2))
                        };
                        if blocked {
                            self.state = MobState::Idle;
                            self.state_timer = 1.0;
                        } else {
                            // Cruising speed, not a stroll: the 0.6 is a
                            // grazer's amble and it made eagles crawl.
                            wish = dir * def.speed * if def.movement_float { 1.0 } else { 0.6 };
                        }
                    }
                }
                MobState::Flee => {
                    if self.state_timer <= 0.0 {
                        self.state = MobState::Idle;
                        self.state_timer = 1.0 + r01(rng) * 2.0;
                    } else {
                        let mut away = -self.pos.local_delta_to(self.target);
                        away.y = 0.0;
                        let dir = if away.length_squared() > 0.001 {
                            away.normalize()
                        } else {
                            Vec3::new(self.yaw.sin(), 0.0, self.yaw.cos())
                        };
                        self.yaw = dir.x.atan2(dir.z);
                        wish = dir * def.speed * 1.6;
                    }
                }
                MobState::Graze => {
                    let mut to = self.pos.local_delta_to(self.target);
                    to.y = 0.0;
                    let flat = to.length();
                    let dy = self.target.y - self.pos.y;
                    if flat < 1.1 && dy.abs() < 1.6 {
                        // The bite. The world applies the plant's side;
                        // the animal trusts its own mouth.
                        if let Some(meal) = self.target.block() {
                            events.push(MobEvent::Ate(meal));
                        }
                        self.belly = def.belly_secs;
                        self.digest = 60.0 + (self.id % 45) as f32;
                        self.state = MobState::Idle;
                        self.state_timer = 2.0 + r01(rng) * 2.0;
                    } else if self.state_timer <= 0.0 {
                        // Fenced out or lost: give up, stay peckish.
                        self.belly = 60.0;
                        self.state = MobState::Idle;
                        self.state_timer = 1.5;
                    } else {
                        let dir = to.normalize_or_zero();
                        self.yaw = dir.x.atan2(dir.z);
                        wish = dir * def.speed * 0.7;
                    }
                }
                MobState::Stalk => match self.quarry {
                    None => {
                        // Prey died, fled the range, or was eaten
                        // first: stay peckish, try again soon.
                        self.belly = self.belly.max(45.0f32.min(def.belly_secs));
                        self.state = MobState::Idle;
                        self.state_timer = 1.5;
                    }
                    Some((prey_id, at)) => {
                        self.target = at;
                        let mut to = self.pos.local_delta_to(at);
                        let dy = to.y;
                        to.y = 0.0;
                        let flat = to.length();
                        if flat < def.half_w + 0.9 && dy.abs() < 2.0 {
                            if def.guards {
                                // Combat: deal damage per swing instead of
                                // instant-killing. The target retaliates via
                                // its own hurt() retaliation path.
                                if self.state_timer <= 0.0 {
                                    events.push(MobEvent::HitMob {
                                        id: prey_id,
                                        dmg: def.attack,
                                        dmg_type: None,
                                        from: self.pos,
                                    });
                                    self.state_timer = 1.5;
                                } else {
                                    // Hold position between swings.
                                    wish = Vec3::ZERO;
                                }
                            } else {
                                // The kill. The world turns the prey into
                                // a carcass; the hunter eats first.
                                events.push(MobEvent::Killed(prey_id));
                                self.belly = def.belly_secs;
                                self.digest = 90.0 + (self.id % 45) as f32;
                            }
                            self.state = MobState::Idle;
                            self.state_timer = if def.guards { 1.5 } else { 3.0 };
                        } else if self.state_timer <= 0.0 {
                            self.belly = 60.0;
                            self.state = MobState::Idle;
                            self.state_timer = 2.0;
                        } else {
                            let dir = to.normalize_or_zero();
                            self.yaw = dir.x.atan2(dir.z);
                            // The pounce pace: faster than a walk,
                            // slower than blind panic.
                            wish = dir * def.speed * 1.5;
                        }
                    }
                },
                MobState::Hunt => match prey {
                    None => {
                        self.state = MobState::Idle;
                        self.state_timer = 1.0;
                    }
                    Some((who, p)) => {
                        let mut to = self.pos.local_delta_to(p.pos);
                        let dist = to.length();
                        to.y = 0.0;
                        let dir = if to.length_squared() > 0.001 {
                            to.normalize()
                        } else {
                            Vec3::Z
                        };
                        self.yaw = dir.x.atan2(dir.z);
                        // Losing everyone for ~8 s ends the hunt.
                        if dist > def.aggro_range * 1.6 {
                            self.lose_aggro += dt;
                            if self.lose_aggro > 8.0 {
                                self.state = MobState::Idle;
                                self.state_timer = 1.0;
                            }
                        } else {
                            self.lose_aggro = 0.0;
                        }
                        self.hunt_wheel(world, def, who, p.pos, dt, events, dir, dist, &mut wish);
                    }
                },
            }
        }

        // Physics: accelerate toward wish, gravity/buoyancy, collide per axis.
        let accel = if def.movement_float || self.on_ground {
            14.0
        } else {
            4.0
        };
        let step = (accel * dt).min(1.0);
        self.vel.x += (wish.x - self.vel.x) * step;
        self.vel.z += (wish.z - self.vel.z) * step;

        if def.movement_swim {
            // Fish: neutral inside the water, helpless out of it. A
            // swimmer drifts toward its wander target's depth; a
            // beached one flops shoreward in little hops.
            let here = block_at_height(world, self.pos, (self.pos.y + 0.2).floor() as i32);
            if world.reg.is_water(here) {
                let want = (self.target.y - self.pos.y).clamp(-1.2, 1.2);
                self.vel.y += (want - self.vel.y) * step;
            } else {
                self.vel.y -= GRAVITY * dt;
                self.vel.y = self.vel.y.max(-TERMINAL);
                if self.on_ground {
                    self.vel.y = 4.0; // the flop
                }
            }
            self.anim_phase += dt * 3.0;
        } else if def.movement_float {
            // Floaters hover: seek a bobbing height above the ground,
            // no gravity at all. The ground that matters is the one
            // directly under them, not the world's surface height —
            // reading the latter told a bat 30 blocks down to climb to
            // daylight, so it spent its life pressed into the cave
            // roof, which is exactly what "hovering in place" looked
            // like from below.
            let (floor, ceil) = world.air_column_at(self.pos, self.pos.y.floor() as i32);
            let want_y = if self.state == MobState::Hunt {
                prey.map(|(_, p)| p.pos.y).unwrap_or(floor as f32) + 1.6
            } else if self.state == MobState::Stalk {
                // The dive: an eagle takes its quarry on the ground.
                self.quarry.map(|(_, at)| at.y).unwrap_or(floor as f32) + 0.5
            } else {
                // Wings cruise; a wisp drifts at head height.
                floor as f32 + if def.winged { 11.0 } else { 2.2 }
            } + (self.anim_phase * 0.2).sin() * 0.3;
            // Never above the roof: a low cave keeps its bats low.
            let want_y = want_y.min(ceil as f32 - 1.2);
            let vy = (want_y - self.pos.y).clamp(-2.5, 2.5);
            self.vel.y += (vy - self.vel.y) * step;
            // Fast enough to be a wingbeat; the bob divides it back down.
            self.anim_phase += dt * 7.0;
        } else {
            let feet = block_at_height(world, self.pos, (self.pos.y + 0.3).floor() as i32);
            if world.reg.is_water(feet) {
                // Bob to the surface rather than drowning.
                self.vel.y += (2.0 - self.vel.y).min(20.0 * dt);
            } else {
                self.vel.y -= GRAVITY * dt;
                self.vel.y = self.vel.y.max(-TERMINAL);
            }
        }

        let d = self.vel * dt;
        self.on_ground = false;
        self.hit_wall = false;
        self.move_axis(world, def, Vec3::new(d.x, 0.0, 0.0));
        self.move_axis(world, def, Vec3::new(0.0, 0.0, d.z));
        self.move_axis(world, def, Vec3::new(0.0, d.y, 0.0));

        // Auto-jump a 1-block step when walking into a wall — with
        // manners. The raid never climbs, and in player-touched
        // country a CALM animal minds the walls too (pens hold at one
        // block high). Panic is different: a fleeing animal will bolt
        // clean over the fence, so keep your livestock calm.
        if !def.movement_float
            && self.hit_wall
            && self.on_ground
            && wish.length_squared() > 0.01
            && self.dash.is_none()
        {
            let panicking = matches!(self.state, MobState::Flee | MobState::Hunt);
            let tended = self
                .pos
                .chunk()
                .is_some_and(|chunk| world.player_touched.contains(&chunk));
            if panicking || (!tended && self.state != MobState::Graze) {
                self.vel.y = JUMP;
            }
        }

        // Legs swing with horizontal travel.
        let hspeed = Vec3::new(self.vel.x, 0.0, self.vel.z).length();
        self.anim_phase += hspeed * dt * 3.2;
    }

    /// The spec 3.6 attack wheel. Approaches, holds, or charges, picking
    /// the first ready attack whose range condition trips and honoring
    /// each attack's own cooldown. The synthesized single-melee list
    /// reproduces the legacy warden swing exactly, so Standard behavior
    /// is unchanged.
    #[allow(clippy::too_many_arguments)]
    fn hunt_wheel(
        &mut self,
        world: &World,
        def: &AnimalDef,
        who: usize,
        target: EntityPos,
        dt: f32,
        events: &mut Vec<MobEvent>,
        dir: Vec3,
        dist: f32,
        wish: &mut Vec3,
    ) {
        if self.attack_cds.len() != def.attacks.len() {
            self.attack_cds = def
                .attacks
                .iter()
                .map(|a| {
                    if a.kind == AttackKind::Projectile {
                        1.0
                    } else {
                        0.0
                    }
                })
                .collect();
        }
        // A dash in flight: move straight along the frozen line.
        if let Some((left, d)) = self.dash {
            self.yaw = self.charge_yaw;
            let step = def.speed * 3.0 * dt;
            if left - step <= 0.0 || self.hit_wall {
                self.dash = None;
                // The charge lands where the dash ends: only a target the
                // line still touches takes its hit.
                if dist < def.half_w + 1.0
                    && let Some(atk) = def.attacks.iter().find(|a| a.kind == AttackKind::Charge)
                {
                    events.push(MobEvent::HitPlayer {
                        who,
                        dmg: atk.damage,
                        dmg_type: atk.damage_type.clone(),
                        attack: atk.name.clone(),
                        from: self.pos,
                    });
                }
            } else {
                self.dash = Some((left - step, d));
                *wish = d * def.speed * 3.0;
            }
            return;
        }
        if self.charge_wind_up > 0.0 {
            // Wind-up: rooted, facing the chosen line.
            self.yaw = self.charge_yaw;
            return;
        }
        let mut charged = false;
        for (i, atk) in def.attacks.iter().enumerate() {
            if self.attack_cds[i] > 0.0 {
                continue;
            }
            let dy = target.y - self.pos.y;
            match atk.kind {
                AttackKind::Melee => {
                    if dist < atk.range && dy.abs() < 2.0 {
                        self.attack_cds[i] = atk.cooldown;
                        events.push(MobEvent::HitPlayer {
                            who,
                            dmg: atk.damage,
                            dmg_type: atk.damage_type.clone(),
                            attack: atk.name.clone(),
                            from: self.pos,
                        });
                    }
                }
                AttackKind::Projectile => {
                    if dist < atk.range
                        && let Some(pr) = &atk.projectile
                    {
                        self.attack_cds[i] = atk.cooldown;
                        let muzzle = self
                            .pos
                            .translated(Vec3::new(0.0, def.height * 0.7, 0.0))
                            .expect("mob muzzle stays in its chart")
                            .pos;
                        let aim = (muzzle.local_delta_to(target) + Vec3::new(0.0, 0.9, 0.0))
                            .normalize_or_zero();
                        events.push(MobEvent::Cast(Projectile {
                            stable_id: 0,
                            pos: muzzle
                                .translated(aim * 0.6)
                                .expect("bolt starts beside its caster")
                                .pos,
                            vel: aim * pr.speed,
                            tile: pr.tile,
                            damage: pr.damage,
                            damage_type: pr.damage_type.clone(),
                            age: 0.0,
                            from_player: false,
                            drop_item: None,
                            preparation_payload: None,
                            owner: 0,
                        }));
                    }
                }
                AttackKind::Charge => {
                    if dist < atk.range && dist > def.half_w + 0.9 {
                        self.attack_cds[i] = atk.cooldown;
                        self.charge_dir = dir;
                        self.charge_yaw = dir.x.atan2(dir.z);
                        // The dash covers the ground to the target, so it
                        // stops where the charge can land.
                        self.charge_range = dist;
                        self.charge_wind_up = 0.4;
                        charged = true;
                    }
                }
            }
        }
        // E9: a phaser blinks into reach instead of closing on foot.
        if def.behavior == BehaviorArchetype::Phaser
            && self.phaser_blink(world, def, dt, target, dir, dist)
        {
            return;
        }
        // Movement: casters hold their band; everyone else closes in.
        // A charging mob keeps closing until the wind-up roots it.
        let caster = def.attacks.iter().any(|a| a.kind == AttackKind::Projectile);
        // E9: a sniper keeps its authored band (a caster with its own
        // parameters outranks the generic 11/5 band).
        if def.behavior == BehaviorArchetype::Sniper
            && let Some(sniper) = def.archetype.sniper.as_ref()
        {
            if dist > sniper.keep_max {
                *wish = dir * def.speed;
            } else if dist < sniper.keep_min {
                *wish = -dir * def.speed * 0.8;
            }
        } else if caster && !charged {
            if dist > 11.0 {
                *wish = dir * def.speed;
            } else if dist < 5.0 {
                *wish = -dir * def.speed * 0.8;
            }
        } else if dist > def.half_w + 0.9 {
            // E9: a rusher closes at a flat-out sprint.
            let mult = if def.behavior == BehaviorArchetype::Rusher {
                def.archetype.rusher.as_ref().map_or(2.4, |r| r.rush_mult)
            } else {
                1.2
            };
            *wish = dir * def.speed * mult;
        }
    }

    /// A phaser's blink (E9): while hunting, on its cooldown, it teleports
    /// to a valid spot just short of its target and strikes there. Returns
    /// true when it blinked, so the caller skips its walking wish.
    fn phaser_blink(
        &mut self,
        world: &World,
        def: &AnimalDef,
        dt: f32,
        target: EntityPos,
        dir: Vec3,
        dist: f32,
    ) -> bool {
        let Some(phaser) = def.archetype.phaser.as_ref() else {
            return false;
        };
        if self.phaser_cd > 0.0 {
            self.phaser_cd = (self.phaser_cd - dt).max(0.0);
            return false;
        }
        // Only blink when outside melee reach but inside the blink range.
        if dist < def.half_w + 1.2 || dist > phaser.blink_range {
            return false;
        }
        let step = (dist - (def.half_w + 0.7)).max(1.0);
        let probe = self
            .pos
            .translated(dir * step)
            .map(|moved| moved.pos)
            .unwrap_or(self.pos);
        if self.collides(world, def, probe) {
            return false;
        }
        self.pos = probe;
        self.phaser_cd = phaser.blink_cd;
        let _ = target;
        true
    }

    /// Spec 3.6 archetype hooks that run before the Standard state
    /// machine. Returns true when the archetype fully handled this tick.
    /// Standard never handles, so its pipeline is byte-for-byte intact.
    fn archetype_tick(
        &mut self,
        world: &World,
        def: &AnimalDef,
        dt: f32,
        rng: &mut u32,
        events: &mut Vec<MobEvent>,
    ) -> bool {
        match def.behavior {
            BehaviorArchetype::Construct => {
                // A hacked construct is inert: it stands, never fights,
                // never flees, and its wheel never fires.
                if self.hacked {
                    self.state = MobState::Idle;
                    self.state_timer = 1.0;
                    return true;
                }
                false
            }
            BehaviorArchetype::Builder => {
                self.builder_dispatch(world, def, dt, rng, events);
                false
            }
            BehaviorArchetype::Support => {
                self.support_pulse(world, def, dt, events);
                false
            }
            BehaviorArchetype::Controller => {
                self.controller_summon(world, def, dt, events);
                false
            }
            _ => false,
        }
    }

    /// A support's heal pulse (E9): every `interval` seconds, while it is
    /// not fleeing, it heals its hostile allies within `radius`.
    fn support_pulse(
        &mut self,
        world: &World,
        def: &AnimalDef,
        dt: f32,
        events: &mut Vec<MobEvent>,
    ) {
        let Some(support) = def.archetype.support.as_ref() else {
            return;
        };
        if self.support_cd > 0.0 {
            self.support_cd = (self.support_cd - dt).max(0.0);
            return;
        }
        if matches!(self.state, MobState::Flee) {
            self.support_cd = 2.0;
            return;
        }
        self.support_cd = support.interval;
        events.push(MobEvent::HealPulse {
            origin: self.pos,
            radius: support.radius,
            heal: support.heal,
        });
        let _ = world;
    }

    /// A controller's summon (E9): every `interval` seconds, while it is
    /// not fleeing and fewer than `max` of its companions live within
    /// `radius`, it calls `count` more.
    fn controller_summon(
        &mut self,
        world: &World,
        def: &AnimalDef,
        dt: f32,
        events: &mut Vec<MobEvent>,
    ) {
        let Some(controller) = def.archetype.controller.as_ref() else {
            return;
        };
        let Some(species) = world.reg.animal_id(&controller.spawn) else {
            self.controller_cd = 60.0;
            return;
        };
        if self.controller_cd > 0.0 {
            self.controller_cd = (self.controller_cd - dt).max(0.0);
            return;
        }
        if matches!(self.state, MobState::Flee) {
            self.controller_cd = 3.0;
            return;
        }
        let radius = def.aggro_range.max(12.0);
        let living = world
            .mobs()
            .iter()
            .filter(|m| m.species == species && m.pos.local_delta_to(self.pos).length() <= radius)
            .count();
        if living >= controller.max as usize {
            self.controller_cd = controller.interval;
            return;
        }
        self.controller_cd = controller.interval;
        events.push(MobEvent::SpawnMinions {
            pos: self.pos,
            species,
            count: controller.count,
        });
    }

    /// A builder stamps its template on a cooldown, up to its cap,
    /// whenever it is not fleeing for its life. The world applies the
    /// cells; the count caps the total.
    fn builder_dispatch(
        &mut self,
        world: &World,
        def: &AnimalDef,
        dt: f32,
        rng: &mut u32,
        events: &mut Vec<MobEvent>,
    ) {
        self.build_cd = (self.build_cd - dt).max(0.0);
        let Some(builder) = &def.builder else {
            return;
        };
        let Some(template) = builder.template.as_deref() else {
            return;
        };
        if self.build_cd > 0.0 || self.built_count >= builder.cap {
            return;
        }
        if matches!(
            self.state,
            MobState::Flee | MobState::Stalk | MobState::Hunt
        ) {
            return;
        }
        // Templates live on the world registry; if the authored name is
        // missing, back off for a long while rather than spamming.
        if world.template(template).is_none() {
            self.build_cd = 120.0;
            return;
        }
        self.build_cd = builder.interval;
        self.built_count += 1;
        // Stamp a short distance in a random horizontal direction, at
        // the builder's own ground line, in the chart's local frame.
        let ang = r01(rng) * std::f32::consts::TAU;
        let (sin, cos) = ang.sin_cos();
        let du = (cos * 3.0).round() as i32;
        let dv = (sin * 3.0).round() as i32;
        let y = self.pos.y.floor() as i32;
        let Some(anchor) = self
            .pos
            .block()
            .and_then(|c| c.offset(du, y - c.y() as i32, dv))
        else {
            self.build_cd = 5.0;
            return;
        };
        events.push(MobEvent::Build {
            template: template.to_string(),
            anchor,
            rot: crate::world::multiblock::Rotation::R0,
        });
    }

    /// The nearest richest plant meal within grazing range: a grown
    /// crop out-scores a fruited bush out-scores wild grass — so an
    /// unfenced field beside the woods is exactly the invitation it
    /// looks like. Returns the meal's cell.
    fn scan_food(&self, world: &World) -> Option<crate::planet::BlockPos> {
        const RANGE: i32 = 12;
        let py = self.pos.y.floor() as i32;
        let center = crate::planet::SurfacePos::new(
            self.pos.face(),
            self.pos.u().floor() as u16,
            self.pos.v().floor() as u16,
        )
        .expect("canonical mob has a valid surface cell");
        let reg = &world.reg;
        let mut best: Option<(crate::planet::BlockPos, i32, i32)> = None;
        for dx in -RANGE..=RANGE {
            for dz in -RANGE..=RANGE {
                for dy in -2..=2i32 {
                    let y = py + dy;
                    if !(0..crate::chunk::CHUNK_Y as i32 - 1).contains(&y) {
                        continue;
                    }
                    let Ok(surface) = crate::planet::SurfacePos::canonicalized(
                        center.face(),
                        center.u() as i32 + dx,
                        center.v() as i32 + dz,
                    ) else {
                        continue;
                    };
                    let cell = crate::planet::BlockPos::new(
                        surface.face(),
                        surface.u(),
                        y as u8,
                        surface.v(),
                    )
                    .expect("food scan coordinates were validated");
                    let b = world.get_block_at(cell);
                    let d = reg.block(b);
                    let richness = if d.crop_family != 0 && d.name.contains("/stage") {
                        // A grown crop: ripe beats growing.
                        if d.crop_next.is_none() { 4 } else { 3 }
                    } else if d.name == "base:grass"
                        && world.get_block_at(cell.with_y((y + 1) as u8)) == crate::registry::AIR
                    {
                        1
                    } else if matches!(
                        d.name.as_str(),
                        "base:rainbell"
                            | "base:lantern_reed"
                            | "base:lantern_reed_dim"
                            | "base:tidekelp"
                    ) {
                        // Existing herbivores accept wet, ordinary-tissue
                        // confluence forage. They avoid dross binders,
                        // storm-charged vines, crystals, and fire followers;
                        // magical habitat is not a universal animal buffet.
                        2
                    } else {
                        0
                    };
                    if richness == 0 {
                        continue;
                    }
                    let dist2 = dx * dx + dy * dy + dz * dz;
                    let better = match best {
                        None => true,
                        Some((_, r, d2)) => richness > r || (richness == r && dist2 < d2),
                    };
                    if better {
                        best = Some((cell, richness, dist2));
                    }
                }
            }
        }
        best.map(|(c, _, _)| c)
    }

    fn collides(&self, world: &World, def: &AnimalDef, pos: EntityPos) -> bool {
        let local = pos.chart_local();
        let min = local - Vec3::new(def.half_w, 0.0, def.half_w);
        let max = local + Vec3::new(def.half_w, def.height, def.half_w);
        let (x0, x1) = (min.x.floor() as i32, max.x.floor() as i32);
        let (y0, y1) = (min.y.floor() as i32, max.y.floor() as i32);
        let (z0, z1) = (min.z.floor() as i32, max.z.floor() as i32);
        for x in x0..=x1 {
            for y in y0..=y1 {
                for z in z0..=z1 {
                    let Ok(surface) = crate::planet::SurfacePos::canonicalized(pos.face(), x, z)
                    else {
                        return true;
                    };
                    if !(0..crate::chunk::CHUNK_Y as i32).contains(&y) {
                        return true;
                    }
                    let cell = crate::planet::BlockPos::new(
                        surface.face(),
                        surface.u(),
                        y as u8,
                        surface.v(),
                    )
                    .expect("collision sample was vertically bounded");
                    if world.reg.is_solid(world.get_block_at(cell)) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn move_axis(&mut self, world: &World, def: &AnimalDef, delta: Vec3) {
        let Ok(target) = self.pos.translated(delta) else {
            return;
        };
        if !self.collides(world, def, target.pos) {
            self.pos = target.pos;
            self.vel = target.rotation.rotate_vec3(self.vel);
            self.yaw = target.rotation.rotate_yaw(self.yaw);
            if target.rotation != crate::planet::QuarterTurn::IDENTITY {
                self.target = self.pos;
            }
            return;
        }
        let mut lo = 0.0f32;
        let mut hi = 1.0f32;
        for _ in 0..8 {
            let mid = (lo + hi) * 0.5;
            let candidate = self
                .pos
                .translated(delta * mid)
                .map(|moved| moved.pos)
                .unwrap_or(self.pos);
            if self.collides(world, def, candidate) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        if let Ok(moved) = self.pos.translated(delta * lo) {
            self.pos = moved.pos;
            self.vel = moved.rotation.rotate_vec3(self.vel);
            self.yaw = moved.rotation.rotate_yaw(self.yaw);
            if moved.rotation != crate::planet::QuarterTurn::IDENTITY {
                self.target = self.pos;
            }
        }
        if delta.y < 0.0 {
            self.on_ground = true;
        }
        if delta.x != 0.0 || delta.z != 0.0 {
            self.hit_wall = true;
        }
        if delta.x != 0.0 {
            self.vel.x = 0.0;
        }
        if delta.y != 0.0 {
            self.vel.y = 0.0;
        }
        if delta.z != 0.0 {
            self.vel.z = 0.0;
        }
    }

    /// Ray vs this mob's collision AABB (slab test); returns hit distance.
    pub fn ray_hit_from(
        &self,
        def: &AnimalDef,
        origin: EntityPos,
        dir: Vec3,
        max_t: f32,
    ) -> Option<f32> {
        let center = origin.local_delta_to(self.pos);
        let min = center - Vec3::new(def.half_w, 0.0, def.half_w);
        let max = center + Vec3::new(def.half_w, def.height, def.half_w);
        let mut t0 = 0.0f32;
        let mut t1 = max_t;
        for axis in 0..3 {
            let (direction, lo, hi) = (dir[axis], min[axis], max[axis]);
            if direction.abs() < 1e-6 {
                if 0.0 < lo || 0.0 > hi {
                    return None;
                }
                continue;
            }
            let inverse = 1.0 / direction;
            let (mut near, mut far) = (lo * inverse, hi * inverse);
            if near > far {
                std::mem::swap(&mut near, &mut far);
            }
            t0 = t0.max(near);
            t1 = t1.min(far);
            if t0 > t1 {
                return None;
            }
        }
        Some(t0)
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub fn ray_hit(&self, def: &AnimalDef, origin: Vec3, dir: Vec3, max_t: f32) -> Option<f32> {
        let min = self.pos - Vec3::new(def.half_w, 0.0, def.half_w);
        let max = self.pos + Vec3::new(def.half_w, def.height, def.half_w);
        let mut t0 = 0.0f32;
        let mut t1 = max_t;
        for a in 0..3 {
            let (o, d, lo, hi) = (origin[a], dir[a], min[a], max[a]);
            if d.abs() < 1e-6 {
                if o < lo || o > hi {
                    return None;
                }
                continue;
            }
            let inv = 1.0 / d;
            let (mut ta, mut tb) = ((lo - o) * inv, (hi - o) * inv);
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            t0 = t0.max(ta);
            t1 = t1.min(tb);
            if t0 > t1 {
                return None;
            }
        }
        Some(t0)
    }

    /// Append this mob's boxy model to the entity mesh.
    pub fn emit(
        &self,
        reg: &Registry,
        lum: ([f32; 3], f32),
        verts: &mut Vec<Vertex>,
        idx: &mut Vec<u32>,
    ) {
        let def = &reg.animals[self.species];
        let origin = self.pos.render_pos();
        let frame = crate::planet::local_frame(self.pos.surface_point());
        let east = frame.east.as_vec3();
        let up = frame.up.as_vec3();
        let north = frame.north.as_vec3();
        // Emissive wardens are their own lantern.
        let lum = if def.emissive { ([1.0; 3], lum.1) } else { lum };
        // Models face -Z; motion forward is (sin yaw, cos yaw) = +Z at 0,
        // so render rotated by yaw + PI to keep the head leading.
        let (syaw, cyaw) = (self.yaw + std::f32::consts::PI).sin_cos();
        let amp =
            (Vec3::new(self.vel.x, 0.0, self.vel.z).length() / def.speed.max(0.1)).clamp(0.0, 1.0);
        let flash = 1.0 + self.hurt_flash * 2.4;

        // Holding station in the air is the most work a wing ever does,
        // so the beat is deepest when a bird is going nowhere and eases
        // into a glide as it picks up speed. Both wings rise together:
        // the roll is signed by which side of the body the box sits on.
        let beat = (self.anim_phase.sin() * (0.62 - 0.30 * amp)).max(-0.5);

        // A box named "leg" mirrors into 4; everything else draws once.
        #[allow(clippy::type_complexity)] // (size, at, is_head, pitch, roll, tex)
        let mut boxes: Vec<([f32; 3], [f32; 3], bool, f32, f32, Option<u16>)> = Vec::new();
        for b in &def.model {
            let is_head = b.name.starts_with("head");
            if b.name == "leg" {
                for (sx, sz) in [(1.0f32, 1.0f32), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
                    let at = [b.at[0] * sx, b.at[1], b.at[2] * sz];
                    // Diagonal pairs swing together.
                    let phase = if sx * sz > 0.0 {
                        0.0
                    } else {
                        std::f32::consts::PI
                    };
                    let swing = (self.anim_phase + phase).sin() * 0.55 * amp;
                    boxes.push((b.size, at, false, swing, 0.0, b.tile));
                }
            } else if b.name.starts_with("wing") {
                boxes.push((b.size, b.at, false, 0.0, beat * b.at[0].signum(), b.tile));
            } else {
                boxes.push((b.size, b.at, is_head, 0.0, 0.0, b.tile));
            }
        }

        let gs = 0.45 + 0.55 * self.growth.min(1.0); // babies are small
        for (size, at, is_head, swing, roll, tile_override) in boxes {
            let (hx, hy, hz) = (
                size[0] * gs / 32.0,
                size[1] * gs / 32.0,
                size[2] * gs / 32.0,
            );
            let center = Vec3::new(at[0] * gs / 16.0, at[1] * gs / 16.0 + hy, at[2] * gs / 16.0);
            // Legs rotate around their top (hip) on the local X axis.
            let pivot_y = at[1] * gs / 16.0 + hy * 2.0;
            let (ss, cs) = swing.sin_cos();
            // Wings rotate around their inner end (the shoulder) on the
            // local Z axis, so the tip sweeps and the root stays put.
            let pivot_x = center.x - center.x.signum() * hx;
            let (rs, rc) = roll.sin_cos();
            let ts = 1.0 / ATLAS_TILES as f32;
            let inset = ts / 32.0;
            for face in 0..6 {
                // The face art goes only on the head's front (-Z); every
                // other surface is fur — a face on the back of a skull
                // reads as cursed.
                let tile = tile_override.unwrap_or(if is_head && face == 5 {
                    def.head_tile
                } else {
                    def.tile
                });
                let (tx, ty) = (tile as u32 % ATLAS_TILES, tile as u32 / ATLAS_TILES);
                // The face normal, through the same swing + yaw as the
                // verts. Emissive wardens stay normal-less: they are
                // their own lantern and shade would dim the glow.
                let normal = if def.emissive {
                    [0.0, 0.0, 0.0]
                } else {
                    let n = NORMALS[face];
                    let (mut nx, mut ny, mut nz) = (n[0] as f32, n[1] as f32, n[2] as f32);
                    if swing != 0.0 {
                        let (y0, z0) = (ny, nz);
                        ny = y0 * cs - z0 * ss;
                        nz = y0 * ss + z0 * cs;
                    }
                    if roll != 0.0 {
                        // Without this the beat is a silhouette only —
                        // the wing's shade would stay flat while it moves.
                        let (x0, y0) = (nx, ny);
                        nx = x0 * rc - y0 * rs;
                        ny = x0 * rs + y0 * rc;
                    }
                    let local = Vec3::new(nx * cyaw + nz * syaw, ny, -nx * syaw + nz * cyaw);
                    (east * local.x + up * local.y + north * local.z).to_array()
                };
                let base = verts.len() as u32;
                for c in CORNERS[face].iter() {
                    let mut lx = center.x + (c[0] - 0.5) * 2.0 * hx;
                    let mut ly = center.y + (c[1] - 0.5) * 2.0 * hy;
                    let mut lz = center.z + (c[2] - 0.5) * 2.0 * hz;
                    if swing != 0.0 {
                        let (dy, dz) = (ly - pivot_y, lz - center.z);
                        ly = pivot_y + dy * cs - dz * ss;
                        lz = center.z + dy * ss + dz * cs;
                    }
                    if roll != 0.0 {
                        let (dx, dy) = (lx - pivot_x, ly - center.y);
                        lx = pivot_x + dx * rc - dy * rs;
                        ly = center.y + dx * rs + dy * rc;
                    }
                    // Yaw the whole mob (model faces -Z forward → +yaw).
                    let wx = lx * cyaw + lz * syaw;
                    let wz = -lx * syaw + lz * cyaw;
                    let (u, v) = match face {
                        0 | 1 => (c[2], 1.0 - c[1]),
                        4 | 5 => (c[0], 1.0 - c[1]),
                        _ => (c[0], c[2]),
                    };
                    // Lit bodies hand the shader raw light; it applies
                    // the face shade from the normal. Emissive ones keep
                    // the old pre-shaded flat model.
                    let shade = if def.emissive {
                        FACE_SHADE[face].max(0.65) * flash
                    } else {
                        flash
                    };
                    let world = origin + east * wx + up * ly + north * wz;
                    verts.push(Vertex {
                        pos: world.to_array(),
                        uv: [
                            tx as f32 * ts + inset + u * (ts - 2.0 * inset),
                            ty as f32 * ts + inset + v * (ts - 2.0 * inset),
                        ],
                        normal,
                        light: [
                            (shade * lum.0[0]).min(2.0),
                            (shade * lum.0[1]).min(2.0),
                            (shade * lum.0[2]).min(2.0),
                        ],
                        sky: (shade * lum.1).min(2.0),
                        ao: 1.0,
                    });
                }
                idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
    }
}

/// Which tiles and shapes dress a humanoid — resolved from a
/// player's Style (pre-tinted variant slots; see style.rs).
pub struct HumanoidArt {
    pub skin: u16,
    pub face: u16,
    /// None = bald; otherwise the side-shell tile for the length.
    pub hair: Option<u16>,
    /// The shell's front (fringe) tile — short even when hair is long.
    pub hair_front: u16,
    pub hair_top: u16,
    /// None = clean-shaven; otherwise a face-band tile.
    pub beard: Option<u16>,
    pub shirt: u16,
    pub trousers: u16,
    pub boot: u16,
    /// Long hair adds a back panel to the collar.
    pub long_hair: bool,
    /// A knee-length skirt over leggings instead of bare trousers.
    pub skirt: bool,
    /// 0 slight, 1 standard, 2 broad — shoulder/arm width.
    pub build: u8,
}

/// What (if anything) the humanoid holds in its right hand.
#[derive(Clone, Copy)]
pub enum HeldArt {
    None,
    /// A placeable block: mini cube with the block's face tiles.
    Cube([u16; 6]),
    /// Anything else: the item's icon as a small sprite.
    Sprite(u16),
    /// Four visibly separate wand materials. `focus_shape` changes geometry,
    /// so focus identity remains readable without color vision.
    Wand {
        body: u16,
        reservoir: u16,
        focus: u16,
        binding: u16,
        focus_shape: u8,
        charge_band: u8,
    },
}

/// A player's body: Steve-proportioned boxes on a 16px-per-block
/// grid, ~29px (1.81 blocks) tall to match the hitbox. `gait` is
/// (phase, amplitude): legs and arms swing in opposition, hinged at
/// hip and shoulder. Hands are their own skin-toned boxes; hair is
/// an alpha-cut overlay box; the held item rides the right hand.
#[allow(clippy::too_many_arguments)]
pub fn emit_humanoid(
    pos: EntityPos,
    yaw: f32,
    art: &HumanoidArt,
    gait: (f32, f32),
    held: HeldArt,
    lum: ([f32; 3], f32),
    verts: &mut Vec<Vertex>,
    idx: &mut Vec<u32>,
) {
    emit_humanoid_interpolated(pos, pos.render_pos(), yaw, art, gait, held, lum, verts, idx);
}

/// Render a logical planetary actor at a separately interpolated embedded
/// origin. The logical address supplies its continuously rotating tangent
/// frame; the embedded origin supplies snapshot smoothing. Keeping those
/// concerns separate prevents remote players from snapping at cube-face
/// seams without ever making them stand in the global Y direction.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_humanoid_interpolated(
    pos: EntityPos,
    origin: Vec3,
    yaw: f32,
    art: &HumanoidArt,
    gait: (f32, f32),
    held: HeldArt,
    lum: ([f32; 3], f32),
    verts: &mut Vec<Vertex>,
    idx: &mut Vec<u32>,
) {
    let frame = crate::planet::local_frame(pos.surface_point());
    let east = frame.east.as_vec3();
    let up = frame.up.as_vec3();
    let north = frame.north.as_vec3();
    let (syaw, cyaw) = (yaw + std::f32::consts::PI).sin_cos();
    let (phase, amp) = gait;
    let leg = phase.sin() * 0.55 * amp;
    let arm = -phase.sin() * 0.45 * amp;
    let ts = 1.0 / ATLAS_TILES as f32;
    let inset = ts / 32.0;

    // Build sets the silhouette: torso and arm width.
    let tw = [7.2f32, 8.0, 9.0][art.build.min(2) as usize];
    let aw = [2.6f32, 3.0, 3.5][art.build.min(2) as usize];
    // Sink the shoulder slightly into the torso instead of joining two
    // independently rasterized boxes on one mathematically exact edge. The
    // overlap prevents a daylight crack at rest and while the arm pivots.
    let ax = (tw + aw) / 2.0 - 0.35;

    // (size px, base [x, y_base, z], tiles, swing rad, pivot y px)
    struct Part {
        size: [f32; 3],
        at: [f32; 3],
        tiles: [u16; 6],
        swing: f32,
        pivot: f32,
        skip_bottom: bool,
        only_front: bool,
    }
    let all = |t: u16| [t; 6];
    let part = |size: [f32; 3], at: [f32; 3], tiles: [u16; 6], swing: f32, pivot: f32| Part {
        size,
        at,
        tiles,
        swing,
        pivot,
        skip_bottom: false,
        only_front: false,
    };
    let head_tiles = [art.skin, art.skin, art.skin, art.skin, art.skin, art.face];
    let mut parts = vec![
        // Boots and legs share the hip hinge so they swing as one limb.
        part([3.0, 3.0, 3.0], [-1.5, 0.0, 0.0], all(art.boot), leg, 12.0),
        part([3.0, 3.0, 3.0], [1.5, 0.0, 0.0], all(art.boot), -leg, 12.0),
        part(
            [3.0, 9.0, 3.0],
            [-1.5, 3.0, 0.0],
            all(art.trousers),
            leg,
            12.0,
        ),
        part(
            [3.0, 9.0, 3.0],
            [1.5, 3.0, 0.0],
            all(art.trousers),
            -leg,
            12.0,
        ),
        part([tw, 10.0, 4.0], [0.0, 12.0, 0.0], all(art.shirt), 0.0, 0.0),
        // Sleeves from the shoulder, skin-toned hands at their ends.
        part([aw, 9.0, 3.0], [-ax, 13.0, 0.0], all(art.shirt), -arm, 22.0),
        part([aw, 9.0, 3.0], [ax, 13.0, 0.0], all(art.shirt), arm, 22.0),
        part([aw, 3.0, 3.0], [-ax, 10.0, 0.0], all(art.skin), -arm, 22.0),
        part([aw, 3.0, 3.0], [ax, 10.0, 0.0], all(art.skin), arm, 22.0),
        part([7.0, 7.0, 7.0], [0.0, 22.0, 0.0], head_tiles, 0.0, 0.0),
    ];
    if art.skirt {
        // A knee-length flare over the leggings; legs swing beneath.
        parts.push(part(
            [tw + 1.5, 6.0, 5.5],
            [0.0, 6.0, 0.0],
            all(art.trousers),
            0.0,
            0.0,
        ));
    }
    if let Some(h) = art.hair {
        // Hair: a slightly inflated alpha-cut shell over the head.
        parts.push(Part {
            size: [7.7, 7.7, 7.7],
            at: [0.0, 21.8, 0.0],
            tiles: [h, h, art.hair_top, h, h, art.hair_front],
            swing: 0.0,
            pivot: 0.0,
            skip_bottom: true,
            only_front: false,
        });
        if art.long_hair {
            // The lengths fall behind the shoulders.
            parts.push(part([7.4, 8.0, 1.6], [0.0, 14.0, 2.9], all(h), 0.0, 0.0));
        }
    }
    if let Some(b) = art.beard {
        // A face band a hair proud of the head, front only - drawn in
        // face-tile coordinates so it sits on the mouth it belongs to.
        parts.push(Part {
            size: [7.4, 7.4, 7.4],
            at: [0.0, 21.9, -0.5],
            tiles: all(b),
            swing: 0.0,
            pivot: 0.0,
            skip_bottom: false,
            only_front: true,
        });
    }

    #[allow(clippy::too_many_arguments)]
    let mut emit_box = |size: [f32; 3],
                        at: [f32; 3],
                        tiles: [u16; 6],
                        swing: f32,
                        pivot_px: f32,
                        skip_bottom: bool,
                        only_front: bool| {
        let (hx, hy, hz) = (size[0] / 32.0, size[1] / 32.0, size[2] / 32.0);
        let center = Vec3::new(at[0] / 16.0, at[1] / 16.0 + hy, at[2] / 16.0);
        let pivot = pivot_px / 16.0;
        let (ss, cs) = swing.sin_cos();
        for f in 0..6 {
            if skip_bottom && f == 3 {
                continue;
            }
            if only_front && f != 5 {
                continue;
            }
            let t = tiles[f];
            let (tx, ty) = (t as u32 % ATLAS_TILES, t as u32 / ATLAS_TILES);
            let n = NORMALS[f];
            let (mut ny, mut nz) = (n[1] as f32, n[2] as f32);
            if swing != 0.0 {
                let (y0, z0) = (ny, nz);
                ny = y0 * cs - z0 * ss;
                nz = y0 * ss + z0 * cs;
            }
            let nx = n[0] as f32;
            let local_normal = Vec3::new(nx * cyaw + nz * syaw, ny, -nx * syaw + nz * cyaw);
            let normal =
                (east * local_normal.x + up * local_normal.y + north * local_normal.z).to_array();
            let base = verts.len() as u32;
            for c in CORNERS[f].iter() {
                let lx = center.x + (c[0] - 0.5) * 2.0 * hx;
                let mut ly = center.y + (c[1] - 0.5) * 2.0 * hy;
                let mut lz = center.z + (c[2] - 0.5) * 2.0 * hz;
                if swing != 0.0 {
                    let (dy, dz) = (ly - pivot, lz - center.z);
                    ly = pivot + dy * cs - dz * ss;
                    lz = center.z + dy * ss + dz * cs;
                }
                let wx = lx * cyaw + lz * syaw;
                let wz = -lx * syaw + lz * cyaw;
                let (u, v) = match f {
                    0 | 1 => (c[2], 1.0 - c[1]),
                    4 | 5 => (c[0], 1.0 - c[1]),
                    _ => (c[0], c[2]),
                };
                let world = origin + east * wx + up * ly + north * wz;
                verts.push(Vertex {
                    pos: world.to_array(),
                    uv: [
                        tx as f32 * ts + inset + u * (ts - 2.0 * inset),
                        ty as f32 * ts + inset + v * (ts - 2.0 * inset),
                    ],
                    normal,
                    light: lum.0,
                    sky: lum.1,
                    ao: 1.0,
                });
            }
            idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    };

    for p in parts {
        emit_box(
            p.size,
            p.at,
            p.tiles,
            p.swing,
            p.pivot,
            p.skip_bottom,
            p.only_front,
        );
    }

    // The held item rides the right hand: swung by the arm, held a
    // touch forward of the palm (model forward is local -Z).
    let hand_center = {
        let pivot = 22.0 / 16.0;
        let (ss, cs) = arm.sin_cos();
        let (hy0, hz0) = (10.0 / 16.0 + 1.5 / 16.0, -3.5 / 16.0);
        let dy = hy0 - pivot;
        Vec3::new(ax / 16.0, pivot + dy * cs - hz0 * ss, dy * ss + hz0 * cs)
    };
    let mut emit_held_quad = |corners: [(Vec3, f32, f32); 4], slot: u16, glow: u8| {
        let (tx, ty) = (slot as u32 % ATLAS_TILES, slot as u32 / ATLAS_TILES);
        let base = verts.len() as u32;
        for (lp, u, v) in corners {
            let wx = lp.x * cyaw + lp.z * syaw;
            let wz = -lp.x * syaw + lp.z * cyaw;
            let world = origin + east * wx + up * lp.y + north * wz;
            verts.push(Vertex {
                pos: world.to_array(),
                uv: [
                    tx as f32 * ts + inset + u * (ts - 2.0 * inset),
                    ty as f32 * ts + inset + v * (ts - 2.0 * inset),
                ],
                normal: [0.0, 0.0, 0.0],
                light: [
                    (lum.0[0] + f32::from(glow) * 0.12).min(1.4),
                    (lum.0[1] + f32::from(glow) * 0.18).min(1.4),
                    (lum.0[2] + f32::from(glow) * 0.24).min(1.4),
                ],
                sky: lum.1,
                ao: 1.0,
            });
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };
    match held {
        HeldArt::None => {}
        HeldArt::Cube(tiles) => {
            let h = 2.2 / 16.0;
            for f in 0..6 {
                let (u0, v0, corners) = (0.0, 0.0, CORNERS[f]);
                let _ = (u0, v0);
                let quad = [0, 1, 2, 3].map(|i| {
                    let c = corners[i];
                    let lp = hand_center
                        + Vec3::new(
                            (c[0] - 0.5) * 2.0 * h,
                            (c[1] - 0.5) * 2.0 * h,
                            (c[2] - 0.5) * 2.0 * h,
                        );
                    let (u, v) = match f {
                        0 | 1 => (c[2], 1.0 - c[1]),
                        4 | 5 => (c[0], 1.0 - c[1]),
                        _ => (c[0], c[2]),
                    };
                    (lp, u, v)
                });
                emit_held_quad(quad, tiles[f], 0);
            }
        }
        HeldArt::Sprite(icon) => {
            let h = 4.5 / 16.0;
            for (dx, dz) in [(1.0f32, 0.0f32), (0.0, 1.0)] {
                for flip in [false, true] {
                    let sgn = if flip { -1.0 } else { 1.0 };
                    let quad = [
                        (
                            hand_center + Vec3::new(-h * sgn * dx, -h, -h * sgn * dz),
                            0.0,
                            1.0,
                        ),
                        (
                            hand_center + Vec3::new(h * sgn * dx, -h, h * sgn * dz),
                            1.0,
                            1.0,
                        ),
                        (
                            hand_center + Vec3::new(h * sgn * dx, h, h * sgn * dz),
                            1.0,
                            0.0,
                        ),
                        (
                            hand_center + Vec3::new(-h * sgn * dx, h, -h * sgn * dz),
                            0.0,
                            0.0,
                        ),
                    ];
                    emit_held_quad(quad, icon, 0);
                }
            }
        }
        HeldArt::Wand {
            body,
            reservoir,
            focus,
            binding,
            focus_shape,
            charge_band,
        } => {
            let mut panel = |center: Vec3, half_w: f32, half_h: f32, slot: u16, glow: u8| {
                for z in [-0.012, 0.012] {
                    emit_held_quad(
                        [
                            (center + Vec3::new(-half_w, -half_h, z), 0.0, 1.0),
                            (center + Vec3::new(half_w, -half_h, z), 1.0, 1.0),
                            (center + Vec3::new(half_w, half_h, z), 1.0, 0.0),
                            (center + Vec3::new(-half_w, half_h, z), 0.0, 0.0),
                        ],
                        slot,
                        glow,
                    );
                }
            };
            let shaft = hand_center + Vec3::new(0.0, 5.2 / 16.0, 0.0);
            panel(shaft, 0.75 / 16.0, 5.8 / 16.0, body, 0);
            panel(
                hand_center + Vec3::new(0.0, 3.0 / 16.0, 0.0),
                1.9 / 16.0,
                1.55 / 16.0,
                reservoir,
                charge_band.saturating_sub(1),
            );
            for y in [0.2 / 16.0, 6.0 / 16.0] {
                panel(
                    hand_center + Vec3::new(0.0, y, 0.0),
                    1.45 / 16.0,
                    0.45 / 16.0,
                    binding,
                    0,
                );
            }
            let tip = hand_center + Vec3::new(0.0, 12.0 / 16.0, 0.0);
            match focus_shape {
                1 => {
                    // A broad diamond/slate.
                    emit_held_quad(
                        [
                            (tip + Vec3::new(0.0, -1.5 / 16.0, 0.0), 0.5, 1.0),
                            (tip + Vec3::new(1.5 / 16.0, 0.0, 0.0), 1.0, 0.5),
                            (tip + Vec3::new(0.0, 1.5 / 16.0, 0.0), 0.5, 0.0),
                            (tip + Vec3::new(-1.5 / 16.0, 0.0, 0.0), 0.0, 0.5),
                        ],
                        focus,
                        charge_band,
                    );
                }
                2 => {
                    // Forked choirstone geometry.
                    panel(tip, 1.9 / 16.0, 0.42 / 16.0, focus, charge_band);
                    for x in [-1.4 / 16.0, 1.4 / 16.0] {
                        panel(
                            tip + Vec3::new(x, 0.95 / 16.0, 0.0),
                            0.38 / 16.0,
                            1.2 / 16.0,
                            focus,
                            charge_band,
                        );
                    }
                }
                3 => {
                    // Narrow wake-iron spearhead.
                    emit_held_quad(
                        [
                            (tip + Vec3::new(-0.95 / 16.0, -1.2 / 16.0, 0.0), 0.0, 1.0),
                            (tip + Vec3::new(0.95 / 16.0, -1.2 / 16.0, 0.0), 1.0, 1.0),
                            (tip + Vec3::new(0.0, 2.0 / 16.0, 0.0), 0.5, 0.0),
                            (tip + Vec3::new(0.0, 2.0 / 16.0, 0.0), 0.5, 0.0),
                        ],
                        focus,
                        charge_band,
                    );
                }
                _ => {
                    // Living/root crown.
                    panel(tip, 0.75 / 16.0, 1.15 / 16.0, focus, charge_band);
                    for x in [-1.2 / 16.0, 1.2 / 16.0] {
                        panel(
                            tip + Vec3::new(x, 0.75 / 16.0, 0.0),
                            0.32 / 16.0,
                            1.0 / 16.0,
                            focus,
                            charge_band,
                        );
                    }
                }
            }
        }
    }
}
