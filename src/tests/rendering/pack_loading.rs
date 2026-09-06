//! Pack loading scenarios.

use super::*;

#[test]
fn pack_discovery_lists_folders() {
    let root = tmp_dir("packdisc");
    std::fs::create_dir_all(root.join("zeta")).unwrap();
    std::fs::create_dir_all(root.join("alpha")).unwrap();
    std::fs::write(
        root.join("alpha/pack.toml"),
        "name = \"Alpha Pack\"\ndescription = \"test pack\"\n",
    )
    .unwrap();
    std::fs::write(root.join("stray.txt"), "not a pack").unwrap();
    let packs = crate::atlas::discover_packs_in(&root);
    assert_eq!(packs.len(), 2, "only directories count");
    assert_eq!(packs[0].id, "alpha");
    assert_eq!(packs[0].name, "Alpha Pack");
    assert_eq!(packs[0].description, "test pack");
    assert_eq!(packs[1].id, "zeta");
    assert_eq!(
        packs[1].name, "zeta",
        "missing pack.toml falls back to dir name"
    );
}

#[test]
fn pack_tile_override_applied_at_slot() {
    let pack = tmp_dir("packstone");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/stone.png"), 8, 8, [255, 0, 255, 255]);
    let atlas = crate::atlas::build_atlas(&[], &[], &[]);
    let base = atlas.color;
    let bpx = atlas.px;
    let atlas = crate::atlas::build_atlas(&[], &[crate::atlas::PackSource::Dir(pack.clone())], &[]);
    let img = atlas.color;
    let px = atlas.px;
    let warns = atlas.warnings;
    assert_eq!(px, bpx);
    assert!(warns.is_empty(), "no warnings: {warns:?}");
    let stone = *crate::atlas::builtin_slots().get("stone").unwrap();
    let dirt = *crate::atlas::builtin_slots().get("dirt").unwrap();
    assert_eq!(
        tile_center(&img, px, stone),
        [255, 0, 255, 255],
        "stone repainted"
    );
    assert_eq!(
        tile_center(&img, px, dirt),
        tile_center(&base, bpx, dirt),
        "untargeted tile falls through to base"
    );
}

#[test]
fn pack_overrides_mod_tile_by_name_and_wins() {
    let modtex = tmp_dir("packmodtex");
    let slot = crate::atlas::FIRST_FREE_SLOT;
    write_solid_png(&modtex.join("ruby_ore.png"), 4, 4, [0, 255, 0, 255]);
    let tex_files = vec![(slot, modtex.join("ruby_ore.png"))];
    let tex_names = vec![("gems/ruby_ore".to_string(), slot)];

    let pack = tmp_dir("packgems");
    std::fs::create_dir_all(pack.join("tiles/gems")).unwrap();
    write_solid_png(
        &pack.join("tiles/gems/ruby_ore.png"),
        4,
        4,
        [255, 0, 255, 255],
    );

    // Without the pack the mod's art lands in the slot...
    let atlas = crate::atlas::build_atlas(&tex_files, &[], &tex_names);
    let img = atlas.color;
    let px = atlas.px;
    assert_eq!(tile_center(&img, px, slot), [0, 255, 0, 255]);
    // ...with the pack, the pack's art wins (layered last).
    let atlas = crate::atlas::build_atlas(
        &tex_files,
        &[crate::atlas::PackSource::Dir(pack.clone())],
        &tex_names,
    );
    let img = atlas.color;
    let px = atlas.px;
    let warns = atlas.warnings;
    assert!(warns.is_empty(), "{warns:?}");
    assert_eq!(
        tile_center(&img, px, slot),
        [255, 0, 255, 255],
        "pack > mod"
    );
}

#[test]
fn pack_unknown_and_unreadable_files_warn() {
    let pack = tmp_dir("packwarn");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/notatile.png"), 4, 4, [1, 2, 3, 255]);
    std::fs::write(pack.join("tiles/stone.png"), b"this is not a png").unwrap();
    let atlas = crate::atlas::build_atlas(&[], &[], &[]);
    let base = atlas.color;
    let bpx = atlas.px;
    let atlas = crate::atlas::build_atlas(&[], &[crate::atlas::PackSource::Dir(pack.clone())], &[]);
    let img = atlas.color;
    let px = atlas.px;
    let warns = atlas.warnings;
    assert_eq!(warns.len(), 2, "unknown name + unreadable png: {warns:?}");
    assert!(warns.iter().any(|w| w.contains("notatile")));
    let stone = *crate::atlas::builtin_slots().get("stone").unwrap();
    assert_eq!(
        tile_center(&img, px, stone),
        tile_center(&base, bpx, stone),
        "unreadable file leaves the base tile intact"
    );
}

#[test]
fn config_pack_round_trips() {
    let mut c = crate::config::Config::default();
    assert_eq!(
        c.pack, "gemini",
        "fresh installs default to the bundled pack"
    );
    c.pack = "dusk".to_string();
    let parsed = crate::config::Config::from_text(&c.to_text());
    assert_eq!(parsed, c, "config text round-trips the pack field");
    c.pack = String::new();
    let parsed = crate::config::Config::from_text(&c.to_text());
    assert!(parsed.pack.is_empty(), "explicit NONE round-trips as none");
    let legacy = crate::config::Config::from_text("volume=0.5\n");
    assert_eq!(
        legacy.pack, "gemini",
        "configs predating packs get the default"
    );
}

#[test]
fn content_stamp_changes_on_pack_edit() {
    let root = tmp_dir("packstamp");
    std::fs::create_dir_all(root.join("dusk/tiles")).unwrap();
    write_solid_png(&root.join("dusk/tiles/stone.png"), 4, 4, [9, 9, 9, 255]);
    let before = crate::content_tree_stamp_of(&[&root]);
    write_solid_png(&root.join("dusk/tiles/dirt.png"), 4, 4, [9, 9, 9, 255]);
    let after = crate::content_tree_stamp_of(&[&root]);
    assert_ne!(
        before, after,
        "adding a pack file re-stamps the content tree"
    );
}

#[test]
fn finer_pack_tiles_are_averaged_down_not_point_sampled() {
    use crate::atlas::{ATLAS_TILES, build_atlas, builtin_slots};
    let stone = *builtin_slots().get("stone").unwrap();
    let pack = tmp_dir("packfine");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    let base = build_atlas(&[], &[], &[]);
    let tile_px = base.px / ATLAS_TILES;
    write_checker_png(
        &pack.join("tiles/stone.png"),
        tile_px * 4,
        1,
        [0, 0, 0, 255],
        [255, 255, 255, 255],
    );
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &[]);
    let tx = stone as u32 % ATLAS_TILES * tile_px;
    let ty = stone as u32 / ATLAS_TILES * tile_px;
    for y in 0..tile_px {
        for x in 0..tile_px {
            let index = (((ty + y) * atlas.px + tx + x) * 4) as usize;
            assert!((atlas.color[index] as i32 - 127).abs() <= 1);
        }
    }
}
