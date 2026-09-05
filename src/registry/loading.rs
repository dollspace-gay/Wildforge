//! Parse mod files, order providers, and build a fresh registry without publishing it.

use super::{ModInfo, Registry, RetrogenPolicy, WORLD_API_VERSION};
use super::linking::build;
use super::schema::{AliasesFile, AnimalsFile, ArcaneFile, BlocksFile, DialogueFile, FeaturesFile, ItemsFile, ModToml, ModesFile, NestFileToml, NpcsFile, PiecesFile, QuestsFile, RawMod, RecipesFile, StructuresFile, TagsFile};
use std::path::Path;

const BASE_BLOCKS: &str = include_str!("../../base/blocks.toml");
const BASE_ITEMS: &str = include_str!("../../base/items.toml");
const BASE_RECIPES: &str = include_str!("../../base/recipes.toml");
const BASE_TAGS: &str = include_str!("../../base/tags.toml");
const BASE_FEATURES: &str = include_str!("../../base/features.toml");
const BASE_ALIASES: &str = include_str!("../../base/aliases.toml");
const BASE_ANIMALS: &str = include_str!("../../base/animals.toml");
const BASE_NPCS: &str = include_str!("../../base/npcs.toml");
const BASE_DIALOGUE: &str = include_str!("../../base/dialogue.toml");
const BASE_QUESTS: &str = include_str!("../../base/quests.toml");
const BASE_STRUCTURES: &str = include_str!("../../base/structures.toml");
const BASE_PIECES: &str = include_str!("../../base/pieces.toml");
const BASE_WORKINGS: &str = include_str!("../../base/workings.toml");
const BASE_PREPARATIONS: &str = include_str!("../../base/preparations.toml");
const BASE_MACHINES: &str = include_str!("../../base/machines.toml");

fn parse_mod_dir(dir: &Path) -> Result<RawMod, String> {
    let manifest =
        std::fs::read_to_string(dir.join("mod.toml")).map_err(|e| format!("mod.toml: {e}"))?;
    let m: ModToml = toml::from_str(&manifest).map_err(|e| format!("mod.toml: {e}"))?;
    if m.world_api != Some(WORLD_API_VERSION) {
        let found = m
            .world_api
            .map_or_else(|| "missing".to_string(), |version| version.to_string());
        return Err(format!(
            "mod.toml: world_api is {found}; this build requires world_api = \
             {WORLD_API_VERSION} (planet positions use face/u/y/v)"
        ));
    }
    let read = |name: &str| super::reading::optional(dir, name).map(Option::unwrap_or_default);
    let blocks: BlocksFile =
        toml::from_str(&read("blocks.toml")?).map_err(|e| format!("blocks.toml: {e}"))?;
    let items: ItemsFile =
        toml::from_str(&read("items.toml")?).map_err(|e| format!("items.toml: {e}"))?;
    let recipes: RecipesFile =
        toml::from_str(&read("recipes.toml")?).map_err(|e| format!("recipes.toml: {e}"))?;
    let features: FeaturesFile =
        toml::from_str(&read("features.toml")?).map_err(|e| format!("features.toml: {e}"))?;
    let tags: TagsFile =
        toml::from_str(&read("tags.toml")?).map_err(|e| format!("tags.toml: {e}"))?;
    let aliases: AliasesFile =
        toml::from_str(&read("aliases.toml")?).map_err(|e| format!("aliases.toml: {e}"))?;
    let animals: AnimalsFile =
        toml::from_str(&read("animals.toml")?).map_err(|e| format!("animals.toml: {e}"))?;
    let npcs: NpcsFile =
        toml::from_str(&read("npcs.toml")?).map_err(|e| format!("npcs.toml: {e}"))?;
    let dialogue: DialogueFile =
        toml::from_str(&read("dialogue.toml")?).map_err(|e| format!("dialogue.toml: {e}"))?;
    let quests: QuestsFile =
        toml::from_str(&read("quests.toml")?).map_err(|e| format!("quests.toml: {e}"))?;
    let structures: StructuresFile =
        toml::from_str(&read("structures.toml")?).map_err(|e| format!("structures.toml: {e}"))?;
    let pieces: PiecesFile =
        toml::from_str(&read("pieces.toml")?).map_err(|e| format!("pieces.toml: {e}"))?;
    let arcane: ArcaneFile =
        toml::from_str(&read("arcane.toml")?).map_err(|e| format!("arcane.toml: {e}"))?;
    if arcane.schema_version.is_some_and(|version| version != 1) {
        return Err("arcane.toml: schema_version must be 1".into());
    }
    let workings: crate::workings::WorkingsFile =
        toml::from_str(&read("workings.toml")?).map_err(|e| format!("workings.toml: {e}"))?;
    if workings
        .schema_version
        .is_some_and(|version| version != crate::workings::WORKINGS_SCHEMA_VERSION)
    {
        return Err(format!(
            "workings.toml: schema_version must be {}",
            crate::workings::WORKINGS_SCHEMA_VERSION
        ));
    }
    let preparations: crate::alchemy::PreparationsFile = toml::from_str(&read("preparations.toml")?)
        .map_err(|e| format!("preparations.toml: {e}"))?;
    if preparations
        .schema_version
        .is_some_and(|version| version != crate::alchemy::PREPARATIONS_SCHEMA_VERSION)
    {
        return Err(format!(
            "preparations.toml: schema_version must be {}",
            crate::alchemy::PREPARATIONS_SCHEMA_VERSION
        ));
    }
    let modes: ModesFile =
        toml::from_str(&read("modes.toml")?).map_err(|e| format!("modes.toml: {e}"))?;
    let skills = if let Some(text) = super::reading::optional(dir, "skills.toml")? {
        Some(crate::skills::parse_skills(&text, &m.id)?)
    } else {
        None
    };
    let machines = if let Some(text) = super::reading::optional(dir, "machines.toml")? {
        let parsed: crate::machines::RawMachineToml = toml::from_str(&text)
            .map_err(|error| format!("machines.toml: {error}"))?;
        if parsed
            .schema_version
            .is_some_and(|version| version != crate::machines::MACHINES_SCHEMA_VERSION)
        {
            return Err(format!(
                "machines.toml: schema_version must be {}",
                crate::machines::MACHINES_SCHEMA_VERSION
            ));
        }
        Some(parsed)
    } else {
        None
    };
    let nests = if let Some(text) = super::reading::optional(dir, "nests.toml")? {
        Some(
            toml::from_str::<NestFileToml>(&text)
                .map_err(|error| format!("nests.toml: {error}"))?,
        )
    } else {
        None
    };
    let screens = if let Some(text) = super::reading::optional(dir, "screens.toml")? {
        let parsed: crate::screens::RawScreensToml = toml::from_str(&text)
            .map_err(|error| format!("screens.toml: {error}"))?;
        if parsed
            .schema_version
            .is_some_and(|version| version != crate::screens::SCREENS_SCHEMA_VERSION)
        {
            return Err(format!(
                "screens.toml: schema_version must be {}",
                crate::screens::SCREENS_SCHEMA_VERSION
            ));
        }
        Some(parsed)
    } else {
        None
    };
    if !features.feature.is_empty() && m.retrogen.is_none() {
        return Err(
            "mod.toml: a worldgen feature requires retrogen = \"untouched_host_only\", \
             \"secondary_recovery\", \"world_event\", or \"no_retrogen\""
                .into(),
        );
    }
    let has_script = dir.join("main.rhai").try_exists().map_err(|error| format!("main.rhai: {error}"))?;
    Ok(RawMod {
        info: ModInfo {
            id: m.id.clone(),
            name: m.name.unwrap_or(m.id),
            version: m.version.unwrap_or_else(|| "0.0.0".into()),
            path: Some(dir.to_path_buf()),
            has_script,
            retrogen: m.retrogen,
            error: None,
        },
        depends: m.depends,
        blocks: blocks.block,
        items: items.item,
        smelts: recipes.smelt.clone(),
        fuels: recipes.fuel.clone(),
        bloomeries: recipes.bloomery.clone(),
        workeds: recipes.worked.clone(),
        kilns: recipes.kiln.clone(),
        kiln_bases: recipes.kiln_base.clone().into_iter().collect(),
        recipes: recipes.recipe,
        features: features.feature,
        tags: tags.tag,
        aliases: aliases.alias,
        animals: animals.animal,
        npcs: npcs.npc,
        dialogues: dialogue.dialogue,
        quests: quests.quest,
        structures: structures.structure,
        loots: structures.loot,
        pieces: pieces.piece,
        pools: pieces.pool,
        assemblies: pieces.assembly,
        settlements: pieces.settlement,
        resonances: arcane.resonance,
        arcane_sites: arcane.sites,
        workings: workings.working,
        preparations: preparations.preparation,
        modes: modes.mode,
        skills,
        machines,
        nests,
        screens,
    })
}

fn base_mod() -> RawMod {
    let blocks: BlocksFile = toml::from_str(BASE_BLOCKS).expect("base blocks.toml");
    let items: ItemsFile = toml::from_str(BASE_ITEMS).expect("base items.toml");
    let recipes: RecipesFile = toml::from_str(BASE_RECIPES).expect("base recipes.toml");
    let tags: TagsFile = toml::from_str(BASE_TAGS).expect("base tags.toml");
    let features: FeaturesFile = toml::from_str(BASE_FEATURES).expect("base features.toml");
    let aliases: AliasesFile = toml::from_str(BASE_ALIASES).expect("base aliases.toml");
    let animals: AnimalsFile = toml::from_str(BASE_ANIMALS).expect("base animals.toml");
    let npcs: NpcsFile = toml::from_str(BASE_NPCS).expect("base npcs.toml");
    let dialogue: DialogueFile = toml::from_str(BASE_DIALOGUE).expect("base dialogue.toml");
    let quests: QuestsFile = toml::from_str(BASE_QUESTS).expect("base quests.toml");
    let structures: StructuresFile = toml::from_str(BASE_STRUCTURES).expect("base structures.toml");
    let pieces: PiecesFile = toml::from_str(BASE_PIECES).expect("base pieces.toml");
    let workings: crate::workings::WorkingsFile =
        toml::from_str(BASE_WORKINGS).expect("base workings.toml");
    let preparations: crate::alchemy::PreparationsFile =
        toml::from_str(BASE_PREPARATIONS).expect("base preparations.toml");
    let machines: crate::machines::RawMachineToml =
        toml::from_str(BASE_MACHINES).expect("base machines.toml");
    RawMod {
        info: ModInfo {
            id: "base".into(),
            name: "Wildforge".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            // The TOML is embedded, but PNG tiles resolve from the
            // repo's base/ directory like any mod's (the game runs
            // from the repo root; the README says as much).
            path: Some(std::path::PathBuf::from("base")),
            has_script: false,
            retrogen: Some(RetrogenPolicy::UntouchedHostOnly),
            error: None,
        },
        depends: vec![],
        blocks: blocks.block,
        items: items.item,
        smelts: recipes.smelt.clone(),
        fuels: recipes.fuel.clone(),
        bloomeries: recipes.bloomery.clone(),
        workeds: recipes.worked.clone(),
        kilns: recipes.kiln.clone(),
        kiln_bases: recipes.kiln_base.clone().into_iter().collect(),
        recipes: recipes.recipe,
        features: features.feature,
        tags: tags.tag,
        aliases: aliases.alias,
        animals: animals.animal,
        npcs: npcs.npc,
        dialogues: dialogue.dialogue,
        quests: quests.quest,
        structures: structures.structure,
        loots: structures.loot,
        pieces: pieces.piece,
        pools: pieces.pool,
        assemblies: pieces.assembly,
        settlements: pieces.settlement,
        resonances: Vec::new(),
        arcane_sites: Vec::new(),
        workings: workings.working,
        preparations: preparations.preparation,
        modes: Vec::new(),
        skills: None,
        machines: Some(machines),
        nests: None,
        screens: None,
    }
}

/// Load base + all mods under `mods_dir` into a fresh registry.
/// Individual bad mods are skipped with their error recorded. Runtime publication
/// must call `Registry::validate` or use `load_validated` instead.
pub fn load(mods_dir: &Path) -> Registry {
    let mut raws = vec![base_mod()];
    let mut failed: Vec<ModInfo> = Vec::new();
    match super::reading::mod_dirs(mods_dir) {
        Ok(dirs) => {
        for dir in dirs {
            match parse_mod_dir(&dir) {
                Ok(r) => raws.push(r),
                Err(e) => failed.push(ModInfo {
                    id: dir.file_name().unwrap_or_default().to_string_lossy().into(),
                    name: String::new(),
                    version: String::new(),
                    path: Some(dir),
                    has_script: false,
                    retrogen: None,
                    error: Some(e),
                }),
            }
        }
    }
        Err(error) => failed.push(ModInfo {
            id: "mods".into(),
            name: "Mod directory".into(),
            version: String::new(),
            path: Some(mods_dir.to_path_buf()),
            has_script: false,
            retrogen: None,
            error: Some(format!("could not read mod directory: {error}")),
        }),
    }

    // Topological order by depends (base first; unknown deps = load error).
    let ids: Vec<String> = raws.iter().map(|r| r.info.id.clone()).collect();
    let mut order: Vec<usize> = Vec::new();
    let mut placed = vec![false; raws.len()];
    for _ in 0..raws.len() {
        let mut progressed = false;
        for i in 0..raws.len() {
            if placed[i] {
                continue;
            }
            let ok = raws[i].depends.iter().all(|d| {
                ids.iter().enumerate().any(|(j, id)| id == d && placed[j]) || d == &raws[i].info.id
            });
            if ok {
                placed[i] = true;
                order.push(i);
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    for i in 0..raws.len() {
        if !placed[i] {
            let mut info = raws[i].info.clone();
            info.error = Some(format!(
                "unresolved or cyclic dependencies: {:?}",
                raws[i].depends
            ));
            failed.push(info);
        }
    }

    let mut registry = build(
        order.into_iter().map(|i| raws.remove_stable(i)).collect(),
        failed,
    );
    registry.content_hash = crate::planet_atlas::genesis_content_hash(mods_dir);
    registry
}

trait RemoveStable {
    fn remove_stable(&mut self, idx: usize) -> RawMod;
}
impl RemoveStable for Vec<RawMod> {
    fn remove_stable(&mut self, idx: usize) -> RawMod {
        // Order indices refer to the original vec; replace with tombstones.
        let dummy = RawMod {
            info: ModInfo {
                id: String::new(),
                name: String::new(),
                version: String::new(),
                path: None,
                has_script: false,
                retrogen: None,
                error: None,
            },
            depends: vec![],
            blocks: vec![],
            items: vec![],
            animals: vec![],
            npcs: vec![],
            dialogues: vec![],
            quests: vec![],
            structures: vec![],
            loots: vec![],
            pieces: vec![],
            pools: vec![],
            assemblies: vec![],
            settlements: vec![],
            recipes: vec![],
            smelts: vec![],
            fuels: vec![],
            bloomeries: vec![],
            workeds: vec![],
            kilns: vec![],
            kiln_bases: vec![],
            features: vec![],
            tags: vec![],
            aliases: vec![],
            resonances: vec![],
            arcane_sites: vec![],
            workings: vec![],
            preparations: vec![],
            modes: vec![],
            skills: None,
            machines: None,
            nests: None,
            screens: None,
        };
        std::mem::replace(&mut self[idx], dummy)
    }
}
