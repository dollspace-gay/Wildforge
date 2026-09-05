//! Canonical province partition, cached labels, and neighborhood queries.

use super::{Biome, Generator, Province, ProvinceKey, ProvinceLabel, hash2};
use crate::planet::{FACE_BLOCKS, Face, SurfacePos, geodesic_distance};
use crate::chunk::SEA_LEVEL;

impl Generator {
    /// Provinces: the world's countries. A jittered-grid Voronoi
    /// partition (the plate trick at a smaller scale) whose climate is
    /// sampled ONCE at the site — so a province has one biome, not a
    /// per-column vote that flips a forest into a desert and back
    /// across a hundred blocks. This is the unit a place can be named
    /// by, and the territory a heart owns.
    pub const PROVINCE_CELLS: u8 = 9;
    /// Life fades across this fringe rather than ending at a line.
    pub(super) const PROVINCE_BLEND: f32 = 70.0;

    pub(super) fn province_key_at(pos: SurfacePos) -> ProvinceKey {
        let cells = u32::from(Self::PROVINCE_CELLS);
        ProvinceKey {
            face: pos.face(),
            u: ((u32::from(pos.u()) * cells) / u32::from(FACE_BLOCKS)).min(cells - 1) as u8,
            v: ((u32::from(pos.v()) * cells) / u32::from(FACE_BLOCKS)).min(cells - 1) as u8,
        }
    }

    pub(super) fn province_nominal_center(face: Face, u: i32, v: i32) -> SurfacePos {
        let cells = i32::from(Self::PROVINCE_CELLS);
        let side = i32::from(FACE_BLOCKS);
        let center_u = ((u * 2 + 1) * side) / (cells * 2);
        let center_v = ((v * 2 + 1) * side) / (cells * 2);
        SurfacePos::canonicalized(face, center_u, center_v)
            .expect("a nearby province-grid center canonicalizes")
    }

    /// Walk the finite country grid through a face seam.
    pub fn province_offset(&self, key: ProvinceKey, du: i32, dv: i32) -> ProvinceKey {
        if let Some(atlas) = &self.atlas {
            let pos = crate::planet_atlas::AtlasPos {
                face: key.face,
                u: u16::from(key.u),
                v: u16::from(key.v),
            };
            let cell = f64::from(atlas.cell_blocks());
            let canonical = crate::planet::canonicalize_surface_point(
                pos.face,
                (f64::from(pos.u) + 0.5 + f64::from(du)) * cell,
                (f64::from(pos.v) + 0.5 + f64::from(dv)) * cell,
            )
            .expect("a nearby atlas-country offset canonicalizes");
            let surface = SurfacePos::new(
                canonical.point.face,
                canonical
                    .point
                    .u
                    .floor()
                    .clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
                canonical
                    .point
                    .v
                    .floor()
                    .clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
            )
            .expect("clamped atlas-country offset");
            return self.province_at(surface).key;
        }
        let pos =
            Self::province_nominal_center(key.face, i32::from(key.u) + du, i32::from(key.v) + dv);
        Self::province_key_at(pos)
    }

    pub(super) fn province_site(&self, key: ProvinceKey) -> SurfacePos {
        if let Some(atlas) = &self.atlas {
            let center = crate::planet_atlas::AtlasPos {
                face: key.face,
                u: u16::from(key.u),
                v: u16::from(key.v),
            }
            .center(atlas.side());
            return SurfacePos::new(
                center.face,
                center.u.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
                center.v.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
            )
            .expect("atlas country heart center is canonical");
        }
        let cells = u32::from(Self::PROVINCE_CELLS);
        let side = u32::from(FACE_BLOCKS);
        let lo_u = u32::from(key.u) * side / cells;
        let hi_u = (u32::from(key.u) + 1) * side / cells;
        let lo_v = u32::from(key.v) * side / cells;
        let hi_v = (u32::from(key.v) + 1) * side / cells;
        let packed = i32::from(key.u) | (i32::from(key.v) << 8);
        let h = hash2(
            self.seed ^ 0x9120_11ce ^ (key.face as u32).wrapping_mul(0x9e37_79b9),
            packed,
            i32::from(key.face as u8),
        );
        let jitter_u = 0.18 + (h & 0xffff) as f64 / 65536.0 * 0.64;
        let jitter_v = 0.18 + ((h >> 16) & 0xffff) as f64 / 65536.0 * 0.64;
        let u = f64::from(lo_u) + f64::from(hi_u - lo_u) * jitter_u;
        let v = f64::from(lo_v) + f64::from(hi_v - lo_v) * jitter_v;
        SurfacePos::new(
            key.face,
            u.floor().min(f64::from(FACE_BLOCKS - 1)) as u16,
            v.floor().min(f64::from(FACE_BLOCKS - 1)) as u16,
        )
        .expect("a jittered province site stays inside its canonical cell")
    }

    pub fn province_center_at(&self, key: ProvinceKey) -> SurfacePos {
        self.province_site(key)
    }

    /// The label and site of a province, computed once and kept.
    pub(super) fn province_label(&self, key: ProvinceKey) -> ProvinceLabel {
        if let Some(hit) = self
            .province_cache
            .read()
            .ok()
            .and_then(|c| c.get(&key).copied())
        {
            return hit;
        }
        let site = self.province_site(key);
        let cl = self.climate_at(site);
        let biome = if self.plate_relief(&cl) > 30.0 {
            Biome::Mountains
        } else if self.offset_base.at(cl.c) < SEA_LEVEL as f32 - 5.0 {
            Biome::Ocean
        } else {
            self.classify(&cl)
        };
        let out = (biome, site, cl.t, cl.h, cl.e);
        if let Ok(mut c) = self.province_cache.write() {
            c.insert(key, out);
        }
        out
    }

    pub fn province_at(&self, pos: SurfacePos) -> Province {
        if let Some(atlas) = &self.atlas {
            let sample = atlas.biome_sample(pos);
            let climate = self.climate_at(pos);
            let Some(country) = atlas.country(sample.country_id) else {
                // Oceans have no terrestrial country. Falling through to the
                // legacy province grid here recursively re-entered the atlas
                // offset path, so every ocean chunk eventually exhausted the
                // thread stack. Give unowned cells an explicit local label.
                let atlas_pos = atlas.atlas_pos(pos);
                let key = ProvinceKey {
                    face: atlas_pos.face,
                    u: atlas_pos.u as u8,
                    v: atlas_pos.v as u8,
                };
                let site = self.province_site(key);
                let biome = Biome::from_index(sample.zonal_biome).unwrap_or(Biome::Ocean);
                return Province {
                    key,
                    site,
                    biome,
                    neighbor: biome,
                    edge: f32::from(atlas.cell_blocks()),
                    t: climate.t,
                    h: climate.h,
                    e: climate.e,
                    nt: climate.t,
                    nh: climate.h,
                    ne: climate.e,
                };
            };
            let key = ProvinceKey {
                face: country.heart_site.face,
                u: country.heart_site.u as u8,
                v: country.heart_site.v as u8,
            };
            let site = self.province_site(key);
            let biome = if atlas.atlas_pos(pos) == country.heart_site {
                Biome::from_index(country.heart_form).unwrap_or(Biome::Plains)
            } else {
                Biome::from_index(country.dominant_biome).unwrap_or(Biome::Plains)
            };
            let mut neighbor = biome;
            for adjacent in atlas.atlas_pos(pos).neighbors4(atlas.side()) {
                let adjacent_cell = atlas.genesis.biomes.values()[adjacent.index(atlas.side())];
                if adjacent_cell.country_id != 0 && adjacent_cell.country_id != sample.country_id {
                    neighbor = atlas
                        .country(adjacent_cell.country_id)
                        .and_then(|record| Biome::from_index(record.dominant_biome))
                        .unwrap_or(biome);
                    break;
                }
            }
            return Province {
                key,
                site,
                biome,
                neighbor,
                edge: if neighbor == biome {
                    f32::from(atlas.cell_blocks())
                } else {
                    f32::from(atlas.cell_blocks()) * 0.5
                },
                t: climate.t,
                h: climate.h,
                e: climate.e,
                nt: climate.t,
                nh: climate.h,
                ne: climate.e,
            };
        }
        let home = Self::province_key_at(pos);
        let mut candidates = Vec::with_capacity(25);
        for du in -2..=2 {
            for dv in -2..=2 {
                let key = self.province_offset(home, du, dv);
                if !candidates.contains(&key) {
                    candidates.push(key);
                }
            }
        }
        let mut best = (f64::MAX, home);
        let mut second = (f64::MAX, home);
        for key in candidates {
            let site = self.province_site(key);
            let d = geodesic_distance(pos.center(), site.center());
            if d < best.0 {
                second = best;
                best = (d, key);
            } else if d < second.0 {
                second = (d, key);
            }
        }
        let edge = ((second.0 - best.0) * 0.5) as f32;
        let (biome, site, t, h, e) = self.province_label(best.1);
        // The neighbor only matters inside the border fringe; deep in
        // a country nobody asks who lives next door.
        let (neighbor, nt, nh, ne) = if edge < Self::PROVINCE_BLEND {
            let n = self.province_label(second.1);
            (n.0, n.2, n.3, n.4)
        } else {
            (biome, t, h, e)
        };
        Province {
            key: best.1,
            site,
            biome,
            neighbor,
            edge,
            t,
            h,
            e,
            nt,
            nh,
            ne,
        }
    }

    /// The form raised by the country heart at this site. Local zonal
    /// ecology can differ (a dry hummock inside a swamp, or taiga around a
    /// tundra country's stone), so heart lifecycle code must not infer this
    /// from `biome_at`.
    pub fn heart_biome_at(&self, pos: SurfacePos) -> Biome {
        if let Some(atlas) = &self.atlas {
            let sample = atlas.biome_sample(pos);
            if let Some(country) = atlas.country(sample.country_id) {
                return Biome::from_index(country.heart_form).unwrap_or(Biome::Plains);
            }
        }
        self.province_at(pos).biome
    }

    /// How close this column is to its country's heart, 0..1. Squared
    /// off so the thickening reads as a gradient you can walk up
    /// rather than a hard edge you cross.
    pub fn heart_nearness_at(&self, pos: SurfacePos) -> f32 {
        let p = self.province_at(pos);
        let site = self.province_center_at(p.key);
        let d = geodesic_distance(pos.center(), site.center()) as f32;
        // Readable from about a third of the way across a province,
        // which is roughly where you would give up and grid-search.
        const REACH: f32 = 300.0;
        (1.0 - (d / REACH).min(1.0)).powi(2)
    }

    /// Country hearts close enough to affect or intersect a local chunk.
    /// Atlas-backed worlds scan the sparse 400–600-record manifest; legacy
    /// fixtures retain their old nearby-grid walk.
    pub fn province_keys_near(&self, pos: SurfacePos, radius_blocks: f64) -> Vec<ProvinceKey> {
        if let Some(atlas) = &self.atlas {
            return atlas
                .biomes
                .countries
                .iter()
                .filter(|country| {
                    geodesic_distance(pos.center(), country.heart_site.center(atlas.side()))
                        <= radius_blocks
                })
                .map(|country| ProvinceKey {
                    face: country.heart_site.face,
                    u: country.heart_site.u as u8,
                    v: country.heart_site.v as u8,
                })
                .collect();
        }
        let home = self.province_at(pos).key;
        let mut out = Vec::new();
        for du in -2..=2 {
            for dv in -2..=2 {
                let key = self.province_offset(home, du, dv);
                if !out.contains(&key) {
                    out.push(key);
                }
            }
        }
        out
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub fn province(&self, wx: i32, wz: i32) -> Province {
        let pos = SurfacePos::from_centered(Face::PosZ, wx, wz)
            .expect("test province query is inside the positive-Z face");
        self.province_at(pos)
    }

}
