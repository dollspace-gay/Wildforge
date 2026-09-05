//! Content material classes and declarative magic/ecology contracts.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap};

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

