//! Raw blocks content schema; no runtime mutation.

use super::{ArcaneContentToml, ArcaneEcologyToml, BrushToml, DiscoveryFixtureToml, ObservationToml};
use crate::registry::{MaterialClass, MaterialVector, ToolKind};
use serde::Deserialize;

#[derive(Deserialize, Clone)]
#[serde(untagged)]
pub(in crate::registry) enum TexSpec {
    One(String),
    Faces {
        top: String,
        side: String,
        #[serde(default)]
        bottom: Option<String>,
    },
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct BlockToml {
    pub(in crate::registry) id: String,
    pub(in crate::registry) name: Option<String>,
    pub(in crate::registry) texture: TexSpec,
    #[serde(default)]
    pub(in crate::registry) hardness: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) unbreakable: bool,
    #[serde(default)]
    pub(in crate::registry) tool: Option<ToolKind>,
    #[serde(default)]
    pub(in crate::registry) requires_tool: bool,
    /// "self" (default), "none", or an item id.
    #[serde(default)]
    pub(in crate::registry) drops: Option<String>,
    #[serde(default)]
    pub(in crate::registry) drop_count: Option<u32>,
    #[serde(default = "yes")]
    pub(in crate::registry) solid: bool,
    #[serde(default = "yes")]
    pub(in crate::registry) opaque: bool,
    #[serde(default)]
    pub(in crate::registry) interaction: Option<String>,
    #[serde(default)]
    pub(in crate::registry) min_tier: u8,
    #[serde(default)]
    pub(in crate::registry) water: Option<u8>,
    #[serde(default)]
    pub(in crate::registry) lava: Option<u8>,
    #[serde(default)]
    pub(in crate::registry) cross: bool,
    #[serde(default)]
    pub(in crate::registry) burns: u8,
    #[serde(default)]
    pub(in crate::registry) floats: bool,
    #[serde(default)]
    pub(in crate::registry) shape: Option<String>,
    #[serde(default)]
    pub(in crate::registry) crop: Option<CropToml>,
    #[serde(default)]
    pub(in crate::registry) harvest: Option<HarvestToml>,
    #[serde(default)]
    pub(in crate::registry) icon: Option<String>,
    #[serde(default)]
    pub(in crate::registry) light: u8,
    #[serde(default)]
    pub(in crate::registry) sapling: Option<SaplingToml>,
    #[serde(default)]
    pub(in crate::registry) bonus_drop: Option<BonusDropToml>,
    #[serde(default)]
    pub(in crate::registry) brush: Option<BrushToml>,
    /// Optional glow color (r,g,b, each 0..1); brightest channel scales to
    /// `light`. Omit for white light.
    #[serde(default)]
    pub(in crate::registry) light_color: Option<[f32; 3]>,
    /// Render height 0..1 (thin slabs like snow layers).
    #[serde(default)]
    pub(in crate::registry) height: Option<f32>,
    /// Gravity: detaches and falls when unsupported (sand, gravel).
    #[serde(default)]
    pub(in crate::registry) falls: bool,
    /// Four top-face textures by fertility quartile (soil blocks).
    #[serde(default)]
    pub(in crate::registry) texture_fertility: Option<Vec<String>>,
    /// Counts as glazing: passes sky light and makes greenhouses.
    #[serde(default)]
    pub(in crate::registry) glass: bool,
    /// Stained light: which RGB channels of block light pass through
    /// (e.g. [1, 0, 0] for red glass). Default: all.
    #[serde(default)]
    pub(in crate::registry) light_filter: Option<[u8; 3]>,
    /// Register an item form for placing (default true).
    #[serde(default = "yes")]
    pub(in crate::registry) item: bool,
    #[serde(default)]
    pub(in crate::registry) material_class: Option<MaterialClass>,
    #[serde(default)]
    pub(in crate::registry) materials: MaterialVector,
    /// Pattern A heat contribution (see BlockDef::heat_retention).
    #[serde(default)]
    pub(in crate::registry) heat_retention: u32,
    #[serde(default)]
    pub(in crate::registry) arcane: Option<ArcaneContentToml>,
    #[serde(default)]
    pub(in crate::registry) observation: Option<ObservationToml>,
    #[serde(default)]
    pub(in crate::registry) discovery_fixture: Option<DiscoveryFixtureToml>,
    #[serde(default)]
    pub(in crate::registry) arcane_ecology: Option<ArcaneEcologyToml>,
    #[serde(default)]
    pub(in crate::registry) dross_scar: Option<DrossScarToml>,
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub(in crate::registry) struct DrossScarToml {
    pub(in crate::registry) kind: crate::dross::ScarKind,
    pub(in crate::registry) handler: crate::dross::ScarHandler,
    pub(in crate::registry) carriers: Vec<crate::dross::DrossCarrier>,
    pub(in crate::registry) min_band: crate::dross::DrossBand,
    #[serde(default)]
    pub(in crate::registry) status: Option<crate::dross::ScarStatusHandler>,
    #[serde(default)]
    pub(in crate::registry) activity: Option<crate::dross::ScarActivityHandler>,
    #[serde(default = "one_u8")]
    pub(in crate::registry) max_sites_per_region: u8,
}

pub(in crate::registry) const fn one_u8() -> u8 {
    1
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct SaplingToml {
    pub(in crate::registry) tree: String,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct BonusDropToml {
    pub(in crate::registry) item: String,
    pub(in crate::registry) chance: f32,
}

pub(in crate::registry) fn yes() -> bool {
    true
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct CropToml {
    pub(in crate::registry) stages: u8,
    #[serde(default)]
    pub(in crate::registry) next_chance: Option<f32>,
    /// Texture per stage (else the block texture is reused).
    #[serde(default)]
    pub(in crate::registry) stage_textures: Vec<String>,
    #[serde(default)]
    pub(in crate::registry) any_soil: bool,
    /// Rotation family 1..=3 (defaults to a stable name hash).
    #[serde(default)]
    pub(in crate::registry) family: Option<u8>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct HarvestToml {
    pub(in crate::registry) item: String,
    #[serde(default)]
    pub(in crate::registry) count: Option<u32>,
    pub(in crate::registry) becomes: String,
}

