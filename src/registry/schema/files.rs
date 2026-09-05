//! Top-level block, item, mode, and recipe document envelopes.

use super::{AliasToml, BlockToml, BloomeryToml, FeatureToml, FuelToml, ItemToml, KilnBaseToml, KilnToml, ModeToml, RecipeToml, SmeltToml, TagToml, WorkedToml};
use serde::Deserialize;

#[derive(Deserialize, Default)]
pub(in crate::registry) struct BlocksFile {
    #[serde(default)]
    pub(in crate::registry) block: Vec<BlockToml>,
}
#[derive(Deserialize, Default)]
pub(in crate::registry) struct ItemsFile {
    #[serde(default)]
    pub(in crate::registry) item: Vec<ItemToml>,
}
#[derive(Deserialize, Default)]
pub(in crate::registry) struct ModesFile {
    #[serde(default)]
    pub(in crate::registry) mode: Vec<ModeToml>,
}
#[derive(Deserialize, Default)]
pub(in crate::registry) struct RecipesFile {
    #[serde(default)]
    pub(in crate::registry) recipe: Vec<RecipeToml>,
    #[serde(default)]
    pub(in crate::registry) smelt: Vec<SmeltToml>,
    #[serde(default)]
    pub(in crate::registry) fuel: Vec<FuelToml>,
    #[serde(default)]
    pub(in crate::registry) bloomery: Vec<BloomeryToml>,
    #[serde(default)]
    pub(in crate::registry) worked: Vec<WorkedToml>,
    #[serde(default)]
    pub(in crate::registry) kiln: Vec<KilnToml>,
    #[serde(default)]
    pub(in crate::registry) kiln_base: Option<KilnBaseToml>,
}
#[derive(Deserialize, Default)]
pub(in crate::registry) struct AliasesFile {
    #[serde(default)]
    pub(in crate::registry) alias: Vec<AliasToml>,
}
#[derive(Deserialize, Default)]
pub(in crate::registry) struct FeaturesFile {
    #[serde(default)]
    pub(in crate::registry) feature: Vec<FeatureToml>,
}
#[derive(Deserialize, Default)]
pub(in crate::registry) struct TagsFile {
    #[serde(default)]
    pub(in crate::registry) tag: Vec<TagToml>,
}
