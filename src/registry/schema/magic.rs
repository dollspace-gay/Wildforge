//! Raw magic content schema; no runtime mutation.

use crate::registry::{
    ArcaneDisposition, ArcaneEcologyKind, EcologyHarvestClass, EcologyRole, EcologySource,
    ReproductionMode,
};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, Clone)]
pub(in crate::registry) struct ObservationToml {
    #[serde(default)]
    pub(in crate::registry) categories: Vec<String>,
    #[serde(default)]
    pub(in crate::registry) properties: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub(in crate::registry) struct DiscoveryItemToml {
    pub(in crate::registry) kind: String,
    #[serde(default)]
    pub(in crate::registry) evidence_class: Option<String>,
    #[serde(default)]
    pub(in crate::registry) authored_text: Vec<String>,
    #[serde(default)]
    pub(in crate::registry) calibration: Option<crate::discovery::CalibrationGrade>,
    #[serde(default)]
    pub(in crate::registry) experiment: Option<crate::discovery::ExperimentKind>,
}

#[derive(Debug, Deserialize, Clone)]
pub(in crate::registry) struct DiscoveryFixtureToml {
    pub(in crate::registry) kind: String,
    #[serde(default)]
    pub(in crate::registry) experiments: Vec<crate::discovery::ExperimentKind>,
    #[serde(default)]
    pub(in crate::registry) record_capacity: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub(in crate::registry) struct ArcaneContentToml {
    pub(in crate::registry) capacity: u64,
    pub(in crate::registry) conductivity: u16,
    pub(in crate::registry) stability: u16,
    pub(in crate::registry) resonance: BTreeMap<String, u16>,
    pub(in crate::registry) on_destroy: ArcaneDisposition,
}

#[derive(Debug, Deserialize, Clone)]
pub(in crate::registry) struct ArcaneEcologyToml {
    pub(in crate::registry) roles: Vec<EcologyRole>,
    #[serde(default = "default_ecology_kind")]
    pub(in crate::registry) kind: ArcaneEcologyKind,
    pub(in crate::registry) habitat: Vec<String>,
    pub(in crate::registry) charge_capacity: u64,
    pub(in crate::registry) uptake_per_day: u32,
    #[serde(default)]
    pub(in crate::registry) release_per_day: u32,
    #[serde(default = "default_ecology_source")]
    pub(in crate::registry) source: EcologySource,
    pub(in crate::registry) resonance: BTreeMap<String, u16>,
    pub(in crate::registry) dross_tolerance: u32,
    #[serde(default)]
    pub(in crate::registry) water_per_day_hu: u32,
    #[serde(default)]
    pub(in crate::registry) nutrient_per_day: u16,
    #[serde(default = "default_reproduction")]
    pub(in crate::registry) reproduction: ReproductionMode,
    #[serde(default = "all_seasons")]
    pub(in crate::registry) seasons: [bool; 4],
    pub(in crate::registry) carrying_capacity: u16,
    pub(in crate::registry) harvest: EcologyHarvestClass,
    pub(in crate::registry) regrowth_days: u16,
    #[serde(default)]
    pub(in crate::registry) min_stability: u16,
    #[serde(default = "permille")]
    pub(in crate::registry) max_stability: u16,
    #[serde(default)]
    pub(in crate::registry) min_richness: u16,
    #[serde(default)]
    pub(in crate::registry) crystal_stages: u8,
    #[serde(default)]
    pub(in crate::registry) preserving_tool_tier: u8,
}

pub(in crate::registry) fn default_ecology_kind() -> ArcaneEcologyKind {
    ArcaneEcologyKind::Organism
}

pub(in crate::registry) fn default_ecology_source() -> EcologySource {
    EcologySource::Ambient
}

pub(in crate::registry) fn default_reproduction() -> ReproductionMode {
    ReproductionMode::Seed
}

pub(in crate::registry) fn all_seasons() -> [bool; 4] {
    [true; 4]
}

pub(in crate::registry) fn permille() -> u16 {
    1_000
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct ResonanceToml {
    pub(in crate::registry) id: String,
    #[serde(default)]
    pub(in crate::registry) label: Option<String>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct ArcaneSiteToml {
    pub(in crate::registry) id: String,
    #[serde(default)]
    pub(in crate::registry) requires: Vec<String>,
    #[serde(default = "default_capacity_factor")]
    pub(in crate::registry) capacity_factor: f32,
    #[serde(default)]
    pub(in crate::registry) resonance: BTreeMap<String, u16>,
    #[serde(default = "default_arcane_site_rarity")]
    pub(in crate::registry) rarity: f32,
    #[serde(default = "default_arcane_site_radius")]
    pub(in crate::registry) radius_cells: u16,
}

pub(in crate::registry) fn default_capacity_factor() -> f32 {
    1.0
}

pub(in crate::registry) fn default_arcane_site_rarity() -> f32 {
    0.01
}

pub(in crate::registry) fn default_arcane_site_radius() -> u16 {
    2
}

#[derive(Deserialize, Default)]
pub(in crate::registry) struct ArcaneFile {
    #[serde(default)]
    pub(in crate::registry) schema_version: Option<u32>,
    #[serde(default)]
    pub(in crate::registry) resonance: Vec<ResonanceToml>,
    #[serde(default, rename = "arcane_site")]
    pub(in crate::registry) sites: Vec<ArcaneSiteToml>,
}
