//! Atlas, model, presentation, light-director, and shader behavior.

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

#[test]
fn season_tint_repaints_foliage() {
    let px = crate::atlas::ATLAS_TILES * 8;
    let summer = crate::atlas::build_procedural(8);
    let grass = *crate::atlas::builtin_slots().get("grass_top").unwrap() as u32;
    let tile_px = |img: &Vec<u8>, slot: u32| -> Vec<u8> {
        let tp = 8u32;
        let (tx, ty) = (slot % 16 * tp, slot / 16 * tp);
        let mut out = Vec::new();
        for y in ty..ty + tp {
            for x in tx..tx + tp {
                let i = ((y * px + x) * 4) as usize;
                out.extend_from_slice(&summer[i..i + 3]);
                let _ = img;
            }
        }
        out
    };
    let reference = tile_px(&summer, grass);
    for season in [0usize, 2, 3] {
        let mut img = summer.clone();
        crate::atlas::season_tint(&mut img, px, season);
        let tp = 8u32;
        let (tx, ty) = (
            grass % crate::atlas::ATLAS_TILES * tp,
            grass / crate::atlas::ATLAS_TILES * tp,
        );
        let mut changed = false;
        let mut k = 0;
        for y in ty..ty + tp {
            for x in tx..tx + tp {
                let i = ((y * px + x) * 4) as usize;
                changed |= img[i..i + 3] != reference[k..k + 3];
                k += 3;
            }
        }
        assert!(changed, "season {season} repaints grass");
    }
    let mut img = summer.clone();
    crate::atlas::season_tint(&mut img, px, 1);
    assert_eq!(img, summer, "summer is the reference look");
}

#[test]
fn particle_pool_caps_culls_and_stays_in_tile() {
    use crate::particles::{CAP, Pool};
    let mut pool = Pool::default();
    let mut rng = 12345u32;

    // Flood far past the cap; the pool holds its line.
    for _ in 0..80 {
        pool.burst(Vec3::new(0.0, 64.0, 0.0), 3, 12, 2.0, &mut rng);
    }
    assert_eq!(pool.v.len(), CAP, "pool caps at {CAP}");

    // Everything dies by max ttl (0.7s for bursts).
    pool.tick(1.0);
    assert!(pool.v.is_empty(), "ttl culls the whole pool");

    // Sub-tile UVs stay inside the source tile for corner slots.
    let tiles = crate::atlas::ATLAS_TILES;
    let ts = 1.0 / tiles as f32;
    for slot in [0u16, (tiles * tiles - 1) as u16] {
        pool.burst(Vec3::ZERO, slot, 12, 2.0, &mut rng);
        let (mut verts, mut idx) = (Vec::new(), Vec::new());
        pool.emit(&mut verts, &mut idx);
        let (tx, ty) = (slot as u32 % tiles, slot as u32 / tiles);
        for v in &verts {
            assert!(
                v.uv[0] >= tx as f32 * ts - 1e-5 && v.uv[0] <= (tx + 1) as f32 * ts + 1e-5,
                "u {} outside tile {slot}",
                v.uv[0]
            );
            assert!(
                v.uv[1] >= ty as f32 * ts - 1e-5 && v.uv[1] <= (ty + 1) as f32 * ts + 1e-5,
                "v {} outside tile {slot}",
                v.uv[1]
            );
        }
        pool.v.clear();
    }
}

#[test]
fn light_promotion_scores_and_hysteresis() {
    use crate::lights::{Key, promote};
    let (a, b, c) = (
        Key::Block(crate::planet::BlockPos::of_world(0, 1, 0).unwrap()),
        Key::Block(crate::planet::BlockPos::of_world(1, 1, 0).unwrap()),
        Key::Block(crate::planet::BlockPos::of_world(2, 1, 0).unwrap()),
    );

    // Empty slots fill best-first.
    let s = promote(&[None, None], &[(a, 1.0), (b, 3.0), (c, 2.0)], 2);
    assert_eq!(s, vec![Some(b), Some(c)]);

    // A marginally better challenger does NOT evict (hysteresis)...
    let s2 = promote(&s, &[(a, 2.2), (b, 3.0), (c, 2.0)], 2);
    assert_eq!(s2, vec![Some(b), Some(c)], "1.1x is not decisive");
    // ...a decisive one does, and takes the weakest slot.
    let s3 = promote(&s, &[(a, 2.6), (b, 3.0), (c, 2.0)], 2);
    assert_eq!(s3, vec![Some(b), Some(a)], "1.3x evicts the weakest");

    // Vanished candidates free their slot in place; slot order is stable
    // (slots are cube-map layers — stability is the cache).
    let s4 = promote(&s3, &[(a, 2.6)], 2);
    assert_eq!(s4, vec![None, Some(a)]);
}

#[test]
fn light_director_caches_until_an_edit_lands_nearby() {
    use crate::lights::{Director, DynLight, Emitter, Key};
    let mut d = Director::new();
    let torch_pos = crate::planet::BlockPos::of_world(4, 64, 4).unwrap();
    let torch = Emitter {
        pos: torch_pos,
        rgb: [14, 11, 6],
        emit: 14,
    };
    d.chunk_meshed(tchunk(0, 0), vec![torch]);
    let cam = crate::planet::block_to_render(torch_pos.surface().center(), 64.5).as_vec3();

    // Steady state: same key, same epoch -> the renderer skips all six
    // cube faces. (Flicker moves the color, never the epoch.)
    let l1 = d.frame(cam, &[], 0.016, true);
    let l2 = d.frame(cam, &[], 0.016, true);
    assert_eq!(l1.len(), 1, "torch promoted");
    assert_eq!((l1[0].key, l1[0].epoch), (l2[0].key, l2[0].epoch));
    assert!(l1[0].suppress.0 > 0.0, "static lights suppress their flood");

    // An edit in a far chunk leaves the cube cached...
    d.chunk_meshed(tchunk(8, 8), vec![]);
    let l3 = d.frame(cam, &[], 0.016, true);
    assert_eq!(l3[0].epoch, l2[0].epoch, "far edits don't invalidate");
    // ...an edit within range invalidates it.
    d.chunk_meshed(tchunk(0, 0), vec![torch]);
    let l4 = d.frame(cam, &[], 0.016, true);
    assert!(l4[0].epoch > l3[0].epoch, "near edits re-render the cube");

    // Dynamic lights: standing still is a cache hit; moving is not.
    let held = |p: Vec3| DynLight {
        key: Key::Held,
        pos: p,
        color: Vec3::new(1.8, 1.4, 0.7),
        range: 16.0,
    };
    let h1 = d.frame(cam, &[held(cam)], 0.016, true);
    let h2 = d.frame(cam, &[held(cam + Vec3::new(0.05, 0.0, 0.0))], 0.016, true);
    let (e1, e2) = (h1[1].epoch, h2[1].epoch);
    assert_eq!(e1, e2, "sub-threshold movement keeps the cube");
    assert_eq!(h2[1].suppress.0, 0.0, "dynamic lights aren't in the flood");
    let h3 = d.frame(cam, &[held(cam + Vec3::new(1.0, 0.0, 0.0))], 0.016, true);
    assert!(h3[1].epoch > e2, "real movement re-renders");

    // A world edit near a STANDING-STILL held light re-renders its
    // cube too — the bug report was shadows of walls no longer there,
    // resetting only once the player wandered past the threshold.
    let hp = cam + Vec3::new(1.0, 0.0, 0.0);
    d.chunk_meshed(tchunk(0, 0), vec![torch]);
    let h4 = d.frame(cam, &[held(hp)], 0.016, true);
    assert!(
        h4[1].epoch > h3[1].epoch,
        "a nearby remesh invalidates a still held light's cube"
    );
    // And the far chunk still doesn't.
    d.chunk_meshed(tchunk(8, 8), vec![]);
    let h5 = d.frame(cam, &[held(hp)], 0.016, true);
    assert_eq!(h5[1].epoch, h4[1].epoch, "far edits leave it cached");
}

#[test]
fn curved_chunk_meshes_join_and_cull_across_a_cube_face() {
    use std::collections::HashSet;

    use crate::planet::{BlockPos, PLANET_RADIUS};

    let reg = base_reg();
    let stone = b(&reg, "base:stone");
    for (index, seam) in directed_planet_seams().into_iter().enumerate() {
        let mut world = World::new(
            47,
            tmp_dir(&format!("planet-mesh-seam-{index}")),
            reg.clone(),
        );
        let source =
            BlockPos::new(seam.source.face(), seam.source.u(), 100, seam.source.v()).unwrap();
        let across =
            BlockPos::new(seam.across.face(), seam.across.u(), 100, seam.across.v()).unwrap();
        world.insert_empty_chunks_for_test([source.chunk(), across.chunk()]);
        for chunk in [source.chunk(), across.chunk()] {
            let chunk = world.chunks_mut().get_mut(&chunk).unwrap();
            for x in 0..crate::chunk::CHUNK_X {
                for z in 0..crate::chunk::CHUNK_Z {
                    chunk.set(x, 0, z, crate::registry::AIR);
                }
            }
        }
        world.set_block_at(source, stone);
        world.set_block_at(across, stone);

        let a = crate::mesher::mesh_chunk(&world, source.chunk(), &Default::default());
        let bmesh = crate::mesher::mesh_chunk(&world, across.chunk(), &Default::default());
        assert_eq!(
            a.opaque_idx.len() + bmesh.opaque_idx.len(),
            10 * 6,
            "directed seam {index}: shared block sides are culled"
        );
        let positions = |mesh: &crate::mesher::ChunkMesh| {
            mesh.opaque_verts
                .iter()
                .map(|vertex| vertex.pos.map(f32::to_bits))
                .collect::<HashSet<_>>()
        };
        let a_positions = positions(&a);
        let b_positions = positions(&bmesh);
        assert!(
            a_positions.intersection(&b_positions).count() >= 4,
            "directed seam {index}: all four shared-face corners must be bit-identical"
        );
        for vertex in a.opaque_verts.iter().chain(&bmesh.opaque_verts) {
            let p = Vec3::from(vertex.pos);
            assert!(
                (PLANET_RADIUS as f32..=PLANET_RADIUS as f32 + 256.0).contains(&p.length()),
                "mesh vertex is embedded radially: {:?}",
                vertex.pos
            );
            let normal = Vec3::from(vertex.normal);
            assert!((normal.length() - 1.0).abs() < 1.0e-4);
        }
        for mesh in [&a, &bmesh] {
            for triangle in mesh.opaque_idx.chunks_exact(3) {
                let p0 = Vec3::from(mesh.opaque_verts[triangle[0] as usize].pos);
                let p1 = Vec3::from(mesh.opaque_verts[triangle[1] as usize].pos);
                let p2 = Vec3::from(mesh.opaque_verts[triangle[2] as usize].pos);
                let normal = Vec3::from(mesh.opaque_verts[triangle[0] as usize].normal);
                assert!(
                    (p1 - p0).cross(p2 - p0).dot(normal) > 0.0,
                    "directed seam {index}: rendered triangle winding must face its geometric normal"
                );
            }
        }
    }
}

#[test]
fn planetary_visual_capture_manifest_is_complete() {
    use std::collections::HashSet;
    use std::path::{Component, Path};

    let assert_relative_png = |relative: &str| {
        let path = Path::new(relative);
        assert!(
            !path.is_absolute(),
            "PNG declaration is relative: {relative}"
        );
        assert_eq!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("png"),
            "PNG declaration has a .png suffix: {relative}"
        );
        assert!(
            path.components()
                .all(|component| matches!(component, Component::Normal(_))),
            "PNG declaration cannot escape the evidence directory: {relative}"
        );
    };

    let screenshots = Path::new(env!("CARGO_MANIFEST_DIR")).join("screenshots");
    let manifest_path = screenshots.join("planetary-qualification.toml");
    let text = std::fs::read_to_string(&manifest_path).unwrap_or_else(|error| {
        panic!(
            "read visual qualification manifest {}: {error}",
            manifest_path.display()
        )
    });
    let manifest: toml::Value = toml::from_str(&text).expect("parse visual manifest");
    assert_eq!(
        manifest["qualification_version"].as_integer(),
        Some(1),
        "visual manifest contract version"
    );

    let atlas = manifest["atlas"].as_table().expect("atlas evidence table");
    for field in [
        "id",
        "directory",
        "preview",
        "location",
        "season",
        "time",
        "settings",
        "purpose",
    ] {
        assert!(
            atlas
                .get(field)
                .and_then(toml::Value::as_str)
                .is_some_and(|value| !value.is_empty()),
            "atlas evidence has non-empty {field}"
        );
    }
    assert!(atlas["seed"].as_integer().is_some());
    assert!(atlas["generator_version"].as_integer().is_some());
    let atlas_root = screenshots.join(atlas["directory"].as_str().unwrap());
    assert_relative_png(atlas["preview"].as_str().unwrap());
    // The persisted manifest intentionally contains the full u64 checksum
    // domain, while generic `toml::Value` is limited to TOML's signed integer
    // domain. The production atlas loader validates those checksums; this
    // evidence test only needs the small format/completion fields.
    let atlas_manifest = std::fs::read_to_string(atlas_root.join("manifest.toml"))
        .expect("read exported atlas manifest");
    let expected_format = atlas["atlas_format_version"].as_integer().unwrap();
    assert!(
        atlas_manifest
            .lines()
            .any(|line| line.trim() == format!("format_version = {expected_format}")),
        "exported atlas format matches capture metadata"
    );
    assert!(
        atlas_manifest
            .lines()
            .any(|line| line.trim() == "complete = true"),
        "exported atlas manifest is complete"
    );
    let report: toml::Value = toml::from_str(
        &std::fs::read_to_string(atlas_root.join("validation-report.toml"))
            .expect("read atlas validation report"),
    )
    .expect("parse atlas validation report");
    assert_eq!(report["validation"].as_str(), Some("passed"));
    let registered = report["registered_layers"]
        .as_array()
        .expect("registered atlas layers");
    let exported = report["exported_maps"]
        .as_array()
        .expect("exported atlas maps");
    assert_eq!(registered.len(), 132, "current qualification layer count");
    assert_eq!(exported.len(), registered.len());
    for map in exported {
        let relative = map.as_str().expect("map path is text");
        assert_relative_png(relative);
        let path = atlas_root.join(relative);
        assert_eq!(path.parent(), Some(atlas_root.join("maps").as_path()));
        let legend = path.with_extension("legend.txt");
        assert!(
            legend.is_file(),
            "exported atlas legend exists: {}",
            legend.display()
        );
    }

    let captures = manifest["capture"].as_array().expect("visual capture list");
    assert!(
        captures.len() >= 6,
        "qualification includes representative live captures"
    );
    let mut ids = HashSet::new();
    let mut files = HashSet::new();
    for capture in captures {
        let capture = capture.as_table().expect("capture table");
        for field in [
            "id", "file", "location", "season", "time", "settings", "purpose",
        ] {
            assert!(
                capture
                    .get(field)
                    .and_then(toml::Value::as_str)
                    .is_some_and(|value| !value.is_empty()),
                "capture has non-empty {field}"
            );
        }
        assert!(capture["seed"].as_integer().is_some());
        assert!(capture["generator_version"].as_integer().is_some());
        assert!(ids.insert(capture["id"].as_str().unwrap()));
        let file = capture["file"].as_str().unwrap();
        assert!(files.insert(file), "capture files are unique");
        assert_relative_png(file);
        assert_eq!(Path::new(file).components().count(), 1);
    }
}

#[test]
fn config_lights_and_darkness_roundtrip() {
    use crate::config::Config;
    let d = Config::default();
    assert_eq!(d.lights, 2, "full shadows by default");
    assert!(d.stark, "stark by default");

    let c = Config {
        lights: 1,
        stark: false,
        ..Default::default()
    };
    let c2 = Config::from_text(&c.to_text());
    assert_eq!(c2.lights, 1);
    assert!(!c2.stark);

    let c3 = Config::from_text("lights=off\ndarkness=soft\noutline=off\n");
    assert_eq!(c3.lights, 0);
    assert!(!c3.stark);
    assert!(!c3.outline, "outline=off persists");
    assert!(Config::default().outline, "outline defaults on");
    let c4 = Config::from_text("lights=banana\ndarkness=???\n");
    assert_eq!(c4.lights, 2, "unknown value falls back to full");
    assert!(c4.stark, "unknown darkness falls back to stark");

    // Point-light shadow technique: the exact voxel-grid march is the default;
    // only an explicit `cube` opts back into the legacy distance cube map.
    assert!(d.point_grid, "grid point shadows by default");
    let c5 = Config {
        point_grid: false,
        ..Default::default()
    };
    assert!(
        !Config::from_text(&c5.to_text()).point_grid,
        "point_shadows=cube survives a round trip"
    );
    assert!(!Config::from_text("point_shadows=cube\n").point_grid);
    assert!(
        Config::from_text("point_shadows=wobble\n").point_grid,
        "unknown value falls back to grid"
    );
    assert!(
        Config::from_text("volume=0.5\n").point_grid,
        "a config predating the setting gets grid"
    );
}

#[test]
fn player_style_packs_clamps_and_names_align() {
    use crate::style::*;
    // Every field combination survives the u32 round trip.
    for skin in 0..SKIN_TONES.len() as u8 {
        for hair in 0..HAIR_COLORS.len() as u8 {
            for hair_style in 0..HAIR_STYLE_NAMES.len() as u8 {
                for beard in 0..BEARD_NAMES.len() as u8 {
                    let s = Style {
                        skin,
                        hair,
                        shirt: (hair % SHIRT_COLORS.len() as u8),
                        trousers: (skin % TROUSER_COLORS.len() as u8),
                        hair_style,
                        beard,
                        legwear: skin % 2,
                        build: hair % 3,
                    };
                    assert_eq!(Style::unpack(s.pack()), s);
                }
            }
        }
    }
    // Garbage clamps into range instead of exploding the palette index.
    let wild = Style::unpack(u32::MAX);
    assert!((wild.skin as usize) < SKIN_TONES.len());
    assert!((wild.hair as usize) < HAIR_COLORS.len());
    assert!((wild.shirt as usize) < SHIRT_COLORS.len());
    assert!((wild.trousers as usize) < TROUSER_COLORS.len());
    assert!((wild.hair_style as usize) < HAIR_STYLE_NAMES.len());
    assert!((wild.beard as usize) < BEARD_NAMES.len());
    assert!((wild.legwear as usize) < LEGWEAR_NAMES.len());
    assert!((wild.build as usize) < BUILD_NAMES.len());
    // Display names track their palettes.
    assert_eq!(HAIR_NAMES.len(), HAIR_COLORS.len());
    assert_eq!(SHIRT_NAMES.len(), SHIRT_COLORS.len());
    assert_eq!(TROUSER_NAMES.len(), TROUSER_COLORS.len());
    // Variant slots stay above the mod region (compile-time consts,
    // but the relationship is the contract worth pinning).
    let (base, span) = (VARIANT_BASE as u32, VARIANT_SLOTS as u32);
    assert!(base >= crate::atlas::FIRST_FREE_SLOT as u32);
    assert_eq!(
        base + span,
        crate::atlas::ATLAS_TILES * crate::atlas::ATLAS_TILES,
        "variants cap the grid, wherever it ends"
    );

    let c = crate::config::Config::from_text("appearance=66051\n");
    assert_eq!(c.appearance, 66051, "appearance persists in config");
}

#[test]
fn humanoid_stands_full_height_with_hands() {
    use crate::mobs::{HeldArt, HumanoidArt, emit_humanoid};
    let art = HumanoidArt {
        skin: 1,
        face: 2,
        hair: Some(3),
        hair_front: 3,
        hair_top: 4,
        beard: None,
        shirt: 5,
        trousers: 6,
        boot: 7,
        long_hair: false,
        skirt: false,
        build: 1,
    };
    let (mut verts, mut idx) = (Vec::new(), Vec::new());
    emit_humanoid(
        ep(Vec3::ZERO),
        0.0,
        &art,
        (0.0, 0.0),
        HeldArt::None,
        ([1.0; 3], 1.0),
        &mut verts,
        &mut idx,
    );
    let origin = ep(Vec3::ZERO);
    let render_origin = origin.render_pos();
    let up = crate::planet::local_frame(origin.surface_point())
        .up
        .as_vec3();
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for v in &verts {
        let radial_height = (Vec3::from_array(v.pos) - render_origin).dot(up);
        lo = lo.min(radial_height);
        hi = hi.max(radial_height);
    }
    // The sinking bug, pinned: feet at the position, head at the hitbox.
    assert!(lo > -0.01, "nothing below the feet (was: waist-deep)");
    assert!(
        (1.75..=1.87).contains(&(hi - lo)),
        "full height, got {}",
        hi - lo
    );
    // 11 parts minus the hair's skipped bottom face = 65 quads; hands
    // and hair are present or this count collapses.
    assert_eq!(idx.len() / 6, 65, "boots, hands, and hair all present");

    // A held block adds its six faces to the right hand.
    let (mut v2, mut i2) = (Vec::new(), Vec::new());
    emit_humanoid(
        ep(Vec3::ZERO),
        0.0,
        &art,
        (0.0, 0.0),
        HeldArt::Cube([9; 6]),
        ([1.0; 3], 1.0),
        &mut v2,
        &mut i2,
    );
    assert_eq!(i2.len() / 6, 71, "held cube rides the hand");

    // Shape choices add and remove real geometry.
    let quads = |art: &HumanoidArt| {
        let (mut v, mut i) = (Vec::new(), Vec::new());
        emit_humanoid(
            ep(Vec3::ZERO),
            0.0,
            art,
            (0.0, 0.0),
            HeldArt::None,
            ([1.0; 3], 1.0),
            &mut v,
            &mut i,
        );
        i.len() / 6
    };
    let base = HumanoidArt {
        skin: 1,
        face: 2,
        hair: Some(3),
        hair_front: 3,
        hair_top: 4,
        beard: None,
        shirt: 5,
        trousers: 6,
        boot: 7,
        long_hair: false,
        skirt: false,
        build: 1,
    };
    let bald = HumanoidArt { hair: None, ..base };
    assert_eq!(quads(&bald), 60, "bald drops the hair shell");
    let bearded = HumanoidArt {
        beard: Some(8),
        ..base
    };
    assert_eq!(quads(&bearded), 66, "a beard is one face band");
    let long = HumanoidArt {
        long_hair: true,
        ..base
    };
    assert_eq!(quads(&long), 71, "long hair adds the back panel");
    let skirted = HumanoidArt {
        skirt: true,
        ..base
    };
    assert_eq!(quads(&skirted), 71, "the skirt is a real box");
}

#[test]
fn atlas_derives_tinted_player_variants() {
    use crate::atlas::{ATLAS_TILES, apply_player_variants, build_procedural, builtin_slots};
    use crate::style::{SKIN_TONES, Style, skin_tile};
    let tp = 8u32;
    let mut img = build_procedural(tp);
    let px = ATLAS_TILES * tp;
    apply_player_variants(&mut img, px);
    let base = *builtin_slots().get("player_skin").unwrap();
    let sample = |slot: u16, dx: u32, dy: u32| -> [u8; 4] {
        let (tx, ty) = (
            slot as u32 % ATLAS_TILES * tp + dx,
            slot as u32 / ATLAS_TILES * tp + dy,
        );
        let i = ((ty * px + tx) * 4) as usize;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    };
    for (i, c) in SKIN_TONES.iter().enumerate() {
        let st = Style {
            skin: i as u8,
            ..Default::default()
        };
        let b = sample(base, 3, 3);
        let v = sample(skin_tile(&st), 3, 3);
        for ch in 0..3 {
            let want = (b[ch] as f32 * c[ch]).min(255.0) as u8;
            assert!(
                (v[ch] as i32 - want as i32).abs() <= 1,
                "variant = base x palette (tone {i} ch {ch})"
            );
        }
        assert_eq!(v[3], b[3], "alpha preserved");
    }
}

/// The shaders are only compiled by naga at device-init time, so a typo in the
/// WGSL would ship undetected by `cargo build`/`test`. Parse and validate both
/// shader files here to fail loudly at CI instead of on someone's screen.
#[test]
fn wgsl_shaders_validate() {
    for (name, src) in [
        ("shader.wgsl", include_str!("../shader.wgsl")),
        ("post.wgsl", include_str!("../post.wgsl")),
    ] {
        let module = naga::front::wgsl::parse_str(src)
            .unwrap_or_else(|e| panic!("{name}: WGSL parse error:\n{}", e.emit_to_string(src)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name}: WGSL validation failed: {e:?}"));
    }
}

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

#[test]
fn frozen_clock_holds_the_sun_without_stopping_the_sim() {
    let reg = base_reg();
    let make = || {
        let world = World::new(7, tmp_dir("freeze"), reg.clone());
        crate::server::Server::new(world, 0.3, 42)
    };
    let step = |server: &mut crate::server::Server| {
        for _ in 0..40 {
            server.advance(crate::server::TICK, &[], &mut Vec::new());
        }
    };

    let mut running = make();
    let start = running.time_of_day;
    step(&mut running);
    assert!(running.time_of_day > start);

    let mut frozen = make();
    frozen.freeze_clock = true;
    frozen.world.ire = 50.0;
    let start = frozen.time_of_day;
    let ire = frozen.world.ire;
    step(&mut frozen);
    assert_eq!(frozen.time_of_day, start);
    assert!(
        frozen.world.ire < ire,
        "non-calendar simulation still advances"
    );
}

#[test]
fn fluid_surfaces_stitch_at_shared_corners() {
    // Two adjacent water cells of different volumes must meet: the
    // thin cell's top corners on the shared edge rise to the full
    // cell's surface, so the water reads as one connected sheet
    // instead of disconnected tiles with gaps between their rims.
    let reg = base_reg();
    let mut w = test_world_with("stitch", reg.clone());
    let stone = b(&reg, "base:stone");
    let y = 200;
    for x in 0..8 {
        for z in 0..8 {
            w.set_block(x, y, z, stone);
        }
    }
    let full = reg.water_block(0); // volume 8, surface 8/9
    let thin = reg.water_block(5); // volume 3, surface 3/9
    w.set_block(2, y + 1, 2, full);
    w.set_block(3, y + 1, 2, thin);
    let mesh = crate::mesher::mesh_chunk(&w, tchunk(0, 0), &Default::default());
    let has = |x: i32, py: f32, z: i32| {
        let point = crate::planet::SurfacePoint {
            face: crate::planet::Face::PosZ,
            u: (x + crate::planet::FACE_BLOCKS as i32 / 2) as f64,
            v: (z + crate::planet::FACE_BLOCKS as i32 / 2) as f64,
        };
        let expected = crate::planet::block_to_render(point, py as f64).as_vec3();
        mesh.water_verts
            .iter()
            .any(|v| (Vec3::from_array(v.pos) - expected).length() < 1e-3)
    };
    let ys = (y + 1) as f32;
    // Shared edge corners sit at the full cell's height...
    assert!(has(3, ys + 8.0 / 9.0, 2), "near shared corner stitched");
    assert!(has(3, ys + 8.0 / 9.0, 3), "far shared corner stitched");
    // ...while the thin cell's outer edge keeps its own height.
    assert!(has(4, ys + 3.0 / 9.0, 2), "outer corner keeps thin height");
    // No thin-cell rim hangs at full height on the outer edge.
    assert!(
        !has(4, ys + 8.0 / 9.0, 2),
        "no floating rim on the thin side"
    );
}

#[test]
fn pack_inherits_layers_maps_over_a_parent_albedo() {
    use crate::atlas::{ATLAS_TILES, build_atlas, builtin_slots, pack_chain_in};
    // The point of inheritance: a child pack that ships only companion maps
    // rides on its parent's albedo instead of replacing the whole pack.
    let root = tmp_dir("packinherit");
    let parent = root.join("base_look");
    let child = root.join("maps_only");
    std::fs::create_dir_all(parent.join("tiles")).unwrap();
    std::fs::create_dir_all(child.join("tiles")).unwrap();
    std::fs::write(parent.join("pack.toml"), "name = \"Base Look\"\n").unwrap();
    std::fs::write(
        child.join("pack.toml"),
        "name = \"Maps Only\"\ninherits = \"base_look\"\n",
    )
    .unwrap();
    write_solid_png(&parent.join("tiles/stone.png"), 8, 8, [255, 0, 255, 255]);
    write_solid_png(&child.join("tiles/stone_h.png"), 8, 8, [64, 64, 64, 255]);

    let chain = pack_chain_in(&root, "maps_only");
    assert_eq!(chain.len(), 2, "ancestor + child");
    let atlas = build_atlas(&[], &chain, &[]);
    let stone = *builtin_slots().get("stone").unwrap();
    assert_eq!(
        tile_center(&atlas.color, atlas.px, stone),
        [255, 0, 255, 255],
        "parent's albedo survives - the child shipped no stone.png"
    );
    let tp = atlas.px / ATLAS_TILES;
    let i = (((stone as u32 / ATLAS_TILES * tp) * atlas.px + stone as u32 % ATLAS_TILES * tp) * 4)
        as usize;
    assert_eq!(atlas.material[i], 64, "child's authored height applied");
}

#[test]
fn pack_inherits_cycle_terminates() {
    use crate::atlas::pack_chain_in;
    let root = tmp_dir("packcycle");
    for (id, parent) in [("a", "b"), ("b", "a")] {
        std::fs::create_dir_all(root.join(id).join("tiles")).unwrap();
        std::fs::write(
            root.join(id).join("pack.toml"),
            format!("inherits = \"{parent}\"\n"),
        )
        .unwrap();
    }
    // Must not hang, and must not visit a pack twice.
    assert_eq!(pack_chain_in(&root, "a").len(), 2);
    // A pack naming itself is ignored outright.
    std::fs::create_dir_all(root.join("solo").join("tiles")).unwrap();
    std::fs::write(root.join("solo/pack.toml"), "inherits = \"solo\"\n").unwrap();
    assert_eq!(pack_chain_in(&root, "solo").len(), 1);
    // A parent that resolves to nothing degrades to the child alone.
    std::fs::create_dir_all(root.join("orphan").join("tiles")).unwrap();
    std::fs::write(root.join("orphan/pack.toml"), "inherits = \"nope\"\n").unwrap();
    assert_eq!(pack_chain_in(&root, "orphan").len(), 1);
}

#[test]
fn pack_can_author_the_interior_layer() {
    use crate::atlas::{ATLAS_TILES, build_atlas, builtin_slots};
    // An interior layer is a full second ALBEDO at a slot of its own, not a
    // greyscale mask: the shader finds it via `interior_base + (alpha - 1)`.
    let pack = tmp_dir("packinterior");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/leaves.png"), 8, 8, [40, 90, 30, 255]);
    write_solid_png(&pack.join("tiles/leaves_i.png"), 8, 8, [10, 20, 200, 255]);
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &[]);
    assert!(atlas.warnings.is_empty(), "{:?}", atlas.warnings);
    let leaves = *builtin_slots().get("leaves").unwrap();
    let tp = atlas.px / ATLAS_TILES;
    let at = |slot: u16| {
        (((slot as u32 / ATLAS_TILES * tp) * atlas.px + slot as u32 % ATLAS_TILES * tp) * 4)
            as usize
    };
    let id = atlas.material[at(leaves) + 3];
    assert_ne!(
        id, 0,
        "surface tile names its interior layer in material alpha"
    );
    let layer = atlas.interior_base + (id as u16 - 1);
    assert_eq!(
        tile_center(&atlas.color, atlas.px, layer),
        [10, 20, 200, 255],
        "interior layer keeps its own colour at its own slot"
    );
    assert_eq!(
        tile_center(&atlas.color, atlas.px, leaves),
        [40, 90, 30, 255],
        "surface albedo untouched"
    );
    assert_eq!(
        atlas.material[at(layer) + 3],
        0,
        "the layer itself has no layer"
    );
}

#[test]
fn luminance_height_fallback_keeps_an_authored_interior() {
    use crate::atlas::{ATLAS_TILES, build_atlas, builtin_slots};
    // stone/cobblestone get a free luminance height when none is authored. That
    // fallback writes R, and must not disturb the interior-layer id in A.
    let pack = tmp_dir("packinteriorstone");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/stone.png"), 8, 8, [120, 120, 120, 255]);
    write_solid_png(&pack.join("tiles/stone_i.png"), 8, 8, [180, 180, 180, 255]);
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &[]);
    let stone = *builtin_slots().get("stone").unwrap();
    let tp = atlas.px / ATLAS_TILES;
    let i = (((stone as u32 / ATLAS_TILES * tp) * atlas.px + stone as u32 % ATLAS_TILES * tp) * 4)
        as usize;
    assert_ne!(
        atlas.material[i + 3],
        0,
        "interior layer id survived the fallback"
    );
    assert!(
        atlas.material[i] < 255,
        "and the free luminance height still applied"
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

#[test]
fn pack_numbered_variants_get_their_own_slots() {
    use crate::atlas::{ATLAS_TILES, build_atlas, builtin_slots};
    let pack = tmp_dir("packvariants");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/gravel.png"), 8, 8, [10, 10, 10, 255]);
    write_solid_png(&pack.join("tiles/gravel_2.png"), 8, 8, [20, 20, 20, 255]);
    write_solid_png(&pack.join("tiles/gravel_3.png"), 8, 8, [30, 30, 30, 255]);
    // A variant may carry its own companion maps.
    write_solid_png(&pack.join("tiles/gravel_2_h.png"), 8, 8, [90, 90, 90, 255]);
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &[]);
    assert!(atlas.warnings.is_empty(), "{:?}", atlas.warnings);
    let gravel = *builtin_slots().get("gravel").unwrap();
    let sig = atlas.variants.signature();
    let alts = &sig.iter().find(|(k, _)| *k == gravel).unwrap().1;
    assert_eq!(alts.len(), 2, "base tile plus two alternates");
    // Each alternate is a distinct slot carrying its own art.
    assert_eq!(
        tile_center(&atlas.color, atlas.px, gravel),
        [10, 10, 10, 255]
    );
    let mut seen: Vec<u8> = alts
        .iter()
        .map(|s| tile_center(&atlas.color, atlas.px, *s)[0])
        .collect();
    seen.sort_unstable();
    assert_eq!(seen, vec![20, 30], "variants painted with their own albedo");
    // gravel_2's authored height landed on gravel_2's slot, not on the base.
    let tp = atlas.px / ATLAS_TILES;
    let at = |slot: u16| {
        let i = (((slot as u32 / ATLAS_TILES * tp) * atlas.px + slot as u32 % ATLAS_TILES * tp) * 4)
            as usize;
        atlas.material[i]
    };
    let v2 = alts
        .iter()
        .find(|s| tile_center(&atlas.color, atlas.px, **s)[0] == 20)
        .unwrap();
    assert_eq!(at(*v2), 90, "variant height on the variant slot");
    assert_eq!(at(gravel), 255, "base tile height untouched");
}

#[test]
fn variant_pick_is_per_face_stable_and_spread() {
    use crate::atlas::{build_atlas, builtin_slots};
    let pack = tmp_dir("packvarpick");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/stone.png"), 8, 8, [10, 10, 10, 255]);
    for (n, v) in [
        ("stone_2.png", 20u8),
        ("stone_3.png", 30),
        ("stone_4.png", 40),
    ] {
        write_solid_png(&pack.join("tiles").join(n), 8, 8, [v, v, v, 255]);
    }
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &[]);
    let v = &atlas.variants;
    let stone = *builtin_slots().get("stone").unwrap();

    // Stable: same block, same face, same answer. This is what makes a
    // position hash safe to use instead of stored per-block state.
    for _ in 0..4 {
        assert_eq!(v.pick(stone, 7, 3, -2, 1), v.pick(stone, 7, 3, -2, 1));
    }
    // Per-face: one cube should not be the same variant on all six sides.
    let faces: std::collections::HashSet<u16> =
        (0..6).map(|f| v.pick(stone, 7, 3, -2, f)).collect();
    assert!(faces.len() > 1, "all six faces picked the same variant");

    // Spread: over a volume, every variant gets used, and none dominates.
    let mut hist = std::collections::HashMap::new();
    for x in 0..16 {
        for y in 0..16 {
            for z in 0..16 {
                *hist.entry(v.pick(stone, x, y, z, 2)).or_insert(0usize) += 1;
            }
        }
    }
    assert_eq!(hist.len(), 4, "all four looks used: {hist:?}");
    let total: usize = hist.values().sum();
    for (slot, n) in &hist {
        let frac = *n as f64 / total as f64;
        assert!(
            (0.15..0.35).contains(&frac),
            "slot {slot} took {frac:.2} of faces, expected ~0.25"
        );
    }
    // A tile with no variants is the identity.
    let dirt = *builtin_slots().get("dirt").unwrap();
    assert_eq!(v.pick(dirt, 7, 3, -2, 1), dirt);
}

#[test]
fn variant_shipping_only_maps_inherits_the_base_look() {
    use crate::atlas::{build_atlas, builtin_slots};
    // `stone_2_n.png` with no `stone_2.png`: the variant still needs something
    // to draw, so it takes the base albedo rather than rendering blank.
    let pack = tmp_dir("packvarmaps");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/stone.png"), 8, 8, [77, 88, 99, 255]);
    write_solid_png(
        &pack.join("tiles/stone_2_n.png"),
        8,
        8,
        [128, 128, 255, 255],
    );
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &[]);
    let stone = *builtin_slots().get("stone").unwrap();
    let sig = atlas.variants.signature();
    let alt = sig.iter().find(|(k, _)| *k == stone).unwrap().1[0];
    assert_eq!(
        tile_center(&atlas.color, atlas.px, alt),
        [77, 88, 99, 255],
        "map-only variant inherited the base albedo"
    );
}

#[test]
fn layer_settings_come_from_pack_toml() {
    use crate::atlas::{build_atlas, builtin_slots};
    // What an interior layer means is the material's business. Opacity defaults
    // to the surface's own ALPHA — the general mechanism — and "luminance" is a
    // named special case for sheets like ice, not a rule compiled into the shader.
    let pack = tmp_dir("packlayers");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    for t in ["ice", "leaves"] {
        write_solid_png(
            &pack.join(format!("tiles/{t}.png")),
            8,
            8,
            [80, 90, 100, 255],
        );
        write_solid_png(
            &pack.join(format!("tiles/{t}_i.png")),
            8,
            8,
            [10, 20, 30, 255],
        );
    }
    std::fs::write(
        pack.join("pack.toml"),
        "[layers.ice]\n\
         depth = 0.13\n\
         opacity = \"luminance\"\n\
         opacity_min = 0.3\n\
         opacity_max = 0.75\n\
         dim = 0.82\n\
         cutoff = 0.0\n\
         [layers.leaves]\n\
         depth = 0.30\n\
         cutoff = 0.35\n",
    )
    .unwrap();
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &[]);
    assert!(atlas.warnings.is_empty(), "{:?}", atlas.warnings);

    let id_of = |name: &str| -> usize {
        let slot = *builtin_slots().get(name).unwrap();
        let tp = atlas.px / crate::atlas::ATLAS_TILES;
        let i = (((slot as u32 / crate::atlas::ATLAS_TILES * tp) * atlas.px
            + slot as u32 % crate::atlas::ATLAS_TILES * tp)
            * 4) as usize;
        atlas.material[i + 3] as usize
    };
    let ice = atlas.layer_params[id_of("ice") - 1];
    let leaves = atlas.layer_params[id_of("leaves") - 1];

    assert_eq!(ice.mode, 1, "ice asked for luminance");
    assert_eq!(
        leaves.mode, 0,
        "leaves said nothing, so the alpha default holds"
    );
    assert!((ice.depth - 0.13).abs() < 1e-6);
    assert!((leaves.depth - 0.30).abs() < 1e-6);
    assert!((ice.opacity_min - 0.3).abs() < 1e-6);
    assert!((leaves.cutoff - 0.35).abs() < 1e-6);
    // Unstated fields fall back rather than zeroing out.
    let d = crate::atlas::LayerParams::default();
    assert!(
        (leaves.dim - d.dim).abs() < 1e-6,
        "unset dim keeps the default"
    );
    assert!((leaves.opacity_max - d.opacity_max).abs() < 1e-6);
}

#[test]
fn a_tile_without_pack_toml_layer_settings_gets_defaults() {
    use crate::atlas::{LayerParams, build_atlas};
    let pack = tmp_dir("packlayerdefault");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/ice.png"), 8, 8, [80, 90, 100, 255]);
    write_solid_png(&pack.join("tiles/ice_i.png"), 8, 8, [10, 20, 30, 255]);
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &[]);
    assert_eq!(atlas.layer_params.len(), 1);
    assert_eq!(atlas.layer_params[0], LayerParams::default());
    assert_eq!(atlas.layer_params[0].mode, 0, "alpha is the default source");
}
