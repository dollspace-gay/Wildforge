//! Dynamic block/item/recipe registries — the foundation of the mod system.
//! Vanilla content is the built-in `base` mod, registered through the same
//! TOML path external mods use.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

mod blocks;
#[path = "registry/runtime.rs"]
mod runtime;
pub use blocks::{AIR, BlockDef, BlockId};
mod content_magic;
pub use content_magic::{
    ArcaneContentDef, ArcaneDisposition, ArcaneEcologyDef, ArcaneEcologyKind, ArcaneSiteRule,
    DiscoveryFixtureDef, DiscoveryItemDef, EcologyHarvestClass, EcologyRole, EcologySource,
    MaterialClass, MaterialVector, ObservationDef, ReproductionMode, RetrogenPolicy, SalvageDef,
};
mod items;
pub use items::{ArmorSlot, BowDef, FoodDef, ItemDef, ItemId, NUTRIENTS, ToolKind};
mod fauna;
pub use fauna::{
    AnimalDef, AquaticHabitatDef, ArchetypeParams, AttackDef, AttackKind, BehaviorArchetype,
    BuilderDef, ControllerDef, HackDef, ModelBox, NestDef, PhaserDef, ProjectileDef, RawNestToml,
    RusherDef, ShieldDef, SniperDef, SupportDef, SwarmDef, TankDef,
};
mod narrative;
pub use narrative::{
    DialogueChoice, DialogueDef, DialogueNode, GateDef, NpcDef, QuestDef, QuestObjective,
    QuestReward, ScriptHook, SettlementDef, SettlementNeed, SettlementTier,
};
mod recipes;
pub use recipes::{
    BloomeryDef, ForgeSalvageDef, Ingredient, KilnDef, RecipeDef, SmeltDef, WorkedDef,
};
mod structures;
pub use structures::{
    AssemblyDef, DungeonDef, LootEntry, OreFeature, PieceChest, PieceConnector, PieceDef,
    PieceMarker, PoolDef, PoolEntry, StructureDef, TerrainAdaptation, VeinShape,
};
mod assets;
mod loading;
mod placeholders;
mod policy;
mod publication;
mod reading;
mod schema;
pub use publication::{ContentErrors, load_validated};
mod linking;
pub use loading::load;
pub const WORLD_API_VERSION: u32 = 2;
mod arcane_validation;
mod ecology_validation;
mod material_graph;
mod salvage;

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
    /// Shared lifetime of a private transferred asset tree, when present.
    asset_snapshot: Option<std::sync::Arc<crate::content_files::AssetSnapshot>>,
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

fn qualify(modid: &str, name: &str) -> String {
    if name.contains(':') {
        name.to_string()
    } else {
        format!("{modid}:{name}")
    }
}

#[cfg(test)]
mod arcane_schema_tests;
#[cfg(test)]
mod npc_spec_tests;
