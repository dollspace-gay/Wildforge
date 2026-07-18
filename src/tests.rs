//! Headless tests: engine mechanics + all four mod-system phases.

use std::path::Path;
use std::sync::Arc;

use glam::Vec3;

use crate::chunk::{CHUNK_Y, ChunkPos};
use crate::inventory::{Inventory, ItemStack, click_stack};
use crate::physics::{Input, Player};
use crate::raycast::raycast;
use crate::registry::{self, AIR, Registry};
use crate::world::World;

fn base_reg() -> Arc<Registry> {
    Arc::new(registry::load(Path::new("/nonexistent-mods-dir")))
}

fn tmp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("wildforge-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn test_world_with(name: &str, reg: Arc<Registry>) -> World {
    let mut w = World::new(42, tmp_dir(name), reg);
    for x in -2..=2 {
        for z in -2..=2 {
            w.ensure_chunk(ChunkPos { x, z });
        }
    }
    w
}

fn test_world(name: &str) -> World {
    test_world_with(name, base_reg())
}

fn b(reg: &Registry, name: &str) -> crate::registry::BlockId {
    reg.block_id(name).unwrap_or_else(|| panic!("missing block {name}"))
}

fn it(reg: &Registry, name: &str) -> crate::registry::ItemId {
    reg.item_id(name).unwrap_or_else(|| panic!("missing item {name}"))
}

// ---------------- phase 1: registry & saves ----------------

#[test]
fn base_registry_has_vanilla_content() {
    let reg = base_reg();
    assert_eq!(reg.block(AIR).name, "base:air");
    for name in ["base:grass", "base:stone", "base:water", "base:crafting_table"] {
        assert!(reg.block_id(name).is_some(), "missing {name}");
    }
    for name in ["base:stick", "base:wood_pickaxe", "base:planks"] {
        assert!(reg.item_id(name).is_some(), "missing {name}");
    }
    // Water family auto-registered with rising levels.
    assert_eq!(reg.water_level(reg.water_block(0)), Some(0));
    assert_eq!(reg.water_level(reg.water_block(5)), Some(5));
    // Block items place their blocks.
    let planks = it(&reg, "base:planks");
    assert_eq!(reg.item(planks).places, reg.block_id("base:planks"));
    assert!(!reg.recipes.is_empty());
}

#[test]
fn registry_tool_rules() {
    let reg = base_reg();
    let stone = b(&reg, "base:stone");
    let pick = it(&reg, "base:wood_pickaxe");
    let spick = it(&reg, "base:stone_pickaxe");
    let axe = it(&reg, "base:wood_axe");
    let bare = reg.effective_hardness(stone, None).unwrap();
    let wood = reg.effective_hardness(stone, Some(pick)).unwrap();
    let sp = reg.effective_hardness(stone, Some(spick)).unwrap();
    assert!(wood < bare && sp < wood);
    assert_eq!(reg.effective_hardness(stone, Some(axe)).unwrap(), bare);
    // Stone drops nothing without a pickaxe; grass drops dirt.
    assert_eq!(reg.drops_for(stone, None), None);
    assert_eq!(
        reg.drops_for(stone, Some(pick)),
        Some((it(&reg, "base:cobblestone"), 1))
    );
    let grass = b(&reg, "base:grass");
    assert_eq!(reg.drops_for(grass, None), Some((it(&reg, "base:dirt"), 1)));
    // Bedrock unbreakable, leaves drop nothing.
    assert!(reg.block(b(&reg, "base:bedrock")).hardness.is_none());
    assert_eq!(reg.block(b(&reg, "base:leaves")).drops, None);
}

#[test]
fn save_v2_roundtrip_with_palette() {
    let reg = base_reg();
    let mut w = test_world_with("v2save", reg.clone());
    let log = b(&reg, "base:log");
    w.set_block(1, 80, 1, log);
    w.set_block(-20, 33, 7, b(&reg, "base:sand"));
    w.save_modified();
    assert!(w.save_dir_for_test().join("palette").exists());

    let mut w2 = World::load_or_create(w.save_dir_for_test(), reg.clone());
    for x in -2..=2 {
        for z in -2..=2 {
            w2.ensure_chunk(ChunkPos { x, z });
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
    let mut w = World::load_or_create(dir, reg.clone());
    w.ensure_chunk(ChunkPos { x: 0, z: 0 });
    assert_eq!(w.get_block(0, 0, 0), b(&reg, "base:bedrock"));
    w.save_modified();
    // ensure_chunk on fresh terrain marks modified=false, so force a write.
    w.set_block(1, 100, 1, b(&reg, "base:planks"));
    w.save_modified();
    let bytes = std::fs::read(w.save_dir_for_test().join("c.0.0.wfc")).unwrap();
    assert!(bytes.starts_with(b"WFC3"), "saves are written as v3 now");
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
    let mut left = total;
    while left > 0 {
        let run = left.min(u16::MAX as usize) as u16;
        data.extend_from_slice(&run.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        left -= run as usize;
    }
    let _ = std::fs::write(dir.join("c.0.0.wfc"), data);
    let mut w = World::load_or_create(dir, reg.clone());
    w.ensure_chunk(ChunkPos { x: 0, z: 0 });
    assert_eq!(
        w.get_block(4, 60, 4),
        reg.unknown_block,
        "missing mod blocks must become the placeholder, not corrupt"
    );
}

#[test]
fn generation_is_deterministic() {
    let mut a = test_world("det-a");
    let mut b2 = test_world("det-b");
    a.ensure_chunk(ChunkPos { x: 5, z: -3 });
    b2.ensure_chunk(ChunkPos { x: 5, z: -3 });
    assert_eq!(
        a.chunks[&ChunkPos { x: 5, z: -3 }].raw(),
        b2.chunks[&ChunkPos { x: 5, z: -3 }].raw()
    );
}

#[test]
fn terrain_has_bedrock_and_surface() {
    let reg = base_reg();
    let w = test_world_with("terrain", reg.clone());
    assert_eq!(w.get_block(0, 0, 0), b(&reg, "base:bedrock"));
    let h = w.surface_height(0, 0);
    assert!(h > 4 && h < CHUNK_Y as i32 - 1);
    assert!(reg.is_solid(w.get_block(0, h, 0)));
    assert!(!reg.is_solid(w.get_block(0, h + 2, 0)));
}

#[test]
fn set_block_roundtrip_and_cross_chunk_access() {
    let reg = base_reg();
    let mut w = test_world_with("set", reg.clone());
    let planks = b(&reg, "base:planks");
    w.set_block(3, 70, 3, planks);
    assert_eq!(w.get_block(3, 70, 3), planks);
    w.set_block(-1, 70, -1, b(&reg, "base:cobblestone"));
    assert_eq!(w.get_block(-1, 70, -1), b(&reg, "base:cobblestone"));
}

// ---------------- phase 2: data mods ----------------

fn write_demo_mod(root: &Path) {
    let dir = root.join("testium");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"testium\"\nname = \"Testium\"\nversion = \"1.0.0\"\ndepends = [\"base\"]\n").unwrap();
    std::fs::write(
        dir.join("blocks.toml"),
        r#"
[[block]]
id = "ore"
name = "Testium Ore"
texture = "@stone"
hardness = 5.0
tool = "pickaxe"
requires_tool = true
drops = "testium:shard"
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("items.toml"),
        r#"
[[item]]
id = "shard"
name = "Testium Shard"
texture = "@stick"
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("recipes.toml"),
        r#"
[[recipe]]
pattern = ["ss", "ss"]
keys = { s = "testium:shard" }
output = "testium:ore"
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("features.toml"),
        r#"
[[feature]]
type = "ore"
block = "testium:ore"
replaces = "base:stone"
vein_size = 8
per_chunk = 24
y_range = [4, 60]
"#,
    )
    .unwrap();
}

#[test]
fn data_mod_loads_blocks_items_recipes_features() {
    let root = tmp_dir("datamod");
    write_demo_mod(&root);
    let reg = registry::load(&root);
    let ore = reg.block_id("testium:ore").expect("mod block registered");
    let shard = reg.item_id("testium:shard").expect("mod item registered");
    assert_eq!(reg.block(ore).label, "Testium Ore");
    assert_eq!(reg.drops_for(ore, reg.item_id("base:wood_pickaxe")), Some((shard, 1)));
    assert_eq!(reg.drops_for(ore, None), None, "requires_tool");
    let t_ore = reg.block_id("testium:ore").unwrap();
    assert!(reg.ores.iter().any(|o| o.block == t_ore), "mod ore feature registered");
    assert!(reg.recipes.iter().any(|r| r.output == reg.item_id("testium:ore").unwrap()));
    let m = reg.mods.iter().find(|m| m.id == "testium").unwrap();
    assert!(m.error.is_none(), "{:?}", m.error);
}

#[test]
fn mod_ore_generates_in_terrain() {
    let root = tmp_dir("oremod");
    write_demo_mod(&root);
    let reg = Arc::new(registry::load(&root));
    let ore = reg.block_id("testium:ore").unwrap();
    let mut w = World::new(42, tmp_dir("oreworld"), reg.clone());
    let mut found = 0;
    for cx in -2..=2 {
        for cz in -2..=2 {
            w.ensure_chunk(ChunkPos { x: cx, z: cz });
        }
    }
    for x in -32..32 {
        for z in -32..32 {
            for y in 4..60 {
                if w.get_block(x, y, z) == ore {
                    found += 1;
                }
            }
        }
    }
    assert!(found > 0, "ore feature should generate veins");
}

#[test]
fn broken_mod_is_skipped_with_error() {
    let root = tmp_dir("brokenmod");
    let dir = root.join("bad");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"bad\"\n").unwrap();
    std::fs::write(dir.join("blocks.toml"), "this is not [ valid toml").unwrap();
    let reg = registry::load(&root);
    // Base still loads fine; bad mod recorded with error.
    assert!(reg.block_id("base:stone").is_some());
    let bad = reg.mods.iter().find(|m| m.id == "bad").expect("bad mod listed");
    assert!(bad.error.is_some());
}

// ---------------- phase 3: scripts ----------------

fn write_script_mod(root: &Path, script: &str) -> Vec<(String, std::path::PathBuf)> {
    let dir = root.join("scripty");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"scripty\"\n").unwrap();
    std::fs::write(dir.join("main.rhai"), script).unwrap();
    vec![("scripty".to_string(), dir)]
}

#[test]
fn script_events_cancel_and_queue_commands() {
    let root = tmp_dir("scriptmod");
    let mods = write_script_mod(
        &root,
        r#"
fn on_block_break(x, y, z, block) {
    if block == "base:bedrock" { return false; }
    storage_set("count", (storage_get("count").len() + 1).to_string());
    give("base:stick", 2);
    hud_message("broke " + block);
    true
}
"#,
    );
    let mut host = crate::script::ScriptHost::new();
    host.load_mods(&mods);
    assert!(host.wants("on_block_break"));
    assert!(!host.wants("on_tick"));

    let w = test_world("script-w");
    // Bedrock break is cancelled.
    let allow = host.dispatch(&w, "on_block_break", (0i64, 0i64, 0i64, "base:bedrock".to_string()));
    assert!(!allow, "script should cancel bedrock break");
    // Normal break allowed + commands queued.
    let allow = host.dispatch(&w, "on_block_break", (1i64, 70i64, 1i64, "base:dirt".to_string()));
    assert!(allow);
    let cmds = host.take_cmds();
    assert!(cmds.iter().any(|c| matches!(c, crate::script::Cmd::Give(n, 2) if n == "base:stick")));
    assert!(cmds.iter().any(|c| matches!(c, crate::script::Cmd::Hud(_))));
    // KV survived across dispatches.
    assert!(!host.kv.borrow().get("scripty").unwrap().get("count").unwrap().is_empty());
}

#[test]
fn script_reads_world_state() {
    let root = tmp_dir("scriptread");
    let mods = write_script_mod(
        &root,
        r#"
fn on_tick(dt) {
    let below = get_block(0, 0, 0);
    if below == "base:bedrock" { hud_message("bedrock confirmed"); }
}
"#,
    );
    let mut host = crate::script::ScriptHost::new();
    host.load_mods(&mods);
    let w = test_world("scriptread-w");
    host.dispatch(&w, "on_tick", (0.1f64,));
    let cmds = host.take_cmds();
    assert!(
        cmds.iter().any(|c| matches!(c, crate::script::Cmd::Hud(m) if m == "bedrock confirmed")),
        "script get_block should see the world"
    );
}

#[test]
fn script_kv_persists_to_disk() {
    let host = crate::script::ScriptHost::new();
    host.kv
        .borrow_mut()
        .entry("m".into())
        .or_default()
        .insert("k".into(), "v".into());
    let dir = tmp_dir("kv");
    host.save_kv(&dir);
    let host2 = crate::script::ScriptHost::new();
    host2.load_kv(&dir);
    assert_eq!(host2.kv.borrow()["m"]["k"], "v");
}

#[test]
fn script_error_keeps_previous_ast() {
    let root = tmp_dir("scripterr");
    let mods = write_script_mod(&root, "fn on_tick(dt) { hud_message(\"v1\"); }");
    let mut host = crate::script::ScriptHost::new();
    host.load_mods(&mods);
    assert!(host.wants("on_tick"));
    // Break the script on disk; reload keeps the old compiled version.
    std::fs::write(root.join("scripty/main.rhai"), "fn on_tick(dt) { this is broken").unwrap();
    host.load_mods(&mods);
    assert!(host.wants("on_tick"), "old AST must survive a bad edit");
    assert!(host.mods[0].error.is_some(), "and the error must be reported");
}

// ---------------- phase 4: hot reload remap ----------------

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

// ---------------- biomes ----------------

use crate::worldgen::{Biome, Generator};

/// Find a column of the given biome near the origin (deterministic per seed).
fn find_biome(g: &Generator, want: Biome) -> Option<(i32, i32)> {
    for r in 0..200 {
        let d = r * 24;
        for (x, z) in [(d, 0), (-d, 0), (0, d), (0, -d), (d, d), (-d, -d), (d, -d), (-d, d)] {
            if g.biome(x, z) == want {
                return Some((x, z));
            }
        }
    }
    None
}

#[test]
fn all_seven_biomes_exist_and_are_deterministic() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let g2 = Generator::new(42, &reg);
    for biome in [
        Biome::Forest,
        Biome::Plains,
        Biome::Desert,
        Biome::Jungle,
        Biome::Scrubland,
        Biome::Taiga,
        Biome::Arctic,
    ] {
        let (x, z) = find_biome(&g, biome)
            .unwrap_or_else(|| panic!("{biome:?} not found within search radius"));
        assert_eq!(g.biome(x, z), g2.biome(x, z), "same seed, same biome");
    }
    // Different seeds shuffle the layout.
    let g3 = Generator::new(1337, &reg);
    let mut diff = 0;
    for i in 0..40 {
        let (x, z) = (i * 173, i * -211);
        if g.biome(x, z) != g3.biome(x, z) {
            diff += 1;
        }
    }
    assert!(diff > 5, "different seeds should give different biome maps ({diff})");
}

/// Generate the chunk containing a column and return (world, surface y).
fn gen_at(reg: &Arc<Registry>, name: &str, x: i32, z: i32) -> (World, i32) {
    let mut w = World::new(42, tmp_dir(name), reg.clone());
    let cp = ChunkPos::of_world(x, z);
    for dx in -1..=1 {
        for dz in -1..=1 {
            w.ensure_chunk(ChunkPos { x: cp.x + dx, z: cp.z + dz });
        }
    }
    let h = w.surface_height(x, z);
    (w, h)
}

#[test]
fn desert_has_sand_surface_and_cacti() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let (x, z) = find_biome(&g, Biome::Desert).unwrap();
    // Find a dry desert column (above sea level).
    let mut spot = None;
    'scan: for dx in 0..64 {
        for dz in 0..64 {
            let (cx, cz) = (x + dx, z + dz);
            if g.biome(cx, cz) == Biome::Desert && g.surface_estimate(cx, cz) > crate::chunk::SEA_LEVEL + 1 {
                spot = Some((cx, cz));
                break 'scan;
            }
        }
    }
    let (x, z) = spot.expect("dry desert column");
    let (w, h) = gen_at(&reg, "desert", x, z);
    assert_eq!(w.get_block(x, h, z), b(&reg, "base:sand"), "desert surface is sand");
    assert_eq!(w.get_block(x, h - 2, z), b(&reg, "base:sand"), "desert subsoil is sand");
    // Cacti generate somewhere in desert chunks (deterministic for seed 42).
    let cactus = b(&reg, "base:cactus");
    let cp = ChunkPos::of_world(x, z);
    let mut w2 = World::new(42, tmp_dir("cacti"), reg.clone());
    let mut found = false;
    'chunks: for dx in -4..=4 {
        for dz in -4..=4 {
            let p = ChunkPos { x: cp.x + dx, z: cp.z + dz };
            w2.ensure_chunk(p);
            let bx = p.x * 16;
            let bz = p.z * 16;
            for lx in 0..16 {
                for lz in 0..16 {
                    for y in 60..90 {
                        if w2.get_block(bx + lx, y, bz + lz) == cactus {
                            found = true;
                            break 'chunks;
                        }
                    }
                }
            }
        }
    }
    assert!(found, "cacti should generate in deserts");
}

#[test]
fn arctic_has_snow_and_frozen_ocean() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let (x, z) = find_biome(&g, Biome::Arctic).unwrap();
    let mut land = None;
    let mut ocean = None;
    for dx in 0..96 {
        for dz in 0..96 {
            let (cx, cz) = (x + dx, z + dz);
            if g.biome(cx, cz) != Biome::Arctic {
                continue;
            }
            let h = g.surface_estimate(cx, cz);
            if h > crate::chunk::SEA_LEVEL + 1 && land.is_none() {
                land = Some((cx, cz));
            }
            if h < crate::chunk::SEA_LEVEL - 2 && ocean.is_none() {
                ocean = Some((cx, cz));
            }
        }
    }
    if let Some((cx, cz)) = land {
        let (w, h) = gen_at(&reg, "arctic-land", cx, cz);
        assert_eq!(w.get_block(cx, h, cz), b(&reg, "base:snow"), "arctic surface is snow");
    }
    if let Some((cx, cz)) = ocean {
        let (w, _) = gen_at(&reg, "arctic-sea", cx, cz);
        assert_eq!(
            w.get_block(cx, crate::chunk::SEA_LEVEL, cz),
            b(&reg, "base:ice"),
            "arctic ocean surface is ice"
        );
        assert!(
            reg.is_water(w.get_block(cx, crate::chunk::SEA_LEVEL - 1, cz)),
            "water under the ice"
        );
    }
    assert!(land.is_some() || ocean.is_some(), "found neither arctic land nor ocean");
}

#[test]
fn jungle_denser_than_plains() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let count_logs = |name: &str, biome: Biome, log_name: &str| -> (u32, u32) {
        let (x, z) = find_biome(&g, biome).unwrap();
        let cp = ChunkPos::of_world(x, z);
        let mut w = World::new(42, tmp_dir(name), reg.clone());
        let log = b(&reg, log_name);
        let mut logs = 0;
        let mut cols = 0;
        for dx in -3..=3 {
            for dz in -3..=3 {
                let p = ChunkPos { x: cp.x + dx, z: cp.z + dz };
                w.ensure_chunk(p);
                for lx in 0..16 {
                    for lz in 0..16 {
                        let (wx, wz) = (p.x * 16 + lx, p.z * 16 + lz);
                        if w.generator.biome(wx, wz) == biome {
                            cols += 1;
                            for y in 60..100 {
                                if w.get_block(wx, y, wz) == log {
                                    logs += 1;
                                    break; // one per column
                                }
                            }
                        }
                    }
                }
            }
        }
        (logs, cols.max(1))
    };
    let (jl, jc) = count_logs("jungle", Biome::Jungle, "base:jungle_log");
    let (pl, pc) = count_logs("plains", Biome::Plains, "base:log");
    let jd = jl as f32 / jc as f32;
    let pd = pl as f32 / pc as f32;
    assert!(
        jd > pd * 3.0,
        "jungle tree density ({jd:.4}) should dwarf plains ({pd:.4})"
    );
}

// ---------------- terrain v2 ----------------

#[test]
fn spline_eval_clamps_and_interpolates() {
    use crate::worldgen::Spline;
    let s = Spline::new(&[(-1.0, 10.0), (0.0, 20.0), (1.0, 100.0)]);
    assert_eq!(s.at(-2.0), 10.0);
    assert_eq!(s.at(2.0), 100.0);
    assert_eq!(s.at(-1.0), 10.0);
    assert!((s.at(-0.5) - 15.0).abs() < 1e-4);
    assert!((s.at(0.5) - 60.0).abs() < 1e-4);
}

#[test]
fn terrain_has_overhangs() {
    // 3D density terrain must produce air-under-solid somewhere.
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("overhang"), reg.clone());
    let mut found = false;
    'outer: for cx in -6..6 {
        for cz in -6..6 {
            w.ensure_chunk(ChunkPos { x: cx, z: cz });
            for lx in 0..16 {
                for lz in 0..16 {
                    let (x, z) = (cx * 16 + lx, cz * 16 + lz);
                    // solid above air above solid, all above sea level
                    for y in 70..200 {
                        if w.get_block(x, y, z) == AIR
                            && reg.is_solid(w.get_block(x, y + 1, z))
                            && (66..y).any(|yy| reg.is_solid(w.get_block(x, yy, z)))
                        {
                            found = true;
                            break 'outer;
                        }
                    }
                }
            }
        }
    }
    assert!(found, "3D terrain should create overhangs");
}

#[test]
fn mountains_rise_above_plains() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let sample_max = |biome: Biome| -> i32 {
        let mut best = 0;
        let mut n = 0;
        for r in 0..400 {
            let d = r * 16;
            for (x, z) in [(d, 0), (-d, 0), (0, d), (0, -d), (d, d), (-d, -d)] {
                if g.biome(x, z) == biome {
                    best = best.max(g.surface_estimate(x, z));
                    n += 1;
                    if n > 200 {
                        return best;
                    }
                }
            }
        }
        best
    };
    let m = sample_max(Biome::Mountains);
    let p = sample_max(Biome::Plains);
    assert!(m > 130, "mountains should reach high ({m})");
    assert!(m > p + 30, "mountains ({m}) far above plains ({p})");
}

#[test]
fn oceans_exist_and_fill_with_water() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    // Find a deep-ocean column via continentalness.
    let mut spot = None;
    'outer: for r in 0..300 {
        let d = r * 24;
        for (x, z) in [(d, 0), (-d, 0), (0, d), (0, -d), (d, d), (-d, -d)] {
            if g.surface_estimate(x, z) < 55 {
                spot = Some((x, z));
                break 'outer;
            }
        }
    }
    let (x, z) = spot.expect("an ocean should exist");
    let mut w = World::new(42, tmp_dir("ocean"), reg.clone());
    w.ensure_chunk(ChunkPos::of_world(x, z));
    assert!(reg.is_water(w.get_block(x, 63, z)) || w.get_block(x, 63, z) == b(&reg, "base:ice"));
    let floor = w.surface_height(x, z);
    assert!(floor < 62, "ocean floor below sea level ({floor})");
    let fb = w.get_block(x, floor, z);
    assert!(
        fb == b(&reg, "base:sand") || fb == b(&reg, "base:gravel"),
        "ocean floor surfaced with sand/gravel, got {}",
        reg.block(fb).name
    );
}

#[test]
fn caves_exist_underground() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("caves2"), reg.clone());
    let mut pockets = 0;
    for cx in -3..3 {
        for cz in -3..3 {
            w.ensure_chunk(ChunkPos { x: cx, z: cz });
            for lx in 0..16 {
                for lz in 0..16 {
                    let (x, z) = (cx * 16 + lx, cz * 16 + lz);
                    let top = w.surface_height(x, z);
                    for y in 6..(top - 10).min(50) {
                        if w.get_block(x, y, z) == AIR {
                            pockets += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(pockets > 200, "underground cave air should be plentiful ({pockets})");
}

#[test]
fn steep_faces_and_peaks_surface_correctly() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    // Find a mountain area and generate it.
    let (mx, mz) = {
        let mut best = (0, 0);
        let mut best_h = 0;
        for r in 0..300 {
            let d = r * 16;
            for (x, z) in [(d, 0), (-d, 0), (0, d), (0, -d), (d, d), (-d, -d)] {
                let h = g.surface_estimate(x, z);
                if h > best_h {
                    best_h = h;
                    best = (x, z);
                }
            }
            if best_h > 165 {
                break;
            }
        }
        best
    };
    let mut w = World::new(42, tmp_dir("peaks"), reg.clone());
    let cp = ChunkPos::of_world(mx, mz);
    for dx in -1..=1 {
        for dz in -1..=1 {
            w.ensure_chunk(ChunkPos { x: cp.x + dx, z: cp.z + dz });
        }
    }
    let snow = b(&reg, "base:snow");
    let stone = b(&reg, "base:stone");
    let (mut snowy, mut stony, mut grassy_high) = (0, 0, 0);
    for lx in 0..16 {
        for lz in 0..16 {
            let (x, z) = (cp.x * 16 + lx, cp.z * 16 + lz);
            let top = w.surface_height(x, z);
            let tb = w.get_block(x, top, z);
            if top >= 170 && tb == snow {
                snowy += 1;
            }
            if tb == stone {
                stony += 1;
            }
            if top >= 170 && tb == b(&reg, "base:grass") {
                grassy_high += 1;
            }
        }
    }
    assert_eq!(grassy_high, 0, "no grass on extreme peaks");
    assert!(snowy + stony > 0, "mountain tops are stone/snow");
}

#[test]
fn wood_families_registered_and_craftable() {
    let reg = base_reg();
    for w in ["birch", "spruce", "jungle", "acacia"] {
        assert!(reg.block_id(&format!("base:{w}_log")).is_some(), "{w} log");
        assert!(reg.block_id(&format!("base:{w}_leaves")).is_some(), "{w} leaves");
        assert!(reg.block_id(&format!("base:{w}_planks")).is_some(), "{w} planks");
        // Each log crafts into ITS OWN planks.
        let log = it(&reg, &format!("base:{w}_log"));
        let mut g = vec![None; 4];
        g[0] = Some(ItemStack::new(&reg, log, 1));
        let r = crate::crafting::match_recipe(&reg, &g, 2)
            .unwrap_or_else(|| panic!("{w} log -> planks recipe"));
        assert_eq!(r.output, it(&reg, &format!("base:{w}_planks")), "{w} planks output");
        assert_eq!(r.count, 4);
    }
    // Leaves are leaf-like: non-opaque, dropless, breakable.
    let bl = b(&reg, "base:spruce_leaves");
    assert!(!reg.is_opaque(bl));
    assert_eq!(reg.block(bl).drops, None);
}

#[test]
fn any_plank_type_is_interchangeable_in_recipes() {
    let reg = base_reg();
    let sticks = it(&reg, "base:stick");
    let table = it(&reg, "base:crafting_table");
    let grid = |size: usize, cells: &[(usize, crate::registry::ItemId)]| {
        let mut g = vec![None; size * size];
        for &(i, item) in cells {
            g[i] = Some(ItemStack::new(&reg, item, 1));
        }
        g
    };
    // Sticks from every plank type.
    for w in ["planks", "birch_planks", "spruce_planks", "jungle_planks", "acacia_planks"] {
        let p = it(&reg, &format!("base:{w}"));
        let g = grid(2, &[(0, p), (2, p)]);
        let r = crate::crafting::match_recipe(&reg, &g, 2)
            .unwrap_or_else(|| panic!("sticks from {w}"));
        assert_eq!(r.output, sticks);
    }
    // A crafting table from MIXED plank types.
    let g = grid(
        2,
        &[
            (0, it(&reg, "base:planks")),
            (1, it(&reg, "base:spruce_planks")),
            (2, it(&reg, "base:jungle_planks")),
            (3, it(&reg, "base:birch_planks")),
        ],
    );
    assert_eq!(
        crate::crafting::match_recipe(&reg, &g, 2).expect("mixed-plank table").output,
        table
    );
    // Tools too: pickaxe head from acacia planks.
    let a = it(&reg, "base:acacia_planks");
    let s = it(&reg, "base:stick");
    let g = grid(3, &[(0, a), (1, a), (2, a), (4, s), (7, s)]);
    assert_eq!(
        crate::crafting::match_recipe(&reg, &g, 3).unwrap().output,
        it(&reg, "base:wood_pickaxe")
    );
}

#[test]
fn mods_can_extend_ingredient_tags() {
    let root = tmp_dir("tagmod");
    let dir = root.join("cherry");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"cherry\"\ndepends = [\"base\"]\n").unwrap();
    std::fs::write(
        dir.join("blocks.toml"),
        "[[block]]\nid = \"planks\"\nname = \"Cherry Planks\"\ntexture = \"@planks\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("tags.toml"),
        "[[tag]]\nid = \"base:planks\"\nitems = [\"cherry:planks\"]\n",
    )
    .unwrap();
    let reg = registry::load(&root);
    let cherry = reg.item_id("cherry:planks").expect("cherry planks item");
    assert!(
        reg.tags["base:planks"].contains(&cherry),
        "mod planks join the shared tag"
    );
    // And they immediately work in base recipes: sticks from cherry planks.
    let mut g = vec![None; 4];
    g[0] = Some(ItemStack::new(&reg, cherry, 1));
    g[2] = Some(ItemStack::new(&reg, cherry, 1));
    let r = crate::crafting::match_recipe(&reg, &g, 2).expect("sticks from cherry planks");
    assert_eq!(r.output, reg.item_id("base:stick").unwrap());
}

#[test]
fn biomes_grow_their_own_wood() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let count_wood = |name: &str, biome: Biome, log: &str| -> (u32, u32) {
        let (x, z) = find_biome(&g, biome).unwrap();
        let cp = ChunkPos::of_world(x, z);
        let mut w = World::new(42, tmp_dir(name), reg.clone());
        let want = b(&reg, log);
        let oak = b(&reg, "base:log");
        let (mut hits, mut oaks) = (0, 0);
        for dx in -4..=4 {
            for dz in -4..=4 {
                let p = ChunkPos { x: cp.x + dx, z: cp.z + dz };
                w.ensure_chunk(p);
                for lx in 0..16 {
                    for lz in 0..16 {
                        let (wx, wz) = (p.x * 16 + lx, p.z * 16 + lz);
                        if w.generator.biome(wx, wz) != biome {
                            continue;
                        }
                        for y in 64..200 {
                            let blk = w.get_block(wx, y, wz);
                            if blk == want {
                                hits += 1;
                            } else if blk == oak {
                                oaks += 1;
                            }
                        }
                    }
                }
            }
        }
        (hits, oaks)
    };
    let (spruce, _) = count_wood("wood-taiga", Biome::Taiga, "base:spruce_log");
    assert!(spruce > 0, "taiga should grow spruce");
    let (jungle, _) = count_wood("wood-jungle", Biome::Jungle, "base:jungle_log");
    assert!(jungle > 0, "jungle should grow jungle wood");
    let (acacia, _) = count_wood("wood-scrub", Biome::Scrubland, "base:acacia_log");
    assert!(acacia > 0, "scrubland shrubs should be acacia");
    // Forests mix oak and birch.
    let (birch, oaks) = count_wood("wood-forest", Biome::Forest, "base:birch_log");
    assert!(birch > 0 && oaks > 0, "forest should mix birch ({birch}) and oak ({oaks})");
}

// ---------------- bronze age ----------------

#[test]
fn tool_tiers_gate_drops() {
    let root = tmp_dir("tiermod");
    let dir = root.join("t");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"t\"\n").unwrap();
    std::fs::write(
        dir.join("blocks.toml"),
        "[[block]]\nid = \"hard\"\ntexture = \"@stone\"\ntool = \"pickaxe\"\nrequires_tool = true\nmin_tier = 2\n",
    )
    .unwrap();
    let reg = registry::load(&root);
    let hard = reg.block_id("t:hard").unwrap();
    let wood = reg.item_id("base:wood_pickaxe");
    let stone = reg.item_id("base:stone_pickaxe");
    let bronze = reg.item_id("base:bronze_pickaxe");
    assert_eq!(reg.drops_for(hard, wood), None, "tier 1 blocked");
    assert!(reg.drops_for(hard, stone).is_some(), "tier 2 allowed");
    assert!(reg.drops_for(hard, bronze).is_some(), "tier 3 allowed");
    // Base ores are tier-0 gated (any pickaxe).
    let copper = b(&reg, "base:copper_ore");
    assert!(reg.drops_for(copper, wood).is_some());
}

#[test]
fn full_bronze_chain_resolves() {
    let reg = base_reg();
    // Smelts: raw -> ingots, blend -> bronze, logs -> charcoal (via tag).
    let raw_cu = it(&reg, "base:raw_copper");
    let cu = it(&reg, "base:copper_ingot");
    assert_eq!(reg.smelt_for(raw_cu).unwrap().output, cu);
    assert_eq!(
        reg.smelt_for(it(&reg, "base:bronze_blend")).unwrap().output,
        it(&reg, "base:bronze_ingot")
    );
    assert_eq!(
        reg.smelt_for(it(&reg, "base:spruce_log")).unwrap().output,
        it(&reg, "base:charcoal"),
        "any log smelts to charcoal via the logs tag"
    );
    // Fuels: charcoal beats logs beats sticks.
    let f = |n: &str| reg.fuel_value(it(&reg, n)).unwrap();
    assert!(f("base:charcoal") > f("base:log"));
    assert!(f("base:log") > f("base:stick"));
    assert!(reg.fuel_value(raw_cu).is_none(), "ore is not fuel");
    // Blend recipe: 3 copper + 1 tin -> 4 blend.
    let tin = it(&reg, "base:tin_ingot");
    let mut g = vec![None; 4];
    g[0] = Some(ItemStack::new(&reg, cu, 1));
    g[1] = Some(ItemStack::new(&reg, cu, 1));
    g[2] = Some(ItemStack::new(&reg, cu, 1));
    g[3] = Some(ItemStack::new(&reg, tin, 1));
    let r = crate::crafting::match_recipe(&reg, &g, 2).expect("bronze blend recipe");
    assert_eq!((r.output, r.count), (it(&reg, "base:bronze_blend"), 4));
    // Bronze pickaxe from ingots.
    let bi = it(&reg, "base:bronze_ingot");
    let s = it(&reg, "base:stick");
    let mut g = vec![None; 9];
    for i in [0, 1, 2] {
        g[i] = Some(ItemStack::new(&reg, bi, 1));
    }
    g[4] = Some(ItemStack::new(&reg, s, 1));
    g[7] = Some(ItemStack::new(&reg, s, 1));
    assert_eq!(
        crate::crafting::match_recipe(&reg, &g, 3).unwrap().output,
        it(&reg, "base:bronze_pickaxe")
    );
    // Furnace craftable from 8 cobblestone.
    let c = it(&reg, "base:cobblestone");
    let mut g = vec![Some(ItemStack::new(&reg, c, 1)); 9];
    g[4] = None;
    assert_eq!(
        crate::crafting::match_recipe(&reg, &g, 3).unwrap().output,
        it(&reg, "base:furnace")
    );
}

#[test]
fn furnace_smelts_with_fuel_over_time() {
    use crate::world::{BlockEntity, FurnaceState};
    let reg = base_reg();
    let mut w = test_world_with("furnace", reg.clone());
    let pos = (2, 80, 2);
    w.set_block(pos.0, pos.1, pos.2, b(&reg, "base:furnace"));
    let raw = it(&reg, "base:raw_copper");
    let log = it(&reg, "base:log");
    w.block_entities.insert(
        pos,
        BlockEntity::Furnace(FurnaceState {
            input: Some(ItemStack::new(&reg, raw, 2)),
            fuel: Some(ItemStack::new(&reg, log, 2)),
            ..Default::default()
        }),
    );
    // 8s smelt at 0.1s ticks; the first log (15s) lights immediately.
    for _ in 0..85 {
        w.tick_entities(0.1);
    }
    let BlockEntity::Furnace(f) = &w.block_entities[&pos] else { panic!("furnace") };
    assert_eq!(f.output.unwrap().item, it(&reg, "base:copper_ingot"));
    assert_eq!(f.input.unwrap().count, 1, "one raw consumed");
    assert_eq!(f.fuel.unwrap().count, 1, "first log consumed for fuel");
    assert!(f.burn_left > 0.0, "log burns 15s, smelt took 8");
    // Second smelt needs the second log (relights at the 15s mark).
    for _ in 0..90 {
        w.tick_entities(0.1);
    }
    let BlockEntity::Furnace(f) = &w.block_entities[&pos] else { panic!("furnace") };
    assert_eq!(f.output.unwrap().count, 2);
    assert!(f.input.is_none());
    assert!(f.fuel.is_none(), "second log lit");
    // No fuel, no input: idle without panicking.
    for _ in 0..50 {
        w.tick_entities(0.1);
    }
}

#[test]
fn furnace_state_persists_and_breaks_drop_contents() {
    use crate::world::{BlockEntity, FurnaceState};
    let reg = base_reg();
    let mut w = test_world_with("furnace-save", reg.clone());
    let pos = (3, 80, 3);
    w.set_block(pos.0, pos.1, pos.2, b(&reg, "base:furnace"));
    w.block_entities.insert(
        pos,
        BlockEntity::Furnace(FurnaceState {
            input: Some(ItemStack::new(&reg, it(&reg, "base:raw_tin"), 5)),
            fuel: Some(ItemStack::new(&reg, it(&reg, "base:charcoal"), 3)),
            ..Default::default()
        }),
    );
    w.save_modified();
    // Reload: state comes back by item name.
    let mut w2 = World::load_or_create(w.save_dir_for_test(), reg.clone());
    for x in -2..=2 {
        for z in -2..=2 {
            w2.ensure_chunk(ChunkPos { x, z });
        }
    }
    let BlockEntity::Furnace(f) = &w2.block_entities[&pos] else { panic!("furnace") };
    assert_eq!(f.input.unwrap().count, 5);
    assert_eq!(f.fuel.unwrap().item, it(&reg, "base:charcoal"));
    // Breaking the block spills the contents.
    w2.set_block(pos.0, pos.1, pos.2, AIR);
    assert!(!w2.block_entities.contains_key(&pos));
    assert_eq!(w2.pending_drops.len(), 2, "input + fuel drop");
}

#[test]
fn copper_aliases_migrate_old_worlds() {
    let reg = base_reg();
    // Old-world palette references the retired copper mod's names.
    let dir = tmp_dir("copper-mig");
    std::fs::write(dir.join("seed"), "42").unwrap();
    std::fs::write(dir.join("palette"), "0 base:air\n1 copper:ore\n").unwrap();
    let mut data = Vec::new();
    data.extend_from_slice(b"WFC3");
    let total = 16 * 16 * 256usize;
    let mut left = total;
    while left > 0 {
        let run = left.min(u16::MAX as usize) as u16;
        data.extend_from_slice(&run.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        left -= run as usize;
    }
    std::fs::write(dir.join("c.0.0.wfc"), data).unwrap();
    let mut w = World::load_or_create(dir, reg.clone());
    w.ensure_chunk(ChunkPos { x: 0, z: 0 });
    assert_eq!(
        w.get_block(4, 60, 4),
        b(&reg, "base:copper_ore"),
        "copper:ore aliases to base:copper_ore instead of placeholder"
    );
}

#[test]
fn base_metal_ores_generate() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("metals"), reg.clone());
    let (cu, sn) = (b(&reg, "base:copper_ore"), b(&reg, "base:tin_ore"));
    let (mut found_cu, mut found_sn) = (0, 0);
    for cx in -3..3 {
        for cz in -3..3 {
            w.ensure_chunk(ChunkPos { x: cx, z: cz });
            for lx in 0..16 {
                for lz in 0..16 {
                    for y in 4..73 {
                        let blk = w.get_block(cx * 16 + lx, y, cz * 16 + lz);
                        if blk == cu {
                            found_cu += 1;
                        } else if blk == sn {
                            found_sn += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(found_cu > 0, "copper generates");
    assert!(found_sn > 0, "tin generates");
    assert!(found_cu > found_sn, "copper more common than tin");
}

// ---------------- food & farming ----------------

#[test]
fn food_data_and_recipes_resolve() {
    let reg = base_reg();
    let bread = reg.item(it(&reg, "base:bread"));
    let f = bread.food.as_ref().expect("bread is food");
    assert_eq!(f.hunger, 6.0);
    assert!(f.nutrition[0] > 0.0, "bread is grain");
    let stew = reg.item(it(&reg, "base:forest_stew")).food.clone().unwrap();
    assert!(stew.nutrition[1] > 0.0 && stew.nutrition[2] > 0.0 && stew.nutrition[3] > 0.0);
    // Stew crafts from mushroom+carrot+berry.
    let g3 = |a: &str, b2: &str, c: &str| {
        let mut g = vec![None; 9];
        g[0] = Some(ItemStack::new(&reg, it(&reg, a), 1));
        g[1] = Some(ItemStack::new(&reg, it(&reg, b2), 1));
        g[2] = Some(ItemStack::new(&reg, it(&reg, c), 1));
        g
    };
    assert_eq!(
        crate::crafting::match_recipe(&reg, &g3("base:mushroom", "base:carrot", "base:berry"), 3)
            .unwrap()
            .output,
        it(&reg, "base:forest_stew")
    );
    // Hoes and smelted foods.
    assert!(reg.item(it(&reg, "base:bronze_hoe")).tool.is_some());
    assert_eq!(
        reg.smelt_for(it(&reg, "base:potato")).unwrap().output,
        it(&reg, "base:baked_potato")
    );
    assert!(reg.block_id("base:wheat_seeds/stage1").is_some(), "stage1 registered");
    assert!(reg.block_id("base:wheat_seeds/stage2").is_some(), "stage2 registered");
    // Carrot plants its crop.
    assert_eq!(
        reg.item(it(&reg, "base:carrot")).places,
        reg.block_id("base:carrot_crop")
    );
}

#[test]
fn crops_grow_on_farmland_via_random_ticks() {
    let reg = base_reg();
    let mut w = test_world_with("crops", reg.clone());
    let h = w.surface_height(4, 4);
    let farm = b(&reg, "base:farmland");
    let seed0 = b(&reg, "base:wheat_seeds");
    // A field on farmland grows; a control column on dirt doesn't.
    for x in 0..16 {
        for z in 0..16 {
            if (x, z) == (6, 6) {
                continue;
            }
            w.set_block(x, h, z, farm);
            w.set_block(x, h + 1, z, seed0);
        }
    }
    w.set_block(6, h, 6, b(&reg, "base:dirt"));
    w.set_block(6, h + 1, 6, seed0);
    let mut rng = 12345u32;
    for _ in 0..3000 {
        w.random_tick(&mut rng);
    }
    let mut advanced = 0;
    for x in 0..16 {
        for z in 0..16 {
            if (x, z) != (6, 6) && w.get_block(x, h + 1, z) != seed0 {
                advanced += 1;
            }
        }
    }
    assert!(advanced > 0, "farmland crops should advance");
    assert_eq!(w.get_block(6, h + 1, 6), seed0, "dirt crop must not grow");
    // Stage chain terminates at ripe (stage2) with a harvest def.
    let ripe = b(&reg, "base:wheat_seeds/stage2");
    assert!(reg.block(ripe).crop_next.is_none());
    let (item, _, becomes) = reg.block(ripe).harvest.expect("ripe wheat harvests");
    assert_eq!(item, it(&reg, "base:wheat"));
    assert_eq!(becomes, seed0);
    // Bushes regrow anywhere.
    let bare = b(&reg, "base:berry_bush");
    for x in 0..16 {
        for z in 8..11 {
            w.set_block(x, h + 3, z, bare);
        }
    }
    for _ in 0..30000 {
        w.random_tick(&mut rng);
    }
    let fruited = b(&reg, "base:berry_bush/stage1");
    let refruited = (0..16)
        .flat_map(|x| (8..11).map(move |z| (x, z)))
        .filter(|&(x, z)| w.get_block(x, h + 3, z) == fruited)
        .count();
    assert!(refruited > 0, "bushes should refruit anywhere");
    // Cross rendering flags.
    assert!(reg.block(seed0).cross);
    assert!(!reg.is_solid(seed0));
}

#[test]
fn wild_food_generates_per_biome() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let has = |biome: Biome, blocks: &[&str], name: &str| -> bool {
        let (x, z) = find_biome(&g, biome).unwrap();
        let cp = ChunkPos::of_world(x, z);
        let mut w = World::new(42, tmp_dir(name), reg.clone());
        let ids: Vec<_> = blocks.iter().filter_map(|n| reg.block_id(n)).collect();
        for dx in -4..=4 {
            for dz in -4..=4 {
                let p = ChunkPos { x: cp.x + dx, z: cp.z + dz };
                w.ensure_chunk(p);
                for lx in 0..16 {
                    for lz in 0..16 {
                        for y in 64..140 {
                            if ids.contains(&w.get_block(p.x * 16 + lx, y, p.z * 16 + lz)) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    };
    assert!(has(Biome::Plains, &["base:wheat_seeds/stage2"], "ww"), "plains wheat");
    assert!(
        has(Biome::Forest, &["base:carrot_crop/stage1", "base:berry_bush/stage1"], "wf"),
        "forest carrots/berries"
    );
    assert!(
        has(Biome::Taiga, &["base:potato_crop/stage1", "base:wild_mushroom"], "wt"),
        "taiga potato/mushroom"
    );
}

#[test]
fn raycast_hits_nonsolid_plants() {
    let reg = base_reg();
    let mut w = test_world_with("plantray", reg.clone());
    let h = w.surface_height(0, 0);
    let bush = b(&reg, "base:berry_bush");
    w.set_block(3, h + 5, 0, bush);
    let hit = raycast(&w, Vec3::new(0.5, h as f32 + 5.5, 0.5), Vec3::X, 6.0)
        .expect("plants must be targetable");
    assert_eq!(hit.block, (3, h + 5, 0));
    assert!(!reg.is_solid(bush), "bush stays non-solid for physics");
}

#[test]
fn browser_and_recipe_index() {
    let reg = base_reg();
    // Filtering: variants hidden, search matches label and id.
    let all = crate::browser_items(&reg, "");
    assert!(all.iter().all(|i| !reg.item(*i).name.contains('/')));
    let q = crate::browser_items(&reg, "bronze");
    assert!(q.iter().any(|i| reg.item(*i).name == "base:bronze_pickaxe"));
    assert!(!crate::browser_items(&reg, "base:stick").is_empty(), "id search");
    // recipes_for/uses_of.
    let bread = it(&reg, "base:bread");
    assert_eq!(reg.recipes_for(bread).len(), 1);
    let planks = it(&reg, "base:planks");
    let (uses, _, _) = reg.uses_of(planks);
    assert!(uses.iter().any(|r| r.output == it(&reg, "base:stick")), "tag uses counted");
    let charcoal = it(&reg, "base:charcoal");
    let (_, _, fuel) = reg.uses_of(charcoal);
    assert!(fuel, "charcoal reported as fuel");
    let (_, smelt_uses, _) = reg.uses_of(it(&reg, "base:raw_copper"));
    assert_eq!(smelt_uses.len(), 1, "raw copper used in smelting");
    assert_eq!(reg.smelts_for(it(&reg, "base:copper_ingot")).len(), 1);
}

#[test]
fn world_meta_roundtrip_and_legacy() {
    use crate::world::{read_world_meta, write_world_meta};
    let dir = tmp_dir("meta");
    write_world_meta(&dir, 777, "creative", 0.0);
    assert_eq!(read_world_meta(&dir), (Some(777), "creative".to_string(), 0.0));
    // Legacy: bare seed file means survival.
    let dir2 = tmp_dir("meta2");
    std::fs::write(dir2.join("seed"), "42").unwrap();
    assert_eq!(read_world_meta(&dir2), (Some(42), "survival".to_string(), 0.0));
    // load_or_create upgrades legacy worlds to world.toml.
    let reg = base_reg();
    let _ = World::load_or_create(dir2.clone(), reg);
    assert!(dir2.join("world.toml").exists());
}

// ---------------- gameplay (regression) ----------------

#[test]
fn raycast_hits_placed_block_with_correct_adjacent() {
    let reg = base_reg();
    let mut w = test_world_with("ray", reg.clone());
    let h = w.surface_height(0, 0);
    let y = h + 10;
    w.set_block(5, y, 0, b(&reg, "base:stone"));
    let hit = raycast(&w, Vec3::new(0.5, y as f32 + 0.5, 0.5), Vec3::X, 10.0).unwrap();
    assert_eq!(hit.block, (5, y, 0));
    assert_eq!(hit.adjacent, (4, y, 0));
    assert!(raycast(&w, Vec3::new(0.5, y as f32 + 0.5, 0.5), Vec3::X, 4.0).is_none());
}

#[test]
fn player_falls_lands_and_jumps() {
    let w = test_world("fall");
    let h = w.surface_height(4, 4);
    let mut p = Player::new(Vec3::new(4.5, h as f32 + 6.0, 4.5));
    let idle = Input { forward: 0.0, strafe: 0.0, jump: false, sprint: false };
    for _ in 0..300 {
        p.update(&w, &idle, Vec3::Z, Vec3::X, 1.0 / 60.0);
    }
    assert!(p.on_ground);
    let ground = p.pos.y;
    let jump = Input { jump: true, ..idle };
    let mut peak = ground;
    for _ in 0..60 {
        p.update(&w, &jump, Vec3::Z, Vec3::X, 1.0 / 60.0);
        peak = peak.max(p.pos.y);
    }
    let gain = peak - ground;
    assert!(gain > 1.0 && gain < 2.0, "jump height {gain}");
}

#[test]
fn water_flows_and_recedes() {
    let reg = base_reg();
    let mut w = test_world_with("flow", reg.clone());
    let h = w.surface_height(4, 4);
    let y = h + 5;
    let stone = b(&reg, "base:stone");
    for x in -8..=16 {
        for z in -8..=16 {
            w.set_block(x, y - 1, z, stone);
        }
    }
    let water = b(&reg, "base:water");
    w.set_block(4, y, 4, water);
    for _ in 0..200 {
        if !w.tick_water(10_000) {
            break;
        }
    }
    assert_eq!(reg.water_level(w.get_block(5, y, 4)), Some(1));
    assert_eq!(reg.water_level(w.get_block(11, y, 4)), Some(7));
    assert_eq!(w.get_block(12, y, 4), AIR);
    // Remove the source: dries up.
    w.set_block(4, y, 4, stone);
    for _ in 0..200 {
        if !w.tick_water(10_000) {
            break;
        }
    }
    assert!(!reg.is_water(w.get_block(6, y, 4)));
}

#[test]
fn inventory_and_clicks() {
    let reg = base_reg();
    let dirt = it(&reg, "base:dirt");
    let stone = it(&reg, "base:stone");
    let pick = it(&reg, "base:wood_pickaxe");
    let mut inv = Inventory::new();
    assert_eq!(inv.add(&reg, dirt, 70), 0);
    assert_eq!(inv.slots[0].unwrap().count, 64);
    assert_eq!(inv.slots[1].unwrap().count, 6);
    inv.add(&reg, pick, 1);
    inv.add(&reg, pick, 1);
    assert_eq!(inv.slots[2].unwrap().count, 1);
    assert_eq!(inv.slots[3].unwrap().count, 1, "tools must not stack");
    // Wear the tool out.
    let uses = reg.item(pick).durability;
    for _ in 0..uses {
        inv.wear_tool(&reg, 2);
    }
    assert!(inv.slots[2].is_none(), "tool breaks at zero durability");
    // Click matrix.
    let d = |n| Some(ItemStack::new(&reg, dirt, n));
    let s = |n| Some(ItemStack::new(&reg, stone, n));
    assert_eq!(click_stack(&reg, d(5), None, false), (None, d(5)));
    assert_eq!(click_stack(&reg, d(60), d(10), false), (d(64), d(6)));
    assert_eq!(click_stack(&reg, s(3), d(5), false), (d(5), s(3)));
    assert_eq!(click_stack(&reg, None, d(5), true), (d(1), d(4)));
    assert_eq!(click_stack(&reg, d(5), None, true), (d(2), d(3)));
}

#[test]
fn crafting_matches_shapes_and_grids() {
    let reg = base_reg();
    let planks = it(&reg, "base:planks");
    let stick = it(&reg, "base:stick");
    let cobble = it(&reg, "base:cobblestone");
    let grid = |size: usize, cells: &[(usize, crate::registry::ItemId)]| {
        let mut g = vec![None; size * size];
        for &(i, item) in cells {
            g[i] = Some(ItemStack::new(&reg, item, 1));
        }
        g
    };
    // Log -> planks in 2x2.
    let log = it(&reg, "base:log");
    let g = grid(2, &[(3, log)]);
    let r = crate::crafting::match_recipe(&reg, &g, 2).expect("log->planks");
    assert_eq!((r.output, r.count), (planks, 4));
    // Pickaxe in 3x3, not in 2x2.
    let g = grid(3, &[(0, planks), (1, planks), (2, planks), (4, stick), (7, stick)]);
    assert_eq!(
        crate::crafting::match_recipe(&reg, &g, 3).unwrap().output,
        it(&reg, "base:wood_pickaxe")
    );
    let g2 = grid(2, &[(0, planks), (1, planks), (2, stick)]);
    assert!(crate::crafting::match_recipe(&reg, &g2, 2).is_none());
    // Mirrored axe.
    let g = grid(3, &[(0, cobble), (1, cobble), (4, cobble), (3, stick), (6, stick)]);
    assert_eq!(
        crate::crafting::match_recipe(&reg, &g, 3).unwrap().output,
        it(&reg, "base:stone_axe")
    );
    // Mod recipe works too (loaded in data_mod test above; here just consume).
    let mut g = grid(2, &[(0, planks), (1, planks)]);
    crate::crafting::consume(&mut g);
    assert!(g[0].is_none() && g[1].is_none());
}

#[test]
fn wood_leaf_tiles_are_opaque_in_atlas() {
    // Regression: a tile painted past the row boundary once left spruce
    // leaves transparent (invisible canopies).
    let reg = base_reg();
    let (img, px, _) = crate::atlas::build_atlas(&reg.tex_files, None, &reg.tex_names);
    let tp = px / 16;
    for name in ["base:leaves", "base:birch_leaves", "base:spruce_leaves",
                 "base:jungle_leaves", "base:acacia_leaves"] {
        let id = reg.block_id(name).unwrap();
        let slot = reg.block(id).tiles[0] as u32;
        let cx = (slot % 16) * tp + tp / 2;
        let cy = (slot / 16) * tp + tp / 2;
        let i = ((cy * px + cx) * 4) as usize;
        assert_eq!(img[i + 3], 255, "{name} tile center must be opaque");
        assert!(img[i + 1] > img[i], "{name} should be green-ish");
    }
}

#[test]
fn atlas_builds_with_mod_texture() {
    let root = tmp_dir("atlasmod");
    let dir = root.join("texmod");
    std::fs::create_dir_all(dir.join("textures")).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"texmod\"\n").unwrap();
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
        enc.write_header().unwrap().write_image_data(&[255, 0, 0, 255].repeat(16)).unwrap();
    }
    std::fs::write(dir.join("textures/red.png"), png_data).unwrap();

    let reg = registry::load(&root);
    let red = reg.block_id("texmod:red").expect("block with png");
    let slot = reg.block(red).tiles[0];
    assert!(slot >= crate::atlas::FIRST_FREE_SLOT, "mod texture gets a free slot");
    assert!(
        reg.tex_names.contains(&("texmod/red".to_string(), slot)),
        "mod texture is pack-addressable by <mod_id>/<stem>: {:?}",
        reg.tex_names
    );
    let (img, px, _) = crate::atlas::build_atlas(&reg.tex_files, None, &reg.tex_names);
    let tp = px / 16;
    let tx = (slot as u32 % 16) * tp + tp / 2;
    let ty = (slot as u32 / 16) * tp + tp / 2;
    let i = ((ty * px + tx) * 4) as usize;
    assert_eq!(&img[i..i + 4], &[255, 0, 0, 255], "png blitted into its slot");
}

#[test]
fn missing_texture_uses_placeholder_not_crash() {
    let root = tmp_dir("misstex");
    let dir = root.join("m");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"m\"\n").unwrap();
    std::fs::write(dir.join("blocks.toml"), "[[block]]\nid = \"x\"\ntexture = \"nope.png\"\n").unwrap();
    let reg = registry::load(&root);
    let x = reg.block_id("m:x").unwrap();
    assert_eq!(reg.block(x).tiles[0], crate::atlas::UNKNOWN_SLOT);
    let m = reg.mods.iter().find(|m| m.id == "m").unwrap();
    assert!(m.error.as_deref().unwrap_or("").contains("missing texture"));
}

#[test]
#[ignore]
fn print_biome_locations() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    for biome in [
        Biome::Forest, Biome::Plains, Biome::Desert, Biome::Jungle,
        Biome::Scrubland, Biome::Taiga, Biome::Arctic, Biome::Mountains,
    ] {
        // Prefer a dry column so the screenshot shows land.
        let (mut bx, mut bz) = find_biome(&g, biome).unwrap();
        'scan: for dx in 0..200 {
            for dz in 0..200 {
                let (x, z) = (bx + dx, bz + dz);
                // Deep interior: same biome 32 blocks in every direction.
                let deep = [(0, 0), (32, 0), (-32, 0), (0, 32), (0, -32)]
                    .iter()
                    .all(|(ox, oz)| g.biome(x + ox, z + oz) == biome);
                if deep && g.surface_estimate(x, z) > crate::chunk::SEA_LEVEL + 2 {
                    bx = x;
                    bz = z;
                    break 'scan;
                }
            }
        }
        println!("{biome:?}: {bx},{bz}");
    }
}

// ---------------- texture packs ----------------

fn write_solid_png(path: &std::path::Path, w: u32, h: u32, rgba: [u8; 4]) {
    let mut data = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut data, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgba.repeat((w * h) as usize)).unwrap();
    }
    std::fs::write(path, data).unwrap();
}

fn tile_center(img: &[u8], px: u32, slot: u16) -> [u8; 4] {
    let tp = px / 16;
    let cx = (slot as u32 % 16) * tp + tp / 2;
    let cy = (slot as u32 / 16) * tp + tp / 2;
    let i = ((cy * px + cx) * 4) as usize;
    [img[i], img[i + 1], img[i + 2], img[i + 3]]
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
    assert_eq!(packs[1].name, "zeta", "missing pack.toml falls back to dir name");
}

#[test]
fn pack_tile_override_applied_at_slot() {
    let pack = tmp_dir("packstone");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/stone.png"), 8, 8, [255, 0, 255, 255]);
    let (base, bpx, _) = crate::atlas::build_atlas(&[], None, &[]);
    let (img, px, warns) = crate::atlas::build_atlas(&[], Some(crate::atlas::PackSource::Dir(pack.clone())), &[]);
    assert_eq!(px, bpx);
    assert!(warns.is_empty(), "no warnings: {warns:?}");
    let stone = *crate::atlas::builtin_slots().get("stone").unwrap();
    let dirt = *crate::atlas::builtin_slots().get("dirt").unwrap();
    assert_eq!(tile_center(&img, px, stone), [255, 0, 255, 255], "stone repainted");
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
    write_solid_png(&pack.join("tiles/gems/ruby_ore.png"), 4, 4, [255, 0, 255, 255]);

    // Without the pack the mod's art lands in the slot...
    let (img, px, _) = crate::atlas::build_atlas(&tex_files, None, &tex_names);
    assert_eq!(tile_center(&img, px, slot), [0, 255, 0, 255]);
    // ...with the pack, the pack's art wins (layered last).
    let (img, px, warns) = crate::atlas::build_atlas(&tex_files, Some(crate::atlas::PackSource::Dir(pack.clone())), &tex_names);
    assert!(warns.is_empty(), "{warns:?}");
    assert_eq!(tile_center(&img, px, slot), [255, 0, 255, 255], "pack > mod");
}

#[test]
fn pack_unknown_and_unreadable_files_warn() {
    let pack = tmp_dir("packwarn");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/notatile.png"), 4, 4, [1, 2, 3, 255]);
    std::fs::write(pack.join("tiles/stone.png"), b"this is not a png").unwrap();
    let (base, bpx, _) = crate::atlas::build_atlas(&[], None, &[]);
    let (img, px, warns) = crate::atlas::build_atlas(&[], Some(crate::atlas::PackSource::Dir(pack.clone())), &[]);
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
    assert_eq!(c.pack, "gemini", "fresh installs default to the bundled pack");
    c.pack = "dusk".to_string();
    let parsed = crate::config::Config::from_text(&c.to_text());
    assert_eq!(parsed, c, "config text round-trips the pack field");
    c.pack = String::new();
    let parsed = crate::config::Config::from_text(&c.to_text());
    assert!(parsed.pack.is_empty(), "explicit NONE round-trips as none");
    let legacy = crate::config::Config::from_text("volume=0.5\n");
    assert_eq!(legacy.pack, "gemini", "configs predating packs get the default");
}

#[test]
fn content_stamp_changes_on_pack_edit() {
    let root = tmp_dir("packstamp");
    std::fs::create_dir_all(root.join("dusk/tiles")).unwrap();
    write_solid_png(&root.join("dusk/tiles/stone.png"), 4, 4, [9, 9, 9, 255]);
    let before = crate::content_tree_stamp_of(&[&root]);
    write_solid_png(&root.join("dusk/tiles/dirt.png"), 4, 4, [9, 9, 9, 255]);
    let after = crate::content_tree_stamp_of(&[&root]);
    assert_ne!(before, after, "adding a pack file re-stamps the content tree");
}

#[test]
fn export_tiles_round_trip_reproduces_atlas() {
    let (img, px, _) = crate::atlas::build_atlas(&[], None, &[]);
    let out = tmp_dir("packexport");
    let n = crate::atlas::export_tiles(&out, &img, px, &[]).unwrap();
    assert_eq!(n, crate::atlas::builtin_slots().len(), "every named builtin tile exported");
    assert!(out.join("pack.toml").exists(), "stub pack.toml written");
    assert!(out.join("tiles/stone.png").exists());
    // Selecting the exported skeleton as a pack reproduces the atlas exactly.
    let (again, apx, warns) = crate::atlas::build_atlas(&[], Some(crate::atlas::PackSource::Dir(out.clone())), &[]);
    assert!(warns.is_empty(), "{warns:?}");
    assert_eq!(apx, px);
    assert_eq!(again, img, "export -> re-import is the identity");
}

// ---------------- world listing / new-world naming ----------------

#[test]
fn world_listing_sees_world_toml_and_legacy_seed() {
    // Regression: the title list only read the legacy `seed` file, so
    // world.toml worlds were invisible and their folder names got reused
    // by NEW WORLD — inheriting the old player.toml (inventory carryover).
    let root = tmp_dir("listworlds");
    crate::world::write_world_meta(&root.join("world1"), 42, "survival", 0.0);
    std::fs::create_dir_all(root.join("old")).unwrap();
    std::fs::write(root.join("old/seed"), "7").unwrap();
    std::fs::create_dir_all(root.join("junk")).unwrap();
    std::fs::write(root.join("stray.txt"), "x").unwrap();
    let worlds = crate::world::list_worlds(&root);
    assert_eq!(
        worlds,
        vec![("old".to_string(), 7), ("world1".to_string(), 42)],
        "world.toml and legacy worlds both list; junk doesn't"
    );
}

#[test]
fn new_world_name_never_reuses_existing_folder() {
    let root = tmp_dir("nextworld");
    crate::world::write_world_meta(&root.join("world1"), 1, "survival", 0.0);
    std::fs::write(root.join("world1/player.toml"), "leftover inventory").unwrap();
    let listed = crate::world::list_worlds(&root);
    assert_eq!(crate::next_world_name(&root, &listed), "world2");
    // Even a folder the listing can't see must not be adopted as "new".
    assert_eq!(crate::next_world_name(&root, &[]), "world2");
    assert_eq!(crate::next_world_name(&tmp_dir("nextworld-empty"), &[]), "world1");
}

// ---------------- animals: species, mobs, hunting ----------------

#[test]
fn base_animals_and_weapons_register() {
    let reg = base_reg();
    assert_eq!(
        reg.animals.iter().filter(|a| !a.hostile).count(),
        7,
        "seven wildlife species"
    );
    assert_eq!(
        reg.animals.iter().filter(|a| a.hostile).count(),
        6,
        "six wardens"
    );
    let deer = &reg.animals[reg.animal_id("base:deer").expect("deer")];
    assert_eq!(deer.biomes, vec!["forest"]);
    assert!(deer.flee_range > 0.0, "deer are skittish");
    assert!(deer.half_w > 0.2 && deer.height > 0.5, "collision derived from model");
    let boar = &reg.animals[reg.animal_id("base:boar").expect("boar")];
    assert_eq!(boar.flee_range, 0.0, "boars are bold");
    // Damage: swords explicit, axes implicit 3, bare items 1.
    let dmg = |n: &str| reg.item(it(&reg, n)).damage;
    assert_eq!(dmg("base:wood_sword"), 4.0);
    assert_eq!(dmg("base:stone_sword"), 5.0);
    assert_eq!(dmg("base:copper_sword"), 6.0);
    assert_eq!(dmg("base:bronze_sword"), 8.0);
    assert_eq!(dmg("base:wood_axe"), 3.0);
    assert_eq!(dmg("base:bread"), 1.0);
    // All four sword recipes resolve.
    for s in ["wood_sword", "stone_sword", "copper_sword", "bronze_sword"] {
        assert!(
            !reg.recipes_for(it(&reg, &format!("base:{s}"))).is_empty(),
            "{s} recipe"
        );
    }
}

#[test]
fn meats_smelt_and_stew_crafts() {
    let reg = base_reg();
    for m in ["venison", "boar", "chevon", "fowl", "rabbit"] {
        let cooked = it(&reg, &format!("base:cooked_{m}"));
        assert!(!reg.smelts_for(cooked).is_empty(), "raw {m} smelts");
        let raw = reg.item(it(&reg, &format!("base:raw_{m}")));
        let idx = crate::registry::NUTRIENTS.iter().position(|n| *n == "protein").unwrap();
        assert!(raw.food.as_ref().unwrap().nutrition[idx] > 0.0, "{m} carries protein");
    }
    assert!(!reg.smelts_for(it(&reg, "base:leather")).is_empty(), "hide tans to leather");
    // Hearty stew via the #base:meats tag: any meat + potato + mushroom.
    let mut g = vec![None; 9];
    g[0] = Some(ItemStack::new(&reg, it(&reg, "base:raw_rabbit"), 1));
    g[1] = Some(ItemStack::new(&reg, it(&reg, "base:potato"), 1));
    g[2] = Some(ItemStack::new(&reg, it(&reg, "base:mushroom"), 1));
    let r = crate::crafting::match_recipe(&reg, &g, 3).expect("hearty stew");
    assert_eq!(r.output, it(&reg, "base:hearty_stew"));
}

#[test]
fn mob_settles_on_ground_and_flees_from_damage() {
    let reg = base_reg();
    let mut w = test_world("mobphys");
    let si = reg.animal_id("base:deer").unwrap();
    let def = reg.animals[si].clone();
    // Flat pad well above any terrain, high in the air.
    let stone = reg.block_id("base:stone").unwrap();
    for x in -6..=6 {
        for z in -6..=6 {
            w.set_block(x, 180, z, stone);
            for y in 181..=186 {
                w.set_block(x, y, z, AIR);
            }
        }
    }
    let mut m = crate::mobs::Mob::new(si, Vec3::new(0.5, 184.0, 0.5), 0.0);
    m.health = def.health;
    let mut rng = 7u32;
    for _ in 0..120 {
        m.tick(&w, &def, Vec3::new(100.0, 181.0, 100.0), true, 0.0, 1.0 / 60.0, &mut rng, &mut Vec::new());
    }
    assert!(m.on_ground, "gravity settles the mob");
    assert!((m.pos.y - 181.0).abs() < 0.3, "standing on the pad, got y={}", m.pos.y);

    // Damage from the east: it panics away, gaining distance from the threat.
    let threat = m.pos + Vec3::new(2.0, 0.0, 0.0);
    m.hurt(&def, 4.0, threat);
    assert_eq!(m.state, crate::mobs::MobState::Flee);
    assert!(m.health < def.health);
    let d0 = (m.pos - threat).length();
    for _ in 0..90 {
        m.tick(&w, &def, Vec3::new(100.0, 181.0, 100.0), true, 0.0, 1.0 / 60.0, &mut rng, &mut Vec::new());
    }
    let d1 = (m.pos - threat).length();
    assert!(d1 > d0 + 1.0, "fled from the threat ({d0:.1} -> {d1:.1})");
    // Panic subsides back to idle within the flee timer.
    for _ in 0..400 {
        m.tick(&w, &def, Vec3::new(100.0, 181.0, 100.0), true, 0.0, 1.0 / 60.0, &mut rng, &mut Vec::new());
    }
    assert_ne!(m.state, crate::mobs::MobState::Flee, "calmed down");
}

#[test]
fn skittish_flees_players_bold_does_not() {
    let reg = base_reg();
    let w = test_world("mobskit");
    let deer_i = reg.animal_id("base:deer").unwrap();
    let boar_i = reg.animal_id("base:boar").unwrap();
    let deer_def = reg.animals[deer_i].clone();
    let boar_def = reg.animals[boar_i].clone();
    let pos = Vec3::new(0.5, 120.0, 0.5);
    let player = pos + Vec3::new(4.0, 0.0, 0.0); // within deer flee_range (10)
    let mut deer = crate::mobs::Mob::new(deer_i, pos, 0.0);
    let mut boar = crate::mobs::Mob::new(boar_i, pos, 0.0);
    let mut rng = 3u32;
    deer.tick(&w, &deer_def, player, true, 0.0, 1.0 / 60.0, &mut rng, &mut Vec::new());
    boar.tick(&w, &boar_def, player, true, 0.0, 1.0 / 60.0, &mut rng, &mut Vec::new());
    assert_eq!(deer.state, crate::mobs::MobState::Flee, "deer spooks");
    assert_ne!(boar.state, crate::mobs::MobState::Flee, "boar doesn't care");
}

#[test]
fn mob_ray_hit_works() {
    let reg = base_reg();
    let si = reg.animal_id("base:deer").unwrap();
    let def = &reg.animals[si];
    let m = crate::mobs::Mob::new(si, Vec3::new(10.0, 64.0, 10.0), 0.0);
    let origin = Vec3::new(10.0, 64.5, 6.0);
    let t = m.ray_hit(def, origin, Vec3::Z, 8.0).expect("aimed ray hits");
    assert!(t > 2.0 && t < 5.0, "hit distance sane: {t}");
    assert!(m.ray_hit(def, origin, -Vec3::Z, 8.0).is_none(), "away ray misses");
    assert!(m.ray_hit(def, origin, Vec3::Z, 2.0).is_none(), "out of reach");
}

#[test]
fn wildlife_seeds_matching_biomes_only() {
    let reg = base_reg();
    let mut w = test_world_with("mobseed", reg.clone());
    // Sweep a wide area; every spawned mob must belong to its chunk's biome.
    for cx in -12..12 {
        for cz in -12..12 {
            w.ensure_chunk(ChunkPos { x: cx, z: cz });
        }
    }
    for m in &w.mobs {
        let def = &reg.animals[m.species];
        // The group roll uses the chunk-center biome; members may scatter a
        // few blocks over a biome edge, which is fine.
        let cp = ChunkPos::of_world(m.pos.x.floor() as i32, m.pos.z.floor() as i32);
        let biome = w
            .generator
            .biome(cp.x * 16 + 8, cp.z * 16 + 8)
            .name()
            .to_lowercase();
        assert!(
            def.biomes.contains(&biome),
            "{} rolled in {biome} chunk",
            def.name
        );
        assert!(m.health > 0.0, "spawned alive");
    }
    assert!(w.mobs.len() <= crate::world::MOB_CAP);
}

#[test]
fn mob_persistence_round_trips_and_skips_unknown() {
    let reg = base_reg();
    let dir = tmp_dir("mobsave");
    let mut w = World::new(11, dir.clone(), reg.clone());
    let si = reg.animal_id("base:goat").unwrap();
    let mut m = crate::mobs::Mob::new(si, Vec3::new(3.5, 90.0, -2.5), 1.25);
    m.health = 7.0;
    w.mobs.push(m);
    w.save_modified();
    // Unknown species entries (removed mod) skip cleanly on load.
    let extra = "\n[[mob]]\nspecies = \"gone:wolf\"\npos = [0, 80, 0]\nyaw = 0\nhealth = 5\n";
    let path = dir.join("animals.toml");
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str(extra);
    std::fs::write(&path, text).unwrap();

    let w2 = World::load_or_create(dir, reg.clone());
    assert_eq!(w2.mobs.len(), 1, "goat loaded, unknown skipped");
    let g = &w2.mobs[0];
    assert_eq!(g.species, si);
    assert_eq!(g.health, 7.0);
    assert!((g.pos - Vec3::new(3.5, 90.0, -2.5)).length() < 0.01);
    assert!((g.yaw - 1.25).abs() < 0.01);
}

#[test]
fn wildlife_seed_marks_persist() {
    let reg = base_reg();
    let dir = tmp_dir("mobmark");
    let mut w = World::new(5, dir.clone(), reg.clone());
    w.ensure_chunk(ChunkPos { x: 0, z: 0 });
    let first = w.mobs.len();
    w.save_modified();
    // Reload: regenerating the same chunk must NOT reroll wildlife.
    let mut w2 = World::load_or_create(dir, reg);
    w2.ensure_chunk(ChunkPos { x: 0, z: 0 });
    assert_eq!(
        w2.mobs.len(),
        first,
        "seeded mark survives; no duplicate wildlife on revisit"
    );
}

#[test]
fn mod_can_add_species() {
    let root = tmp_dir("modanimal");
    let dir = root.join("fauna");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"fauna\"\n").unwrap();
    std::fs::write(
        dir.join("animals.toml"),
        r#"
[[animal]]
id = "shadow_cat"
biomes = ["forest"]
health = 6
tex = "@deer"
drops = [{ item = "base:hide", min = 1, max = 1 }]
"#,
    )
    .unwrap();
    let reg = registry::load(&root);
    let si = reg.animal_id("fauna:shadow_cat").expect("mod species registers");
    let def = &reg.animals[si];
    assert!(!def.model.is_empty(), "default model filled in");
    assert_eq!(def.drops.len(), 1, "drop resolved to base:hide");
}

#[test]
fn mobs_freeze_in_unloaded_chunks_and_unstick_when_buried() {
    let reg = base_reg();
    let mut w = test_world("mobfreeze"); // chunks -2..=2 are loaded
    let si = reg.animal_id("base:deer").unwrap();
    // Regression: mobs outside loaded chunks used to fall through the
    // unloaded (all-air) world, then get buried when the chunk streamed in.
    let far_i = w.mobs.len(); // test_world seeds natural wildlife too
    let mut far = crate::mobs::Mob::new(si, Vec3::new(500.5, 80.0, 500.5), 0.0);
    far.health = 10.0;
    w.mobs.push(far);
    let mut rng = 1u32;
    for _ in 0..60 {
        w.tick_mobs(Vec3::ZERO, 1.0, true, 0.0, 1.0 / 60.0, &mut rng);
    }
    assert_eq!(w.mobs[far_i].pos.y, 80.0, "frozen, not falling, outside loaded chunks");

    // A mob already wedged inside solid ground pops up to the surface.
    let stone = reg.block_id("base:stone").unwrap();
    for y in 100..=110 {
        for x in 0..4 {
            for z in 0..4 {
                w.set_block(x, y, z, stone);
            }
        }
    }
    let buried_i = w.mobs.len();
    let mut buried = crate::mobs::Mob::new(si, Vec3::new(1.5, 104.0, 1.5), 0.0);
    buried.health = 10.0;
    w.mobs.push(buried);
    w.tick_mobs(Vec3::new(60.0, 80.0, 60.0), 1.0, true, 0.0, 1.0 / 60.0, &mut rng);
    assert!(
        w.mobs[buried_i].pos.y >= 110.5,
        "unstuck above the stone, got y={}",
        w.mobs[buried_i].pos.y
    );
}

#[test]
fn embedded_gemini_pack_applies_without_folder() {
    let tiles = crate::atlas::embedded_pack("gemini").expect("gemini compiled in");
    assert!(tiles.len() > 100, "full pack embedded, got {}", tiles.len());
    assert!(crate::atlas::embedded_pack("nope").is_none());
    let (base, bpx, _) = crate::atlas::build_atlas(&[], None, &[]);
    let (img, px, warns) =
        crate::atlas::build_atlas(&[], Some(crate::atlas::PackSource::Embedded(tiles)), &[]);
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
    assert!(listed.iter().any(|p| p.id == "gemini"), "gemini listed: {listed:?}");
}

#[test]
fn model_boxes_can_carry_their_own_texture() {
    let reg = base_reg();
    let deer = &reg.animals[reg.animal_id("base:deer").unwrap()];
    let antler_slot = *crate::atlas::builtin_slots().get("antler").unwrap();
    let antlers: Vec<_> =
        deer.model.iter().filter(|b| b.name.starts_with("antler")).collect();
    assert_eq!(antlers.len(), 2, "deer has an antler pair");
    for b in &antlers {
        assert_eq!(b.tile, Some(antler_slot), "antlers use the bone tile, not fur");
    }
    assert!(
        deer.model.iter().filter(|b| b.name.starts_with("tine")).count() == 2,
        "antlers branch"
    );
    let body = deer.model.iter().find(|b| b.name == "body").unwrap();
    assert_eq!(body.tile, None, "body stays on the fur tile");
}

// ---------------- lighting ----------------

#[test]
fn torch_light_propagates_and_walls_block_it() {
    let reg = base_reg();
    let mut w = test_world("lighttorch");
    let stone = reg.block_id("base:stone").unwrap();
    let torch = reg.block_id("base:torch").unwrap();
    // Sealed 9x9x9 stone box, hollow interior, well above terrain.
    for x in 0..9 {
        for z in 0..9 {
            for y in 150..159 {
                let shell = x == 0 || x == 8 || z == 0 || z == 8 || y == 150 || y == 158;
                w.set_block(x, y, z, if shell { stone } else { AIR });
            }
        }
    }
    assert_eq!(w.light_at(4, 154, 4), (0, 0), "sealed box is pitch black");
    w.set_block(4, 151, 4, torch);
    assert_eq!(w.light_at(4, 151, 4).0, 14, "torch emits 14");
    assert_eq!(w.light_at(5, 151, 4).0, 13, "one step dims by one");
    assert_eq!(w.light_at(7, 151, 4).0, 11, "three steps");
    assert_eq!(w.light_at(4, 153, 4).0, 12, "propagates vertically too");
    assert_eq!(w.light_at(10, 151, 4).0, 0, "opaque wall stops it");
    w.set_block(4, 151, 4, AIR);
    assert_eq!(w.light_at(5, 151, 4).0, 0, "removing the torch relights dark");
}

#[test]
fn sky_light_surface_cave_and_roof_opening() {
    let reg = base_reg();
    let mut w = test_world("lightsky");
    let stone = reg.block_id("base:stone").unwrap();
    // Open surface reads full sky.
    let y = w.surface_height(2, 2);
    assert_eq!(w.light_at(2, y + 1, 2).1, 15, "surface is full daylight");
    // Sealed box: no sky inside; opening the roof floods it.
    for x in 20..29 {
        for z in 20..29 {
            for yy in 150..159 {
                let shell =
                    x == 20 || x == 28 || z == 20 || z == 28 || yy == 150 || yy == 158;
                w.set_block(x, yy, z, if shell { stone } else { AIR });
            }
        }
    }
    assert_eq!(w.light_at(24, 154, 24).1, 0, "sealed roof blocks sky");
    w.set_block(24, 158, 24, AIR); // skylight hole
    assert_eq!(w.light_at(24, 154, 24).1, 15, "column under the hole is lit");
    assert_eq!(w.light_at(26, 154, 24).1, 13, "and floods sideways, dimming");
}

#[test]
fn light_crosses_chunk_borders() {
    let reg = base_reg();
    let mut w = test_world("lightseam");
    let torch = reg.block_id("base:torch").unwrap();
    // Torch on the last column of chunk (0,0); the neighbor chunk must see it.
    w.set_block(15, 200, 8, torch);
    assert_eq!(w.light_at(15, 200, 8).0, 14);
    assert_eq!(w.light_at(16, 200, 8).0, 13, "crosses the seam");
    assert_eq!(w.light_at(19, 200, 8).0, 10, "keeps dimming next door");
}

#[test]
fn water_dims_sky_and_mod_blocks_can_glow() {
    let reg = base_reg();
    let mut w = test_world("lightwater");
    let water = reg.water_ids[0];
    let stone = reg.block_id("base:stone").unwrap();
    // A water-filled shaft walled in stone: light only enters from above,
    // dimming one level per water block.
    for x in 2..7 {
        for z in 2..7 {
            for y in 179..183 {
                w.set_block(x, y, z, stone);
            }
        }
    }
    for y in 180..183 {
        w.set_block(4, y, 4, water);
    }
    assert_eq!(w.light_at(4, 182, 4).1, 14, "first water block dims to 14");
    assert_eq!(w.light_at(4, 180, 4).1, 12, "third dims to 12");

    // Mod block with light = 9.
    let root = tmp_dir("glowmod");
    let dir = root.join("glow");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"glow\"\n").unwrap();
    std::fs::write(
        dir.join("blocks.toml"),
        "[[block]]\nid = \"lamp\"\ntexture = \"@stone\"\nlight = 9\n",
    )
    .unwrap();
    let reg2 = Arc::new(registry::load(&root));
    let lamp = reg2.block_id("glow:lamp").unwrap();
    assert_eq!(reg2.block(lamp).light_emit, 9);
    let mut w2 = test_world_with("lightmod", reg2);
    w2.set_block(4, 200, 4, lamp);
    assert_eq!(w2.light_at(4, 200, 4).0, 9, "emitter itself");
    assert_eq!(w2.light_at(4, 202, 4).0, 7, "two steps out");
}

#[test]
fn placing_a_roof_casts_shadow() {
    let reg = base_reg();
    let mut w = test_world("lightshadow");
    let stone = reg.block_id("base:stone").unwrap();
    let y = w.surface_height(8, 8);
    assert_eq!(w.light_at(8, y + 1, 8).1, 15);
    w.set_block(8, y + 3, 8, stone); // roof two above the ground cell
    let shaded = w.light_at(8, y + 1, 8).1;
    assert!(shaded < 15, "column shadowed, got {shaded}");
    assert!(shaded >= 12, "but side-lit by flood, got {shaded}");
}

#[test]
fn relight_perf_sane() {
    let mut w = test_world("lightperf");
    let t0 = std::time::Instant::now();
    for _ in 0..10 {
        w.relight_and_cascade(ChunkPos { x: 0, z: 0 });
    }
    let per = t0.elapsed().as_secs_f32() / 10.0;
    assert!(per < 0.05, "relight cascade averaged {per:.4}s");
}

#[test]
fn torch_needs_ground_and_pops_without_it() {
    let reg = base_reg();
    let mut w = test_world("torchpop");
    let stone = reg.block_id("base:stone").unwrap();
    let torch = reg.block_id("base:torch").unwrap();
    w.set_block(5, 150, 5, stone);
    w.set_block(5, 151, 5, torch);
    assert_eq!(w.get_block(5, 151, 5), torch);
    // Mining the support pops the torch off as a drop.
    w.set_block(5, 150, 5, AIR);
    assert_eq!(w.get_block(5, 151, 5), AIR, "torch popped");
    assert!(
        w.pending_drops.iter().any(|(_, s)| reg.item(s.item).name == "base:torch"),
        "torch dropped as an item"
    );
    // Torch recipe: charcoal over stick -> 4.
    let mut g = vec![None; 9];
    g[0] = Some(ItemStack::new(&reg, it(&reg, "base:charcoal"), 1));
    g[3] = Some(ItemStack::new(&reg, it(&reg, "base:stick"), 1));
    let r = crate::crafting::match_recipe(&reg, &g, 3).expect("torch recipe");
    assert_eq!(r.output, it(&reg, "base:torch"));
    assert_eq!(r.count, 4);
}

// ---------------- chests ----------------

#[test]
fn chest_stores_spills_and_persists() {
    let reg = base_reg();
    let dir = tmp_dir("chestsave");
    let mut w = World::new(9, dir.clone(), reg.clone());
    w.ensure_chunk(ChunkPos { x: 0, z: 0 });
    let chest = reg.block_id("base:chest").unwrap();
    assert_eq!(reg.block(chest).interaction.as_deref(), Some("chest"));
    let pos = (4, 100, 4);
    w.set_block(pos.0, pos.1, pos.2, chest);
    let mut state = crate::world::ChestState::default();
    state.slots[0] = Some(ItemStack::new(&reg, it(&reg, "base:bread"), 3));
    state.slots[26] = Some(ItemStack::new(&reg, it(&reg, "base:bronze_ingot"), 7));
    w.block_entities.insert(pos, crate::world::BlockEntity::Chest(state));
    w.save_modified();

    // Round-trip by name, plus an unknown item that must skip cleanly.
    let path = dir.join("entities.toml");
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("\n[[chest]]\npos = [9, 90, 9]\n[[chest.slot]]\nindex = 0\nitem = \"gone:widget\"\ncount = 5\ndurability = 0\n");
    std::fs::write(&path, text).unwrap();
    let w2 = World::load_or_create(dir, reg.clone());
    let Some(crate::world::BlockEntity::Chest(c)) = w2.block_entities.get(&pos) else {
        panic!("chest reloaded")
    };
    assert_eq!(c.slots[0].map(|s| (reg.item(s.item).name.clone(), s.count)),
        Some(("base:bread".to_string(), 3)));
    assert_eq!(c.slots[26].map(|s| s.count), Some(7));
    let Some(crate::world::BlockEntity::Chest(c2)) = w2.block_entities.get(&(9, 90, 9)) else {
        panic!("second chest reloaded")
    };
    assert!(c2.slots.iter().all(|s| s.is_none()), "unknown item skipped");

    // Breaking the chest spills every stack.
    let mut w3 = w2;
    w3.set_block(pos.0, pos.1, pos.2, AIR);
    assert!(w3.block_entities.get(&pos).is_none());
    let spilled: Vec<_> = w3.pending_drops.iter().map(|(_, s)| s.count).collect();
    assert_eq!(spilled.iter().sum::<u32>(), 10, "3 bread + 7 ingots spilled");

    // Recipe: 8 planks in a ring.
    let mut g = vec![None; 9];
    for i in 0..9 {
        if i != 4 {
            g[i] = Some(ItemStack::new(&reg, it(&reg, "base:planks"), 1));
        }
    }
    let r = crate::crafting::match_recipe(&reg, &g, 3).expect("chest recipe");
    assert_eq!(r.output, it(&reg, "base:chest"));
}

// ---------------- hostiles: ire, wardens, projectiles ----------------

#[test]
fn ire_gains_decay_tiers_and_persistence() {
    let reg = base_reg();
    let dir = tmp_dir("iresave");
    let mut w = World::new(3, dir.clone(), reg.clone());
    // Block classes.
    assert_eq!(w.ire_for_block(reg.block_id("base:log").unwrap()), 0.3);
    assert_eq!(w.ire_for_block(reg.block_id("base:copper_ore").unwrap()), 0.4);
    assert_eq!(w.ire_for_block(reg.block_id("base:stone").unwrap()), 0.05);
    assert_eq!(w.ire_for_block(reg.block_id("base:planks").unwrap()), 0.0);
    // Tier thresholds.
    w.add_ire(30.0);
    assert_eq!(w.ire_tier(), 1, "uneasy");
    w.add_ire(60.0);
    assert_eq!(w.ire_tier(), 3, "wrathful");
    w.add_ire(500.0);
    assert_eq!(w.ire, 100.0, "clamped");
    // Decay: -4 per day.
    w.tick_ire(0.5);
    assert!((w.ire - 98.0).abs() < 0.01);
    // Planting refunds, capped at 8/day.
    for _ in 0..100 {
        w.plant_ire(0.5);
    }
    assert!((w.ire - 90.0).abs() < 0.01, "daily cap of 8, got {}", w.ire);
    w.tick_ire(0.6); // day rolls over -> cap resets
    w.plant_ire(0.5);
    assert!(w.ire < 90.0 - 2.0, "cap reset next day");
    // Persistence via world.toml.
    w.save_modified();
    let w2 = World::load_or_create(dir, reg);
    assert!((w2.ire - w.ire).abs() < 0.01, "ire round-trips");
}

#[test]
fn warden_hunts_strikes_and_caster_fires() {
    let reg = base_reg();
    let mut w = test_world("wardenhunt");
    let stone = reg.block_id("base:stone").unwrap();
    for x in -4..12 {
        for z in -4..12 {
            w.set_block(x, 150, z, stone);
            for y in 151..156 {
                w.set_block(x, y, z, AIR);
            }
        }
    }
    let ti = reg.animal_id("base:thornling").unwrap();
    let def = reg.animals[ti].clone();
    assert!(def.hostile && def.attack > 0.0);
    let player = Vec3::new(1.5, 151.0, 1.5);
    let mut m = crate::mobs::Mob::new(ti, Vec3::new(6.5, 151.0, 1.5), 0.0);
    m.health = def.health;
    let mut rng = 5u32;
    let mut events = Vec::new();
    m.tick(&w, &def, player, true, 0.0, 1.0 / 60.0, &mut rng, &mut events);
    assert_eq!(m.state, crate::mobs::MobState::Hunt, "aggro within range");
    // Walk it onto the player: contact damage fires once, then cools down.
    m.pos = player + Vec3::new(0.8, 0.0, 0.0);
    for _ in 0..30 {
        m.tick(&w, &def, player, true, 0.0, 1.0 / 60.0, &mut rng, &mut events);
        m.pos = player + Vec3::new(0.8, 0.0, 0.0);
    }
    let hits = events
        .iter()
        .filter(|e| matches!(e, crate::mobs::MobEvent::HitPlayer(..)))
        .count();
    assert_eq!(hits, 1, "swing cooldown limits contact damage");
    // Creative players are invisible to the wild.
    let mut calm = crate::mobs::Mob::new(ti, Vec3::new(6.5, 151.0, 1.5), 0.0);
    calm.health = def.health;
    let mut ev2 = Vec::new();
    calm.tick(&w, &def, player, false, 0.0, 1.0 / 60.0, &mut rng, &mut ev2);
    assert_ne!(calm.state, crate::mobs::MobState::Hunt, "no aggro when unattackable");

    // Dryad: holds range and lobs a thorn bolt.
    let di = reg.animal_id("base:dryad").unwrap();
    let ddef = reg.animals[di].clone();
    let mut d = crate::mobs::Mob::new(di, Vec3::new(9.5, 151.0, 1.5), 0.0);
    d.health = ddef.health;
    let mut ev3 = Vec::new();
    for _ in 0..90 {
        d.tick(&w, &ddef, player, true, 0.0, 1.0 / 60.0, &mut rng, &mut ev3);
    }
    assert!(
        ev3.iter().any(|e| matches!(e, crate::mobs::MobEvent::Cast(_))),
        "caster fired"
    );
}

#[test]
fn floaters_hover_and_projectiles_collide() {
    let reg = base_reg();
    let mut w = test_world("floaty");
    let ei = reg.animal_id("base:emberkin").unwrap();
    let def = reg.animals[ei].clone();
    assert!(def.movement_float && def.emissive);
    let gy = w.surface_height(4, 4);
    let mut m = crate::mobs::Mob::new(ei, Vec3::new(4.5, gy as f32 + 3.0, 4.5), 0.0);
    m.health = def.health;
    let mut rng = 9u32;
    let far = Vec3::new(200.0, 80.0, 200.0);
    for _ in 0..240 {
        m.tick(&w, &def, far, true, 0.0, 1.0 / 60.0, &mut rng, &mut Vec::new());
    }
    let under = w.surface_height(m.pos.x.floor() as i32, m.pos.z.floor() as i32);
    assert!(
        m.pos.y > under as f32 + 0.8,
        "wisp hovers instead of sinking (y={} ground={under})",
        m.pos.y
    );
    let _ = gy;

    // Projectile into a wall dies; into the player connects.
    let stone = reg.block_id("base:stone").unwrap();
    w.set_block(10, 200, 10, stone);
    let mut p = crate::mobs::Projectile {
        pos: Vec3::new(10.5, 200.5, 7.0),
        vel: Vec3::new(0.0, 0.0, 20.0),
        tile: 0,
        damage: 3.0,
        age: 0.0,
        from_player: false,
        drop_item: None,
    };
    let mut outcome = crate::mobs::ProjHit::None;
    for _ in 0..60 {
        outcome = p.tick(&w, far, 1.0 / 30.0);
        if !matches!(outcome, crate::mobs::ProjHit::None) {
            break;
        }
    }
    assert!(matches!(outcome, crate::mobs::ProjHit::Block), "bolt stopped by the wall");
    w.projectiles.push(crate::mobs::Projectile {
        pos: Vec3::new(4.5, 120.9, 2.0),
        vel: Vec3::new(0.0, 0.0, 12.0),
        tile: 0,
        damage: 3.0,
        age: 0.0,
        from_player: false,
        drop_item: None,
    });
    let mut dmg = 0.0;
    for _ in 0..60 {
        dmg += w.tick_projectiles(Vec3::new(4.5, 120.0, 4.5), 1.0 / 30.0);
    }
    assert_eq!(dmg, 3.0, "bolt connected with the player");
}

#[test]
fn spawner_respects_darkness_ire_and_tiers() {
    let reg = base_reg();
    let mut w = test_world("wardenspawn");
    let player = Vec3::new(8.0, (w.surface_height(8, 8) + 1) as f32, 8.0);
    let world_spawn = Vec3::new(-500.0, 70.0, -500.0); // far away, no exclusion
    let mut rng = 77u32;
    // Daytime: surface spawns are impossible (only underground wardens may
    // appear, if a cave pocket is found).
    for _ in 0..200 {
        w.tick_hostile_spawns(player, world_spawn, 1.0, 5.0, &mut rng);
    }
    for m in &w.mobs {
        let d = &reg.animals[m.species];
        if d.hostile {
            assert!(
                d.biomes.iter().any(|b| b == "underground"),
                "daytime surface spawn of {}",
                d.name
            );
        }
    }
    // Night at Calm: spawns only ire_min = 0 wardens, within the ring.
    w.mobs.retain(|m| !reg.animals[m.species].hostile);
    for _ in 0..300 {
        w.tick_hostile_spawns(player, world_spawn, 0.12, 5.0, &mut rng);
    }
    let hostiles: Vec<_> =
        w.mobs.iter().filter(|m| reg.animals[m.species].hostile).collect();
    assert!(hostiles.len() <= 2, "calm budget respected: {}", hostiles.len());
    for m in &hostiles {
        let d = &reg.animals[m.species];
        assert_eq!(d.ire_min, 0.0, "no provoked-tier wardens at calm");
        let dist = (m.pos - player).length();
        assert!((20.0..90.0).contains(&dist), "ring distance {dist}");
    }
    // Wrathful: higher budget, elites allowed.
    w.ire = 95.0;
    for _ in 0..300 {
        w.tick_hostile_spawns(player, world_spawn, 0.12, 5.0, &mut rng);
    }
    let n = w.mobs.iter().filter(|m| reg.animals[m.species].hostile).count();
    assert!(n > 2, "wrathful nights are busier: {n}");
}

#[test]
fn wardens_dissolve_at_dawn_and_never_save() {
    let reg = base_reg();
    let dir = tmp_dir("wardensave");
    let mut w = World::new(21, dir.clone(), reg.clone());
    w.ensure_chunk(ChunkPos { x: 0, z: 0 });
    let ti = reg.animal_id("base:thornling").unwrap();
    let deer_i = reg.animal_id("base:deer").unwrap();
    let y = w.surface_height(4, 4) as f32 + 1.0;
    for (si, x) in [(ti, 4.5f32), (deer_i, 6.5)] {
        let mut m = crate::mobs::Mob::new(si, Vec3::new(x, y, 4.5), 0.0);
        m.health = reg.animals[si].health;
        w.mobs.push(m);
    }
    // Never persisted.
    w.save_modified();
    let w2 = World::load_or_create(dir, reg.clone());
    assert_eq!(w2.mobs.len(), 1, "only the deer survived the save");
    assert_eq!(w2.mobs[0].species, deer_i);
    // Dawn dissolve: full daylight on an open surface removes the warden.
    let player = Vec3::new(5.0, y, 5.0);
    let mut rng = 3u32;
    w.tick_mobs(player, 1.0, true, 0.0, 1.0 / 60.0, &mut rng);
    assert!(
        !w.mobs.iter().any(|m| reg.animals[m.species].hostile),
        "warden dissolved in daylight"
    );
    assert!(
        w.mobs.iter().any(|m| m.species == deer_i),
        "the deer does not dissolve"
    );
}

// ---------------- bows & armor ----------------

#[test]
fn bow_armor_recipes_and_data_resolve() {
    let reg = base_reg();
    // Bows parse with charge stats and stack singly.
    let hb = reg.item(it(&reg, "base:hunting_bow"));
    assert_eq!(hb.bow.as_ref().unwrap().damage, 6.0);
    assert_eq!(hb.max_stack, 1);
    assert_eq!(hb.durability, 96);
    let wb = reg.item(it(&reg, "base:warbow"));
    assert_eq!(wb.bow.as_ref().unwrap().damage, 10.0);
    // Arrows are an ammo class.
    assert_eq!(reg.item(it(&reg, "base:arrow")).ammo.as_deref(), Some("arrow"));
    // Recipes: bows, arrow x4, all eight pieces.
    for n in ["hunting_bow", "warbow", "leather_helmet", "leather_chestplate",
              "leather_leggings", "leather_boots", "bronze_helmet",
              "bronze_chestplate", "bronze_leggings", "bronze_boots"] {
        assert!(!reg.recipes_for(it(&reg, &format!("base:{n}"))).is_empty(), "{n} recipe");
    }
    let arrows = reg.recipes_for(it(&reg, "base:arrow"));
    assert_eq!(arrows[0].count, 4, "one craft yields four arrows");
    // Armor points: full leather 7, full bronze 11; slots match.
    use crate::registry::ArmorSlot;
    let pts = |n: &str| reg.item(it(&reg, n)).armor.unwrap();
    assert_eq!(pts("base:leather_helmet"), (ArmorSlot::Head, 1));
    assert_eq!(pts("base:leather_chestplate"), (ArmorSlot::Chest, 3));
    let leather: u32 = ["helmet", "chestplate", "leggings", "boots"]
        .iter()
        .map(|p| pts(&format!("base:leather_{p}")).1)
        .sum();
    let bronze: u32 = ["helmet", "chestplate", "leggings", "boots"]
        .iter()
        .map(|p| pts(&format!("base:bronze_{p}")).1)
        .sum();
    assert_eq!((leather, bronze), (7, 11));
}

#[test]
fn armor_reduction_curve() {
    assert_eq!(crate::reduced_damage(10.0, 0), 10.0);
    assert!((crate::reduced_damage(10.0, 7) - 7.2).abs() < 0.01, "full leather: 28%");
    assert!((crate::reduced_damage(10.0, 11) - 5.6).abs() < 0.01, "full bronze: 44%");
    assert!((crate::reduced_damage(10.0, 50) - 4.0).abs() < 0.001, "capped at 60%");
}

#[test]
fn player_arrows_strike_mobs_and_stick_in_walls() {
    let reg = base_reg();
    let mut w = test_world("arrows");
    let deer_i = reg.animal_id("base:deer").unwrap();
    let mut deer = crate::mobs::Mob::new(deer_i, Vec3::new(8.5, 220.0, 8.5), 0.0);
    deer.health = 10.0;
    let di = w.mobs.len(); // natural wildlife is seeded too — track ours
    w.mobs.push(deer);
    let arrow_item = it(&reg, "base:arrow");
    // Arrow flying at the deer: hits through the normal hurt path.
    w.projectiles.push(crate::mobs::Projectile {
        pos: Vec3::new(8.5, 220.5, 5.0),
        vel: Vec3::new(0.0, 0.5, 18.0),
        tile: 0,
        damage: 6.0,
        age: 0.0,
        from_player: true,
        drop_item: Some(arrow_item),
    });
    let far = Vec3::new(300.0, 80.0, 300.0);
    let mut player_dmg = 0.0;
    for _ in 0..40 {
        player_dmg += w.tick_projectiles(far, 1.0 / 30.0);
    }
    assert!(w.mobs[di].health < 10.0, "arrow connected (health {})", w.mobs[di].health);
    assert_eq!(player_dmg, 0.0, "player arrows never hit the player");
    assert!(w.pending_drops.is_empty(), "flesh hits consume the arrow");
    // Arrow into a wall drops a recoverable arrow item.
    let stone = reg.block_id("base:stone").unwrap();
    for y in 220..226 {
        w.set_block(2, y, 20, stone);
    }
    w.projectiles.push(crate::mobs::Projectile {
        pos: Vec3::new(2.5, 222.5, 16.0),
        vel: Vec3::new(0.0, 0.0, 16.0),
        tile: 0,
        damage: 6.0,
        age: 0.0,
        from_player: true,
        drop_item: Some(arrow_item),
    });
    for _ in 0..40 {
        w.tick_projectiles(far, 1.0 / 30.0);
    }
    assert!(
        w.pending_drops.iter().any(|(_, s)| s.item == arrow_item),
        "wall hit dropped the arrow for recovery"
    );
}

// ---------------- stewardship ----------------

#[test]
fn saplings_parse_drop_and_grow() {
    let reg = base_reg();
    // Leaves carry sapling bonus drops of their own species.
    let oak_leaves = reg.block(reg.block_id("base:leaves").unwrap());
    let (bd_item, ch) = oak_leaves.bonus_drop.expect("leaves drop saplings");
    assert_eq!(reg.item(bd_item).name, "base:oak_sapling");
    assert!((ch - 0.1).abs() < 0.001);
    let spruce = reg.block(reg.block_id("base:spruce_leaves").unwrap());
    assert_eq!(
        reg.item(spruce.bonus_drop.unwrap().0).name,
        "base:spruce_sapling"
    );
    // Grow: sapling on dirt in open air becomes a real tree.
    let mut w = test_world("sapgrow");
    let dirt = reg.block_id("base:dirt").unwrap();
    let sap = reg.block_id("base:oak_sapling").unwrap();
    w.set_block(4, 199, 4, dirt);
    w.set_block(4, 200, 4, sap);
    let ire0 = {
        w.ire = 50.0;
        w.ire
    };
    assert!(w.try_grow_sapling(4, 200, 4, 7), "clear sky: grows");
    let log = reg.block_id("base:log").unwrap();
    assert_eq!(w.get_block(4, 200, 4), log, "trunk replaces the sapling");
    let leaves = reg.block_id("base:leaves").unwrap();
    let mut leaf_count = 0;
    for x in 0..9 {
        for z in 0..9 {
            for y in 200..212 {
                if w.get_block(x, y, z) == leaves {
                    leaf_count += 1;
                }
            }
        }
    }
    assert!(leaf_count > 8, "canopy grew ({leaf_count} leaves)");
    assert!((w.ire - (ire0 - 2.0)).abs() < 0.01, "maturation refunds 2 ire");
    // Blocked trunk: stays a sapling.
    let stone = reg.block_id("base:stone").unwrap();
    w.set_block(8, 199, 8, dirt);
    w.set_block(8, 200, 8, sap);
    w.set_block(8, 202, 8, stone);
    assert!(!w.try_grow_sapling(8, 200, 8, 7), "blocked: stays");
    assert_eq!(w.get_block(8, 200, 8), sap);
}

#[test]
fn offering_stone_values_and_dawn() {
    let reg = base_reg();
    let mut w = test_world("offer");
    let stone = reg.block_id("base:offering_stone").unwrap();
    assert_eq!(reg.block(stone).interaction.as_deref(), Some("offering"));
    assert_eq!(reg.block(stone).light_emit, 5, "faint wildlight");
    assert!(!reg.recipes_for(it(&reg, "base:offering_stone")).is_empty());
    // Value table: the wild's own materials 2.0, meat 1.0, bread hunger*0.25.
    let v = |name: &str, n: u32| {
        w.offering_value(&ItemStack::new(&reg, it(&reg, name), n))
    };
    assert_eq!(v("base:heartwood", 1), 2.0);
    assert_eq!(v("base:raw_venison", 2), 2.0);
    assert!((v("base:bread", 1) - 1.5).abs() < 0.01, "bread hunger 6 * 0.25");
    assert_eq!(v("base:oak_sapling", 1), 1.0);
    // Dawn: items taken, refund capped at 10.
    w.ire = 60.0;
    let mut st = crate::world::OfferingState::default();
    st.slots[0] = Some(ItemStack::new(&reg, it(&reg, "base:heartwood"), 4)); // 8.0
    st.slots[1] = Some(ItemStack::new(&reg, it(&reg, "base:raw_rabbit"), 5)); // 5.0
    w.block_entities.insert((3, 90, 3), crate::world::BlockEntity::Offering(st));
    let r = w.accept_offerings();
    assert!((r - 10.0).abs() < 0.01, "capped at 10, got {r}");
    assert!((w.ire - 50.0).abs() < 0.01);
    let Some(crate::world::BlockEntity::Offering(o)) = w.block_entities.get(&(3, 90, 3))
    else {
        panic!()
    };
    assert!(o.slots.iter().all(|s| s.is_none()), "the wild took everything");
    assert_eq!(w.accept_offerings(), 0.0, "empty stone gives nothing");
}

#[test]
fn breeding_makes_babies_that_grow() {
    let reg = base_reg();
    let mut w = test_world("breed");
    let deer_i = reg.animal_id("base:deer").unwrap();
    w.ire = 20.0;
    let before = w.mobs.len();
    for x in [4.5f32, 6.5] {
        let mut m = crate::mobs::Mob::new(deer_i, Vec3::new(x, 220.0, 4.5), 0.0);
        m.health = 10.0;
        m.fed = true;
        w.mobs.push(m);
    }
    let mut rng = 3u32;
    let events = w.tick_mobs(Vec3::new(200.0, 80.0, 200.0), 1.0, true, 0.0, 1.0 / 60.0, &mut rng);
    assert!(
        events.iter().any(|e| matches!(e, crate::mobs::MobEvent::Bred(_))),
        "birth event"
    );
    assert_eq!(w.mobs.len(), before + 3, "two parents + one baby");
    let baby = w.mobs.iter().find(|m| m.growth < 1.0).expect("a baby exists");
    assert!(baby.growth < 0.1);
    assert!((w.ire - 19.0).abs() < 0.01, "a birth refunds 1 ire");
    let parents_fed = w.mobs.iter().filter(|m| m.fed).count();
    assert_eq!(parents_fed, 0, "parents spent their meal");
    // Growth advances with time; babies persist through saves.
    let baby_growth = baby.growth;
    for _ in 0..120 {
        w.tick_mobs(Vec3::new(200.0, 80.0, 200.0), 1.0, true, 0.0, 1.0 / 60.0, &mut rng);
    }
    let baby2 = w.mobs.iter().find(|m| m.growth < 1.0).expect("still young");
    assert!(baby2.growth > baby_growth, "babies grow");
    // No immediate re-breeding: cooldown holds.
    let n_now = w.mobs.len();
    let ev2 = w.tick_mobs(Vec3::new(200.0, 80.0, 200.0), 1.0, true, 0.0, 1.0 / 60.0, &mut rng);
    assert!(!ev2.iter().any(|e| matches!(e, crate::mobs::MobEvent::Bred(_))));
    assert_eq!(w.mobs.len(), n_now);
}

#[test]
fn bedroll_and_breed_data_parse() {
    let reg = base_reg();
    let br = reg.item(it(&reg, "base:bedroll"));
    assert!(br.bedroll);
    assert_eq!(br.durability, 12);
    assert!(!reg.recipes_for(it(&reg, "base:bedroll")).is_empty());
    // Favorite foods resolved per species.
    let food_of = |sp: &str| {
        let d = &reg.animals[reg.animal_id(sp).unwrap()];
        d.breed_food.map(|f| reg.item(f).name.clone())
    };
    assert_eq!(food_of("base:deer").as_deref(), Some("base:berry"));
    assert_eq!(food_of("base:goat").as_deref(), Some("base:wheat"));
    assert_eq!(food_of("base:thornling"), None, "wardens don't breed");
}

// ---------------- iron & steel ----------------

#[test]
fn iron_and_steel_chains_resolve() {
    let reg = base_reg();
    // Ore gated on bronze.
    let ore = reg.block(reg.block_id("base:iron_ore").unwrap());
    assert_eq!(ore.min_tier, 3, "bronze picks required");
    // Chain: raw -> ingot -> blend -> steel.
    assert!(!reg.smelts_for(it(&reg, "base:iron_ingot")).is_empty());
    assert!(!reg.recipes_for(it(&reg, "base:steel_blend")).is_empty());
    assert!(!reg.smelts_for(it(&reg, "base:steel_ingot")).is_empty());
    // Tiers and damage.
    let tool = |n: &str| reg.item(it(&reg, n)).tool.unwrap();
    assert_eq!(tool("base:iron_pickaxe").2, 4);
    assert_eq!(tool("base:steel_pickaxe").2, 5);
    assert_eq!(reg.item(it(&reg, "base:steel_sword")).damage, 13.0);
    // Armor totals: iron 14, steel 18.
    let total = |m: &str| -> u32 {
        ["helmet", "chestplate", "leggings", "boots"]
            .iter()
            .map(|p| reg.item(it(&reg, &format!("base:{m}_{p}"))).armor.unwrap().1)
            .sum()
    };
    assert_eq!(total("iron"), 14);
    assert_eq!(total("steel"), 18);
    // All craftables resolve.
    for n in ["iron_pickaxe", "iron_sword", "steel_axe", "steel_boots",
              "iron_block", "steel_block", "shears", "excavation_brush"] {
        assert!(!reg.recipes_for(it(&reg, &format!("base:{n}"))).is_empty(), "{n}");
    }
    // Ember burns hot: 2x smelt speed.
    assert_eq!(reg.fuel_value(it(&reg, "base:ember")), Some((80.0, 2.0)));
    assert_eq!(reg.fuel_value(it(&reg, "base:charcoal")).map(|(_, s)| s), Some(1.0));
    // Shears flagged.
    assert!(reg.item(it(&reg, "base:shears")).shears);
}

#[test]
fn iron_ore_generates_in_band() {
    let reg = base_reg();
    let mut w = test_world("ironband");
    let ore = reg.block_id("base:iron_ore").unwrap();
    let mut found = 0;
    let mut out_of_band = 0;
    for x in -32..32 {
        for z in -32..32 {
            w.ensure_chunk(ChunkPos::of_world(x * 4, z * 4));
        }
    }
    for (_, c) in w.chunks.iter() {
        for (i, b) in c.raw().iter().enumerate() {
            if *b == ore.0 {
                found += 1;
                let y = i % 256;
                if !(1..=54).contains(&y) {
                    out_of_band += 1; // random-walk veins drift up to 5
                }
            }
        }
    }
    assert!(found > 0, "iron generates");
    assert_eq!(out_of_band, 0, "iron stays in its band");
}

#[test]
fn ember_fuel_speeds_the_furnace() {
    let reg = base_reg();
    let mut w = test_world("emberfast");
    let mut f = crate::world::FurnaceState::default();
    f.input = Some(ItemStack::new(&reg, it(&reg, "base:steel_blend"), 1));
    f.fuel = Some(ItemStack::new(&reg, it(&reg, "base:ember"), 1));
    w.block_entities.insert((0, 90, 0), crate::world::BlockEntity::Furnace(f));
    // 14 s steel smelt at 2x should finish in ~7 s.
    for _ in 0..80 {
        w.tick_entities(0.1);
    }
    let Some(crate::world::BlockEntity::Furnace(f)) = w.block_entities.get(&(0, 90, 0))
    else {
        panic!()
    };
    assert!(
        f.output.map(|s| reg.item(s.item).name.clone()).as_deref()
            == Some("base:steel_ingot"),
        "steel done in 8s of ember fire (progress {})",
        f.progress
    );
}

// ---------------- ruins & archaeology ----------------

#[test]
fn structures_parse_and_place() {
    let reg = base_reg();
    assert_eq!(reg.structures.len(), 6, "six base ruins");
    assert!(reg.loots.contains_key("base:ruin_artifacts"));
    assert!(reg.loots.contains_key("base:ruin_chest"));
    let cellar = reg
        .structures
        .iter()
        .position(|s| s.name == "base:buried_cellar")
        .expect("cellar");
    let mut w = test_world("ruinplace");
    w.place_structure(cellar, 2, 120, 2, 12345);
    let cob = reg.block_id("base:cobblestone").unwrap();
    assert_eq!(w.get_block(2, 121, 2), cob, "cellar wall");
    assert_eq!(w.get_block(4, 121, 4), AIR, "carved interior");
    // The chest exists, is loot-filled, and belongs to the wild.
    let chest = w
        .block_entities
        .iter()
        .find_map(|(_, e)| match e {
            crate::world::BlockEntity::Chest(c) => Some(c),
            _ => None,
        })
        .expect("ruin chest placed");
    assert!(chest.wild_owned, "the wild keeps its trophies");
    assert!(chest.slots.iter().flatten().count() >= 3, "loot inside");
    // Worn tools from loot arrive worn.
    let mut rng = 99u32;
    let mut saw_worn = false;
    for _ in 0..300 {
        for s in w.roll_loot("base:ruin_artifacts", 1, &mut rng) {
            let max = reg.item(s.item).durability;
            if max > 0 && s.durability < max {
                saw_worn = true;
                assert!(s.durability <= max / 4, "old tools are truly old");
            }
        }
    }
    assert!(saw_worn, "artifact tools roll worn");
}

#[test]
fn brushing_yields_once_and_transmutes() {
    let reg = base_reg();
    let mut w = test_world("brushing");
    let masonry = reg.block_id("base:cracked_masonry").unwrap();
    w.set_block(3, 150, 3, masonry);
    let mut rng = 7u32;
    let found = w.brush_block(3, 150, 3, &mut rng).expect("artifact found");
    assert!(found.count >= 1);
    assert_eq!(
        w.get_block(3, 150, 3),
        reg.block_id("base:cobblestone").unwrap(),
        "remnant becomes plain stone"
    );
    assert!(w.brush_block(3, 150, 3, &mut rng).is_none(), "artifact only once");
    // Breaking a remnant instead just drops cobble (greed loses the find).
    let d = reg.drops_for(masonry, None).unwrap();
    assert_eq!(reg.item(d.0).name, "base:cobblestone");
}

#[test]
fn ruins_generate_deterministically() {
    let reg = base_reg();
    let mut w = test_world_with("ruingen1", reg.clone());
    let markers = [
        reg.block_id("base:mossy_cobblestone").unwrap(),
        reg.block_id("base:packed_earth").unwrap(),
        reg.block_id("base:cracked_masonry").unwrap(),
    ];
    for cx in -10..10 {
        for cz in -10..10 {
            w.ensure_chunk(ChunkPos { x: cx, z: cz });
        }
    }
    let mut found_at = None;
    'outer: for (pos, c) in w.chunks.iter() {
        for (i, b) in c.raw().iter().enumerate() {
            if markers.contains(&crate::registry::BlockId(*b)) {
                found_at = Some((*pos, i));
                break 'outer;
            }
        }
    }
    let (pos, idx) = found_at.expect("some ruin generated in 400 chunks");
    // Same seed, fresh world: the same block sits in the same place.
    let mut w2 = test_world_with("ruingen2", reg.clone());
    w2.ensure_chunk(pos);
    assert_eq!(
        w2.chunks.get(&pos).unwrap().raw()[idx],
        w.chunks.get(&pos).unwrap().raw()[idx],
        "structure placement is deterministic"
    );
}

#[test]
fn charms_and_tablets_work() {
    let reg = base_reg();
    // Data parses.
    assert_eq!(reg.item(it(&reg, "base:charm_quiet")).charm.as_deref(), Some("quiet"));
    assert!(reg.item(it(&reg, "base:etched_tablet")).tablet);
    // The quiet charm shortens warden attention: at 10.5 blocks a
    // thornling (aggro 12) hunts normally but not with -2.
    let mut w = test_world("charmq");
    let ti = reg.animal_id("base:thornling").unwrap();
    let def = reg.animals[ti].clone();
    let player = Vec3::new(0.5, 200.0, 0.5);
    let pos = player + Vec3::new(10.5, 0.0, 0.0);
    let mut rng = 4u32;
    let mut a = crate::mobs::Mob::new(ti, pos, 0.0);
    a.health = def.health;
    a.tick(&w, &def, player, true, 0.0, 1.0 / 60.0, &mut rng, &mut Vec::new());
    assert_eq!(a.state, crate::mobs::MobState::Hunt, "in range normally");
    let mut b = crate::mobs::Mob::new(ti, pos, 0.0);
    b.health = def.health;
    b.tick(&w, &def, player, true, -2.0, 1.0 / 60.0, &mut rng, &mut Vec::new());
    assert_ne!(b.state, crate::mobs::MobState::Hunt, "quiet charm keeps you unseen");
    let _ = &mut w;
}

// ---------------- the server (sim/client split) ----------------

#[test]
fn server_ticks_at_fixed_rate_and_runs_the_world() {
    let reg = base_reg();
    let world = test_world("simsplit");
    let mut sv = crate::server::Server::new(world, 0.3, 42);
    let ctx = crate::server::PlayerCtx {
        pos: Vec3::new(8.0, 80.0, 8.0),
        spawn: Vec3::new(-500.0, 70.0, -500.0),
        attackable: true,
        aggro_mod: 0.0,
    };
    let t0 = sv.time_of_day;
    let mut evs = Vec::new();
    // 2 wall-seconds in odd chunks: the fixed tick must absorb it evenly.
    for _ in 0..120 {
        sv.advance(1.0 / 60.0, &ctx, &mut evs);
    }
    let advanced = sv.time_of_day - t0;
    assert!(
        (advanced - 2.0 / crate::server::DAY_LENGTH).abs() < 0.0005,
        "clock advanced by the simulated time, got {advanced}"
    );
    // A hitch doesn't spiral the simulation.
    sv.advance(30.0, &ctx, &mut evs);
    assert!(sv.time_of_day - t0 < 0.01, "hitch capped, not replayed");
    // Ire tier events flow through the server.
    sv.world.ire = 95.0;
    let mut evs2 = Vec::new();
    sv.advance(0.1, &ctx, &mut evs2);
    assert!(
        evs2.iter().any(|e| matches!(
            e,
            crate::server::SimEvent::IreTier { rose: true, .. }
        )),
        "tier change surfaced as a SimEvent"
    );
    let _ = reg;
}
