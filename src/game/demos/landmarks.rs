//! Landmarks capture scene construction.

use crate::chunk::ChunkPos;
use crate::game::Game;
use crate::planet::{EntityPos, SurfacePos};
use crate::world::TerrainRead;
use glam::Vec3;

impl Game {
    pub(super) fn stage_capture_edifice(&mut self, spawn: EntityPos) {
        // Dev: fly to the nearest country's heart and look at what
        // stands over it. WILDFORGE_DEMO_EDIFICE=<n> steps outward
        // through neighbouring provinces, so each family can be seen.
        if let Ok(which) = std::env::var("WILDFORGE_DEMO_EDIFICE") {
            let skip: usize = which.parse().unwrap_or(0);
            let g = &self.runtime.local().world.generator;
            let home = g.province_at(spawn.surface()).key;
            let sites: Vec<SurfacePos> = (0..6)
                .flat_map(|r: i32| {
                    (-r..=r)
                        .flat_map(move |i| [(i, -r), (i, r), (-r, i), (r, i)])
                        .collect::<Vec<_>>()
                })
                .map(|(du, dv)| g.province_center_at(g.province_offset(home, du, dv)))
                .filter(|&site| g.surface_estimate_at(site) > crate::chunk::SEA_LEVEL + 4)
                .fold(Vec::new(), |mut acc, s| {
                    // The ring walk visits (0,0) four times over.
                    if !acc.contains(&s) {
                        acc.push(s);
                    }
                    acc
                });
            if let Some(&site) = sites.get(skip) {
                let ed =
                    crate::edifice::edifice_of(self.runtime.local().world.generator.biome_at(site));
                let center = ChunkPos::from_surface(site);
                for cx in -3..=3 {
                    for cz in -3..=3 {
                        self.runtime
                            .local_mut()
                            .world
                            .ensure_chunk(center.offset(cx, cz));
                    }
                }
                let base = self.runtime.view().surface_height_at(site);
                // Stand well back and a little above the crest.
                let back = std::env::var("WILDFORGE_DEMO_BACK")
                    .ok()
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or((ed.reach * 4).max(40) as f32);
                self.player.pos = EntityPos::new(
                    site.face(),
                    f32::from(site.u()) + 0.5,
                    base as f32 + ed.rise as f32 * 0.7,
                    f32::from(site.v()) + 0.5,
                )
                .unwrap()
                .translated(Vec3::new(0.0, 0.0, back))
                .unwrap()
                .pos;
                self.player.vel = Vec3::ZERO;
                self.camera.yaw = -std::f32::consts::FRAC_PI_2;
                self.camera.pitch = -0.22;
                self.flying = true;
                eprintln!(
                    "edifice demo: {:?} {:?} at {:?} {},{base},{}",
                    self.runtime.local().world.generator.biome_at(site),
                    ed.family,
                    site.face(),
                    site.u(),
                    site.v()
                );
            }
        }
    }
}
