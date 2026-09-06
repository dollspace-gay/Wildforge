//! Save codecs scenarios.

use super::*;

#[test]
fn save_v2_roundtrip_with_palette() {
    let reg = base_reg();
    let mut w = test_world_with("v2save", reg.clone());
    let log = b(&reg, "base:log");
    w.set_block(1, 80, 1, log);
    w.set_block(-20, 33, 7, b(&reg, "base:sand"));
    save_world(&mut w);
    assert!(w.save_dir_for_test().join("palette").exists());

    let mut w2 = World::load_or_create(w.save_dir_for_test(), reg.clone()).unwrap();
    for x in -2..=2 {
        for z in -2..=2 {
            w2.ensure_chunk(tchunk(x, z));
        }
    }
    assert_eq!(w2.get_block(1, 80, 1), log);
    assert_eq!(w2.get_block(-20, 33, 7), b(&reg, "base:sand"));
    assert_eq!(w2.get_block(0, 0, 0), b(&reg, "base:bedrock"));
}

#[test]
fn pre_v3_saves_regenerate_cleanly() {
    let reg = base_reg();
    let dir = tmp_dir("oldsave");
    std::fs::write(dir.join("seed"), "42").unwrap();
    // A stale v2 chunk file must be ignored (regenerated), not crash.
    std::fs::write(dir.join("c.0.0.wfc"), b"WFC2garbagegarbage").unwrap();
    let error = World::load_or_create(dir.clone(), reg).err().unwrap();
    assert!(error.to_string().contains("legacy flat"));
    assert!(
        dir.join("c.0.0.wfc").exists(),
        "refusal leaves old data alone"
    );
    assert!(!dir.join("world.toml").exists());
}

#[test]
fn palette_less_v3_chunks_keep_their_legacy_numeric_ids() {
    let reg = base_reg();
    let dir = tmp_dir("palette-less-v3");
    std::fs::write(dir.join("seed"), "42").unwrap();
    let stone = b(&reg, "base:stone");
    let mut data = Vec::new();
    data.extend_from_slice(b"WFC3");
    let total = 16 * 16 * 256usize;
    let mut left = total;
    while left > 0 {
        let run = left.min(u16::MAX as usize) as u16;
        data.extend_from_slice(&run.to_le_bytes());
        data.extend_from_slice(&stone.0.to_le_bytes());
        left -= run as usize;
    }
    std::fs::write(dir.join("c.0.0.wfc"), data).unwrap();

    let error = World::load_or_create(dir, reg).err().unwrap();
    assert!(error.to_string().contains("legacy flat"));
}

#[test]
fn unknown_palette_entries_become_placeholder() {
    let reg = base_reg();
    let dir = tmp_dir("unknown");
    std::fs::write(dir.join("seed"), "42").unwrap();
    // Palette maps id 1 to a mod block that no longer exists.
    std::fs::write(dir.join("palette"), "0 base:air\n1 gonemod:ore\n").unwrap();
    let mut data = Vec::new();
    data.extend_from_slice(b"WFC3");
    let total = 16 * 16 * 256usize;
    for (count, id) in [(60u16, 0u16), (1, 1), ((total - 61) as u16, 0)] {
        data.extend_from_slice(&count.to_le_bytes());
        data.extend_from_slice(&id.to_le_bytes());
    }
    let _ = std::fs::write(dir.join("c.0.0.wfc"), data);
    let error = World::load_or_create(dir, reg).err().unwrap();
    assert!(error.to_string().contains("legacy flat"));
}

#[test]
fn all_placeholder_chunks_regenerate_instead_of_becoming_obelisks() {
    let reg = base_reg();
    let dir = tmp_dir("placeholder-obelisk");
    std::fs::write(dir.join("seed"), "42").unwrap();
    std::fs::write(
        dir.join("palette"),
        format!("{} base:unknown\n", reg.unknown_block.0),
    )
    .unwrap();
    let mut data = Vec::new();
    data.extend_from_slice(b"WFC3");
    let total = 16 * 16 * 256usize;
    let mut left = total;
    while left > 0 {
        let run = left.min(u16::MAX as usize) as u16;
        data.extend_from_slice(&run.to_le_bytes());
        data.extend_from_slice(&reg.unknown_block.0.to_le_bytes());
        left -= run as usize;
    }
    std::fs::write(dir.join("c.0.0.wfc"), data).unwrap();

    let error = World::load_or_create(dir, reg).err().unwrap();
    assert!(error.to_string().contains("legacy flat"));
}

#[test]
fn world_remaps_after_registry_change() {
    let root = tmp_dir("reloadmod");
    write_demo_mod(&root);
    let reg_with = Arc::new(registry::load(&root));
    let ore = reg_with.block_id("testium:ore").unwrap();
    let mut w = test_world_with("reload-w", reg_with.clone());
    w.set_block(2, 70, 2, ore);
    // "Remove" the mod: rebuild registry from an empty dir, remap the world.
    let reg_without = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    w.reg = reg_without.clone();
    w.remap_from(&reg_with);
    assert_eq!(
        w.get_block(2, 70, 2),
        reg_without.unknown_block,
        "mod block becomes placeholder after mod removal"
    );
    // Vanilla blocks survive the remap unchanged (by name).
    assert_eq!(w.get_block(0, 0, 0), b(&reg_without, "base:bedrock"));
}

#[test]
fn world_meta_roundtrip_and_legacy_refusal() {
    use crate::world::{
        WORLD_GENERATOR_VERSION, WORLD_TOPOLOGY, load_world_meta, read_world_meta, write_world_meta,
    };
    let dir = tmp_dir("meta");
    write_world_meta(&dir, 777, "creative", 0.0).unwrap();
    assert_eq!(
        read_world_meta(&dir),
        (Some(777), "creative".to_string(), 0.0)
    );
    let text = std::fs::read_to_string(dir.join("world.toml")).unwrap();
    assert!(text.contains(&format!("topology = \"{WORLD_TOPOLOGY}\"")));
    assert!(text.contains("face_blocks = 8192"));
    assert!(text.contains("world_height = 256"));
    assert!(text.contains("planet_radius = 5215."));
    assert!(text.contains(&format!("generator_version = {WORLD_GENERATOR_VERSION}")));
    // Camera mode round-trips through the same header.
    let mut meta = load_world_meta(&dir).unwrap().unwrap();
    assert_eq!(meta.camera, "first", "default camera is first-person");
    meta.camera = "orbit".into();
    crate::world::write_world_meta_full(
        &dir,
        meta.seed,
        &meta.mode,
        meta.ire,
        meta.day,
        &meta.camera,
    )
    .unwrap();
    assert_eq!(
        load_world_meta(&dir).unwrap().unwrap().camera,
        "orbit",
        "chosen camera mode is persisted per world"
    );
    let previous = text.replace(
        &format!("generator_version = {WORLD_GENERATOR_VERSION}"),
        "generator_version = 9",
    );
    std::fs::write(dir.join("world.toml"), previous).unwrap();
    assert_eq!(
        load_world_meta(&dir).unwrap().unwrap().seed,
        777,
        "the biome correction must not strand version-9 planets"
    );

    // A flat save is rejected without modifying it.
    let dir2 = tmp_dir("meta2");
    std::fs::write(dir2.join("seed"), "42").unwrap();
    let before = std::fs::read(dir2.join("seed")).unwrap();
    let error = load_world_meta(&dir2).unwrap_err();
    assert!(error.to_string().contains("legacy flat"));
    let reg = base_reg();
    assert!(World::load_or_create(dir2.clone(), reg).is_err());
    assert!(!dir2.join("world.toml").exists());
    assert_eq!(std::fs::read(dir2.join("seed")).unwrap(), before);

    // An old world.toml is likewise not inferred to be planetary.
    let dir3 = tmp_dir("meta3");
    std::fs::write(dir3.join("world.toml"), "seed = 9\nmode = \"survival\"\n").unwrap();
    assert!(
        load_world_meta(&dir3)
            .unwrap_err()
            .to_string()
            .contains("missing topology")
    );
}

#[test]
fn old_worlds_keep_their_partial_sand_as_ordinary_sand() {
    // Partial (sub-voxel) sand is gone. A world saved when it existed
    // must not come back full of unknown blocks — the alias converts
    // it to ordinary sand, keeping the volume the player had.
    let reg = base_reg();
    let sand = reg.block_id("base:sand").expect("sand exists");
    assert_eq!(
        reg.block_id("base:surface_sand"),
        Some(sand),
        "an old save's surface sand resolves to plain sand"
    );
    // And nothing claims the octant geometry any more.
    assert!(
        reg.blocks.iter().all(|b| b.name != "base:surface_sand"),
        "the block itself is gone from the registry"
    );
    // The metadata byte survives it: soil still carries fertility.
    let mut w = test_world_with("post-sand-meta", reg.clone());
    let farm = b(&reg, "base:farmland");
    let h = w.surface_height(4, 4);
    w.set_block_meta(4, h, 4, farm, crate::world::soil::soil_meta(31, 2));
    assert_eq!(w.fertility_at(4, h, 4), 31, "the meta plane still works");
}

/// The palette describes the registry, not the world, so writing it on
/// the autosave timer was 4 KB of churn every twenty seconds saying the
/// same thing. It still has to be written when it would differ — a
/// fresh world, or one whose block registry changed since last time.
#[test]
fn the_palette_is_written_when_it_would_differ_and_not_on_a_timer() {
    let reg = base_reg();
    let dir = tmp_dir("palette-churn");
    let palette = dir.join("palette");
    let mut w = World::new(9, dir.clone(), reg.clone());
    w.ensure_chunk(tchunk(0, 0));
    save_world(&mut w);
    assert!(palette.exists(), "a fresh world owes a palette");

    std::fs::remove_file(&palette).unwrap();
    save_world(&mut w);
    assert!(!palette.exists(), "an unchanged palette is not rewritten");

    // Reopening against a save whose palette is missing or stale counts
    // as a difference, so the next save puts one back — and every chunk
    // that loads is rewritten in the ids it names.
    let mut w = World::load_or_create(dir.clone(), reg.clone()).unwrap();
    w.ensure_chunk(tchunk(0, 0));
    save_world(&mut w);
    assert!(palette.exists(), "a stale palette is replaced");
}
