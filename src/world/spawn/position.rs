//! Position for the common spawn contract.

use crate::chunk::ChunkPos;
use crate::chunk::SEA_LEVEL;
use crate::planet::SurfacePos;
use crate::world::World;

impl World {
    pub(super) fn prepared_spawn_position(
        &self,
        wanted: crate::planet::EntityPos,
    ) -> Option<crate::planet::EntityPos> {
        let origin = wanted.surface();
        let mut best = None;
        let mut best_score = i32::MAX;
        for du in -32i32..=32 {
            for dv in -32i32..=32 {
                let Ok(surface) = SurfacePos::canonicalized(
                    origin.face(),
                    i32::from(origin.u()) + du,
                    i32::from(origin.v()) + dv,
                ) else {
                    continue;
                };
                if !self.chunks.contains_key(&ChunkPos::from_surface(surface)) {
                    continue;
                }
                let height = self.surface_height_at(surface);
                let feet = height + 1;
                if height <= SEA_LEVEL + 1 || !self.standable_at(surface, feet) {
                    continue;
                }
                let slope = crate::planet::neighbors4(surface)
                    .into_iter()
                    .map(|neighbor| (height - self.surface_height_at(neighbor)).abs())
                    .max()
                    .unwrap_or_default();
                if slope > 2 {
                    continue;
                }
                let score = slope * 100 + du.abs() + dv.abs();
                if score < best_score {
                    best_score = score;
                    best = crate::planet::EntityPos::new(
                        surface.face(),
                        f32::from(surface.u()) + 0.5,
                        feet as f32 + 0.2,
                        f32::from(surface.v()) + 0.5,
                    )
                    .ok();
                }
            }
        }
        best
    }
}
