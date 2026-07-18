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
fn legacy_v1_world_migrates() {
    let reg = base_reg();
    let dir = tmp_dir("v1mig");
    std::fs::write(dir.join("seed"), "42").unwrap();
    // Hand-write a v1 chunk: (count u16 le, id u8) — 32768 cells.
    // id 3 = stone in the old enum order; fill everything with it.
    let mut data = Vec::new();
    let total = 16 * 16 * 128usize;
    let mut left = total;
    while left > 0 {
        let run = left.min(u16::MAX as usize) as u16;
        data.extend_from_slice(&run.to_le_bytes());
        data.push(3u8);
        left -= run as usize;
    }
    std::fs::write(dir.join("c.0.0.wfc"), data).unwrap();

    let mut w = World::load_or_create(dir, reg.clone());
    w.ensure_chunk(ChunkPos { x: 0, z: 0 });
    assert_eq!(w.get_block(5, 60, 5), b(&reg, "base:stone"), "v1 ids must remap by name");
    // Re-save produces v2.
    w.save_modified();
    let bytes = std::fs::read(w.save_dir_for_test().join("c.0.0.wfc")).unwrap();
    assert!(bytes.starts_with(b"WFC2"));
}

#[test]
fn unknown_palette_entries_become_placeholder() {
    let reg = base_reg();
    let dir = tmp_dir("unknown");
    std::fs::write(dir.join("seed"), "42").unwrap();
    // Palette maps id 1 to a mod block that no longer exists.
    std::fs::write(dir.join("palette"), "0 base:air\n1 gonemod:ore\n").unwrap();
    let mut data = Vec::new();
    data.extend_from_slice(b"WFC2");
    let total = 16 * 16 * 128usize;
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
    assert_eq!(reg.ores.len(), 1);
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
    let (img, px) = crate::atlas::build_with_mods(&reg.tex_files);
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
