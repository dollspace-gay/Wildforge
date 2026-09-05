//! Raw manifest content schema; no runtime mutation.

use crate::registry::{RetrogenPolicy};
use serde::Deserialize;

#[derive(Deserialize)]
pub(in crate::registry) struct ModToml {
    pub(in crate::registry) id: String,
    #[serde(default)]
    pub(in crate::registry) world_api: Option<u32>,
    #[serde(default)]
    pub(in crate::registry) name: Option<String>,
    #[serde(default)]
    pub(in crate::registry) version: Option<String>,
    #[serde(default)]
    pub(in crate::registry) depends: Vec<String>,
    #[serde(default)]
    pub(in crate::registry) retrogen: Option<RetrogenPolicy>,
}

/// A `[[mode]]` entry from `modes.toml`: a named ruleset a world's `mode`
/// string can name. The mode inherits every field from its `base`
/// (built-in "survival" or "creative", or another declared mode) and
/// overrides the fields it declares.
#[derive(Deserialize, Clone)]
pub(in crate::registry) struct ModeToml {
    pub(in crate::registry) id: String,
    #[serde(default)]
    pub(in crate::registry) base: Option<String>,
    #[serde(default)]
    pub(in crate::registry) creative: Option<bool>,
    #[serde(default)]
    pub(in crate::registry) hunger: Option<bool>,
    #[serde(default)]
    pub(in crate::registry) fall_damage: Option<bool>,
    #[serde(default)]
    pub(in crate::registry) drowning: Option<bool>,
    #[serde(default)]
    pub(in crate::registry) lava_burn: Option<bool>,
    #[serde(default)]
    pub(in crate::registry) hostile_spawns: Option<bool>,
    #[serde(default)]
    pub(in crate::registry) ire: Option<bool>,
    #[serde(default)]
    pub(in crate::registry) hearts: Option<bool>,
    #[serde(default)]
    pub(in crate::registry) weather_extremes: Option<bool>,
    #[serde(default)]
    pub(in crate::registry) pvp: Option<bool>,
    #[serde(default)]
    pub(in crate::registry) skills: Option<bool>,
    #[serde(default)]
    pub(in crate::registry) equipment: Option<bool>,
    #[serde(default)]
    pub(in crate::registry) industrial_ire: Option<bool>,
    #[serde(default)]
    pub(in crate::registry) nest_spawns: Option<bool>,
}

