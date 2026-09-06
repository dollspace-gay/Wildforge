//! Raw recipes content schema; no runtime mutation.

use super::one_u32;
use crate::registry::MaterialVector;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct RecipeToml {
    pub(in crate::registry) pattern: Vec<String>,
    #[serde(default)]
    pub(in crate::registry) keys: HashMap<String, String>,
    pub(in crate::registry) output: String,
    #[serde(default)]
    pub(in crate::registry) count: Option<u32>,
    #[serde(default)]
    pub(in crate::registry) station: Option<String>,
    #[serde(default)]
    pub(in crate::registry) loss: MaterialVector,
    #[serde(default)]
    pub(in crate::registry) byproducts: Vec<ByproductToml>,
    /// Per-player KV key that must read truthy to craft (spec 3.5). A recipe
    /// without a `tech` field has no tech gate; `learn_recipe` quest rewards
    /// unlock the runtime default `learned:<recipe_id>` key.
    #[serde(default)]
    pub(in crate::registry) tech: Option<String>,
    /// Item consumed from the player's inventory (not the grid) on a
    /// successful craft (spec 3.5).
    #[serde(default)]
    pub(in crate::registry) blueprint: Option<String>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct ByproductToml {
    pub(in crate::registry) item: String,
    #[serde(default = "one_u32")]
    pub(in crate::registry) count: u32,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct SmeltToml {
    pub(in crate::registry) input: String,
    pub(in crate::registry) output: String,
    #[serde(default)]
    pub(in crate::registry) time: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) spit: Option<SpitToml>,
    #[serde(default)]
    pub(in crate::registry) loss: MaterialVector,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct SpitToml {
    pub(in crate::registry) item: String,
    #[serde(default)]
    pub(in crate::registry) count: Option<u32>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct FuelToml {
    pub(in crate::registry) item: String,
    pub(in crate::registry) burn: f32,
    #[serde(default)]
    pub(in crate::registry) speed: Option<f32>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct BloomeryToml {
    pub(in crate::registry) charge: String,
    pub(in crate::registry) fuel: String,
    pub(in crate::registry) bloom: String,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct KilnToml {
    pub(in crate::registry) powder: String,
    pub(in crate::registry) glass: String,
    #[serde(default)]
    pub(in crate::registry) consumes: bool,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct KilnBaseToml {
    pub(in crate::registry) sand: String,
    pub(in crate::registry) fuel: String,
    pub(in crate::registry) clear: String,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct WorkedToml {
    pub(in crate::registry) input: String,
    pub(in crate::registry) output: String,
    #[serde(default)]
    pub(in crate::registry) strikes: Option<u32>,
    /// Which station block works it: "anvil" (default) or "quern".
    #[serde(default)]
    pub(in crate::registry) station: Option<String>,
    /// "hammer" (default) or "none" (bare hands, e.g. the quern).
    #[serde(default)]
    pub(in crate::registry) tool: Option<String>,
    #[serde(default)]
    pub(in crate::registry) count: Option<u32>,
    #[serde(default)]
    pub(in crate::registry) loss: MaterialVector,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct AliasToml {
    pub(in crate::registry) old: String,
    pub(in crate::registry) new: String,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct TagToml {
    pub(in crate::registry) id: String,
    pub(in crate::registry) items: Vec<String>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct FeatureToml {
    pub(in crate::registry) r#type: String,
    pub(in crate::registry) block: String,
    #[serde(default)]
    pub(in crate::registry) replaces: Option<String>,
    #[serde(default)]
    pub(in crate::registry) vein_size: Option<u32>,
    #[serde(default)]
    pub(in crate::registry) per_chunk: Option<u32>,
    #[serde(default)]
    pub(in crate::registry) y_range: Option<[i32; 2]>,
    #[serde(default)]
    pub(in crate::registry) shape: Option<String>,
    #[serde(default)]
    pub(in crate::registry) chance: Option<f32>,
    // -- spec 2.5 gate features (`type = "gate"`) --
    #[serde(default)]
    pub(in crate::registry) id: Option<String>,
    #[serde(default)]
    pub(in crate::registry) flag: Option<String>,
    #[serde(default)]
    pub(in crate::registry) value: Option<String>,
    #[serde(default)]
    pub(in crate::registry) unlocked_block: Option<String>,
    #[serde(default)]
    pub(in crate::registry) message: Option<String>,
    #[serde(default)]
    pub(in crate::registry) unbreakable_when_locked: Option<bool>,
}
