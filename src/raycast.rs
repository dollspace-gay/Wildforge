//! Voxel raycast (Amanatides & Woo DDA).

use glam::Vec3;

use crate::planet::{BlockPos, EntityPos};
use crate::world::World;

#[cfg(test)]
pub struct Hit {
    pub block: (i32, i32, i32),
    /// Block adjacent to the hit face (where a new block would be placed).
    pub adjacent: (i32, i32, i32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanetHit {
    pub block: BlockPos,
    /// Canonical cell adjacent to the selected face.
    pub adjacent: BlockPos,
}

/// Topology-aware voxel DDA in the origin's local face frame.
pub fn raycast_at(world: &World, origin: EntityPos, dir: Vec3, max_dist: f32) -> Option<PlanetHit> {
    cast_at(world, origin, dir, max_dist, false)
}

pub fn raycast_water_at(
    world: &World,
    origin: EntityPos,
    dir: Vec3,
    max_dist: f32,
) -> Option<PlanetHit> {
    cast_at(world, origin, dir, max_dist, true)
}

#[cfg(test)]
pub fn raycast(world: &World, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<Hit> {
    cast(world, origin, dir, max_dist, false)
}

/// Like `raycast`, but water is a hit too — what a bucket wants.
#[cfg(test)]
pub fn raycast_water(world: &World, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<Hit> {
    cast(world, origin, dir, max_dist, true)
}

#[cfg(test)]
fn cast(world: &World, origin: Vec3, dir: Vec3, max_dist: f32, hit_water: bool) -> Option<Hit> {
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }
    let mut x = origin.x.floor() as i32;
    let mut y = origin.y.floor() as i32;
    let mut z = origin.z.floor() as i32;

    let step_x = if dir.x > 0.0 { 1 } else { -1 };
    let step_y = if dir.y > 0.0 { 1 } else { -1 };
    let step_z = if dir.z > 0.0 { 1 } else { -1 };

    let t_delta = Vec3::new(
        if dir.x != 0.0 {
            (1.0 / dir.x).abs()
        } else {
            f32::INFINITY
        },
        if dir.y != 0.0 {
            (1.0 / dir.y).abs()
        } else {
            f32::INFINITY
        },
        if dir.z != 0.0 {
            (1.0 / dir.z).abs()
        } else {
            f32::INFINITY
        },
    );

    let frac = |v: f32| v - v.floor();
    let mut t_max = Vec3::new(
        if dir.x > 0.0 {
            (1.0 - frac(origin.x)) * t_delta.x
        } else {
            frac(origin.x) * t_delta.x
        },
        if dir.y > 0.0 {
            (1.0 - frac(origin.y)) * t_delta.y
        } else {
            frac(origin.y) * t_delta.y
        },
        if dir.z > 0.0 {
            (1.0 - frac(origin.z)) * t_delta.z
        } else {
            frac(origin.z) * t_delta.z
        },
    );

    let mut prev = (x, y, z);
    let mut t = 0.0f32;
    while t <= max_dist {
        let b = world.get_block(x, y, z);
        // Hit anything mineable: solids AND non-solid plants (cross blocks),
        // but never air — and water only when the caller wants it.
        if b != crate::registry::AIR && (hit_water || !world.reg.is_fluid(b)) {
            return Some(Hit {
                block: (x, y, z),
                adjacent: prev,
            });
        }
        prev = (x, y, z);
        if t_max.x < t_max.y && t_max.x < t_max.z {
            x += step_x;
            t = t_max.x;
            t_max.x += t_delta.x;
        } else if t_max.y < t_max.z {
            y += step_y;
            t = t_max.y;
            t_max.y += t_delta.y;
        } else {
            z += step_z;
            t = t_max.z;
            t_max.z += t_delta.z;
        }
    }
    None
}

fn cast_at(
    world: &World,
    mut point: EntityPos,
    dir: Vec3,
    max_dist: f32,
    hit_water: bool,
) -> Option<PlanetHit> {
    let mut dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO || max_dist < 0.0 {
        return None;
    }
    let mut current = point.block()?;
    let mut previous = current;
    let mut travelled = 0.0f32;

    let boundary_time = |coordinate: f32, velocity: f32| {
        if velocity > 0.0 {
            ((coordinate.floor() + 1.0 - coordinate) / velocity).max(1.0e-6)
        } else if velocity < 0.0 {
            let distance = coordinate - coordinate.floor();
            let distance = if distance <= 1.0e-6 { 1.0 } else { distance };
            (distance / -velocity).max(1.0e-6)
        } else {
            f32::INFINITY
        }
    };

    loop {
        let block = world.get_block_at(current);
        if block != crate::registry::AIR && (hit_water || !world.reg.is_fluid(block)) {
            return Some(PlanetHit {
                block: current,
                adjacent: previous,
            });
        }

        let tu = boundary_time(point.u(), dir.x);
        let ty = boundary_time(point.y(), dir.y);
        let tv = boundary_time(point.v(), dir.z);
        let step = tu.min(ty).min(tv);
        if !step.is_finite() || travelled + step > max_dist {
            return None;
        }
        travelled += step;
        previous = current;

        // Move onto the selected voxel plane, then nudge only the axes that
        // actually crossed it. A time epsilon is not robust here: at chart
        // coordinates near 8192, the old 1e-4 nudge was smaller than one f32
        // ULP and exact corner rays could remain on the source face. Two
        // thousandths of a block is safely representable at this planet size
        // while remaining far below any logical cell dimension.
        const PLANE_NUDGE: f32 = 2.0e-3;
        let mut delta = dir * step;
        let tied = |time: f32| (time - step).abs() <= 1.0e-6;
        if tied(tu) {
            delta.x += PLANE_NUDGE.copysign(dir.x);
        }
        if tied(ty) {
            delta.y += PLANE_NUDGE.copysign(dir.y);
        }
        if tied(tv) {
            delta.z += PLANE_NUDGE.copysign(dir.z);
        }
        // Canonicalization rotates both the chart and ray direction if the
        // crossed plane is a cube-face edge, after which the next DDA
        // interval is ordinary again.
        let crossed = point
            .translated(delta)
            .expect("a bounded ray step crosses at most one planet edge");
        point = crossed.pos;
        dir = crossed.rotation.rotate_vec3(dir);
        current = point.block()?;
    }
}
