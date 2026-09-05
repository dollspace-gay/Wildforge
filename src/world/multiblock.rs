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
use std::hash::Hash;

use crate::registry::{AIR, BlockId, Registry};


/// A whole-rotation of a shape around the vertical axis, applied to each
/// cell offset before the world is probed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

    /// The inverse rotation (undoes one quarter-turn around Y).
    pub fn inverse(self) -> Rotation {
        use Rotation::{R0, R90, R180, R270};
        match self {
            R0 => R0,
            R90 => R270,
            R180 => R180,
            R270 => R90,
        }
    }

    /// Rotate a f32 `(dx, dy, dz)` offset clockwise around the Y axis,
    /// matching the pattern of the integer `apply`. Linear, so the same
    /// permutation works directly on floats.
    pub fn apply_vec(self, (dx, dy, dz): (f32, f32, f32)) -> (f32, f32, f32) {
        match self {
            Rotation::R0 => (dx, dy, dz),
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
    /// The cell is a swappable module slot: the block must belong to the
    /// named category's module catalog (spec Part 1.3). The match also
    /// records *which* cell matched and its category, so revalidation can
    /// fold each installed module's qualitative capabilities.
    Module(&'static str),
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

/// The outcome of a successful shape match, in store-local coordinates.
#[derive(Debug)]
pub struct MatchResult<P> {
    /// The matched core, in store coordinates.
    pub core: P,
    /// Every matched cell position mapped to the block actually there.
    /// Read by Pattern A stat folding (spec Part 2.1) on revalidation.
    pub matched: HashMap<P, BlockId>,
    /// Every module-slot cell mapped to its category id. Read when the
    /// frame's qualitative capabilities are folded from the installed
    /// modules (spec Part 1.3).
    pub slots: HashMap<P, &'static str>,
}

pub use super::block_store::{BlockRead, BlockStore};

/// Return the first rotation of `shape` that the store satisfies at `anchor`,
/// or `None` if no rotation matches.
pub fn match_shape<B: BlockRead>(
    store: &B,
    anchor: B::Pos,
    shape: &MultiblockShape,
) -> Option<MatchResult<B::Pos>> {
    for &rotation in shape.rotations {
        if let Some(result) = match_rotation(store, anchor, shape, rotation) {
            return Some(result);
        }
    }
    None
}

/// Test every cell of `shape` at `anchor`, rotated by `rotation`. Returns
/// `None` if any cell is out of the store or fails its constraint.
fn match_rotation<B: BlockRead>(
    store: &B,
    anchor: B::Pos,
    shape: &MultiblockShape,
    rotation: Rotation,
) -> Option<MatchResult<B::Pos>> {
    let mut matched = HashMap::with_capacity(shape.cells.len());
    let mut slots = HashMap::new();
    for cell in &shape.cells {
        let (dx, dy, dz) = rotation.apply(cell.offset);
        let at = store.offset(anchor, (dx, dy, dz))?;
        let block = store.get_block(at);
        if !constraint_ok(store, &cell.constraint, block) {
            return None;
        }
        matched.insert(at, block);
        if let BlockConstraint::Module(category) = cell.constraint {
            slots.insert(at, category);
        }
    }
    let (cdx, cdy, cdz) = rotation.apply(shape.core);
    let core = store.offset(anchor, (cdx, cdy, cdz))?;
    Some(MatchResult {
        core,
        matched,
        slots,
    })
}

fn constraint_ok<B: BlockRead>(store: &B, constraint: &BlockConstraint, block: BlockId) -> bool {
    match constraint {
        BlockConstraint::OneOf(ids) => ids.contains(&block),
        BlockConstraint::Exact(id) => *id == block,
        BlockConstraint::Tag(tag) => block_in_tag(store.reg(), tag, block),
        BlockConstraint::Air => block == AIR,
        BlockConstraint::SolidOrGlass => {
            store.reg().is_solid(block) || store.reg().block(block).glass
        }
        BlockConstraint::Module(category) => {
            modules_in_category(store.reg(), category).contains(&block)
        }
    }
}

/// True if `block`'s item belongs to the named registry tag.
fn block_in_tag(reg: &Registry, tag: &str, block: BlockId) -> bool {
    reg.tags.get(tag).is_some_and(|tagged| {
        reg.item_id(&reg.block(block).name)
            .is_some_and(|item| tagged.contains(&item))
    })
}

/// Which registered machine a generic multiblock instance is. Capability
/// E7: this is the index of a [`crate::machines::MachineDef`] in
/// `Registry::machines`, so the kinds are data (`machines.toml`), not an
/// enum. Index 0 (the first base machine) is the default; the engine
/// dispatches on the def's closed native handler instead of matching
/// variants.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
pub struct MachineKind(pub u16);

impl MachineKind {
    /// The registry index this kind addresses.
    pub fn index(self) -> usize {
        self.0 as usize
    }

    /// The registered machine id for this kind (the save/UI name), or the
    /// empty string when the pack does not declare the kind.
    pub fn name(self, reg: &Registry) -> String {
        reg.machine(self).map(|m| m.id.clone()).unwrap_or_default()
    }

    /// The closed native handler this kind runs, if the pack declares it.
    pub fn handler(self, reg: &Registry) -> Option<crate::machines::MachineHandler> {
        reg.machine(self).map(|m| m.handler)
    }

    /// Look a kind up by its registered machine id.
    pub fn from_name(reg: &Registry, name: &str) -> Option<MachineKind> {
        reg.machine_kind(name)
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
pub fn fold_stats<B: BlockRead>(store: &B, matched: &HashMap<B::Pos, BlockId>) -> EffectiveStats {
    let mut stats = EffectiveStats::default();
    for &block in matched.values() {
        let heat = store.reg().block(block).heat_retention;
        if heat > 0 {
            stats.heat += heat;
            stats.heat_cells += 1;
        }
    }
    stats
}

/// Qualitative capabilities a slot module can grant its frame (spec
/// Part 1.3). Distinct from Pattern A numeric stats on purpose: these
/// change what a structure *can do*, not how well it does it, so they
/// are folded into a separate set rather than into [`EffectiveStats`].
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Capabilities {
    /// A vitrified tile face: keeps glaze from crawling off the wares.
    pub ceramic: bool,
    /// A soapstone face: holds an even, gentle heat for slow work.
    pub refractory: bool,
}

impl Capabilities {
    /// Union another module's capabilities into this set — a frame is
    /// capable of anything any installed module enables.
    pub fn merge(&mut self, other: Capabilities) {
        self.ceramic |= other.ceramic;
        self.refractory |= other.refractory;
    }
}

/// One row of the module catalog: a block and the qualitative
/// capabilities it grants when installed in a slot of its category.
pub struct ModuleDef {
    /// The block, by registered id (resolved like `Tag`/`OneOf` at match
    /// time).
    pub block: &'static str,
    /// What this module lets the frame do. Numeric contributions are not
    /// fields here — the block itself already carries `heat_retention`,
    /// which `fold_stats` reads.
    pub capabilities: Capabilities,
}

/// The module catalog, keyed by category id (spec Part 1.3). Static data
/// mirroring how `MachineKind::mouth()` lists ids; a category shared by
/// every stack machine because they all use the same `stack_shape`.
pub fn module_catalog(category: &'static str) -> &'static [ModuleDef] {
    match category {
        "casing" => &CASING_CATALOG,
        _ => &[],
    }
}

/// The `"casing"` category: the shared stack shell's swappable tier
/// blocks. The baseline tier (plain firebrick) is what every player
/// build drops in by default; the advanced tier is numeric-only; the
/// porcelain and soapstone tiles grant distinct qualitative capabilities.
static CASING_CATALOG: [ModuleDef; 4] = [
    ModuleDef {
        block: "base:firebrick",
        capabilities: Capabilities {
            ceramic: false,
            refractory: false,
        },
    },
    ModuleDef {
        block: "base:firebrick_advanced",
        capabilities: Capabilities {
            ceramic: false,
            refractory: false,
        },
    },
    ModuleDef {
        block: "base:casing_porcelain",
        capabilities: Capabilities {
            ceramic: true,
            refractory: false,
        },
    },
    ModuleDef {
        block: "base:casing_soapstone",
        capabilities: Capabilities {
            ceramic: false,
            refractory: true,
        },
    },
];

/// Every block id qualifying as a module for `category` (the resolution
/// `BlockConstraint::Module` uses).
pub fn modules_in_category(reg: &Registry, category: &'static str) -> Vec<BlockId> {
    module_catalog(category)
        .iter()
        .filter_map(|m| reg.block_id(m.block))
        .collect()
}

/// The capabilities `block` grants when installed in `category`.
pub fn module_capabilities(reg: &Registry, category: &'static str, block: BlockId) -> Capabilities {
    module_catalog(category)
        .iter()
        .find(|m| reg.block_id(m.block) == Some(block))
        .map_or_else(Capabilities::default, |m| m.capabilities)
}

/// Fold the qualitative capabilities granted by every installed slot
/// module. Unlike stats (numeric sums), capabilities union across the
/// frame; a module's *identity* at its slot decides what it grants.
pub fn fold_capabilities<B: BlockRead>(
    store: &B,
    matched: &HashMap<B::Pos, BlockId>,
    slots: &HashMap<B::Pos, &'static str>,
) -> Capabilities {
    let mut caps = Capabilities::default();
    for (pos, category) in slots {
        if let Some(&module) = matched.get(pos) {
            caps.merge(module_capabilities(store.reg(), category, module));
        }
    }
    caps
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

/// True if `pos` lies within `(min, max)` cell offsets of `anchor`, in the
/// same store-local coordinate space. Pure arithmetic: the edit hook pays
/// nothing per instance it does not actually revalidate.
pub fn pos_within_extent<B: BlockRead>(
    store: &B,
    pos: B::Pos,
    anchor: B::Pos,
    (min, max): ((i32, i32, i32), (i32, i32, i32)),
) -> bool {
    let Some((dx, dy, dz)) = store.cell_delta(anchor, pos) else {
        return false;
    };
    dx >= min.0 && dx <= max.0 && dy >= min.1 && dy <= max.1 && dz >= min.2 && dz <= max.2
}
