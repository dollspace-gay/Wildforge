//! Wildlife: boxy passive mobs with wander/flee AI, hunted for food.
//! Species are data (`registry::AnimalDef`); this module is the runtime —
//! movement, steering, rendering, and ray hits.

use glam::Vec3;

use crate::atlas::ATLAS_TILES;
use crate::mesher::{CORNERS, FACE_SHADE, Vertex};
use crate::registry::{AnimalDef, Registry};
use crate::world::World;

const GRAVITY: f32 = 28.0;
const TERMINAL: f32 = 40.0;
const JUMP: f32 = 7.6;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MobState {
    Idle,
    Wander,
    Flee,
}

#[derive(Clone, Debug)]
pub struct Mob {
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
}

fn r01(rng: &mut u32) -> f32 {
    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
    (*rng >> 8) as f32 / (1 << 24) as f32
}

impl Mob {
    pub fn new(species: usize, pos: Vec3, yaw: f32) -> Mob {
        Mob {
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
        }
    }

    /// Take damage from an attacker at `from`: knockback + panic.
    pub fn hurt(&mut self, dmg: f32, from: Vec3) {
        self.health -= dmg;
        self.hurt_flash = 0.35;
        let mut away = self.pos - from;
        away.y = 0.0;
        let dir = if away.length_squared() > 0.001 { away.normalize() } else { Vec3::Z };
        self.vel += dir * 6.0 + Vec3::new(0.0, 4.5, 0.0);
        self.state = MobState::Flee;
        self.state_timer = 5.0;
        self.target = from;
    }

    pub fn tick(&mut self, world: &World, def: &AnimalDef, player: Vec3, dt: f32, rng: &mut u32) {
        self.state_timer -= dt;
        self.hurt_flash = (self.hurt_flash - dt).max(0.0);

        // Skittish species bolt when the player closes in.
        if def.flee_range > 0.0 && self.state != MobState::Flee {
            let mut d = player - self.pos;
            d.y = 0.0;
            if d.length_squared() < def.flee_range * def.flee_range {
                self.state = MobState::Flee;
                self.state_timer = 4.0;
                self.target = player;
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
        }

        // Physics: accelerate toward wish, gravity/buoyancy, collide per axis.
        let accel = if self.on_ground { 14.0 } else { 4.0 };
        let step = (accel * dt).min(1.0);
        self.vel.x += (wish.x - self.vel.x) * step;
        self.vel.z += (wish.z - self.vel.z) * step;

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

        let d = self.vel * dt;
        self.on_ground = false;
        self.hit_wall = false;
        self.move_axis(world, def, Vec3::new(d.x, 0.0, 0.0));
        self.move_axis(world, def, Vec3::new(0.0, 0.0, d.z));
        self.move_axis(world, def, Vec3::new(0.0, d.y, 0.0));

        // Auto-jump a 1-block step when walking into a wall.
        if self.hit_wall && self.on_ground && wish.length_squared() > 0.01 {
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
    pub fn emit(&self, reg: &Registry, verts: &mut Vec<Vertex>, idx: &mut Vec<u32>) {
        let def = &reg.animals[self.species];
        // Models face -Z; motion forward is (sin yaw, cos yaw) = +Z at 0,
        // so render rotated by yaw + PI to keep the head leading.
        let (syaw, cyaw) = (self.yaw + std::f32::consts::PI).sin_cos();
        let amp = (Vec3::new(self.vel.x, 0.0, self.vel.z).length() / def.speed.max(0.1))
            .clamp(0.0, 1.0);
        let flash = 1.0 + self.hurt_flash * 2.4;

        // A box named "leg" mirrors into 4; everything else draws once.
        let mut boxes: Vec<([f32; 3], [f32; 3], bool, f32)> = Vec::new();
        for b in &def.model {
            let is_head = b.name.starts_with("head");
            if b.name == "leg" {
                for (sx, sz) in [(1.0f32, 1.0f32), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
                    let at = [b.at[0] * sx, b.at[1], b.at[2] * sz];
                    // Diagonal pairs swing together.
                    let phase = if sx * sz > 0.0 { 0.0 } else { std::f32::consts::PI };
                    let swing = (self.anim_phase + phase).sin() * 0.55 * amp;
                    boxes.push((b.size, at, false, swing));
                }
            } else {
                boxes.push((b.size, b.at, is_head, 0.0));
            }
        }

        for (size, at, is_head, swing) in boxes {
            let (hx, hy, hz) = (size[0] / 32.0, size[1] / 32.0, size[2] / 32.0);
            let center = Vec3::new(at[0] / 16.0, at[1] / 16.0 + hy, at[2] / 16.0);
            // Legs rotate around their top (hip) on the local X axis.
            let pivot_y = at[1] / 16.0 + hy * 2.0;
            let (ss, cs) = swing.sin_cos();
            let ts = 1.0 / ATLAS_TILES as f32;
            let inset = ts / 32.0;
            for face in 0..6 {
                // The face art goes only on the head's front (-Z); every
                // other surface is fur — a face on the back of a skull
                // reads as cursed.
                let tile = if is_head && face == 5 { def.head_tile } else { def.tile };
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
                    verts.push(Vertex {
                        pos: [self.pos.x + wx, self.pos.y + ly, self.pos.z + wz],
                        uv: [
                            tx as f32 * ts + inset + u * (ts - 2.0 * inset),
                            ty as f32 * ts + inset + v * (ts - 2.0 * inset),
                        ],
                        light: (FACE_SHADE[face].max(0.65) * flash).min(2.0),
                    });
                }
                idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
    }
}
