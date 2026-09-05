//! Stable TOML encoding for sparse history and genesis models.

use crate::planet_atlas::{AtlasError, BiomeModel, GeologyModel, HistoryLayers, HydrologyModel};

pub(in crate::planet_atlas) fn encode_history(history: &HistoryLayers) -> Result<String, AtlasError> {
    toml::to_string_pretty(history)
        .map_err(|error| AtlasError::Corrupt(format!("history encoding failed: {error}")))
}

pub(in crate::planet_atlas) fn encode_geology(geology: &GeologyModel) -> Result<String, AtlasError> {
    toml::to_string_pretty(geology)
        .map_err(|error| AtlasError::Corrupt(format!("geology encoding failed: {error}")))
}

pub(in crate::planet_atlas) fn encode_hydrology(hydrology: &HydrologyModel) -> Result<String, AtlasError> {
    toml::to_string_pretty(hydrology)
        .map_err(|error| AtlasError::Corrupt(format!("hydrology encoding failed: {error}")))
}

pub(in crate::planet_atlas) fn encode_biomes(biomes: &BiomeModel) -> Result<String, AtlasError> {
    toml::to_string_pretty(biomes)
        .map_err(|error| AtlasError::Corrupt(format!("biome encoding failed: {error}")))
}
