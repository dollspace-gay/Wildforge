//! Pack maps scenarios.

use super::*;

#[test]
fn material_atlas_authors_ice_and_pack_override_clears_it() {
    use crate::atlas::{ATLAS_TILES, build_atlas, builtin_slots};
    let ice = *builtin_slots().get("ice").unwrap();
    let grass = *builtin_slots().get("grass_top").unwrap();
    let stone = *builtin_slots().get("stone").unwrap();
    let channel_extreme = |material: &[u8], px: u32, slot: u16, channel: usize, maximum: bool| {
        let tile_px = px / ATLAS_TILES;
        let (tx, ty) = (
            slot as u32 % ATLAS_TILES * tile_px,
            slot as u32 / ATLAS_TILES * tile_px,
        );
        let mut result = if maximum { 0 } else { 255 };
        for y in 0..tile_px {
            for x in 0..tile_px {
                let index = (((ty + y) * px + tx + x) * 4) as usize;
                result = if maximum {
                    result.max(material[index + channel])
                } else {
                    result.min(material[index + channel])
                };
            }
        }
        result
    };

    let atlas = build_atlas(&[], &[], &[]);
    assert_eq!(atlas.material.len(), (atlas.px * atlas.px * 4) as usize);
    assert_eq!(
        tile_center(&atlas.material, atlas.px, grass),
        [255, 0, 0, 0]
    );
    assert_eq!(
        channel_extreme(&atlas.material, atlas.px, ice, 0, false),
        255
    );
    assert!(channel_extreme(&atlas.material, atlas.px, ice, 1, false) > 0);
    assert!(channel_extreme(&atlas.material, atlas.px, stone, 0, false) < 240);

    let pack = tmp_dir("packice");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/ice.png"), 8, 8, [200, 220, 255, 255]);
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &[]);
    assert_eq!(channel_extreme(&atlas.material, atlas.px, ice, 1, true), 0);
}

#[test]
fn pack_companion_maps_author_normals_and_height() {
    use crate::atlas::{ATLAS_TILES, build_atlas, builtin_slots};
    let stone = *builtin_slots().get("stone").unwrap();
    let grass = *builtin_slots().get("grass_top").unwrap();
    let uniform_channel = |image: &[u8], px: u32, slot: u16, channel: usize| {
        let tile_px = px / ATLAS_TILES;
        let (tx, ty) = (
            slot as u32 % ATLAS_TILES * tile_px,
            slot as u32 / ATLAS_TILES * tile_px,
        );
        let first = image[((ty * px + tx) * 4) as usize + channel];
        for y in 0..tile_px {
            for x in 0..tile_px {
                let index = (((ty + y) * px + tx + x) * 4) as usize;
                if image[index + channel] != first {
                    return None;
                }
            }
        }
        Some(first)
    };

    let base = build_atlas(&[], &[], &[]);
    assert!(
        base.normal
            .chunks_exact(4)
            .all(|pixel| pixel == [128, 128, 255, 255])
    );
    assert!(base.material.chunks_exact(4).all(|pixel| pixel[2] == 0));

    let pack = tmp_dir("packmaps");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/stone.png"), 8, 8, [90, 90, 90, 255]);
    write_solid_png(&pack.join("tiles/stone_n.png"), 8, 8, [180, 60, 240, 255]);
    write_solid_png(&pack.join("tiles/stone_h.png"), 8, 8, [64, 64, 64, 255]);
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack.clone())], &[]);
    assert!(atlas.warnings.is_empty(), "{:?}", atlas.warnings);
    assert_eq!(
        tile_center(&atlas.normal, atlas.px, stone),
        [180, 60, 240, 255]
    );
    assert_eq!(
        uniform_channel(&atlas.material, atlas.px, stone, 2),
        Some(255)
    );
    assert_eq!(
        uniform_channel(&atlas.material, atlas.px, stone, 0),
        Some(64)
    );
    assert_eq!(
        uniform_channel(&atlas.material, atlas.px, grass, 2),
        Some(0)
    );

    let names = vec![("stone_n".to_string(), 20)];
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &names);
    assert_eq!(tile_center(&atlas.color, atlas.px, 20), [180, 60, 240, 255]);
    assert_eq!(
        tile_center(&atlas.normal, atlas.px, stone),
        [128, 128, 255, 255]
    );
}

#[test]
fn greyscale_companion_maps_load() {
    use crate::atlas::{ATLAS_TILES, build_atlas, builtin_slots};
    // Height/interior maps are greyscale by nature; every editor and generator
    // writes them single-channel. Rejecting that failed silently but for a warning.
    let pack = tmp_dir("packgrey");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/gravel.png"), 8, 8, [90, 90, 90, 255]);
    let mut data = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut data, 8, 8);
        enc.set_color(png::ColorType::Grayscale);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .unwrap()
            .write_image_data(&[77u8; 64])
            .unwrap();
    }
    std::fs::write(pack.join("tiles/gravel_h.png"), data).unwrap();
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &[]);
    assert!(
        atlas.warnings.is_empty(),
        "greyscale map should load: {:?}",
        atlas.warnings
    );
    let gravel = *builtin_slots().get("gravel").unwrap();
    let tp = atlas.px / ATLAS_TILES;
    let i = (((gravel as u32 / ATLAS_TILES * tp) * atlas.px + gravel as u32 % ATLAS_TILES * tp) * 4)
        as usize;
    assert_eq!(atlas.material[i], 77, "greyscale height reached material R");
}
