//! Player AABB physics against the voxel world (early-alpha Minecraft feel).

use glam::Vec3;

use crate::world::World;

pub const PLAYER_HALF_W: f32 = 0.3;
pub const PLAYER_HEIGHT: f32 = 1.8;
pub const EYE_HEIGHT: f32 = 1.62;

/// How high the player auto-steps when walking into a low ledge (octant sand,
/// slabs). Below a full block so 1-tall walls still stop you.
const STEP_HEIGHT: f32 = 0.55;
const GRAVITY: f32 = 28.0;
const JUMP_SPEED: f32 = 8.6;
const WALK_SPEED: f32 = 4.4;
const SPRINT_MULT: f32 = 1.6;
const SWIM_SPEED: f32 = 3.0;
const TERMINAL: f32 = 55.0;

pub struct Player {
    /// Feet-center position.
    pub pos: Vec3,
    pub vel: Vec3,
    pub on_ground: bool,
    pub in_water: bool,
    /// Hit a wall horizontally last frame (used for the jump-out-of-water hop).
    pub pushed_wall: bool,
}

pub struct Input {
    pub forward: f32, // -1..1
    pub strafe: f32,  // -1..1
    pub jump: bool,
    pub sprint: bool,
}

impl Player {
    pub fn new(pos: Vec3) -> Player {
        Player {
            pos,
            vel: Vec3::ZERO,
            on_ground: false,
            in_water: false,
            pushed_wall: false,
        }
    }

    pub fn eye(&self) -> Vec3 {
        self.pos + Vec3::new(0.0, EYE_HEIGHT, 0.0)
    }

    fn head_in_water(&self, world: &World) -> bool {
        let e = self.eye();
        let b = world.get_block(e.x.floor() as i32, e.y.floor() as i32, e.z.floor() as i32);
        world.reg.is_fluid(b)
    }

    fn body_in_water(&self, world: &World) -> bool {
        let p = self.pos + Vec3::new(0.0, 0.6, 0.0);
        let b = world.get_block(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        world.reg.is_fluid(b)
    }

    pub fn update(&mut self, world: &World, input: &Input, flat_fwd: Vec3, right: Vec3, dt: f32) {
        self.in_water = self.body_in_water(world);

        // Horizontal wish velocity.
        let mut wish = flat_fwd * input.forward + right * input.strafe;
        if wish.length_squared() > 1.0 {
            wish = wish.normalize();
        }
        let speed = if self.in_water {
            SWIM_SPEED
        } else if input.sprint {
            WALK_SPEED * SPRINT_MULT
        } else {
            WALK_SPEED
        };
        // Snappy ground control, floatier air control.
        let accel = if self.on_ground || self.in_water {
            18.0
        } else {
            6.0
        };
        let target = wish * speed;
        let cur = Vec3::new(self.vel.x, 0.0, self.vel.z);
        let delta = target - cur;
        let step = (accel * dt).min(1.0);
        self.vel.x += delta.x * step;
        self.vel.z += delta.z * step;

        // Vertical
        if self.in_water {
            self.vel.y -= GRAVITY * 0.25 * dt;
            self.vel.y = self.vel.y.max(-4.0);
            if input.jump {
                // Swimming against a ledge: hop hard enough to climb out onto
                // a block above the surface (Minecraft's jump-out-of-water).
                if self.pushed_wall {
                    self.vel.y = self.vel.y.max(8.2);
                } else {
                    self.vel.y = (self.vel.y + 24.0 * dt).min(3.5);
                }
            }
        } else {
            self.vel.y -= GRAVITY * dt;
            self.vel.y = self.vel.y.max(-TERMINAL);
            if input.jump && self.on_ground {
                self.vel.y = JUMP_SPEED;
            }
        }

        // Move axis-by-axis with collision resolution.
        let d = self.vel * dt;
        let was_ground = self.on_ground;
        self.on_ground = false;
        self.pushed_wall = false;
        self.walk_axis(world, Vec3::new(d.x, 0.0, 0.0), was_ground);
        self.walk_axis(world, Vec3::new(0.0, 0.0, d.z), was_ground);
        self.move_axis(world, Vec3::new(0.0, d.y, 0.0));

        // Void safety: respawn above surface if fallen out.
        if self.pos.y < -10.0 {
            let x = self.pos.x.floor() as i32;
            let z = self.pos.z.floor() as i32;
            self.pos.y = world.surface_height(x, z) as f32 + 2.0;
            self.vel = Vec3::ZERO;
        }
        let _ = self.head_in_water(world); // (used by renderer via head_underwater)
    }

    pub fn head_underwater(&self, world: &World) -> bool {
        self.head_in_water(world)
    }

    /// Creative flight: direct velocity, no gravity, collisions kept.
    pub fn fly(&mut self, world: &World, wish: Vec3, dt: f32) {
        self.vel = wish;
        self.on_ground = false;
        self.in_water = false;
        let d = wish * dt;
        self.move_axis(world, Vec3::new(d.x, 0.0, 0.0));
        self.move_axis(world, Vec3::new(0.0, 0.0, d.z));
        self.move_axis(world, Vec3::new(0.0, d.y, 0.0));
    }

    pub(crate) fn collides(&self, world: &World, pos: Vec3) -> bool {
        let min = pos - Vec3::new(PLAYER_HALF_W, 0.0, PLAYER_HALF_W);
        let max = pos + Vec3::new(PLAYER_HALF_W, PLAYER_HEIGHT, PLAYER_HALF_W);
        let (x0, x1) = (min.x.floor() as i32, max.x.floor() as i32);
        let (y0, y1) = (min.y.floor() as i32, max.y.floor() as i32);
        let (z0, z1) = (min.z.floor() as i32, max.z.floor() as i32);
        for x in x0..=x1 {
            for y in y0..=y1 {
                for z in z0..=z1 {
                    let b = world.get_block(x, y, z);
                    if !world.reg.is_solid(b) {
                        continue;
                    }
                    if !world.reg.block(b).sub_voxel {
                        return true;
                    }
                    // Sub-voxel block: collide against each filled octant's
                    // half-cube instead of the whole cell.
                    let mask = world.get_meta(x, y, z);
                    for o in 0..8u32 {
                        if mask & (1 << o) == 0 {
                            continue;
                        }
                        let ox = (o & 1) as f32 * 0.5;
                        let oz = ((o >> 1) & 1) as f32 * 0.5;
                        let oy = ((o >> 2) & 1) as f32 * 0.5;
                        let bmin = Vec3::new(x as f32 + ox, y as f32 + oy, z as f32 + oz);
                        if min.x < bmin.x + 0.5
                            && max.x > bmin.x
                            && min.y < bmin.y + 0.5
                            && max.y > bmin.y
                            && min.z < bmin.z + 0.5
                            && max.z > bmin.z
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Horizontal move with auto-step: if blocked while grounded, try lifting up
    /// to `STEP_HEIGHT`, re-advancing, and settling onto a low ledge (octant
    /// sand, slabs). Falls back to the plain slide if that gains no ground.
    fn walk_axis(&mut self, world: &World, delta: Vec3, grounded: bool) {
        let start = self.pos;
        self.move_axis(world, delta);
        let advanced = (self.pos - start).dot(delta);
        let wanted = delta.dot(delta);
        if !grounded || advanced + 1e-4 >= wanted {
            return; // moved freely, or airborne — no stepping
        }
        let flat = self.pos;
        let flat_ground = self.on_ground;
        let lifted = start + Vec3::new(0.0, STEP_HEIGHT, 0.0);
        if self.collides(world, lifted) {
            return; // no headroom to step
        }
        self.pos = lifted;
        self.move_axis(world, delta);
        self.on_ground = false;
        self.move_axis(world, Vec3::new(0.0, -STEP_HEIGHT, 0.0));
        // Keep the step only if it landed on a ledge farther along than the slide.
        if !(self.on_ground && (self.pos - start).dot(delta) > advanced + 1e-4) {
            self.pos = flat;
            self.on_ground = flat_ground;
        }
    }

    fn move_axis(&mut self, world: &World, delta: Vec3) {
        let target = self.pos + delta;
        if !self.collides(world, target) {
            self.pos = target;
            return;
        }
        // Binary-search the largest non-colliding fraction, then zero velocity on that axis.
        let mut lo = 0.0f32;
        let mut hi = 1.0f32;
        for _ in 0..8 {
            let mid = (lo + hi) * 0.5;
            if self.collides(world, self.pos + delta * mid) {
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
            self.pushed_wall = true;
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

    /// Would placing a block at these world coords overlap the player?
    pub fn overlaps_block(&self, bx: i32, by: i32, bz: i32) -> bool {
        let min = self.pos - Vec3::new(PLAYER_HALF_W, 0.0, PLAYER_HALF_W);
        let max = self.pos + Vec3::new(PLAYER_HALF_W, PLAYER_HEIGHT, PLAYER_HALF_W);
        (bx as f32) < max.x
            && (bx + 1) as f32 > min.x
            && (by as f32) < max.y
            && (by + 1) as f32 > min.y
            && (bz as f32) < max.z
            && (bz + 1) as f32 > min.z
    }
}
