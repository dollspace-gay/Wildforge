//! Deterministic map colors, PNG encoding, and oblique globe preview.

use crate::chunk::SEA_LEVEL;
use crate::planet_atlas::identity::mix64;
use crate::planet_atlas::{AtlasError, PlanetAtlas};
use std::fs;
use std::io::BufWriter;
use std::path::Path;

pub(in crate::planet_atlas::diagnostics) fn scalar_color(
    value: f64,
    minimum: f64,
    maximum: f64,
) -> [u8; 3] {
    let t = if maximum > minimum {
        ((value - minimum) / (maximum - minimum)).clamp(0.0, 1.0)
    } else {
        0.5
    };
    if t < 1.0 / 3.0 {
        lerp_color([10, 22, 72], [20, 190, 210], t * 3.0)
    } else if t < 2.0 / 3.0 {
        lerp_color([20, 190, 210], [245, 220, 70], (t - 1.0 / 3.0) * 3.0)
    } else {
        lerp_color([245, 220, 70], [180, 25, 35], (t - 2.0 / 3.0) * 3.0)
    }
}

pub(in crate::planet_atlas::diagnostics) fn categorical_color(value: u64) -> [u8; 3] {
    if value == 0 || value == u64::from(u32::MAX) {
        return [5, 7, 12];
    }
    let mixed = mix64(value ^ 0x4154_4c41_534d_4150);
    [
        48 + (mixed & 0x9f) as u8,
        48 + ((mixed >> 16) & 0x9f) as u8,
        48 + ((mixed >> 32) & 0x9f) as u8,
    ]
}

pub(in crate::planet_atlas::diagnostics) fn direction_color(angle: f64) -> [u8; 3] {
    let phase = ((angle / std::f64::consts::TAU) + 1.0).fract() * 6.0;
    let segment = phase.floor() as u8;
    let x = ((1.0 - (phase % 2.0 - 1.0).abs()) * 220.0) as u8;
    match segment {
        0 => [235, x, 25],
        1 => [x, 235, 25],
        2 => [25, 235, x],
        3 => [25, x, 235],
        4 => [x, 25, 235],
        _ => [235, 25, x],
    }
}

fn lerp_color(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    std::array::from_fn(|index| {
        (f64::from(a[index]) + (f64::from(b[index]) - f64::from(a[index])) * t) as u8
    })
}

pub(in crate::planet_atlas::diagnostics) fn write_png(
    path: &Path,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Result<(), AtlasError> {
    let file = fs::File::create(path)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_gamma(png::ScaledFloat::new(1.0 / 2.2));
    let mut writer = encoder.write_header().map_err(|error| {
        AtlasError::Corrupt(format!("PNG header for {}: {error}", path.display()))
    })?;
    writer.write_image_data(pixels).map_err(|error| {
        AtlasError::Corrupt(format!("PNG data for {}: {error}", path.display()))
    })?;
    Ok(())
}

pub(in crate::planet_atlas::diagnostics) fn export_globe_preview(
    atlas: &PlanetAtlas,
    path: &Path,
) -> Result<(), AtlasError> {
    const SIDE: u32 = 512;
    let mut pixels = vec![0u8; SIDE as usize * SIDE as usize * 3];
    let mut depth = vec![f32::NEG_INFINITY; SIDE as usize * SIDE as usize];
    for (pos, geometry) in atlas.genesis.geometry.iter() {
        let direction = geometry.unit_direction;
        // A fixed oblique view shows latitude and more than one cube face.
        let view_x = direction[0] * 0.866 + direction[2] * 0.5;
        let view_z = -direction[0] * 0.354 + direction[1] * 0.707 + direction[2] * 0.612;
        let view_y = direction[0] * 0.354 + direction[1] * 0.707 - direction[2] * 0.612;
        if view_z <= 0.0 {
            continue;
        }
        let x = (((view_x * 0.47 + 0.5) * SIDE as f32) as i32).clamp(0, SIDE as i32 - 1);
        let y = (((-view_y * 0.47 + 0.5) * SIDE as f32) as i32).clamp(0, SIDE as i32 - 1);
        let biome = atlas.genesis.biomes.get(pos).expect("matching atlas grids");
        let terrain = atlas
            .genesis
            .terrain
            .get(pos)
            .expect("matching atlas grids");
        let color = if terrain.eroded_elevation <= SEA_LEVEL as f32 {
            [22, 72, 145]
        } else {
            categorical_color(u64::from(biome.baseline_biome) + 1)
        };
        for dy in -1..=1 {
            for dx in -1..=1 {
                let px = (x + dx).clamp(0, SIDE as i32 - 1) as u32;
                let py = (y + dy).clamp(0, SIDE as i32 - 1) as u32;
                let index = (py * SIDE + px) as usize;
                if view_z > depth[index] {
                    depth[index] = view_z;
                    pixels[index * 3..index * 3 + 3].copy_from_slice(&color);
                }
            }
        }
    }
    write_png(path, SIDE, SIDE, &pixels)
}
