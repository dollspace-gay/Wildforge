//! Voxel raycast (Amanatides & Woo DDA).
//!
//! Two DDA variants exist here: a topology-aware walk (`cast_at`) that
//! handles planet-face seams for the main chunk grid, and a flat
//! generic DDA (`cast`) that works in any axis-aligned block store's
//! local space.  [`raycast_target_at`] combines both: it walks the
//! world and every nearby `LocalStructure` in parallel, returning the
//! nearer hit.

use glam::Vec3;

use crate::planet::{BlockPos, EntityPos};
use crate::registry::{AIR, BlockId};
use crate::world::World;
use crate::world::local_structure::LocalStructureId;
use crate::world::multiblock::BlockStore;

/// A hit in flat `(i32,i32,i32)` coordinate space (no planet topology).
/// Used when casting against a structure's local store or in test scenes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FlatHit {
    /// The cell the ray struck.
    pub block: (i32, i32, i32),
    /// The cell adjacent to the face that was struck (where a new block
    /// would be placed).
    pub adjacent: (i32, i32, i32),
}

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
    cast(
        origin,
        dir,
        max_dist,
        |(x, y, z)| world.get_block(x, y, z),
        |b| b != crate::registry::AIR && !world.reg.is_fluid(b),
    )
    .map(|flat| Hit {
        block: flat.block,
        adjacent: flat.adjacent,
    })
}

/// The outcome of a structure-aware raycast: the ray hit a world block
/// or a structure-hosted block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetHit {
    World(PlanetHit),
    Structure {
        id: LocalStructureId,
        /// Canonical local offset of the struck cell.
        block: (i32, i32, i32),
        /// Canonical local offset adjacent to the struck face.
        adjacent: (i32, i32, i32),
    },
}

/// Generic flat DDA over any block-lookup function.  Walks the grid from
/// `origin` along `dir` up to `max_dist` and returns the first cell for
/// which `is_hit` returns true.  The origin and direction are in the
/// same coordinate space as the block-probe function.
pub fn cast<F, G>(
    origin: Vec3,
    dir: Vec3,
    max_dist: f32,
    mut block_at: F,
    mut is_hit: G,
) -> Option<FlatHit>
where
    F: FnMut((i32, i32, i32)) -> BlockId,
    G: FnMut(BlockId) -> bool,
{
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
        let b = block_at((x, y, z));
        if b != AIR && is_hit(b) {
            return Some(FlatHit {
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

/// Return the world hit and the nearest structure hit along `origin` → `dir`,
/// whichever is closer.  Structure hits are computed by transforming the ray
/// into the structure's canonical local space (inverse rotation + anchor
/// offset) and running the flat DDA against the structure's own block store.
///
/// Structures on a different face from the origin are skipped (cube‑edge
/// targeting is not required for parked‑structure gameplay in this phase).
///
/// ## Coordinate convention
///
pub fn raycast_target_at(
    world: &World,
    origin: EntityPos,
    dir: Vec3,
    max_dist: f32,
) -> Option<TargetHit> {
    // EntityPos::u() / v() return values that, when passed through
    // `point.block()`, produce the correct absolute BlockPos u/v.
    // They ARE the absolute frame — no conversion needed.
    let (ou, oy, ov) = (origin.u(), origin.y(), origin.v());
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }

    // World hit (topology‑aware).
    let world_hit = raycast_at(world, origin, dir, max_dist);

    // Best candidate: world hit's distance (absolute frame).
    let structures = world.local_structures();
    let mut best: Option<(f32, TargetHit)> = world_hit.map(|h| {
        let cu = h.block.u() as f32 + 0.5;
        let cy = h.block.y() as f32 + 0.5;
        let cv = h.block.v() as f32 + 0.5;
        let d = (cu - ou) * dir.x + (cy - oy) * dir.y + (cv - ov) * dir.z;
        (d, TargetHit::World(h))
    });

    for s in structures.iter() {
        if s.transform.anchor.face() != origin.face() {
            eprintln!("DEBUG raycast: face mismatch");
            continue;
        }
        let anchor = s.transform.anchor;
        let (au, ay, av) = (anchor.u() as f32, anchor.y() as f32, anchor.v() as f32);
        let local_blocks = &s.blocks;
        let first = local_blocks.keys().next();
        let Some(first) = first else {
            continue;
        };
        let mut mn =
            s.transform
                .rotation
                .apply_vec((first.0 as f32, first.1 as f32, first.2 as f32));
        let mut mx = mn;
        for &(du, dy, dv) in local_blocks.keys() {
            let w = s
                .transform
                .rotation
                .apply_vec((du as f32, dy as f32, dv as f32));
            mn.0 = mn.0.min(w.0);
            mn.1 = mn.1.min(w.1);
            mn.2 = mn.2.min(w.2);
            mx.0 = mx.0.max(w.0 + 1.0);
            mx.1 = mx.1.max(w.1 + 1.0);
            mx.2 = mx.2.max(w.2 + 1.0);
        }
        let aabb_min = Vec3::new(au + mn.0, ay + mn.1, av + mn.2);
        let aabb_max = Vec3::new(au + mx.0, ay + mx.1, av + mx.2);

        // Slab‑test ray / AABB (absolute frame).
        let inv_dir = Vec3::new(1.0 / dir.x, 1.0 / dir.y, 1.0 / dir.z);
        let t1 = (aabb_min - Vec3::new(ou, oy, ov)) * inv_dir;
        let t2 = (aabb_max - Vec3::new(ou, oy, ov)) * inv_dir;
        let tmin = t1.min(t2);
        let tmax = t1.max(t2);
        let t0 = tmin.x.max(tmin.y).max(tmin.z);
        let t1 = tmax.x.min(tmax.y).min(tmax.z);
        if t1 < 0.0 || t0 > t1 {
            continue;
        }
        if let Some((best_d, _)) = &best
            && t0 >= *best_d
        {
            continue;
        }

        // Transform the ray into the structure's canonical local space.
        let (lox, loy, loz) = s
            .transform
            .rotation
            .inverse()
            .apply_vec((ou - au, oy - ay, ov - av));
        let (ldx, ldy, ldz) = s
            .transform
            .rotation
            .inverse()
            .apply_vec((dir.x, dir.y, dir.z));
        let reg = world.reg();
        let is_hit = |b: BlockId| b != AIR && !reg.is_fluid(b);

        if let Some(flat) = cast(
            Vec3::new(lox, loy, loz),
            Vec3::new(ldx, ldy, ldz),
            max_dist,
            |p| s.blocks.get(&p).copied().unwrap_or(AIR),
            is_hit,
        ) {
            let Some(world_block) = s.world_position(flat.block) else {
                continue;
            };
            let cu = world_block.u() as f32 + 0.5;
            let cy = world_block.y() as f32 + 0.5;
            let cv = world_block.v() as f32 + 0.5;
            let d = (cu - ou) * dir.x + (cy - oy) * dir.y + (cv - ov) * dir.z;
            if best.as_ref().is_none_or(|(best_d, _)| d < *best_d) {
                best = Some((
                    d,
                    TargetHit::Structure {
                        id: s.id,
                        block: flat.block,
                        adjacent: flat.adjacent,
                    },
                ));
            }
        }
    }
    best.map(|(_, t)| t)
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
        // hidden settlement cell: behaves as air for aiming
        if !world.is_hidden(current)
            && block != crate::registry::AIR
            && (hit_water || !world.reg.is_fluid(block))
        {
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
