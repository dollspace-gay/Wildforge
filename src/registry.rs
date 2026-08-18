//! Dynamic block/item/recipe registries — the foundation of the mod system.
//! Vanilla content is the built-in `base` mod, registered through the same
//! TOML path external mods use.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[path = "registry/runtime.rs"]
mod runtime;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BlockId(pub u16);
pub const AIR: BlockId = BlockId(0);

/// Economically meaningful material classes. Every content definition has
/// one, even when it does not participate in the exact finite-material
/// ledger. This makes omissions visible to tools and mods instead of letting
/// "unclassified" become an accidental sixth class.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialClass {
    Renewable,
    GeologicallyFinite,
    TransformativeFinite,
    Consumptive,
    Exceptional,
}

pub type MaterialVector = BTreeMap<String, u64>;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArcaneDisposition {
    Ambient,
    Dross,
    Scar,
}

/// Declarative magic behavior shared by blocks, items, plants, minerals, and
/// creatures. All ratios are integer permille; content cannot smuggle NaN or
/// platform-dependent rounding into authoritative accounting.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArcaneContentDef {
    pub capacity: u64,
    pub conductivity_permille: u16,
    pub stability_permille: u16,
    pub resonance: BTreeMap<String, u16>,
    pub on_destroy: ArcaneDisposition,
}

/// Player-visible qualitative facets a tuning lens may report.  These names
/// are data ABI: records retain them when a provider is removed, while the
/// engine refuses definitions that ask to expose exact or private state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservationDef {
    pub categories: Vec<String>,
    pub properties: Vec<String>,
}

/// Physical knowledge behavior for an item. `evidence_class` is intentionally
/// string-addressed so a mod can add archaeology without an engine enum; the
/// action-bearing `kind` remains a small, validated vocabulary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiscoveryItemDef {
    pub kind: String,
    pub evidence_class: Option<String>,
    pub authored_text: Vec<String>,
    pub calibration: Option<crate::discovery::CalibrationGrade>,
    pub experiment: Option<crate::discovery::ExperimentKind>,
}

/// A placed discovery fixture. Experiments are explicit capabilities rather
/// than callbacks, keeping host authority and conservation in engine code.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiscoveryFixtureDef {
    pub kind: String,
    pub experiments: Vec<crate::discovery::ExperimentKind>,
    pub record_capacity: u16,
}

/// The causal job an organism or formation performs in the Current cycle.
/// These are data identities (and therefore pack/mod ABI), not flavor tags.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EcologyRole {
    Gatherer,
    Reservoir,
    Conductor,
    Transformer,
    Indicator,
    Parasite,
    Stabilizer,
    Catalyst,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArcaneEcologyKind {
    Organism,
    Crystal,
    FiniteMineral,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EcologySource {
    Ambient,
    Dross,
    Heart,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReproductionMode {
    Seed,
    Spore,
    Runner,
    Bud,
    None,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EcologyHarvestClass {
    Fruit,
    Prune,
    Coppice,
    Spore,
    SeedPreserving,
    Destructive,
}

/// Validated, deterministic lifecycle parameters shared by base content and
/// mods. Integer units keep the unloaded simulation bit-identical on every
/// platform. Water uses hydrology units (HU), nutrients are a compact local
/// ecological pool, and Current uses the arcane ledger's integer unit.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArcaneEcologyDef {
    pub roles: Vec<EcologyRole>,
    pub kind: ArcaneEcologyKind,
    pub habitat: Vec<String>,
    pub charge_capacity: u64,
    pub uptake_per_day: u32,
    pub release_per_day: u32,
    pub source: EcologySource,
    pub resonance: BTreeMap<String, u16>,
    pub dross_tolerance: u32,
    pub water_per_day_hu: u32,
    pub nutrient_per_day: u16,
    pub reproduction: ReproductionMode,
    /// Local astronomical seasons: spring, summer, autumn, winter.
    pub seasons: [bool; 4],
    pub carrying_capacity: u16,
    pub harvest: EcologyHarvestClass,
    pub regrowth_days: u16,
    pub min_stability_permille: u16,
    pub max_stability_permille: u16,
    pub min_richness_permille: u16,
    /// Crystal-only number of exact charge stages; zero for other kinds.
    pub crystal_stages: u8,
    /// Crystal-only tool tier that preserves the persistent bud.
    pub preserving_tool_tier: u8,
}

/// Bounded creation-time predicate for atlas-backed magical geography.
/// Mods describe causes; they never receive a mutable per-cell callback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArcaneSiteRule {
    pub id: String,
    pub provider: String,
    pub requires: Vec<String>,
    pub capacity_factor_permille: u16,
    pub base_resonance_bias: [u16; 6],
    pub rarity_per_million: u32,
    pub radius_cells: u16,
    pub retrogen: RetrogenPolicy,
}

/// How much useful material a workshop can recover from an object. Values
/// are integer permille so persistence and validation never depend on float
/// rounding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalvageDef {
    pub station: String,
    pub recovery_permille: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RetrogenPolicy {
    UntouchedHostOnly,
    SecondaryRecovery,
    WorldEvent,
    NoRetrogen,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ItemId(pub u16);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolKind {
    Pickaxe,
    Axe,
    Shovel,
    Hoe,
}

#[derive(Clone, Debug)]
pub struct BlockDef {
    pub name: String,  // "base:stone"
    pub label: String, // "Stone"
    /// Atlas slots per face: +X -X +Y -Y +Z -Z.
    pub tiles: [u16; 6],
    pub hardness: Option<f32>,
    pub tool: Option<ToolKind>,
    pub requires_tool: bool,
    /// Resolved drop (item, count); None = drops nothing.
    pub drops: Option<(ItemId, u32)>,
    pub solid: bool,
    pub opaque: bool,
    /// Right-click behavior: "crafting", "furnace", ...
    pub interaction: Option<String>,
    /// Minimum tool tier for drops when requires_tool is set.
    pub min_tier: u8,
    /// 0 = fluid source, 1..=7 flowing levels. None = not a fluid.
    pub water_level: Option<u8>,
    /// True for the lava chain (water_level then means lava volume).
    pub lava: bool,
    /// Render as two crossed quads instead of a cube (plants).
    pub cross: bool,
    /// How readily this block takes fire, 0 = never. Higher catches
    /// sooner. Wood, leaf and stalk burn; stone, earth and glass do not.
    pub burns: u8,
    /// Stands with nothing beneath it. Fire is the one cross block
    /// that is not a plant: it climbs, and the support rule that keeps
    /// torches honest was knocking out every flame the one below it
    /// had just lit.
    pub floats: bool,
    /// Custom mesh: "obelisk" (tapered pillar) or "signboard"
    /// (board on a post). Render-only; collision stays the cube.
    pub shape: Option<String>,
    /// Crop: (final stage block advances no further). tick advances stages.
    pub crop_next: Option<BlockId>,
    pub crop_chance: f32,
    pub crop_any_soil: bool,
    /// Right-click harvest: (item, count, block it becomes).
    pub harvest: Option<(ItemId, u32, BlockId)>,
    /// Emitted light 0..15 (torches, glowing mod blocks).
    pub light_emit: u8,
    /// Grows into this tree species on random ticks (saplings).
    pub sapling: Option<String>,
    /// Extra chance drop on break: (item, probability).
    pub bonus_drop: Option<(ItemId, f32)>,
    /// Archaeology: (loot table, block it becomes) when brushed.
    pub brush: Option<(String, BlockId)>,
    /// Rendered height 0..1; None = full cube (snow layers are 0.125).
    pub height: Option<f32>,
    /// Falls when unsupported.
    pub falls: bool,
    /// Glazing: a glass roof is a greenhouse.
    pub glass: bool,
    /// Per-channel block-light pass-through (stained glass).
    pub light_filter: [bool; 3],
    /// Per-channel emission (r,g,b), each 0..15. The brightest channel equals
    /// `light_emit`, so a colored light keeps its intensity; the dimmer
    /// channels fall off sooner, warming/cooling the glow with distance.
    pub light_rgb: [u8; 3],
    /// Soil: top-face tiles by fertility quartile (dust, poor, normal,
    /// rich) — the mesher reads the block's meta byte to pick one.
    pub fert_tiles: Option<[u16; 4]>,
    /// Crop rotation family (1..=3); 0 = not a crop. Stamped into the
    /// soil at maturation so monoculture drains harder than rotation.
    pub crop_family: u8,
    pub material_class: MaterialClass,
    pub materials: MaterialVector,
    pub dismantles_to: Option<ItemId>,
    /// Pattern A stat contribution this block lends a matched multiblock
    /// shell. Base firebrick is 1; an "advanced" tier raises it. Folding
    /// is pure data: the sum of the matched shell's cells (see
    /// [`crate::world::multiblock::fold_stats`]), never dispatched per
    /// machine.
    pub heat_retention: u32,
    pub arcane: Option<ArcaneContentDef>,
    pub arcane_ecology: Option<ArcaneEcologyDef>,
    pub observation: Option<ObservationDef>,
    pub discovery_fixture: Option<DiscoveryFixtureDef>,
}

/// Resolve a block's per-channel emission from its level and optional color.
/// The color is hue-normalized so the brightest channel always reaches the
/// full level (preserving the scalar light contract the world/tests rely on).
fn resolve_light_rgb(level: u8, color: Option<[f32; 3]>) -> [u8; 3] {
    match color {
        None => [level, level, level],
        Some(c) => {
            let m = c[0].max(c[1]).max(c[2]).max(1e-3);
            let ch = |v: f32| ((level as f32) * (v / m).clamp(0.0, 1.0)).round() as u8;
            [ch(c[0]), ch(c[1]), ch(c[2])]
        }
    }
}

pub const NUTRIENTS: [&str; 5] = ["grain", "vegetable", "fruit", "fungi", "protein"];

#[derive(Clone, Debug)]
pub struct FoodDef {
    pub hunger: f32,
    pub eat_time: f32,
    pub nutrition: [f32; 5],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArmorSlot {
    Head = 0,
    Chest = 1,
    Legs = 2,
    Feet = 3,
}

impl ArmorSlot {
    pub fn parse(s: &str) -> Option<ArmorSlot> {
        match s {
            "head" => Some(ArmorSlot::Head),
            "chest" => Some(ArmorSlot::Chest),
            "legs" => Some(ArmorSlot::Legs),
            "feet" => Some(ArmorSlot::Feet),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BowDef {
    /// Damage at full charge (half-hearts).
    pub damage: f32,
    /// Arrow velocity at full charge.
    pub speed: f32,
}

#[derive(Clone, Debug)]
pub struct ItemDef {
    pub name: String,
    pub label: String,
    pub icon: u16,
    pub max_stack: u32,
    /// (kind, speed multiplier on matching blocks, tier)
    pub tool: Option<(ToolKind, f32, u8)>,
    pub durability: u32,
    /// Placing this item puts down this block.
    pub places: Option<BlockId>,
    pub food: Option<FoodDef>,
    /// Attack damage in half-hearts (swords set it high; tools get a
    /// modest implicit value, bare items 1).
    pub damage: f32,
    /// Damage class wielded against the wild ("pierce", "blunt", "fire"...).
    pub damage_type: Option<String>,
    pub bow: Option<BowDef>,
    /// Ammo class this item belongs to ("arrow"); bows consume it.
    pub ammo: Option<String>,
    /// (slot, armor points) — each point blocks 4% damage from the wild.
    pub armor: Option<(ArmorSlot, u32)>,
    /// Right-click to camp: sleep to dawn, set spawn (bedrolls).
    pub bedroll: bool,
    /// Breaking leaves with this drops the leaf block itself.
    pub shears: bool,
    /// Passive charm effect: "quiet" | "bark" | "hunger" (one charm slot).
    pub charm: Option<String>,
    /// Bounded authoritative charm behavior. Legacy string declarations are
    /// upgraded into this form during registry load.
    pub charm_def: Option<crate::implements::CharmDef>,
    /// One physical role in a component-built wand.
    pub wand_component: Option<crate::implements::WandComponentDef>,
    /// A finished implement shell whose per-instance state lives in the
    /// world's implements sidecar.
    pub implement: Option<crate::implements::ImplementItemDef>,
    /// Right-click reads a line from the lost takers.
    pub tablet: bool,
    /// Right-click to set light to something. The one place a fire
    /// is marked as a player's.
    pub striker: bool,
    /// Reachable only from the creative browser. Every block gets one
    /// of these so a builder can place lava, fire, a heart, a crop
    /// mid-growth or a fluid at any level — the states a survival
    /// player meets in the world but can never hold.
    pub creative_only: bool,
    /// Sweeps remnant blocks (archaeology).
    pub brush_tool: bool,
    /// Right-click throw speed (None = not throwable).
    pub throw_speed: Option<f32>,
    /// Carried-light color x intensity (a glowing item that isn't a
    /// placeable lamp, e.g. a raw ember). Placeable emitters glow
    /// automatically from their block's light.
    pub glow: Option<[f32; 3]>,
    /// Works blooms on an anvil.
    pub hammer: bool,
    /// Right-click disables (hacks) a construct instead of destroying it.
    pub hack: bool,
    /// Recoverable finite constituents per item, in canonical integer units.
    pub materials: MaterialVector,
    /// True only when the content file fixes the vector. Derived vectors may
    /// be recomputed as upstream recipe identities reach their fixed point.
    pub materials_declared: bool,
    pub material_class: MaterialClass,
    pub salvage: Option<SalvageDef>,
    /// A zero-durability finite item changes identity instead of vanishing.
    pub broken_into: Option<ItemId>,
    pub arcane: Option<ArcaneContentDef>,
    pub arcane_ecology: Option<ArcaneEcologyDef>,
    pub observation: Option<ObservationDef>,
    pub discovery: Option<DiscoveryItemDef>,
}

/// One box of an animal's model. Sizes/offsets in px (16 px = 1 block);
/// `at` is (center x, bottom y, center z). A box named "leg" is mirrored
/// into four legs at (±x, y, ±z) by the renderer.
#[derive(Clone, Debug)]
pub struct ModelBox {
    pub name: String,
    pub size: [f32; 3],
    pub at: [f32; 3],
    /// Explicit texture for this box (e.g. bone antlers); None = fur.
    pub tile: Option<u16>,
}

#[derive(Clone, Debug)]
pub struct ProjectileDef {
    pub tile: u16,
    pub damage: f32,
    pub damage_type: Option<String>,
    pub speed: f32,
    pub cooldown: f32,
}

/// What an attack does once its range condition trips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackKind {
    /// Instant contact swing (the standard warden melee).
    Melee,
    /// Wind up briefly, then dash in a straight line until impact.
    Charge,
    /// Loose a bolt at the target (reuses [`ProjectileDef`]).
    Projectile,
}

/// One entry in a species' attack wheel (spec 3.6). `attacks` empty on an
/// animal synthesizes the implicit single melee from the `attack` scalar.
#[derive(Clone, Debug)]
pub struct AttackDef {
    /// Name shown in hit logs and matched by script hooks ("melee",
    /// "lunge", "ember").
    pub name: String,
    pub kind: AttackKind,
    /// Half-hearts dealt on contact / per bolt.
    pub damage: f32,
    /// Seconds between uses of this attack.
    pub cooldown: f32,
    /// Trigger distance to the target (melee/charge); for projectile
    /// attacks this is the cast trigger range.
    pub range: f32,
    /// Damage class the attack's strikes carry.
    pub damage_type: Option<String>,
    /// Bolt for projectile attacks.
    pub projectile: Option<ProjectileDef>,
}

/// A warden's behavioral archetype (spec 3.6). Standard is the legacy
/// warden pipeline; the others layer one new shape on top of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BehaviorArchetype {
    Standard,
    /// Big and tough: authored resistances plus a multi-attack wheel; a
    /// blow from a damage class it is vulnerable to (mult > 1) enrages it.
    Brute,
    /// Hackable-vs-destroy: a right-click with the right tool disables it
    /// (freezes, turns non-aggressive, drops its core); destroying it
    /// yields scrap.
    Construct,
    /// Stamps a template near itself on a cooldown, up to a cap.
    Builder,
}

/// What a builder builds (spec 3.6). All cells land through the ordinary
/// block path, so they save and reload like any world block.
#[derive(Clone, Debug)]
pub struct BuilderDef {
    pub template: Option<String>,
    /// Maximum stamps this builder will ever place.
    pub cap: u32,
    /// Seconds between stamps.
    pub interval: f32,
}

/// The hack path for a construct (spec 3.6), distinct from its destroy
/// `drops`.
#[derive(Clone, Debug)]
pub struct HackDef {
    /// Item tag the tool must carry ("hack").
    pub tool: Option<String>,
    /// (item, min, max) rolled once when the construct is disabled.
    pub drops: Vec<(ItemId, u32, u32)>,
}

#[derive(Clone, Debug)]
pub struct AnimalDef {
    pub name: String, // "base:deer"
    // Parsed content metadata retained for diagnostics and future UI labels.
    #[cfg_attr(not(test), allow(dead_code))]
    pub label: String,
    /// Lowercase biome names this species spawns in.
    pub biomes: Vec<String>,
    /// Optional local climate/habitat predicates. Every predicate must match;
    /// biome names remain supported as convenient compound tags.
    pub habitats: Vec<String>,
    pub temperature_c: Option<[f32; 2]>,
    pub vegetation: Option<[u8; 2]>,
    pub elevation: Option<[i16; 2]>,
    pub health: f32,
    pub speed: f32,
    /// Player distance that spooks it (0 = bold, only flees when hurt).
    pub flee_range: f32,
    pub group: [u32; 2],
    /// 1-in-N eligible fresh chunks spawn a group.
    pub rarity: u32,
    pub tile: u16,
    pub head_tile: u16,
    pub sound_pitch: f32,
    /// (item, min, max) rolled independently on death.
    pub drops: Vec<(ItemId, u32, u32)>,
    pub model: Vec<ModelBox>,
    /// Collision half-width / height derived from the model.
    pub half_w: f32,
    pub height: f32,
    // ---- hostile (warden) fields ----
    pub hostile: bool,
    /// Contact damage in half-hearts.
    pub attack: f32,
    /// Damage-class multipliers: absent class = 1.0 (full damage).
    pub resistances: HashMap<String, f32>,
    /// The attack wheel (spec 3.6). Never empty: an authored `attacks`
    /// list is kept as-is, otherwise a single implicit melee is
    /// synthesized from `attack`.
    pub attacks: Vec<AttackDef>,
    pub behavior: BehaviorArchetype,
    pub builder: Option<BuilderDef>,
    pub hack: Option<HackDef>,
    pub aggro_range: f32,
    /// Minimum world ire before this warden may spawn.
    pub ire_min: f32,
    /// Floaters hover with no gravity (ember/frost wisps).
    pub movement_float: bool,
    /// Swimmers live inside the water and never leave it willingly.
    pub movement_swim: bool,
    /// Physical water conditions this swimmer can inhabit. Non-swimmers do
    /// not carry this record. Defaults keep third-party swimmers compatible,
    /// while base species declare narrower ecological niches.
    pub aquatic: Option<AquaticHabitatDef>,
    /// The model carries a `wing*` box, so this floater is a bird and
    /// not a wisp: it beats, and it cruises high.
    pub winged: bool,
    /// Rendered at full block-light — its own lantern.
    pub emissive: bool,
    /// Point-light color x intensity carried by the creature (client
    /// presentation; wardens announcing themselves in light).
    pub glow: Option<[f32; 3]>,
    /// Spawns only where effective light is below this.
    pub spawn_light_max: u8,
    pub projectile: Option<ProjectileDef>,
    /// Favorite food: feed two adults to breed (wildlife only).
    pub breed_food: Option<ItemId>,
    /// Tamed carriers accept saddlebags (deer, boar).
    pub carrier: bool,
    /// A rideable vehicle (boats): no breeding, no ire, spawned by item.
    pub vehicle: bool,
    /// Seconds from a meal to the next hunger (0 = no belly at all).
    pub belly_secs: f32,
    /// Eats plants when hungry: grass, growing crops, fruited bushes.
    pub grazes: bool,
    /// Species this animal hunts when hungry (resolved indices).
    pub prey: Vec<usize>,
    /// Hunts players on sight, fed or not. The polar bear needs no
    /// reason. (Wildlife, not warden: persists, ignores daylight.)
    pub fierce: bool,
    pub arcane: Option<ArcaneContentDef>,
    /// Some(npc index) marks a synthesized companion species backed by an
    /// `NpcDef` (friendly characters, spec 3.1). Wildlife is None.
    pub npc: Option<usize>,
}

impl AnimalDef {
    /// Multiplier applied to damage of `dmg_type` (1.0 when untyped or not
    /// declared). The wild keys this on a damage-class string, not a block.
    pub fn damage_multiplier(&self, dmg_type: &str) -> f32 {
        self.resistances.get(dmg_type).copied().unwrap_or(1.0)
    }
}

/// A friendly scripted character, authored apart from wildlife (spec 3.1).
/// Carries no hostile/fauna fields; fixed position or patrol waypoints, a
/// talk radius, and a dialogue tree id.
#[derive(Clone, Debug)]
pub struct NpcDef {
    pub name: String, // "base:elder"
    pub label: String,
    /// Dialogue tree id this NPC enters when talked to (3.2).
    pub dialogue: Option<String>,
    /// Companion `AnimalDef` index (renders/persists this NPC as a Mob).
    /// The companion species is synthesized at load from the NPC model/tex.
    pub species: usize,
    /// Talk interaction range in blocks.
    pub talk_radius: f32,
    /// Patrol waypoints as `(du, dy, dv)` offsets from the spawn anchor;
    /// empty = stands fixed. The walk loops.
    pub patrol: Vec<[f32; 3]>,
    /// Seconds paused at each waypoint before setting off again.
    pub pause: f32,
    pub sound_pitch: f32,
}

/// One branching dialogue node (spec 3.2). `condition` gates the node's
/// availability; `choices` lead to further nodes or close the dialogue.
#[derive(Clone, Debug)]
pub struct DialogueDef {
    pub id: String,
    /// Optional npc the tree belongs to (info only; NPC defs reference trees
    /// by id, not the other way).
    pub npc: Option<String>,
    pub root: String,
    pub nodes: Vec<DialogueNode>,
}

/// A named hook into a mod script, evaluated through the existing dispatch
/// machinery (`ScriptHost::dispatch`). `""` mod = any defining mod.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptHook {
    pub mod_id: String,
    pub fn_name: String,
}

impl ScriptHook {
    /// Parse `"mod:fn"` or bare `"fn"` (any mod).
    pub(crate) fn parse(raw: &str) -> ScriptHook {
        match raw.split_once(':') {
            Some((mod_id, fn_name)) if !mod_id.is_empty() && !fn_name.is_empty() => {
                ScriptHook {
                    mod_id: mod_id.into(),
                    fn_name: fn_name.into(),
                }
            }
            _ => ScriptHook {
                mod_id: String::new(),
                fn_name: raw.into(),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct DialogueNode {
    pub id: String,
    pub text: String,
    /// Script hook previously evaluated; `false` hides the node.
    pub condition: Option<ScriptHook>,
    pub choices: Vec<DialogueChoice>,
}

#[derive(Clone, Debug)]
pub struct DialogueChoice {
    pub label: String,
    /// Hook previously evaluated; `false` hides the choice.
    pub condition: Option<ScriptHook>,
    /// Hook run when the choice is selected (rewards, quest flags).
    pub callback: Option<ScriptHook>,
    /// Next node id; `None` closes the dialogue.
    pub next: Option<String>,
}

/// A tracked quest (spec 3.3). Definitions are data; objective/completion
/// state lives in the per-mod KV store (already save- and hot-reload-safe).
#[derive(Clone, Debug)]
pub struct QuestDef {
    pub id: String,
    pub title: String,
    pub description: String,
    /// Giving npc id (optional).
    pub giver: Option<String>,
    /// Quest that must be done first (optional).
    pub prereq: Option<String>,
    pub objectives: Vec<QuestObjective>,
    pub rewards: Vec<QuestReward>,
}

#[derive(Clone, Debug)]
pub struct QuestObjective {
    pub key: String,
    pub description: String,
    pub count: u32,
}

/// One growth tier of a settlement (spec 3.4): the tier number and the
/// reputation value that reveals it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettlementTier {
    pub tier: u32,
    pub threshold: u32,
}

/// A settlement (spec 3.4): a piece assembly whose tier-tagged pieces are
/// all placed at worldgen, with tier-2+ pieces hidden (non-collidable, not
/// rendered, unbreakable) until the player's numeric reputation crosses the
/// tier's threshold. Reputation lives in the per-player KV under `rep_key`.
#[derive(Clone, Debug)]
pub struct SettlementDef {
    /// Qualified id (`mod:settlement`).
    pub id: String,
    /// Per-player KV key holding the numeric reputation.
    pub rep_key: String,
    /// Growth tiers, ascending, with increasing thresholds.
    pub tiers: Vec<SettlementTier>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuestReward {
    Give(ItemId, u32),
    SetFlag(String, String),
    Reputation(String, u32),
    /// Unlocks the recipe whose output item id is the recipe id (spec 3.5):
    /// writes the `learned:<recipe_id>` KV key truthy at apply time.
    LearnRecipe(String),
}

/// A flag-gated feature (spec 2.5): a sealed block placed by a
/// `feature:<id>` assembly marker that stays locked until the player's KV
/// flag `flag` reads `value`, then breaks normally or is replaced by
/// `unlocked_block`. Definitions are data; the locked state is derived live
/// from the per-player KV each time the block is touched.
#[derive(Clone, Debug)]
pub struct GateDef {
    /// Qualified id referenced by `feature:<id>` markers.
    pub id: String,
    /// The sealed block placed at the marker.
    pub block: BlockId,
    /// Per-player KV key consulted to unlock (e.g. `elder_told_tales`).
    pub flag: String,
    /// KV value that unlocks the gate (default `"true"`).
    pub value: String,
    /// If set, the sealed block is replaced by this once unlocked (e.g. air
    /// to "open" a door). If `None`, the sealed block just becomes breakable.
    pub unlocked_block: Option<BlockId>,
    /// Locked-interaction toast.
    pub message: String,
    /// Whether the sealed block is unbreakable while locked (default true).
    pub unbreakable_when_locked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AquaticHabitatDef {
    pub temperature_c: [f32; 2],
    pub depth_blocks: [u8; 2],
    pub discharge: [f32; 2],
    pub salinity: [u8; 2],
}

impl Default for AquaticHabitatDef {
    fn default() -> Self {
        Self {
            temperature_c: [-5.0, 40.0],
            depth_blocks: [2, u8::MAX],
            discharge: [0.0, f32::MAX],
            salinity: [0, u8::MAX],
        }
    }
}

/// A recipe slot requirement: one exact item, or any member of a tag.
#[derive(Clone, Debug)]
pub enum Ingredient {
    One(ItemId),
    Any(Vec<ItemId>),
}

impl Ingredient {
    pub fn matches(&self, item: ItemId) -> bool {
        match self {
            Ingredient::One(i) => *i == item,
            Ingredient::Any(list) => list.contains(&item),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RecipeDef {
    pub w: usize,
    pub h: usize,
    pub pattern: Vec<Option<Ingredient>>,
    pub output: ItemId,
    pub count: u32,
    /// Visible specialist assembly recipe. It participates in the browser,
    /// material graph, and survival census, but ordinary crafting cannot
    /// match it.
    pub station: Option<String>,
    /// Explicitly dispersed/consumed finite mass. The validator requires the
    /// input vector to equal output + byproducts + this vector.
    pub loss: MaterialVector,
    pub byproducts: Vec<(ItemId, u32)>,
    /// Per-player KV key that must read truthy to craft (spec 3.5). `None`
    /// means no tech gate; a recipe unlocked by a `learn_recipe` reward uses
    /// the runtime default `learned:<recipe_id>`.
    pub tech: Option<String>,
    /// Item consumed from the player's inventory (not the grid) on a
    /// successful craft (spec 3.5). Counts as extra input in the material
    /// graph.
    pub blueprint: Option<ItemId>,
}

#[derive(Clone, Debug)]
pub struct SmeltDef {
    pub input: Ingredient,
    pub output: ItemId,
    pub time: f32,
    /// Byproduct spat out the furnace mouth as item drops (cupellation:
    /// the silver stays in the slot, the lead pours out).
    pub spit: Option<(ItemId, u32)>,
    pub loss: MaterialVector,
}

#[derive(Clone, Debug)]
pub struct ForgeSalvageDef {
    pub input: ItemId,
    pub output: ItemId,
    pub byproduct: ItemId,
    pub recovery_permille: u16,
}

/// A bloomery batch chain: charge + fuel fire into blooms.
#[derive(Clone, Debug)]
pub struct BloomeryDef {
    pub charge: ItemId,
    pub fuel: ItemId,
    pub bloom: ItemId,
}

/// Station work: beat or grind an input into its output over N
/// strikes/turns at a block whose `interaction` matches `station`.
#[derive(Clone, Debug)]
pub struct WorkedDef {
    pub input: ItemId,
    pub output: ItemId,
    pub strikes: u32,
    pub station: String,
    pub needs_hammer: bool,
    pub count: u32,
    pub loss: MaterialVector,
}

#[derive(Clone, Debug)]
pub struct KilnDef {
    pub powder: ItemId,
    pub glass: ItemId,
    pub consumes: bool,
}

/// How a mineral deposit grows from its seed cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VeinShape {
    /// The classic drunk walk — roughly round pockets.
    Walk,
    /// A flat lens: long in x/z, grudging in y (coal seams).
    Seam,
    /// A near-vertical streak (quartz veins and their gold).
    Streak,
}

#[derive(Clone, Debug)]
pub struct OreFeature {
    pub block: BlockId,
    pub replaces: BlockId,
    pub vein_size: u32,
    pub per_chunk: u32,
    pub y_min: i32,
    pub y_max: i32,
    pub shape: VeinShape,
    /// Per-vein roll probability: 1.0 plants every roll, fractions
    /// thin a host down to traces (the bronze bootstrap lives here).
    pub chance: f32,
    /// Stable content identity, not the runtime block id.
    pub resource_key: String,
    pub mod_id: String,
    pub retrogen: RetrogenPolicy,
}

/// One weighted entry in a loot table.
#[derive(Clone, Debug)]
pub struct LootEntry {
    pub item: ItemId,
    pub weight: u32,
    pub count: (u32, u32),
    /// Spawn worn: fraction of max durability (old tools from ruins).
    pub durability_frac: Option<f32>,
}

/// A worldgen structure template: palette + bottom-up layers.
/// Special chars: '.' = leave terrain, '~' = force air, 'C' = loot chest.
#[derive(Clone, Debug)]
pub struct StructureDef {
    // Stable qualified id retained even though generation currently iterates
    // the resolved templates directly.
    #[cfg_attr(not(test), allow(dead_code))]
    pub name: String,
    pub biomes: Vec<String>,
    /// 1-in-N chunks (per matching biome).
    pub rarity: u32,
    /// None = on the surface; Some(min, max) = buried this deep.
    pub buried: Option<(i32, i32)>,
    pub palette: HashMap<char, BlockId>,
    pub layers: Vec<Vec<String>>,
    pub loot: Option<String>,
}

/// One connector point on a piece: a local cell offset, a typed connector
/// (`kind` pairs only with the same kind), and the cardinal direction the
/// connector points *out of* the piece. Children attach across a matching
/// connector of the same kind, facing opposite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PieceConnector {
    pub du: i32,
    pub dy: i32,
    pub dv: i32,
    pub kind: String,
    pub facing: crate::planet::Direction4,
}

/// A typed marker resolved to a world position when a piece is placed
/// (spec 2.4 spawn markers, 2.5 feature anchors). Carried and resolved
/// here; consumed by later phases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PieceMarker {
    pub du: i32,
    pub dy: i32,
    pub dv: i32,
    pub kind: String,
}

/// A cell that becomes a wild-owned loot chest when the piece is stamped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PieceChest {
    pub du: i32,
    pub dy: i32,
    pub dv: i32,
    /// Qualified loot table id.
    pub loot: String,
}

/// A gen-walk piece: cells reuse the Phase 4 template cell format (offset +
/// block name), so a captured region is already a valid piece body.
#[derive(Clone, Debug)]
pub struct PieceDef {
    pub name: String,
    /// `(du, dy, dv, block-name)` cells — exactly `TemplateCell`, reused.
    pub cells: Vec<crate::world::template::TemplateCell>,
    pub connectors: Vec<PieceConnector>,
    pub markers: Vec<PieceMarker>,
    pub chests: Vec<PieceChest>,
    /// Growth tier of the piece within its settlement (spec 3.4). Tier 1 is
    /// visible immediately; tier >= 2 is placed but hidden until reputation
    /// reveals the tier.
    pub settlement_tier: u32,
}

/// One weighted entry in a per-kind pool.
#[derive(Clone, Debug)]
pub struct PoolEntry {
    pub piece: String,
    pub weight: u32,
}

/// A weighted list of interchangeable pieces, keyed by connector kind.
#[derive(Clone, Debug)]
pub struct PoolDef {
    pub id: String,
    pub entries: Vec<PoolEntry>,
}

/// How a piece assembly meets the voxel terrain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainAdaptation {
    /// Place as authored at the surface anchor.
    None,
    /// Sink the piece so its floor sits at/below the surface line.
    Bury,
    /// Place inside solid terrain; terrain stays as the outer shell.
    Encapsulate,
}

/// A biome/rarity-gated piece assembly: an entry piece plus per-kind pools.
#[derive(Clone, Debug)]
pub struct AssemblyDef {
    pub name: String,
    pub biomes: Vec<String>,
    /// 1-in-N chunks (per matching biome).
    pub rarity: u32,
    pub entry_piece: String,
    /// connector kind -> pool id.
    pub pools: HashMap<String, String>,
    /// Steps from the entry piece before connectors stop being followed.
    pub max_depth: u32,
    /// Total pieces placed before the walk stops.
    pub max_pieces: u32,
    pub terrain: TerrainAdaptation,
    /// If set, this assembly generates the named settlement (spec 3.4): its
    /// tier-tagged pieces place hidden growth tiers at worldgen.
    pub settlement: Option<String>,
}

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

#[derive(Deserialize)]
struct ModToml {
    id: String,
    #[serde(default)]
    world_api: Option<u32>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    depends: Vec<String>,
    #[serde(default)]
    retrogen: Option<RetrogenPolicy>,
}

/// A `[[mode]]` entry from `modes.toml`: a named ruleset a world's `mode`
/// string can name. The mode inherits every field from its `base`
/// (built-in "survival" or "creative", or another declared mode) and
/// overrides the fields it declares.
#[derive(Deserialize, Clone)]
struct ModeToml {
    id: String,
    #[serde(default)]
    base: Option<String>,
    #[serde(default)]
    creative: Option<bool>,
    #[serde(default)]
    hunger: Option<bool>,
    #[serde(default)]
    fall_damage: Option<bool>,
    #[serde(default)]
    drowning: Option<bool>,
    #[serde(default)]
    lava_burn: Option<bool>,
    #[serde(default)]
    hostile_spawns: Option<bool>,
    #[serde(default)]
    ire: Option<bool>,
    #[serde(default)]
    hearts: Option<bool>,
    #[serde(default)]
    weather_extremes: Option<bool>,
    #[serde(default)]
    pvp: Option<bool>,
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
enum TexSpec {
    One(String),
    Faces {
        top: String,
        side: String,
        #[serde(default)]
        bottom: Option<String>,
    },
}

#[derive(Deserialize, Clone)]
struct BlockToml {
    id: String,
    name: Option<String>,
    texture: TexSpec,
    #[serde(default)]
    hardness: Option<f32>,
    #[serde(default)]
    unbreakable: bool,
    #[serde(default)]
    tool: Option<ToolKind>,
    #[serde(default)]
    requires_tool: bool,
    /// "self" (default), "none", or an item id.
    #[serde(default)]
    drops: Option<String>,
    #[serde(default)]
    drop_count: Option<u32>,
    #[serde(default = "yes")]
    solid: bool,
    #[serde(default = "yes")]
    opaque: bool,
    #[serde(default)]
    interaction: Option<String>,
    #[serde(default)]
    min_tier: u8,
    #[serde(default)]
    water: Option<u8>,
    #[serde(default)]
    lava: Option<u8>,
    #[serde(default)]
    cross: bool,
    #[serde(default)]
    burns: u8,
    #[serde(default)]
    floats: bool,
    #[serde(default)]
    shape: Option<String>,
    #[serde(default)]
    crop: Option<CropToml>,
    #[serde(default)]
    harvest: Option<HarvestToml>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    light: u8,
    #[serde(default)]
    sapling: Option<SaplingToml>,
    #[serde(default)]
    bonus_drop: Option<BonusDropToml>,
    #[serde(default)]
    brush: Option<BrushToml>,
    /// Optional glow color (r,g,b, each 0..1); brightest channel scales to
    /// `light`. Omit for white light.
    #[serde(default)]
    light_color: Option<[f32; 3]>,
    /// Render height 0..1 (thin slabs like snow layers).
    #[serde(default)]
    height: Option<f32>,
    /// Gravity: detaches and falls when unsupported (sand, gravel).
    #[serde(default)]
    falls: bool,
    /// Four top-face textures by fertility quartile (soil blocks).
    #[serde(default)]
    texture_fertility: Option<Vec<String>>,
    /// Counts as glazing: passes sky light and makes greenhouses.
    #[serde(default)]
    glass: bool,
    /// Stained light: which RGB channels of block light pass through
    /// (e.g. [1, 0, 0] for red glass). Default: all.
    #[serde(default)]
    light_filter: Option<[u8; 3]>,
    /// Register an item form for placing (default true).
    #[serde(default = "yes")]
    item: bool,
    #[serde(default)]
    material_class: Option<MaterialClass>,
    #[serde(default)]
    materials: MaterialVector,
    /// Pattern A heat contribution (see BlockDef::heat_retention).
    #[serde(default)]
    heat_retention: u32,
    #[serde(default)]
    arcane: Option<ArcaneContentToml>,
    #[serde(default)]
    observation: Option<ObservationToml>,
    #[serde(default)]
    discovery_fixture: Option<DiscoveryFixtureToml>,
    #[serde(default)]
    arcane_ecology: Option<ArcaneEcologyToml>,
    #[serde(default)]
    dross_scar: Option<DrossScarToml>,
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct DrossScarToml {
    kind: crate::dross::ScarKind,
    handler: crate::dross::ScarHandler,
    carriers: Vec<crate::dross::DrossCarrier>,
    min_band: crate::dross::DrossBand,
    #[serde(default)]
    status: Option<crate::dross::ScarStatusHandler>,
    #[serde(default)]
    activity: Option<crate::dross::ScarActivityHandler>,
    #[serde(default = "one_u8")]
    max_sites_per_region: u8,
}

const fn one_u8() -> u8 {
    1
}

#[derive(Deserialize, Clone)]
struct SaplingToml {
    tree: String,
}

#[derive(Deserialize, Clone)]
struct BonusDropToml {
    item: String,
    chance: f32,
}

fn yes() -> bool {
    true
}

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

#[derive(Deserialize, Clone)]
struct CropToml {
    stages: u8,
    #[serde(default)]
    next_chance: Option<f32>,
    /// Texture per stage (else the block texture is reused).
    #[serde(default)]
    stage_textures: Vec<String>,
    #[serde(default)]
    any_soil: bool,
    /// Rotation family 1..=3 (defaults to a stable name hash).
    #[serde(default)]
    family: Option<u8>,
}

#[derive(Deserialize, Clone)]
struct HarvestToml {
    item: String,
    #[serde(default)]
    count: Option<u32>,
    becomes: String,
}

#[derive(Deserialize, Clone)]
struct FoodToml {
    hunger: f32,
    #[serde(default)]
    eat_time: Option<f32>,
    #[serde(default)]
    nutrition: HashMap<String, f32>,
}

#[derive(Deserialize, Clone)]
struct ItemToml {
    id: String,
    name: Option<String>,
    texture: String,
    #[serde(default)]
    max_stack: Option<u32>,
    #[serde(default)]
    tool: Option<ToolKind>,
    #[serde(default)]
    tool_speed: Option<f32>,
    #[serde(default)]
    tool_tier: Option<u8>,
    #[serde(default)]
    durability: Option<u32>,
    #[serde(default)]
    food: Option<FoodToml>,
    #[serde(default)]
    places: Option<String>,
    #[serde(default)]
    damage: Option<f32>,
    /// Player-wielded damage class ("pierce", "blunt", "fire"...); the
    /// wild's resistances key on it. None = untyped (always full).
    #[serde(default)]
    damage_type: Option<String>,
    #[serde(default)]
    bow: Option<BowToml>,
    #[serde(default)]
    ammo: Option<String>,
    #[serde(default)]
    armor: Option<ArmorToml>,
    #[serde(default)]
    bedroll: bool,
    #[serde(default)]
    shears: bool,
    #[serde(default)]
    charm: Option<CharmToml>,
    #[serde(default)]
    wand_component: Option<crate::implements::WandComponentDef>,
    #[serde(default)]
    implement: Option<crate::implements::ImplementItemDef>,
    #[serde(default)]
    tablet: bool,
    #[serde(default)]
    striker: bool,
    #[serde(default)]
    brush_tool: bool,
    /// Right-click throw: projectile speed (snowballs).
    #[serde(default)]
    throw: Option<ThrowToml>,
    /// Works blooms on an anvil.
    #[serde(default)]
    hammer: bool,
    /// Right-click disables (hacks) a construct instead of destroying it.
    #[serde(default)]
    hack: bool,
    /// Carried-light color for non-placeable glowing items.
    #[serde(default)]
    glow: Option<[f32; 3]>,
    #[serde(default)]
    material_class: Option<MaterialClass>,
    #[serde(default)]
    materials: MaterialVector,
    #[serde(default)]
    salvage: Option<SalvageToml>,
    #[serde(default)]
    arcane: Option<ArcaneContentToml>,
    #[serde(default)]
    arcane_ecology: Option<ArcaneEcologyToml>,
    #[serde(default)]
    observation: Option<ObservationToml>,
    #[serde(default)]
    discovery: Option<DiscoveryItemToml>,
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
enum CharmToml {
    Legacy(String),
    Detailed(crate::implements::CharmDef),
}

impl CharmToml {
    fn effect_id(&self) -> String {
        match self {
            Self::Legacy(effect) => effect.clone(),
            Self::Detailed(definition) => definition.effect.id().into(),
        }
    }

    fn definition(&self) -> Option<crate::implements::CharmDef> {
        match self {
            Self::Legacy(effect) => {
                let effect = match effect.as_str() {
                    "quiet" => crate::implements::CharmEffect::Quiet,
                    "bark" => crate::implements::CharmEffect::Bark,
                    "hunger" => crate::implements::CharmEffect::Hunger,
                    _ => return None,
                };
                Some(crate::implements::CharmDef {
                    effect,
                    charge_per_trigger: match effect {
                        crate::implements::CharmEffect::Quiet => 2,
                        crate::implements::CharmEffect::Bark => 4,
                        crate::implements::CharmEffect::Hunger => 1,
                    },
                    capacity: 4_096,
                    stability: 800,
                    dross_per_transfer: 25,
                })
            }
            Self::Detailed(definition) => Some(definition.clone()),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
struct ObservationToml {
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    properties: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct DiscoveryItemToml {
    kind: String,
    #[serde(default)]
    evidence_class: Option<String>,
    #[serde(default)]
    authored_text: Vec<String>,
    #[serde(default)]
    calibration: Option<crate::discovery::CalibrationGrade>,
    #[serde(default)]
    experiment: Option<crate::discovery::ExperimentKind>,
}

#[derive(Debug, Deserialize, Clone)]
struct DiscoveryFixtureToml {
    kind: String,
    #[serde(default)]
    experiments: Vec<crate::discovery::ExperimentKind>,
    #[serde(default)]
    record_capacity: u16,
}

#[derive(Debug, Deserialize, Clone)]
struct ArcaneContentToml {
    capacity: u64,
    conductivity: u16,
    stability: u16,
    resonance: BTreeMap<String, u16>,
    on_destroy: ArcaneDisposition,
}

#[derive(Debug, Deserialize, Clone)]
struct ArcaneEcologyToml {
    roles: Vec<EcologyRole>,
    #[serde(default = "default_ecology_kind")]
    kind: ArcaneEcologyKind,
    habitat: Vec<String>,
    charge_capacity: u64,
    uptake_per_day: u32,
    #[serde(default)]
    release_per_day: u32,
    #[serde(default = "default_ecology_source")]
    source: EcologySource,
    resonance: BTreeMap<String, u16>,
    dross_tolerance: u32,
    #[serde(default)]
    water_per_day_hu: u32,
    #[serde(default)]
    nutrient_per_day: u16,
    #[serde(default = "default_reproduction")]
    reproduction: ReproductionMode,
    #[serde(default = "all_seasons")]
    seasons: [bool; 4],
    carrying_capacity: u16,
    harvest: EcologyHarvestClass,
    regrowth_days: u16,
    #[serde(default)]
    min_stability: u16,
    #[serde(default = "permille")]
    max_stability: u16,
    #[serde(default)]
    min_richness: u16,
    #[serde(default)]
    crystal_stages: u8,
    #[serde(default)]
    preserving_tool_tier: u8,
}

fn default_ecology_kind() -> ArcaneEcologyKind {
    ArcaneEcologyKind::Organism
}

fn default_ecology_source() -> EcologySource {
    EcologySource::Ambient
}

fn default_reproduction() -> ReproductionMode {
    ReproductionMode::Seed
}

fn all_seasons() -> [bool; 4] {
    [true; 4]
}

fn permille() -> u16 {
    1_000
}

#[derive(Deserialize, Clone)]
struct ResonanceToml {
    id: String,
    #[serde(default)]
    label: Option<String>,
}

#[derive(Deserialize, Clone)]
struct ArcaneSiteToml {
    id: String,
    #[serde(default)]
    requires: Vec<String>,
    #[serde(default = "default_capacity_factor")]
    capacity_factor: f32,
    #[serde(default)]
    resonance: BTreeMap<String, u16>,
    #[serde(default = "default_arcane_site_rarity")]
    rarity: f32,
    #[serde(default = "default_arcane_site_radius")]
    radius_cells: u16,
}

fn default_capacity_factor() -> f32 {
    1.0
}

fn default_arcane_site_rarity() -> f32 {
    0.01
}

fn default_arcane_site_radius() -> u16 {
    2
}

#[derive(Deserialize, Default)]
struct ArcaneFile {
    #[serde(default)]
    schema_version: Option<u32>,
    #[serde(default)]
    resonance: Vec<ResonanceToml>,
    #[serde(default, rename = "arcane_site")]
    sites: Vec<ArcaneSiteToml>,
}

#[derive(Deserialize, Clone)]
struct SalvageToml {
    station: String,
    recovery: f32,
}

#[derive(Deserialize, Clone)]
struct ThrowToml {
    #[serde(default)]
    speed: Option<f32>,
}

#[derive(Deserialize, Clone)]
struct BowToml {
    damage: f32,
    #[serde(default)]
    speed: Option<f32>,
}

#[derive(Deserialize, Clone)]
struct ArmorToml {
    slot: String,
    points: u32,
}

#[derive(Deserialize, Clone)]
struct BoxToml {
    size: [f32; 3],
    at: [f32; 3],
    #[serde(default)]
    tex: Option<String>,
}

#[derive(Deserialize, Clone)]
struct AnimalDropToml {
    item: String,
    #[serde(default)]
    min: Option<u32>,
    #[serde(default)]
    max: Option<u32>,
}

/// One damage-class multiplier: `mult = 0.5` halves that class, `mult = 2.0`
/// doubles it. Absent classes pass untouched (full damage).
#[derive(Deserialize, Clone)]
struct ResistToml {
    #[serde(rename = "type")]
    kind: String,
    mult: f32,
}

/// `resist` accepts a single table or a list of tables.
#[derive(Deserialize, Clone)]
#[serde(untagged)]
enum ResistTomlList {
    One(ResistToml),
    Many(Vec<ResistToml>),
}

impl ResistTomlList {
    fn resolved(&self) -> HashMap<String, f32> {
        match self {
            Self::One(r) => {
                let mut map = HashMap::new();
                map.insert(r.kind.clone(), r.mult);
                map
            }
            Self::Many(list) => list.iter().map(|r| (r.kind.clone(), r.mult)).collect(),
        }
    }
}

#[derive(Deserialize, Clone)]
struct AnimalToml {
    id: String,
    #[serde(default)]
    name: Option<String>,
    biomes: Vec<String>,
    #[serde(default)]
    habitats: Vec<String>,
    #[serde(default)]
    temperature_c: Option<[f32; 2]>,
    #[serde(default)]
    vegetation: Option<[u8; 2]>,
    #[serde(default)]
    elevation: Option<[i16; 2]>,
    #[serde(default)]
    health: Option<f32>,
    #[serde(default)]
    speed: Option<f32>,
    #[serde(default)]
    flee_range: Option<f32>,
    #[serde(default)]
    group: Option<[u32; 2]>,
    #[serde(default)]
    rarity: Option<u32>,
    tex: String,
    #[serde(default)]
    head_tex: Option<String>,
    #[serde(default)]
    sound_pitch: Option<f32>,
    #[serde(default)]
    drops: Vec<AnimalDropToml>,
    /// Damage-class multipliers ("fire", "pierce"...); see ResistTomlList.
    #[serde(default)]
    resist: Option<ResistTomlList>,
    /// Authored attack wheel; empty = the `attack` scalar becomes a
    /// single implicit melee (full back-compat).
    #[serde(default)]
    attacks: Vec<AttackToml>,
    /// Archetype: "standard" (default), "brute", "construct", "builder".
    #[serde(default)]
    behavior: Option<String>,
    /// Builder options (spec 3.6): template name, stamp cap, interval.
    #[serde(default)]
    builder: Option<BuilderToml>,
    /// Construct hack options (spec 3.6): tool tag + core drops.
    #[serde(default)]
    hack: Option<HackToml>,
    #[serde(default)]
    model: HashMap<String, BoxToml>,
    #[serde(default)]
    hostile: bool,
    #[serde(default)]
    attack: Option<f32>,
    #[serde(default)]
    aggro_range: Option<f32>,
    #[serde(default)]
    ire_min: Option<f32>,
    #[serde(default)]
    movement: Option<String>,
    #[serde(default)]
    aquatic: Option<AquaticHabitatToml>,
    #[serde(default)]
    emissive: bool,
    #[serde(default)]
    glow: Option<[f32; 3]>,
    #[serde(default)]
    spawn_light_max: Option<u8>,
    #[serde(default)]
    projectile: Option<ProjectileToml>,
    #[serde(default)]
    breed_food: Option<String>,
    #[serde(default)]
    carrier: bool,
    #[serde(default)]
    belly: Option<f32>,
    #[serde(default)]
    grazes: bool,
    #[serde(default)]
    prey: Vec<String>,
    #[serde(default)]
    fierce: bool,
    #[serde(default)]
    vehicle: bool,
    #[serde(default)]
    arcane: Option<ArcaneContentToml>,
}

#[derive(Deserialize, Clone, Default)]
struct AquaticHabitatToml {
    #[serde(default)]
    temperature_c: Option<[f32; 2]>,
    #[serde(default)]
    depth_blocks: Option<[u8; 2]>,
    #[serde(default)]
    discharge: Option<[f32; 2]>,
    #[serde(default)]
    salinity: Option<[u8; 2]>,
}

#[derive(Deserialize, Clone)]
struct ProjectileToml {
    tex: String,
    damage: f32,
    #[serde(default)]
    damage_type: Option<String>,
    #[serde(default)]
    speed: Option<f32>,
    #[serde(default)]
    cooldown: Option<f32>,
}

/// One authored entry in a species' attack wheel (spec 3.6).
#[derive(Deserialize, Clone)]
struct AttackToml {
    #[serde(default)]
    name: Option<String>,
    /// "melee" | "charge" | "projectile".
    kind: String,
    #[serde(default)]
    damage: Option<f32>,
    #[serde(default)]
    cooldown: Option<f32>,
    /// Trigger distance in blocks; defaults to melee reach for melee,
    /// the cast range for projectile attacks.
    #[serde(default)]
    range: Option<f32>,
    #[serde(default)]
    damage_type: Option<String>,
    #[serde(default)]
    projectile: Option<ProjectileToml>,
}

#[derive(Deserialize, Clone)]
struct BuilderToml {
    #[serde(default)]
    template: Option<String>,
    #[serde(default)]
    cap: Option<u32>,
    #[serde(default)]
    interval: Option<f32>,
}

#[derive(Deserialize, Clone)]
struct HackToml {
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    drops: Vec<AnimalDropToml>,
}

#[derive(Deserialize, Clone)]
struct RecipeToml {
    pattern: Vec<String>,
    #[serde(default)]
    keys: HashMap<String, String>,
    output: String,
    #[serde(default)]
    count: Option<u32>,
    #[serde(default)]
    station: Option<String>,
    #[serde(default)]
    loss: MaterialVector,
    #[serde(default)]
    byproducts: Vec<ByproductToml>,
    /// Per-player KV key that must read truthy to craft (spec 3.5). A recipe
    /// without a `tech` field has no tech gate; `learn_recipe` quest rewards
    /// unlock the runtime default `learned:<recipe_id>` key.
    #[serde(default)]
    tech: Option<String>,
    /// Item consumed from the player's inventory (not the grid) on a
    /// successful craft (spec 3.5).
    #[serde(default)]
    blueprint: Option<String>,
}

#[derive(Deserialize, Clone)]
struct ByproductToml {
    item: String,
    #[serde(default = "one_u32")]
    count: u32,
}

#[derive(Deserialize, Clone)]
struct SmeltToml {
    input: String,
    output: String,
    #[serde(default)]
    time: Option<f32>,
    #[serde(default)]
    spit: Option<SpitToml>,
    #[serde(default)]
    loss: MaterialVector,
}

#[derive(Deserialize, Clone)]
struct SpitToml {
    item: String,
    #[serde(default)]
    count: Option<u32>,
}

#[derive(Deserialize, Clone)]
struct FuelToml {
    item: String,
    burn: f32,
    #[serde(default)]
    speed: Option<f32>,
}

#[derive(Deserialize, Clone)]
struct BloomeryToml {
    charge: String,
    fuel: String,
    bloom: String,
}

#[derive(Deserialize, Clone)]
struct KilnToml {
    powder: String,
    glass: String,
    #[serde(default)]
    consumes: bool,
}

#[derive(Deserialize, Clone)]
struct KilnBaseToml {
    sand: String,
    fuel: String,
    clear: String,
}

#[derive(Deserialize, Clone)]
struct WorkedToml {
    input: String,
    output: String,
    #[serde(default)]
    strikes: Option<u32>,
    /// Which station block works it: "anvil" (default) or "quern".
    #[serde(default)]
    station: Option<String>,
    /// "hammer" (default) or "none" (bare hands, e.g. the quern).
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    count: Option<u32>,
    #[serde(default)]
    loss: MaterialVector,
}

#[derive(Deserialize, Clone)]
struct AliasToml {
    old: String,
    new: String,
}

#[derive(Deserialize, Clone)]
struct TagToml {
    id: String,
    items: Vec<String>,
}

#[derive(Deserialize, Clone)]
struct FeatureToml {
    r#type: String,
    block: String,
    #[serde(default)]
    replaces: Option<String>,
    #[serde(default)]
    vein_size: Option<u32>,
    #[serde(default)]
    per_chunk: Option<u32>,
    #[serde(default)]
    y_range: Option<[i32; 2]>,
    #[serde(default)]
    shape: Option<String>,
    #[serde(default)]
    chance: Option<f32>,
    // -- spec 2.5 gate features (`type = "gate"`) --
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    flag: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    unlocked_block: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    unbreakable_when_locked: Option<bool>,
}

#[derive(Deserialize, Default)]
struct BlocksFile {
    #[serde(default)]
    block: Vec<BlockToml>,
}
#[derive(Deserialize, Default)]
struct ItemsFile {
    #[serde(default)]
    item: Vec<ItemToml>,
}
#[derive(Deserialize, Default)]
struct ModesFile {
    #[serde(default)]
    mode: Vec<ModeToml>,
}
#[derive(Deserialize, Default)]
struct RecipesFile {
    #[serde(default)]
    recipe: Vec<RecipeToml>,
    #[serde(default)]
    smelt: Vec<SmeltToml>,
    #[serde(default)]
    fuel: Vec<FuelToml>,
    #[serde(default)]
    bloomery: Vec<BloomeryToml>,
    #[serde(default)]
    worked: Vec<WorkedToml>,
    #[serde(default)]
    kiln: Vec<KilnToml>,
    #[serde(default)]
    kiln_base: Option<KilnBaseToml>,
}
#[derive(Deserialize, Default)]
struct AliasesFile {
    #[serde(default)]
    alias: Vec<AliasToml>,
}
#[derive(Deserialize, Default)]
struct FeaturesFile {
    #[serde(default)]
    feature: Vec<FeatureToml>,
}
#[derive(Deserialize, Default)]
struct TagsFile {
    #[serde(default)]
    tag: Vec<TagToml>,
}
#[derive(Deserialize, Clone)]
struct BrushToml {
    table: String,
    becomes: String,
}

#[derive(Deserialize, Clone)]
struct LootEntryToml {
    item: String,
    #[serde(default = "one_u32")]
    weight: u32,
    #[serde(default)]
    count: Option<[u32; 2]>,
    #[serde(default)]
    durability: Option<f32>,
}

fn one_u32() -> u32 {
    1
}

#[derive(Deserialize, Clone)]
struct LootToml {
    id: String,
    entries: Vec<LootEntryToml>,
}

#[derive(Deserialize, Clone)]
struct StructureToml {
    id: String,
    biomes: Vec<String>,
    rarity: u32,
    #[serde(default)]
    placement: Option<String>,
    #[serde(default)]
    depth: Option<[i32; 2]>,
    palette: HashMap<String, String>,
    layers: Vec<Vec<String>>,
    #[serde(default)]
    loot: Option<String>,
}

#[derive(Deserialize, Default)]
struct StructuresFile {
    #[serde(default)]
    structure: Vec<StructureToml>,
    #[serde(default)]
    loot: Vec<LootToml>,
}

#[derive(Deserialize, Clone)]
struct PieceToml {
    id: String,
    #[serde(default)]
    cells: Vec<PieceCellToml>,
    #[serde(default)]
    connectors: Vec<ConnectorToml>,
    #[serde(default)]
    markers: Vec<MarkerToml>,
    #[serde(default)]
    chests: Vec<ChestToml>,
    #[serde(default)]
    settlement_tier: u32,
}

#[derive(Deserialize, Clone)]
struct PieceCellToml {
    #[serde(default)]
    du: i32,
    #[serde(default)]
    dy: i32,
    #[serde(default)]
    dv: i32,
    block: String,
}

#[derive(Deserialize, Clone)]
struct ConnectorToml {
    #[serde(default)]
    du: i32,
    #[serde(default)]
    dy: i32,
    #[serde(default)]
    dv: i32,
    kind: String,
    facing: String,
}

#[derive(Deserialize, Clone)]
struct MarkerToml {
    #[serde(default)]
    du: i32,
    #[serde(default)]
    dy: i32,
    #[serde(default)]
    dv: i32,
    kind: String,
}

#[derive(Deserialize, Clone)]
struct ChestToml {
    #[serde(default)]
    du: i32,
    #[serde(default)]
    dy: i32,
    #[serde(default)]
    dv: i32,
    #[serde(default)]
    loot: String,
}

#[derive(Deserialize, Clone)]
struct PoolToml {
    id: String,
    entries: Vec<PoolEntryToml>,
}

#[derive(Deserialize, Clone)]
struct PoolEntryToml {
    piece: String,
    #[serde(default)]
    weight: u32,
}

#[derive(Deserialize, Clone)]
struct AssemblyToml {
    id: String,
    biomes: Vec<String>,
    rarity: u32,
    entry: String,
    #[serde(default)]
    pools: HashMap<String, String>,
    #[serde(default)]
    max_depth: u32,
    #[serde(default)]
    max_pieces: u32,
    #[serde(default)]
    terrain: Option<String>,
    #[serde(default)]
    settlement: Option<String>,
}

#[derive(Deserialize, Clone)]
struct SettlementToml {
    id: String,
    #[serde(default)]
    rep_key: Option<String>,
    #[serde(default)]
    tiers: Vec<SettlementTierToml>,
}

#[derive(Deserialize, Clone)]
struct SettlementTierToml {
    tier: u32,
    threshold: u32,
}

#[derive(Deserialize, Default)]
struct PiecesFile {
    #[serde(default)]
    piece: Vec<PieceToml>,
    #[serde(default)]
    pool: Vec<PoolToml>,
    #[serde(default)]
    assembly: Vec<AssemblyToml>,
    #[serde(default)]
    settlement: Vec<SettlementToml>,
}

#[derive(Deserialize, Default)]
struct AnimalsFile {
    #[serde(default)]
    animal: Vec<AnimalToml>,
}

#[derive(Deserialize, Default)]
struct NpcsFile {
    #[serde(default)]
    npc: Vec<NpcToml>,
}

#[derive(Deserialize, Default)]
struct DialogueFile {
    #[serde(default)]
    dialogue: Vec<DialogueToml>,
}

#[derive(Deserialize, Default)]
struct QuestsFile {
    #[serde(default)]
    quest: Vec<QuestToml>,
}

#[derive(Deserialize, Clone)]
struct NpcToml {
    id: String,
    #[serde(default)]
    name: Option<String>,
    tex: String,
    #[serde(default)]
    head_tex: Option<String>,
    #[serde(default)]
    dialogue: Option<String>,
    #[serde(default)]
    talk_radius: Option<f32>,
    #[serde(default)]
    patrol: Vec<[f32; 3]>,
    #[serde(default)]
    pause: Option<f32>,
    #[serde(default)]
    sound_pitch: Option<f32>,
    #[serde(default)]
    model: HashMap<String, BoxToml>,
}

#[derive(Deserialize, Clone)]
struct DialogueToml {
    id: String,
    #[serde(default)]
    npc: Option<String>,
    root: String,
    #[serde(default)]
    nodes: Vec<DialogueNodeToml>,
}

#[derive(Deserialize, Clone)]
struct DialogueNodeToml {
    id: String,
    text: String,
    #[serde(default)]
    condition: Option<String>,
    #[serde(default)]
    choices: Vec<DialogueChoiceToml>,
}

#[derive(Deserialize, Clone)]
struct DialogueChoiceToml {
    label: String,
    #[serde(default)]
    condition: Option<String>,
    #[serde(default)]
    callback: Option<String>,
    #[serde(default)]
    next: Option<String>,
}

#[derive(Deserialize, Clone)]
struct QuestToml {
    id: String,
    title: String,
    description: String,
    #[serde(default)]
    giver: Option<String>,
    #[serde(default)]
    prereq: Option<String>,
    #[serde(default)]
    objectives: Vec<QuestObjectiveToml>,
    #[serde(default)]
    rewards: Vec<QuestRewardToml>,
}

#[derive(Deserialize, Clone)]
struct QuestObjectiveToml {
    key: String,
    description: String,
    #[serde(default)]
    count: u32,
}

#[derive(Deserialize, Clone)]
struct QuestRewardToml {
    #[serde(default)]
    item: Option<String>,
    #[serde(default)]
    count: u32,
    #[serde(default)]
    set_flag: Option<String>,
    #[serde(default)]
    flag_value: Option<String>,
    #[serde(default)]
    add_reputation: Option<String>,
    #[serde(default)]
    rep_amount: u32,
    /// Recipe id whose `learned:<id>` tech key this reward sets truthy (spec
    /// 3.5). Resolved to the qualified output-item id, which is also the
    /// runtime recipe id.
    #[serde(default)]
    learn_recipe: Option<String>,
}

struct RawMod {
    info: ModInfo,
    depends: Vec<String>,
    blocks: Vec<BlockToml>,
    items: Vec<ItemToml>,
    recipes: Vec<RecipeToml>,
    smelts: Vec<SmeltToml>,
    fuels: Vec<FuelToml>,
    bloomeries: Vec<BloomeryToml>,
    workeds: Vec<WorkedToml>,
    kilns: Vec<KilnToml>,
    kiln_bases: Vec<KilnBaseToml>,
    features: Vec<FeatureToml>,
    tags: Vec<TagToml>,
    aliases: Vec<AliasToml>,
    animals: Vec<AnimalToml>,
    npcs: Vec<NpcToml>,
    dialogues: Vec<DialogueToml>,
    quests: Vec<QuestToml>,
    structures: Vec<StructureToml>,
    loots: Vec<LootToml>,
    pieces: Vec<PieceToml>,
    pools: Vec<PoolToml>,
    assemblies: Vec<AssemblyToml>,
    settlements: Vec<SettlementToml>,
    resonances: Vec<ResonanceToml>,
    arcane_sites: Vec<ArcaneSiteToml>,
    workings: Vec<crate::workings::RawWorkingDef>,
    preparations: Vec<crate::alchemy::RawPreparationDef>,
    modes: Vec<ModeToml>,
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
}

// ---------------- loading ----------------

const BASE_BLOCKS: &str = include_str!("../base/blocks.toml");
const BASE_ITEMS: &str = include_str!("../base/items.toml");
const BASE_RECIPES: &str = include_str!("../base/recipes.toml");
const BASE_TAGS: &str = include_str!("../base/tags.toml");
const BASE_FEATURES: &str = include_str!("../base/features.toml");
const BASE_ALIASES: &str = include_str!("../base/aliases.toml");
const BASE_ANIMALS: &str = include_str!("../base/animals.toml");
const BASE_NPCS: &str = include_str!("../base/npcs.toml");
const BASE_DIALOGUE: &str = include_str!("../base/dialogue.toml");
const BASE_QUESTS: &str = include_str!("../base/quests.toml");
const BASE_STRUCTURES: &str = include_str!("../base/structures.toml");
const BASE_PIECES: &str = include_str!("../base/pieces.toml");
const BASE_WORKINGS: &str = include_str!("../base/workings.toml");
const BASE_PREPARATIONS: &str = include_str!("../base/preparations.toml");
pub const WORLD_API_VERSION: u32 = 2;

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
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap_or_default();
    let blocks: BlocksFile =
        toml::from_str(&read("blocks.toml")).map_err(|e| format!("blocks.toml: {e}"))?;
    let items: ItemsFile =
        toml::from_str(&read("items.toml")).map_err(|e| format!("items.toml: {e}"))?;
    let recipes: RecipesFile =
        toml::from_str(&read("recipes.toml")).map_err(|e| format!("recipes.toml: {e}"))?;
    let features: FeaturesFile =
        toml::from_str(&read("features.toml")).map_err(|e| format!("features.toml: {e}"))?;
    let tags: TagsFile =
        toml::from_str(&read("tags.toml")).map_err(|e| format!("tags.toml: {e}"))?;
    let aliases: AliasesFile =
        toml::from_str(&read("aliases.toml")).map_err(|e| format!("aliases.toml: {e}"))?;
    let animals: AnimalsFile =
        toml::from_str(&read("animals.toml")).map_err(|e| format!("animals.toml: {e}"))?;
    let npcs: NpcsFile =
        toml::from_str(&read("npcs.toml")).map_err(|e| format!("npcs.toml: {e}"))?;
    let dialogue: DialogueFile =
        toml::from_str(&read("dialogue.toml")).map_err(|e| format!("dialogue.toml: {e}"))?;
    let quests: QuestsFile =
        toml::from_str(&read("quests.toml")).map_err(|e| format!("quests.toml: {e}"))?;
    let structures: StructuresFile =
        toml::from_str(&read("structures.toml")).map_err(|e| format!("structures.toml: {e}"))?;
    let pieces: PiecesFile =
        toml::from_str(&read("pieces.toml")).map_err(|e| format!("pieces.toml: {e}"))?;
    let arcane: ArcaneFile =
        toml::from_str(&read("arcane.toml")).map_err(|e| format!("arcane.toml: {e}"))?;
    if arcane.schema_version.is_some_and(|version| version != 1) {
        return Err("arcane.toml: schema_version must be 1".into());
    }
    let workings: crate::workings::WorkingsFile =
        toml::from_str(&read("workings.toml")).map_err(|e| format!("workings.toml: {e}"))?;
    if workings
        .schema_version
        .is_some_and(|version| version != crate::workings::WORKINGS_SCHEMA_VERSION)
    {
        return Err(format!(
            "workings.toml: schema_version must be {}",
            crate::workings::WORKINGS_SCHEMA_VERSION
        ));
    }
    let preparations: crate::alchemy::PreparationsFile = toml::from_str(&read("preparations.toml"))
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
        toml::from_str(&read("modes.toml")).map_err(|e| format!("modes.toml: {e}"))?;
    if !features.feature.is_empty() && m.retrogen.is_none() {
        return Err(
            "mod.toml: a worldgen feature requires retrogen = \"untouched_host_only\", \
             \"secondary_recovery\", \"world_event\", or \"no_retrogen\""
                .into(),
        );
    }
    let has_script = dir.join("main.rhai").exists();
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
    }
}

/// Load base + all mods under `mods_dir` into a fresh registry.
/// Individual bad mods are skipped with their error recorded.
pub fn load(mods_dir: &Path) -> Registry {
    let mut raws = vec![base_mod()];
    let mut failed: Vec<ModInfo> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(mods_dir) {
        let mut dirs: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && p.join("mod.toml").exists())
            .collect();
        dirs.sort();
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
        };
        std::mem::replace(&mut self[idx], dummy)
    }
}

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
                || it.implement.is_some();
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
                None | Some("standard" | "brute" | "construct" | "builder") => {}
                Some(other) => errs.push(format!(
                    "animal {}: unknown behavior {other}",
                    a.id
                )),
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
        reg.settlements.push(SettlementDef {
            rep_key: s.rep_key.clone().unwrap_or_else(|| format!("rep_{id}")),
            id,
            tiers,
        });
    }
    // Cross-validate settlement wiring (spec 3.4): every assembly that names
    // a settlement must resolve one, a piece tagged tier > 1 must be
    // reachable from a settlement assembly (a hidden tier that can never be
    // placed would silently never exist), and its tier must exist in that
    // settlement's declared tiers (else reveal could never happen).
    let mut assembly_settlements: Vec<Option<usize>> = reg.assemblies.iter().map(|_| None).collect();
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
                _ => BehaviorArchetype::Standard,
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
                        quest_errors.push(format!(
                            "{id}: quest unlocks unknown recipe {recipe_id}"
                        ));
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
                    recipe_errors.push(format!(
                        "{}: recipe blueprint {name} is unknown",
                        r.output
                    ));
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
                gate_errors
                    .push(format!("{modid}: gate feature missing `id`"));
                continue;
            };
            let id = qualify(&modid, gate_id);
            if reg.gates.iter().any(|g| g.id == id) {
                gate_errors
                    .push(format!("{id}: duplicate gate feature id"));
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
                    mode_errors
                        .push(format!("mode {id}: built-in mode id is reserved"));
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

    reconcile_material_definitions(&mut reg);
    // Gate feature errors survive past `validate_material_graph`, which
    // rebuilds `material_errors` from scratch.
    reg.material_errors.extend(gate_errors);
    // Settlement, quest-reward, and recipe-gate errors, likewise collected
    // locally.
    reg.material_errors.extend(settlement_errors);
    reg.material_errors.extend(recipe_errors);
    reg.material_errors.extend(mode_errors);
    reg.mods.append(&mut failed);
    reg
}

fn add_materials(into: &mut MaterialVector, from: &MaterialVector, multiplier: u64) {
    for (material, units) in from {
        *into.entry(material.clone()).or_default() = into
            .get(material)
            .copied()
            .unwrap_or_default()
            .saturating_add(units.saturating_mul(multiplier));
    }
    into.retain(|_, units| *units != 0);
}

fn ingredient_materials(reg: &Registry, ingredient: &Ingredient) -> Option<MaterialVector> {
    match ingredient {
        Ingredient::One(item) => Some(reg.item(*item).materials.clone()),
        Ingredient::Any(items) => {
            let first = items
                .first()
                .map(|item| reg.item(*item).materials.clone())?;
            items
                .iter()
                .all(|item| reg.item(*item).materials == first)
                .then_some(first)
        }
    }
}

fn subtract_materials(total: &MaterialVector, sinks: &MaterialVector) -> Option<MaterialVector> {
    let mut left = total.clone();
    for (material, units) in sinks {
        let value = left.get_mut(material)?;
        *value = value.checked_sub(*units)?;
    }
    left.retain(|_, units| *units != 0);
    Some(left)
}

fn per_item_materials(total: &MaterialVector, count: u32) -> Option<MaterialVector> {
    let divisor = u64::from(count.max(1));
    total
        .iter()
        .map(|(material, units)| {
            units
                .is_multiple_of(divisor)
                .then(|| (material.clone(), units / divisor))
        })
        .collect()
}

fn set_materials_if_missing(reg: &mut Registry, item: ItemId, materials: MaterialVector) -> bool {
    if reg.item(item).materials_declared || reg.item(item).materials == materials {
        return false;
    }
    let definition = &mut reg.items[item.0 as usize];
    definition.materials = materials;
    if !matches!(
        definition.material_class,
        MaterialClass::Consumptive | MaterialClass::Exceptional
    ) {
        definition.material_class = MaterialClass::GeologicallyFinite;
    }
    true
}

/// Close material identity over all transformation graphs, then validate the
/// fixed point. Content authors annotate geological sources and intentional
/// sinks; ordinary components inherit exact constituents from their recipes.
fn reconcile_material_definitions(reg: &mut Registry) {
    for _ in 0..reg.items.len().min(64) {
        let mut changed = false;
        for recipe in reg.recipes.clone() {
            let mut input = MaterialVector::new();
            let mut known = true;
            for ingredient in recipe.pattern.iter().flatten() {
                if let Some(vector) = ingredient_materials(reg, ingredient) {
                    add_materials(&mut input, &vector, 1);
                } else {
                    known = false;
                }
            }
            // A blueprint item is extra input consumed on craft (spec 3.5).
            if let Some(blueprint) = recipe.blueprint {
                add_materials(&mut input, &reg.item(blueprint).materials, 1);
            }
            if !known {
                continue;
            }
            let mut sinks = recipe.loss.clone();
            for (item, count) in &recipe.byproducts {
                add_materials(&mut sinks, &reg.item(*item).materials, u64::from(*count));
            }
            if let Some(remaining) = subtract_materials(&input, &sinks)
                && let Some(per_item) = per_item_materials(&remaining, recipe.count)
            {
                changed |= set_materials_if_missing(reg, recipe.output, per_item);
            }
        }
        for smelt in reg.smelts.clone() {
            let Some(input) = ingredient_materials(reg, &smelt.input) else {
                continue;
            };
            let mut sinks = smelt.loss.clone();
            if let Some((item, count)) = smelt.spit {
                add_materials(&mut sinks, &reg.item(item).materials, u64::from(count));
            }
            if let Some(output) = subtract_materials(&input, &sinks) {
                changed |= set_materials_if_missing(reg, smelt.output, output);
            }
        }
        for worked in reg.worked.clone() {
            let input = reg.item(worked.input).materials.clone();
            if let Some(remaining) = subtract_materials(&input, &worked.loss)
                && let Some(output) = per_item_materials(&remaining, worked.count)
            {
                changed |= set_materials_if_missing(reg, worked.output, output);
            }
        }
        for bloomery in reg.bloomery.clone() {
            let input = reg.item(bloomery.charge).materials.clone();
            changed |= set_materials_if_missing(reg, bloomery.bloom, input);
        }
        if !changed {
            break;
        }
    }

    // A block's held form and placed form are one material object. Ore blocks
    // without a held form inherit their exact drop vector.
    for block_index in 0..reg.blocks.len() {
        let block_id = BlockId(block_index as u16);
        let direct = reg
            .item_id(&reg.blocks[block_index].name)
            .map(|item| reg.item(item).materials.clone())
            .filter(|materials| !materials.is_empty());
        let dropped = reg.blocks[block_index]
            .drops
            .map(|(item, count)| {
                let mut materials = MaterialVector::new();
                add_materials(&mut materials, &reg.item(item).materials, u64::from(count));
                materials
            })
            .filter(|materials| !materials.is_empty());
        if let Some(materials) = direct.or(dropped) {
            reg.blocks[block_index].materials = materials;
            reg.blocks[block_index].material_class = MaterialClass::GeologicallyFinite;
        }
        // Creative-only state items are still classified and carry identity
        // if an operator places one into a survival world.
        for item in &mut reg.items {
            if item.places == Some(block_id) && item.materials.is_empty() {
                item.materials = reg.blocks[block_index].materials.clone();
                item.material_class = reg.blocks[block_index].material_class;
            }
        }
    }

    // The assembly bench changes three physical components into one composite
    // without using the ordinary crafting graph (the Wellglass owner must
    // survive). Give the completed lens exactly the frame + Echo Slate
    // constituents so the finite-material audit sees a relabel, not a sink.
    if let (Some(frame), Some(slate), Some(mount), Some(lens)) = (
        reg.item_id("base:tuning_lens_frame"),
        reg.item_id("base:echo_slate"),
        reg.item_id("base:tuning_lens_mount"),
        reg.item_id("base:tuning_lens"),
    ) {
        let mut materials = reg.item(frame).materials.clone();
        add_materials(&mut materials, &reg.item(slate).materials, 1);
        reg.items[mount.0 as usize].materials = materials.clone();
        reg.items[mount.0 as usize].materials_declared = true;
        reg.items[lens.0 as usize].materials = materials;
        reg.items[lens.0 as usize].materials_declared = true;
    }

    for item in &mut reg.items {
        if !item.materials.is_empty() && item.salvage.is_none() {
            // Food reuses the durability field as a freshness clock. It is a
            // consumable, not a metal object that belongs in a forge.
            if (item.durability > 0 && item.food.is_none()) || item.armor.is_some() {
                item.salvage = Some(SalvageDef {
                    station: "forge".into(),
                    recovery_permille: 900,
                });
            } else if item.places.is_some() {
                item.salvage = Some(SalvageDef {
                    station: "dismantling".into(),
                    recovery_permille: 950,
                });
            }
        }
    }

    register_salvage_content(reg);
    validate_material_graph(reg);
    validate_arcane_graph(reg);
    validate_arcane_ecology_graph(reg);
    validate_dross_scar_graph(reg);
}

fn validate_dross_scar_graph(reg: &mut Registry) {
    const MAX_DROSS_SCAR_DEFINITIONS: usize = 4_096;
    let mut errors = Vec::new();
    if reg.dross_scars.len() > MAX_DROSS_SCAR_DEFINITIONS {
        errors.push(format!(
            "dross scar registry exceeds its {MAX_DROSS_SCAR_DEFINITIONS}-definition safety bound"
        ));
    }
    for definition in reg.dross_scars.values() {
        let block = reg.block(definition.block);
        let has_scar_observation = block.observation.as_ref().is_some_and(|observation| {
            observation
                .categories
                .iter()
                .any(|category| category == "scar")
                && observation
                    .properties
                    .iter()
                    .any(|property| property == "dross")
        });
        if block.solid
            || block.opaque
            || block.interaction.is_some()
            || block.water_level.is_some()
            || block.hardness.is_none()
            || block.height.is_some_and(|height| height > 0.25)
            || !has_scar_observation
        {
            errors.push(format!(
                "{}: a scar must be removable, nonstructural, non-fluid, inventory-free, at most quarter-height, and visibly categorized as scar/dross",
                definition.content_id
            ));
        }
        let Some((drop, count)) = block.drops else {
            errors.push(format!(
                "{}: a scar lifecycle needs one recoverable contained drop",
                definition.content_id
            ));
            continue;
        };
        let drop = reg.item(drop);
        if count != 1
            || drop.max_stack != 1
            || drop.arcane.as_ref().is_none_or(|arcane| {
                arcane.capacity == 0
                    || !matches!(
                        arcane.on_destroy,
                        ArcaneDisposition::Dross | ArcaneDisposition::Scar
                    )
            })
        {
            errors.push(format!(
                "{}: scar recovery must yield exactly one finite-capacity, non-erasing arcane item",
                definition.content_id
            ));
        }
        if definition.handler == crate::dross::ScarHandler::WaterMarginFilm
            && !definition
                .carriers
                .contains(&crate::dross::DrossCarrier::Water)
        {
            errors.push(format!(
                "{}: a water-margin film must accept waterborne dross",
                definition.content_id
            ));
        }
        if definition.handler == crate::dross::ScarHandler::MineralCrust
            && !definition
                .carriers
                .contains(&crate::dross::DrossCarrier::Soil)
        {
            errors.push(format!(
                "{}: a mineral crust must accept soil/sediment dross",
                definition.content_id
            ));
        }
    }
    for kind in crate::dross::ScarKind::ALL {
        if !reg
            .dross_scars
            .values()
            .any(|definition| definition.provider == "base" && definition.kind == kind)
        {
            errors.push(format!(
                "base content needs a safe fallback dross scar for {kind:?}"
            ));
        }
    }
    reg.arcane_errors.extend(errors);
}

fn validate_arcane_ecology_graph(reg: &mut Registry) {
    let mut errors = Vec::new();
    let mut base_roles = BTreeMap::<EcologyRole, usize>::new();
    for block in &reg.blocks {
        let Some(ecology) = &block.arcane_ecology else {
            continue;
        };
        if block.arcane.is_none() {
            errors.push(format!(
                "{}: magical ecology needs an arcane destruction disposition",
                block.name
            ));
        }
        if ecology.charge_capacity > block.arcane.as_ref().map_or(0, |arcane| arcane.capacity) {
            errors.push(format!(
                "{}: ecology capacity exceeds the block's conserved Current capacity",
                block.name
            ));
        }
        if block.name.starts_with("base:") && ecology.kind != ArcaneEcologyKind::FiniteMineral {
            for role in &ecology.roles {
                *base_roles.entry(*role).or_default() += 1;
            }
        }
        if block.harvest.is_some() {
            errors.push(format!(
                "{}: ecology harvest cannot also use the ordinary repeatable block-harvest path",
                block.name
            ));
        }
        if ecology.kind == ArcaneEcologyKind::FiniteMineral
            && (block.material_class != MaterialClass::GeologicallyFinite
                || block.materials.is_empty()
                || !reg
                    .ores
                    .iter()
                    .any(|ore| ore.block == reg.block_by_name[&block.name]))
        {
            errors.push(format!(
                "{}: finite resonant geology needs a finite material identity and deposit rule",
                block.name
            ));
        }
        if ecology.kind == ArcaneEcologyKind::Crystal && (block.drops.is_none() || block.cross) {
            errors.push(format!(
                "{}: a regenerative crystal needs a physical shard drop and cluster block",
                block.name
            ));
        }
    }
    for required in [
        EcologyRole::Gatherer,
        EcologyRole::Reservoir,
        EcologyRole::Conductor,
        EcologyRole::Transformer,
        EcologyRole::Indicator,
        EcologyRole::Stabilizer,
        EcologyRole::Catalyst,
    ] {
        if base_roles.get(&required).copied().unwrap_or(0) < 2 {
            errors.push(format!(
                "base magical ecology needs two reachable renewable {:?} lifecycles",
                required
            ));
        }
    }
    reg.arcane_errors.extend(errors);
}

fn split_recovery(
    materials: &MaterialVector,
    recovery_permille: u16,
) -> (MaterialVector, MaterialVector) {
    let mut recovered = MaterialVector::new();
    let mut remainder = MaterialVector::new();
    for (material, units) in materials {
        let keep = units.saturating_mul(u64::from(recovery_permille)) / 1000;
        if keep != 0 {
            recovered.insert(material.clone(), keep);
        }
        if *units != keep {
            remainder.insert(material.clone(), units - keep);
        }
    }
    (recovered, remainder)
}

fn push_salvage_item(
    reg: &mut Registry,
    source: &ItemDef,
    suffix: &str,
    label_prefix: &str,
    materials: MaterialVector,
) -> ItemId {
    let item = ItemId(reg.items.len() as u16);
    let name = format!("{}/{suffix}", source.name);
    reg.items.push(ItemDef {
        name: name.clone(),
        label: format!("{label_prefix} {}", source.label),
        icon: source.icon,
        max_stack: 64,
        tool: None,
        durability: 0,
        places: None,
        food: None,
        damage: 1.0,
        damage_type: None,
        bow: None,
        ammo: None,
        armor: None,
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
        materials,
        materials_declared: true,
        material_class: MaterialClass::GeologicallyFinite,
        salvage: None,
        broken_into: None,
        arcane: None,
        arcane_ecology: None,
        observation: None,
        discovery: None,
    });
    reg.item_by_name.insert(name, item);
    item
}

fn register_salvage_content(reg: &mut Registry) {
    let durable = reg
        .items
        .iter()
        .take(reg.items.len())
        .enumerate()
        .filter(|(_, item)| {
            item.durability > 0 && item.food.is_none() && !item.materials.is_empty()
        })
        .map(|(index, item)| (ItemId(index as u16), item.clone()))
        .collect::<Vec<_>>();
    for (original_id, original) in durable {
        if original.name == "base:tuning_lens"
            && let Some(mount) = reg.item_id("base:tuning_lens_mount")
        {
            // The custom wear path consumes only the replaceable Wellglass
            // owner. The fitted frame and Echo Slate plate are one conserved
            // physical mount, not generic damaged salvage.
            reg.items[original_id.0 as usize].broken_into = Some(mount);
            continue;
        }
        let damaged = push_salvage_item(
            reg,
            &original,
            "damaged",
            "Damaged",
            original.materials.clone(),
        );
        reg.items[damaged.0 as usize].max_stack = 1;
        reg.items[damaged.0 as usize].salvage = Some(SalvageDef {
            station: "forge".into(),
            recovery_permille: 900,
        });
        reg.items[original_id.0 as usize].broken_into = Some(damaged);

        let (primitive, primitive_scale) = split_recovery(&original.materials, 750);
        let primitive_out = push_salvage_item(
            reg,
            &original,
            "primitive_scrap",
            "Crude Scrap from",
            primitive,
        );
        let primitive_tail = push_salvage_item(
            reg,
            &original,
            "primitive_scale",
            "Scale from",
            primitive_scale,
        );
        reg.recipes.push(RecipeDef {
            w: 1,
            h: 1,
            pattern: vec![Some(Ingredient::One(damaged))],
            output: primitive_out,
            count: 1,
            station: None,
            loss: MaterialVector::new(),
            byproducts: vec![(primitive_tail, 1)],
            tech: None,
            blueprint: None,
        });

        let (forge, forge_scale) = split_recovery(&original.materials, 900);
        let forge_out =
            push_salvage_item(reg, &original, "forge_scrap", "Forged Stock from", forge);
        let forge_tail = push_salvage_item(
            reg,
            &original,
            "forge_scale",
            "Forge Scale from",
            forge_scale,
        );
        reg.forge_salvage.push(ForgeSalvageDef {
            input: damaged,
            output: forge_out,
            byproduct: forge_tail,
            recovery_permille: 900,
        });
    }

    let machine_blocks = reg
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| {
            !block.materials.is_empty()
                && matches!(
                    block.interaction.as_deref(),
                    Some(
                        "furnace"
                            | "bloomery"
                            | "kiln"
                            | "forge"
                            | "anvil"
                            | "quern"
                            | "millstone"
                            | "sawmill"
                            | "lathe"
                            | "iron_lathe"
                            | "boring"
                            | "pump"
                            | "generator"
                            | "separator"
                            | "firebox"
                    )
                )
        })
        .filter_map(|(index, block)| {
            reg.item_id(&block.name)
                .map(|item| (BlockId(index as u16), reg.item(item).clone()))
        })
        .collect::<Vec<_>>();
    for (block, source) in machine_blocks {
        let bundle = push_salvage_item(
            reg,
            &source,
            "dismantling_bundle",
            "Dismantled",
            source.materials.clone(),
        );
        reg.items[bundle.0 as usize].max_stack = 1;
        reg.items[bundle.0 as usize].salvage = Some(SalvageDef {
            station: "dismantling".into(),
            recovery_permille: 950,
        });
        reg.blocks[block.0 as usize].dismantles_to = Some(bundle);
        let (recovered, scale) = split_recovery(&source.materials, 950);
        let output = push_salvage_item(
            reg,
            &source,
            "dismantled_stock",
            "Clean Stock from",
            recovered,
        );
        let byproduct = push_salvage_item(
            reg,
            &source,
            "dismantling_scale",
            "Dismantling Scale from",
            scale,
        );
        reg.forge_salvage.push(ForgeSalvageDef {
            input: bundle,
            output,
            byproduct,
            recovery_permille: 950,
        });
    }

    // Lit/running machine blocks deliberately have no item form; they drop
    // the canonical cold machine. Give those state variants the same clean
    // dismantling bundle instead of accidentally making a running machine a
    // 100%-recovery loophole.
    let inherited = reg
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| block.dismantles_to.is_none() && !block.materials.is_empty())
        .filter_map(|(index, block)| {
            let dropped_item = block.drops?.0;
            let canonical_block = reg.item(dropped_item).places?;
            let bundle = reg.block(canonical_block).dismantles_to?;
            Some((index, bundle))
        })
        .collect::<Vec<_>>();
    for (index, bundle) in inherited {
        reg.blocks[index].dismantles_to = Some(bundle);
    }
}

fn validate_material_graph(reg: &mut Registry) {
    let mut errors = Vec::new();
    for (index, recipe) in reg.recipes.iter().enumerate() {
        let mut input = MaterialVector::new();
        let mut valid_tags = true;
        for ingredient in recipe.pattern.iter().flatten() {
            if let Some(vector) = ingredient_materials(reg, ingredient) {
                add_materials(&mut input, &vector, 1);
            } else {
                valid_tags = false;
            }
        }
        // A blueprint item is extra input consumed on craft (spec 3.5).
        if let Some(blueprint) = recipe.blueprint {
            add_materials(&mut input, &reg.item(blueprint).materials, 1);
        }
        if !valid_tags {
            errors.push(format!(
                "recipe {index}: tag members have unequal material mass"
            ));
            continue;
        }
        let mut accounted = recipe.loss.clone();
        add_materials(
            &mut accounted,
            &reg.item(recipe.output).materials,
            u64::from(recipe.count),
        );
        for (item, count) in &recipe.byproducts {
            add_materials(
                &mut accounted,
                &reg.item(*item).materials,
                u64::from(*count),
            );
        }
        if input != accounted {
            errors.push(format!(
                "recipe {index} -> {} is not material-balanced: input {input:?}, accounted {accounted:?}",
                reg.item(recipe.output).name
            ));
        }
    }
    for (index, smelt) in reg.smelts.iter().enumerate() {
        let Some(input) = ingredient_materials(reg, &smelt.input) else {
            errors.push(format!(
                "smelt {index}: tag members have unequal material mass"
            ));
            continue;
        };
        let mut accounted = smelt.loss.clone();
        add_materials(&mut accounted, &reg.item(smelt.output).materials, 1);
        if let Some((item, count)) = smelt.spit {
            add_materials(&mut accounted, &reg.item(item).materials, u64::from(count));
        }
        if input != accounted {
            errors.push(format!(
                "smelt {index} -> {} is not material-balanced: input {input:?}, accounted {accounted:?}",
                reg.item(smelt.output).name
            ));
        }
    }
    for (index, worked) in reg.worked.iter().enumerate() {
        let input = reg.item(worked.input).materials.clone();
        let mut accounted = worked.loss.clone();
        add_materials(
            &mut accounted,
            &reg.item(worked.output).materials,
            u64::from(worked.count),
        );
        if input != accounted {
            errors.push(format!(
                "worked {index} -> {} is not material-balanced: input {input:?}, accounted {accounted:?}",
                reg.item(worked.output).name
            ));
        }
    }
    for (index, kiln) in reg.kiln.iter().enumerate() {
        if !reg.item(kiln.powder).materials.is_empty() && !kiln.consumes {
            errors.push(format!(
                "kiln {index} consumes finite {} without consumes = true",
                reg.item(kiln.powder).name
            ));
        }
    }
    for (index, bloomery) in reg.bloomery.iter().enumerate() {
        let input = &reg.item(bloomery.charge).materials;
        let output = &reg.item(bloomery.bloom).materials;
        if input != output {
            errors.push(format!(
                "bloomery {index} changes charge identity: {input:?} -> {output:?}"
            ));
        }
    }
    reg.material_errors = errors;
}

/// Transformation stations currently conserve or destroy an input item's
/// Current, but they do not have authority to mint a newly charged owner.
/// Reject content graphs that would therefore produce an unbacked magical
/// item. Natural discoveries and creature drops are bound at their world
/// source instead and are intentionally outside this graph.
fn validate_arcane_graph(reg: &mut Registry) {
    let mut errors = Vec::new();
    {
        let mut charged_output = |kind: &str, index: usize, item: ItemId| {
            if reg.item(item).arcane.is_some() && reg.item(item).implement.is_none() {
                errors.push(format!(
                    "{kind} {index} has charged output {}; transformations cannot create Current",
                    reg.item(item).name
                ));
            }
        };
        for (index, recipe) in reg.recipes.iter().enumerate() {
            let output = reg.item(recipe.output);
            let identity_preserving_lens_assembly = recipe.station.as_deref()
                == Some("lens_assembly_bench")
                && output
                    .discovery
                    .as_ref()
                    .is_some_and(|definition| definition.kind == "tuning_lens")
                && output.arcane.as_ref().is_some_and(|output_arcane| {
                    recipe.pattern.iter().flatten().any(|ingredient| {
                        let items: &[ItemId] = match ingredient {
                            Ingredient::One(item) => std::slice::from_ref(item),
                            Ingredient::Any(items) => items,
                        };
                        items
                            .iter()
                            .any(|item| reg.item(*item).arcane.as_ref() == Some(output_arcane))
                    })
                });
            if !identity_preserving_lens_assembly {
                charged_output("recipe", index, recipe.output);
            }
            for (item, _) in &recipe.byproducts {
                charged_output("recipe byproduct", index, *item);
            }
        }
        for (index, smelt) in reg.smelts.iter().enumerate() {
            charged_output("smelt", index, smelt.output);
            if let Some((item, _)) = smelt.spit {
                charged_output("smelt byproduct", index, item);
            }
        }
        for (index, worked) in reg.worked.iter().enumerate() {
            charged_output("worked recipe", index, worked.output);
        }
        for (index, kiln) in reg.kiln.iter().enumerate() {
            charged_output("kiln recipe", index, kiln.glass);
        }
        if let Some((_, _, output)) = reg.kiln_base {
            charged_output("kiln base", 0, output);
        }
        for (index, bloomery) in reg.bloomery.iter().enumerate() {
            charged_output("bloomery recipe", index, bloomery.bloom);
        }
        for (index, salvage) in reg.forge_salvage.iter().enumerate() {
            charged_output("forge salvage", index, salvage.output);
            charged_output("forge salvage byproduct", index, salvage.byproduct);
        }
    }
    let mut unsupported_input = |kind: &str, index: usize, item: ItemId| {
        if reg.item(item).arcane.is_some() {
            errors.push(format!(
                "{kind} {index} consumes charged input {}; that station has no Current transaction",
                reg.item(item).name
            ));
        }
    };
    for (index, worked) in reg.worked.iter().enumerate() {
        unsupported_input("worked recipe", index, worked.input);
    }
    for (index, kiln) in reg.kiln.iter().enumerate() {
        unsupported_input("kiln recipe", index, kiln.powder);
    }
    if let Some((sand, fuel, _)) = reg.kiln_base {
        unsupported_input("kiln base sand", 0, sand);
        unsupported_input("kiln base fuel", 0, fuel);
    }
    for (index, bloomery) in reg.bloomery.iter().enumerate() {
        unsupported_input("bloomery charge", index, bloomery.charge);
        unsupported_input("bloomery fuel", index, bloomery.fuel);
    }
    for (index, salvage) in reg.forge_salvage.iter().enumerate() {
        unsupported_input("forge salvage", index, salvage.input);
    }
    reg.arcane_errors.extend(errors);
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

impl Registry {
    /// Select one eligible declarative scar shell with a stable key. Existing
    /// sites persist the returned content id, so later pack reordering cannot
    /// repaint them. Per-definition regional caps prevent a mod from
    /// declaring an unbounded self-replicator.
    pub fn select_dross_scar(
        &self,
        kind: crate::dross::ScarKind,
        carrier: crate::dross::DrossCarrier,
        band: crate::dross::DrossBand,
        existing_in_region: &BTreeMap<String, usize>,
        stable_key: u64,
    ) -> Option<&crate::dross::DrossScarDef> {
        let eligible = self
            .dross_scars
            .values()
            .filter(|definition| {
                definition.kind == kind
                    && definition.carriers.contains(&carrier)
                    && band >= definition.min_band
                    && existing_in_region
                        .get(&definition.content_id)
                        .copied()
                        .unwrap_or_default()
                        < usize::from(definition.max_sites_per_region)
            })
            .collect::<Vec<_>>();
        if eligible.is_empty() {
            return self.dross_scars.values().find(|definition| {
                definition.provider == "base"
                    && definition.kind == kind
                    && band >= definition.min_band
                    && existing_in_region
                        .get(&definition.content_id)
                        .copied()
                        .unwrap_or_default()
                        < usize::from(definition.max_sites_per_region)
            });
        }
        Some(eligible[stable_key as usize % eligible.len()])
    }

    /// Resolve a persisted site. A removed provider leaves its stable identity
    /// in the save, but materialization uses the safe base shell for the same
    /// climate kind until that provider returns.
    pub fn resolve_dross_scar(
        &self,
        content_id: &str,
        kind: crate::dross::ScarKind,
    ) -> Option<&crate::dross::DrossScarDef> {
        self.dross_scars.get(content_id).or_else(|| {
            self.dross_scars
                .values()
                .find(|definition| definition.provider == "base" && definition.kind == kind)
        })
    }

    /// Resolve a world's `mode` string to its survival ruleset (capability
    /// E1). Built-ins are `survival` and `creative`; anything else is a
    /// mod-declared `[[mode]]`, chained through its `base`. A missing or
    /// broken mode falls back to survival so an unknown mode string never
    /// strips safety netting.
    pub fn ruleset_for(&self, mode: &str) -> crate::ruleset::Ruleset {
        if mode == "creative" {
            return crate::ruleset::Ruleset::creative();
        }
        let mut chain: Vec<&ModeDef> = Vec::new();
        let mut current = mode;
        for _ in 0..=self.modes.len() {
            let Some(def) = self.modes.iter().find(|d| d.id == current) else {
                break;
            };
            if chain.iter().any(|d| d.id == def.id) {
                return crate::ruleset::Ruleset::survival();
            }
            chain.push(def);
            current = def.base.as_deref().unwrap_or("survival");
        }
        let mut ruleset = crate::ruleset::Ruleset::survival();
        for def in chain.into_iter().rev() {
            ruleset.apply_overrides(def);
        }
        ruleset
    }

    pub fn install_saved_arcane_placeholders(
        &mut self,
        ledger: &crate::arcane::ArcaneLedger,
    ) -> usize {
        let mut added = 0;
        for (name, saved) in &ledger.block_manifests {
            if let Some(id) = self.block_id(name) {
                self.blocks[id.0 as usize].arcane = Some(saved.arcane.clone());
                continue;
            }
            let id = BlockId(self.blocks.len() as u16);
            let mut placeholder = self.block(self.unknown_block).clone();
            placeholder.name = name.clone();
            placeholder.label = format!("Missing charged content: {name}");
            placeholder.material_class = MaterialClass::Exceptional;
            placeholder.arcane = Some(saved.arcane.clone());
            self.blocks.push(placeholder);
            self.block_by_name.insert(name.clone(), id);
            added += 1;
        }
        for (name, saved) in &ledger.item_manifests {
            if let Some(id) = self.item_id(name) {
                self.items[id.0 as usize].arcane = Some(saved.arcane.clone());
                self.items[id.0 as usize].max_stack = 1;
                continue;
            }
            let id = ItemId(self.items.len() as u16);
            self.items.push(ItemDef {
                name: name.clone(),
                label: format!("Missing charged content: {name}"),
                icon: crate::atlas::UNKNOWN_SLOT,
                max_stack: 1,
                tool: None,
                durability: saved.durability,
                places: self.block_id(name),
                food: None,
                damage: 1.0,
                damage_type: None,
                bow: None,
                ammo: None,
                armor: None,
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
                materials: MaterialVector::new(),
                materials_declared: true,
                material_class: MaterialClass::Exceptional,
                salvage: None,
                broken_into: None,
                arcane: Some(saved.arcane.clone()),
                arcane_ecology: None,
                observation: None,
                discovery: None,
            });
            self.item_by_name.insert(name.clone(), id);
            added += 1;
        }
        added
    }

    /// Recreate named save placeholders before palette remapping. Their
    /// qualified names remain the removed mod's names, so chunks never get
    /// rewritten as an anonymous `base:unknown`; reinstalling the mod maps
    /// the same palette names back to the real definitions.
    pub fn install_saved_placeholders(
        &mut self,
        world: &Path,
        ledger: &crate::materials::MaterialLedger,
    ) -> std::io::Result<usize> {
        let Ok(palette) = std::fs::read_to_string(world.join("palette")) else {
            return Ok(0);
        };
        let mut added = 0;
        for name in palette
            .lines()
            .filter_map(|line| line.split_once(' ').map(|(_, name)| name.trim()))
        {
            if self.block_by_name.contains_key(name) {
                continue;
            }
            let mod_id = name.split_once(':').map(|(id, _)| id).unwrap_or_default();
            let materials = ledger
                .block_manifests
                .get(name)
                .map(|definition| definition.materials.clone())
                .or_else(|| {
                    ledger
                        .retrogen
                        .values()
                        .find(|record| record.resource_key == name || record.mod_id == mod_id)
                        .map(|record| record.unit_materials.clone())
                })
                .unwrap_or_default();
            let id = BlockId(self.blocks.len() as u16);
            let mut placeholder = self.block(self.unknown_block).clone();
            placeholder.name = name.to_string();
            placeholder.label = format!("Missing content: {name}");
            placeholder.materials = materials;
            if !placeholder.materials.is_empty() {
                placeholder.material_class = MaterialClass::GeologicallyFinite;
            }
            self.blocks.push(placeholder);
            self.block_by_name.insert(name.to_string(), id);
            added += 1;
        }
        for (name, saved) in &ledger.item_manifests {
            if self.item_by_name.contains_key(name) {
                continue;
            }
            let id = ItemId(self.items.len() as u16);
            self.items.push(ItemDef {
                name: name.clone(),
                label: format!("Missing content: {name}"),
                icon: crate::atlas::UNKNOWN_SLOT,
                max_stack: saved.max_stack.max(1),
                tool: None,
                durability: saved.durability,
                places: self.block_id(name),
                food: None,
                damage: 1.0,
                damage_type: None,
                bow: None,
                ammo: None,
                armor: None,
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
                materials: saved.materials.clone(),
                materials_declared: true,
                material_class: saved.material_class,
                salvage: None,
                broken_into: None,
                arcane: None,
                arcane_ecology: None,
                observation: None,
                discovery: None,
            });
            self.item_by_name.insert(name.clone(), id);
            added += 1;
        }
        Ok(added)
    }
}

#[cfg(test)]
mod arcane_schema_tests {
    use super::*;

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

