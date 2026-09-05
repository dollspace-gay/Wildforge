//! Climate and biome classification from immutable atlas/noise inputs.

use super::{Biome, Climate, Generator, Tectonics, CENTROIDS};
use crate::chunk::SEA_LEVEL;
use crate::planet::{Direction4, SurfacePos, step4, surface_to_unit};
#[cfg(test)]
use super::hash2;
#[cfg(test)]
use noise::NoiseFn;

impl Generator {
    #[cfg(test)]
    pub(super) const PLATE_SIZE: f64 = 1400.0;

    #[cfg(test)]
    pub(super) fn plate_center(&self, px: i32, pz: i32) -> (f64, f64) {
        let h = hash2(self.seed ^ 0x91a7e, px, pz);
        (
            (px as f64 + 0.15 + ((h & 0xffff) as f64 / 65536.0) * 0.7) * Self::PLATE_SIZE,
            (pz as f64 + 0.15 + (((h >> 16) & 0xffff) as f64 / 65536.0) * 0.7) * Self::PLATE_SIZE,
        )
    }

    #[cfg(test)]
    pub(super) fn plate_vel(&self, px: i32, pz: i32) -> (f32, f32) {
        let a = (hash2(self.seed ^ 0x7ec70, px, pz) % 6283) as f32 / 1000.0;
        (a.cos(), a.sin())
    }

    #[cfg(test)]
    pub(super) fn plate_oceanic(&self, px: i32, pz: i32) -> bool {
        hash2(self.seed ^ 0x0c00, px, pz) % 10 < 4
    }

    /// The static plate map: jittered-grid Voronoi cells, each with a
    /// deterministic (conceptual) drift vector and crust kind. Nearest
    /// two centers give the boundary; the closing speed across it
    /// decides fold ranges, trenches, and rifts.
    #[cfg(test)]
    pub fn tectonics(&self, wx: i32, wz: i32) -> Tectonics {
        let gx = (wx as f64 / Self::PLATE_SIZE).floor() as i32;
        let gz = (wz as f64 / Self::PLATE_SIZE).floor() as i32;
        let mut best = (f64::MAX, 0i32, 0i32);
        let mut second = (f64::MAX, 0i32, 0i32);
        for dx in -1..=1 {
            for dz in -1..=1 {
                let (px, pz) = (gx + dx, gz + dz);
                let (cx, cz) = self.plate_center(px, pz);
                let d = (cx - wx as f64).hypot(cz - wz as f64);
                if d < best.0 {
                    second = best;
                    best = (d, px, pz);
                } else if d < second.0 {
                    second = (d, px, pz);
                }
            }
        }
        let (ax, az) = self.plate_center(best.1, best.2);
        let (bx, bz) = self.plate_center(second.1, second.2);
        let (mut nx, mut nz) = ((bx - ax) as f32, (bz - az) as f32);
        let nl = (nx * nx + nz * nz).sqrt().max(1e-3);
        nx /= nl;
        nz /= nl;
        let (vax, vaz) = self.plate_vel(best.1, best.2);
        let (vbx, vbz) = self.plate_vel(second.1, second.2);
        Tectonics {
            boundary_dist: ((second.0 - best.0) * 0.5) as f32,
            convergence: ((vax - vbx) * nx + (vaz - vbz) * nz) * 0.5,
            along: wx as f32 * -nz + wz as f32 * nx,
            oceanic: self.plate_oceanic(best.1, best.2),
            neighbor_oceanic: self.plate_oceanic(second.1, second.2),
        }
    }

    #[cfg(test)]
    pub fn climate(&self, wx: i32, wz: i32) -> Climate {
        let x = wx as f64;
        let z = wz as f64;
        let tec = self.tectonics(wx, wz);
        // Continents are plate-shaped now: crust kind sets the base
        // level, blended across boundaries, with the old perlin as
        // coastline wiggle and inland variety.
        let crust = |oceanic: bool| if oceanic { -0.62 } else { 0.28 };
        let own = crust(tec.oceanic);
        let other = crust(tec.neighbor_oceanic);
        let blend = (tec.boundary_dist / 260.0).clamp(0.0, 1.0);
        let base_c = own * blend + (own + other) * 0.5 * (1.0 - blend);
        let c = base_c + self.cont.get([x / 900.0, z / 900.0]) as f32 * 0.45;
        let e = self.ero.get([x / 700.0 + 13.5, z / 700.0 - 7.2]) as f32;
        let r_raw = self.ridge.get([x / 400.0 - 3.3, z / 400.0 + 21.7]) as f32;
        let r = 1.0 - (2.0 * r_raw.abs() - 1.0).abs(); // folded, 0..1
        // Slow fields: ~2500-block features. At the old 0.0026 the
        // climate turned over every ~385 blocks, which is what a
        // per-column classifier turned into confetti — and a province
        // needs to be small against its climate for its site to speak
        // for the whole country.
        let t = self.temperature.get([x * 0.0004, z * 0.0004]) as f32;
        let h = self.moisture.get([x * 0.0004 + 31.7, z * 0.0004 - 17.3]) as f32;
        Climate { t, h, c, e, r, tec }
    }

    /// Seam-safe planetary climate. Every field is sampled from the embedded
    /// unit direction; latitude supplies the broad temperature belt while
    /// low-frequency 3D noise breaks it into recognizable regions.
    pub fn climate_at(&self, pos: SurfacePos) -> Climate {
        let unit = surface_to_unit(pos.center());
        let c = Self::noise_at(&self.cont, pos, 1_650.0, [0.0, 0.0, 0.0]) * 1.15;
        let e = Self::noise_at(&self.ero, pos, 720.0, [13.5, -7.2, 4.1]);
        let ridge_raw = Self::noise_at(&self.ridge, pos, 410.0, [-3.3, 21.7, 8.9]);
        let r = 1.0 - (2.0 * ridge_raw.abs() - 1.0).abs();
        let lat_heat = 1.0 - 2.0 * unit.y.abs() as f32;
        let t = (lat_heat * 0.82
            + Self::noise_at(&self.temperature, pos, 2_300.0, [2.7, -4.9, 8.3]) * 0.34)
            .clamp(-1.0, 1.0);
        let h = (Self::noise_at(&self.moisture, pos, 1_900.0, [31.7, -17.3, 11.9])
            + Self::noise_at(&self.moisture, pos, 520.0, [-9.1, 6.4, 23.0]) * 0.28)
            .clamp(-1.0, 1.0);

        // A continuous stand-in for static plate readings. Zero crossings of
        // the folded ridge field are boundaries; a second vector field says
        // whether the two sides converge or part.
        let boundary_dist = ridge_raw.abs() * 760.0;
        let convergence = Self::noise_at(&self.detail, pos, 1_100.0, [47.0, -19.0, 5.0]) * 0.75;
        let along = Self::noise_at(&self.bandwarp, pos, 280.0, [3.0, 7.0, 13.0]) * 2_000.0;
        let own_c = Self::noise_at(&self.cont, pos, 1_650.0, [0.0, 0.0, 0.0]);
        let across = step4(pos, Direction4::East).pos;
        let neighbor_c = Self::noise_at(&self.cont, across, 1_650.0, [0.0, 0.0, 0.0]);
        let mut tec = Tectonics {
            boundary_dist,
            convergence,
            along,
            oceanic: own_c < -0.12,
            neighbor_oceanic: neighbor_c < -0.12,
        };
        let (mut t, mut h, mut c, mut e) = (t, h, c, e);
        if let Some(atlas) = &self.atlas {
            let climate = atlas.climate_sample(pos.center());
            let terrain = atlas.terrain_sample(pos.center());
            let geology = atlas.geology_sample(pos.center());
            t = (climate.mean_temperature / 32.0).clamp(-1.0, 1.0);
            h = ((climate.mean_precipitation - 900.0) / 750.0).clamp(-1.0, 1.0);
            c = ((terrain.eroded_elevation - SEA_LEVEL as f32) / 48.0).clamp(-1.0, 1.0);
            e = (((terrain.base_elevation - terrain.eroded_elevation) / 7.0) * 2.0 - 1.0)
                .clamp(-1.0, 1.0);
            tec = Tectonics {
                boundary_dist: geology.boundary_distance_blocks,
                convergence: match geology.boundary {
                    crate::planet_atlas::DetailedBoundary::ContinentalCollision
                    | crate::planet_atlas::DetailedBoundary::OceanContinentSubduction
                    | crate::planet_atlas::DetailedBoundary::OceanOceanSubduction => {
                        geology.boundary_strength
                    }
                    crate::planet_atlas::DetailedBoundary::ContinentalRift
                    | crate::planet_atlas::DetailedBoundary::OceanRidge => {
                        -geology.boundary_strength
                    }
                    _ => 0.0,
                },
                along: geology.boundary_strike.x * f32::from(pos.u())
                    + geology.boundary_strike.y * f32::from(pos.v()),
                oceanic: geology.continental_fraction < 0.5,
                neighbor_oceanic: match geology.boundary {
                    crate::planet_atlas::DetailedBoundary::ContinentalCollision
                    | crate::planet_atlas::DetailedBoundary::ContinentalRift => false,
                    crate::planet_atlas::DetailedBoundary::OceanRidge
                    | crate::planet_atlas::DetailedBoundary::OceanOceanSubduction => true,
                    crate::planet_atlas::DetailedBoundary::OceanContinentSubduction => {
                        geology.continental_fraction >= 0.5
                    }
                    _ => geology.continental_fraction < 0.5,
                },
            };
        }
        Climate { t, h, c, e, r, tec }
    }

    /// Planetary biome classification used by generation and typed callers.
    pub fn biome_at(&self, pos: SurfacePos) -> Biome {
        // The Deep has no biomes; dungeons read as generic grassland so
        // habitat checks behave predictably below (capability E10).
        if pos.face().is_deep() {
            return Biome::Plains;
        }
        if let Some(atlas) = &self.atlas {
            let sample = atlas.biome_sample(pos);
            if sample.habitat_flags
                & (crate::planet_atlas::HABITAT_AQUATIC_FRESH
                    | crate::planet_atlas::HABITAT_AQUATIC_BRACKISH
                    | crate::planet_atlas::HABITAT_AQUATIC_SALT)
                != 0
            {
                return Biome::Ocean;
            }
            if sample.habitat_flags & crate::planet_atlas::HABITAT_WETLAND != 0
                && !matches!(
                    sample.zonal_biome,
                    crate::planet_atlas::BIOME_ARCTIC
                        | crate::planet_atlas::BIOME_TUNDRA
                        | crate::planet_atlas::BIOME_MOUNTAINS
                )
            {
                return Biome::Swamp;
            }
            return Biome::from_index(sample.zonal_biome).unwrap_or(Biome::Plains);
        }
        let climate = self.climate_at(pos);
        if self.plate_relief(&climate) > 30.0 {
            Biome::Mountains
        } else if self.offset_base.at(climate.c) < SEA_LEVEL as f32 - 5.0 {
            Biome::Ocean
        } else {
            let province = self.province_at(pos);
            let (mut t, mut h, mut e) = (province.t, province.h, province.e);
            if province.neighbor != province.biome && province.edge < Self::PROVINCE_BLEND {
                let depth = (province.edge / Self::PROVINCE_BLEND).clamp(0.0, 1.0);
                let fringe = self.hash_surface(0x0051_f16e, pos) as f32 / u32::MAX as f32;
                if fringe > 0.5 + depth * 0.5 {
                    t = province.nt;
                    h = province.nh;
                    e = province.ne;
                }
            }
            self.classify(&Climate { t, h, e, ..climate })
        }
    }

    #[cfg(test)]
    pub fn biome(&self, wx: i32, wz: i32) -> Biome {
        self.biome_from_at(wx, wz, &self.climate(wx, wz))
    }

    /// The biome a column reads as: its province's label, dithered
    /// with the neighbor's through the border fringe (so a forest
    /// thins into plains instead of ending at a line), with terrain
    /// keeping its local veto.
    #[cfg(test)]
    pub fn biome_from_at(&self, wx: i32, wz: i32, cl: &Climate) -> Biome {
        // A young fold range is Mountains whatever the country says.
        if self.plate_relief(cl) > 30.0 {
            return Biome::Mountains;
        }
        let p = self.province(wx, wz);
        // Culture from the country, terrain from the column: the
        // province fixes temperature and humidity across its whole
        // extent (that is what stops the confetti), while sea level
        // and relief stay local — so a coast is still a coast and a
        // basin is still a basin inside a single country.
        let (mut zt, mut zh, mut ze) = (p.t, p.h, p.e);
        if p.neighbor != p.biome && p.edge < Self::PROVINCE_BLEND {
            // Interleave the two zones across the fringe: near the
            // border it is a coin the noise flips, deep in it never is.
            let f = (p.edge / Self::PROVINCE_BLEND).clamp(0.0, 1.0);
            // Coarse enough that the fringe reads as fingers of one
            // country reaching into the other, not as static.
            let n = self.detail.get([wx as f64 / 55.0, wz as f64 / 55.0]) as f32;
            if n * 0.5 + 0.5 > 0.5 + f * 0.5 {
                zt = p.nt;
                zh = p.nh;
                ze = p.ne;
            }
        }
        self.classify(&Climate {
            t: zt,
            h: zh,
            e: ze,
            ..*cl
        })
    }

    /// Nearest-centroid classification of one climate sample. Provinces
    /// are labelled with this at their site; nothing else should call
    /// it per-column (that was the patchwork).
    pub(super) fn classify(&self, cl: &Climate) -> Biome {
        let mut best = Biome::Plains;
        let mut best_d = f32::MAX;
        for (biome, t, h, c, e) in CENTROIDS {
            // Mountains only exist meaningfully inland — the same land mask
            // that gates their height gates the biome label.
            if biome == Biome::Mountains && cl.c < 0.05 {
                continue;
            }
            let d = (cl.t - t).powi(2)
                + (cl.h - h).powi(2)
                + (cl.c - c).powi(2) * 1.5
                + (cl.e - e).powi(2);
            if d < best_d {
                best_d = d;
                best = biome;
            }
        }
        best
    }

}
