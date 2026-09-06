//! Sparse player history independent of immutable genesis.

use crate::planet_atlas::AtlasPos;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NamedAtlasPlace {
    pub id: u64,
    pub name: String,
    pub pos: AtlasPos,
}

/// Sparse player history is intentionally not baked into immutable genesis.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistoryLayers {
    pub version: u32,
    #[serde(default)]
    pub regional_ire: BTreeMap<AtlasPos, i16>,
    #[serde(default)]
    pub regional_tending: BTreeMap<AtlasPos, u64>,
    #[serde(default)]
    pub heart_state: BTreeMap<u32, String>,
    #[serde(default)]
    pub named_places: Vec<NamedAtlasPlace>,
    #[serde(default)]
    pub waystones: Vec<NamedAtlasPlace>,
    #[serde(default)]
    pub touched_cells: BTreeSet<AtlasPos>,
    #[serde(default)]
    pub bloom: BTreeMap<AtlasPos, u32>,
    #[serde(default)]
    pub exhaustion: BTreeMap<AtlasPos, u32>,
    #[serde(default)]
    pub resource_extraction: BTreeMap<u32, u64>,
    #[serde(default)]
    pub retrogen_stamps: BTreeMap<String, Vec<u32>>,
}
