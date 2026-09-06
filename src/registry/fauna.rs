//! Wildlife appearance, behavior, spawning, and habitat definitions.

use super::{ArcaneContentDef, BlockId, ItemId};
use serde::Deserialize;
use std::collections::HashMap;

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
    /// A flat-out sprinter: while hunting it closes at a burst multiple of
    /// its speed and its melee swings are never delayed.
    Rusher,
    /// A living wall: reduced knockback (holds its ground) on top of its
    /// authored `resist` data.
    Tank,
    /// Keeps a preferred distance band while hunting: retreats when too
    /// close, closes when too far, fires from inside the band.
    Sniper,
    /// A field medic: periodically heals its allies inside a radius.
    Support,
    /// A brood mother: on death it releases a swarm of a companion species.
    Swarm,
    /// A minion commander: periodically spawns its companions while it
    /// lives and fights.
    Controller,
    /// Blinks: on a cooldown it teleports to a valid spot near its target
    /// before striking.
    Phaser,
    /// A shielded front: damage from the direction it faces is reduced;
    /// it stays vulnerable from behind.
    ShieldBearer,
}

/// Data-driven knobs for the E9 behavior archetypes. Each archetype reads
/// its own `Option` and falls back to engine defaults when the mod omits it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArchetypeParams {
    pub rusher: Option<RusherDef>,
    pub tank: Option<TankDef>,
    pub sniper: Option<SniperDef>,
    pub support: Option<SupportDef>,
    pub swarm: Option<SwarmDef>,
    pub controller: Option<ControllerDef>,
    pub phaser: Option<PhaserDef>,
    pub shield: Option<ShieldDef>,
}

/// Sprint multiple while hunting (default 2.4×).
#[derive(Clone, Debug, PartialEq)]
pub struct RusherDef {
    pub rush_mult: f32,
}

/// Knockback taken while defending (default 0.25×).
#[derive(Clone, Debug, PartialEq)]
pub struct TankDef {
    pub knockback_mult: f32,
}

/// Preferred engagement band in blocks (defaults 9..16).
#[derive(Clone, Debug, PartialEq)]
pub struct SniperDef {
    pub keep_min: f32,
    pub keep_max: f32,
}

/// Ally-heal pulse: radius, seconds between pulses, health restored.
#[derive(Clone, Debug, PartialEq)]
pub struct SupportDef {
    pub radius: f32,
    pub interval: f32,
    pub heal: f32,
}

/// Death burst: the companion species (by qualified id) and how many.
#[derive(Clone, Debug, PartialEq)]
pub struct SwarmDef {
    pub spawn: String,
    pub count: u32,
}

/// Recurring minion spawner: the companion species, how many each wave,
/// seconds between waves, and the max living at once.
#[derive(Clone, Debug, PartialEq)]
pub struct ControllerDef {
    pub spawn: String,
    pub count: u32,
    pub interval: f32,
    pub max: u32,
}

/// Blink: how far a phase-jump may reach and its cooldown.
#[derive(Clone, Debug, PartialEq)]
pub struct PhaserDef {
    pub blink_range: f32,
    pub blink_cd: f32,
}

/// Front-facing damage multiplier (default 0.35×) and the facing cone.
#[derive(Clone, Debug, PartialEq)]
pub struct ShieldDef {
    pub front_mult: f32,
    pub front_deg: f32,
}

/// A nest/dens spawn-gate (capability E9): a world block whose presence
/// lets its species respawn nearby; breaking it stops the respawns.
#[derive(Clone, Debug)]
pub struct NestDef {
    pub id: String,
    /// Qualified block id that marks the nest.
    pub block: BlockId,
    /// Qualified species id the nest spawns.
    pub species: usize,
    /// Blocks from the nest a spawn may land.
    pub radius: f32,
    /// Seconds between spawn attempts per nest.
    pub interval: f32,
    /// Max living spawns of this species within `radius` of the nest.
    pub cap: u32,
}

/// The raw `[[nest]]` row from `nests.toml` (mods own the content).
#[derive(Deserialize, Clone)]
pub struct RawNestToml {
    pub id: String,
    #[serde(default)]
    pub block: String,
    #[serde(default)]
    pub species: String,
    #[serde(default)]
    pub radius: Option<f32>,
    #[serde(default)]
    pub interval: Option<f32>,
    #[serde(default)]
    pub cap: Option<u32>,
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
    /// E9 archetype knobs: one Option per archetype, engine defaults when
    /// the mod omits them.
    pub archetype: ArchetypeParams,
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
    /// Capability E15: a non-hostile creature that defends its area from
    /// hostile mobs. Guards pick the nearest hostile as quarry, approach,
    /// and deal contact damage per swing instead of instant-killing.
    pub guards: bool,
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
