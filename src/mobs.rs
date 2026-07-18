//! Wildlife: boxy passive mobs with wander/flee AI, hunted for food.
//! Species are data (`registry::AnimalDef`); this module is the runtime —
//! movement, steering, rendering, and ray hits.

use glam::Vec3;

use crate::atlas::ATLAS_TILES;
use crate::mesher::{CORNERS, FACE_SHADE, Vertex};
use crate::registry::{AnimalDef, Registry};
use crate::server::PlayerCtx;
use crate::world::World;

const GRAVITY: f32 = 28.0;
const TERMINAL: f32 = 40.0;
const JUMP: f32 = 7.6;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MobState {
    Idle,
    Wander,
    Flee,
    /// Hostiles only: chase/attack the player.
    Hunt,
}

/// Things a mob did this tick that the game loop must apply.
pub enum MobEvent {
    /// Contact damage: (player index, half-hearts, attacker position).
    HitPlayer(usize, f32, Vec3),
    /// A caster fired: projectile spawn.
    Cast(Projectile),
    /// A wildlife pair bred at this position.
    Bred(Vec3),
}

/// A bolt in flight: warden thorn/ember/frost, or a player's arrow.
#[derive(Clone, Debug)]
pub struct Projectile {
    pub pos: Vec3,
    pub vel: Vec3,
    pub tile: u16,
    pub damage: f32,
    pub age: f32,
    /// Player arrows seek mobs; warden bolts seek the player.
    pub from_player: bool,
    /// Item recovered when this sticks into a block (arrows).
    pub drop_item: Option<crate::registry::ItemId>,
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
        self.pos += self.vel * dt;
        let b = world.get_block(
            self.pos.x.floor() as i32,
            self.pos.y.floor() as i32,
            self.pos.z.floor() as i32,
        );
        if world.reg.is_solid(b) {
            return ProjHit::Block;
        }
        if self.from_player {
            for (i, m) in world.mobs.iter().enumerate() {
                let Some(def) = world.reg.animals.get(m.species) else { continue };
                let d = self.pos - m.pos;
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
                let d = self.pos - (p.pos + Vec3::new(0.0, 0.9, 0.0));
                if d.x.abs() < 0.5 && d.z.abs() < 0.5 && d.y.abs() < 1.1 {
                    return ProjHit::Player(i);
                }
            }
        }
        ProjHit::None
    }

    /// Small spinning sprite, drawn with the entity pipeline.
    pub fn emit(&self, verts: &mut Vec<Vertex>, idx: &mut Vec<u32>) {
        let (tx, ty) = (self.tile as u32 % ATLAS_TILES, self.tile as u32 / ATLAS_TILES);
        let ts = 1.0 / ATLAS_TILES as f32;
        let inset = ts / 32.0;
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
                    verts.push(Vertex {
                        pos: [self.pos.x + dx * o, self.pos.y + y, self.pos.z + dz * o],
                        uv: [u, v],
                        normal: [0.0, 0.0, 0.0],
                        light: 1.0, // bolts glow faintly
                        sky: 1.0,
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
    pub pos: Vec3,
    pub vel: Vec3,
    /// Facing; forward = (sin yaw, 0, cos yaw).
    pub yaw: f32,
    pub health: f32,
    pub state: MobState,
    pub state_timer: f32,
    /// Wander destination, or the point we're fleeing from.
    pub target: Vec3,
    pub anim_phase: f32,
    pub hurt_flash: f32,
    pub on_ground: bool,
    hit_wall: bool,
    attack_cd: f32,
    cast_cd: f32,
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
}

fn r01(rng: &mut u32) -> f32 {
    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
    (*rng >> 8) as f32 / (1 << 24) as f32
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
    pub fn new(species: usize, pos: Vec3, yaw: f32) -> Mob {
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
            attack_cd: 0.0,
            cast_cd: 1.0,
            lose_aggro: 0.0,
            fed: false,
            calm: 0.0,
            breed_cd: 0.0,
            growth: 1.0,
            last_hit_by: 0,
        }
    }

    /// Take damage from an attacker at `from`: knockback, then panic
    /// (wildlife) or retaliation (wardens).
    pub fn hurt(&mut self, def: &AnimalDef, dmg: f32, from: Vec3) {
        self.health -= dmg;
        self.hurt_flash = 0.35;
        let mut away = self.pos - from;
        away.y = 0.0;
        let dir = if away.length_squared() > 0.001 { away.normalize() } else { Vec3::Z };
        let kb = if def.movement_float { 2.5 } else { 6.0 };
        self.vel += dir * kb + Vec3::new(0.0, if def.movement_float { 1.0 } else { 4.5 }, 0.0);
        if def.hostile {
            self.state = MobState::Hunt;
            self.state_timer = 10.0;
            self.lose_aggro = 0.0;
        } else {
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
        // The mob cares about whoever is closest (and, for hunting,
        // closest *attackable*).
        let nearest = players
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                (a.pos - self.pos)
                    .length_squared()
                    .total_cmp(&(b.pos - self.pos).length_squared())
            })
            .map(|(i, p)| (i, *p));
        let prey = players
            .iter()
            .enumerate()
            .filter(|(_, p)| p.attackable)
            .min_by(|(_, a), (_, b)| {
                (a.pos - self.pos)
                    .length_squared()
                    .total_cmp(&(b.pos - self.pos).length_squared())
            })
            .map(|(i, p)| (i, *p));
        self.state_timer -= dt;
        self.hurt_flash = (self.hurt_flash - dt).max(0.0);
        self.attack_cd = (self.attack_cd - dt).max(0.0);
        self.cast_cd = (self.cast_cd - dt).max(0.0);
        self.calm = (self.calm - dt).max(0.0);
        self.breed_cd = (self.breed_cd - dt).max(0.0);
        if self.growth < 1.0 {
            self.growth = (self.growth + dt / 1200.0).min(1.0);
        }

        // Skittish species bolt when anyone closes in — unless recently
        // fed (feeding is taming-lite).
        if let Some((_, near)) = nearest {
            if def.flee_range > 0.0
                && !def.hostile
                && self.calm <= 0.0
                && self.state != MobState::Flee
            {
                let mut d = near.pos - self.pos;
                d.y = 0.0;
                if d.length_squared() < def.flee_range * def.flee_range {
                    self.state = MobState::Flee;
                    self.state_timer = 4.0;
                    self.target = near.pos;
                }
            }
        }
        // Wardens take notice (the quiet charm shortens their attention).
        if let Some((_, p)) = prey {
            if def.hostile && self.state != MobState::Hunt {
                let range = (def.aggro_range + p.aggro_mod).max(2.0);
                if (p.pos - self.pos).length_squared() < range * range {
                    self.state = MobState::Hunt;
                    self.lose_aggro = 0.0;
                }
            }
        }

        // State transitions + wish velocity.
        let mut wish = Vec3::ZERO;
        match self.state {
            MobState::Idle => {
                if self.state_timer <= 0.0 {
                    if r01(rng) < 0.6 {
                        let ang = r01(rng) * std::f32::consts::TAU;
                        let dist = 4.0 + r01(rng) * 6.0;
                        self.target =
                            self.pos + Vec3::new(ang.sin() * dist, 0.0, ang.cos() * dist);
                        self.state = MobState::Wander;
                        self.state_timer = 6.0;
                    } else {
                        self.state_timer = 1.5 + r01(rng) * 3.0;
                        self.yaw += (r01(rng) - 0.5) * 1.2;
                    }
                }
            }
            MobState::Wander => {
                let mut to = self.target - self.pos;
                to.y = 0.0;
                if to.length_squared() < 0.6 || self.state_timer <= 0.0 {
                    self.state = MobState::Idle;
                    self.state_timer = 2.0 + r01(rng) * 3.0;
                } else {
                    let dir = to.normalize();
                    self.yaw = dir.x.atan2(dir.z);
                    // Don't wander into deep water: probe one block ahead.
                    let probe = self.pos + dir * 1.2;
                    let (px, pz) = (probe.x.floor() as i32, probe.z.floor() as i32);
                    let py = self.pos.y.floor() as i32;
                    let deep = world.reg.is_water(world.get_block(px, py - 1, pz))
                        && world.reg.is_water(world.get_block(px, py - 2, pz));
                    if deep {
                        self.state = MobState::Idle;
                        self.state_timer = 1.0;
                    } else {
                        wish = dir * def.speed * 0.6;
                    }
                }
            }
            MobState::Flee => {
                if self.state_timer <= 0.0 {
                    self.state = MobState::Idle;
                    self.state_timer = 1.0 + r01(rng) * 2.0;
                } else {
                    let mut away = self.pos - self.target;
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
            MobState::Hunt => match prey {
                None => {
                    self.state = MobState::Idle;
                    self.state_timer = 1.0;
                }
                Some((who, p)) => {
                    let mut to = p.pos - self.pos;
                    let dist = to.length();
                    to.y = 0.0;
                    let dir =
                        if to.length_squared() > 0.001 { to.normalize() } else { Vec3::Z };
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
                    match &def.projectile {
                        Some(pr) => {
                            // Casters hold their range and lob bolts.
                            if dist > 11.0 {
                                wish = dir * def.speed;
                            } else if dist < 5.0 {
                                wish = -dir * def.speed * 0.8;
                            }
                            if dist < 14.0 && self.cast_cd <= 0.0 {
                                self.cast_cd = pr.cooldown;
                                let muzzle =
                                    self.pos + Vec3::new(0.0, def.height * 0.7, 0.0);
                                let aim = (p.pos + Vec3::new(0.0, 0.9, 0.0) - muzzle)
                                    .normalize_or_zero();
                                events.push(MobEvent::Cast(Projectile {
                                    pos: muzzle + aim * 0.6,
                                    vel: aim * pr.speed,
                                    tile: pr.tile,
                                    damage: pr.damage,
                                    age: 0.0,
                                    from_player: false,
                                    drop_item: None,
                                    owner: 0,
                                }));
                            }
                        }
                        None => {
                            wish = dir * def.speed * 1.2;
                            // Contact swing with a cooldown.
                            let dy = p.pos.y - self.pos.y;
                            if dist < def.half_w + 0.9 && dy.abs() < 2.0 && self.attack_cd <= 0.0
                            {
                                self.attack_cd = 1.0;
                                events.push(MobEvent::HitPlayer(who, def.attack, self.pos));
                            }
                        }
                    }
                }
            },
        }

        // Physics: accelerate toward wish, gravity/buoyancy, collide per axis.
        let accel = if def.movement_float || self.on_ground { 14.0 } else { 4.0 };
        let step = (accel * dt).min(1.0);
        self.vel.x += (wish.x - self.vel.x) * step;
        self.vel.z += (wish.z - self.vel.z) * step;

        if def.movement_float {
            // Wisps hover: seek a bobbing height above ground (or the
            // player's eyes while hunting), no gravity at all.
            let gy = world.surface_height(self.pos.x.floor() as i32, self.pos.z.floor() as i32);
            let want_y = if self.state == MobState::Hunt {
                prey.map(|(_, p)| p.pos.y).unwrap_or(gy as f32) + 1.6
            } else {
                gy as f32 + 2.2
            } + (self.anim_phase * 0.7).sin() * 0.3;
            let vy = (want_y - self.pos.y).clamp(-2.5, 2.5);
            self.vel.y += (vy - self.vel.y) * step;
            self.anim_phase += dt * 2.0; // wisps always shimmer
        } else {
            let feet = world.get_block(
                self.pos.x.floor() as i32,
                (self.pos.y + 0.3).floor() as i32,
                self.pos.z.floor() as i32,
            );
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

        // Auto-jump a 1-block step when walking into a wall.
        if !def.movement_float && self.hit_wall && self.on_ground && wish.length_squared() > 0.01 {
            self.vel.y = JUMP;
        }

        // Legs swing with horizontal travel.
        let hspeed = Vec3::new(self.vel.x, 0.0, self.vel.z).length();
        self.anim_phase += hspeed * dt * 3.2;
    }

    fn collides(&self, world: &World, def: &AnimalDef, pos: Vec3) -> bool {
        let min = pos - Vec3::new(def.half_w, 0.0, def.half_w);
        let max = pos + Vec3::new(def.half_w, def.height, def.half_w);
        let (x0, x1) = (min.x.floor() as i32, max.x.floor() as i32);
        let (y0, y1) = (min.y.floor() as i32, max.y.floor() as i32);
        let (z0, z1) = (min.z.floor() as i32, max.z.floor() as i32);
        for x in x0..=x1 {
            for y in y0..=y1 {
                for z in z0..=z1 {
                    if world.reg.is_solid(world.get_block(x, y, z)) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn move_axis(&mut self, world: &World, def: &AnimalDef, delta: Vec3) {
        let target = self.pos + delta;
        if !self.collides(world, def, target) {
            self.pos = target;
            return;
        }
        let mut lo = 0.0f32;
        let mut hi = 1.0f32;
        for _ in 0..8 {
            let mid = (lo + hi) * 0.5;
            if self.collides(world, def, self.pos + delta * mid) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        self.pos += delta * lo;
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
    pub fn emit(&self, reg: &Registry, lum: (f32, f32), verts: &mut Vec<Vertex>, idx: &mut Vec<u32>) {
        let def = &reg.animals[self.species];
        // Emissive wardens are their own lantern.
        let lum = if def.emissive { (1.0, lum.1) } else { lum };
        // Models face -Z; motion forward is (sin yaw, cos yaw) = +Z at 0,
        // so render rotated by yaw + PI to keep the head leading.
        let (syaw, cyaw) = (self.yaw + std::f32::consts::PI).sin_cos();
        let amp = (Vec3::new(self.vel.x, 0.0, self.vel.z).length() / def.speed.max(0.1))
            .clamp(0.0, 1.0);
        let flash = 1.0 + self.hurt_flash * 2.4;

        // A box named "leg" mirrors into 4; everything else draws once.
        let mut boxes: Vec<([f32; 3], [f32; 3], bool, f32, Option<u16>)> = Vec::new();
        for b in &def.model {
            let is_head = b.name.starts_with("head");
            if b.name == "leg" {
                for (sx, sz) in [(1.0f32, 1.0f32), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
                    let at = [b.at[0] * sx, b.at[1], b.at[2] * sz];
                    // Diagonal pairs swing together.
                    let phase = if sx * sz > 0.0 { 0.0 } else { std::f32::consts::PI };
                    let swing = (self.anim_phase + phase).sin() * 0.55 * amp;
                    boxes.push((b.size, at, false, swing, b.tile));
                }
            } else {
                boxes.push((b.size, b.at, is_head, 0.0, b.tile));
            }
        }

        let gs = 0.45 + 0.55 * self.growth.min(1.0); // babies are small
        for (size, at, is_head, swing, tile_override) in boxes {
            let (hx, hy, hz) =
                (size[0] * gs / 32.0, size[1] * gs / 32.0, size[2] * gs / 32.0);
            let center =
                Vec3::new(at[0] * gs / 16.0, at[1] * gs / 16.0 + hy, at[2] * gs / 16.0);
            // Legs rotate around their top (hip) on the local X axis.
            let pivot_y = at[1] * gs / 16.0 + hy * 2.0;
            let (ss, cs) = swing.sin_cos();
            let ts = 1.0 / ATLAS_TILES as f32;
            let inset = ts / 32.0;
            for face in 0..6 {
                // The face art goes only on the head's front (-Z); every
                // other surface is fur — a face on the back of a skull
                // reads as cursed.
                let tile = tile_override
                    .unwrap_or(if is_head && face == 5 { def.head_tile } else { def.tile });
                let (tx, ty) = (tile as u32 % ATLAS_TILES, tile as u32 / ATLAS_TILES);
                let base = verts.len() as u32;
                for c in CORNERS[face].iter() {
                    let lx = center.x + (c[0] - 0.5) * 2.0 * hx;
                    let mut ly = center.y + (c[1] - 0.5) * 2.0 * hy;
                    let mut lz = center.z + (c[2] - 0.5) * 2.0 * hz;
                    if swing != 0.0 {
                        let (dy, dz) = (ly - pivot_y, lz - center.z);
                        ly = pivot_y + dy * cs - dz * ss;
                        lz = center.z + dy * ss + dz * cs;
                    }
                    // Yaw the whole mob (model faces -Z forward → +yaw).
                    let wx = lx * cyaw + lz * syaw;
                    let wz = -lx * syaw + lz * cyaw;
                    let (u, v) = match face {
                        0 | 1 => (c[2], 1.0 - c[1]),
                        4 | 5 => (c[0], 1.0 - c[1]),
                        _ => (c[0], c[2]),
                    };
                    let shade = FACE_SHADE[face].max(0.65) * flash;
                    verts.push(Vertex {
                        pos: [self.pos.x + wx, self.pos.y + ly, self.pos.z + wz],
                        uv: [
                            tx as f32 * ts + inset + u * (ts - 2.0 * inset),
                            ty as f32 * ts + inset + v * (ts - 2.0 * inset),
                        ],
                        normal: [0.0, 0.0, 0.0],
                        light: (shade * lum.0).min(2.0),
                        sky: (shade * lum.1).min(2.0),
                    });
                }
                idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
    }
}

/// A remote player's body: a boxy humanoid at `pos` facing `yaw`.
/// (Same box math as mob models; static pose, name tag drawn by the UI.)
pub fn emit_humanoid(
    pos: Vec3,
    yaw: f32,
    tile: u16,
    face: u16,
    lum: (f32, f32),
    verts: &mut Vec<Vertex>,
    idx: &mut Vec<u32>,
) {
    // (size px, at px, is_head)
    let boxes: [([f32; 3], [f32; 3], bool); 6] = [
        ([2.0, 6.0, 2.0], [-1.5, 0.0, 0.0], false),
        ([2.0, 6.0, 2.0], [1.5, 0.0, 0.0], false),
        ([6.0, 6.0, 3.0], [0.0, 6.0, 0.0], false),
        ([2.0, 6.0, 2.0], [-4.0, 6.0, 0.0], false),
        ([2.0, 6.0, 2.0], [4.0, 6.0, 0.0], false),
        ([5.0, 5.0, 5.0], [0.0, 12.0, 0.0], true),
    ];
    let (syaw, cyaw) = (yaw + std::f32::consts::PI).sin_cos();
    let ts = 1.0 / ATLAS_TILES as f32;
    let inset = ts / 32.0;
    for (size, at, is_head) in boxes {
        let (hx, hy, hz) = (size[0] / 32.0, size[1] / 32.0, size[2] / 32.0);
        let center = Vec3::new(at[0] / 16.0, at[1] / 16.0 + hy, at[2] / 16.0);
        for f in 0..6 {
            let t = if is_head && f == 5 { face } else { tile };
            let (tx, ty) = (t as u32 % ATLAS_TILES, t as u32 / ATLAS_TILES);
            let base = verts.len() as u32;
            for c in CORNERS[f].iter() {
                let lx = center.x + (c[0] - 0.5) * 2.0 * hx;
                let ly = center.y + (c[1] - 0.5) * 2.0 * hy;
                let lz = center.z + (c[2] - 0.5) * 2.0 * hz;
                let wx = lx * cyaw + lz * syaw;
                let wz = -lx * syaw + lz * cyaw;
                let (u, v) = match f {
                    0 | 1 => (c[2], 1.0 - c[1]),
                    4 | 5 => (c[0], 1.0 - c[1]),
                    _ => (c[0], c[2]),
                };
                let shade = FACE_SHADE[f].max(0.65);
                verts.push(Vertex {
                    pos: [pos.x + wx, pos.y + ly, pos.z + wz],
                    uv: [
                        tx as f32 * ts + inset + u * (ts - 2.0 * inset),
                        ty as f32 * ts + inset + v * (ts - 2.0 * inset),
                    ],
                    light: shade * lum.0,
                    sky: shade * lum.1,
                });
            }
            idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
}
