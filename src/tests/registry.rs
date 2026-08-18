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
fn items_declare_carry_weight_and_stat_modifiers() {
    let root = tmp_dir("stat-mod");
    let dir = root.join("statmod");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mod.toml"),
        "id = \"statmod\"\nworld_api = 2\ndepends = [\"base\"]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("items.toml"),
        r#"
[[item]]
id = "pack"
name = "Haul Pack"
texture = "@leather"
carry_weight = 12

[[item.stats]]
kind = "carry"
flat = 128

[[item.stats]]
kind = "build_range"
mult_permille = 1200

[[item.stats]]
kind = "move_speed"
mult_permille = 900
"#,
    )
    .unwrap();
    let reg = registry::load(&root);
    let pack = reg.item_id("statmod:pack").expect("item registered");
    assert_eq!(reg.item(pack).carry_weight, 12);
    let stats = &reg.item(pack).stats;
    assert_eq!(stats.len(), 3, "all declared stats land on the item");
    let carry = stats
        .iter()
        .find(|m| m.kind == crate::stats::StatKind::Carry)
        .expect("carry modifier");
    assert_eq!(carry.flat, 128.0);
    assert_eq!(carry.mult_permille, 1_000, "flat-only keeps the neutral mult");
    let range = stats
        .iter()
        .find(|m| m.kind == crate::stats::StatKind::BuildRange)
        .expect("build range modifier");
    assert_eq!(range.mult_permille, 1_200);
    assert_eq!(range.flat, 0.0, "mult-only keeps a neutral flat");
    // Un-declared items keep the lightweight default.
    assert_eq!(reg.item(reg.item_id("base:stick").unwrap()).carry_weight, 1);
    assert!(reg.item(reg.item_id("base:stick").unwrap()).stats.is_empty());
}

#[test]
fn fixture_mod_declares_qualified_arcane_content_and_resonance() {
    let root = tmp_dir("arcane-valid-mod");
    let dir = root.join("greenfire");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mod.toml"),
        "id = \"greenfire\"\nworld_api = 2\ndepends = [\"base\"]\nretrogen = \"untouched_host_only\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("arcane.toml"),
        "schema_version = 1\n[[resonance]]\nid = \"verdance\"\nlabel = \"Verdance\"\n\n[[arcane_site]]\nid = \"singing_fault\"\nrequires = [\"fault\", \"carbonate_rock\"]\ncapacity_factor = 1.15\nresonance = { \"base:echo\" = 2, \"base:stone\" = 1 }\nrarity = 0.02\nradius_cells = 3\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("items.toml"),
        r#"
[[item]]
id = "seed"
name = "Verdant Seed"
texture = "@stick"
arcane = { capacity = 33, conductivity = 625, stability = 875, resonance = { verdance = 3, "base:root" = 1 }, on_destroy = "dross" }
"#,
    )
    .unwrap();
    let registry = registry::load(&root);
    assert!(
        registry.arcane_errors.is_empty(),
        "{:?}",
        registry.arcane_errors
    );
    let seed = registry.item_id("greenfire:seed").unwrap();
    let definition = registry.item(seed).arcane.as_ref().unwrap();
    assert_eq!(registry.item(seed).max_stack, 1);
    assert_eq!(definition.capacity, 33);
    assert_eq!(definition.resonance["greenfire:verdance"], 3);
    assert!(
        registry
            .arcane_registry
            .definitions
            .contains_key("greenfire:verdance")
    );
    let site = registry
        .arcane_sites
        .iter()
        .find(|site| site.id == "greenfire:singing_fault")
        .unwrap();
    assert_eq!(site.capacity_factor_permille, 1_150);
    assert_eq!(site.rarity_per_million, 20_000);
    assert_eq!(site.base_resonance_bias[3], 1);
    assert_eq!(site.base_resonance_bias[5], 2);
    assert_eq!(
        site.retrogen,
        crate::registry::RetrogenPolicy::UntouchedHostOnly
    );
}

#[test]
fn fixture_mod_preparation_uses_a_closed_residue_chain_and_rejects_forbidden_handlers() {
    let root = tmp_dir("alchemy-valid-mod");
    let dir = root.join("mirecraft");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mod.toml"),
        "id = \"mirecraft\"\nworld_api = 2\ndepends = [\"base\"]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("items.toml"),
        r#"
[[item]]
id = "mire_tonic"
name = "Mire Tonic"
texture = "@glass_bottle"
max_stack = 1
arcane = { capacity = 64, conductivity = 300, stability = 800, resonance = { "base:echo" = 1 }, on_destroy = "dross" }

[[item]]
id = "spent_mire"
name = "Spent Mire"
texture = "@compost"
material_class = "consumptive"
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("preparations.toml"),
        r#"
schema_version = 1

[[preparation]]
id = "mire_tonic"
label = "Mire Tonic"
process = "distill"
handler = "trace_sight"
application = "drink"
carrier = "alcohol"
solvent_item = "base:fermented_alcohol"
solvent_units = 256
dissolved_units = 0
ingredients = [
  { item = "base:echo_cap_ring", count = 1, retention_permille = 760 },
  { item = "base:rainbell_dew", count = 1, retention_permille = 880 },
]
charge_units = 12
resonance = "base:echo"
charge_rate = [1, 3]
dross_units = 2
steps = ["grind", "load", "heat", "charge", "distill", "cool", "filter"]
temperature_millic = [60000, 82000]
process_ticks = 1200
agitation = "still"
cleanliness_min = 800
output_item = "mirecraft:mire_tonic"
empty_vessel = "base:glass_bottle"
doses = 4
dose_units = 64
residue_item = "mirecraft:spent_mire"
residue_count = 1
shelf_life_ticks = 604800
storage_temperature_millic = [-10000, 30000]
stack_group = "mire_sight"
effect = { duration_ticks = 1200, recovery_ticks = 600, strength = 80 }
description = "A bounded local trace aid with a physical spent-mire residue."
"#,
    )
    .unwrap();
    let valid = registry::load(&root);
    assert!(valid.arcane_errors.is_empty(), "{:?}", valid.arcane_errors);
    let preparation = &valid.preparations["mirecraft:mire_tonic"];
    assert_eq!(preparation.output_item, "mirecraft:mire_tonic");
    assert_eq!(preparation.residue_item, "mirecraft:spent_mire");
    assert_eq!(
        preparation.handler,
        crate::alchemy::PreparationHandler::TraceSight
    );

    let bad_root = tmp_dir("alchemy-forbidden-mod");
    let bad_dir = bad_root.join("goldmaker");
    std::fs::create_dir_all(&bad_dir).unwrap();
    std::fs::write(
        bad_dir.join("mod.toml"),
        "id = \"goldmaker\"\nworld_api = 2\ndepends = [\"base\"]\n",
    )
    .unwrap();
    std::fs::write(
        bad_dir.join("preparations.toml"),
        "schema_version = 1\n[[preparation]]\nid = \"gold\"\nlabel = \"Gold\"\nprocess = \"infuse\"\nhandler = \"transmute_matter\"\n",
    )
    .unwrap();
    let invalid = registry::load(&bad_root);
    let error = invalid
        .mods
        .iter()
        .find(|info| info.id == "goldmaker")
        .and_then(|info| info.error.as_deref())
        .unwrap_or_default();
    assert!(
        error.contains("preparations.toml") || error.contains("unknown variant"),
        "{error}"
    );
    assert!(!invalid.preparations.contains_key("goldmaker:gold"));
}

fn write_ecology_fixture_mod(root: &Path, blocks: &str, features: Option<&str>) {
    let dir = root.join("ecofix");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mod.toml"),
        "id = \"ecofix\"\nworld_api = 2\ndepends = [\"base\"]\nretrogen = \"untouched_host_only\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("blocks.toml"), blocks).unwrap();
    if let Some(features) = features {
        std::fs::write(dir.join("features.toml"), features).unwrap();
    }
}

#[test]
fn valid_mod_ecology_organism_crystal_and_finite_mineral_use_engine_lifecycles() {
    let root = tmp_dir("ecology-valid-fixtures");
    write_ecology_fixture_mod(
        &root,
        r#"
[[block]]
id = "mire_chime"
name = "Mire Chime"
texture = "@mushroom"
cross = true
solid = false
opaque = false
hardness = 0.1
drops = "self"
arcane = { capacity = 120, conductivity = 600, stability = 600, resonance = { "base:tide" = 3, "base:root" = 1 }, on_destroy = "ambient" }
arcane_ecology = { roles = ["gatherer", "indicator"], habitat = ["wetland"], charge_capacity = 96, uptake_per_day = 3, release_per_day = 1, source = "ambient", resonance = { "base:tide" = 3, "base:root" = 1 }, dross_tolerance = 12, water_per_day_hu = 12, nutrient_per_day = 2, reproduction = "spore", seasons = [true, true, true, true], carrying_capacity = 12, harvest = "spore", regrowth_days = 8, min_stability = 0, max_stability = 1000, min_richness = 0 }

[[block]]
id = "springglass"
name = "Springglass"
texture = "@amethyst_block"
hardness = 5.0
tool = "pickaxe"
requires_tool = true
min_tier = 2
drops = "self"
material_class = "exceptional"
arcane = { capacity = 180, conductivity = 700, stability = 800, resonance = { "base:stone" = 3, "base:tide" = 1 }, on_destroy = "dross" }
arcane_ecology = { roles = ["reservoir", "indicator"], kind = "crystal", habitat = ["subsurface", "host_rock"], charge_capacity = 160, uptake_per_day = 4, release_per_day = 0, source = "ambient", resonance = { "base:stone" = 3, "base:tide" = 1 }, dross_tolerance = 20, water_per_day_hu = 2, nutrient_per_day = 0, reproduction = "bud", seasons = [true, true, true, true], carrying_capacity = 4, harvest = "seed_preserving", regrowth_days = 12, min_stability = 0, max_stability = 1000, min_richness = 0, crystal_stages = 3, preserving_tool_tier = 2 }

[[block]]
id = "songstone"
name = "Songstone"
texture = "@stone"
hardness = 5.0
tool = "pickaxe"
requires_tool = true
min_tier = 2
material_class = "geologically_finite"
materials = { songstone = 1200 }
arcane = { capacity = 70, conductivity = 900, stability = 500, resonance = { "base:stone" = 3, "base:echo" = 1 }, on_destroy = "ambient" }
arcane_ecology = { roles = ["conductor", "indicator"], kind = "finite_mineral", habitat = ["sedimentary_host"], charge_capacity = 64, uptake_per_day = 0, release_per_day = 0, source = "ambient", resonance = { "base:stone" = 3, "base:echo" = 1 }, dross_tolerance = 8, water_per_day_hu = 0, nutrient_per_day = 0, reproduction = "none", seasons = [true, true, true, true], carrying_capacity = 1, harvest = "destructive", regrowth_days = 0, min_stability = 0, max_stability = 1000, min_richness = 0 }
"#,
        Some(
            r#"
[[feature]]
type = "ore"
block = "ecofix:songstone"
replaces = "base:shale"
shape = "seam"
vein_size = 2
per_chunk = 1
chance = 0.1
y_range = [24, 64]
"#,
        ),
    );
    let reg = registry::load(&root);
    assert!(reg.arcane_errors.is_empty(), "{:?}", reg.arcane_errors);
    for content in [
        "ecofix:mire_chime",
        "ecofix:springglass",
        "ecofix:songstone",
    ] {
        assert!(
            reg.arcane_ecology.contains_key(content),
            "missing {content}"
        );
    }
    assert!(
        reg.ores
            .iter()
            .any(|ore| ore.block == reg.block_id("ecofix:songstone").unwrap())
    );
    let atlas = crate::planet_atlas::PlanetAtlas::fixture(9_911, 16).unwrap();
    let mut geography = crate::arcane_geography::ArcaneGeography::generate(
        &atlas,
        &reg,
        &crate::planet_atlas::CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    for content in ["ecofix:mire_chime", "ecofix:springglass"] {
        assert!(
            geography
                .dynamic
                .ecology
                .sites
                .iter()
                .any(|site| site.content_id == content),
            "{content} never entered deterministic site genesis"
        );
    }
    let before = geography.audit().unwrap().accounted_total;
    let mut water = geography
        .dynamic
        .ecology
        .sites
        .iter()
        .map(|site| (site.atlas_pos, 1_000_000))
        .collect();
    let living = atlas
        .biomes
        .countries
        .iter()
        .map(|country| country.id)
        .collect();
    crate::arcane_ecology::advance_toward(
        &mut geography,
        &atlas,
        &reg,
        1,
        crate::arcane_ecology::ECOLOGY_MAX_SITES,
        crate::arcane_ecology::EcologyConditions::new(
            &mut water,
            &living,
            &std::collections::BTreeSet::new(),
            &std::collections::BTreeSet::new(),
        ),
    )
    .unwrap();

    for (content, tool_tier, crystal_stage) in [
        ("ecofix:mire_chime", 0, None),
        ("ecofix:springglass", 2, Some(2)),
    ] {
        let index = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .position(|site| site.content_id == content)
            .unwrap();
        {
            let site = &mut geography.dynamic.ecology.sites[index];
            site.materialized_y = 50;
            site.stage = crate::arcane_ecology::EcologyStage::Mature;
            if let Some(stage) = crystal_stage {
                site.crystal_stage = stage;
            }
        }
        let pos = geography.dynamic.ecology.sites[index].block_pos().unwrap();
        let plan = crate::arcane_ecology::plan_harvest(&geography, &reg, pos, tool_tier)
            .unwrap_or_else(|| panic!("{content} did not enter the shared harvest path"));
        crate::arcane_ecology::apply_harvest(&mut geography, &reg, &plan, 2).unwrap();
    }

    let mineral = reg.arcane_ecology["ecofix:songstone"].clone();
    let surface = geography
        .dynamic
        .ecology
        .sites
        .iter()
        .find_map(|site| site.surface())
        .unwrap();
    let pos = crate::planet::BlockPos::new(surface.face(), surface.u(), 30, surface.v()).unwrap();
    let current =
        crate::arcane_ecology::plan_finite_mineral_harvest(&geography, &atlas, pos, &mineral)
            .expect("fixture finite mineral enters the shared dual-ledger plan");
    crate::arcane_ecology::apply_finite_mineral_harvest(&mut geography, &atlas, pos, &current)
        .unwrap();
    assert_eq!(geography.audit().unwrap().accounted_total, before);
}

#[test]
fn invalid_ecology_mods_reject_free_growth_missing_deposits_and_duplicate_harvest() {
    let root = tmp_dir("ecology-invalid-fixtures");
    write_ecology_fixture_mod(
        &root,
        r#"
[[block]]
id = "free_growth"
name = "Free Growth"
texture = "@mushroom"
cross = true
solid = false
opaque = false
hardness = 0.1
drops = "self"
arcane = { capacity = 80, conductivity = 500, stability = 500, resonance = { "base:root" = 1 }, on_destroy = "ambient" }
arcane_ecology = { roles = ["gatherer"], habitat = ["wetland"], charge_capacity = 64, uptake_per_day = 2, release_per_day = 0, source = "ambient", resonance = { "base:root" = 1 }, dross_tolerance = 8, water_per_day_hu = 0, nutrient_per_day = 1, reproduction = "seed", seasons = [true, true, true, true], carrying_capacity = 4, harvest = "fruit", regrowth_days = 4, min_stability = 0, max_stability = 1000, min_richness = 0 }
"#,
        None,
    );
    let reg = registry::load(&root);
    let error = reg
        .mods
        .iter()
        .find(|entry| entry.id == "ecofix")
        .and_then(|entry| entry.error.as_deref())
        .unwrap_or("");
    assert!(error.contains("water demand"), "{error}");

    let root = tmp_dir("ecology-invalid-graph-fixtures");
    write_ecology_fixture_mod(
        &root,
        r#"
[[block]]
id = "repeatable"
name = "Repeatable"
texture = "@mushroom"
cross = true
solid = false
opaque = false
hardness = 0.1
drops = "self"
harvest = { item = "base:stick", count = 1, becomes = "ecofix:repeatable" }
arcane = { capacity = 80, conductivity = 500, stability = 500, resonance = { "base:root" = 1 }, on_destroy = "ambient" }
arcane_ecology = { roles = ["indicator"], habitat = ["wetland"], charge_capacity = 64, uptake_per_day = 1, release_per_day = 0, source = "ambient", resonance = { "base:root" = 1 }, dross_tolerance = 8, water_per_day_hu = 1, nutrient_per_day = 1, reproduction = "seed", seasons = [true, true, true, true], carrying_capacity = 4, harvest = "fruit", regrowth_days = 4, min_stability = 0, max_stability = 1000, min_richness = 0 }

[[block]]
id = "unbacked_ore"
name = "Unbacked Ore"
texture = "@stone"
hardness = 4.0
material_class = "geologically_finite"
materials = { unbacked = 1200 }
arcane = { capacity = 70, conductivity = 700, stability = 700, resonance = { "base:stone" = 1 }, on_destroy = "ambient" }
arcane_ecology = { roles = ["conductor"], kind = "finite_mineral", habitat = ["sedimentary_host"], charge_capacity = 64, uptake_per_day = 0, release_per_day = 0, source = "ambient", resonance = { "base:stone" = 1 }, dross_tolerance = 8, water_per_day_hu = 0, nutrient_per_day = 0, reproduction = "none", seasons = [true, true, true, true], carrying_capacity = 1, harvest = "destructive", regrowth_days = 0, min_stability = 0, max_stability = 1000, min_richness = 0 }
"#,
        None,
    );
    let reg = registry::load(&root);
    assert!(
        reg.arcane_errors
            .iter()
            .any(|error| error.contains("repeatable block-harvest")),
        "{:?}",
        reg.arcane_errors
    );
    assert!(
        reg.arcane_errors
            .iter()
            .any(|error| error.contains("deposit rule")),
        "{:?}",
        reg.arcane_errors
    );
}

#[test]
fn broken_mod_is_skipped_with_error() {
    let root = tmp_dir("brokenmod");
    let dir = root.join("bad");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"bad\"\nworld_api = 2\n").unwrap();
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
fn script_arcane_gate_exposes_no_raw_credit_or_charged_spawn() {
    let root = tmp_dir("script-arcane-raw-gate");
    let mods = write_script_mod(
        &root,
        r#"
fn on_tick(dt) {
    add_current(1000);
    spawn_charged_item("base:ember", 1000);
}
"#,
    );
    let mut host = crate::script::ScriptHost::new();
    host.load_mods(&mods);
    let world = test_world("script-arcane-raw-gate-world");
    host.dispatch(&world, "on_tick", (0.1f64,));
    assert!(host.take_cmds().is_empty());
}

#[test]
fn script_events_cancel_and_queue_commands() {
    let root = tmp_dir("scriptmod");
    let mods = write_script_mod(
        &root,
        r#"
fn on_block_break(face, u, y, v, block) {
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
        (
            "pos_z".to_string(),
            4096i64,
            0i64,
            4096i64,
            "base:bedrock".to_string(),
        ),
    );
    assert!(!allow, "script should cancel bedrock break");
    // Normal break allowed + commands queued.
    let allow = host.dispatch(
        &w,
        "on_block_break",
        (
            "pos_z".to_string(),
            4097i64,
            70i64,
            4097i64,
            "base:dirt".to_string(),
        ),
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
fn script_quest_host_fns_queue_progress_and_accept() {
    let root = tmp_dir("scriptquest");
    let mods = write_script_mod(
        &root,
        r#"
fn on_tick(dt) {
    quest_accept("base:elder_cerium");
    quest_progress("base:elder_cerium", "gather", 2);
}
"#,
    );
    let mut host = crate::script::ScriptHost::new();
    host.load_mods(&mods);
    assert!(host.wants("on_tick"));
    let w = test_world("script-quest-world");
    host.dispatch(&w, "on_tick", (0.1f64,));
    let cmds = host.take_cmds();
    assert!(
        cmds.iter().any(|c| matches!(
            c,
            crate::script::Cmd::QuestAccept(id) if id == "base:elder_cerium"
        )),
        "quest_accept queues Cmd::QuestAccept"
    );
    assert!(
        cmds.iter().any(|c| matches!(
            c,
            crate::script::Cmd::QuestProgress { quest_id, objective, n }
                if quest_id == "base:elder_cerium" && objective == "gather" && *n == 2
        )),
        "quest_progress queues Cmd::QuestProgress"
    );
    // Scripts never write KV directly from quest host fns.
    assert!(host.kv.borrow().is_empty());
}

#[test]
fn base_gate_definitions_resolve_and_map_by_block() {
    let reg = base_reg();
    let idx = reg
        .gate_id("base:sealed_elder_door")
        .expect("base sealed door registers");
    let gate = &reg.gates[idx];
    assert_eq!(gate.flag, "elder_told_tales");
    assert_eq!(gate.value, "true");
    assert_eq!(
        gate.unlocked_block,
        reg.block_id("base:air"),
        "the sealed door opens to air"
    );
    assert!(gate.unbreakable_when_locked, "default stays sealed");
    assert_eq!(
        gate.message, "The elder's door is sealed shut.",
        "authored message preserved"
    );
    // The reverse map lets the runtime find the gate from its placed block.
    assert_eq!(
        reg.gate_for_block(gate.block),
        Some(idx),
        "sealed block maps back to its gate"
    );
    // A `type = "gate"` entry must never be treated as an ore.
    assert!(
        reg.ores.iter().all(|ore| ore.block != gate.block),
        "gate sealed block is not also an ore"
    );
}

#[test]
fn gate_feature_errors_fail_the_mod_and_never_gate() {
    let root = tmp_dir("gate-bad-mod");
    let dir = root.join("gatecrash");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mod.toml"),
        "id = \"gatecrash\"\nworld_api = 2\ndepends = [\"base\"]\nretrogen = \"untouched_host_only\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("features.toml"),
        r#"
[[feature]]
type = "gate"
id = "ghost_door"
block = "base:no_such_block"
flag = "ghost_key"
value = "true"
"#,
    )
    .unwrap();
    let invalid = registry::load(&root);
    assert!(
        invalid
            .material_errors
            .iter()
            .any(|e| e.contains("gatecrash:ghost_door") && e.contains("unknown block")),
        "unknown sealed block fails the pack: {:?}",
        invalid.material_errors
    );
    assert!(
        invalid.gates.iter().all(|g| g.id != "gatecrash:ghost_door"),
        "the broken gate never installs"
    );

    // A gate missing its flag key would softlock forever — also refused.
    let root2 = tmp_dir("gate-noflag-mod");
    let dir2 = root2.join("gatecrash2");
    std::fs::create_dir_all(&dir2).unwrap();
    std::fs::write(
        dir2.join("mod.toml"),
        "id = \"gatecrash2\"\nworld_api = 2\ndepends = [\"base\"]\nretrogen = \"untouched_host_only\"\n",
    )
    .unwrap();
    std::fs::write(
        dir2.join("features.toml"),
        r#"
[[feature]]
type = "gate"
id = "no_flag"
block = "base:cracked_masonry"
"#,
    )
    .unwrap();
    let invalid2 = registry::load(&root2);
    assert!(
        invalid2
            .material_errors
            .iter()
            .any(|e| e.contains("missing `flag`")),
        "missing flag key fails the pack: {:?}",
        invalid2.material_errors
    );
}

#[test]
fn gate_feature_in_an_external_mod_resolves_qualified() {
    let root = tmp_dir("gate-good-mod");
    let dir = root.join("gateworks");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mod.toml"),
        "id = \"gateworks\"\nworld_api = 2\ndepends = [\"base\"]\nretrogen = \"untouched_host_only\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("features.toml"),
        r#"
[[feature]]
type = "gate"
id = "iron_vault"
block = "base:cracked_masonry"
flag = "vault_key"
value = "true"
unlocked_block = "base:air"
"#,
    )
    .unwrap();
    let reg = registry::load(&root);
    let idx = reg
        .gate_id("gateworks:iron_vault")
        .expect("external mod gate registers with its mod prefix");
    assert_eq!(reg.gates[idx].flag, "vault_key");
    assert_eq!(reg.gates[idx].unlocked_block, reg.block_id("base:air"));
}

#[test]
fn base_settlement_definitions_resolve_with_rep_key_and_tiers() {
    let reg = base_reg();
    let idx = reg
        .settlement_id("base:elder_haven")
        .expect("base settlement registers");
    let def = &reg.settlements[idx];
    assert_eq!(
        def.rep_key, "rep_base:elder_haven",
        "default rep key derives from the qualified id"
    );
    assert_eq!(
        def.tiers.iter().map(|t| t.tier).collect::<Vec<_>>(),
        vec![2, 3],
        "tiers resolve in declared order"
    );
    assert_eq!(
        def.tiers.iter().map(|t| t.threshold).collect::<Vec<_>>(),
        vec![2, 5],
        "thresholds preserved"
    );
    // The tier-2 piece must resolve and be tagged.
    let hall = reg
        .pieces
        .iter()
        .find(|p| p.name == "base:haven_hall")
        .expect("haven_hall registers");
    assert_eq!(hall.settlement_tier, 2, "growth piece is tier 2");
    // The settlement assembly resolves to the settlement.
    let asm = reg
        .assemblies
        .iter()
        .find(|a| a.name == "base:elder_haven")
        .expect("settlement assembly registers");
    assert_eq!(
        asm.settlement.as_deref(),
        Some("base:elder_haven"),
        "assembly names its settlement"
    );
}

#[test]
fn settlement_errors_fail_the_mod_and_never_install() {
    // A tier>1 piece under a non-settlement assembly is refused.
    let root = tmp_dir("settle-tier-bad");
    let dir = root.join("settlecrash");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mod.toml"),
        "id = \"settlecrash\"\nworld_api = 2\ndepends = [\"base\"]\nretrogen = \"untouched_host_only\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pieces.toml"),
        r#"
[[piece]]
id = "ghost_tower"
settlement_tier = 2
connectors = [{ du = 0, dy = 0, dv = 0, kind = "path", facing = "west" }]
cells = [{ du = 0, dy = 0, dv = 0, block = "base:cobblestone" }]

[[pool]]
id = "orphan"
entries = [{ piece = "ghost_tower", weight = 1 }]

[[assembly]]
id = "wander_camp"
biomes = ["plains"]
rarity = 100
entry = "ghost_tower"
pools = { path = "orphan" }
max_depth = 1
max_pieces = 1
terrain = "none"
"#,
    )
    .unwrap();
    let invalid = registry::load(&root);
    assert!(
        invalid
            .material_errors
            .iter()
            .any(|e| e.contains("settlecrash:ghost_tower")
                && e.contains("no settlement assembly reaches it")),
        "tier>1 piece with no settlement assembly fails: {:?}",
        invalid.material_errors
    );

    // An assembly naming a settlement with no matching [[settlement]] fails.
    let root2 = tmp_dir("settle-ref-bad");
    let dir2 = root2.join("settleref");
    std::fs::create_dir_all(&dir2).unwrap();
    std::fs::write(
        dir2.join("mod.toml"),
        "id = \"settleref\"\nworld_api = 2\ndepends = [\"base\"]\nretrogen = \"untouched_host_only\"\n",
    )
    .unwrap();
    std::fs::write(
        dir2.join("pieces.toml"),
        r#"
[[assembly]]
id = "ghost_village"
biomes = ["plains"]
rarity = 100
entry = "base:watch_platform"
pools = {}
max_depth = 1
max_pieces = 1
terrain = "none"
settlement = "nowhere"
"#,
    )
    .unwrap();
    let invalid2 = registry::load(&root2);
    assert!(
        invalid2
            .material_errors
            .iter()
            .any(|e| e.contains("settleref:ghost_village") && e.contains("unknown settlement")),
        "assembly naming an unknown settlement fails: {:?}",
        invalid2.material_errors
    );

    // A tier that no [[settlement]] declares is refused.
    let root3 = tmp_dir("settle-tier-undef");
    let dir3 = root3.join("settletier");
    std::fs::create_dir_all(&dir3).unwrap();
    std::fs::write(
        dir3.join("mod.toml"),
        "id = \"settletier\"\nworld_api = 2\ndepends = [\"base\"]\nretrogen = \"untouched_host_only\"\n",
    )
    .unwrap();
    std::fs::write(
        dir3.join("pieces.toml"),
        r#"
[[settlement]]
id = "hollow"
tiers = [{ tier = 2, threshold = 2 }]

[[piece]]
id = "tier_four_spire"
settlement_tier = 4
connectors = [{ du = 0, dy = 0, dv = 0, kind = "path", facing = "west" }]
cells = [{ du = 0, dy = 0, dv = 0, block = "base:cobblestone" }]

[[pool]]
id = "spire"
entries = [{ piece = "tier_four_spire", weight = 1 }]

[[assembly]]
id = "hollow_hold"
biomes = ["plains"]
rarity = 100
entry = "tier_four_spire"
pools = { path = "spire" }
max_depth = 1
max_pieces = 1
terrain = "none"
settlement = "hollow"
"#,
    )
    .unwrap();
    let invalid3 = registry::load(&root3);
    assert!(
        invalid3
            .material_errors
            .iter()
            .any(|e| e.contains("settletier:tier_four_spire") && e.contains("tier 4 not declared")),
        "undeclared tier fails: {:?}",
        invalid3.material_errors
    );
}

#[test]
fn recipe_gate_fields_parse_and_learn_default_applies() {
    // The base Maker's Tablet recipe is blueprint-gated and is a
    // `learn_recipe` target, so it carries the default `learned:` tech key.
    let reg = base_reg();
    let tablet = reg
        .recipes
        .iter()
        .find(|r| r.output == it(&reg, "base:etched_tablet"))
        .expect("base etched_tablet recipe registers");
    assert_eq!(
        tablet.tech.as_deref(),
        Some("learned:base:etched_tablet"),
        "learn_recipe reward implies the default tech key"
    );
    let plate = it(&reg, "base:maker_calibration_plate");
    assert_eq!(tablet.blueprint, Some(plate), "blueprint gate resolved");

    // A plain recipe keeps no gate.
    let planks = reg
        .recipes
        .iter()
        .find(|r| r.output == it(&reg, "base:planks"))
        .expect("base planks recipe");
    assert_eq!(planks.tech, None, "ungated recipe has no tech key");
    assert_eq!(planks.blueprint, None, "ungated recipe has no blueprint");
}

#[test]
fn recipe_unknown_blueprint_fails_load_naming_the_recipe() {
    let root = tmp_dir("recipe-bad-blueprint");
    let dir = root.join("badbp");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mod.toml"),
        "id = \"badbp\"\nworld_api = 2\ndepends = [\"base\"]\nretrogen = \"untouched_host_only\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("recipes.toml"),
        r#"
[[recipe]]
pattern = ["ss"]
keys = { s = "base:stick" }
output = "base:planks"
count = 2
blueprint = "base:no_such_blueprint"
"#,
    )
    .unwrap();
    let invalid = registry::load(&root);
    assert!(
        invalid
            .material_errors
            .iter()
            .any(|e| e.contains("base:planks")
                && e.contains("blueprint")
                && e.contains("base:no_such_blueprint")),
        "unknown blueprint names the recipe and item: {:?}",
        invalid.material_errors
    );
}

#[test]
fn learn_recipe_reward_resolves_known_and_fails_unknown() {
    // Known recipe: the base elder_maker quest's reward resolves.
    let reg = base_reg();
    let quest = reg
        .quests
        .iter()
        .find(|q| q.id == "base:elder_maker")
        .expect("base elder_maker quest");
    assert!(
        quest
            .rewards
            .iter()
            .any(|r| matches!(r, crate::registry::QuestReward::LearnRecipe(id) if id == "base:etched_tablet")),
        "elder_maker unlocks the tablet recipe"
    );

    // Unknown recipe: the reward is dropped with a load error.
    let root = tmp_dir("recipe-bad-learn");
    let dir = root.join("badlearn");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mod.toml"),
        "id = \"badlearn\"\nworld_api = 2\ndepends = [\"base\"]\nretrogen = \"untouched_host_only\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("quests.toml"),
        r#"
[[quest]]
id = "mystery"
title = "Mystery"
description = "Learn a lost craft."
giver = "elder"
prereq = ""
objectives = [{ key = "proof", description = "Prove yourself", count = 1 }]
rewards = [
  { learn_recipe = "base:ghost_recipe" },
]
"#,
    )
    .unwrap();
    let invalid = registry::load(&root);
    assert!(
        invalid
            .material_errors
            .iter()
            .any(|e| e.contains("badlearn:mystery")
                && e.contains("unlocks unknown recipe")
                && e.contains("base:ghost_recipe")),
        "unknown learn_recipe reward names the quest and recipe: {:?}",
        invalid.material_errors
    );
}

#[test]
fn gated_recipe_material_balance_accounts_blueprint_input() {
    // The blueprint item's materials are extra input: a recipe whose pattern
    // alone balances but whose blueprint carries mass must fail validation.
    let root = tmp_dir("recipe-bad-balance");
    let dir = root.join("badbal");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mod.toml"),
        "id = \"badbal\"\nworld_api = 2\ndepends = [\"base\"]\nretrogen = \"untouched_host_only\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("items.toml"),
        r#"
[[item]]
id = "heavy_token"
name = "Heavy Token"
texture = "@stick"
materials = { iron = 1200 }
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("recipes.toml"),
        r#"
[[recipe]]
pattern = ["s"]
keys = { s = "base:iron_ingot" }
output = "base:iron_ingot"
blueprint = "heavy_token"
"#,
    )
    .unwrap();
    let invalid = registry::load(&root);
    assert!(
        invalid
            .material_errors
            .iter()
            .any(|e| e.contains("base:iron_ingot")
                && e.contains("not material-balanced")),
        "blueprint input must be accounted: {:?}",
        invalid.material_errors
    );
}

#[test]
fn duplicate_settlement_id_fails() {
    let root = tmp_dir("settle-dup");
    let dir = root.join("settledup");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mod.toml"),
        "id = \"settledup\"\nworld_api = 2\ndepends = [\"base\"]\nretrogen = \"untouched_host_only\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pieces.toml"),
        r#"
[[settlement]]
id = "twin"
tiers = [{ tier = 2, threshold = 2 }]

[[settlement]]
id = "twin"
tiers = [{ tier = 2, threshold = 2 }]
"#,
    )
    .unwrap();
    let invalid = registry::load(&root);
    assert!(
        invalid
            .material_errors
            .iter()
            .any(|e| e.contains("duplicate") && e.contains("settledup:twin")),
        "duplicate settlement id fails: {:?}",
        invalid.material_errors
    );
}

#[test]
fn rep_quest_reward_resolves_settlement_and_missing_target_fails() {
    // A valid rep reward resolves.
    let reg = base_reg();
    let quest = reg
        .quests
        .iter()
        .find(|q| q.id == "base:elder_honor")
        .expect("base rep quest registers");
    assert!(
        quest
            .rewards
            .iter()
            .any(|r| matches!(
                r,
                crate::registry::QuestReward::Reputation(settlement, 3)
                    if settlement == "base:elder_haven"
            )),
        "elder_honor grants 3 reputation to elder_haven"
    );

    // A quest rewarding a settlement that does not exist is refused.
    let root = tmp_dir("settle-quest-bad");
    let dir = root.join("settlequest");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mod.toml"),
        "id = \"settlequest\"\nworld_api = 2\ndepends = [\"base\"]\nretrogen = \"untouched_host_only\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("quests.toml"),
        r#"
[[quest]]
id = "wandering_favor"
title = "Favor"
description = "Help."
giver = "elder"
objectives = [{ key = "help", description = "Help", count = 1 }]
rewards = [{ add_reputation = "no_such_place", rep_amount = 1 }]
"#,
    )
    .unwrap();
    let invalid = registry::load(&root);
    assert!(
        invalid
            .material_errors
            .iter()
            .any(|e| e.contains("settlequest:wandering_favor")
                && e.contains("unknown settlement")),
        "rep reward for unknown settlement fails: {:?}",
        invalid.material_errors
    );
}

#[test]
fn script_reads_world_state() {
    let root = tmp_dir("scriptread");
    let mods = write_script_mod(
        &root,
        r#"
fn on_tick(dt) {
    let below = get_block("pos_z", 4096, 0, 4096);
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
    host.save_kv(&dir).unwrap();
    let host2 = crate::script::ScriptHost::new();
    host2.load_kv(&dir);
    assert_eq!(host2.kv.borrow()["m"]["k"], "v");

    host.kv.borrow_mut().clear();
    host.save_kv(&dir).unwrap();
    assert!(
        !dir.join("modstore.toml").exists(),
        "an empty store removes its stale replace-in-full file"
    );
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
        "id = \"cherry\"\nworld_api = 2\ndepends = [\"base\"]\n",
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
    std::fs::write(dir.join("mod.toml"), "id = \"t\"\nworld_api = 2\n").unwrap();
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
    assert_eq!(
        reg.block_id("copper:ore"),
        Some(b(&reg, "base:copper_ore")),
        "the content alias remains available to importers even though flat saves are refused"
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
    let all = crate::browser_items(&reg, "", false);
    assert!(all.iter().all(|i| !reg.item(*i).name.contains('/')));
    let q = crate::browser_items(&reg, "bronze", false);
    assert!(q.iter().any(|i| reg.item(*i).name == "base:bronze_pickaxe"));
    assert!(
        !crate::browser_items(&reg, "base:stick", false).is_empty(),
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
        reg.animals
            .iter()
            .filter(|a| !a.hostile && !a.vehicle && a.npc.is_none())
            .count(),
        38,
        "the full roster: wildlife, hunters, water, herds, the carcass"
    );
    assert!(
        reg.animals.iter().any(|a| a.npc.is_some()),
        "the NPC companion species is registered alongside wildlife"
    );
    assert_eq!(
        reg.animals.iter().filter(|a| a.hostile).count(),
        9,
        "nine wardens: the classic six plus the spec 3.6 archetypes"
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
    // Cube-sphere chart steps have modest, intentional metric distortion.
    // Pick a chart offset whose measured arc lies between the two aggro
    // radii instead of treating one chart step as one physical block.
    let pos = player + Vec3::new(11.5, 0.0, 0.0);
    let measured = ep(player).horizontal_distance_to(ep(pos));
    assert!(
        (10.0..12.0).contains(&measured),
        "fixture arc is {measured}"
    );
    let mut rng = 4u32;
    let mut a = crate::mobs::Mob::new(ti, pos, 0.0);
    a.health = def.health;
    a.tick(
        &w,
        &def,
        &[crate::server::PlayerCtx {
            id: 0,
            pos: ep(player),
            spawn: ep(Vec3::ZERO),
            attackable: true,
            aggro_mod: 0.0,
            quiet_charm: None,
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
            id: 0,
            pos: ep(player),
            spawn: ep(Vec3::ZERO),
            attackable: true,
            aggro_mod: -2.0,
            quiet_charm: None,
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
            w.ensure_chunk(tchunk(cx, cz));
            let c = &w.chunks()[&tchunk(cx, cz)];
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
            (
                "pos_z".to_string(),
                1i64,
                2i64,
                3i64,
                "meadow:sunstone_ore".to_string(),
            ),
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
        (
            "pos_z".to_string(),
            0i64,
            0i64,
            0i64,
            "base:dirt".to_string(),
        )
    ));
}

/// The whole content graph, audited: recipes well-formed, tables and
/// palettes resolve, and every survival item is actually obtainable
/// (dropped, harvested, looted, crafted, or smelted from things that
/// are). Catches half-wired content before a player does.
#[test]
fn content_graph_is_complete_and_obtainable() {
    use crate::registry::Ingredient;
    let reg = base_reg();
    for m in &reg.mods {
        assert!(m.error.is_none(), "mod {} load error: {:?}", m.id, m.error);
    }
    // The shared mod-lint harness owns the structural checks, biome sanity,
    // the obtainability closure, fuel, and breed-food reachability.
    assert!(
        crate::mod_lint::qualify_mods(Path::new("/nonexistent-mods-dir")).is_qualified(),
        "base content must pass the shared mod-lint harness"
    );
    let ok = crate::mod_lint::obtainable_items(&reg);
    let ing_ok = |ing: &Ingredient, ok: &std::collections::HashSet<u16>| match ing {
        Ingredient::One(i) => ok.contains(&i.0),
        Ingredient::Any(l) => l.iter().any(|i| ok.contains(&i.0)),
    };
    let lens = reg.item_id("base:tuning_lens").unwrap();
    let lens_recipe = reg
        .recipes
        .iter()
        .find(|recipe| recipe.output == lens)
        .expect("the public content graph exposes the tuning lens route");
    assert_eq!(lens_recipe.station.as_deref(), Some("lens_assembly_bench"));
    for ingredient in lens_recipe.pattern.iter().flatten() {
        assert!(
            ing_ok(ingredient, &ok),
            "a solo player cannot obtain a tuning-lens ingredient"
        );
        let candidates: Vec<_> = match ingredient {
            Ingredient::One(item) => vec![*item],
            Ingredient::Any(items) => items.clone(),
        };
        assert!(
            candidates.iter().any(|item| {
                reg.item(*item)
                    .discovery
                    .as_ref()
                    .and_then(|definition| definition.evidence_class.as_ref())
                    .is_none()
            }),
            "the industrial lens route must not depend on archaeological loot"
        );
    }
    // World-only block items and fuel/breed-food reachability are part of
    // the shared harness assert above; the lens route is base-specific.
}

#[test]
fn the_glass_cabinet_is_complete() {
    let reg = base_reg();
    // Eleven colors plus crystal, each fed by its mineral.
    for (powder, glass) in [
        ("base:verdigris_powder", "base:teal_glass"),
        ("base:ochre_powder", "base:amber_glass"),
        ("base:cobalt_powder", "base:blue_glass"),
        ("base:cinnabar_powder", "base:red_glass"),
        ("base:manganese_powder", "base:violet_glass"),
        ("base:chrome_powder", "base:green_glass"),
        ("base:raw_gold", "base:cranberry_glass"),
        ("base:silver_ingot", "base:yellow_glass"),
        ("base:tin_powder", "base:milk_glass"),
        ("base:rare_earth_powder", "base:rose_glass"),
        ("base:uranium_powder", "base:glow_glass"),
        ("base:lead_ingot", "base:crystal_glass"),
    ] {
        let p = it(&reg, powder);
        let gi = it(&reg, glass);
        assert!(
            reg.kiln
                .iter()
                .any(|recipe| recipe.powder == p && recipe.glass == gi),
            "the kiln knows {powder} -> {glass}"
        );
        assert!(reg.block(b(&reg, glass)).glass, "{glass} is glass");
    }
    // Glowglass emits; crystal is clear of tint filters.
    let glow = b(&reg, "base:glow_glass");
    assert!(reg.block(glow).light_emit >= 6, "glowglass glows");
    let crystal = b(&reg, "base:crystal_glass");
    assert_eq!(
        reg.block(crystal).light_filter,
        [true, true, true],
        "crystal filters nothing"
    );
}

#[test]
fn base_tiles_ship_inside_the_binary() {
    // Every PNG in base/textures is embedded at build time: a copied
    // exe must run from any working directory without the repo.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("base/textures");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "png") {
            let stem = p.file_stem().unwrap().to_string_lossy();
            assert!(
                crate::atlas::embedded_base_tile(&stem).is_some(),
                "{stem} embedded"
            );
            n += 1;
        }
    }
    assert!(n > 50, "the base art shipped ({n})");
}

/// Every name the UI can draw has to be drawable. The 5x7 font falls
/// through to a blank cell for anything it doesn't know, so a stray
/// apostrophe or comma is not an error anywhere — it is a hole in the
/// middle of a word, and it shipped that way in "PROSPECTOR'S PICK"
/// and "FEEDS: VEGETABLE, FRUIT" before this test existed.
#[test]
fn every_shipped_label_can_actually_be_drawn() {
    let reg = base_reg();
    let check = |kind: &str, name: &str, label: &str| {
        if let Some(bad) = label.chars().find(|&c| !crate::ui::has_glyph(c)) {
            panic!("{kind} {name}: label {label:?} contains {bad:?}, which draws as a blank");
        }
    };
    for b in &reg.blocks {
        check("block", &b.name, &b.label);
    }
    for i in &reg.items {
        check("item", &i.name, &i.label);
    }
    for a in &reg.animals {
        check("animal", &a.name, &a.label);
    }
}
