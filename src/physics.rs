//! Player AABB physics against the voxel world (early-alpha Minecraft feel).

use glam::Vec3;

#[cfg(test)]
use crate::planet::Face;
use crate::planet::{BlockPos, EntityPos, QuarterTurn, SurfacePos};
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

/// The longest single hop a move may take before the world is consulted again,
/// in blocks.
///
/// A solid block is one unit thick, so sampling a path at intervals shorter
/// than a block means no block can sit entirely between two samples. A quarter
/// block leaves generous margin and costs nothing at ordinary speeds: a
/// sprinting sixtieth of a second covers about a tenth of a block, which is
/// still one hop and still one collision test — the same work this did before.
const SWEEP_STEP: f32 = 0.25;

/// Safety valve on the hop count. Terminal velocity across the longest tick
/// the server will simulate is under fourteen blocks (fifty-six hops); this is
/// an order of magnitude past that, so it only ever catches a nonsense delta.
const MAX_SWEEP_HOPS: i32 = 512;

pub struct Player {
    /// Feet-center position.
    pub pos: EntityPos,
    pub vel: Vec3,
    pub on_ground: bool,
    pub in_water: bool,
    /// Hit a wall horizontally last frame (used for the jump-out-of-water hop).
    pub pushed_wall: bool,
    /// Face-frame rotation accumulated by the latest move. Camera yaw and
    /// other local-frame state consume this after physics.
    pub frame_rotation: QuarterTurn,
}

pub struct Input {
    pub forward: f32, // -1..1
    pub strafe: f32,  // -1..1
    pub jump: bool,
    pub sprint: bool,
    /// Derived move-speed multiplier from the player stat surface.
    pub speed_mult: f32,
}

impl Player {
    #[cfg(test)]
    pub fn new(pos: Vec3) -> Player {
        let half = f32::from(crate::planet::FACE_BLOCKS) * 0.5;
        let pos = EntityPos::new(Face::PosZ, pos.x + half, pos.y, pos.z + half)
            .expect("legacy player spawn is inside the finite PosZ chart");
        Self::new_at(pos)
    }

    pub fn new_at(pos: EntityPos) -> Player {
        Player {
            pos,
            vel: Vec3::ZERO,
            on_ground: false,
            in_water: false,
            pushed_wall: false,
            frame_rotation: QuarterTurn::IDENTITY,
        }
    }

    pub fn eye(&self) -> EntityPos {
        self.pos
            .translated(Vec3::new(0.0, EYE_HEIGHT, 0.0))
            .expect("vertical eye offset remains on the same chart")
            .pos
    }

    fn head_in_water(&self, world: &World) -> bool {
        let Some(pos) = self.eye().block() else {
            return false;
        };
        let b = world.get_block_at(pos);
        world.reg.is_fluid(b)
    }

    fn body_in_water(&self, world: &World) -> bool {
        let Some(pos) = self
            .pos
            .translated(Vec3::new(0.0, 0.6, 0.0))
            .ok()
            .and_then(|p| p.pos.block())
        else {
            return false;
        };
        let b = world.get_block_at(pos);
        world.reg.is_fluid(b)
    }

    pub fn update(&mut self, world: &World, input: &Input, flat_fwd: Vec3, right: Vec3, dt: f32) {
        self.frame_rotation = QuarterTurn::IDENTITY;
        self.in_water = self.body_in_water(world);

        // Horizontal wish velocity.
        // `flat_fwd` and `right` are chart deltas calibrated through the
        // curved surface Jacobian. Their Euclidean chart lengths are not
        // meaningful, so normalize the player's two orthogonal intent axes
        // before combining them rather than renormalizing the chart vector.
        let intent_length = input.forward.hypot(input.strafe).max(1.0);
        let wish = (flat_fwd * input.forward + right * input.strafe) / intent_length;
        let speed = if self.in_water {
            SWIM_SPEED
        } else if input.sprint {
            WALK_SPEED * SPRINT_MULT
        } else {
            WALK_SPEED
        } * input.speed_mult;
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
        if self.pos.y() < -10.0 {
            let block = self
                .pos
                .translated(Vec3::new(0.0, -self.pos.y(), 0.0))
                .expect("vertical respawn projection stays on the face")
                .pos;
            let surface = SurfacePos::new(
                block.face(),
                block.u().floor() as u16,
                block.v().floor() as u16,
            )
            .expect("canonical player surface");
            self.pos = EntityPos::new(
                block.face(),
                block.u(),
                world.surface_height_at(surface) as f32 + 2.0,
                block.v(),
            )
            .expect("respawn height is finite");
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

    pub(crate) fn collides(&self, world: &World, pos: EntityPos) -> bool {
        let local = pos.chart_local();
        let min = local - Vec3::new(PLAYER_HALF_W, 0.0, PLAYER_HALF_W);
        let max = local + Vec3::new(PLAYER_HALF_W, PLAYER_HEIGHT, PLAYER_HALF_W);
        let (x0, x1) = (min.x.floor() as i32, max.x.floor() as i32);
        let (y0, y1) = (min.y.floor() as i32, max.y.floor() as i32);
        let (z0, z1) = (min.z.floor() as i32, max.z.floor() as i32);
        for x in x0..=x1 {
            for y in y0..=y1 {
                for z in z0..=z1 {
                    if !(0..crate::chunk::CHUNK_Y as i32).contains(&y) {
                        continue;
                    }
                    let Ok(surface) = SurfacePos::canonicalized(pos.face(), x, z) else {
                        return true;
                    };
                    let b = world.get_block_at(
                        crate::planet::BlockPos::new(
                            surface.face(),
                            surface.u(),
                            y as u8,
                            surface.v(),
                        )
                        .expect("collision cell is validated"),
                    );
                    if !world.reg.is_solid(b) {
                        continue;
                    }
                    if world.is_hidden(
                        crate::planet::BlockPos::new(
                            surface.face(),
                            surface.u(),
                            y as u8,
                            surface.v(),
                        )
                        .expect("collision cell is validated"),
                    ) {
                        continue;
                    }
                    // Partial-height solids (steps, low work blocks) occupy
                    // only the bottom of their voxel.  Treating them as a
                    // full cube made the auto-step path impossible even
                    // though rendering already honored `height`.
                    let block_top = y as f32 + world.reg.block(b).height.unwrap_or(1.0);
                    if max.y <= y as f32 || min.y >= block_top {
                        continue;
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Horizontal move with auto-step: if blocked while grounded, try lifting up
    /// to `STEP_HEIGHT`, re-advancing, and settling onto a low ledge (slabs,
    /// snow layers). Falls back to the plain slide if that gains no ground.
    fn walk_axis(&mut self, world: &World, delta: Vec3, grounded: bool) {
        let start = self.pos;
        let start_vel = self.vel;
        let start_rotation = self.frame_rotation;
        let advanced = self.move_axis(world, delta);
        if !grounded || advanced + 1e-4 >= 1.0 {
            return; // moved freely, or airborne — no stepping
        }
        let flat = (self.pos, self.vel, self.frame_rotation);
        let flat_ground = self.on_ground;
        let lifted = start
            .translated(Vec3::new(0.0, STEP_HEIGHT, 0.0))
            .expect("vertical step remains on the current chart")
            .pos;
        if self.collides(world, lifted) {
            return; // no headroom to step
        }
        self.pos = lifted;
        self.vel = start_vel;
        self.frame_rotation = start_rotation;
        let stepped = self.move_axis(world, delta);
        self.on_ground = false;
        self.move_axis(world, Vec3::new(0.0, -STEP_HEIGHT, 0.0));
        // Keep the step only if it landed on a ledge farther along than the slide.
        if !(self.on_ground && stepped > advanced + 1e-4) {
            self.pos = flat.0;
            self.vel = flat.1;
            self.frame_rotation = flat.2;
            self.on_ground = flat_ground;
        }
    }

    fn move_axis(&mut self, world: &World, delta: Vec3) -> f32 {
        let dist = delta.length();
        if dist <= 0.0 {
            return 1.0;
        }
        // Walk the path rather than teleporting to the end of it.
        //
        // This used to test the destination and nothing else, so a step long
        // enough to clear a wall passed straight through: both ends in open
        // air, the wall between them never consulted. A hitch is enough —
        // the server simulates up to a quarter second at a time, and a sprint
        // covers well over a block in that.
        let hops = (dist / SWEEP_STEP).ceil().max(1.0);
        let hops = (hops as i32).min(MAX_SWEEP_HOPS);
        // Fraction of `delta` covered so far, every bit of it tested clear.
        let mut travelled = 0.0f32;
        let mut blocked = false;
        for i in 1..=hops {
            let t = i as f32 / hops as f32;
            let candidate = self
                .pos
                .translated(delta * t)
                .expect("a swept player hop crosses at most one planet edge")
                .pos;
            if self.collides(world, candidate) {
                // Snug up to whatever stopped us, searching only inside this
                // hop. The interval is shorter than a block, so a clear point
                // found in it cannot be on the far side of the obstacle —
                // which is exactly what the old full-span search could return,
                // because `collides` is not monotonic in `t` and bisection
                // assumed it was.
                let (mut lo, mut hi) = (travelled, t);
                for _ in 0..8 {
                    let mid = (lo + hi) * 0.5;
                    let candidate = self
                        .pos
                        .translated(delta * mid)
                        .expect("collision bisection crosses at most one planet edge")
                        .pos;
                    if self.collides(world, candidate) {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                travelled = lo;
                blocked = true;
                break;
            }
            travelled = t;
        }
        let moved = self
            .pos
            .translated(delta * travelled)
            .expect("a committed player move crosses at most one planet edge");
        self.pos = moved.pos;
        self.vel = moved.rotation.rotate_vec3(self.vel);
        self.frame_rotation = moved.rotation.then(self.frame_rotation);
        if !blocked {
            return travelled;
        }
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
        travelled
    }

    /// Would placing a block at these world coords overlap the player?
    pub fn overlaps_block_at(&self, block: BlockPos) -> bool {
        let center = block.entity_center();
        let delta = self.pos.local_delta_to(center);
        let min = Vec3::new(delta.x - 0.5, delta.y - 0.5, delta.z - 0.5);
        let max = Vec3::new(delta.x + 0.5, delta.y + 0.5, delta.z + 0.5);
        -PLAYER_HALF_W < max.x
            && PLAYER_HALF_W > min.x
            && 0.0 < max.y
            && PLAYER_HEIGHT > min.y
            && -PLAYER_HALF_W < max.z
            && PLAYER_HALF_W > min.z
    }
}
