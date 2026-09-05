//! Stable genesis identity, seed mixing, and spherical noise.

use crate::planet::{SurfacePoint, surface_to_unit};
use crate::planet_atlas::{ATLAS_ALGORITHM_VERSION, AtlasPos};
use noise::{NoiseFn, Perlin};
use std::path::{Path};

pub(in crate::planet_atlas) fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x1000_0000_01b3)
    })
}

/// Genesis compatibility covers built-in data, the externally loaded mod
/// tree, atlas algorithm generation, and the package version. It deliberately
/// excludes scripts because host scripts never define baseline voxel content.
pub fn genesis_content_hash(mods_dir: &Path) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut extend = |bytes: &[u8]| {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x1000_0000_01b3);
        }
    };
    for content in [
        include_bytes!("../../base/blocks.toml").as_slice(),
        include_bytes!("../../base/items.toml").as_slice(),
        include_bytes!("../../base/recipes.toml").as_slice(),
        include_bytes!("../../base/tags.toml").as_slice(),
        include_bytes!("../../base/features.toml").as_slice(),
        include_bytes!("../../base/aliases.toml").as_slice(),
        include_bytes!("../../base/animals.toml").as_slice(),
        include_bytes!("../../base/structures.toml").as_slice(),
        include_bytes!("../../base/workings.toml").as_slice(),
        include_bytes!("../../base/preparations.toml").as_slice(),
    ] {
        extend(content);
    }
    extend(env!("CARGO_PKG_VERSION").as_bytes());
    extend(&ATLAS_ALGORITHM_VERSION.to_le_bytes());
    extend(&crate::content_files::content_hash(mods_dir).to_le_bytes());
    hash
}

pub(in crate::planet_atlas) fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

pub(in crate::planet_atlas) fn cell_hash(seed: u32, pos: AtlasPos, salt: u64) -> u64 {
    mix64(
        u64::from(seed)
            ^ salt
            ^ (pos.face as u64) << 56
            ^ u64::from(pos.u) << 24
            ^ u64::from(pos.v),
    )
}

pub(in crate::planet_atlas) fn unit_noise(noise: &Perlin, point: SurfacePoint, scale: f64, offset: [f64; 3]) -> f32 {
    let unit = surface_to_unit(point);
    noise.get([
        unit.x * scale + offset[0],
        unit.y * scale + offset[1],
        unit.z * scale + offset[2],
    ]) as f32
}
