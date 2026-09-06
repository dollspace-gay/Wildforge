//! Seeded mineral observations shared by generation tooling and guest picks.

use super::super::{ProspectHit, ProspectReading};
use super::Geography;
use crate::chunk::{CHUNK_X, CHUNK_Z, ChunkPos};
use crate::planet::{SurfacePos, geodesic_distance};

impl Geography {
    /// Does a granite pluton intrude this column at mineable depth?
    /// Mirrors sample_lattice's threshold math. The census measures
    /// with it; the prospecting pick reads with it.
    pub fn pluton_at_surface(&self, pos: SurfacePos) -> bool {
        if let Some(atlas) = &self.atlas {
            return atlas
                .nearest_intrusion(pos.center(), 420.0)
                .is_some_and(|site| {
                    geodesic_distance(pos.center(), site.pos.center(atlas.side()))
                        <= f64::from(site.radius_blocks)
                });
        }
        let prov = Self::radial_noise_at(&self.granite3d, pos, 77.7, 1_400.0, [0.0; 3]);
        let prov_pen = (0.44 - prov).max(0.0) * 1.8;
        for y in [16.0f64, 32.0, 48.0, 64.0] {
            let g = Self::radial_noise_at(&self.granite3d, pos, y, 230.0, [0.0; 3]);
            if g > 0.55 + prov_pen + y as f32 * 0.0012 {
                return true;
            }
        }
        false
    }

    /// One prospecting reading: what regional geology lies near this
    /// spot, and roughly which way. Everything here is a pure function
    /// of seed and position — the pick reveals, it never rolls.
    /// Detection reaches are deliberately shorter than the rarity
    /// bands: mapping a region takes a SWEEP of readings (surveying is
    /// work, which is what makes a finished survey worth trading).
    pub fn prospect_at(&self, pos: SurfacePos) -> ProspectReading {
        if let Some(atlas) = &self.atlas {
            let hit = |target: crate::planet_atlas::AtlasPos, radius: f64| {
                let distance = geodesic_distance(pos.center(), target.center(atlas.side()));
                ProspectHit {
                    distance: (distance - radius).max(0.0).round() as i32,
                    bearing: crate::planet::great_circle_bearing(
                        pos.center(),
                        target.center(atlas.side()),
                    ),
                }
            };
            let geology = atlas.geology_sample(pos.center());
            let pluton = atlas
                .nearest_intrusion(pos.center(), 1_216.0)
                .map(|site| hit(site.pos, f64::from(site.radius_blocks)));
            let volcano = atlas
                .nearest_volcano(pos.center(), 1_600.0)
                .map(|site| hit(site.pos, f64::from(site.edifice_radius_blocks)));
            let pipe = atlas
                .nearest_deposit(
                    pos.center(),
                    crate::planet_atlas::MineralKind::Diamond,
                    768.0,
                )
                .map(|site| hit(site.pos, f64::from(site.radius_blocks)));
            let geode = atlas
                .nearest_deposit(pos.center(), crate::planet_atlas::MineralKind::Geode, 384.0)
                .map(|site| hit(site.pos, f64::from(site.radius_blocks)));
            return ProspectReading {
                province_name: atlas
                    .province_name(geology.geological_province)
                    .map(str::to_string),
                bedrock: Some(geology.bedrock),
                pluton,
                volcano,
                pipe,
                geode,
            };
        }
        let reading = |target: SurfacePos| ProspectHit {
            distance: geodesic_distance(pos.center(), target.center()).round() as i32,
            bearing: crate::planet::great_circle_bearing(pos.center(), target.center()),
        };
        let ring = |step: i32, cap: i32, hit: &dyn Fn(SurfacePos) -> bool| {
            if hit(pos) {
                return Some(reading(pos));
            }
            let mut r = step;
            while r <= cap {
                let mut i = -r;
                while i <= r {
                    for (dx, dz) in [(i, -r), (i, r), (-r, i), (r, i)] {
                        if let Ok(target) = SurfacePos::canonicalized(
                            pos.face(),
                            i32::from(pos.u()) + dx,
                            i32::from(pos.v()) + dz,
                        ) && hit(target)
                        {
                            return Some(reading(target));
                        }
                    }
                    i += step;
                }
                r += step;
            }
            None
        };
        let cp = ChunkPos::from_surface(pos);
        let chunk_ring = |cap: i32, hit: &dyn Fn(ChunkPos) -> bool| {
            if hit(cp) {
                return Some(reading(pos));
            }
            for r in 1..=cap {
                let mut i = -r;
                while i <= r {
                    for (dx, dz) in [(i, -r), (i, r), (-r, i), (r, i)] {
                        let target_chunk = cp.offset(dx, dz);
                        if hit(target_chunk) {
                            let target = SurfacePos::new(
                                target_chunk.face(),
                                target_chunk.u() * CHUNK_X as u16 + CHUNK_X as u16 / 2,
                                target_chunk.v() * CHUNK_Z as u16 + CHUNK_Z as u16 / 2,
                            )
                            .expect("chunk center is canonical");
                            return Some(reading(target));
                        }
                    }
                    i += 1;
                }
            }
            None
        };
        ProspectReading {
            province_name: None,
            bedrock: None,
            pluton: ring(64, 1216, &|surface| self.pluton_at_surface(surface)),
            // Goal-1's temporary planetary generator does not stamp the old
            // planar volcano regions.
            volcano: None,
            pipe: chunk_ring(24, &|p| self.pipe_at(p).is_some()),
            geode: chunk_ring(12, &|p| self.geode_at(p).is_some()),
        }
    }
}
