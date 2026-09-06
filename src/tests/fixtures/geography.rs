//! Deterministic atlas and biome selection for subsystem scenarios.

use crate::worldgen::{Biome, Generator};

pub(in crate::tests) fn find_biomes(
    g: &Generator,
    want: Biome,
    n: usize,
) -> Vec<crate::planet::SurfacePos> {
    let mut out = Vec::new();
    for face in crate::planet::Face::ALL {
        for u in 0..Generator::PROVINCE_CELLS {
            for v in 0..Generator::PROVINCE_CELLS {
                let key = crate::worldgen::ProvinceKey { face, u, v };
                let site = g.province_center_at(key);
                let sample = crate::planet::SurfacePos::canonicalized(
                    site.face(),
                    i32::from(site.u()) + 48,
                    i32::from(site.v()) + 48,
                )
                .expect("a country-interior sample canonicalizes");
                if g.biome_at(sample) == want
                    && g.province_at(sample).key == g.province_at(site).key
                    && g.surface_estimate_at(sample) > crate::chunk::SEA_LEVEL + 2
                    && g.plate_relief(&g.climate_at(sample)) <= 30.0
                {
                    out.push(sample);
                    if out.len() >= n {
                        return out;
                    }
                }
            }
        }
    }
    out
}

pub(in crate::tests) fn find_biome(
    g: &Generator,
    want: Biome,
) -> Option<crate::planet::SurfacePos> {
    // Dry land off a fold range: a province center can sit under the
    // sea or on a peak, and neither grows what the country grows.
    find_biome_where(g, want, |pos| {
        g.surface_estimate_at(pos) > crate::chunk::SEA_LEVEL + 2
            && g.plate_relief(&g.climate_at(pos)) <= 30.0
    })
}

/// As `find_biome`, but the caller adds conditions (dry, inland, off
/// a plate boundary). Rays miss whole countries now that provinces
/// are ~900 blocks; this spirals a grid at half-province spacing.
pub(in crate::tests) fn find_biome_where(
    g: &Generator,
    want: Biome,
    pred: impl Fn(crate::planet::SurfacePos) -> bool,
) -> Option<crate::planet::SurfacePos> {
    for face in crate::planet::Face::ALL {
        for u in 0..Generator::PROVINCE_CELLS {
            for v in 0..Generator::PROVINCE_CELLS {
                let key = crate::worldgen::ProvinceKey { face, u, v };
                let site = g.province_center_at(key);
                let sample = crate::planet::SurfacePos::canonicalized(
                    site.face(),
                    i32::from(site.u()) + 48,
                    i32::from(site.v()) + 48,
                )
                .expect("a country-interior sample canonicalizes");
                if g.biome_at(sample) == want
                    && g.province_at(sample).key == g.province_at(site).key
                    && pred(sample)
                {
                    return Some(sample);
                }
            }
        }
    }
    None
}

pub(in crate::tests) fn find_water_features(
    generator: &crate::worldgen::Generator,
    wanted: usize,
) -> Vec<(crate::planet::SurfacePos, i32)> {
    let mut found: Vec<(crate::planet::SurfacePos, i32)> = Vec::new();
    'faces: for face in crate::planet::Face::ALL {
        for u in (8..crate::planet::FACE_BLOCKS).step_by(16) {
            for v in (8..crate::planet::FACE_BLOCKS).step_by(16) {
                let pos = crate::planet::SurfacePos::new(face, u, v).unwrap();
                let Some(fill) = generator.water_features_at(pos) else {
                    continue;
                };
                if fill <= crate::chunk::SEA_LEVEL + 3
                    || found.iter().any(|(other, _)| {
                        crate::planet::geodesic_distance(pos.center(), other.center()) < 200.0
                    })
                {
                    continue;
                }
                found.push((pos, fill));
                if found.len() >= wanted {
                    break 'faces;
                }
            }
        }
    }
    found
}
