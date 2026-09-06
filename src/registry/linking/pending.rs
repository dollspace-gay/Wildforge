//! Unresolved references cross registration and linking through named records.

use crate::registry::BlockId;
use crate::registry::schema::{
    AliasToml, AnimalToml, AssemblyToml, BloomeryToml, BonusDropToml, BrushToml, DialogueToml,
    FeatureToml, FuelToml, HarvestToml, KilnBaseToml, KilnToml, LootToml, NpcToml, PieceToml,
    PoolToml, QuestToml, RecipeToml, SettlementToml, SmeltToml, StructureToml, TagToml, WorkedToml,
};
use std::collections::HashMap;

pub(in crate::registry) struct PendingDrop {
    pub(in crate::registry) modid: String,
    pub(in crate::registry) block: usize,
    pub(in crate::registry) rule: String,
    pub(in crate::registry) count: u32,
}

pub(in crate::registry) struct PendingAnimal {
    pub(in crate::registry) modid: String,
    pub(in crate::registry) definition: AnimalToml,
    pub(in crate::registry) tile: u16,
    pub(in crate::registry) head_tile: u16,
    pub(in crate::registry) box_tiles: HashMap<String, u16>,
    pub(in crate::registry) proj_tile: Option<u16>,
    pub(in crate::registry) attack_proj_tiles: Vec<Option<u16>>,
}

pub(in crate::registry) struct PendingNpc {
    pub(in crate::registry) modid: String,
    pub(in crate::registry) definition: NpcToml,
    pub(in crate::registry) tile: u16,
    pub(in crate::registry) head: u16,
    pub(in crate::registry) box_tiles: HashMap<String, u16>,
}

#[derive(Default)]
pub(in crate::registry) struct PendingContent {
    pub(in crate::registry) drops: Vec<PendingDrop>,
    pub(in crate::registry) recipes: Vec<(String, RecipeToml)>,
    pub(in crate::registry) features: Vec<(String, FeatureToml)>,
    pub(in crate::registry) tags: Vec<(String, TagToml)>,
    pub(in crate::registry) smelts: Vec<(String, SmeltToml)>,
    pub(in crate::registry) bloomeries: Vec<(String, BloomeryToml)>,
    pub(in crate::registry) workeds: Vec<(String, WorkedToml)>,
    pub(in crate::registry) kilns: Vec<(String, KilnToml)>,
    pub(in crate::registry) kiln_bases: Vec<(String, KilnBaseToml)>,
    pub(in crate::registry) fuels: Vec<(String, FuelToml)>,
    pub(in crate::registry) aliases: Vec<(String, AliasToml)>,
    pub(in crate::registry) harvests: Vec<(String, BlockId, HarvestToml)>,
    pub(in crate::registry) bonus: Vec<(String, usize, BonusDropToml)>,
    pub(in crate::registry) brush: Vec<(String, usize, BrushToml)>,
    pub(in crate::registry) structs: Vec<(String, StructureToml)>,
    pub(in crate::registry) loots: Vec<(String, LootToml)>,
    pub(in crate::registry) pieces: Vec<(String, PieceToml)>,
    pub(in crate::registry) pools: Vec<(String, PoolToml)>,
    pub(in crate::registry) assemblies: Vec<(String, AssemblyToml)>,
    pub(in crate::registry) places: Vec<(String, (String, String))>,
    pub(in crate::registry) animals: Vec<PendingAnimal>,
    pub(in crate::registry) npcs: Vec<PendingNpc>,
    pub(in crate::registry) dialogues: Vec<(String, DialogueToml)>,
    pub(in crate::registry) quests: Vec<(String, QuestToml)>,
    pub(in crate::registry) settlements: Vec<(String, SettlementToml)>,
}
