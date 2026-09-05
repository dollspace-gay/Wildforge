//! Dynamic block/item/recipe registries — the foundation of the mod system.
//! Vanilla content is the built-in `base` mod, registered through the same
//! TOML path external mods use.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;


#[path = "registry/runtime.rs"]
mod runtime;
mod blocks;
pub use blocks::{BlockId, AIR, BlockDef};
mod content_magic;
pub use content_magic::{MaterialClass, MaterialVector, ArcaneDisposition, ArcaneContentDef, ObservationDef, DiscoveryItemDef, DiscoveryFixtureDef, EcologyRole, ArcaneEcologyKind, EcologySource, ReproductionMode, EcologyHarvestClass, ArcaneEcologyDef, ArcaneSiteRule, SalvageDef, RetrogenPolicy};
mod items;
pub use items::{ItemId, ToolKind, NUTRIENTS, FoodDef, ArmorSlot, BowDef, ItemDef};
mod fauna;
pub use fauna::{ModelBox, ProjectileDef, AttackKind, AttackDef, BehaviorArchetype, ArchetypeParams, RusherDef, TankDef, SniperDef, SupportDef, SwarmDef, ControllerDef, PhaserDef, ShieldDef, NestDef, RawNestToml, BuilderDef, HackDef, AnimalDef, AquaticHabitatDef};
mod narrative;
pub use narrative::{NpcDef, DialogueDef, ScriptHook, DialogueNode, DialogueChoice, QuestDef, QuestObjective, SettlementTier, SettlementDef, SettlementNeed, QuestReward, GateDef};
mod recipes;
pub use recipes::{Ingredient, RecipeDef, SmeltDef, ForgeSalvageDef, BloomeryDef, WorkedDef, KilnDef};
mod structures;
pub use structures::{VeinShape, OreFeature, LootEntry, StructureDef, PieceConnector, PieceMarker, PieceChest, PieceDef, PoolEntry, PoolDef, TerrainAdaptation, AssemblyDef, DungeonDef};
use blocks::resolve_light_rgb;
mod policy;
mod placeholders;
mod schema;
mod loading;
pub use loading::load;
pub const WORLD_API_VERSION: u32 = 2;
pub use schema::NestFileToml;
use schema::{AliasToml, AnimalToml, ArcaneContentToml, ArcaneEcologyToml, AssemblyToml, BloomeryToml, BonusDropToml, BrushToml, CharmToml, DialogueToml, DiscoveryFixtureToml, DiscoveryItemToml, FeatureToml, FuelToml, HarvestToml, KilnBaseToml, KilnToml, LootToml, NpcToml, ObservationToml, PieceToml, PoolToml, QuestToml, RawMod, RecipeToml, ResistTomlList, SalvageToml, SettlementToml, SmeltToml, StructureToml, TagToml, TexSpec, WorkedToml, permille};
mod material_graph;
mod salvage;
mod ecology_validation;
mod arcane_validation;
use material_graph::reconcile_material_definitions;
#[cfg(test)]
use arcane_validation::validate_arcane_graph;

#[derive(Clone, Debug)]
pub struct ModInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub path: Option<PathBuf>,
    pub has_script: bool,
    pub retrogen: Option<RetrogenPolicy>,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct Registry {
    pub content_hash: u64,
    pub blocks: Vec<BlockDef>,
    pub items: Vec<ItemDef>,
    pub recipes: Vec<RecipeDef>,
    pub ores: Vec<OreFeature>,
    pub block_by_name: HashMap<String, BlockId>,
    pub item_by_name: HashMap<String, ItemId>,
    /// water_ids[level] — source at 0, flows 1..=7.
    pub water_ids: [BlockId; 8],
    /// lava_ids[level] — the lava chain, same layout as water_ids.
    pub lava_ids: [BlockId; 8],
    pub unknown_block: BlockId,
    pub mods: Vec<ModInfo>,
    pub smelts: Vec<SmeltDef>,
    pub forge_salvage: Vec<ForgeSalvageDef>,
    /// (fuel ingredient, burn seconds, smelt-speed multiplier)
    pub fuels: Vec<(Ingredient, f32, f32)>,
    /// Bloomery firing chains (the steelworks).
    pub bloomery: Vec<BloomeryDef>,
    /// Kiln color chains: powder -> glass.
    pub kiln: Vec<KilnDef>,
    /// Kiln staples: (sand, fuel, clear glass output).
    pub kiln_base: Option<(ItemId, ItemId, ItemId)>,
    /// Anvil work recipes (bloom -> bar).
    pub worked: Vec<WorkedDef>,
    /// Item groups usable as `#tag` recipe ingredients; mods can extend them.
    pub tags: HashMap<String, Vec<ItemId>>,
    /// Mod textures to pack: (slot, png path).
    pub tex_files: Vec<(u16, PathBuf)>,
    /// Pack-addressable names for mod textures: ("<mod_id>/<file stem>", slot).
    pub tex_names: Vec<(String, u16)>,
    pub animals: Vec<AnimalDef>,
    pub npcs: Vec<NpcDef>,
    pub dialogues: Vec<DialogueDef>,
    pub quests: Vec<QuestDef>,
    pub structures: Vec<StructureDef>,
    pub pieces: Vec<PieceDef>,
    pub pools: Vec<PoolDef>,
    pub assemblies: Vec<AssemblyDef>,
    /// Settlements (spec 3.4): tiered piece assemblies revealed by reputation.
    pub settlements: Vec<SettlementDef>,
    /// Flag-gated features (spec 2.5): sealed blocks placed by
    /// `feature:<id>` markers, locked until a per-player KV flag reads a
    /// value. Indexed by `gate_for_block` at load.
    pub gates: Vec<GateDef>,
    /// Sealed block -> gate index, for looking up a gate by its placed block
    /// without marker provenance.
    pub gate_for_block: HashMap<BlockId, usize>,
    pub loots: HashMap<String, Vec<LootEntry>>,
    /// Named survival rulesets from mod `modes.toml` (capability E1), in
    /// dependency order. A world's `mode` string names one of these or the
    /// built-in `survival` / `creative`.
    pub modes: Vec<ModeDef>,
    /// The resolved skill tree (capability E5): branches, nodes, and XP
    /// sources declared across all mods' `skills.toml`. Empty in base.
    pub skills: crate::skills::SkillTree,
    /// Data-driven machine kinds (capability E7) in declaration order. The
    /// index is the stable `MachineKind` id; kind 0 (the first base
    /// machine) is the default. `MachineKind::default()` must stay a valid
    /// machine in every shipped pack, so base declares its machines first.
    pub machines: Vec<crate::machines::MachineDef>,
    /// Nest/dens spawn-gate blocks (capability E9) in declaration order. A
    /// nest's index is its persisted record id; records drop cleanly when a
    /// mod removes a nest.
    pub nests: Vec<NestDef>,
    /// Data-driven mod screens (capability E11) in declaration order. The
    /// index is the runtime `Screen::Mod` id; base ships none.
    pub screens: Vec<crate::screens::ScreenDef>,
    /// Load-time conservation/schema failures. Keeping these attached to the
    /// registry lets the mods screen explain a bad pack and lets production
    /// world creation refuse it without panicking the content browser.
    pub material_errors: Vec<String>,
    /// Versioned, string-addressed resonance identities. Removed providers
    /// remain in each world's saved ledger even when absent here.
    pub arcane_registry: crate::arcane::ResonanceRegistry,
    /// Qualified, bounded magical-geography predicates in dependency order.
    pub arcane_sites: Vec<ArcaneSiteRule>,
    /// Qualified block/item lifecycle definitions, validated at pack load.
    pub arcane_ecology: BTreeMap<String, ArcaneEcologyDef>,
    /// Declarative shells around the closed set of native working handlers.
    /// The definitions carry costs and bounds, never mutation callbacks.
    pub workings: BTreeMap<String, crate::workings::WorkingDef>,
    /// Declarative physical preparation/process contracts. Effects resolve to
    /// the closed native alchemy handler set; no data pack gains raw mutation.
    pub preparations: BTreeMap<String, crate::alchemy::PreparationDef>,
    /// Qualified scar content shells around the closed native placement,
    /// status, and activity handlers. Runtime sites persist these identities.
    pub dross_scars: BTreeMap<String, crate::dross::DrossScarDef>,
    pub arcane_errors: Vec<String>,
}

// ---------------- TOML schema ----------------

fn inferred_material_class(name: &str) -> MaterialClass {
    let local = name.rsplit(':').next().unwrap_or(name);
    if local.contains("heart")
        || local.contains("charm")
        || local.contains("ember")
        || local.contains("frost")
        || local.contains("living_")
    {
        MaterialClass::Exceptional
    } else if local.contains("coal")
        || local.contains("charcoal")
        || local.contains("fuel")
        || local.contains("food")
        || local.contains("bread")
    {
        MaterialClass::Consumptive
    } else if local.contains("ore")
        || local.starts_with("raw_")
        || local.contains("ingot")
        || local.contains("metal")
        || local.contains("diamond")
        || local.contains("monazite")
        || local.contains("bastnasite")
    {
        MaterialClass::GeologicallyFinite
    } else if local.contains("stone")
        || local.contains("sand")
        || local.contains("clay")
        || local.contains("glass")
        || local.contains("brick")
        || local.contains("ceramic")
        || local.contains("gravel")
        || local.contains("dirt")
    {
        MaterialClass::TransformativeFinite
    } else {
        MaterialClass::Renewable
    }
}

fn salvage_def(
    raw: &Option<SalvageToml>,
    errs: &mut Vec<String>,
    name: &str,
) -> Option<SalvageDef> {
    raw.as_ref().map(|salvage| {
        if !(0.0..=1.0).contains(&salvage.recovery) {
            errs.push(format!("{name}: salvage recovery must be between 0 and 1"));
        }
        SalvageDef {
            station: salvage.station.clone(),
            recovery_permille: (salvage.recovery.clamp(0.0, 1.0) * 1000.0).round() as u16,
        }
    })
}

fn observation_def(
    raw: Option<&ObservationToml>,
    mod_id: &str,
    content_id: &str,
) -> Result<Option<ObservationDef>, String> {
    const VISIBLE_PROPERTIES: &[&str] = &[
        "strength",
        "stability",
        "resonance",
        "dross",
        "drift",
        "capacity",
        "conductivity",
        "biological_response",
        "dross_response",
        "condition",
    ];
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.categories.is_empty() || raw.categories.len() > 8 || raw.properties.len() > 12 {
        return Err(format!(
            "{content_id}: observation needs 1..=8 categories and at most 12 visible properties"
        ));
    }
    let categories = raw
        .categories
        .iter()
        .map(|category| {
            let category = if category.contains(':')
                || matches!(
                    category.as_str(),
                    "region"
                        | "block"
                        | "item"
                        | "apparatus"
                        | "heart"
                        | "wake"
                        | "sample"
                        | "echo"
                        | "scar"
                        | "working"
                        | "organism"
                        | "mineral"
                        | "archaeology"
                ) {
                category.clone()
            } else {
                qualify(mod_id, category)
            };
            if category.len() > 64
                || !category.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b':' | b'-')
                })
            {
                return Err(format!(
                    "{content_id}: invalid observation category {category}"
                ));
            }
            Ok(category)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut properties = Vec::new();
    for property in &raw.properties {
        if !VISIBLE_PROPERTIES.contains(&property.as_str()) {
            return Err(format!(
                "{content_id}: observation property {property} is not a qualitative public facet"
            ));
        }
        if !properties.contains(property) {
            properties.push(property.clone());
        }
    }
    Ok(Some(ObservationDef {
        categories,
        properties,
    }))
}

fn discovery_item_def(
    raw: Option<&DiscoveryItemToml>,
    mod_id: &str,
    content_id: &str,
) -> Result<Option<DiscoveryItemDef>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    const KINDS: &[&str] = &[
        "tuning_lens",
        "lens_frame",
        "field_ledger",
        "survey_folio",
        "artifact",
        "calibration_plate",
        "reference_object",
    ];
    if !KINDS.contains(&raw.kind.as_str()) {
        return Err(format!(
            "{content_id}: unknown discovery item kind {}",
            raw.kind
        ));
    }
    if raw.kind == "artifact" && raw.evidence_class.is_none() {
        return Err(format!("{content_id}: an artifact needs an evidence_class"));
    }
    if raw.authored_text.len() > 16
        || raw
            .authored_text
            .iter()
            .any(|line| line.is_empty() || line.len() > 240 || line.chars().any(char::is_control))
    {
        return Err(format!(
            "{content_id}: artifact phrase tables allow at most 16 bounded printable lines"
        ));
    }
    let evidence_class = raw.evidence_class.as_ref().map(|class| {
        if class.contains(':') || crate::discovery::EVIDENCE_CLASSES.contains(&class.as_str()) {
            class.clone()
        } else {
            qualify(mod_id, class)
        }
    });
    if raw.kind == "calibration_plate" && raw.calibration.is_none() {
        return Err(format!(
            "{content_id}: a calibration plate needs a calibration grade"
        ));
    }
    if raw.kind == "reference_object" && raw.experiment.is_none() {
        return Err(format!(
            "{content_id}: a reference object needs an experiment family"
        ));
    }
    Ok(Some(DiscoveryItemDef {
        kind: raw.kind.clone(),
        evidence_class,
        authored_text: raw.authored_text.clone(),
        calibration: raw.calibration,
        experiment: raw.experiment,
    }))
}

fn discovery_fixture_def(
    raw: Option<&DiscoveryFixtureToml>,
    content_id: &str,
) -> Result<Option<DiscoveryFixtureDef>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    const KINDS: &[&str] = &[
        "survey_folio",
        "writing_surface",
        "experiment_apparatus",
        "lens_assembly",
    ];
    if !KINDS.contains(&raw.kind.as_str())
        || raw.experiments.len() > crate::discovery::ExperimentKind::ALL.len()
        || raw.record_capacity > crate::discovery::SURVEY_FOLIO_RECORDS as u16
    {
        return Err(format!(
            "{content_id}: invalid or over-budget discovery fixture"
        ));
    }
    if raw.kind == "experiment_apparatus" && raw.experiments.is_empty() {
        return Err(format!(
            "{content_id}: experiment apparatus has no experiments"
        ));
    }
    Ok(Some(DiscoveryFixtureDef {
        kind: raw.kind.clone(),
        experiments: raw.experiments.clone(),
        record_capacity: raw.record_capacity,
    }))
}

fn arcane_def(
    raw: Option<&ArcaneContentToml>,
    mod_id: &str,
    content_id: &str,
    registry: &crate::arcane::ResonanceRegistry,
) -> Result<Option<ArcaneContentDef>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.capacity == 0 {
        return Err(format!("{content_id}: arcane capacity must be positive"));
    }
    if raw.conductivity > 1_000 || raw.stability > 1_000 {
        return Err(format!(
            "{content_id}: arcane conductivity and stability must be integer permille in 0..=1000"
        ));
    }
    if raw.resonance.is_empty() {
        return Err(format!(
            "{content_id}: arcane resonance mixture is required"
        ));
    }
    let mut resonance = BTreeMap::<String, u16>::new();
    let mut total = 0u32;
    for (name, weight) in &raw.resonance {
        if *weight == 0 {
            return Err(format!("{content_id}: resonance {name} has zero weight"));
        }
        let name = qualify(mod_id, name);
        if !registry.definitions.contains_key(&name) {
            return Err(format!("{content_id}: unknown resonance {name}"));
        }
        total = total
            .checked_add(u32::from(*weight))
            .ok_or_else(|| format!("{content_id}: resonance weights overflow"))?;
        resonance.insert(name, *weight);
    }
    if total == 0 || total > u32::from(u16::MAX) {
        return Err(format!(
            "{content_id}: resonance weight sum must fit a positive u16"
        ));
    }
    Ok(Some(ArcaneContentDef {
        capacity: raw.capacity,
        conductivity_permille: raw.conductivity,
        stability_permille: raw.stability,
        resonance,
        on_destroy: raw.on_destroy,
    }))
}

fn arcane_ecology_def(
    raw: Option<&ArcaneEcologyToml>,
    mod_id: &str,
    content_id: &str,
    registry: &crate::arcane::ResonanceRegistry,
) -> Result<Option<ArcaneEcologyDef>, String> {
    const HABITAT_PREDICATES: &[&str] = &[
        "arid_spring",
        "cave",
        "cool_night",
        "cool_or_temperate",
        "dross_margin",
        "evaporite_host",
        "exposed",
        "fertile",
        "fire_disturbed",
        "freshwater_margin",
        "heartshadow",
        "host_rock",
        "mafic_host",
        "marine",
        "metamorphic_host",
        "moist_cave",
        "nutrient_rich",
        "old_forest",
        "old_organic",
        "permanent_cold",
        "rocky_soil",
        "sedimentary_host",
        "storm_exposed",
        "subsurface",
        "swamp",
        "temperate_ground",
        "warm_wet",
        "wetland",
    ];
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.roles.is_empty() {
        return Err(format!(
            "{content_id}: arcane ecology needs at least one causal role"
        ));
    }
    let unique = raw
        .roles
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    if unique.len() != raw.roles.len() {
        return Err(format!(
            "{content_id}: arcane ecology roles contain duplicates"
        ));
    }
    if raw.habitat.is_empty()
        || raw.habitat.iter().any(|tag| {
            tag.is_empty()
                || tag.len() > 48
                || !tag
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
    {
        return Err(format!(
            "{content_id}: arcane ecology requires lowercase ordinary habitat predicates"
        ));
    }
    if let Some(unknown) = raw
        .habitat
        .iter()
        .find(|tag| !HABITAT_PREDICATES.contains(&tag.as_str()))
    {
        return Err(format!(
            "{content_id}: unknown or unreachable ecology habitat predicate {unknown}"
        ));
    }
    if raw.charge_capacity == 0
        || raw.carrying_capacity == 0
        || raw.carrying_capacity > 4_096
        || raw.min_stability > raw.max_stability
        || raw.max_stability > 1_000
        || raw.min_richness > 1_000
    {
        return Err(format!(
            "{content_id}: ecology capacity/carrying/stability/richness bounds are invalid"
        ));
    }
    if !raw.seasons.into_iter().any(|active| active) {
        return Err(format!("{content_id}: ecology has no active growth season"));
    }
    if raw.resonance.is_empty() {
        return Err(format!(
            "{content_id}: ecology requires a resonance mixture"
        ));
    }
    let mut resonance = BTreeMap::new();
    let mut total = 0u32;
    for (name, weight) in &raw.resonance {
        let qualified = qualify(mod_id, name);
        if *weight == 0 || !registry.definitions.contains_key(&qualified) {
            return Err(format!(
                "{content_id}: unknown or zero-weight ecology resonance {qualified}"
            ));
        }
        if !crate::arcane::BASE_RESONANCES.contains(&qualified.as_str()) {
            return Err(format!(
                "{content_id}: ecology resonance {qualified} has no planetary geographic band"
            ));
        }
        total = total
            .checked_add(u32::from(*weight))
            .ok_or_else(|| format!("{content_id}: ecology resonance weights overflow"))?;
        resonance.insert(qualified, *weight);
    }
    if total == 0 || total > u32::from(u16::MAX) {
        return Err(format!(
            "{content_id}: ecology resonance mixture is invalid"
        ));
    }
    match raw.kind {
        ArcaneEcologyKind::Organism => {
            if raw.reproduction == ReproductionMode::None
                || raw.water_per_day_hu == 0
                || raw.nutrient_per_day == 0
            {
                return Err(format!(
                    "{content_id}: organism growth needs reproduction, nutrients, and a declared water demand"
                ));
            }
            if raw.crystal_stages != 0 || raw.preserving_tool_tier != 0 {
                return Err(format!(
                    "{content_id}: only crystals may declare stages or a preserving tool tier"
                ));
            }
        }
        ArcaneEcologyKind::Crystal => {
            if raw.reproduction != ReproductionMode::Bud
                || !(2..=8).contains(&raw.crystal_stages)
                || raw.preserving_tool_tier == 0
                || raw.uptake_per_day == 0
            {
                return Err(format!(
                    "{content_id}: crystals need bud reproduction, 2..=8 exact stages, uptake, and a preserving tool tier"
                ));
            }
        }
        ArcaneEcologyKind::FiniteMineral => {
            if raw.reproduction != ReproductionMode::None
                || raw.uptake_per_day != 0
                || raw.release_per_day != 0
                || raw.crystal_stages != 0
            {
                return Err(format!(
                    "{content_id}: finite minerals cannot reproduce, grow, release, or declare crystal stages"
                ));
            }
        }
    }
    if raw.uptake_per_day == 0
        && unique.contains(&EcologyRole::Gatherer)
        && raw.kind != ArcaneEcologyKind::FiniteMineral
    {
        return Err(format!(
            "{content_id}: a gatherer cannot have free zero-uptake growth"
        ));
    }
    if raw.source == EcologySource::Dross && !unique.contains(&EcologyRole::Transformer) {
        return Err(format!(
            "{content_id}: dross uptake requires the transformer role"
        ));
    }
    Ok(Some(ArcaneEcologyDef {
        roles: raw.roles.clone(),
        kind: raw.kind,
        habitat: raw.habitat.clone(),
        charge_capacity: raw.charge_capacity,
        uptake_per_day: raw.uptake_per_day,
        release_per_day: raw.release_per_day,
        source: raw.source,
        resonance,
        dross_tolerance: raw.dross_tolerance,
        water_per_day_hu: raw.water_per_day_hu,
        nutrient_per_day: raw.nutrient_per_day,
        reproduction: raw.reproduction,
        seasons: raw.seasons,
        carrying_capacity: raw.carrying_capacity,
        harvest: raw.harvest,
        regrowth_days: raw.regrowth_days,
        min_stability_permille: raw.min_stability,
        max_stability_permille: raw.max_stability,
        min_richness_permille: raw.min_richness,
        crystal_stages: raw.crystal_stages,
        preserving_tool_tier: raw.preserving_tool_tier,
    }))
}

/// A resolved `[[mode]]` (E1 ruleset): which survival toggles are live and
/// which base it inherits from. Stored on the registry for `ruleset_for`.
#[derive(Clone, Debug)]
pub struct ModeDef {
    pub id: String,
    pub base: Option<String>,
    pub creative: Option<bool>,
    pub hunger: Option<bool>,
    pub fall_damage: Option<bool>,
    pub drowning: Option<bool>,
    pub lava_burn: Option<bool>,
    pub hostile_spawns: Option<bool>,
    pub ire: Option<bool>,
    pub hearts: Option<bool>,
    pub weather_extremes: Option<bool>,
    pub pvp: Option<bool>,
    pub skills: Option<bool>,
    pub equipment: Option<bool>,
    pub industrial_ire: Option<bool>,
    pub nest_spawns: Option<bool>,
}

// ---------------- loading ----------------

fn build(raws: Vec<RawMod>, mut failed: Vec<ModInfo>) -> Registry {
    let mut reg = Registry {
        content_hash: 0,
        blocks: Vec::new(),
        items: Vec::new(),
        recipes: Vec::new(),
        ores: Vec::new(),
        block_by_name: HashMap::new(),
        item_by_name: HashMap::new(),
        water_ids: [AIR; 8],
        lava_ids: [AIR; 8],
        unknown_block: AIR,
        mods: Vec::new(),
        smelts: Vec::new(),
        forge_salvage: Vec::new(),
        fuels: Vec::new(),
        bloomery: Vec::new(),
        worked: Vec::new(),
        kiln: Vec::new(),
        kiln_base: None,
        tags: HashMap::new(),
        tex_files: Vec::new(),
        tex_names: Vec::new(),
        animals: Vec::new(),
        npcs: Vec::new(),
        dialogues: Vec::new(),
        quests: Vec::new(),
        structures: Vec::new(),
        pieces: Vec::new(),
        pools: Vec::new(),
        assemblies: Vec::new(),
        settlements: Vec::new(),
        gates: Vec::new(),
        gate_for_block: HashMap::new(),
        loots: HashMap::new(),
        modes: Vec::new(),
        skills: crate::skills::SkillTree::default(),
        machines: Vec::new(),
        nests: Vec::new(),
        screens: Vec::new(),
        material_errors: Vec::new(),
        arcane_registry: crate::arcane::ResonanceRegistry::base(),
        arcane_sites: Vec::new(),
        arcane_ecology: BTreeMap::new(),
        workings: BTreeMap::new(),
        preparations: BTreeMap::new(),
        dross_scars: BTreeMap::new(),
        arcane_errors: Vec::new(),
    };
    for raw in &raws {
        for resonance in &raw.resonances {
            let id = qualify(&raw.info.id, &resonance.id);
            if id.len() > 96
                || !id.contains(':')
                || !id.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b":_-".contains(&byte)
                })
            {
                reg.arcane_errors.push(format!(
                    "{id}: resonance id must be a lowercase qualified content id"
                ));
                continue;
            }
            if reg.arcane_registry.definitions.contains_key(&id) {
                reg.arcane_errors
                    .push(format!("{id}: duplicate resonance identity"));
                continue;
            }
            reg.arcane_registry.definitions.insert(
                id.clone(),
                crate::arcane::ResonanceDefinition {
                    id,
                    label: resonance
                        .label
                        .clone()
                        .unwrap_or_else(|| resonance.id.clone()),
                    provider: raw.info.id.clone(),
                    active: true,
                },
            );
        }
    }
    // Working shells resolve only after every provider's resonance identities
    // exist. A bad shell is never installed, and the shared content error gate
    // prevents authoritative worlds from opening with only part of a pack.
    for raw in &raws {
        for working in &raw.workings {
            match crate::workings::WorkingDef::from_raw(&raw.info.id, working.clone()) {
                Ok(definition) => {
                    if !reg
                        .arcane_registry
                        .definitions
                        .contains_key(&definition.focus)
                    {
                        reg.arcane_errors.push(format!(
                            "{}: unknown focus resonance {}",
                            definition.id, definition.focus
                        ));
                    } else if reg.workings.contains_key(&definition.id) {
                        reg.arcane_errors
                            .push(format!("{}: duplicate working identity", definition.id));
                    } else {
                        reg.workings.insert(definition.id.clone(), definition);
                    }
                }
                Err(error) => reg.arcane_errors.push(error.to_string()),
            }
        }
    }
    const SITE_REQUIREMENTS: [&str; 9] = [
        "fault",
        "carbonate_rock",
        "groundwater",
        "volcanic",
        "river",
        "coast",
        "old_crust",
        "heart",
        "wetland",
    ];
    for raw in &raws {
        for site in &raw.arcane_sites {
            let id = qualify(&raw.info.id, &site.id);
            let mut invalid = false;
            if id.len() > 96
                || !id.contains(':')
                || !id.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b":_-".contains(&byte)
                })
            {
                reg.arcane_errors.push(format!(
                    "{id}: arcane site id must be a lowercase qualified content id"
                ));
                invalid = true;
            }
            if reg.arcane_sites.iter().any(|rule| rule.id == id) {
                reg.arcane_errors
                    .push(format!("{id}: duplicate arcane site identity"));
                invalid = true;
            }
            for requirement in &site.requires {
                if !SITE_REQUIREMENTS.contains(&requirement.as_str()) {
                    reg.arcane_errors.push(format!(
                        "{id}: unknown arcane site requirement {requirement}"
                    ));
                    invalid = true;
                }
            }
            if !site.capacity_factor.is_finite()
                || !(0.25..=4.0).contains(&site.capacity_factor)
                || !site.rarity.is_finite()
                || !(0.0..=1.0).contains(&site.rarity)
                || site.radius_cells == 0
                || site.radius_cells > 64
            {
                reg.arcane_errors.push(format!(
                    "{id}: capacity_factor must be 0.25..=4, rarity 0..=1, and radius_cells 1..=64"
                ));
                invalid = true;
            }
            let mut resonance = [0u16; 6];
            for (name, weight) in &site.resonance {
                let qualified = qualify(&raw.info.id, name);
                let Some(slot) = crate::arcane::BASE_RESONANCES
                    .iter()
                    .position(|candidate| *candidate == qualified)
                else {
                    reg.arcane_errors.push(format!(
                        "{id}: geography genesis currently accepts only the six base resonances; found {qualified}"
                    ));
                    invalid = true;
                    continue;
                };
                resonance[slot] = *weight;
            }
            if invalid {
                continue;
            }
            reg.arcane_sites.push(ArcaneSiteRule {
                id,
                provider: raw.info.id.clone(),
                requires: site.requires.clone(),
                capacity_factor_permille: (site.capacity_factor * 1_000.0).round() as u16,
                base_resonance_bias: resonance,
                rarity_per_million: (site.rarity * 1_000_000.0).round() as u32,
                radius_cells: site.radius_cells,
                retrogen: raw.info.retrogen.unwrap_or(RetrogenPolicy::NoRetrogen),
            });
        }
    }
    let mut tex_slots: HashMap<String, u16> = crate::atlas::builtin_slots();
    let mut next_slot: u16 = crate::atlas::FIRST_FREE_SLOT;

    // Air (id 0) and the unknown-block placeholder are engine-registered.
    let air = BlockDef {
        name: "base:air".into(),
        label: "Air".into(),
        tiles: [0; 6],
        hardness: None,
        tool: None,
        requires_tool: false,
        drops: None,
        solid: false,
        opaque: false,
        interaction: None,
        min_tier: 0,
        water_level: None,
        lava: false,
        cross: false,
        burns: 0,
        floats: false,
        shape: None,
        crop_next: None,
        crop_chance: 0.0,
        crop_any_soil: false,
        harvest: None,
        light_emit: 0,
        sapling: None,
        bonus_drop: None,
        brush: None,
        height: None,
        falls: false,
        glass: false,
        light_filter: [true; 3],
        light_rgb: [0, 0, 0],
        fert_tiles: None,
        crop_family: 0,
        material_class: MaterialClass::Renewable,
        materials: MaterialVector::new(),
        dismantles_to: None,
        heat_retention: 0,
        arcane: None,
        arcane_ecology: None,
        observation: None,
        discovery_fixture: None,
    };
    reg.block_by_name.insert(air.name.clone(), BlockId(0));
    reg.blocks.push(air);

    let mut resolve_tex = |spec: &str, mod_path: &Option<PathBuf>, errs: &mut Vec<String>| -> u16 {
        if let Some(name) = spec.strip_prefix('@') {
            return *tex_slots.get(name).unwrap_or_else(|| {
                errs.push(format!("unknown builtin texture @{name}"));
                &crate::atlas::UNKNOWN_SLOT
            });
        }
        let key = format!(
            "{}/{}",
            mod_path
                .as_deref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            spec
        );
        if let Some(s) = tex_slots.get(&key) {
            return *s;
        }
        let Some(dir) = mod_path else {
            errs.push(format!("texture {spec} needs a mod directory"));
            return crate::atlas::UNKNOWN_SLOT;
        };
        let path = dir.join("textures").join(spec);
        let stem = spec.strip_suffix(".png").unwrap_or(spec);
        let embedded =
            dir.as_os_str() == "base" && crate::atlas::embedded_base_tile(stem).is_some();
        if !path.exists() && !embedded {
            errs.push(format!("missing texture {spec}"));
            return crate::atlas::UNKNOWN_SLOT;
        }
        // Mod tiles own FIRST_FREE_SLOT up to the reserved player
        // rows at the top of the 32-wide atlas (a stale 256 cap from
        // the 16-wide era once lived here).
        if next_slot >= crate::style::EXTRA_BASE {
            errs.push("texture atlas full".into());
            return crate::atlas::UNKNOWN_SLOT;
        }
        let slot = next_slot;
        next_slot += 1;
        tex_slots.insert(key, slot);
        let mod_id = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let stem = spec.strip_suffix(".png").unwrap_or(spec);
        reg.tex_names.push((format!("{mod_id}/{stem}"), slot));
        reg.tex_files.push((slot, path));
        slot
    };

    // Pass 1: register blocks and items (unresolved drops/recipes yet).
    struct PendingDrop {
        modid: String,
        block: usize,
        rule: String,
        count: u32,
    }
    let mut pending_drops: Vec<PendingDrop> = Vec::new();
    let mut pending_recipes: Vec<(String, RecipeToml)> = Vec::new();
    let mut pending_features: Vec<(String, FeatureToml)> = Vec::new();
    let mut pending_tags: Vec<(String, TagToml)> = Vec::new();
    let mut pending_smelts: Vec<(String, SmeltToml)> = Vec::new();
    let mut pending_bloomeries: Vec<(String, BloomeryToml)> = Vec::new();
    let mut pending_workeds: Vec<(String, WorkedToml)> = Vec::new();
    let mut pending_kilns: Vec<(String, KilnToml)> = Vec::new();
    let mut pending_kiln_bases: Vec<(String, KilnBaseToml)> = Vec::new();
    let mut pending_fuels: Vec<(String, FuelToml)> = Vec::new();
    let mut pending_aliases: Vec<(String, AliasToml)> = Vec::new();
    let mut pending_harvests: Vec<(String, BlockId, HarvestToml)> = Vec::new();
    let mut pending_bonus: Vec<(String, usize, BonusDropToml)> = Vec::new();
    let mut pending_brush: Vec<(String, usize, BrushToml)> = Vec::new();
    let mut pending_structs: Vec<(String, StructureToml)> = Vec::new();
    let mut pending_loots: Vec<(String, LootToml)> = Vec::new();
    let mut pending_pieces: Vec<(String, PieceToml)> = Vec::new();
    let mut pending_pools: Vec<(String, PoolToml)> = Vec::new();
    let mut pending_assemblies: Vec<(String, AssemblyToml)> = Vec::new();
    let mut pending_places: Vec<(String, (String, String))> = Vec::new();
    // (mod id, toml, body tile, head tile, per-box tiles) — resolve in pass 1.
    #[allow(clippy::type_complexity)]
    let mut pending_animals: Vec<(
        String,
        AnimalToml,
        u16,
        u16,
        HashMap<String, u16>,
        Option<u16>,
        Vec<Option<u16>>,
    )> = Vec::new();
    #[allow(clippy::type_complexity)]
    let mut pending_npcs: Vec<(String, NpcToml, u16, u16, HashMap<String, u16>)> = Vec::new();
    let mut pending_dialogues: Vec<(String, DialogueToml)> = Vec::new();
    let mut pending_quests: Vec<(String, QuestToml)> = Vec::new();
    let mut pending_settlements: Vec<(String, SettlementToml)> = Vec::new();

    for raw in &raws {
        if raw.info.id.is_empty() {
            continue; // tombstone
        }
        let mut errs: Vec<String> = Vec::new();
        for b in &raw.blocks {
            let full = qualify(&raw.info.id, &b.id);
            if reg.block_by_name.contains_key(&full) {
                errs.push(format!("duplicate block {full}"));
                continue;
            }
            let tiles = match &b.texture {
                TexSpec::One(t) => [resolve_tex(t, &raw.info.path, &mut errs); 6],
                TexSpec::Faces { top, side, bottom } => {
                    let t = resolve_tex(top, &raw.info.path, &mut errs);
                    let s = resolve_tex(side, &raw.info.path, &mut errs);
                    let bo = bottom
                        .as_ref()
                        .map(|x| resolve_tex(x, &raw.info.path, &mut errs))
                        .unwrap_or(t);
                    [s, s, t, bo, s, s]
                }
            };
            let fert_tiles = b.texture_fertility.as_ref().map(|v| {
                if v.len() != 4 {
                    errs.push(format!("{full}: texture_fertility wants 4 entries"));
                }
                let mut ft = [tiles[2]; 4];
                for (i, t) in v.iter().take(4).enumerate() {
                    ft[i] = resolve_tex(t, &raw.info.path, &mut errs);
                }
                ft
            });
            let arcane =
                match arcane_def(b.arcane.as_ref(), &raw.info.id, &full, &reg.arcane_registry) {
                    Ok(definition) => definition,
                    Err(error) => {
                        errs.push(error.clone());
                        reg.arcane_errors.push(error);
                        None
                    }
                };
            let arcane_ecology = match arcane_ecology_def(
                b.arcane_ecology.as_ref(),
                &raw.info.id,
                &full,
                &reg.arcane_registry,
            ) {
                Ok(definition) => definition,
                Err(error) => {
                    errs.push(error.clone());
                    reg.arcane_errors.push(error);
                    None
                }
            };
            let mut observation = match observation_def(b.observation.as_ref(), &raw.info.id, &full)
            {
                Ok(definition) => definition,
                Err(error) => {
                    errs.push(error);
                    None
                }
            };
            if observation.is_none() && b.interaction.as_deref() == Some("heart") {
                observation = Some(ObservationDef {
                    categories: vec!["heart".into()],
                    properties: vec![
                        "strength".into(),
                        "stability".into(),
                        "resonance".into(),
                        "dross".into(),
                        "condition".into(),
                    ],
                });
            }
            let discovery_fixture = match discovery_fixture_def(b.discovery_fixture.as_ref(), &full)
            {
                Ok(definition) => definition,
                Err(error) => {
                    errs.push(error);
                    None
                }
            };
            let id = BlockId(reg.blocks.len() as u16);
            let is_fluid = b.water.is_some() || b.lava.is_some();
            reg.blocks.push(BlockDef {
                name: full.clone(),
                label: b.name.clone().unwrap_or_else(|| b.id.clone()),
                tiles,
                hardness: if b.unbreakable || is_fluid {
                    None
                } else {
                    b.hardness.or(Some(1.0))
                },
                tool: b.tool,
                requires_tool: b.requires_tool,
                drops: None,
                solid: b.solid && !is_fluid,
                opaque: b.opaque && !is_fluid,
                interaction: b.interaction.clone(),
                min_tier: b.min_tier,
                water_level: b.water.or(b.lava),
                lava: b.lava.is_some(),
                cross: b.cross,
                burns: b.burns,
                floats: b.floats,
                shape: b.shape.clone(),
                crop_next: None,
                crop_chance: 0.0,
                crop_any_soil: b.crop.as_ref().is_some_and(|c| c.any_soil),
                harvest: None,
                light_emit: b.light.min(15),
                sapling: b.sapling.as_ref().map(|t| t.tree.clone()),
                bonus_drop: None,
                brush: None,
                height: b.height.map(|h| h.clamp(0.05, 1.0)),
                falls: b.falls,
                glass: b.glass,
                light_filter: b
                    .light_filter
                    .map(|f| [f[0] > 0, f[1] > 0, f[2] > 0])
                    .unwrap_or([true; 3]),
                light_rgb: resolve_light_rgb(b.light.min(15), b.light_color),
                fert_tiles,
                crop_family: b
                    .crop
                    .as_ref()
                    .map(|c| {
                        c.family.map(|f| f.clamp(1, 3)).unwrap_or_else(|| {
                            // Stable name-derived family for mods.
                            let h = full
                                .bytes()
                                .fold(0u32, |a, ch| a.wrapping_mul(31).wrapping_add(ch as u32));
                            (h % 3 + 1) as u8
                        })
                    })
                    .unwrap_or(0),
                material_class: b
                    .material_class
                    .unwrap_or_else(|| inferred_material_class(&full)),
                materials: b.materials.clone(),
                dismantles_to: None,
                heat_retention: b.heat_retention,
                arcane: arcane.clone(),
                arcane_ecology: arcane_ecology.clone(),
                observation: observation.clone(),
                discovery_fixture: discovery_fixture.clone(),
            });
            if let Some(scar) = &b.dross_scar {
                let valid_carriers = !scar.carriers.is_empty()
                    && scar.carriers.len() <= 3
                    && scar.carriers.iter().all(|carrier| {
                        matches!(
                            carrier,
                            crate::dross::DrossCarrier::Air
                                | crate::dross::DrossCarrier::Water
                                | crate::dross::DrossCarrier::Soil
                        )
                    })
                    && scar
                        .carriers
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        == scar.carriers.len();
                let valid_band = matches!(
                    scar.min_band,
                    crate::dross::DrossBand::Seep
                        | crate::dross::DrossBand::Scar
                        | crate::dross::DrossBand::BreachRisk
                );
                if !valid_carriers || !valid_band || !(1..=8).contains(&scar.max_sites_per_region) {
                    let error = format!(
                        "{full}: dross scar needs 1..=3 unique environmental carriers, a seep-or-higher band, and 1..=8 sites per region"
                    );
                    errs.push(error.clone());
                    reg.arcane_errors.push(error);
                } else if reg.dross_scars.contains_key(&full) {
                    let error = format!("{full}: duplicate dross scar identity");
                    errs.push(error.clone());
                    reg.arcane_errors.push(error);
                } else {
                    reg.dross_scars.insert(
                        full.clone(),
                        crate::dross::DrossScarDef {
                            content_id: full.clone(),
                            provider: raw.info.id.clone(),
                            block: id,
                            kind: scar.kind,
                            handler: scar.handler,
                            carriers: scar.carriers.clone(),
                            min_band: scar.min_band,
                            status: scar.status,
                            activity: scar.activity,
                            max_sites_per_region: scar.max_sites_per_region,
                        },
                    );
                }
            }
            if let Some(ecology) = &arcane_ecology {
                reg.arcane_ecology.insert(full.clone(), ecology.clone());
            }
            reg.block_by_name.insert(full.clone(), id);
            if let Some(bd) = &b.bonus_drop {
                pending_bonus.push((raw.info.id.clone(), id.0 as usize, bd.clone()));
            }
            if let Some(br) = &b.brush {
                pending_brush.push((raw.info.id.clone(), id.0 as usize, br.clone()));
            }
            pending_drops.push(PendingDrop {
                modid: raw.info.id.clone(),
                block: id.0 as usize,
                rule: b.drops.clone().unwrap_or_else(|| {
                    if is_fluid {
                        "none".into()
                    } else {
                        "self".into()
                    }
                }),
                count: b.drop_count.unwrap_or(1),
            });
            if let Some(crop) = &b.crop {
                // Auto-register growth stages; each links to the next.
                let mut prev = id;
                for st in 1..crop.stages {
                    let sid = BlockId(reg.blocks.len() as u16);
                    let mut def = reg.blocks[id.0 as usize].clone();
                    def.name = format!("{full}/stage{st}");
                    if let Some(t) = crop.stage_textures.get(st as usize - 1) {
                        let s = resolve_tex(t, &raw.info.path, &mut errs);
                        def.tiles = [s; 6];
                    }
                    reg.block_by_name.insert(def.name.clone(), sid);
                    reg.blocks.push(def);
                    reg.blocks[prev.0 as usize].crop_next = Some(sid);
                    reg.blocks[prev.0 as usize].crop_chance = crop.next_chance.unwrap_or(0.2);
                    prev = sid;
                }
                // The final stage grows no further (clones inherit the
                // base's link otherwise).
                reg.blocks[prev.0 as usize].crop_next = None;
                reg.blocks[prev.0 as usize].crop_chance = 0.0;
            }
            if let Some(h) = &b.harvest {
                // Harvest applies to the final growth stage (or the block
                // itself when it has no stages).
                let target = BlockId(reg.blocks.len() as u16 - 1);
                let target = if b.crop.is_some() { target } else { id };
                pending_harvests.push((raw.info.id.clone(), target, h.clone()));
            }
            if b.water == Some(0) || b.lava == Some(0) {
                // Auto-register the 7 flowing variants (either fluid).
                let ids = if b.lava.is_some() {
                    &mut reg.lava_ids
                } else {
                    &mut reg.water_ids
                };
                ids[0] = id;
                for l in 1..=7u8 {
                    let fid = BlockId(reg.blocks.len() as u16);
                    let mut def = reg.blocks[id.0 as usize].clone();
                    def.name = format!("{full}/flow{l}");
                    def.water_level = Some(l);
                    reg.block_by_name.insert(def.name.clone(), fid);
                    reg.blocks.push(def);
                    ids[l as usize] = fid;
                }
            }
            if b.item && !is_fluid {
                let icon_slot = b
                    .icon
                    .as_ref()
                    .map(|t| resolve_tex(t, &raw.info.path, &mut errs))
                    .unwrap_or(tiles[0]);
                let iid = ItemId(reg.items.len() as u16);
                reg.items.push(ItemDef {
                    name: full.clone(),
                    label: reg.blocks[id.0 as usize].label.clone(),
                    icon: icon_slot,
                    max_stack: if arcane.is_some()
                        || discovery_fixture
                            .as_ref()
                            .is_some_and(|fixture| fixture.kind == "survey_folio")
                    {
                        1
                    } else {
                        64
                    },
                    tool: None,
                    durability: 0,
                    places: Some(id),
                    food: None,
                    damage: 1.0,
                    damage_type: None,
                    bow: None,
                    ammo: None,
                    armor: None,
                    carry_weight: 1,
                    stats: Vec::new(),
                    frame: None,
                    component: None,
                    bedroll: false,
                    shears: false,
                    charm: None,
                    charm_def: None,
                    wand_component: None,
                    implement: None,
                    tablet: false,
                    striker: false,
                    creative_only: false,
                    brush_tool: false,
                    throw_speed: None,
                    hammer: false,
                    hack: false,
                    glow: None,
                    materials: b.materials.clone(),
                    materials_declared: !b.materials.is_empty(),
                    material_class: b
                        .material_class
                        .unwrap_or_else(|| inferred_material_class(&full)),
                    salvage: None,
                    broken_into: None,
                    arcane,
                    arcane_ecology,
                    observation,
                    discovery: discovery_fixture.as_ref().and_then(|fixture| {
                        (fixture.kind == "survey_folio").then(|| DiscoveryItemDef {
                            kind: "survey_folio".into(),
                            evidence_class: None,
                            authored_text: Vec::new(),
                            calibration: None,
                            experiment: None,
                        })
                    }),
                });
                reg.item_by_name.insert(full, iid);
            }
        }
        for it in &raw.items {
            let full = qualify(&raw.info.id, &it.id);
            if reg.item_by_name.contains_key(&full) {
                errs.push(format!("duplicate item {full}"));
                continue;
            }
            let icon = resolve_tex(&it.texture, &raw.info.path, &mut errs);
            let tool = it
                .tool
                .map(|k| (k, it.tool_speed.unwrap_or(4.0), it.tool_tier.unwrap_or(1)));
            let iid = ItemId(reg.items.len() as u16);
            let food = it.food.as_ref().map(|f| {
                let mut n = [0.0f32; 5];
                for (k, v) in &f.nutrition {
                    if let Some(i) = NUTRIENTS.iter().position(|x| x == k) {
                        n[i] = *v;
                    }
                }
                FoodDef {
                    hunger: f.hunger,
                    eat_time: f.eat_time.unwrap_or(1.5),
                    nutrition: n,
                }
            });
            let damage = it.damage.unwrap_or(match tool {
                Some((ToolKind::Axe, _, _)) => 3.0,
                Some(_) => 2.0,
                None => 1.0,
            });
            let armor = it
                .armor
                .as_ref()
                .and_then(|a| ArmorSlot::parse(&a.slot).map(|s| (s, a.points)));
            let arcane = match arcane_def(
                it.arcane.as_ref(),
                &raw.info.id,
                &full,
                &reg.arcane_registry,
            ) {
                Ok(definition) => definition,
                Err(error) => {
                    errs.push(error.clone());
                    reg.arcane_errors.push(error);
                    None
                }
            };
            let arcane_ecology = match arcane_ecology_def(
                it.arcane_ecology.as_ref(),
                &raw.info.id,
                &full,
                &reg.arcane_registry,
            ) {
                Ok(definition) => definition,
                Err(error) => {
                    errs.push(error.clone());
                    reg.arcane_errors.push(error);
                    None
                }
            };
            let observation = match observation_def(it.observation.as_ref(), &raw.info.id, &full) {
                Ok(definition) => definition,
                Err(error) => {
                    errs.push(error);
                    None
                }
            };
            let discovery = match discovery_item_def(it.discovery.as_ref(), &raw.info.id, &full) {
                Ok(definition) => definition,
                Err(error) => {
                    errs.push(error);
                    None
                }
            };
            let charm_def = it.charm.as_ref().and_then(CharmToml::definition);
            if let Some(raw_charm) = &it.charm {
                match &charm_def {
                    Some(definition) => {
                        if let Err(error) = crate::implements::validate_charm(&full, definition) {
                            let error = error.to_string();
                            errs.push(error.clone());
                            reg.arcane_errors.push(error);
                        }
                    }
                    None => {
                        let error =
                            format!("{full}: unknown charm effect {}", raw_charm.effect_id());
                        errs.push(error.clone());
                        reg.arcane_errors.push(error);
                    }
                }
            }
            if let Some(component) = &it.wand_component
                && let Err(error) = crate::implements::validate_component(&full, component)
            {
                let error = error.to_string();
                errs.push(error.clone());
                reg.arcane_errors.push(error);
            }
            let one_only = tool.is_some()
                || it.bow.is_some()
                || armor.is_some()
                || arcane.is_some()
                || discovery.is_some()
                || charm_def.is_some()
                || it.wand_component.is_some()
                || it.implement.is_some()
                || it.frame.is_some()
                || it.component.is_some();
            let frame = match &it.frame {
                Some(raw) => {
                    let def = crate::equipment::FrameDef {
                        slots: raw
                            .slots
                            .iter()
                            .map(|s| crate::equipment::FrameSlotDef {
                                slot_type: s.kind.clone(),
                                max: s.max,
                            })
                            .collect(),
                    };
                    for error in def.validate() {
                        errs.push(format!("{full}: {error}"));
                    }
                    if it.component.is_some() {
                        errs.push(format!(
                            "{full}: an item cannot be both a frame and a component"
                        ));
                    }
                    Some(def)
                }
                None => {
                    if let Some(slot_type) = &it.component
                        && slot_type.is_empty()
                    {
                        errs.push(format!("{full}: component slot type must not be empty"));
                    }
                    None
                }
            };
            reg.items.push(ItemDef {
                name: full.clone(),
                label: it.name.clone().unwrap_or_else(|| it.id.clone()),
                icon,
                max_stack: if one_only {
                    1
                } else {
                    it.max_stack.unwrap_or(64)
                },
                tool,
                durability: it.durability.unwrap_or(if tool.is_some() { 59 } else { 0 }),
                places: None,
                food,
                damage,
                damage_type: it.damage_type.clone(),
                bow: it.bow.as_ref().map(|b| BowDef {
                    damage: b.damage,
                    speed: b.speed.unwrap_or(24.0),
                }),
                ammo: it.ammo.clone(),
                armor,
                carry_weight: it.carry_weight.unwrap_or(1),
                stats: it
                    .stats
                    .iter()
                    .filter_map(|s| {
                        let kind = match s.kind.parse() {
                            Ok(kind) => kind,
                            Err(error) => {
                                errs.push(format!("{full}: {error}"));
                                return None;
                            }
                        };
                        Some(crate::stats::StatModifier {
                            kind,
                            flat: s.flat.unwrap_or(0.0),
                            mult_permille: s.mult_permille.unwrap_or(1_000),
                        })
                    })
                    .collect(),
                frame,
                component: it.component.clone(),
                bedroll: it.bedroll,
                shears: it.shears,
                charm: it.charm.as_ref().map(CharmToml::effect_id),
                charm_def,
                wand_component: it.wand_component.clone(),
                implement: it.implement.clone(),
                tablet: it.tablet,
                striker: it.striker,
                creative_only: false,
                brush_tool: it.brush_tool,
                throw_speed: it.throw.as_ref().map(|t| t.speed.unwrap_or(18.0)),
                hammer: it.hammer,
                hack: it.hack,
                glow: it.glow,
                materials: it.materials.clone(),
                materials_declared: !it.materials.is_empty(),
                material_class: it
                    .material_class
                    .unwrap_or_else(|| inferred_material_class(&full)),
                salvage: salvage_def(&it.salvage, &mut errs, &full),
                broken_into: None,
                arcane,
                arcane_ecology: arcane_ecology.clone(),
                observation,
                discovery,
            });
            if let Some(ecology) = arcane_ecology {
                reg.arcane_ecology.entry(full.clone()).or_insert(ecology);
            }
            reg.item_by_name.insert(full, iid);
        }
        for r in &raw.recipes {
            pending_recipes.push((raw.info.id.clone(), r.clone()));
        }
        for f in &raw.features {
            pending_features.push((raw.info.id.clone(), f.clone()));
        }
        for it in &raw.items {
            if let Some(p) = &it.places {
                pending_places.push((raw.info.id.clone(), (it.id.clone(), p.clone())));
            }
        }
        for t in &raw.tags {
            pending_tags.push((raw.info.id.clone(), t.clone()));
        }
        for s in &raw.smelts {
            pending_smelts.push((raw.info.id.clone(), s.clone()));
        }
        for b in &raw.bloomeries {
            pending_bloomeries.push((raw.info.id.clone(), b.clone()));
        }
        for w in &raw.workeds {
            pending_workeds.push((raw.info.id.clone(), w.clone()));
        }
        for k in &raw.kilns {
            pending_kilns.push((raw.info.id.clone(), k.clone()));
        }
        for k in &raw.kiln_bases {
            pending_kiln_bases.push((raw.info.id.clone(), k.clone()));
        }
        for st in &raw.structures {
            pending_structs.push((raw.info.id.clone(), st.clone()));
        }
        for p in &raw.pieces {
            pending_pieces.push((raw.info.id.clone(), p.clone()));
        }
        for p in &raw.pools {
            pending_pools.push((raw.info.id.clone(), p.clone()));
        }
        for a in &raw.assemblies {
            pending_assemblies.push((raw.info.id.clone(), a.clone()));
        }
        for s in &raw.settlements {
            pending_settlements.push((raw.info.id.clone(), s.clone()));
        }
        for lt in &raw.loots {
            pending_loots.push((raw.info.id.clone(), lt.clone()));
        }
        for a in &raw.animals {
            let tile = resolve_tex(&a.tex, &raw.info.path, &mut errs);
            let head = a
                .head_tex
                .as_ref()
                .map(|t| resolve_tex(t, &raw.info.path, &mut errs))
                .unwrap_or(tile);
            let box_tiles: HashMap<String, u16> = a
                .model
                .iter()
                .filter_map(|(n, b)| {
                    b.tex
                        .as_ref()
                        .map(|t| (n.clone(), resolve_tex(t, &raw.info.path, &mut errs)))
                })
                .collect();
            let proj_tile = a
                .projectile
                .as_ref()
                .map(|pr| resolve_tex(&pr.tex, &raw.info.path, &mut errs));
            let mut attack_proj_tiles: Vec<Option<u16>> = Vec::new();
            for atk in &a.attacks {
                match atk.kind.as_str() {
                    "melee" | "charge" | "projectile" => {}
                    other => errs.push(format!("animal {}: unknown attack kind {other}", a.id)),
                }
                attack_proj_tiles.push(
                    atk.projectile
                        .as_ref()
                        .map(|pr| resolve_tex(&pr.tex, &raw.info.path, &mut errs)),
                );
            }
            match a.behavior.as_deref() {
                None
                | Some(
                    "standard" | "brute" | "construct" | "builder" | "rusher" | "tank" | "sniper"
                    | "support" | "swarm" | "controller" | "phaser" | "shield_bearer",
                ) => {}
                Some(other) => errs.push(format!("animal {}: unknown behavior {other}", a.id)),
            }
            // E9: an archetype that names a companion species must resolve
            // it (checked against base + this mod's roster names here; the
            // full cross-mod resolution happens after the roster exists).
            if a.behavior.as_deref() == Some("swarm") || a.behavior.as_deref() == Some("controller")
            {
                let spawn = if a.behavior.as_deref() == Some("swarm") {
                    a.swarm.as_ref().and_then(|s| s.spawn.clone())
                } else {
                    a.controller.as_ref().and_then(|c| c.spawn.clone())
                };
                if spawn.is_none() {
                    errs.push(format!(
                        "animal {}: behavior {} requires a companion species (`spawn`)",
                        a.id,
                        a.behavior.as_deref().unwrap_or("")
                    ));
                }
            }
            pending_animals.push((
                raw.info.id.clone(),
                a.clone(),
                tile,
                head,
                box_tiles,
                proj_tile,
                attack_proj_tiles,
            ));
        }
        for n in &raw.npcs {
            let tile = resolve_tex(&n.tex, &raw.info.path, &mut errs);
            let head = n
                .head_tex
                .as_ref()
                .map(|t| resolve_tex(t, &raw.info.path, &mut errs))
                .unwrap_or(tile);
            let box_tiles: HashMap<String, u16> = n
                .model
                .iter()
                .filter_map(|(name, b)| {
                    b.tex
                        .as_ref()
                        .map(|t| (name.clone(), resolve_tex(t, &raw.info.path, &mut errs)))
                })
                .collect();
            pending_npcs.push((raw.info.id.clone(), n.clone(), tile, head, box_tiles));
        }
        for d in &raw.dialogues {
            pending_dialogues.push((raw.info.id.clone(), d.clone()));
        }
        for q in &raw.quests {
            pending_quests.push((raw.info.id.clone(), q.clone()));
        }
        for f in &raw.fuels {
            pending_fuels.push((raw.info.id.clone(), f.clone()));
        }
        for a in &raw.aliases {
            pending_aliases.push((raw.info.id.clone(), a.clone()));
        }
        let mut info = raw.info.clone();
        if !errs.is_empty() {
            info.error = Some(errs.join("; "));
        }
        reg.mods.push(info);
    }

    // The unknown-block placeholder.
    let unk = BlockId(reg.blocks.len() as u16);
    reg.blocks.push(BlockDef {
        name: "base:unknown".into(),
        label: "Unknown".into(),
        tiles: [crate::atlas::UNKNOWN_SLOT; 6],
        hardness: Some(0.5),
        tool: None,
        requires_tool: false,
        drops: None,
        solid: true,
        opaque: true,
        interaction: None,
        min_tier: 0,
        water_level: None,
        lava: false,
        cross: false,
        burns: 0,
        floats: false,
        shape: None,
        crop_next: None,
        crop_chance: 0.0,
        crop_any_soil: false,
        harvest: None,
        light_emit: 0,
        sapling: None,
        bonus_drop: None,
        brush: None,
        height: None,
        falls: false,
        glass: false,
        light_filter: [true; 3],
        light_rgb: [0, 0, 0],
        fert_tiles: None,
        crop_family: 0,
        material_class: MaterialClass::TransformativeFinite,
        materials: MaterialVector::new(),
        dismantles_to: None,
        heat_retention: 0,
        arcane: None,
        arcane_ecology: None,
        observation: None,
        discovery_fixture: None,
    });
    reg.block_by_name.insert("base:unknown".into(), unk);
    reg.unknown_block = unk;

    // Pass 2: resolve drops, recipes, features by name.
    let lookup_item = |reg: &Registry, modid: &str, name: &str| -> Option<ItemId> {
        reg.item_id(&qualify(modid, name))
            .or_else(|| reg.item_id(name))
    };
    // Tags first (recipes reference them). Multiple mods extend the same tag.
    for (modid, t) in pending_tags {
        let tag_name = qualify(&modid, &t.id);
        for item in &t.items {
            if let Some(id) = lookup_item(&reg, &modid, item) {
                let entry = reg.tags.entry(tag_name.clone()).or_default();
                if !entry.contains(&id) {
                    entry.push(id);
                }
            }
        }
    }
    for (modid, bi, bd) in pending_bonus {
        if let Some(item) = lookup_item(&reg, &modid, &bd.item) {
            reg.blocks[bi].bonus_drop = Some((item, bd.chance));
        }
    }
    let lookup_block = |reg: &Registry, modid: &str, name: &str| -> Option<BlockId> {
        reg.block_id(&qualify(modid, name))
            .or_else(|| reg.block_id(name))
    };
    for (modid, bi, br) in pending_brush {
        if let Some(becomes) = lookup_block(&reg, &modid, &br.becomes) {
            reg.blocks[bi].brush = Some((qualify(&modid, &br.table), becomes));
        }
    }
    for (modid, lt) in pending_loots {
        let entries: Vec<LootEntry> = lt
            .entries
            .iter()
            .filter_map(|e| {
                lookup_item(&reg, &modid, &e.item).map(|item| LootEntry {
                    item,
                    weight: e.weight.max(1),
                    count: e.count.map(|c| (c[0], c[1])).unwrap_or((1, 1)),
                    durability_frac: e.durability,
                })
            })
            .collect();
        if !entries.is_empty() {
            reg.loots.insert(qualify(&modid, &lt.id), entries);
        }
    }
    for (modid, st) in pending_structs {
        let mut palette = HashMap::new();
        let mut ok = true;
        for (ch, block) in &st.palette {
            let Some(c) = ch.chars().next() else { continue };
            match lookup_block(&reg, &modid, block) {
                Some(b) => {
                    palette.insert(c, b);
                }
                None => ok = false,
            }
        }
        if !ok {
            continue;
        }
        reg.structures.push(StructureDef {
            name: qualify(&modid, &st.id),
            biomes: st.biomes.iter().map(|b| b.to_lowercase()).collect(),
            rarity: st.rarity.max(1),
            buried: if st.placement.as_deref() == Some("buried") {
                let d = st.depth.unwrap_or([5, 15]);
                Some((d[0], d[1].max(d[0])))
            } else {
                None
            },
            palette,
            layers: st.layers,
            loot: st.loot.as_ref().map(|l| qualify(&modid, l)),
        });
    }
    let lookup_piece = |reg: &Registry, modid: &str, name: &str| -> Option<String> {
        qualified_piece_id(reg, modid, name)
    };
    // Pieces reference only block *names* (resolved at stamp time), so cells
    // pass through verbatim. Connector facings are validated against the four
    // cardinal directions at load, keeping the walk free of parse errors.
    for (modid, p) in pending_pieces {
        let mut connectors = Vec::new();
        let mut ok = true;
        for c in p.connectors {
            let Some(facing) = parse_direction4(&c.facing) else {
                ok = false;
                break;
            };
            connectors.push(PieceConnector {
                du: c.du,
                dy: c.dy,
                dv: c.dv,
                kind: qualify(&modid, &c.kind),
                facing,
            });
        }
        if !ok {
            continue;
        }
        let cells = p
            .cells
            .into_iter()
            .map(|c| crate::world::template::TemplateCell {
                du: c.du,
                dy: c.dy,
                dv: c.dv,
                block: c.block,
            })
            .collect();
        let markers = p
            .markers
            .into_iter()
            .map(|m| PieceMarker {
                du: m.du,
                dy: m.dy,
                dv: m.dv,
                kind: qualify(&modid, &m.kind),
            })
            .collect();
        let chests = p
            .chests
            .into_iter()
            .filter_map(|c| {
                reg.loots
                    .contains_key(&qualify(&modid, &c.loot))
                    .then(|| PieceChest {
                        du: c.du,
                        dy: c.dy,
                        dv: c.dv,
                        loot: qualify(&modid, &c.loot),
                    })
            })
            .collect();
        reg.pieces.push(PieceDef {
            name: qualify(&modid, &p.id),
            cells,
            connectors,
            markers,
            chests,
            settlement_tier: p.settlement_tier.max(1),
        });
    }
    for (modid, pool) in pending_pools {
        let entries: Vec<PoolEntry> = pool
            .entries
            .into_iter()
            .filter_map(|e| {
                lookup_piece(&reg, &modid, &e.piece).map(|_| PoolEntry {
                    piece: qualify(&modid, &e.piece),
                    weight: e.weight.max(1),
                })
            })
            .collect();
        if !entries.is_empty() {
            reg.pools.push(PoolDef {
                id: qualify(&modid, &pool.id),
                entries,
            });
        }
    }
    for (modid, a) in pending_assemblies {
        let Some(entry_piece) = lookup_piece(&reg, &modid, &a.entry) else {
            continue;
        };
        let terrain = match a.terrain.as_deref() {
            Some("bury") => TerrainAdaptation::Bury,
            Some("encapsulate") => TerrainAdaptation::Encapsulate,
            _ => TerrainAdaptation::None,
        };
        let pools = a
            .pools
            .into_iter()
            .filter_map(|(kind, pool)| {
                let kind = qualify(&modid, &kind);
                let pool = qualify(&modid, &pool);
                reg.pools
                    .iter()
                    .any(|p| p.id == pool)
                    .then_some((kind, pool))
            })
            .collect();
        reg.assemblies.push(AssemblyDef {
            name: qualify(&modid, &a.id),
            biomes: a.biomes.iter().map(|b| b.to_lowercase()).collect(),
            rarity: a.rarity.max(1),
            entry_piece,
            pools,
            max_depth: a.max_depth.max(1),
            max_pieces: a.max_pieces.max(1),
            terrain,
            settlement: a.settlement.as_ref().map(|s| qualify(&modid, s)),
            dungeon: a.dungeon.as_ref().map(|d| DungeonDef {
                reset: d.reset.unwrap_or(60.0).max(1.0),
            }),
        });
    }
    // Settlements (spec 3.4): resolve tier lists and validate them. The
    // assembly->settlement wiring is checked against this list when
    // assemblies resolve; a piece tier is validated against its assembly's
    // settlement when pieces resolve (below, after settlements exist).
    // Settlement errors are collected locally: `validate_material_graph`
    // rebuilds `material_errors` from scratch at the end of build, so pushing
    // straight to it here would be wiped.
    let mut settlement_errors = Vec::new();
    // Recipe gate errors are collected locally too (spec 3.5): unknown
    // blueprint references must surface in `material_errors`, which
    // `validate_material_graph` rebuilds from scratch.
    let mut recipe_errors = Vec::new();
    for (modid, s) in pending_settlements {
        let id = qualify(&modid, &s.id);
        if reg.settlements.iter().any(|existing| existing.id == id) {
            settlement_errors.push(format!("{id}: duplicate settlement id"));
            continue;
        }
        let mut tiers: Vec<SettlementTier> = s
            .tiers
            .iter()
            .map(|t| SettlementTier {
                tier: t.tier.max(2),
                threshold: t.threshold,
            })
            .collect();
        tiers.sort_by_key(|t| t.tier);
        tiers.dedup_by_key(|t| t.tier);
        if !tiers.windows(2).all(|w| w[0].threshold < w[1].threshold) {
            settlement_errors.push(format!(
                "{id}: settlement tiers must have strictly increasing thresholds"
            ));
            continue;
        }
        // Capability E13: resolve the delivery needs against the roster.
        let mut needs: Vec<SettlementNeed> = Vec::new();
        for need in &s.need {
            let Some(item) = lookup_item(&reg, &modid, &need.item) else {
                settlement_errors.push(format!("{id}: need item {} does not resolve", need.item));
                continue;
            };
            needs.push(SettlementNeed {
                item,
                rep_per_unit: need.rep.unwrap_or(1).max(1),
            });
        }
        reg.settlements.push(SettlementDef {
            rep_key: s.rep_key.clone().unwrap_or_else(|| format!("rep_{id}")),
            id,
            tiers,
            needs,
        });
    }
    // Cross-validate settlement wiring (spec 3.4): every assembly that names
    // a settlement must resolve one, a piece tagged tier > 1 must be
    // reachable from a settlement assembly (a hidden tier that can never be
    // placed would silently never exist), and its tier must exist in that
    // settlement's declared tiers (else reveal could never happen).
    let mut assembly_settlements: Vec<Option<usize>> =
        reg.assemblies.iter().map(|_| None).collect();
    for (i, asm) in reg.assemblies.iter().enumerate() {
        if let Some(settlement) = &asm.settlement {
            match reg.settlements.iter().position(|s| &s.id == settlement) {
                Some(idx) => assembly_settlements[i] = Some(idx),
                None => settlement_errors.push(format!(
                    "{}: assembly names unknown settlement {settlement}",
                    asm.name
                )),
            }
        }
    }
    for piece in &reg.pieces {
        if piece.settlement_tier <= 1 {
            continue;
        }
        let mut reachable = false;
        for (i, asm) in reg.assemblies.iter().enumerate() {
            if assembly_settlements[i].is_none() {
                continue;
            }
            let in_pool = asm.pools.values().any(|pool| {
                reg.pools
                    .iter()
                    .find(|p| &p.id == pool)
                    .is_some_and(|p| p.entries.iter().any(|e| e.piece == piece.name))
            });
            if !in_pool {
                continue;
            }
            reachable = true;
            let settlement = assembly_settlements[i].expect("checked above");
            if !reg.settlements[settlement]
                .tiers
                .iter()
                .any(|t| t.tier == piece.settlement_tier)
            {
                settlement_errors.push(format!(
                    "{}: piece tier {} not declared in settlement {}",
                    piece.name, piece.settlement_tier, reg.settlements[settlement].id
                ));
            }
        }
        if !reachable {
            settlement_errors.push(format!(
                "{}: piece tagged settlement_tier {} but no settlement assembly reaches it",
                piece.name, piece.settlement_tier
            ));
        }
    }
    for pd in pending_drops {
        let d = match pd.rule.as_str() {
            "none" => None,
            "self" => {
                let name = reg.blocks[pd.block].name.clone();
                reg.item_id(&name).map(|i| (i, pd.count))
            }
            // Bare names qualify with the declaring mod, like every
            // other cross-reference field.
            other => lookup_item(&reg, &pd.modid, other).map(|i| (i, pd.count)),
        };
        reg.blocks[pd.block].drops = d;
    }
    // Ingredient helper shared by recipes/smelts/fuels.
    let resolve_ing = |reg: &Registry, modid: &str, name: &str| -> Option<Ingredient> {
        if let Some(tag) = name.strip_prefix('#') {
            reg.tags
                .get(&qualify(modid, tag))
                .filter(|l| !l.is_empty())
                .map(|l| Ingredient::Any(l.clone()))
        } else {
            lookup_item(reg, modid, name).map(Ingredient::One)
        }
    };
    for (modid, s) in pending_smelts {
        if let (Some(input), Some(output)) = (
            resolve_ing(&reg, &modid, &s.input),
            lookup_item(&reg, &modid, &s.output),
        ) {
            let spit = s.spit.as_ref().and_then(|sp| {
                lookup_item(&reg, &modid, &sp.item)
                    .map(|it| (it, sp.count.unwrap_or(1).clamp(1, 16)))
            });
            reg.smelts.push(SmeltDef {
                input,
                output,
                time: s.time.unwrap_or(8.0),
                spit,
                loss: s.loss.clone(),
            });
        }
    }
    for (modid, b) in pending_bloomeries {
        if let (Some(charge), Some(fuel), Some(bloom)) = (
            lookup_item(&reg, &modid, &b.charge),
            lookup_item(&reg, &modid, &b.fuel),
            lookup_item(&reg, &modid, &b.bloom),
        ) {
            reg.bloomery.push(BloomeryDef {
                charge,
                fuel,
                bloom,
            });
        }
    }
    for (modid, w) in pending_workeds {
        if let (Some(input), Some(output)) = (
            lookup_item(&reg, &modid, &w.input),
            lookup_item(&reg, &modid, &w.output),
        ) {
            reg.worked.push(WorkedDef {
                input,
                output,
                strikes: w.strikes.unwrap_or(3).max(1),
                station: w.station.clone().unwrap_or_else(|| "anvil".into()),
                needs_hammer: w.tool.as_deref().unwrap_or("hammer") == "hammer",
                count: w.count.unwrap_or(1).max(1),
                loss: w.loss.clone(),
            });
        }
    }
    for (modid, k) in pending_kilns {
        if let (Some(p), Some(g)) = (
            lookup_item(&reg, &modid, &k.powder),
            lookup_item(&reg, &modid, &k.glass),
        ) {
            reg.kiln.push(KilnDef {
                powder: p,
                glass: g,
                consumes: k.consumes,
            });
        }
    }
    for (modid, k) in pending_kiln_bases {
        if let (Some(sa), Some(fu), Some(cl)) = (
            lookup_item(&reg, &modid, &k.sand),
            lookup_item(&reg, &modid, &k.fuel),
            lookup_item(&reg, &modid, &k.clear),
        ) {
            reg.kiln_base = Some((sa, fu, cl));
        }
    }
    for (modid, f) in pending_fuels {
        if let Some(ing) = resolve_ing(&reg, &modid, &f.item) {
            reg.fuels.push((ing, f.burn, f.speed.unwrap_or(1.0)));
        }
    }
    let mut pending_prey: Vec<(usize, String, Vec<String>)> = Vec::new();
    for (modid, a, tile, head_tile, box_tiles, proj_tile, attack_proj_tiles) in pending_animals {
        if !a.prey.is_empty() {
            pending_prey.push((reg.animals.len(), modid.clone(), a.prey.clone()));
        }
        let full = qualify(&modid, &a.id);
        if reg.animals.iter().any(|x| x.name == full) {
            continue; // duplicate id — first wins, like blocks/items
        }
        let drops = a
            .drops
            .iter()
            .filter_map(|d| {
                lookup_item(&reg, &modid, &d.item)
                    .map(|i| (i, d.min.unwrap_or(1), d.max.unwrap_or(1)))
            })
            .collect();
        let mut model: Vec<ModelBox> = a
            .model
            .iter()
            .map(|(name, b)| ModelBox {
                name: name.clone(),
                size: b.size,
                at: b.at,
                tile: box_tiles.get(name).copied(),
            })
            .collect();
        if model.is_empty() {
            model = vec![
                ModelBox {
                    name: "body".into(),
                    size: [6.0, 6.0, 10.0],
                    at: [0.0, 7.0, 0.0],
                    tile: None,
                },
                ModelBox {
                    name: "head".into(),
                    size: [4.0, 4.0, 4.0],
                    at: [0.0, 11.0, -6.0],
                    tile: None,
                },
                ModelBox {
                    name: "leg".into(),
                    size: [2.0, 7.0, 2.0],
                    at: [2.0, 0.0, 3.0],
                    tile: None,
                },
            ];
        }
        model.sort_by(|a, b| a.name.cmp(&b.name));
        let mut half_w = 0.2f32;
        let mut height = 0.4f32;
        for b in &model {
            half_w = half_w
                .max((b.at[0].abs() + b.size[0] / 2.0) / 16.0)
                .max((b.at[2].abs() + b.size[2] / 2.0) / 16.0);
            height = height.max((b.at[1] + b.size[1]) / 16.0);
        }
        let winged = model.iter().any(|b| b.name.starts_with("wing"));
        let movement_swim = a.movement.as_deref() == Some("swim");
        let aquatic = movement_swim.then(|| {
            let mut habitat = AquaticHabitatDef::default();
            if let Some(configured) = &a.aquatic {
                habitat.temperature_c = configured.temperature_c.unwrap_or(habitat.temperature_c);
                habitat.depth_blocks = configured.depth_blocks.unwrap_or(habitat.depth_blocks);
                habitat.discharge = configured.discharge.unwrap_or(habitat.discharge);
                habitat.salinity = configured.salinity.unwrap_or(habitat.salinity);
            }
            habitat
        });
        let full = qualify(&modid, &a.id);
        let arcane = match arcane_def(a.arcane.as_ref(), &modid, &full, &reg.arcane_registry) {
            Ok(definition) => definition,
            Err(error) => {
                reg.arcane_errors.push(error);
                None
            }
        };
        reg.animals.push(AnimalDef {
            name: full,
            label: a.name.clone().unwrap_or_else(|| a.id.clone()),
            biomes: a.biomes.iter().map(|b| b.to_lowercase()).collect(),
            habitats: a.habitats.iter().map(|tag| tag.to_lowercase()).collect(),
            temperature_c: a.temperature_c,
            vegetation: a.vegetation,
            elevation: a.elevation,
            health: a.health.unwrap_or(8.0),
            speed: a.speed.unwrap_or(2.0),
            flee_range: a.flee_range.unwrap_or(6.0),
            group: a.group.unwrap_or([1, 2]),
            rarity: a.rarity.unwrap_or(6).max(1),
            tile,
            head_tile,
            sound_pitch: a.sound_pitch.unwrap_or(1.0),
            drops,
            model,
            half_w: half_w.min(0.45),
            height,
            hostile: a.hostile,
            attack: a.attack.unwrap_or(3.0),
            resistances: a
                .resist
                .as_ref()
                .map(ResistTomlList::resolved)
                .unwrap_or_default(),
            attacks: {
                let reach = half_w.min(0.45) + 0.9;
                let attack = a.attack.unwrap_or(3.0);
                let mut list: Vec<AttackDef> = Vec::with_capacity(a.attacks.len());
                for (atk, ptile) in a.attacks.iter().zip(&attack_proj_tiles) {
                    let kind = match atk.kind.as_str() {
                        "charge" => AttackKind::Charge,
                        "projectile" => AttackKind::Projectile,
                        _ => AttackKind::Melee,
                    };
                    list.push(AttackDef {
                        name: atk.name.clone().unwrap_or_else(|| atk.kind.clone()),
                        kind,
                        damage: atk.damage.unwrap_or(attack),
                        cooldown: atk.cooldown.unwrap_or(1.0),
                        range: atk.range.unwrap_or(match kind {
                            AttackKind::Projectile => 14.0,
                            _ => reach,
                        }),
                        damage_type: atk.damage_type.clone(),
                        projectile: atk.projectile.as_ref().map(|pr| ProjectileDef {
                            tile: ptile.unwrap_or(crate::atlas::UNKNOWN_SLOT),
                            damage: pr.damage,
                            damage_type: pr.damage_type.clone(),
                            speed: pr.speed.unwrap_or(14.0),
                            cooldown: pr.cooldown.unwrap_or(2.0),
                        }),
                    });
                }
                if list.is_empty() {
                    // Back-compat synthesis for the pre-spec 3.6 scalar
                    // fields. A legacy `projectile` becomes a ranged "cast"
                    // attack riding the projectile's own cooldown; every
                    // warden keeps the implicit melee `attack` scalar, so a
                    // caster still swings when the player closes in.
                    if let Some(pr) = a.projectile.as_ref() {
                        list.push(AttackDef {
                            name: "cast".into(),
                            kind: AttackKind::Projectile,
                            damage: pr.damage,
                            cooldown: pr.cooldown.unwrap_or(2.0),
                            range: 14.0,
                            damage_type: pr.damage_type.clone(),
                            projectile: Some(ProjectileDef {
                                tile: proj_tile.unwrap_or(crate::atlas::UNKNOWN_SLOT),
                                damage: pr.damage,
                                damage_type: pr.damage_type.clone(),
                                speed: pr.speed.unwrap_or(14.0),
                                cooldown: pr.cooldown.unwrap_or(2.0),
                            }),
                        });
                    }
                    list.push(AttackDef {
                        name: "melee".into(),
                        kind: AttackKind::Melee,
                        damage: attack,
                        cooldown: 1.0,
                        range: reach,
                        damage_type: None,
                        projectile: None,
                    });
                }
                list
            },
            behavior: match a.behavior.as_deref() {
                Some("brute") => BehaviorArchetype::Brute,
                Some("construct") => BehaviorArchetype::Construct,
                Some("builder") => BehaviorArchetype::Builder,
                Some("rusher") => BehaviorArchetype::Rusher,
                Some("tank") => BehaviorArchetype::Tank,
                Some("sniper") => BehaviorArchetype::Sniper,
                Some("support") => BehaviorArchetype::Support,
                Some("swarm") => BehaviorArchetype::Swarm,
                Some("controller") => BehaviorArchetype::Controller,
                Some("phaser") => BehaviorArchetype::Phaser,
                Some("shield_bearer") => BehaviorArchetype::ShieldBearer,
                _ => BehaviorArchetype::Standard,
            },
            archetype: ArchetypeParams {
                rusher: a.rusher.as_ref().map(|r| RusherDef {
                    rush_mult: r.rush_mult.unwrap_or(2.4),
                }),
                tank: a.tank.as_ref().map(|t| TankDef {
                    knockback_mult: t.knockback_mult.unwrap_or(0.25),
                }),
                sniper: a.sniper.as_ref().map(|s| SniperDef {
                    keep_min: s.keep_min.unwrap_or(9.0),
                    keep_max: s.keep_max.unwrap_or(16.0),
                }),
                support: a.support.as_ref().map(|s| SupportDef {
                    radius: s.radius.unwrap_or(10.0),
                    interval: s.interval.unwrap_or(6.0),
                    heal: s.heal.unwrap_or(2.0),
                }),
                swarm: a.swarm.as_ref().map(|s| SwarmDef {
                    spawn: qualify(&modid, s.spawn.as_deref().unwrap_or("")),
                    count: s.count.unwrap_or(3),
                }),
                controller: a.controller.as_ref().map(|c| ControllerDef {
                    spawn: qualify(&modid, c.spawn.as_deref().unwrap_or("")),
                    count: c.count.unwrap_or(2),
                    interval: c.interval.unwrap_or(12.0),
                    max: c.max.unwrap_or(6),
                }),
                phaser: a.phaser.as_ref().map(|p| PhaserDef {
                    blink_range: p.blink_range.unwrap_or(7.0),
                    blink_cd: p.blink_cd.unwrap_or(5.0),
                }),
                shield: a.shield.as_ref().map(|sh| ShieldDef {
                    front_mult: sh.front_mult.unwrap_or(0.35),
                    front_deg: sh.front_deg.unwrap_or(90.0),
                }),
            },
            builder: a.builder.as_ref().map(|b| BuilderDef {
                template: b.template.clone(),
                cap: b.cap.unwrap_or(8),
                interval: b.interval.unwrap_or(30.0),
            }),
            hack: a.hack.as_ref().map(|h| HackDef {
                tool: h.tool.clone(),
                drops: h
                    .drops
                    .iter()
                    .filter_map(|d| {
                        lookup_item(&reg, &modid, &d.item)
                            .map(|i| (i, d.min.unwrap_or(1), d.max.unwrap_or(1)))
                    })
                    .collect(),
            }),
            aggro_range: a.aggro_range.unwrap_or(12.0),
            ire_min: a.ire_min.unwrap_or(0.0),
            movement_float: a.movement.as_deref() == Some("float"),
            movement_swim,
            aquatic,
            winged,
            emissive: a.emissive,
            glow: a.glow,
            spawn_light_max: a.spawn_light_max.unwrap_or(3),
            breed_food: a
                .breed_food
                .as_ref()
                .and_then(|f| lookup_item(&reg, &modid, f)),
            carrier: a.carrier,
            vehicle: a.vehicle,
            belly_secs: a.belly.unwrap_or(0.0).max(0.0),
            grazes: a.grazes,
            prey: Vec::new(), // resolved after every species exists
            fierce: a.fierce,
            guards: a.guards,
            arcane,
            projectile: a.projectile.as_ref().map(|pr| ProjectileDef {
                tile: proj_tile.unwrap_or(crate::atlas::UNKNOWN_SLOT),
                damage: pr.damage,
                damage_type: pr.damage_type.clone(),
                speed: pr.speed.unwrap_or(14.0),
                cooldown: pr.cooldown.unwrap_or(2.0),
            }),
            npc: None,
        });
    }
    // Friendly NPCs (spec 3.1): each synthesizes a companion AnimalDef so
    // the whole mob pipeline (render, persist, network, raycast) treats it
    // as an ordinary species. The companion is non-hostile, never flees,
    // never tames, has no drops/belly/prey, and is never wildlife-spawned
    // (empty biomes). `AnimalDef.npc` points back to the NpcDef.
    for (modid, n, tile, head, box_tiles) in pending_npcs {
        let full = qualify(&modid, &n.id);
        if reg.npcs.iter().any(|x| x.name == full) {
            continue; // duplicate id — first wins, like blocks/items
        }
        let species = reg.animals.len();
        let model: Vec<ModelBox> = n
            .model
            .iter()
            .map(|(name, b)| ModelBox {
                name: name.clone(),
                size: b.size,
                at: b.at,
                tile: box_tiles.get(name).copied(),
            })
            .collect();
        let (model, half_w, height) = if model.is_empty() {
            // Default humanoid silhouette: a head, torso, and legs.
            let m = vec![
                ModelBox {
                    name: "head".into(),
                    size: [6.0, 6.0, 6.0],
                    at: [0.0, 22.0, 0.0],
                    tile: None,
                },
                ModelBox {
                    name: "body".into(),
                    size: [8.0, 10.0, 4.0],
                    at: [0.0, 12.0, 0.0],
                    tile: None,
                },
                ModelBox {
                    name: "leg".into(),
                    size: [3.0, 10.0, 3.0],
                    at: [1.5, 2.0, 0.0],
                    tile: None,
                },
            ];
            let mut half_w = 0.2f32;
            let mut height = 0.4f32;
            for b in &m {
                half_w = half_w
                    .max((b.at[0].abs() + b.size[0] / 2.0) / 16.0)
                    .max((b.at[2].abs() + b.size[2] / 2.0) / 16.0);
                height = height.max((b.at[1] + b.size[1]) / 16.0);
            }
            (m, half_w.min(0.45), height)
        } else {
            let mut half_w = 0.2f32;
            let mut height = 0.4f32;
            for b in &model {
                half_w = half_w
                    .max((b.at[0].abs() + b.size[0] / 2.0) / 16.0)
                    .max((b.at[2].abs() + b.size[2] / 2.0) / 16.0);
                height = height.max((b.at[1] + b.size[1]) / 16.0);
            }
            (model, half_w.min(0.45), height)
        };
        reg.animals.push(AnimalDef {
            name: format!("{full}#npc"),
            label: n.name.clone().unwrap_or_else(|| n.id.clone()),
            biomes: Vec::new(),
            habitats: Vec::new(),
            temperature_c: None,
            vegetation: None,
            elevation: None,
            health: 1000.0, // effectively unkillable this phase
            speed: 1.6,
            flee_range: 0.0,
            group: [1, 1],
            rarity: 1_000_000,
            tile,
            head_tile: head,
            sound_pitch: n.sound_pitch.unwrap_or(1.0),
            drops: Vec::new(),
            model,
            half_w,
            height,
            hostile: false,
            attack: 0.0,
            resistances: HashMap::new(),
            attacks: Vec::new(),
            behavior: BehaviorArchetype::Standard,
            builder: None,
            hack: None,
            archetype: ArchetypeParams::default(),
            aggro_range: 0.0,
            ire_min: 0.0,
            movement_float: false,
            movement_swim: false,
            aquatic: None,
            winged: false,
            emissive: false,
            glow: None,
            spawn_light_max: 0,
            breed_food: None,
            carrier: false,
            vehicle: false,
            belly_secs: 0.0,
            grazes: false,
            prey: Vec::new(),
            fierce: false,
            guards: false,
            arcane: None,
            projectile: None,
            npc: Some(reg.npcs.len()),
        });
        reg.npcs.push(NpcDef {
            name: full.clone(),
            label: n.name.clone().unwrap_or_else(|| n.id.clone()),
            dialogue: n
                .dialogue
                .as_ref()
                .map(|d| qualify(&modid, d))
                .or_else(|| n.dialogue.as_ref().cloned()),
            species,
            talk_radius: n.talk_radius.unwrap_or(3.0),
            patrol: n.patrol.clone(),
            pause: n.pause.unwrap_or(2.0),
            sound_pitch: n.sound_pitch.unwrap_or(1.0),
        });
    }
    // Dialogue and quest definitions resolve by name after every npc/item
    // exists; a bad reference drops the def and reports, never panics.
    for (modid, d) in pending_dialogues {
        let id = qualify(&modid, &d.id);
        if reg.dialogues.iter().any(|x| x.id == id) {
            continue;
        }
        reg.dialogues.push(DialogueDef {
            id,
            npc: d.npc.as_ref().map(|n| qualify(&modid, n)),
            root: d.root.clone(),
            nodes: d
                .nodes
                .iter()
                .map(|node| DialogueNode {
                    id: node.id.clone(),
                    text: node.text.clone(),
                    condition: node.condition.as_deref().map(ScriptHook::parse),
                    choices: node
                        .choices
                        .iter()
                        .map(|choice| DialogueChoice {
                            label: choice.label.clone(),
                            condition: choice.condition.as_deref().map(ScriptHook::parse),
                            callback: choice.callback.as_deref().map(ScriptHook::parse),
                            next: choice.next.clone(),
                        })
                        .collect(),
                })
                .collect(),
        });
    }
    for (modid, q) in pending_quests {
        let id = qualify(&modid, &q.id);
        if reg.quests.iter().any(|x| x.id == id) {
            continue;
        }
        let mut quest_errors: Vec<String> = Vec::new();
        let rewards: Vec<QuestReward> = q
            .rewards
            .iter()
            .filter_map(|reward| {
                if let Some(item) = &reward.item {
                    let iid = lookup_item(&reg, &modid, item)?;
                    Some(QuestReward::Give(iid, reward.count.max(1)))
                } else if let Some(settlement) = &reward.add_reputation {
                    let settlement_id = qualify(&modid, settlement);
                    if !reg.settlements.iter().any(|s| s.id == settlement_id) {
                        quest_errors.push(format!(
                            "{id}: quest rewards reputation for unknown settlement {settlement_id}"
                        ));
                        return None;
                    }
                    Some(QuestReward::Reputation(
                        settlement_id,
                        reward.rep_amount.max(1),
                    ))
                } else if let Some(recipe_out) = &reward.learn_recipe {
                    let recipe_id = qualify(&modid, recipe_out);
                    // The recipe id is its output item's qualified id; the
                    // recipe must be declared somewhere in this load.
                    let declared = pending_recipes
                        .iter()
                        .any(|(m, r)| qualify(m, &r.output) == recipe_id);
                    if !declared {
                        quest_errors
                            .push(format!("{id}: quest unlocks unknown recipe {recipe_id}"));
                        return None;
                    }
                    Some(QuestReward::LearnRecipe(recipe_id))
                } else {
                    reward.set_flag.as_ref().map(|flag| {
                        QuestReward::SetFlag(
                            flag.clone(),
                            reward.flag_value.clone().unwrap_or_else(|| "1".into()),
                        )
                    })
                }
            })
            .collect();
        settlement_errors.extend(quest_errors);
        reg.quests.push(QuestDef {
            id: id.clone(),
            title: q.title.clone(),
            description: q.description.clone(),
            giver: q.giver.as_ref().map(|g| qualify(&modid, g)),
            prereq: q.prereq.as_ref().map(|p| qualify(&modid, p)),
            objectives: q
                .objectives
                .iter()
                .map(|objective| QuestObjective {
                    key: objective.key.clone(),
                    description: objective.description.clone(),
                    count: objective.count.max(1),
                })
                .collect(),
            rewards,
        });
    }
    // Prey lists resolve after the whole roster exists (a fox may be
    // declared before the rabbit it hunts).
    for (hunter, modid, names) in pending_prey {
        let ids: Vec<usize> = names
            .iter()
            .filter_map(|n| {
                let q = qualify(&modid, n);
                reg.animal_id(&q).or_else(|| reg.animal_id(n))
            })
            .collect();
        reg.animals[hunter].prey = ids;
    }
    for (modid, block, h) in pending_harvests {
        let becomes = reg
            .block_id(&qualify(&modid, &h.becomes))
            .or_else(|| reg.block_id(&h.becomes));
        let item = lookup_item(&reg, &modid, &h.item);
        if let (Some(item), Some(becomes)) = (item, becomes) {
            reg.blocks[block.0 as usize].harvest = Some((item, h.count.unwrap_or(2), becomes));
        }
    }
    // Aliases: old name -> already-registered new id (lossless renames).
    for (modid, a) in pending_aliases {
        let new = qualify(&modid, &a.new);
        if let Some(id) = reg.block_by_name.get(&new).copied() {
            reg.block_by_name.entry(a.old.clone()).or_insert(id);
        }
        if let Some(id) = reg.item_by_name.get(&new).copied() {
            reg.item_by_name.entry(a.old.clone()).or_insert(id);
        }
    }
    // Item `places` links (food items that plant crops).
    for (modid, it_toml) in &pending_places {
        if let (Some(item), Some(block)) = (
            reg.item_id(&qualify(modid, &it_toml.0)),
            reg.block_id(&qualify(modid, &it_toml.1))
                .or_else(|| reg.block_id(&it_toml.1)),
        ) {
            let inherited_ecology = reg.block(block).arcane_ecology.clone();
            let item_name = reg.item(item).name.clone();
            let definition = &mut reg.items[item.0 as usize];
            definition.places = Some(block);
            if definition.arcane_ecology.is_none() {
                definition.arcane_ecology = inherited_ecology.clone();
            }
            if let Some(ecology) = inherited_ecology {
                reg.arcane_ecology.entry(item_name).or_insert(ecology);
            }
        }
    }
    // Recipes unlocked by a `learn_recipe` reward default their tech key to
    // `learned:<recipe_id>` (spec 3.5) when they don't declare an explicit
    // `tech`. The quests are parsed above, so their rewards are visible here.
    let learned_recipe_ids: std::collections::HashSet<&str> = reg
        .quests
        .iter()
        .flat_map(|q| &q.rewards)
        .filter_map(|reward| match reward {
            crate::registry::QuestReward::LearnRecipe(id) => Some(id.as_str()),
            _ => None,
        })
        .collect();
    for (modid, r) in pending_recipes {
        let h = r.pattern.len();
        let w = r
            .pattern
            .iter()
            .map(|s| s.chars().count())
            .max()
            .unwrap_or(0);
        if h == 0 || w == 0 || h > 3 || w > 3 {
            continue;
        }
        let mut pattern = vec![None; w * h];
        let mut ok = true;
        for (y, row) in r.pattern.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                if ch == '.' || ch == ' ' {
                    continue;
                }
                let key = ch.to_string();
                let Some(name) = r.keys.get(&key) else {
                    ok = false;
                    continue;
                };
                if let Some(tag) = name.strip_prefix('#') {
                    let tag_name = qualify(&modid, tag);
                    match reg.tags.get(&tag_name) {
                        Some(list) if !list.is_empty() => {
                            pattern[y * w + x] = Some(Ingredient::Any(list.clone()))
                        }
                        _ => ok = false,
                    }
                } else {
                    match lookup_item(&reg, &modid, name) {
                        Some(i) => pattern[y * w + x] = Some(Ingredient::One(i)),
                        None => ok = false,
                    }
                }
            }
        }
        let Some(out) = lookup_item(&reg, &modid, &r.output) else {
            continue;
        };
        let blueprint = match r.blueprint.as_deref() {
            Some(name) => match lookup_item(&reg, &modid, name) {
                Some(item) => Some(item),
                None => {
                    recipe_errors.push(format!("{}: recipe blueprint {name} is unknown", r.output));
                    continue;
                }
            },
            None => None,
        };
        if ok {
            let tech = r.tech.clone().or_else(|| {
                let recipe_id = reg.item(out).name.clone();
                learned_recipe_ids
                    .contains(recipe_id.as_str())
                    .then(|| format!("learned:{recipe_id}"))
            });
            reg.recipes.push(RecipeDef {
                w,
                h,
                pattern,
                output: out,
                count: r.count.unwrap_or(1),
                station: r.station.clone(),
                loss: r.loss.clone(),
                byproducts: r
                    .byproducts
                    .iter()
                    .filter_map(|byproduct| {
                        lookup_item(&reg, &modid, &byproduct.item)
                            .map(|item| (item, byproduct.count))
                    })
                    .collect(),
                tech,
                blueprint,
            });
        }
    }
    // Crop stages inherit their parent's drops (after drop resolution).
    for i in 0..reg.blocks.len() {
        if reg.blocks[i].name.contains("/stage") {
            let base = reg.blocks[i]
                .name
                .split("/stage")
                .next()
                .unwrap()
                .to_string();
            if let Some(pid) = reg.block_by_name.get(&base).copied() {
                reg.blocks[i].drops = reg.blocks[pid.0 as usize].drops;
            }
        }
    }
    // Gate errors are collected locally: `validate_material_graph` rebuilds
    // `material_errors` from scratch at the end of build, so pushing straight
    // to it here would be wiped.
    let mut gate_errors = Vec::new();
    for (modid, f) in pending_features {
        if f.r#type == "ore" {
            let lookup_block = |name: &str| {
                reg.block_id(&qualify(&modid, name))
                    .or_else(|| reg.block_id(name))
            };
            let (Some(block), Some(replaces)) = (
                lookup_block(&f.block),
                lookup_block(f.replaces.as_deref().unwrap_or("base:stone")),
            ) else {
                continue;
            };
            let [y0, y1] = f.y_range.unwrap_or([4, 60]);
            reg.ores.push(OreFeature {
                block,
                replaces,
                vein_size: f.vein_size.unwrap_or(5).clamp(1, 32),
                per_chunk: f.per_chunk.unwrap_or(6).clamp(0, 64),
                y_min: y0,
                y_max: y1,
                shape: match f.shape.as_deref() {
                    Some("seam") => VeinShape::Seam,
                    Some("streak") => VeinShape::Streak,
                    _ => VeinShape::Walk,
                },
                chance: f.chance.unwrap_or(1.0).clamp(0.0, 1.0),
                resource_key: reg.block(block).name.clone(),
                mod_id: modid.clone(),
                retrogen: reg
                    .mods
                    .iter()
                    .find(|info| info.id == modid)
                    .and_then(|info| info.retrogen)
                    .unwrap_or(RetrogenPolicy::NoRetrogen),
            });
        } else if f.r#type == "gate" {
            // Spec 2.5: a sealed block placed by `feature:<id>` markers,
            // locked until the player's KV flag reads `value`. An unknown
            // block fails the pack load (a sealed wall you can never open is
            // a silent softlock, unlike an unknown ore that just never grows).
            let Some(gate_id) = f.id.as_deref() else {
                gate_errors.push(format!("{modid}: gate feature missing `id`"));
                continue;
            };
            let id = qualify(&modid, gate_id);
            if reg.gates.iter().any(|g| g.id == id) {
                gate_errors.push(format!("{id}: duplicate gate feature id"));
                continue;
            }
            let Some(flag) = f.flag.clone() else {
                gate_errors.push(format!(
                    "{id}: gate feature missing `flag` (the KV key it unlocks on)"
                ));
                continue;
            };
            let lookup_block = |name: &str| {
                reg.block_id(&qualify(&modid, name))
                    .or_else(|| reg.block_id(name))
            };
            let Some(block) = lookup_block(&f.block) else {
                gate_errors.push(format!(
                    "{id}: gate feature references unknown block {:?}",
                    f.block
                ));
                continue;
            };
            let unlocked_block = f
                .unlocked_block
                .as_deref()
                .and_then(lookup_block)
                .or_else(|| {
                    if f.unlocked_block.as_deref() == Some("base:air") {
                        Some(AIR)
                    } else {
                        None
                    }
                });
            if f.unlocked_block.is_some() && unlocked_block.is_none() {
                gate_errors.push(format!(
                    "{id}: gate feature references unknown unlocked_block {:?}",
                    f.unlocked_block.as_deref().unwrap_or_default()
                ));
                continue;
            }
            reg.gates.push(GateDef {
                id,
                block,
                flag,
                value: f.value.clone().unwrap_or_else(|| "true".into()),
                unlocked_block,
                message: f
                    .message
                    .clone()
                    .unwrap_or_else(|| "It's locked tight.".into()),
                unbreakable_when_locked: f.unbreakable_when_locked.unwrap_or(true),
            });
        }
    }
    // Reverse block -> gate map, built after every gate resolves so a shared
    // sealed block can back multiple gates (last wins; authors should use one
    // block per gate unless they deliberately share).
    for (index, gate) in reg.gates.iter().enumerate() {
        reg.gate_for_block.insert(gate.block, index);
    }

    // Every block a builder cannot otherwise hold gets a creative-only
    // item: lava, fire, a heart, a crop mid-growth, a fluid at any
    // level. These never appear in survival, never craft, and never
    // count toward obtainability — they exist so the browser can offer
    // every state of every block the way a builder expects.
    let placeable: std::collections::HashSet<u16> = reg
        .items
        .iter()
        .filter_map(|i| i.places.map(|b| b.0))
        .collect();
    for bid in 0..reg.blocks.len() as u16 {
        if placeable.contains(&bid) || bid == AIR.0 {
            continue;
        }
        let d = &reg.blocks[bid as usize];
        let (name, label, icon) = (d.name.clone(), d.label.clone(), d.tiles[2]);
        // The placeholder block, and anything a pack has left without
        // art, would put a missing-texture tile in the browser.
        if icon == crate::atlas::UNKNOWN_SLOT {
            continue;
        }
        let iid = ItemId(reg.items.len() as u16);
        reg.items.push(ItemDef {
            name: format!("{name}/place"),
            label,
            icon,
            max_stack: if d.arcane.is_some() { 1 } else { 64 },
            tool: None,
            durability: 0,
            places: Some(BlockId(bid)),
            food: None,
            damage: 1.0,
            damage_type: None,
            bow: None,
            ammo: None,
            armor: None,
            carry_weight: 1,
            stats: Vec::new(),
            frame: None,
            component: None,
            bedroll: false,
            shears: false,
            charm: None,
            charm_def: None,
            wand_component: None,
            implement: None,
            tablet: false,
            striker: false,
            creative_only: true,
            brush_tool: false,
            throw_speed: None,
            hammer: false,
            hack: false,
            glow: None,
            materials: d.materials.clone(),
            materials_declared: !d.materials.is_empty(),
            material_class: d.material_class,
            salvage: None,
            broken_into: None,
            arcane: d.arcane.clone(),
            arcane_ecology: d.arcane_ecology.clone(),
            observation: d.observation.clone(),
            discovery: None,
        });
        reg.item_by_name.insert(format!("{name}/place"), iid);
    }

    // Preparations resolve after items so their physical solvent, ingredient,
    // vessel, residue, and output identities can all be proven. Invalid data
    // never installs a partial effect shell.
    for raw in &raws {
        for preparation in &raw.preparations {
            if reg.preparations.len() >= crate::alchemy::MAX_PREPARATION_DEFINITIONS {
                reg.arcane_errors.push(format!(
                    "{}: preparation registry exceeds its {}-definition safety bound",
                    raw.info.id,
                    crate::alchemy::MAX_PREPARATION_DEFINITIONS
                ));
                continue;
            }
            match crate::alchemy::PreparationDef::from_raw(&raw.info.id, preparation.clone()) {
                Ok(definition) => {
                    if reg.preparations.contains_key(&definition.id) {
                        reg.arcane_errors
                            .push(format!("{}: duplicate preparation identity", definition.id));
                    } else if let Err(error) = definition.validate_registry(&reg) {
                        reg.arcane_errors.push(error.to_string());
                    } else {
                        reg.preparations.insert(definition.id.clone(), definition);
                    }
                }
                Err(error) => reg.arcane_errors.push(error.to_string()),
            }
        }
    }

    // Named rulesets (capability E1): resolve `[[mode]]` base chains so a
    // world's `mode` string maps to a Ruleset via `ruleset_for`. A mode
    // whose base is undeclared or cyclic is recorded as a load error and
    // falls back to survival semantics.
    let mut mode_errors = Vec::new();
    {
        let mut pending: Vec<ModeDef> = Vec::new();
        for raw in &raws {
            for m in &raw.modes {
                let id = qualify(&raw.info.id, &m.id);
                if m.id == "survival" || m.id == "creative" {
                    mode_errors.push(format!("mode {id}: built-in mode id is reserved"));
                    continue;
                }
                if pending.iter().any(|p| p.id == id) {
                    mode_errors.push(format!("mode {id}: duplicate mode id"));
                    continue;
                }
                let base = m.base.as_deref().map(|b| {
                    if b == "survival" || b == "creative" {
                        b.to_string()
                    } else {
                        qualify(&raw.info.id, b)
                    }
                });
                pending.push(ModeDef {
                    id,
                    base,
                    creative: m.creative,
                    hunger: m.hunger,
                    fall_damage: m.fall_damage,
                    drowning: m.drowning,
                    lava_burn: m.lava_burn,
                    hostile_spawns: m.hostile_spawns,
                    ire: m.ire,
                    hearts: m.hearts,
                    weather_extremes: m.weather_extremes,
                    pvp: m.pvp,
                    skills: m.skills,
                    equipment: m.equipment,
                    industrial_ire: m.industrial_ire,
                    nest_spawns: m.nest_spawns,
                });
            }
        }
        for mode in &pending {
            let mut base = mode.base.clone().unwrap_or_else(|| "survival".into());
            let mut chain = vec![mode.id.clone()];
            // Chase the base chain to its root, cycle-guarded.
            while base != "survival" && base != "creative" {
                let Some(next) = pending.iter().find(|p| p.id == base) else {
                    mode_errors.push(format!(
                        "mode {}: base {base} is not a declared mode",
                        mode.id
                    ));
                    break;
                };
                if chain.contains(&next.id) {
                    mode_errors.push(format!(
                        "mode {}: cyclic base chain through {}",
                        mode.id, next.id
                    ));
                    break;
                }
                chain.push(next.id.clone());
                base = next.base.clone().unwrap_or_else(|| "survival".into());
            }
        }
        let modes = pending;
        reg.modes = modes;
    }

    // Capability E5: merge every mod's skill tree into the registry.
    // Failures surface as pack errors on the mods screen.
    let raw_skills: Vec<crate::skills::RawSkillToml> =
        raws.iter().filter_map(|raw| raw.skills.clone()).collect();
    match crate::skills::resolve(&raw_skills) {
        Ok(tree) => reg.skills = tree,
        Err(errors) => reg.material_errors.extend(errors),
    }

    // Capability E7: merge every mod's machine kinds into the registry, in
    // declaration order (base first, so kind 0 is a base machine).
    // Failures surface as pack errors on the mods screen.
    let raw_machines: Vec<(String, crate::machines::RawMachineToml)> = raws
        .iter()
        .filter_map(|raw| {
            raw.machines
                .clone()
                .map(|machines| (raw.info.id.clone(), machines))
        })
        .collect();
    let mut machine_errors = Vec::new();
    match crate::machines::resolve(&raw_machines) {
        Ok(machines) => reg.machines = machines,
        Err(errors) => machine_errors.extend(errors),
    }

    // Capability E9: resolve mods' nest spawn-gates after the block and
    // species rosters exist. Each `[[nest]]` names a block (its marker) and
    // a species; both must resolve or the nest is dropped with an error.
    let mut nest_errors = Vec::new();
    for (modid, nests) in raws
        .iter()
        .filter_map(|raw| raw.nests.clone().map(|nests| (raw.info.id.clone(), nests)))
    {
        for nest in &nests.nest {
            let full = qualify(&modid, &nest.id);
            let block = qualify(&modid, &nest.block);
            let species = qualify(&modid, &nest.species);
            let Some(block) = reg.block_id(&block).or_else(|| reg.block_id(&nest.block)) else {
                nest_errors.push(format!("nest {full}: unknown block {}", nest.block));
                continue;
            };
            let Some(species) = reg
                .animal_id(&species)
                .or_else(|| reg.animal_id(&nest.species))
            else {
                nest_errors.push(format!("nest {full}: unknown species {}", nest.species));
                continue;
            };
            reg.nests.push(NestDef {
                id: full,
                block,
                species,
                radius: nest.radius.unwrap_or(24.0),
                interval: nest.interval.unwrap_or(8.0),
                cap: nest.cap.unwrap_or(4),
            });
        }
    }

    reconcile_material_definitions(&mut reg);
    // Gate feature errors survive past `validate_material_graph`, which
    // rebuilds `material_errors` from scratch.
    reg.material_errors.extend(gate_errors);
    // Settlement, quest-reward, and recipe-gate errors, likewise collected
    // locally.
    reg.material_errors.extend(settlement_errors);
    reg.material_errors.extend(recipe_errors);
    reg.material_errors.extend(mode_errors);
    reg.material_errors.extend(machine_errors);
    reg.material_errors.extend(nest_errors);
    // Capability E11: merge every mod's screens into the registry, in
    // declaration order. Failures surface as pack errors on the mods
    // screen, collected locally because `validate_material_graph` rebuilds
    // `material_errors` from scratch.
    let raw_screens: Vec<(String, crate::screens::RawScreensToml)> = raws
        .iter()
        .filter_map(|raw| {
            raw.screens
                .clone()
                .map(|screens| (raw.info.id.clone(), screens))
        })
        .collect();
    match crate::screens::resolve(&raw_screens) {
        Ok(screens) => reg.screens = screens,
        Err(errors) => reg.material_errors.extend(errors),
    }
    reg.mods.append(&mut failed);
    reg
}

fn qualify(modid: &str, name: &str) -> String {
    if name.contains(':') {
        name.to_string()
    } else {
        format!("{modid}:{name}")
    }
}

/// Resolve a (possibly bare) piece reference to its qualified name if the
/// piece is registered. Used only by the pool/assembly resolver after all
/// pieces are loaded.
fn qualified_piece_id(reg: &Registry, modid: &str, name: &str) -> Option<String> {
    let id = qualify(modid, name);
    reg.pieces.iter().any(|p| p.name == id).then_some(id)
}

fn parse_direction4(name: &str) -> Option<crate::planet::Direction4> {
    use crate::planet::Direction4;
    match name {
        "east" => Some(Direction4::East),
        "north" => Some(Direction4::North),
        "west" => Some(Direction4::West),
        "south" => Some(Direction4::South),
        _ => None,
    }
}


#[cfg(test)]
mod arcane_schema_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn base_arcane_content_is_valid_and_single_instance() {
        let registry = load(Path::new("__no_arcane_schema_mods__"));
        assert!(
            registry.arcane_errors.is_empty(),
            "{}",
            registry.arcane_errors.join("\n")
        );
        assert!(
            registry
                .items
                .iter()
                .filter(|item| item.arcane.is_some())
                .all(|item| item.max_stack == 1)
        );
        for name in crate::arcane::BASE_RESONANCES {
            assert!(registry.arcane_registry.definitions.contains_key(name));
        }
        for name in ["base:plant_fiber", "base:living_wood"] {
            assert!(
                registry
                    .item(registry.item_id(name).unwrap())
                    .arcane
                    .is_none(),
                "ordinary renewable material {name} must remain stackable and uncharged"
            );
        }
        for name in [
            "base:thorn_fiber",
            "base:dryad_heartwood",
            "base:lantern_fungus",
        ] {
            let item = registry.item(registry.item_id(name).unwrap());
            assert!(item.arcane.is_some(), "{name} must be magical content");
            assert_eq!(item.max_stack, 1, "{name} must identify one charged owner");
        }
        let fungus = registry.block(registry.block_id("base:lantern_fungus").unwrap());
        assert!(fungus.arcane.is_some());
        assert!(
            registry
                .block(registry.block_id("base:jungle_bush").unwrap())
                .arcane
                .is_none()
        );
    }

    #[test]
    fn base_scars_cover_the_closed_lifecycle_and_removed_content_falls_back() {
        let registry = load(Path::new("__no_dross_scar_mods__"));
        assert!(registry.arcane_errors.is_empty());
        assert_eq!(
            registry
                .dross_scars
                .values()
                .filter(|definition| definition.provider == "base")
                .count(),
            crate::dross::ScarKind::ALL.len()
        );
        for kind in crate::dross::ScarKind::ALL {
            let fallback = registry
                .resolve_dross_scar("removed_provider:old_scar", kind)
                .expect("every climate kind has a safe base fallback");
            assert_eq!(fallback.kind, kind);
            assert_eq!(fallback.provider, "base");
        }
        let kind = crate::dross::ScarKind::WetFilm;
        let first = registry
            .select_dross_scar(
                kind,
                crate::dross::DrossCarrier::Water,
                crate::dross::DrossBand::Seep,
                &BTreeMap::new(),
                7,
            )
            .unwrap();
        let full = BTreeMap::from([(first.content_id.clone(), 1usize)]);
        assert!(
            registry
                .select_dross_scar(
                    kind,
                    crate::dross::DrossCarrier::Water,
                    crate::dross::DrossBand::Seep,
                    &full,
                    7,
                )
                .is_none(),
            "a definition's regional cap must not be bypassed by fallback selection"
        );
    }

    #[test]
    fn mod_scar_shell_loads_and_unsafe_lifecycles_fail_closed() {
        let root =
            std::env::temp_dir().join(format!("wildforge-dross-scar-mod-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let provider = root.join("safe_scar");
        std::fs::create_dir_all(provider.join("textures")).unwrap();
        std::fs::write(
            provider.join("mod.toml"),
            "id = \"safe_scar\"\nworld_api = 2\n",
        )
        .unwrap();
        std::fs::copy(
            Path::new("base/textures/cattail.png"),
            provider.join("textures/thread.png"),
        )
        .unwrap();
        let safe = r#"
[[block]]
id = "river_threads"
texture = "thread.png"
hardness = 0.2
solid = false
opaque = false
height = 0.08
drops = "base:scar_fragment"
item = false
observation = { categories = ["scar"], properties = ["dross", "resonance", "condition"] }
dross_scar = { kind = "wet_film", handler = "filament_growth", carriers = ["water"], min_band = "seep", status = "recovery_drag", activity = "animated_castoff", max_sites_per_region = 2 }
"#;
        std::fs::write(provider.join("blocks.toml"), safe).unwrap();
        let registry = load(&root);
        assert!(
            registry.arcane_errors.is_empty(),
            "{}",
            registry.arcane_errors.join("\n")
        );
        let definition = registry.dross_scars.get("safe_scar:river_threads").unwrap();
        assert_eq!(definition.max_sites_per_region, 2);
        assert_eq!(
            definition.handler,
            crate::dross::ScarHandler::FilamentGrowth
        );

        let unsafe_provider = root.join("unsafe_scar");
        std::fs::create_dir_all(unsafe_provider.join("textures")).unwrap();
        std::fs::write(
            unsafe_provider.join("mod.toml"),
            "id = \"unsafe_scar\"\nworld_api = 2\n",
        )
        .unwrap();
        std::fs::copy(
            Path::new("base/textures/cattail.png"),
            unsafe_provider.join("textures/thread.png"),
        )
        .unwrap();
        std::fs::write(
            unsafe_provider.join("blocks.toml"),
            safe.replace("id = \"river_threads\"", "id = \"bad_threads\"")
                .replace("solid = false", "solid = true")
                .replace("max_sites_per_region = 2", "max_sites_per_region = 255"),
        )
        .unwrap();
        let rejected = load(&root);
        assert!(rejected.arcane_errors.iter().any(|error| {
            error.contains("unsafe_scar:bad_threads")
                && (error.contains("sites per region") || error.contains("nonstructural"))
        }));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn transformation_graph_rejects_unbacked_charged_outputs() {
        let mut registry = load(Path::new("__no_arcane_output_mods__"));
        assert!(registry.arcane_errors.is_empty());
        registry.recipes.push(RecipeDef {
            w: 1,
            h: 1,
            pattern: vec![Some(Ingredient::One(
                registry.item_id("base:plant_fiber").unwrap(),
            ))],
            output: registry.item_id("base:ember").unwrap(),
            count: 1,
            station: None,
            loss: MaterialVector::new(),
            byproducts: Vec::new(),
            tech: None,
            blueprint: None,
        });
        validate_arcane_graph(&mut registry);
        assert!(
            registry
                .arcane_errors
                .iter()
                .any(|error| error.contains("recipe") && error.contains("base:ember")),
            "{:?}",
            registry.arcane_errors
        );
    }

    #[test]
    fn schema_rejects_unknown_zero_and_overflowing_resonances() {
        let registry = crate::arcane::ResonanceRegistry::base();
        let unknown = ArcaneContentToml {
            capacity: 1,
            conductivity: 1,
            stability: 1,
            resonance: BTreeMap::from([("missing".into(), 1)]),
            on_destroy: ArcaneDisposition::Ambient,
        };
        assert!(
            arcane_def(Some(&unknown), "fixture", "fixture:item", &registry)
                .unwrap_err()
                .contains("unknown resonance")
        );

        let zero = ArcaneContentToml {
            capacity: 1,
            conductivity: 1,
            stability: 1,
            resonance: BTreeMap::from([("base:root".into(), 0)]),
            on_destroy: ArcaneDisposition::Ambient,
        };
        assert!(
            arcane_def(Some(&zero), "fixture", "fixture:item", &registry)
                .unwrap_err()
                .contains("zero weight")
        );

        let too_wide = ArcaneContentToml {
            capacity: 1,
            conductivity: 1_001,
            stability: 1,
            resonance: BTreeMap::from([("base:root".into(), 1)]),
            on_destroy: ArcaneDisposition::Ambient,
        };
        assert!(
            arcane_def(Some(&too_wide), "fixture", "fixture:item", &registry)
                .unwrap_err()
                .contains("0..=1000")
        );
    }

    #[test]
    fn destruction_policy_is_mandatory_and_integer_overflow_is_actionable() {
        let missing = toml::from_str::<ArcaneContentToml>(
            "capacity=1\nconductivity=1\nstability=1\nresonance={root=1}",
        )
        .unwrap_err()
        .to_string();
        assert!(missing.contains("on_destroy"));
        let overflow = toml::from_str::<ArcaneContentToml>(
            "capacity=18446744073709551616\nconductivity=1\nstability=1\nresonance={root=1}\non_destroy='ambient'",
        )
        .unwrap_err()
        .to_string();
        assert!(overflow.contains("number") || overflow.contains("u64"));
    }
}

#[cfg(test)]
mod npc_spec_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn base_npc_synthesizes_a_companion_species() {
        let registry = load(Path::new("__no_npc_mods__"));
        assert!(registry.arcane_errors.is_empty());
        let npc_id = registry
            .npc_id("base:elder")
            .expect("base elder npc must load");
        let npc = &registry.npcs[npc_id];
        assert_eq!(npc.name, "base:elder");
        assert_eq!(npc.label, "Elder Rowan");
        assert_eq!(npc.talk_radius, 3.0);
        assert!(npc.dialogue.as_deref() == Some("base:elder"));
        // Companion species exists, is not wildlife, and points back.
        assert!(registry.is_npc_species(npc.species));
        let companion = &registry.animals[npc.species];
        assert_eq!(companion.npc, Some(npc_id));
        assert!(!companion.hostile);
        assert!(companion.biomes.is_empty(), "NPCs never spawn as wildlife");
        assert!(companion.drops.is_empty());
    }

    #[test]
    fn base_dialogue_tree_parses_and_links_choices() {
        let registry = load(Path::new("__no_npc_mods__"));
        let d = registry
            .dialogues
            .iter()
            .find(|d| d.id == "base:elder")
            .expect("base elder dialogue must load");
        assert_eq!(d.root, "welcome");
        let root = d
            .nodes
            .iter()
            .find(|n| n.id == "welcome")
            .expect("root node exists");
        assert!(!root.text.is_empty());
        assert_eq!(root.choices.len(), 2);
        let ores = root
            .choices
            .iter()
            .find(|c| c.next.as_deref() == Some("ores"))
            .expect("ores choice links forward");
        assert_eq!(ores.label, "Ask about the ores");
        let ores_node = d
            .nodes
            .iter()
            .find(|n| n.id == "ores")
            .expect("ores node exists");
        let accept = ores_node
            .choices
            .iter()
            .find(|c| c.callback.is_some())
            .expect("accept choice runs a callback");
        assert_eq!(
            accept.callback.as_ref().unwrap(),
            &ScriptHook {
                mod_id: "base".into(),
                fn_name: "accept_cerium_quest".into(),
            }
        );
    }

    #[test]
    fn base_quest_definitions_resolve_rewards() {
        let registry = load(Path::new("__no_npc_mods__"));
        let quest = registry
            .quests
            .iter()
            .find(|q| q.id == "base:elder_cerium")
            .expect("base elder_cerium quest must load");
        assert_eq!(quest.giver.as_deref(), Some("base:elder"));
        assert_eq!(quest.objectives.len(), 1);
        assert_eq!(quest.objectives[0].key, "cerium_shards");
        assert_eq!(quest.objectives[0].count, 6);
        assert_eq!(quest.rewards.len(), 2);
        let give = quest
            .rewards
            .iter()
            .find(|r| matches!(r, QuestReward::Give(..)))
            .expect("item reward present");
        if let QuestReward::Give(item, count) = give {
            assert_eq!(registry.item(*item).name, "base:amethyst_shard");
            assert_eq!(*count, 2);
        }
        assert!(quest
            .rewards
            .iter()
            .any(|r| matches!(r, QuestReward::SetFlag(flag, v) if flag == "elder_told_tales" && v == "true")));
    }
}
