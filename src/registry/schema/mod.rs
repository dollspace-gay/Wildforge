//! Parsed documents and their provider bundle, private to registry linking.

mod files;
pub(in crate::registry) use files::{
    AliasesFile, BlocksFile, FeaturesFile, ItemsFile, ModesFile, RecipesFile, TagsFile,
};
mod manifest;
pub(in crate::registry) use manifest::{ModToml, ModeToml};
mod blocks;
pub(in crate::registry) use blocks::{BlockToml, BonusDropToml, HarvestToml, TexSpec, one_u8};
mod items;
pub(in crate::registry) use items::{CharmToml, ItemToml, SalvageToml};
mod magic;
pub(in crate::registry) use magic::{
    ArcaneContentToml, ArcaneEcologyToml, ArcaneFile, ArcaneSiteToml, DiscoveryFixtureToml,
    DiscoveryItemToml, ObservationToml, ResonanceToml,
};
mod fauna;
pub(in crate::registry) use fauna::{AnimalToml, BoxToml, ResistTomlList};
mod recipes;
pub(in crate::registry) use recipes::{
    AliasToml, BloomeryToml, FeatureToml, FuelToml, KilnBaseToml, KilnToml, RecipeToml, SmeltToml,
    TagToml, WorkedToml,
};
mod structures;
pub(in crate::registry) use structures::{
    AssemblyToml, BrushToml, LootToml, PieceToml, PiecesFile, PoolToml, SettlementToml,
    StructureToml, StructuresFile, one_u32,
};
mod narrative;
pub(in crate::registry) use narrative::{
    AnimalsFile, DialogueFile, DialogueToml, NpcToml, NpcsFile, QuestToml, QuestsFile,
};

// Raw content bundle, private to registry loading/linking.

use crate::registry::{ModInfo, RawNestToml};
use serde::Deserialize;

pub(in crate::registry) struct RawMod {
    pub(in crate::registry) info: ModInfo,
    pub(in crate::registry) depends: Vec<String>,
    pub(in crate::registry) blocks: Vec<BlockToml>,
    pub(in crate::registry) items: Vec<ItemToml>,
    pub(in crate::registry) recipes: Vec<RecipeToml>,
    pub(in crate::registry) smelts: Vec<SmeltToml>,
    pub(in crate::registry) fuels: Vec<FuelToml>,
    pub(in crate::registry) bloomeries: Vec<BloomeryToml>,
    pub(in crate::registry) workeds: Vec<WorkedToml>,
    pub(in crate::registry) kilns: Vec<KilnToml>,
    pub(in crate::registry) kiln_bases: Vec<KilnBaseToml>,
    pub(in crate::registry) features: Vec<FeatureToml>,
    pub(in crate::registry) tags: Vec<TagToml>,
    pub(in crate::registry) aliases: Vec<AliasToml>,
    pub(in crate::registry) animals: Vec<AnimalToml>,
    pub(in crate::registry) npcs: Vec<NpcToml>,
    pub(in crate::registry) dialogues: Vec<DialogueToml>,
    pub(in crate::registry) quests: Vec<QuestToml>,
    pub(in crate::registry) structures: Vec<StructureToml>,
    pub(in crate::registry) loots: Vec<LootToml>,
    pub(in crate::registry) pieces: Vec<PieceToml>,
    pub(in crate::registry) pools: Vec<PoolToml>,
    pub(in crate::registry) assemblies: Vec<AssemblyToml>,
    pub(in crate::registry) settlements: Vec<SettlementToml>,
    pub(in crate::registry) resonances: Vec<ResonanceToml>,
    pub(in crate::registry) arcane_sites: Vec<ArcaneSiteToml>,
    pub(in crate::registry) workings: Vec<crate::workings::RawWorkingDef>,
    pub(in crate::registry) preparations: Vec<crate::alchemy::RawPreparationDef>,
    pub(in crate::registry) modes: Vec<ModeToml>,
    pub(in crate::registry) skills: Option<crate::skills::RawSkillToml>,
    pub(in crate::registry) machines: Option<crate::machines::RawMachineToml>,
    pub(in crate::registry) nests: Option<NestFileToml>,
    pub(in crate::registry) screens: Option<crate::screens::RawScreensToml>,
}

/// The `nests.toml` content (capability E9): a list of `[[nest]]` spawn-gate
/// rows. Mods own the content; base ships none.
#[derive(Deserialize, Clone, Default)]
pub struct NestFileToml {
    #[serde(default)]
    pub nest: Vec<RawNestToml>,
}
