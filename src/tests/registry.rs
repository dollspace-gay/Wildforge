//! Registry content, recipes, mods, scripts, and content-graph integrity.

use super::*;

#[test]
fn base_registry_has_vanilla_content() {
    let reg = base_reg();
    assert_eq!(reg.block(AIR).name, "base:air");
    for name in [
        "base:grass",
        "base:stone",
        "base:water",
        "base:crafting_table",
    ] {
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
fn every_base_content_entry_has_a_real_texture() {
    let reg = base_reg();
    assert_no_missing_textures(&reg);
    let base = reg.mods.iter().find(|entry| entry.id == "base").unwrap();
    assert_eq!(base.error, None, "base content resolution errors");
}

#[test]
fn every_shipped_mod_content_entry_has_a_real_texture() {
    let mods = Path::new(env!("CARGO_MANIFEST_DIR")).join("mods");
    let reg = registry::load(&mods);
    assert_no_missing_textures(&reg);
    let errors: Vec<_> = reg
        .mods
        .iter()
        .filter_map(|entry| entry.error.as_ref().map(|error| (&entry.id, error)))
        .collect();
    assert!(
        errors.is_empty(),
        "shipped mod resolution errors: {errors:?}"
    );
}

fn assert_no_missing_textures(reg: &Registry) {
    let unknown = crate::atlas::UNKNOWN_SLOT;
    let missing_blocks: Vec<_> = reg
        .blocks
        .iter()
        .filter(|block| block.name != "base:unknown" && block.tiles.contains(&unknown))
        .map(|block| block.name.as_str())
        .collect();
    let missing_items: Vec<_> = reg
        .items
        .iter()
        .filter(|item| item.icon == unknown)
        .map(|item| item.name.as_str())
        .collect();
    assert!(
        missing_blocks.is_empty(),
        "blocks using the missing-texture tile: {missing_blocks:?}"
    );
    assert!(
        missing_items.is_empty(),
        "items using the missing-texture tile: {missing_items:?}"
    );
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
fn data_mod_loads_blocks_items_recipes_features() {
    let root = tmp_dir("datamod");
    write_demo_mod(&root);
    let reg = registry::load(&root);
    let ore = reg.block_id("testium:ore").expect("mod block registered");
    let shard = reg.item_id("testium:shard").expect("mod item registered");
    assert_eq!(reg.block(ore).label, "Testium Ore");
    assert_eq!(
        reg.drops_for(ore, reg.item_id("base:wood_pickaxe")),
        Some((shard, 1))
    );
    assert_eq!(reg.drops_for(ore, None), None, "requires_tool");
    let t_ore = reg.block_id("testium:ore").unwrap();
    assert!(
        reg.ores.iter().any(|o| o.block == t_ore),
        "mod ore feature registered"
    );
    assert!(
        reg.recipes
            .iter()
            .any(|r| r.output == reg.item_id("testium:ore").unwrap())
    );
    let m = reg.mods.iter().find(|m| m.id == "testium").unwrap();
    assert!(m.error.is_none(), "{:?}", m.error);
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
    let bad = reg
        .mods
        .iter()
        .find(|m| m.id == "bad")
        .expect("bad mod listed");
    assert!(bad.error.is_some());
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
    let allow = host.dispatch(
        &w,
        "on_block_break",
        (0i64, 0i64, 0i64, "base:bedrock".to_string()),
    );
    assert!(!allow, "script should cancel bedrock break");
    // Normal break allowed + commands queued.
    let allow = host.dispatch(
        &w,
        "on_block_break",
        (1i64, 70i64, 1i64, "base:dirt".to_string()),
    );
    assert!(allow);
    let cmds = host.take_cmds();
    assert!(
        cmds.iter()
            .any(|c| matches!(c, crate::script::Cmd::Give(n, 2) if n == "base:stick"))
    );
    assert!(cmds.iter().any(|c| matches!(c, crate::script::Cmd::Hud(_))));
    // KV survived across dispatches.
    assert!(
        !host
            .kv
            .borrow()
            .get("scripty")
            .unwrap()
            .get("count")
            .unwrap()
            .is_empty()
    );
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
        cmds.iter()
            .any(|c| matches!(c, crate::script::Cmd::Hud(m) if m == "bedrock confirmed")),
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
    std::fs::write(
        root.join("scripty/main.rhai"),
        "fn on_tick(dt) { this is broken",
    )
    .unwrap();
    host.load_mods(&mods);
    assert!(host.wants("on_tick"), "old AST must survive a bad edit");
    assert!(
        host.mods[0].error.is_some(),
        "and the error must be reported"
    );
}

#[test]
fn wood_families_registered_and_craftable() {
    let reg = base_reg();
    for w in ["birch", "spruce", "jungle", "acacia"] {
        assert!(reg.block_id(&format!("base:{w}_log")).is_some(), "{w} log");
        assert!(
            reg.block_id(&format!("base:{w}_leaves")).is_some(),
            "{w} leaves"
        );
        assert!(
            reg.block_id(&format!("base:{w}_planks")).is_some(),
            "{w} planks"
        );
        // Each log crafts into ITS OWN planks.
        let log = it(&reg, &format!("base:{w}_log"));
        let mut g = vec![None; 4];
        g[0] = Some(ItemStack::new(&reg, log, 1));
        let r = crate::crafting::match_recipe(&reg, &g, 2)
            .unwrap_or_else(|| panic!("{w} log -> planks recipe"));
        assert_eq!(
            r.output,
            it(&reg, &format!("base:{w}_planks")),
            "{w} planks output"
        );
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
    for w in [
        "planks",
        "birch_planks",
        "spruce_planks",
        "jungle_planks",
        "acacia_planks",
    ] {
        let p = it(&reg, &format!("base:{w}"));
        let g = grid(2, &[(0, p), (2, p)]);
        let r =
            crate::crafting::match_recipe(&reg, &g, 2).unwrap_or_else(|| panic!("sticks from {w}"));
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
        crate::crafting::match_recipe(&reg, &g, 2)
            .expect("mixed-plank table")
            .output,
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
    std::fs::write(
        dir.join("mod.toml"),
        "id = \"cherry\"\ndepends = [\"base\"]\n",
    )
    .unwrap();
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
    assert!(
        reg.block_id("base:wheat_seeds/stage1").is_some(),
        "stage1 registered"
    );
    assert!(
        reg.block_id("base:wheat_seeds/stage2").is_some(),
        "stage2 registered"
    );
    // Carrot plants its crop.
    assert_eq!(
        reg.item(it(&reg, "base:carrot")).places,
        reg.block_id("base:carrot_crop")
    );
}

#[test]
fn browser_and_recipe_index() {
    let reg = base_reg();
    // Filtering: variants hidden, search matches label and id.
    let all = crate::browser_items(&reg, "");
    assert!(all.iter().all(|i| !reg.item(*i).name.contains('/')));
    let q = crate::browser_items(&reg, "bronze");
    assert!(q.iter().any(|i| reg.item(*i).name == "base:bronze_pickaxe"));
    assert!(
        !crate::browser_items(&reg, "base:stick").is_empty(),
        "id search"
    );
    // recipes_for/uses_of.
    let bread = it(&reg, "base:bread");
    assert_eq!(reg.recipes_for(bread).len(), 1);
    let planks = it(&reg, "base:planks");
    let (uses, _, _) = reg.uses_of(planks);
    assert!(
        uses.iter().any(|r| r.output == it(&reg, "base:stick")),
        "tag uses counted"
    );
    let charcoal = it(&reg, "base:charcoal");
    let (_, _, fuel) = reg.uses_of(charcoal);
    assert!(fuel, "charcoal reported as fuel");
    let (_, smelt_uses, _) = reg.uses_of(it(&reg, "base:raw_copper"));
    assert_eq!(smelt_uses.len(), 1, "raw copper used in smelting");
    assert_eq!(reg.smelts_for(it(&reg, "base:copper_ingot")).len(), 1);
}

#[test]
fn bucket_items_registered_and_craftable() {
    let reg = base_reg();
    let bucket = it(&reg, "base:bucket");
    let full = it(&reg, "base:bucket_water");
    assert_eq!(reg.item(bucket).max_stack, 1);
    assert_eq!(reg.item(full).max_stack, 1);
    assert!(!reg.recipes_for(bucket).is_empty(), "iron buys a bucket");
    // Both icons live in reserved atlas rows, clear of mod slots.
    use crate::atlas::builtin_slots;
    let slots = builtin_slots();
    assert_eq!(slots.get("bucket"), Some(&239));
    assert_eq!(
        slots.get("bucket_water"),
        Some(&(crate::style::EXTRA_BASE + 5))
    );
}

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
    assert!(
        deer.half_w > 0.2 && deer.height > 0.5,
        "collision derived from model"
    );
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
        let idx = crate::registry::NUTRIENTS
            .iter()
            .position(|n| *n == "protein")
            .unwrap();
        assert!(
            raw.food.as_ref().unwrap().nutrition[idx] > 0.0,
            "{m} carries protein"
        );
    }
    assert!(
        !reg.smelts_for(it(&reg, "base:leather")).is_empty(),
        "hide tans to leather"
    );
    // Hearty stew via the #base:meats tag: any meat + potato + mushroom.
    let mut g = vec![None; 9];
    g[0] = Some(ItemStack::new(&reg, it(&reg, "base:raw_rabbit"), 1));
    g[1] = Some(ItemStack::new(&reg, it(&reg, "base:potato"), 1));
    g[2] = Some(ItemStack::new(&reg, it(&reg, "base:mushroom"), 1));
    let r = crate::crafting::match_recipe(&reg, &g, 3).expect("hearty stew");
    assert_eq!(r.output, it(&reg, "base:hearty_stew"));
}

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
    assert_eq!(
        reg.item(it(&reg, "base:arrow")).ammo.as_deref(),
        Some("arrow")
    );
    // Recipes: bows, arrow x4, all eight pieces.
    for n in [
        "hunting_bow",
        "warbow",
        "leather_helmet",
        "leather_chestplate",
        "leather_leggings",
        "leather_boots",
        "bronze_helmet",
        "bronze_chestplate",
        "bronze_leggings",
        "bronze_boots",
    ] {
        assert!(
            !reg.recipes_for(it(&reg, &format!("base:{n}"))).is_empty(),
            "{n} recipe"
        );
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

#[test]
fn iron_and_steel_chains_resolve() {
    let reg = base_reg();
    // Ore gated on bronze.
    let ore = reg.block(reg.block_id("base:iron_ore").unwrap());
    assert_eq!(ore.min_tier, 3, "bronze picks required");
    // Chain: raw -> ingot -> bloomery -> bloom -> anvil -> steel.
    assert!(!reg.smelts_for(it(&reg, "base:iron_ingot")).is_empty());
    let chain = reg.bloomery.first().expect("bloomery chain registered");
    assert_eq!(chain.charge, it(&reg, "base:iron_ingot"));
    assert_eq!(chain.fuel, it(&reg, "base:charcoal"));
    assert_eq!(chain.bloom, it(&reg, "base:steel_bloom"));
    let worked = reg.worked.first().expect("anvil work registered");
    assert_eq!(worked.input, it(&reg, "base:steel_bloom"));
    assert_eq!(worked.output, it(&reg, "base:steel_ingot"));
    assert_eq!(worked.strikes, 3);
    // The old blend name still resolves for old saves (alias).
    assert_eq!(
        reg.item_id("base:steel_blend"),
        Some(it(&reg, "base:steel_bloom"))
    );
    // Tiers and damage.
    let tool = |n: &str| reg.item(it(&reg, n)).tool.unwrap();
    assert_eq!(tool("base:iron_pickaxe").2, 4);
    assert_eq!(tool("base:steel_pickaxe").2, 5);
    assert_eq!(reg.item(it(&reg, "base:steel_sword")).damage, 13.0);
    // Armor totals: iron 14, steel 18.
    let total = |m: &str| -> u32 {
        ["helmet", "chestplate", "leggings", "boots"]
            .iter()
            .map(|p| {
                reg.item(it(&reg, &format!("base:{m}_{p}")))
                    .armor
                    .unwrap()
                    .1
            })
            .sum()
    };
    assert_eq!(total("iron"), 14);
    assert_eq!(total("steel"), 18);
    // All craftables resolve.
    for n in [
        "iron_pickaxe",
        "iron_sword",
        "steel_axe",
        "steel_boots",
        "iron_block",
        "steel_block",
        "shears",
        "excavation_brush",
    ] {
        assert!(
            !reg.recipes_for(it(&reg, &format!("base:{n}"))).is_empty(),
            "{n}"
        );
    }
    // Ember burns hot: 2x smelt speed.
    assert_eq!(reg.fuel_value(it(&reg, "base:ember")), Some((80.0, 2.0)));
    assert_eq!(
        reg.fuel_value(it(&reg, "base:charcoal")).map(|(_, s)| s),
        Some(1.0)
    );
    // Shears flagged.
    assert!(reg.item(it(&reg, "base:shears")).shears);
}

#[test]
fn charms_and_tablets_work() {
    let reg = base_reg();
    // Data parses.
    assert_eq!(
        reg.item(it(&reg, "base:charm_quiet")).charm.as_deref(),
        Some("quiet")
    );
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
    a.tick(
        &w,
        &def,
        &[crate::server::PlayerCtx {
            pos: player,
            spawn: Vec3::ZERO,
            attackable: true,
            aggro_mod: 0.0,
        }],
        1.0 / 60.0,
        &mut rng,
        &mut Vec::new(),
    );
    assert_eq!(a.state, crate::mobs::MobState::Hunt, "in range normally");
    let mut b = crate::mobs::Mob::new(ti, pos, 0.0);
    b.health = def.health;
    b.tick(
        &w,
        &def,
        &[crate::server::PlayerCtx {
            pos: player,
            spawn: Vec3::ZERO,
            attackable: true,
            aggro_mod: -2.0,
        }],
        1.0 / 60.0,
        &mut rng,
        &mut Vec::new(),
    );
    assert_ne!(
        b.state,
        crate::mobs::MobState::Hunt,
        "quiet charm keeps you unseen"
    );
    let _ = &mut w;
}

/// The modding guide is executable: every `# mods/meadow/<file>` code
/// block in mods/README.md is extracted verbatim, written to a mods
/// dir, loaded, and its documented behavior asserted. Docs that drift
/// from the code fail here.
#[test]
fn mods_readme_example_mod_loads_and_works() {
    use crate::registry::Ingredient;
    let doc = std::fs::read_to_string("mods/README.md").expect("mods/README.md exists");
    let root = tmp_dir("readme-mod");
    let dir = root.join("meadow");
    std::fs::create_dir_all(dir.join("textures")).unwrap();

    // Extract fenced blocks labeled `# mods/meadow/<file>` (toml) or
    // `// mods/meadow/<file>` (rhai).
    let mut found = 0;
    for chunk in doc.split("```").skip(1).step_by(2) {
        let body = chunk.split_once('\n').map(|x| x.1).unwrap_or("");
        let first = body.lines().next().unwrap_or("");
        let label = first
            .trim_start_matches('#')
            .trim_start_matches("//")
            .trim();
        if let Some(rel) = label.strip_prefix("mods/meadow/") {
            std::fs::write(dir.join(rel), body).unwrap();
            found += 1;
        }
    }
    assert!(
        found >= 7,
        "doc ships a complete example, found {found} files"
    );

    // The doc's example references these PNGs; any size works.
    for tex in [
        "sunstone.png",
        "sunstone_ore.png",
        "sun_shard.png",
        "hen.png",
        "hen_face.png",
    ] {
        let mut png = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png, 4, 4);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header()
                .unwrap()
                .write_image_data(&[240, 200, 60, 255].repeat(16))
                .unwrap();
        }
        std::fs::write(dir.join("textures").join(tex), png).unwrap();
    }

    let reg = Arc::new(registry::load(&root));
    let meadow = reg
        .mods
        .iter()
        .find(|m| m.id == "meadow")
        .expect("meadow loads");
    assert!(meadow.error.is_none(), "meadow error: {:?}", meadow.error);
    assert!(meadow.has_script, "doc example ships a script");

    // Blocks: qualified names, light, tool gating, cross-mod drops.
    let sunstone = reg.block_id("meadow:sunstone").expect("block qualified");
    let d = reg.block(sunstone);
    assert_eq!(d.light_emit, 9, "light = 9 as documented");
    assert_eq!(
        d.light_rgb.iter().copied().max(),
        Some(9),
        "light_color is hue-normalized: brightest channel = light level"
    );
    assert!(
        d.light_rgb[0] > d.light_rgb[2],
        "warm tint: red channel outshines blue"
    );
    assert!(d.requires_tool && d.tool == Some(crate::registry::ToolKind::Pickaxe));
    let ore = reg.block_id("meadow:sunstone_ore").unwrap();
    let shard = reg.item_id("meadow:sun_shard").expect("item registered");
    assert_eq!(
        reg.block(ore).drops,
        Some((shard, 1)),
        "bare drop name auto-qualifies"
    );
    assert_eq!(reg.block(ore).min_tier, 1);

    // Items: @builtin icon + food defaults.
    let bread = reg.item_id("meadow:honey_bread").unwrap();
    let f = reg.item(bread).food.as_ref().expect("food block parses");
    assert_eq!(f.hunger, 7.0);
    assert_eq!(f.eat_time, 1.5, "eat_time defaults to 1.5 as documented");
    assert_eq!(reg.item(bread).max_stack, 64, "max_stack defaults to 64");

    // Recipes: 2x2 shard square crafts a sunstone (matched in-grid).
    let stone_item = reg.item_id("meadow:sunstone").unwrap();
    let mut grid: [Option<ItemStack>; 4] = [Some(ItemStack::new(&reg, shard, 1)); 4];
    let r = crate::crafting::match_recipe(&reg, &grid, 2).expect("2x2 recipe matches");
    assert_eq!(r.output, stone_item);
    grid[3] = None;
    assert!(
        crate::crafting::match_recipe(&reg, &grid, 2).is_none(),
        "shape is exact"
    );

    // Tag recipes: #shiny qualifies to meadow:shiny and accepts both members.
    let tag = reg.tags.get("meadow:shiny").expect("tag qualified");
    let copper = reg.item_id("base:copper_ingot").unwrap();
    assert!(
        tag.contains(&shard) && tag.contains(&copper),
        "tag lists both items"
    );
    let tag_recipe = reg
        .recipes
        .iter()
        .find(|r| r.output == shard && r.count == 4)
        .expect("tag recipe registered");
    assert!(
        tag_recipe
            .pattern
            .iter()
            .flatten()
            .any(|i| matches!(i, Ingredient::Any(l) if l.contains(&copper))),
        "#shiny resolved to an any-of ingredient"
    );

    // Smelt + fuel with documented defaults.
    let smelt = reg.smelt_for(stone_item).expect("smelt registered");
    assert_eq!(smelt.output, shard);
    assert_eq!(smelt.time, 6.0);
    assert!(
        reg.fuels
            .iter()
            .any(|(i, burn, speed)| i.matches(shard) && *burn == 20.0 && *speed == 1.5),
        "fuel with burn/speed as documented"
    );

    // Ore feature generates inside the documented band.
    let feat = reg
        .ores
        .iter()
        .find(|o| o.block == ore)
        .expect("ore feature registered");
    assert_eq!((feat.y_min, feat.y_max), (10, 40));
    let mut w = World::new(9, root.join("world"), reg.clone());
    let mut hits = 0;
    'scan: for cx in 0..6 {
        for cz in 0..6 {
            w.ensure_chunk(ChunkPos { x: cx, z: cz });
            let c = &w.chunks()[&ChunkPos { x: cx, z: cz }];
            for x in 0..16 {
                for z in 0..16 {
                    for y in 4..48 {
                        if c.get(x, y, z) == ore {
                            assert!((10..=45).contains(&(y as i32)), "vein walks stay near band");
                            hits += 1;
                            if hits > 3 {
                                break 'scan;
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(hits > 0, "sunstone ore generates in the world");

    // Animal: model boxes, breed food, drops.
    let hen = reg
        .animals
        .iter()
        .find(|a| a.name == "meadow:meadow_hen")
        .expect("animal registered");
    assert_eq!(hen.label, "Meadow Hen");
    assert_eq!(hen.health, 6.0);
    assert_eq!(
        hen.breed_food,
        reg.item_id("base:wheat"),
        "breed food resolves cross-mod"
    );
    assert!(
        hen.model.iter().any(|b| b.name == "leg"),
        "model boxes parse"
    );
    assert!(hen.biomes.contains(&"plains".to_string()));

    // Structure + loot table, qualified and linked.
    let shrine = reg
        .structures
        .iter()
        .find(|s| s.name == "meadow:sun_shrine")
        .expect("structure registered");
    assert_eq!(shrine.loot.as_deref(), Some("meadow:shrine_loot"));
    assert!(
        shrine.palette.values().any(|b| *b == sunstone),
        "palette maps to mod block"
    );
    let loot = reg
        .loots
        .get("meadow:shrine_loot")
        .expect("loot table registered");
    assert!(loot.iter().any(|e| e.item == shard && e.count == (1, 3)));
    assert!(
        loot.iter().any(|e| e.durability_frac == Some(0.4)),
        "worn-tool loot entry parses"
    );

    // Script: events fire, storage counts, sounds queue.
    let mut host = crate::script::ScriptHost::new();
    host.load_mods(&[("meadow".to_string(), dir.clone())]);
    host.dispatch(&w, "on_world_start", ("qa".to_string(),));
    let cmds = host.take_cmds();
    assert!(
        cmds.iter()
            .any(|c| matches!(c, crate::script::Cmd::Hud(m) if m.contains("meadow"))),
        "on_world_start toasts"
    );
    for _ in 0..2 {
        host.dispatch(
            &w,
            "on_block_break",
            (1i64, 2i64, 3i64, "meadow:sunstone_ore".to_string()),
        );
    }
    let cmds = host.take_cmds();
    assert!(
        cmds.iter()
            .any(|c| matches!(c, crate::script::Cmd::Hud(m) if m.contains("2"))),
        "storage_get/set counts across events"
    );
    assert!(
        cmds.iter()
            .any(|c| matches!(c, crate::script::Cmd::Sound(s) if s == "craft")),
        "play_sound queues"
    );
    // Breaking anything else stays allowed (handler returns true).
    assert!(host.dispatch(
        &w,
        "on_block_break",
        (0i64, 0i64, 0i64, "base:dirt".to_string())
    ));
}

/// The whole content graph, audited: recipes well-formed, tables and
/// palettes resolve, and every survival item is actually obtainable
/// (dropped, harvested, looted, crafted, or smelted from things that
/// are). Catches half-wired content before a player does.
#[test]
fn content_graph_is_complete_and_obtainable() {
    use crate::registry::Ingredient;
    use std::collections::HashSet;
    let reg = base_reg();
    for m in &reg.mods {
        assert!(m.error.is_none(), "mod {} load error: {:?}", m.id, m.error);
    }

    // Structure of every recipe / table / template.
    for r in &reg.recipes {
        let out = &reg.item(r.output).name;
        assert_eq!(r.pattern.len(), r.w * r.h, "recipe for {out} malformed");
        assert!(
            r.count > 0 && r.w <= 3 && r.h <= 3,
            "recipe for {out} malformed"
        );
        assert!(
            r.pattern.iter().any(|p| p.is_some()),
            "recipe for {out} is empty"
        );
    }
    for (name, entries) in &reg.loots {
        assert!(!entries.is_empty(), "loot table {name} is empty");
        for e in entries {
            assert!(
                e.weight > 0 && e.count.0 <= e.count.1,
                "loot table {name} entry malformed"
            );
        }
    }
    for st in &reg.structures {
        if let Some(l) = &st.loot {
            assert!(
                reg.loots.contains_key(l),
                "structure {} wants missing loot table {l}",
                st.name
            );
        }
        for layer in &st.layers {
            for row in layer {
                for ch in row.chars() {
                    assert!(
                        matches!(ch, '.' | '~' | 'C') || st.palette.contains_key(&ch),
                        "structure {} uses unmapped char '{ch}'",
                        st.name
                    );
                }
            }
        }
    }
    for b in &reg.blocks {
        if let Some((table, _)) = &b.brush {
            assert!(
                reg.loots.contains_key(table),
                "{} brushes into missing table {table}",
                b.name
            );
        }
    }
    let biomes = [
        "forest",
        "plains",
        "desert",
        "jungle",
        "scrubland",
        "taiga",
        "arctic",
        "mountains",
    ];
    for a in &reg.animals {
        assert!(!a.model.is_empty(), "animal {} has no model", a.name);
        assert!(a.health > 0.0, "animal {} has no health", a.name);
        if !a.hostile {
            assert!(
                !a.biomes.is_empty() && a.biomes.iter().all(|b| biomes.contains(&b.as_str())),
                "animal {} has invalid biomes {:?}",
                a.name,
                a.biomes
            );
        }
    }

    // Obtainability: seed with world sources, then close over crafting
    // and smelting until nothing new appears.
    let mut ok: HashSet<u16> = HashSet::new();
    for b in &reg.blocks {
        if b.hardness.is_some() {
            if let Some((it, n)) = b.drops
                && n > 0
            {
                ok.insert(it.0);
            }
            if let Some((it, _)) = b.bonus_drop {
                ok.insert(it.0);
            }
        }
        if let Some((it, _, _)) = b.harvest {
            ok.insert(it.0);
        }
    }
    for a in &reg.animals {
        for (it, _, mx) in &a.drops {
            if *mx > 0 {
                ok.insert(it.0);
            }
        }
    }
    for entries in reg.loots.values() {
        for e in entries {
            ok.insert(e.item.0);
        }
    }
    // Shears special-case: leaves come off whole (code path, not data).
    if reg.items.iter().any(|i| i.shears) {
        for b in &reg.blocks {
            if b.name.contains("leaves")
                && b.hardness.is_some()
                && let Some(it) = reg.item_id(&b.name)
            {
                ok.insert(it.0);
            }
        }
    }
    let ing_ok = |ing: &Ingredient, ok: &HashSet<u16>| match ing {
        Ingredient::One(i) => ok.contains(&i.0),
        Ingredient::Any(l) => l.iter().any(|i| ok.contains(&i.0)),
    };
    loop {
        let mut grew = false;
        for r in &reg.recipes {
            if !ok.contains(&r.output.0) && r.pattern.iter().flatten().all(|i| ing_ok(i, &ok)) {
                ok.insert(r.output.0);
                grew = true;
            }
        }
        for s in &reg.smelts {
            if !ok.contains(&s.output.0) && ing_ok(&s.input, &ok) {
                ok.insert(s.output.0);
                grew = true;
            }
        }
        // The steelworks: a fired bloomery turns charge into blooms,
        // and the anvil works blooms into bars (proven by sim tests).
        for b in &reg.bloomery {
            if !ok.contains(&b.bloom.0) && ok.contains(&b.charge.0) && ok.contains(&b.fuel.0) {
                ok.insert(b.bloom.0);
                grew = true;
            }
        }
        for w in &reg.worked {
            if !ok.contains(&w.output.0) && ok.contains(&w.input.0) {
                ok.insert(w.output.0);
                grew = true;
            }
        }
        // The bucket: dip it in any fluid and it comes up full — a
        // code path, like shears. Water and lava alike.
        for full_name in ["base:bucket_water", "base:bucket_lava"] {
            if let (Some(b), Some(f)) = (reg.item_id("base:bucket"), reg.item_id(full_name))
                && ok.contains(&b.0)
                && !ok.contains(&f.0)
            {
                ok.insert(f.0);
                grew = true;
            }
        }
        if let Some((sand, fuel, clear)) = reg.kiln_base
            && ok.contains(&sand.0)
            && ok.contains(&fuel.0)
        {
            if !ok.contains(&clear.0) {
                ok.insert(clear.0);
                grew = true;
            }
            for (p, g) in &reg.kiln {
                if !ok.contains(&g.0) && ok.contains(&p.0) {
                    ok.insert(g.0);
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }
    // World-only block items: the block deliberately drops a different
    // item or nothing (grass, ice, ores, ruin masonry, bedrock) - the
    // silk-touch category. Anything else unobtainable is a content bug.
    let world_only = |name: &str| {
        reg.block_id(name).is_some_and(|b| {
            let d = reg.block(b);
            let own = reg.item_id(name);
            d.hardness.is_none() || d.drops.map(|(it, _)| Some(it)) != Some(own)
        })
    };
    let missing: Vec<&str> = reg
        .items
        .iter()
        .enumerate()
        .filter(|(i, d)| !ok.contains(&(*i as u16)) && !world_only(&d.name))
        .map(|(_, d)| d.name.as_str())
        .collect();
    assert!(missing.is_empty(), "unobtainable in survival: {missing:?}");

    // The furnace can actually run, and husbandry foods exist.
    assert!(
        reg.fuels
            .iter()
            .any(|(f, burn, _)| *burn > 0.0 && ing_ok(f, &ok)),
        "no obtainable fuel"
    );
    for a in &reg.animals {
        if let Some(bf) = a.breed_food {
            assert!(ok.contains(&bf.0), "breed food for {} unobtainable", a.name);
        }
    }
}
