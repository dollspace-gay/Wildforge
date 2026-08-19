//! Mod content qualification: the battery of generic checks a pure data +
//! script mod must pass to be trusted in a world.
//!
//! This is capability E0 of the belt-quest port plan: it gives a mod repo
//! (with no Rust of its own) a single command to run in CI against
//! WildForge with the mod installed — `wildforge --mod-qualification
//! <mods_dir>`. The same checks back the `content_graph_is_complete_and_
//! obtainable` suite so base content and mod content share one harness.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::registry::{Ingredient, Registry};

/// One mod tree qualification. `failures` are hard gates (the mod must not
/// be installed into a world that claims them); `warnings` are advisories.
#[derive(Clone, Debug)]
pub struct ModLintReport {
    pub mods_dir: PathBuf,
    pub failures: Vec<String>,
    pub warnings: Vec<String>,
}

impl ModLintReport {
    pub fn is_qualified(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn render(&self) -> String {
        let mut out = format!("mods: {}\n", self.mods_dir.display());
        if self.failures.is_empty() {
            out.push_str("failures: none\n");
        } else {
            for f in &self.failures {
                out.push_str(&format!("failure: {f}\n"));
            }
        }
        if !self.warnings.is_empty() {
            for w in &self.warnings {
                out.push_str(&format!("warning: {w}\n"));
            }
        }
        out.push_str(if self.is_qualified() {
            "qualification status: PASS\n"
        } else {
            "qualification status: FAIL\n"
        });
        out
    }
}

/// Load the registry from `mods_dir` and run every generic content check a
/// data + script mod must pass. `base` is always present, so a mod that
/// depends on base validates against the complete shared content graph.
pub fn qualify_mods(mods_dir: &Path) -> ModLintReport {
    let reg = crate::registry::load(mods_dir);
    let mut report = ModLintReport {
        mods_dir: mods_dir.to_path_buf(),
        failures: Vec::new(),
        warnings: Vec::new(),
    };
    for m in &reg.mods {
        if let Some(error) = &m.error {
            report
                .failures
                .push(format!("mod {}: {error}", m.id));
        }
    }
    report.failures.extend(reg.material_errors.iter().cloned());
    report.failures.extend(reg.arcane_errors.iter().cloned());
    report.failures.extend(missing_textures(&reg));
    report.failures.extend(content_errors(&reg));
    report.failures.extend(script_errors(&reg));
    report
}

/// Blocks and items that resolved to the missing-texture tile.
fn missing_textures(reg: &Registry) -> Vec<String> {
    let unknown = crate::atlas::UNKNOWN_SLOT;
    let mut out = Vec::new();
    for b in &reg.blocks {
        if b.name != "base:unknown" && b.tiles.contains(&unknown) {
            out.push(format!("block {} has no real texture", b.name));
        }
    }
    for i in &reg.items {
        if i.icon == unknown {
            out.push(format!("item {} has no real texture", i.name));
        }
    }
    out
}

/// Script compile errors across every mod that ships a `main.rhai`.
fn script_errors(reg: &Registry) -> Vec<String> {
    let mut host = crate::script::ScriptHost::new();
    let dirs: Vec<(String, PathBuf)> = reg
        .mods
        .iter()
        .filter(|m| m.has_script && m.error.is_none())
        .filter_map(|m| m.path.clone().map(|p| (m.id.clone(), p)))
        .collect();
    host.load_mods(&dirs);
    host.mods
        .iter()
        .filter_map(|m| m.error.clone())
        .collect()
}

/// The survival content graph: the set of item ids a solo player can reach
/// from world sources (blocks, drops, loot, code paths) by closing over
/// crafting, smelting, the steelworks, implements, alchemy, kilns,
/// separation, and the few dedicated code-path converters.
pub fn obtainable_items(reg: &Registry) -> HashSet<u16> {
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
            if r.pattern.iter().flatten().all(|i| ing_ok(i, &ok)) {
                grew |= ok.insert(r.output.0);
                for (byproduct, count) in &r.byproducts {
                    if *count > 0 {
                        grew |= ok.insert(byproduct.0);
                    }
                }
            }
        }
        for s in &reg.smelts {
            if ing_ok(&s.input, &ok) {
                grew |= ok.insert(s.output.0);
                if let Some((spit, count)) = s.spit
                    && count > 0
                {
                    grew |= ok.insert(spit.0);
                }
            }
        }
        // The steelworks: a fired bloomery turns charge into blooms,
        // and the anvil works blooms into bars (proven by sim tests).
        for b in &reg.bloomery {
            if ok.contains(&b.charge.0) && ok.contains(&b.fuel.0) {
                grew |= ok.insert(b.bloom.0);
                if reg.item(b.charge).materials.contains_key("iron")
                    && let Some(slag) = reg.item_id("base:iron_slag")
                {
                    grew |= ok.insert(slag.0);
                }
            }
        }
        for w in &reg.worked {
            if !ok.contains(&w.output.0) && ok.contains(&w.input.0) {
                ok.insert(w.output.0);
                grew = true;
            }
        }
        // Durability breakage, clean machine dismantling, and forge salvage
        // are runtime transformations rather than ordinary recipes. They are
        // still edges in the survival content graph.
        for item in &reg.items {
            if let Some(damaged) = item.broken_into
                && reg.item_id(&item.name).is_some_and(|id| ok.contains(&id.0))
            {
                grew |= ok.insert(damaged.0);
            }
        }
        for block in &reg.blocks {
            if let Some(bundle) = block.dismantles_to
                && reg
                    .item_id(&block.name)
                    .is_some_and(|item| ok.contains(&item.0))
            {
                grew |= ok.insert(bundle.0);
            }
        }
        for salvage in &reg.forge_salvage {
            if ok.contains(&salvage.input.0) {
                grew |= ok.insert(salvage.output.0);
                grew |= ok.insert(salvage.byproduct.0);
            }
        }
        // Implements are made and failed through the embodied binding-frame
        // lifecycle rather than synthetic grid recipes. Model those runtime
        // edges only when the complete apparatus and at least one obtainable
        // component for every physical role are present.
        let frame_ready = [
            "base:binding_frame",
            "base:focus_mount",
            "base:arcane_conductor",
            "base:charge_vessel",
            "base:containment_post",
        ]
        .into_iter()
        .all(|name| reg.item_id(name).is_some_and(|item| ok.contains(&item.0)));
        let component_roster_ready =
            crate::implements::ComponentRole::ALL.into_iter().all(|role| {
                reg.items.iter().enumerate().any(|(index, item)| {
                    ok.contains(&(index as u16))
                        && item
                            .wand_component
                            .as_ref()
                            .is_some_and(|component| component.role == role)
                })
            });
        if frame_ready
            && component_roster_ready
            && let Some(wand) = reg.item_id("base:bound_wand")
        {
            grew |= ok.insert(wand.0);
        }
        // Heat/strain failure conserves an implement as one stable fragment
        // bundle. This is another runtime transformation, not a recipe.
        let failable_implement = ["base:bound_wand", "base:charge_vessel"]
            .into_iter()
            .any(|name| reg.item_id(name).is_some_and(|item| ok.contains(&item.0)));
        if failable_implement
            && let Some(fragments) = reg.item_id("base:implement_fragment")
        {
            grew |= ok.insert(fragments.0);
        }
        // Apothecary carriers and preparations are embodied station
        // lifecycles, not crafting-grid recipes. Close those runtime edges
        // only when every ordinary input and the relevant laboratory blocks
        // are already obtainable.
        let has = |name: &str, ok: &HashSet<u16>| {
            reg.item_id(name).is_some_and(|item| ok.contains(&item.0))
        };
        if [
            "base:infusion_basin",
            "base:bucket_water",
            "base:wheat",
            "base:berry",
        ]
        .into_iter()
        .all(|name| has(name, &ok))
            && let Some(output) = reg.item_id("base:fermented_alcohol")
        {
            grew |= ok.insert(output.0);
        }
        if ["base:alchemy_mortar", "base:wheat_seeds"]
            .into_iter()
            .all(|name| has(name, &ok))
            && let Some(output) = reg.item_id("base:plant_oil")
        {
            grew |= ok.insert(output.0);
        }
        let laboratory_ready = [
            "base:alchemy_mortar",
            "base:infusion_basin",
            "base:alembic",
            "base:filter_stand",
            "base:arcane_conductor",
            "base:filter_cloth",
        ]
        .into_iter()
        .all(|name| has(name, &ok));
        if laboratory_ready {
            for preparation in reg.preparations.values() {
                let inputs_ready = has(&preparation.solvent_item, &ok)
                    && has(&preparation.empty_vessel, &ok)
                    && preparation
                        .ingredients
                        .iter()
                        .all(|ingredient| has(&ingredient.item, &ok));
                if !inputs_ready {
                    continue;
                }
                for name in [&preparation.output_item, &preparation.residue_item] {
                    if let Some(item) = reg.item_id(name) {
                        grew |= ok.insert(item.0);
                    }
                }
                for failure in crate::alchemy::BatchFailure::ALL {
                    if let Some(item) = reg.item_id(failure.item_id()) {
                        grew |= ok.insert(item.0);
                    }
                }
                if preparation
                    .steps
                    .contains(&crate::alchemy::ProcessStep::Filter)
                    && let Some(item) = reg.item_id("base:spent_filter")
                {
                    grew |= ok.insert(item.0);
                }
                if preparation.handler == crate::alchemy::PreparationHandler::DrossWash
                    && let Some(item) = reg.item_id("base:dross_sludge")
                {
                    grew |= ok.insert(item.0);
                }
                if preparation.handler == crate::alchemy::PreparationHandler::PreserveSpecimen
                    && let Some(item) = reg.item_id("base:spent_carrier")
                {
                    grew |= ok.insert(item.0);
                }
            }
        }
        // The bucket: dip it in any fluid and it comes up full — a
        // code path, like shears. Water and lava alike.
        for full_name in [
            "base:bucket_water",
            "base:bucket_brackish",
            "base:bucket_salt",
            "base:bucket_lava",
        ] {
            if let (Some(b), Some(f)) = (reg.item_id("base:bucket"), reg.item_id(full_name))
                && ok.contains(&b.0)
                && !ok.contains(&f.0)
            {
                ok.insert(f.0);
                grew = true;
            }
        }
        // Smoked meat: raw cuts cure on a rack over a torch - a code
        // path (tick_smokers), like the freshness sweep below.
        if let Some(sm) = reg.item_id("base:smoked_meat")
            && !ok.contains(&sm.0)
            && reg
                .tags
                .get("base:raw_meats")
                .is_some_and(|t| t.iter().any(|i| ok.contains(&i.0)))
        {
            ok.insert(sm.0);
            grew = true;
        }
        // Spoiled mush: any perishable food left too long becomes it -
        // a code path (the freshness sweep), like the bucket dip.
        if let Some(m) = reg.item_id("base:spoiled_mush")
            && !ok.contains(&m.0)
            && reg
                .items
                .iter()
                .enumerate()
                .any(|(i, d)| d.food.is_some() && d.durability > 0 && ok.contains(&(i as u16)))
        {
            ok.insert(m.0);
            grew = true;
        }
        // Dung: any grazing species digests its meals into it - a
        // code path (MobEvent::Dung), like the smoker. Guano is the
        // bats' variety, gathered under a roost.
        if let Some(d) = reg.item_id("base:dung")
            && !ok.contains(&d.0)
            && reg.animals.iter().any(|a| a.grazes && a.belly_secs > 0.0)
        {
            ok.insert(d.0);
            grew = true;
        }
        if let Some(g) = reg.item_id("base:guano")
            && !ok.contains(&g.0)
            && reg
                .animals
                .iter()
                .any(|a| a.name.ends_with(":bat") && a.belly_secs > 0.0)
        {
            ok.insert(g.0);
            grew = true;
        }
        // Compost: a heap of greens cooks down - a code path
        // (compost_fill/random tick), fed by anything compostable.
        if let (Some(c), Some(_)) = (
            reg.item_id("base:compost"),
            reg.block_id("base:compost_heap"),
        ) && !ok.contains(&c.0)
            && reg.items.iter().enumerate().any(|(i, d)| {
                crate::world::soil::compost_value(&d.name) > 0 && ok.contains(&(i as u16))
            })
        {
            ok.insert(c.0);
            grew = true;
        }
        // A quickened seed: a living heart gives one to a bare hand -
        // a code path (the heart interaction), like the bucket dip.
        // Every country has a heart, and every heart gives its own.
        for name in (1..=12u8)
            .filter_map(crate::worldgen::Biome::from_index)
            .map(|b| crate::world::seed_of_form(crate::world::heart_form(b)))
        {
            if let Some(s) = reg.item_id(name)
                && !ok.contains(&s.0)
            {
                ok.insert(s.0);
                grew = true;
            }
        }
        // The separator: mixed rare-earth powder splits into
        // neodymium and cerium on a firebrick stack - a code path
        // (tick_separators), like the smoker.
        if let Some(p) = reg.item_id("base:rare_earth_powder")
            && ok.contains(&p.0)
        {
            for name in ["base:neodymium", "base:cerium"] {
                if let Some(i) = reg.item_id(name)
                    && !ok.contains(&i.0)
                {
                    ok.insert(i.0);
                    grew = true;
                }
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
            for recipe in &reg.kiln {
                if !ok.contains(&recipe.glass.0) && ok.contains(&recipe.powder.0) {
                    ok.insert(recipe.glass.0);
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }
    ok
}

/// Structural and reachability problems in the registry. `world_only` block
/// items (the silk-touch category) and creative-only placers are exempt.
fn content_errors(reg: &Registry) -> Vec<String> {
    let mut out = Vec::new();

    // Structure of every recipe / table / template.
    for r in &reg.recipes {
        let out_item = &reg.item(r.output).name;
        if r.pattern.len() != r.w * r.h {
            out.push(format!("recipe for {out_item} malformed"));
        }
        if r.count == 0 || r.w > 3 || r.h > 3 {
            out.push(format!("recipe for {out_item} malformed"));
        }
        if !r.pattern.iter().any(|p| p.is_some()) {
            out.push(format!("recipe for {out_item} is empty"));
        }
    }
    for (name, entries) in &reg.loots {
        if entries.is_empty() {
            out.push(format!("loot table {name} is empty"));
        }
        for e in entries {
            if e.weight == 0 || e.count.0 > e.count.1 {
                out.push(format!("loot table {name} entry malformed"));
            }
        }
    }
    for st in &reg.structures {
        if let Some(l) = &st.loot
            && !reg.loots.contains_key(l)
        {
            out.push(format!("structure {} wants missing loot table {l}", st.name));
        }
        for layer in &st.layers {
            for row in layer {
                for ch in row.chars() {
                    if !matches!(ch, '.' | '~' | 'C') && !st.palette.contains_key(&ch) {
                        out.push(format!(
                            "structure {} uses unmapped char '{ch}'",
                            st.name
                        ));
                    }
                }
            }
        }
    }
    for b in &reg.blocks {
        if let Some((table, _)) = &b.brush
            && !reg.loots.contains_key(table)
        {
            out.push(format!("{} brushes into missing table {table}", b.name));
        }
    }

    // Biome sanity: a wanderer's habitat must name a real province culture.
    let biomes = [
        "forest",
        "plains",
        "desert",
        "jungle",
        "scrubland",
        "taiga",
        "arctic",
        "mountains",
        "tundra",
        "savanna",
        "badlands",
        "swamp",
        "underground",
        "ocean",
    ];
    for a in &reg.animals {
        if !a.hostile
            && !a.vehicle
            && a.npc.is_none()
            && a.rarity != u32::MAX
            && (a.biomes.is_empty() || !a.biomes.iter().all(|b| biomes.contains(&b.as_str())))
        {
            out.push(format!("animal {} has invalid biomes {:?}", a.name, a.biomes));
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
    let ok = obtainable_items(reg);
    for (index, d) in reg.items.iter().enumerate() {
        if !d.creative_only && !ok.contains(&(index as u16)) && !world_only(&d.name) {
            out.push(format!("unobtainable in survival: {}", d.name));
        }
    }

    // The furnace can actually run, and husbandry foods exist.
    if !reg
        .fuels
        .iter()
        .any(|(f, burn, _)| *burn > 0.0 && ing_ok(f, &ok))
    {
        out.push("no obtainable fuel".into());
    }
    for a in &reg.animals {
        if let Some(bf) = a.breed_food
            && !ok.contains(&bf.0)
        {
            out.push(format!("breed food for {} unobtainable", a.name));
        }
    }
    out
}

fn ing_ok(ing: &Ingredient, ok: &HashSet<u16>) -> bool {
    match ing {
        Ingredient::One(i) => ok.contains(&i.0),
        Ingredient::Any(l) => l.iter().any(|i| ok.contains(&i.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A unique, per-test temp tree so parallel test runs never share
    /// registry state.
    fn mods_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "wildforge-mod-lint-{label}-{}",
            std::process::id()
        ))
    }

    fn write_mod(label: &str, id: &str, files: &[(&str, &str)]) {
        let dir = mods_dir(label).join(id);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, body) in files {
            std::fs::write(dir.join(name), body).unwrap();
        }
    }

    fn clean(label: &str) {
        let _ = std::fs::remove_dir_all(mods_dir(label));
    }

    fn base_mod_toml(id: &str) -> String {
        format!(
            "id = \"{id}\"\nname = \"{id}\"\nversion = \"1.0.0\"\nworld_api = 2\ndepends = [\"base\"]\n"
        )
    }

    #[test]
    fn base_content_passes_qualification() {
        let report = qualify_mods(Path::new("/nonexistent-mods-dir"));
        assert!(report.is_qualified(), "{}", report.render());
    }

    #[test]
    fn a_missing_texture_fails_qualification() {
        clean("missing-texture");
        write_mod(
            "missing-texture",
            "ghosty",
            &[
                ("mod.toml", &base_mod_toml("ghosty")),
                (
                    "items.toml",
                    r#"
[[item]]
id = "ghost"
name = "Ghost"
texture = "no_such_texture.png"
"#,
                ),
            ],
        );
        let report = qualify_mods(&mods_dir("missing-texture"));
        assert!(!report.is_qualified());
        assert!(
            report
                .render()
                .contains("missing texture no_such_texture.png"),
            "{}",
            report.render()
        );
        clean("missing-texture");
    }

    #[test]
    fn an_unobtainable_item_fails_qualification() {
        clean("dreamer");
        write_mod(
            "dreamer",
            "dreamer",
            &[
                ("mod.toml", &base_mod_toml("dreamer")),
                (
                    "items.toml",
                    r#"
[[item]]
id = "dream_sword"
name = "Dream Sword"
texture = "@stick"
"#,
                ),
            ],
        );
        let report = qualify_mods(&mods_dir("dreamer"));
        assert!(!report.is_qualified());
        assert!(
            report.render().contains("unobtainable in survival"),
            "{}",
            report.render()
        );
        clean("dreamer");
    }

    #[test]
    fn a_broken_script_fails_qualification() {
        clean("scripter");
        write_mod(
            "scripter",
            "scripter",
            &[
                ("mod.toml", &base_mod_toml("scripter")),
                (
                    "main.rhai",
                    "fn on_tick(dt) { this will not parse !!! }\n",
                ),
            ],
        );
        let report = qualify_mods(&mods_dir("scripter"));
        assert!(!report.is_qualified());
        assert!(
            report.render().contains("scripter/main.rhai"),
            "{}",
            report.render()
        );
        clean("scripter");
    }

    #[test]
    fn obtainable_items_closes_over_a_simple_mod_recipe() {
        clean("crafty");
        write_mod(
            "crafty",
            "crafty",
            &[
                ("mod.toml", &base_mod_toml("crafty")),
                (
                    "items.toml",
                    r#"
[[item]]
id = "diamond"
name = "Diamond"
texture = "@ice"

[[item]]
id = "diamond_pick"
name = "Diamond Pick"
texture = "@stick"
"#,
                ),
                (
                    "blocks.toml",
                    r#"
[[block]]
id = "diamond_ore"
name = "Diamond Ore"
texture = "@stone"
hardness = 5.0
tool = "pickaxe"
requires_tool = true
drops = "crafty:diamond"
"#,
                ),
                (
                    "recipes.toml",
                    r#"
[[recipe]]
pattern = ["ddd", " s ", " s "]
keys = { d = "crafty:diamond", s = "base:stick" }
output = "crafty:diamond_pick"
"#,
                ),
            ],
        );
        let reg = crate::registry::load(&mods_dir("crafty"));
        let ok = obtainable_items(&reg);
        let diamond = reg.item_id("crafty:diamond").expect("diamond");
        let pick = reg.item_id("crafty:diamond_pick").expect("pick");
        assert!(ok.contains(&diamond.0), "ore drops the diamond");
        assert!(ok.contains(&pick.0), "diamonds craft the pick");
        let report = qualify_mods(&mods_dir("crafty"));
        assert!(report.is_qualified(), "{}", report.render());
        clean("crafty");
    }

    #[test]
    fn an_invalid_frame_fails_qualification() {
        clean("frame-smith");
        write_mod(
            "frame-smith",
            "frame_smith",
            &[
                ("mod.toml", &base_mod_toml("frame_smith")),
                (
                    "items.toml",
                    r#"
[[item]]
id = "broken_frame"
name = "Broken Frame"
texture = "@leather_chestplate"
armor = { slot = "chest", points = 1 }
frame = { slots = [ { type = "gem", max = 0 } ] }
"#,
                ),
            ],
        );
        let report = qualify_mods(&mods_dir("frame-smith"));
        assert!(!report.is_qualified());
        assert!(
            report.render().contains("max 0"),
            "{}",
            report.render()
        );
        clean("frame-smith");
    }

    #[test]
    fn a_component_with_a_recipe_passes_qualification() {
        clean("cutgem");
        write_mod(
            "cutgem",
            "cutgem",
            &[
                ("mod.toml", &base_mod_toml("cutgem")),
                (
                    "items.toml",
                    r#"
[[item]]
id = "cut_ruby"
name = "Cut Ruby"
texture = "@cinnabar_powder"
component = "gem"
[[item.stats]]
kind = "health"
flat = 2
"#,
                ),
                (
                    "recipes.toml",
                    r#"
[[recipe]]
pattern = ["r"]
keys = { r = "base:stick" }
output = "cutgem:cut_ruby"
"#,
                ),
            ],
        );
        let report = qualify_mods(&mods_dir("cutgem"));
        assert!(report.is_qualified(), "{}", report.render());
        clean("cutgem");
    }
}
