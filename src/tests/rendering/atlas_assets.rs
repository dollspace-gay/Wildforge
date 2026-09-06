//! Atlas assets scenarios.

use super::*;

#[test]
fn wood_leaf_tiles_are_opaque_in_atlas() {
    // Regression: a tile painted past the row boundary once left spruce
    // leaves transparent (invisible canopies).
    let reg = base_reg();
    let atlas = crate::atlas::build_atlas(&reg.tex_files, &[], &reg.tex_names);
    let img = atlas.color;
    let px = atlas.px;
    let tp = px / crate::atlas::ATLAS_TILES;
    for name in [
        "base:leaves",
        "base:birch_leaves",
        "base:spruce_leaves",
        "base:jungle_leaves",
        "base:acacia_leaves",
    ] {
        let id = reg.block_id(name).unwrap();
        let slot = reg.block(id).tiles[0] as u32;
        let cx = (slot % crate::atlas::ATLAS_TILES) * tp + tp / 2;
        let cy = (slot / crate::atlas::ATLAS_TILES) * tp + tp / 2;
        let i = ((cy * px + cx) * 4) as usize;
        assert_eq!(img[i + 3], 255, "{name} tile center must be opaque");
        assert!(img[i + 1] > img[i], "{name} should be green-ish");
    }
}

#[test]
fn block_albedo_reports_the_colour_a_block_bounces() {
    // The bounce grid stores one albedo per block, and every colour a room
    // takes on comes through it. If it ever reported grey for the plasters the
    // GI demo would go quietly monochrome and still look plausible, so pin the
    // channel ordering here rather than trusting a screenshot to catch it.
    let reg = base_reg();
    let atlas = crate::atlas::build_atlas(&reg.tex_files, &[], &reg.tex_names);
    let albedo = reg.block_albedo(&crate::atlas::slot_albedo(&atlas.color, atlas.px));

    let of = |name: &str| albedo[reg.block_id(name).unwrap().0 as usize];
    let (red, blue, white) = (
        of("base:red_plaster"),
        of("base:blue_plaster"),
        of("base:white_plaster"),
    );
    assert!(
        red[0] as u16 > 2 * red[1] as u16 && red[0] > red[2],
        "red plaster should be dominantly red, got {red:?}"
    );
    assert!(
        blue[2] as u16 > 2 * blue[1] as u16 && blue[2] > blue[0],
        "blue plaster should be dominantly blue, got {blue:?}"
    );
    // Near-neutral and bright: it is the surface the bounce is read against,
    // so a tint of its own would be indistinguishable from the effect.
    let span = white.iter().max().unwrap() - white.iter().min().unwrap();
    assert!(
        white[0] > 150 && span < 40,
        "white plaster should be bright and neutral, got {white:?}"
    );
}

#[test]
fn atlas_builds_with_mod_texture() {
    let root = tmp_dir("atlasmod");
    let dir = root.join("texmod");
    std::fs::create_dir_all(dir.join("textures")).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"texmod\"\nworld_api = 2\n").unwrap();
    std::fs::write(
        dir.join("blocks.toml"),
        "[[block]]\nid = \"red\"\ntexture = \"red.png\"\n",
    )
    .unwrap();
    // 4x4 solid red PNG.
    let mut png_data = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png_data, 4, 4);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .unwrap()
            .write_image_data(&[255, 0, 0, 255].repeat(16))
            .unwrap();
    }
    std::fs::write(dir.join("textures/red.png"), png_data).unwrap();

    let reg = registry::load(&root);
    let red = reg.block_id("texmod:red").expect("block with png");
    let slot = reg.block(red).tiles[0];
    assert!(
        slot >= crate::atlas::FIRST_FREE_SLOT,
        "mod texture gets a free slot"
    );
    assert!(
        reg.tex_names.contains(&("texmod/red".to_string(), slot)),
        "mod texture is pack-addressable by <mod_id>/<stem>: {:?}",
        reg.tex_names
    );
    let atlas = crate::atlas::build_atlas(&reg.tex_files, &[], &reg.tex_names);
    let img = atlas.color;
    let px = atlas.px;
    let tp = px / crate::atlas::ATLAS_TILES;
    let tx = (slot as u32 % crate::atlas::ATLAS_TILES) * tp + tp / 2;
    let ty = (slot as u32 / crate::atlas::ATLAS_TILES) * tp + tp / 2;
    let i = ((ty * px + tx) * 4) as usize;
    assert_eq!(
        &img[i..i + 4],
        &[255, 0, 0, 255],
        "png blitted into its slot"
    );
}

#[test]
fn missing_texture_uses_placeholder_not_crash() {
    let root = tmp_dir("misstex");
    let dir = root.join("m");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"m\"\nworld_api = 2\n").unwrap();
    std::fs::write(
        dir.join("blocks.toml"),
        "[[block]]\nid = \"x\"\ntexture = \"nope.png\"\n",
    )
    .unwrap();
    let reg = registry::load(&root);
    let x = reg.block_id("m:x").unwrap();
    assert_eq!(reg.block(x).tiles[0], crate::atlas::UNKNOWN_SLOT);
    let m = reg.mods.iter().find(|m| m.id == "m").unwrap();
    assert!(m.error.as_deref().unwrap_or("").contains("missing texture"));
}

#[test]
fn export_tiles_round_trip_reproduces_atlas() {
    let atlas = crate::atlas::build_atlas(&[], &[], &[]);
    let img = atlas.color;
    let px = atlas.px;
    let out = tmp_dir("packexport");
    let n = crate::atlas::export_tiles(&out, &img, px, &[]).unwrap();
    assert_eq!(
        n,
        crate::atlas::builtin_slots().len(),
        "every named builtin tile exported"
    );
    assert!(out.join("pack.toml").exists(), "stub pack.toml written");
    assert!(out.join("tiles/stone.png").exists());
    // Selecting the exported skeleton as a pack reproduces the atlas exactly.
    let atlas = crate::atlas::build_atlas(&[], &[crate::atlas::PackSource::Dir(out.clone())], &[]);
    let again = atlas.color;
    let apx = atlas.px;
    let warns = atlas.warnings;
    assert!(warns.is_empty(), "{warns:?}");
    assert_eq!(apx, px);
    assert_eq!(again, img, "export -> re-import is the identity");
}

#[test]
fn embedded_gemini_pack_applies_without_folder() {
    let tiles = crate::atlas::embedded_pack("gemini").expect("gemini compiled in");
    assert!(tiles.len() > 100, "full pack embedded, got {}", tiles.len());
    assert!(crate::atlas::embedded_pack("nope").is_none());
    let atlas = crate::atlas::build_atlas(&[], &[], &[]);
    let base = atlas.color;
    let bpx = atlas.px;
    let atlas = crate::atlas::build_atlas(&[], &[crate::atlas::PackSource::Embedded(tiles)], &[]);
    let img = atlas.color;
    let px = atlas.px;
    let warns = atlas.warnings;
    assert!(warns.is_empty(), "{warns:?}");
    assert_eq!(px, bpx);
    let stone = *crate::atlas::builtin_slots().get("stone").unwrap();
    assert_ne!(
        tile_center(&img, px, stone),
        tile_center(&base, bpx, stone),
        "embedded pack repaints stone over the procedural base"
    );
    // The built-in pack is always discoverable, folder or not.
    let listed = crate::atlas::discover_packs();
    assert!(
        listed.iter().any(|p| p.id == "gemini"),
        "gemini listed: {listed:?}"
    );
}

#[test]
fn model_boxes_can_carry_their_own_texture() {
    let reg = base_reg();
    let deer = &reg.animals[reg.animal_id("base:deer").unwrap()];
    let antler_slot = *crate::atlas::builtin_slots().get("antler").unwrap();
    let antlers: Vec<_> = deer
        .model
        .iter()
        .filter(|b| b.name.starts_with("antler"))
        .collect();
    assert_eq!(antlers.len(), 2, "deer has an antler pair");
    for b in &antlers {
        assert_eq!(
            b.tile,
            Some(antler_slot),
            "antlers use the bone tile, not fur"
        );
    }
    assert!(
        deer.model
            .iter()
            .filter(|b| b.name.starts_with("tine"))
            .count()
            == 2,
        "antlers branch"
    );
    let body = deer.model.iter().find(|b| b.name == "body").unwrap();
    assert_eq!(body.tile, None, "body stays on the fur tile");
}

#[test]
fn atlas_layout_is_slot_stable() {
    use crate::atlas::{ATLAS_TILES, FIRST_FREE_SLOT, build_procedural, builtin_slots};
    assert_eq!(ATLAS_TILES, 64, "the atlas is a 64x64 grid (4096 slots)");
    let tp = 8u32;
    let img = build_procedural(tp);
    let px = ATLAS_TILES * tp;
    assert_eq!(
        img.len() as u32,
        px * px * 4,
        "side = ATLAS_TILES * tile_px"
    );
    // Slot numbers are stable identifiers; only layout derives from them.
    let sample = |slot: u32| -> [u8; 4] {
        let (tx, ty) = (
            slot % ATLAS_TILES * tp + tp / 2,
            slot / ATLAS_TILES * tp + tp / 2,
        );
        let i = ((ty * px + tx) * 4) as usize;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    };
    // Grass top (slot 0): green, painted.
    let g = sample(0);
    assert!(
        g[1] > g[0] && g[1] > g[2] && g[3] == 255,
        "grass at slot 0: {g:?}"
    );
    // The magenta missing-texture checkerboard still lives at its slot.
    let unk = *builtin_slots().get("unknown").unwrap() as u32;
    let u = sample(unk);
    assert!(
        (u[0] > 180 && u[2] > 180) || (u[0] < 40 && u[2] < 40),
        "unknown checkerboard at slot {unk}: {u:?}"
    );
    // Every builtin slot sits under the mod floor OR in the reserved
    // player region at the top (extra bases + derived variants), and
    // no two names share.
    let slots = builtin_slots();
    let mut seen = std::collections::HashSet::new();
    for (name, &slot) in slots.iter() {
        assert!(
            !(FIRST_FREE_SLOT..crate::style::EXTRA_BASE).contains(&slot),
            "{name} outside builtin/reserved regions"
        );
        assert!(seen.insert(slot), "slot {slot} ({name}) is unique");
    }
    // A slot in the second half of the grid (impossible under 16x16)
    // resolves to sane coordinates.
    assert!(FIRST_FREE_SLOT as u32 <= ATLAS_TILES * ATLAS_TILES);
}
