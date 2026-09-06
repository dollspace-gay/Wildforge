//! Atlas, model, presentation, light-director, and shader behavior.

use super::*;

const STRATA_NAMES: [&str; 8] = [
    "sandstone",
    "limestone",
    "shale",
    "granite",
    "marble",
    "slate",
    "quartzite",
    "basalt",
];

fn read_strata_png(name: &str) -> (u32, u32, Vec<[u8; 3]>) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("base/textures")
        .join(format!("{name}.png"));
    let file = std::fs::File::open(path).unwrap();
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().unwrap();
    let mut buffer = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buffer).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb, "{name} must be RGB8");
    let pixels = buffer[..info.buffer_size()]
        .chunks_exact(3)
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    (info.width, info.height, pixels)
}

fn atlas_slot_bytes(image: &[u8], px: u32, slot: u16) -> Vec<u8> {
    let tile_px = px / crate::atlas::ATLAS_TILES;
    let origin_x = slot as u32 % crate::atlas::ATLAS_TILES * tile_px;
    let origin_y = slot as u32 / crate::atlas::ATLAS_TILES * tile_px;
    let mut tile = Vec::with_capacity((tile_px * tile_px * 4) as usize);
    for y in 0..tile_px {
        let start = (((origin_y + y) * px + origin_x) * 4) as usize;
        tile.extend_from_slice(&image[start..start + (tile_px * 4) as usize]);
    }
    tile
}

fn linear_luminance(pixel: [u8; 3]) -> f32 {
    let linear = |channel: u8| {
        let value = f32::from(channel) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(pixel[0]) + 0.7152 * linear(pixel[1]) + 0.0722 * linear(pixel[2])
}

fn large_scale_signature(pixels: &[[u8; 3]], width: usize) -> (Vec<f32>, f32) {
    const SIDE: usize = 8;
    let cell = width / SIDE;
    let mut reduced = Vec::with_capacity(SIDE * SIDE);
    for tile_y in 0..SIDE {
        for tile_x in 0..SIDE {
            let mut total = 0.0;
            for y in tile_y * cell..(tile_y + 1) * cell {
                for x in tile_x * cell..(tile_x + 1) * cell {
                    total += linear_luminance(pixels[y * width + x]);
                }
            }
            reduced.push(total / (cell * cell) as f32);
        }
    }
    let mean = reduced.iter().sum::<f32>() / reduced.len() as f32;
    for value in &mut reduced {
        *value -= mean;
    }
    let rms =
        (reduced.iter().map(|value| value * value).sum::<f32>() / reduced.len() as f32).sqrt();
    let norm = reduced
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt()
        .max(1e-9);
    for value in &mut reduced {
        *value /= norm;
    }
    (reduced, rms)
}

fn write_checker_png(
    path: &std::path::Path,
    side: u32,
    cell: u32,
    first: [u8; 4],
    second: [u8; 4],
) {
    let mut pixels = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side {
        for x in 0..side {
            pixels.extend_from_slice(if ((x / cell) + (y / cell)).is_multiple_of(2) {
                &first
            } else {
                &second
            });
        }
    }
    let mut data = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut data, side, side);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
    }
    std::fs::write(path, data).unwrap();
}

mod atlas_assets;
mod fog;
mod geometry;
mod lights;
mod pack_layers;
mod pack_loading;
mod pack_maps;
mod qualification;
mod scene;
mod strata;
mod variants;
