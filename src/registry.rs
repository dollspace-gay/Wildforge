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
mod policy;
mod placeholders;
mod schema;
mod loading;
mod linking;
use linking::{arcane_def};
pub use loading::load;
pub const WORLD_API_VERSION: u32 = 2;
pub use schema::NestFileToml;
use schema::{RawMod, ResistTomlList};
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

    let linking::PendingContent {
        drops: pending_drops,
        recipes: pending_recipes,
        features: pending_features,
        tags: pending_tags,
        smelts: pending_smelts,
        bloomeries: pending_bloomeries,
        workeds: pending_workeds,
        kilns: pending_kilns,
        kiln_bases: pending_kiln_bases,
        fuels: pending_fuels,
        aliases: pending_aliases,
        harvests: pending_harvests,
        bonus: pending_bonus,
        brush: pending_brush,
        structs: pending_structs,
        loots: pending_loots,
        pieces: pending_pieces,
        pools: pending_pools,
        assemblies: pending_assemblies,
        places: pending_places,
        animals: pending_animals,
        npcs: pending_npcs,
        dialogues: pending_dialogues,
        quests: pending_quests,
        settlements: pending_settlements,
    } = linking::register(&mut reg, &raws);

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
    for linking::PendingAnimal { modid, definition: a, tile, head_tile, box_tiles, proj_tile, attack_proj_tiles } in pending_animals {
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
    for linking::PendingNpc { modid, definition: n, tile, head, box_tiles } in pending_npcs {
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
    use super::schema::ArcaneContentToml;
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
