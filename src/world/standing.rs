//! Standing-position search shared by authority and resident-only prediction.

use super::{ReplicaWorld, TerrainRead, World};
use crate::chunk::{CHUNK_Y, ChunkPos};

/// Only authority may prepare missing columns. Replica searches retain the
/// previous remote World's resident-only behavior and air fallback.
pub(super) trait StandingTerrain: TerrainRead {
    fn prepare_standing_chunk(&mut self, position: ChunkPos);
}

impl StandingTerrain for World {
    fn prepare_standing_chunk(&mut self, position: ChunkPos) {
        self.ensure_chunk(position);
    }
}
impl StandingTerrain for ReplicaWorld {
    fn prepare_standing_chunk(&mut self, _position: ChunkPos) {}
}

pub(super) fn settle(
    terrain: &mut impl StandingTerrain,
    want: crate::planet::EntityPos,
) -> crate::planet::EntityPos {
    let surface = crate::planet::SurfacePos::new(
        want.face(),
        want.u().floor() as u16,
        want.v().floor() as u16,
    )
    .expect("canonical entity has a valid surface cell");
    terrain.prepare_standing_chunk(ChunkPos::from_surface(surface));
    let feet = (want.y.floor() as i32).clamp(1, CHUNK_Y as i32 - 2);
    if terrain.standable_at(surface, feet) {
        return want;
    }
    for d in 1..CHUNK_Y as i32 {
        for y in [feet - d, feet + d] {
            if y >= 1 && y < CHUNK_Y as i32 - 1 && terrain.standable_at(surface, y) {
                return crate::planet::EntityPos::new(
                    want.face(),
                    want.u(),
                    y as f32 + 0.2,
                    want.v(),
                )
                .expect("settled height preserves a canonical surface position");
            }
        }
    }
    // The column offers nothing (filled sky-to-bedrock): walk
    // outward for the nearest column with open ground.
    for r in 1..=8i32 {
        for dz in -r..=r {
            for dx in -r..=r {
                if dx.abs() != r && dz.abs() != r {
                    continue;
                }
                let neighbor = crate::planet::SurfacePos::canonicalized(
                    surface.face(),
                    surface.u() as i32 + dx,
                    surface.v() as i32 + dz,
                )
                .expect("spawn rescue radius crosses at most one face edge");
                terrain.prepare_standing_chunk(ChunkPos::from_surface(neighbor));
                let y = terrain.surface_height_at(neighbor) + 1;
                if y < CHUNK_Y as i32 - 1 && terrain.standable_at(neighbor, y) {
                    return crate::planet::EntityPos::new(
                        neighbor.face(),
                        neighbor.u() as f32 + 0.5,
                        y as f32 + 0.2,
                        neighbor.v() as f32 + 0.5,
                    )
                    .expect("neighbor cell center is canonical");
                }
            }
        }
    }
    // Last resort: on top of whatever this column calls surface.
    let y = terrain.surface_height_at(surface) + 1;
    crate::planet::EntityPos::new(want.face(), want.u(), y as f32 + 0.2, want.v())
        .expect("settled height preserves a canonical surface position")
}
