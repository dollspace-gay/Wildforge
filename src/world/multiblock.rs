//! Generic, data-driven multiblock shape matching.
//!
//! This module is deliberately free of any specific machine's logic: it
//! just knows how to test a set of cells, each carrying a single block
//! constraint, against the world around an anchor position, trying the
//! shape's allowed rotations. Machine definitions live in [`super::machines`]
//! as data (`MultiblockShape` values); this file is the recognizer.
//!
//! Groundwork for the extension plans (spec Part 1.2): a single engine
//! primitive that later slots-based modular frames, stat aggregation, and
//! block-built trains can all build on.

use std::collections::HashMap;

use crate::planet::BlockPos;
use crate::registry::{AIR, BlockId, Registry};
use crate::world::World;

/// A whole-rotation of a shape around the vertical axis, applied to each
/// cell offset before the world is probed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotation {
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    /// The four cardinal rotations the stack/stall machines try today.
    pub const CARDINAL: [Rotation; 4] =
        [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270];

    /// Rotate a `(dx, dy, dz)` offset clockwise around the Y axis.
    pub fn apply(self, (dx, dy, dz): (i32, i32, i32)) -> (i32, i32, i32) {
        match self {
            Rotation::R0 => (dx, dy, dz),
            // +X -> -Z -> -X -> +Z
            Rotation::R90 => (dz, dy, -dx),
            Rotation::R180 => (-dx, dy, -dz),
            Rotation::R270 => (-dz, dy, dx),
        }
    }
}

/// What a single cell of a shape may be.
#[derive(Clone, Debug)]
pub enum BlockConstraint {
    /// The block must be one of these ids (e.g. a machine's mouth blocks).
    OneOf(Vec<BlockId>),
    /// The block must be a single named id (e.g. firebrick).
    #[allow(dead_code)]
    Exact(BlockId),
    /// The block's item must belong to the named tag (e.g. `"base:logs"`).
    Tag(&'static str),
    /// The cell must be air.
    Air,
    /// Solid, or a glazing (glass) block — the stall's awning.
    SolidOrGlass,
}

/// One cell of a shape: an offset from the anchor plus what must sit there.
#[derive(Clone, Debug)]
pub struct ShapeCell {
    pub offset: (i32, i32, i32),
    pub constraint: BlockConstraint,
}

/// A fully-rotatable multiblock shape described purely as data.
#[derive(Clone, Debug)]
pub struct MultiblockShape {
    /// Cells to verify, relative to the anchor. The anchor itself is never a
    /// cell in this list; a machine that recognizes its controller as part of
    /// the shape lists an explicit cell at `(0, 0, 0)`.
    pub cells: Vec<ShapeCell>,
    /// The cell (relative to the anchor, pre-rotation) reported back as the
    /// matched "core" — e.g. a stack's fire column. Shapes with no special
    /// core just point at the anchor `(0, 0, 0)`.
    pub core: (i32, i32, i32),
    /// Which rotations may be tried, in order.
    pub rotations: &'static [Rotation],
}

/// The outcome of a successful shape match.
#[derive(Debug)]
pub struct MatchResult {
    /// The matched core, in world coordinates.
    pub core: BlockPos,
    /// Every matched cell position mapped to the block actually there.
    /// Read by Pattern A stat folding (spec Part 2.1) on revalidation.
    pub matched: HashMap<BlockPos, BlockId>,
}

/// Return the first rotation of `shape` that the world satisfies at `anchor`,
/// or `None` if no rotation matches.
pub fn match_shape(
    world: &World,
    anchor: BlockPos,
    shape: &MultiblockShape,
) -> Option<MatchResult> {
    for &rotation in shape.rotations {
        if let Some(result) = match_rotation(world, anchor, shape, rotation) {
            return Some(result);
        }
    }
    None
}

/// Test every cell of `shape` at `anchor`, rotated by `rotation`. Returns
/// `None` if any cell is out of the world or fails its constraint.
fn match_rotation(
    world: &World,
    anchor: BlockPos,
    shape: &MultiblockShape,
    rotation: Rotation,
) -> Option<MatchResult> {
    let mut matched = HashMap::with_capacity(shape.cells.len());
    for cell in &shape.cells {
        let (dx, dy, dz) = rotation.apply(cell.offset);
        let at = anchor.offset(dx, dy, dz)?;
        let block = world.get_block_at(at);
        if !constraint_ok(world, &cell.constraint, block) {
            return None;
        }
        matched.insert(at, block);
    }
    let (cdx, cdy, cdz) = rotation.apply(shape.core);
    let core = anchor.offset(cdx, cdy, cdz)?;
    Some(MatchResult { core, matched })
}

fn constraint_ok(world: &World, constraint: &BlockConstraint, block: BlockId) -> bool {
    match constraint {
        BlockConstraint::OneOf(ids) => ids.contains(&block),
        BlockConstraint::Exact(id) => *id == block,
        BlockConstraint::Tag(tag) => block_in_tag(&world.reg, tag, block),
        BlockConstraint::Air => block == AIR,
        BlockConstraint::SolidOrGlass => world.reg.is_solid(block) || world.reg.block(block).glass,
    }
}

/// True if `block`'s item belongs to the named registry tag.
fn block_in_tag(reg: &Registry, tag: &str, block: BlockId) -> bool {
    reg.tags.get(tag).is_some_and(|tagged| {
        reg.item_id(&reg.block(block).name)
            .is_some_and(|item| tagged.contains(&item))
    })
}

/// Which registered machine a generic multiblock instance is. Doubles as
/// the instance's shape ID: each kind maps one-to-one to its shell shape
/// (built in [`crate::world::machines`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
pub enum MachineKind {
    #[default]
    Bloomery,
    Forge,
    Kiln,
    Separator,
}

impl MachineKind {
    /// The save/UI name for this kind.
    pub fn name(self) -> &'static str {
        match self {
            MachineKind::Bloomery => "bloomery",
            MachineKind::Forge => "forge",
            MachineKind::Kiln => "kiln",
            MachineKind::Separator => "separator",
        }
    }

    pub fn from_name(name: &str) -> Option<MachineKind> {
        match name {
            "bloomery" => Some(MachineKind::Bloomery),
            "forge" => Some(MachineKind::Forge),
            "kiln" => Some(MachineKind::Kiln),
            "separator" => Some(MachineKind::Separator),
            _ => None,
        }
    }
}

/// Folded Pattern A stats of a matched multiblock shell. Recomputed on
/// revalidation, never per tick.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct EffectiveStats {
    /// Sum of the matched shell cells' heat retention.
    pub heat: u32,
    /// Number of matched cells carrying a nonzero heat contribution.
    pub heat_cells: u32,
    /// Whether the shell carries a chimney over its core (a kiln becomes
    /// a glassworks). Set by revalidation, not per tick.
    pub chimney: bool,
}

impl EffectiveStats {
    /// Fires at 1.0x on an all-base shell; a hotter ring (an advanced
    /// firebrick tier in a ring position) fires proportionally faster.
    pub fn heat_multiplier(self) -> f32 {
        if self.heat_cells == 0 {
            1.0
        } else {
            self.heat as f32 / self.heat_cells as f32
        }
    }
}

/// Combine every matched cell's block-property contribution into the
/// instance's effective stats (Pattern A, spec Part 2.1). The combination
/// rule is fixed per stat (heat sums) and lives here, not per machine.
pub fn fold_stats(world: &World, matched: &HashMap<BlockPos, BlockId>) -> EffectiveStats {
    let mut stats = EffectiveStats::default();
    for &block in matched.values() {
        let heat = world.reg.block(block).heat_retention;
        if heat > 0 {
            stats.heat += heat;
            stats.heat_cells += 1;
        }
    }
    stats
}

/// The axis-aligned cell-offset ranges a shape's cells occupy over every
/// allowed rotation, relative to the anchor. A machine's edit region
/// derives from this, so an edit hook can decide in O(1) whether a block
/// change could possibly have touched a given instance's shell.
pub fn shape_extent(shape: &MultiblockShape) -> ((i32, i32, i32), (i32, i32, i32)) {
    let mut min = (i32::MAX, i32::MAX, i32::MAX);
    let mut max = (i32::MIN, i32::MIN, i32::MIN);
    for &rotation in shape.rotations {
        for cell in &shape.cells {
            let o = rotation.apply(cell.offset);
            min.0 = min.0.min(o.0);
            min.1 = min.1.min(o.1);
            min.2 = min.2.min(o.2);
            max.0 = max.0.max(o.0);
            max.1 = max.1.max(o.1);
            max.2 = max.2.max(o.2);
        }
        let c = rotation.apply(shape.core);
        min.0 = min.0.min(c.0);
        min.1 = min.1.min(c.1);
        min.2 = min.2.min(c.2);
        max.0 = max.0.max(c.0);
        max.1 = max.1.max(c.1);
        max.2 = max.2.max(c.2);
    }
    (min, max)
}

/// True if `pos` (same face as `anchor`) lies within `(min, max)` cell
/// offsets of `anchor`. Pure arithmetic: the edit hook pays nothing per
/// instance it does not actually revalidate.
pub fn pos_within_extent(
    pos: BlockPos,
    anchor: BlockPos,
    (min, max): ((i32, i32, i32), (i32, i32, i32)),
) -> bool {
    if pos.face() != anchor.face() {
        return false;
    }
    let dx = i32::from(pos.u()) - i32::from(anchor.u());
    let dy = i32::from(pos.y()) - i32::from(anchor.y());
    let dz = i32::from(pos.v()) - i32::from(anchor.v());
    dx >= min.0 && dx <= max.0 && dy >= min.1 && dy <= max.1 && dz >= min.2 && dz <= max.2
}
