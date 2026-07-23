//! Voxel raycast (Amanatides & Woo DDA).

use glam::Vec3;

use crate::world::World;

pub struct Hit {
    pub block: (i32, i32, i32),
    /// Block adjacent to the hit face (where a new block would be placed).
    pub adjacent: (i32, i32, i32),
}

pub fn raycast(world: &World, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<Hit> {
    cast(world, origin, dir, max_dist, false)
}

/// Like `raycast`, but water is a hit too — what a bucket wants.
pub fn raycast_water(world: &World, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<Hit> {
    cast(world, origin, dir, max_dist, true)
}

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
