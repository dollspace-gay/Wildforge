//! Spawn rescue coordinator for the authoritative world.

use super::{AIR, CHUNK_Y, ChunkPos, SEA_LEVEL, World, standing};

#[cfg(test)]
use glam::Vec3;

impl World {
    /// Somewhere a player can be put down: dry, solid-footed, and
    /// above the tideline. Searches outward from the asked column and,
    /// finding nothing but open water, raises a small sand island
    /// rather than dropping anyone into the sea.
    #[cfg(test)]
    pub fn safe_spawn(&mut self, x: i32, z: i32) -> Vec3 {
        let surface = crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
            .expect("legacy spawn query is inside PosZ");
        self.safe_spawn_at(surface).local()
    }

    pub fn safe_spawn_at(&mut self, wanted: crate::planet::SurfacePos) -> crate::planet::EntityPos {
        let stands_dry = |w: &mut World,
                          surface: crate::planet::SurfacePos|
         -> Option<crate::planet::EntityPos> {
            w.ensure_chunk(ChunkPos::from_surface(surface));
            let h = w.surface_height_at(surface);
            let feet = h + 1;
            (h > SEA_LEVEL && w.standable_at(surface, feet)).then(|| {
                crate::planet::EntityPos::new(
                    surface.face(),
                    surface.u() as f32 + 0.5,
                    feet as f32 + 0.2,
                    surface.v() as f32 + 0.5,
                )
                .expect("surface cell center is a canonical entity position")
            })
        };
        if let Some(p) = stands_dry(self, wanted) {
            return p;
        }
        // Rings outward: near land first, so a coastal spawn walks
        // ashore instead of conjuring ground it didn't need. The ring
        // is probed with the generator's cheap height estimate, and
        // only a column that looks dry is actually generated — this
        // search used to build hundreds of chunks per join.
        for r in (4..=96).step_by(4) {
            for (dx, dz) in [
                (r, 0),
                (-r, 0),
                (0, r),
                (0, -r),
                (r, r),
                (-r, -r),
                (r, -r),
                (-r, r),
            ] {
                let Ok(surface) = crate::planet::SurfacePos::canonicalized(
                    wanted.face(),
                    wanted.u() as i32 + dx,
                    wanted.v() as i32 + dz,
                ) else {
                    continue;
                };
                if self.generator.surface_estimate_at(surface) <= SEA_LEVEL + 1 {
                    continue;
                }
                if let Some(p) = stands_dry(self, surface) {
                    return p;
                }
            }
        }
        // Open ocean in every direction: make landfall.
        let top = self.raise_castaway_isle_at(wanted);
        crate::planet::EntityPos::new(
            wanted.face(),
            wanted.u() as f32 + 0.5,
            top as f32 + 1.2,
            wanted.v() as f32 + 0.5,
        )
        .expect("surface cell center is a canonical entity position")
    }

    /// Raise a small sand island for a castaway spawn: a low dome up
    /// out of the water with its own patch of dry ground. Returns the
    /// height of the ground at its center.
    pub fn raise_castaway_isle_at(&mut self, center: crate::planet::SurfacePos) -> i32 {
        const R: i32 = 5;
        let sand = self.reg.block_id("base:sand").unwrap_or(AIR);
        let crest = SEA_LEVEL + 2;
        for dx in -R..=R {
            for dz in -R..=R {
                let d2 = dx * dx + dz * dz;
                if d2 > R * R {
                    continue;
                }
                let surface = crate::planet::SurfacePos::canonicalized(
                    center.face(),
                    center.u() as i32 + dx,
                    center.v() as i32 + dz,
                )
                .expect("castaway island radius crosses at most one face edge");
                self.ensure_chunk(ChunkPos::from_surface(surface));
                let at = |y: i32| {
                    crate::planet::BlockPos::new(surface.face(), surface.u(), y as u8, surface.v())
                        .expect("castaway island height is inside the shell")
                };
                // A dome: full height at the middle, shelving into the
                // water at the rim.
                let top = crest - (d2 as f32 / 6.0).round() as i32;
                let floor = (1..=SEA_LEVEL)
                    .rev()
                    .find(|&y| self.reg.is_solid(self.get_block_at(at(y))))
                    .unwrap_or(1);
                for y in floor + 1..=top {
                    self.set_block_at(at(y), sand);
                }
                // Dry it out overhead, so the island is actually air.
                for y in top + 1..=SEA_LEVEL + 4 {
                    if self.reg.is_fluid(self.get_block_at(at(y))) {
                        self.set_block_at(at(y), AIR);
                    }
                }
            }
        }
        crest
    }

    /// Resolve a requested standing position into one a body can
    /// actually occupy. Saved spawns go stale — a bedroll gets built
    /// over, terrain regenerates under an old save — and a player
    /// placed inside a hill is simply stuck. A valid spot returns
    /// unchanged; otherwise the nearest clear opening in the column
    /// wins (downward on ties, matching the old come-to-ground rule),
    /// then a ring of neighbor columns, then the column surface.
    #[cfg(test)]
    pub fn settle_spawn(&mut self, want: Vec3) -> Vec3 {
        crate::planet::EntityPos::from_local(crate::planet::Face::PosZ, want)
            .map(|want| self.settle_spawn_at(want).local())
            .unwrap_or(want)
    }

    pub fn settle_spawn_at(&mut self, want: crate::planet::EntityPos) -> crate::planet::EntityPos {
        standing::settle(self, want)
    }

    /// Free a restored *position* only if it is embedded in solid.
    /// Unlike `settle_spawn`, a legitimate mid-air or mid-swim save
    /// passes through untouched — physics owns falling and floating;
    /// this only rescues a body inside a hill.
    #[cfg(test)]
    pub fn free_position(&mut self, pos: Vec3) -> Vec3 {
        crate::planet::EntityPos::from_local(crate::planet::Face::PosZ, pos)
            .map(|pos| self.free_position_at(pos).local())
            .unwrap_or(pos)
    }

    pub fn free_position_at(&mut self, pos: crate::planet::EntityPos) -> crate::planet::EntityPos {
        let surface = crate::planet::SurfacePos::new(
            pos.face(),
            pos.u().floor() as u16,
            pos.v().floor() as u16,
        )
        .expect("canonical entity has a valid surface cell");
        self.ensure_chunk(ChunkPos::from_surface(surface));
        let feet = (pos.y.floor() as i32).clamp(1, CHUNK_Y as i32 - 2);
        let block = |y: i32| {
            crate::planet::BlockPos::new(surface.face(), surface.u(), y as u8, surface.v())
                .expect("rescued height is inside the shell")
        };
        let embedded = self.reg.is_solid(self.get_block_at(block(feet)))
            || self.reg.is_solid(self.get_block_at(block(feet + 1)));
        if embedded {
            self.settle_spawn_at(pos)
        } else {
            pos
        }
    }
}
