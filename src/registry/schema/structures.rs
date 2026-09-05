//! Raw structures content schema; no runtime mutation.

use serde::Deserialize;
use std::collections::{HashMap};

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct BrushToml {
    pub(in crate::registry) table: String,
    pub(in crate::registry) becomes: String,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct LootEntryToml {
    pub(in crate::registry) item: String,
    #[serde(default = "one_u32")]
    pub(in crate::registry) weight: u32,
    #[serde(default)]
    pub(in crate::registry) count: Option<[u32; 2]>,
    #[serde(default)]
    pub(in crate::registry) durability: Option<f32>,
}

pub(in crate::registry) fn one_u32() -> u32 {
    1
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct LootToml {
    pub(in crate::registry) id: String,
    pub(in crate::registry) entries: Vec<LootEntryToml>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct StructureToml {
    pub(in crate::registry) id: String,
    pub(in crate::registry) biomes: Vec<String>,
    pub(in crate::registry) rarity: u32,
    #[serde(default)]
    pub(in crate::registry) placement: Option<String>,
    #[serde(default)]
    pub(in crate::registry) depth: Option<[i32; 2]>,
    pub(in crate::registry) palette: HashMap<String, String>,
    pub(in crate::registry) layers: Vec<Vec<String>>,
    #[serde(default)]
    pub(in crate::registry) loot: Option<String>,
}

#[derive(Deserialize, Default)]
pub(in crate::registry) struct StructuresFile {
    #[serde(default)]
    pub(in crate::registry) structure: Vec<StructureToml>,
    #[serde(default)]
    pub(in crate::registry) loot: Vec<LootToml>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct PieceToml {
    pub(in crate::registry) id: String,
    #[serde(default)]
    pub(in crate::registry) cells: Vec<PieceCellToml>,
    #[serde(default)]
    pub(in crate::registry) connectors: Vec<ConnectorToml>,
    #[serde(default)]
    pub(in crate::registry) markers: Vec<MarkerToml>,
    #[serde(default)]
    pub(in crate::registry) chests: Vec<ChestToml>,
    #[serde(default)]
    pub(in crate::registry) settlement_tier: u32,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct PieceCellToml {
    #[serde(default)]
    pub(in crate::registry) du: i32,
    #[serde(default)]
    pub(in crate::registry) dy: i32,
    #[serde(default)]
    pub(in crate::registry) dv: i32,
    pub(in crate::registry) block: String,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct ConnectorToml {
    #[serde(default)]
    pub(in crate::registry) du: i32,
    #[serde(default)]
    pub(in crate::registry) dy: i32,
    #[serde(default)]
    pub(in crate::registry) dv: i32,
    pub(in crate::registry) kind: String,
    pub(in crate::registry) facing: String,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct MarkerToml {
    #[serde(default)]
    pub(in crate::registry) du: i32,
    #[serde(default)]
    pub(in crate::registry) dy: i32,
    #[serde(default)]
    pub(in crate::registry) dv: i32,
    pub(in crate::registry) kind: String,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct ChestToml {
    #[serde(default)]
    pub(in crate::registry) du: i32,
    #[serde(default)]
    pub(in crate::registry) dy: i32,
    #[serde(default)]
    pub(in crate::registry) dv: i32,
    #[serde(default)]
    pub(in crate::registry) loot: String,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct PoolToml {
    pub(in crate::registry) id: String,
    pub(in crate::registry) entries: Vec<PoolEntryToml>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct PoolEntryToml {
    pub(in crate::registry) piece: String,
    #[serde(default)]
    pub(in crate::registry) weight: u32,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct AssemblyToml {
    pub(in crate::registry) id: String,
    pub(in crate::registry) biomes: Vec<String>,
    pub(in crate::registry) rarity: u32,
    pub(in crate::registry) entry: String,
    #[serde(default)]
    pub(in crate::registry) pools: HashMap<String, String>,
    #[serde(default)]
    pub(in crate::registry) max_depth: u32,
    #[serde(default)]
    pub(in crate::registry) max_pieces: u32,
    #[serde(default)]
    pub(in crate::registry) terrain: Option<String>,
    #[serde(default)]
    pub(in crate::registry) settlement: Option<String>,
    /// Dungeon run rules (capability E10): presence makes this assembly a
    /// dungeon that runs on demand in the Deep.
    #[serde(default)]
    pub(in crate::registry) dungeon: Option<DungeonToml>,
}

#[derive(Deserialize, Clone, Default)]
pub(in crate::registry) struct DungeonToml {
    #[serde(default)]
    pub(in crate::registry) reset: Option<f32>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct SettlementToml {
    pub(in crate::registry) id: String,
    #[serde(default)]
    pub(in crate::registry) rep_key: Option<String>,
    #[serde(default)]
    pub(in crate::registry) tiers: Vec<SettlementTierToml>,
    #[serde(default)]
    pub(in crate::registry) need: Vec<SettlementNeedToml>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct SettlementNeedToml {
    pub(in crate::registry) item: String,
    #[serde(default)]
    pub(in crate::registry) rep: Option<u32>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct SettlementTierToml {
    pub(in crate::registry) tier: u32,
    pub(in crate::registry) threshold: u32,
}

#[derive(Deserialize, Default)]
pub(in crate::registry) struct PiecesFile {
    #[serde(default)]
    pub(in crate::registry) piece: Vec<PieceToml>,
    #[serde(default)]
    pub(in crate::registry) pool: Vec<PoolToml>,
    #[serde(default)]
    pub(in crate::registry) assembly: Vec<AssemblyToml>,
    #[serde(default)]
    pub(in crate::registry) settlement: Vec<SettlementToml>,
}

