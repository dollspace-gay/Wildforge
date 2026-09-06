//! Ore, structure, piece, assembly, and dungeon content definitions.

use super::{BlockId, ItemId, RetrogenPolicy};
use std::collections::HashMap;

/// How a mineral deposit grows from its seed cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VeinShape {
    /// The classic drunk walk — roughly round pockets.
    Walk,
    /// A flat lens: long in x/z, grudging in y (coal seams).
    Seam,
    /// A near-vertical streak (quartz veins and their gold).
    Streak,
}

#[derive(Clone, Debug)]
pub struct OreFeature {
    pub block: BlockId,
    pub replaces: BlockId,
    pub vein_size: u32,
    pub per_chunk: u32,
    pub y_min: i32,
    pub y_max: i32,
    pub shape: VeinShape,
    /// Per-vein roll probability: 1.0 plants every roll, fractions
    /// thin a host down to traces (the bronze bootstrap lives here).
    pub chance: f32,
    /// Stable content identity, not the runtime block id.
    pub resource_key: String,
    pub mod_id: String,
    pub retrogen: RetrogenPolicy,
}

/// One weighted entry in a loot table.
#[derive(Clone, Debug)]
pub struct LootEntry {
    pub item: ItemId,
    pub weight: u32,
    pub count: (u32, u32),
    /// Spawn worn: fraction of max durability (old tools from ruins).
    pub durability_frac: Option<f32>,
}

/// A worldgen structure template: palette + bottom-up layers.
/// Special chars: '.' = leave terrain, '~' = force air, 'C' = loot chest.
#[derive(Clone, Debug)]
pub struct StructureDef {
    // Stable qualified id retained even though generation currently iterates
    // the resolved templates directly.
    #[cfg_attr(not(test), allow(dead_code))]
    pub name: String,
    pub biomes: Vec<String>,
    /// 1-in-N chunks (per matching biome).
    pub rarity: u32,
    /// None = on the surface; Some(min, max) = buried this deep.
    pub buried: Option<(i32, i32)>,
    pub palette: HashMap<char, BlockId>,
    pub layers: Vec<Vec<String>>,
    pub loot: Option<String>,
}

/// One connector point on a piece: a local cell offset, a typed connector
/// (`kind` pairs only with the same kind), and the cardinal direction the
/// connector points *out of* the piece. Children attach across a matching
/// connector of the same kind, facing opposite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PieceConnector {
    pub du: i32,
    pub dy: i32,
    pub dv: i32,
    pub kind: String,
    pub facing: crate::planet::Direction4,
}

/// A typed marker resolved to a world position when a piece is placed
/// (spec 2.4 spawn markers, 2.5 feature anchors). Carried and resolved
/// here; consumed by later phases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PieceMarker {
    pub du: i32,
    pub dy: i32,
    pub dv: i32,
    pub kind: String,
}

/// A cell that becomes a wild-owned loot chest when the piece is stamped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PieceChest {
    pub du: i32,
    pub dy: i32,
    pub dv: i32,
    /// Qualified loot table id.
    pub loot: String,
}

/// A gen-walk piece: cells reuse the Phase 4 template cell format (offset +
/// block name), so a captured region is already a valid piece body.
#[derive(Clone, Debug)]
pub struct PieceDef {
    pub name: String,
    /// `(du, dy, dv, block-name)` cells — exactly `TemplateCell`, reused.
    pub cells: Vec<crate::world::template::TemplateCell>,
    pub connectors: Vec<PieceConnector>,
    pub markers: Vec<PieceMarker>,
    pub chests: Vec<PieceChest>,
    /// Growth tier of the piece within its settlement (spec 3.4). Tier 1 is
    /// visible immediately; tier >= 2 is placed but hidden until reputation
    /// reveals the tier.
    pub settlement_tier: u32,
}

/// One weighted entry in a per-kind pool.
#[derive(Clone, Debug)]
pub struct PoolEntry {
    pub piece: String,
    pub weight: u32,
}

/// A weighted list of interchangeable pieces, keyed by connector kind.
#[derive(Clone, Debug)]
pub struct PoolDef {
    pub id: String,
    pub entries: Vec<PoolEntry>,
}

/// How a piece assembly meets the voxel terrain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainAdaptation {
    /// Place as authored at the surface anchor.
    None,
    /// Sink the piece so its floor sits at/below the surface line.
    Bury,
    /// Place inside solid terrain; terrain stays as the outer shell.
    Encapsulate,
}

/// A biome/rarity-gated piece assembly: an entry piece plus per-kind pools.
#[derive(Clone, Debug)]
pub struct AssemblyDef {
    pub name: String,
    pub biomes: Vec<String>,
    /// 1-in-N chunks (per matching biome).
    pub rarity: u32,
    pub entry_piece: String,
    /// connector kind -> pool id.
    pub pools: HashMap<String, String>,
    /// Steps from the entry piece before connectors stop being followed.
    pub max_depth: u32,
    /// Total pieces placed before the walk stops.
    pub max_pieces: u32,
    pub terrain: TerrainAdaptation,
    /// If set, this assembly generates the named settlement (spec 3.4): its
    /// tier-tagged pieces place hidden growth tiers at worldgen.
    pub settlement: Option<String>,
    /// If set, this assembly is a DUNGEON (capability E10): it never
    /// generates at worldgen — runs stamp it into a reserved Deep slot on
    /// demand and reset when empty.
    pub dungeon: Option<DungeonDef>,
}

/// A dungeon run's rules (capability E10).
#[derive(Clone, Debug)]
pub struct DungeonDef {
    /// Seconds after the last participant leaves before the zone resets.
    pub reset: f32,
}
