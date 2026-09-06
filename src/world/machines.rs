//! Falling blocks, multiblock machines, clamps, anvils, and archaeology.
use crate::inventory::ItemStack;
use crate::machines::MachineHandler;
use crate::planet::BlockPos;
use crate::planet_atlas::LocalWeatherSample;
use crate::registry::BlockId;
use crate::registry::Registry;
use crate::world::BlockEntity;
use crate::world::World;
use crate::world::multiblock::BlockConstraint;
use crate::world::multiblock::BlockRead;
use crate::world::multiblock::BlockStore;
use crate::world::multiblock::MachineKind;
use crate::world::multiblock::MatchResult;
use crate::world::multiblock::MultiblockShape;
use crate::world::multiblock::Rotation;
use crate::world::multiblock::ShapeCell;
use crate::world::multiblock::fold_capabilities;
use crate::world::multiblock::fold_stats;
use crate::world::multiblock::match_shape;
use crate::world::multiblock::pos_within_extent;
use crate::world::multiblock::shape_extent;
use std::collections::HashMap;
use std::sync::Arc;

/// A powered station's batch limit: what one loading can hold.
pub const STATION_BULK: u32 = 16;

/// Powered stations that are the capital sibling of a hand process
/// read that process's worked table (the millstone IS a quern with a
/// shaft where your arm was).
pub fn worked_table_for(station: &str) -> &str {
    match station {
        "millstone" => "quern",
        s => s,
    }
}

/// Stations whose strikes come from the shaft line, not a player.
pub fn station_powered(station: &str) -> bool {
    matches!(
        station,
        "millstone" | "sawmill" | "lathe" | "iron_lathe" | "boring"
    )
}

impl World {
    // ---------------- wildlife ----------------
}

/// Locate a registered machine slot without exposing any mutation operation.
pub(super) fn slot_of_instance_at<B: BlockRead>(
    store: &B,
    pos: B::Pos,
) -> Option<(B::Pos, &'static str)> {
    for (anchor, entity) in store.block_entities() {
        let BlockEntity::Multiblock(m) = entity else {
            continue;
        };
        let extent = m.kind.edit_region(store, *anchor);
        if !pos_within_extent(store, pos, *anchor, extent) {
            continue;
        }
        if let Some(matched) = m.kind.validate(store, *anchor)
            && let Some(category) = matched.slots.get(&pos)
        {
            return Some((*anchor, *category));
        }
    }
    None
}

/// Shared machine-recognition queries for authority and read-only scenes.
pub(super) fn check_machine_at<B: BlockRead>(store: &B, name: &str, pos: B::Pos) -> Option<B::Pos> {
    let kind = store.reg().machine_kind(name).unwrap_or_default();
    kind.validate(store, pos).map(|result| result.core)
}

pub(super) fn check_glassworks_at<B: BlockRead>(store: &B, pos: B::Pos) -> Option<B::Pos> {
    let core = check_machine_at(store, "base:kiln", pos)?;
    has_chimney_at(store, core).then_some(core)
}

pub(super) fn check_stall_at<B: BlockRead>(store: &B, pos: B::Pos) -> bool {
    match_shape(store, pos, &stall_shape()).is_some()
}

impl MachineKind {
    /// The two mouth blocks a kind routes its craft through: the handed
    /// and lit faces of its mouth station. The lit face only exists for
    /// fire handlers.
    fn mouth(self, reg: &Registry) -> [Option<BlockId>; 2] {
        let Some(def) = reg.machine(self) else {
            return [None, None];
        };
        let a = reg.block_id(&def.mouth);
        let b = def.mouth_lit.as_deref().and_then(|lit| reg.block_id(lit));
        [a, b]
    }

    /// Validate this kind's full shell at `anchor`: the firebrick stack,
    /// the mouth block, and — for the forge handler — the chimney and anvil.
    /// Returns the match result (core + folded cell map) on success.
    pub fn validate<B: BlockRead>(self, store: &B, anchor: B::Pos) -> Option<MatchResult<B::Pos>> {
        let def = store.reg().machine(self)?;
        let shape = stack_shape(&self.mouth(store.reg()));
        let matched = match_shape(store, anchor, &shape)?;
        if def.handler.requires_anvil() {
            if !has_chimney_at(store, matched.core) {
                return None;
            }
            let anvil = store.reg().block_id("base:stone_anvil")?;
            let mut found = false;
            for dx in -3i32..=3 {
                for dz in -3i32..=3 {
                    for dy in -1..=1 {
                        if let Some(at) = store.offset(anchor, (dx, dy, dz))
                            && store.get_block(at) == anvil
                        {
                            found = true;
                        }
                    }
                }
            }
            if !found {
                return None;
            }
        }
        Some(matched)
    }

    /// The axis-aligned block-cell region (relative to `anchor`) that an
    /// edit must fall inside to warrant revalidating this instance. Covers
    /// the shell cells, the core, the forge's chimney, and its anvil scan.
    pub fn edit_region<B: BlockRead>(
        self,
        store: &B,
        _anchor: B::Pos,
    ) -> ((i32, i32, i32), (i32, i32, i32)) {
        let Some(def) = store.reg().machine(self) else {
            return ((-3, -1, -3), (3, 5, 3));
        };
        let shell = shape_extent(&stack_shape(&self.mouth(store.reg())));
        if def.handler.reads_chimney() {
            // A kiln's stats read the chimney too (glassworks): cover the
            // three courses of ring over the core, which sits one cell
            // out from the anchor in any cardinal direction.
            let (mn, mx) = shell;
            return (
                (mn.0.min(-2), mn.1, mn.2.min(-2)),
                (mx.0.max(2), mx.1.max(5), mx.2.max(2)),
            );
        }
        if !def.handler.requires_anvil() {
            return shell;
        }
        // Union with the chimney (three courses over the core) and the
        // anvil search box (3x3x3 around the mouth anchor).
        let chimney = shape_extent(&chimney_shape());
        let (mut mn, mut mx) = (shell.0, shell.1);
        mn.0 = mn.0.min(chimney.0.0);
        mn.1 = mn.1.min(chimney.0.1);
        mn.2 = mn.2.min(chimney.0.2);
        mx.0 = mx.0.max(chimney.1.0);
        mx.1 = mx.1.max(chimney.1.1);
        mx.2 = mx.2.max(chimney.1.2);
        mn.0 = mn.0.min(-3);
        mn.1 = mn.1.min(-1);
        mn.2 = mn.2.min(-3);
        mx.0 = mx.0.max(3);
        mx.1 = mx.1.max(1);
        mx.2 = mx.2.max(3);
        (mn, mx)
    }
}

/// The shared shell: a 3-wide, 3-tall firebrick ring around an open core
/// cell (1,0,0) from the mouth anchor, with the mouth block filling the
/// cell opposite the core on the base course. Tries all four cardinal
/// directions; the first that satisfies the ring returns its core.
fn stack_shape(mouth: &[Option<BlockId>; 2]) -> MultiblockShape {
    let mut cells = Vec::with_capacity(3 * 8 + 3);
    for ly in 0..3 {
        // Core column: open air.
        cells.push(ShapeCell {
            offset: (1, ly, 0),
            constraint: BlockConstraint::Air,
        });
        for rx in -1..=1 {
            for rz in -1..=1 {
                if rx == 0 && rz == 0 {
                    continue;
                }
                let offset = (1 + rx, ly, rz);
                let constraint = if offset == (0, 0, 0) {
                    // The mouth block selects WHICH machine this is.
                    BlockConstraint::OneOf(mouth.iter().flatten().copied().collect())
                } else if offset == (1, 0, -1) {
                    // One base-course ring cell is a swappable casing
                    // module slot (spec Part 1.3): any catalog member
                    // holds the stack, and the installed module's
                    // capabilities fold into the frame.
                    BlockConstraint::Module("casing")
                } else {
                    // Any firebrick tier holds a stack together.
                    BlockConstraint::Tag("base:firebrick")
                };
                cells.push(ShapeCell { offset, constraint });
            }
        }
    }
    MultiblockShape {
        cells,
        core: (1, 0, 0),
        rotations: &Rotation::CARDINAL,
    }
}

/// Three more courses of firebrick ring over the stack's core, flue
/// open — the chimney that makes a station a workshop. Rotation-invariant.
fn chimney_shape() -> MultiblockShape {
    let mut cells = Vec::with_capacity(3 * 8 + 3);
    for ly in 3..6 {
        cells.push(ShapeCell {
            offset: (0, ly, 0),
            constraint: BlockConstraint::Air,
        });
        for rx in -1..=1 {
            for rz in -1..=1 {
                if rx == 0 && rz == 0 {
                    continue;
                }
                cells.push(ShapeCell {
                    offset: (rx, ly, rz),
                    constraint: BlockConstraint::Tag("base:firebrick"),
                });
            }
        }
    }
    MultiblockShape {
        cells,
        core: (0, 0, 0),
        rotations: &[Rotation::R0],
    }
}

/// A market stall: two log posts flanking the counter (two tall), bridged
/// by a three-wide awning of solid or glass at post-top height. Tries both
/// axes; the stall stands while either reads true.
fn stall_shape() -> MultiblockShape {
    let mut cells = Vec::with_capacity(7);
    for side in [-1, 1] {
        cells.push(ShapeCell {
            offset: (side, 0, 0),
            constraint: BlockConstraint::Tag("base:logs"),
        });
        cells.push(ShapeCell {
            offset: (side, 1, 0),
            constraint: BlockConstraint::Tag("base:logs"),
        });
    }
    for i in -1..=1 {
        cells.push(ShapeCell {
            offset: (i, 2, 0),
            constraint: BlockConstraint::SolidOrGlass,
        });
    }
    MultiblockShape {
        cells,
        core: (0, 0, 0),
        rotations: &Rotation::CARDINAL,
    }
}

/// Three more courses of firebrick ring over the stack, flue open — the
/// chimney that turns a station into a workshop. Rain never reaches a
/// chimneyed fire.
fn has_chimney_at<B: BlockRead>(store: &B, core: B::Pos) -> bool {
    let shape = chimney_shape();
    match_shape(store, core, &shape).is_some()
}

/// Light a charged machine on a validated shell: fold its stats, flip the
/// mouth block to its lit face, and bank the fire. Generic over the store,
/// so a structure-hosted machine lights exactly like a world-hosted one.
pub(crate) fn light_machine_at<B: BlockStore>(
    store: &mut B,
    pos: B::Pos,
    kind: MachineKind,
    matched: MatchResult<B::Pos>,
) -> Result<(), &'static str> {
    let Some(def) = store.reg().machine(kind).cloned() else {
        return Err("unknown machine");
    };
    let wants = (def.min_charge, def.min_fuel);
    let mut stats = fold_stats(store, &matched.matched);
    if def.handler.reads_chimney() {
        stats.chimney = has_chimney_at(store, matched.core);
    }
    let capabilities = fold_capabilities(store, &matched.matched, &matched.slots);
    let Some(lit_block) = def.mouth_lit.clone() else {
        return Err("this machine has no lit face");
    };
    let world_core = store.to_world(matched.core);
    let Some(BlockEntity::Multiblock(m)) = store.block_entities_mut().get_mut(&pos) else {
        return Err("nothing charged");
    };
    if m.kind != kind || m.lit {
        return Err("already firing");
    }
    let n_charge: u32 = m.charge.iter().flatten().map(|s| s.count).sum();
    let n_fuel: u32 = m.fuel.iter().flatten().map(|s| s.count).sum();
    if n_charge < u32::from(wants.0) || n_fuel < u32::from(wants.1) {
        return Err(match def.handler {
            MachineHandler::Bloomery => "needs at least 2 charge and 2 charcoal",
            MachineHandler::Forge => "needs charge and fuel",
            MachineHandler::Kiln => "needs at least 2 sand and 2 charcoal",
            _ => "nothing to charge",
        });
    }
    m.lit = true;
    m.progress = 0.0;
    m.core = world_core;
    m.stats = stats;
    m.capabilities = capabilities;
    store.swap_block_keep_entity(pos, &lit_block);
    Ok(())
}

/// Re-match one instance. On success the shell's folded stats and
/// capabilities are refreshed (a tier or module swap changes the effective
/// heat); on failure a lit machine is doused right here and its lit block
/// face put back, reaching the same end state `tick_kilns` used to reach
/// by polling. Structure-hosted machines behave the same way, against the
/// structure's own store only.
pub(super) fn revalidate_machine_at<B: BlockStore>(store: &mut B, anchor: B::Pos) {
    let Some(BlockEntity::Multiblock(mut m)) = store.block_entities_mut().remove(&anchor) else {
        return;
    };
    let kind = m.kind;
    let was_lit = m.lit;
    let def = store.reg().machine(kind).cloned();
    let Some(matched) = kind.validate(store, anchor) else {
        if was_lit && def.as_ref().is_some_and(|def| !def.handler.hand_fed()) {
            m.lit = false;
            m.progress = 0.0;
            let unlit = def
                .as_ref()
                .map(|def| def.mouth.as_str())
                .unwrap_or("base:bloomery");
            store.swap_block_keep_entity(anchor, unlit);
        }
        store
            .block_entities_mut()
            .insert(anchor, BlockEntity::Multiblock(m));
        return;
    };
    let mut stats = fold_stats(store, &matched.matched);
    if def.as_ref().is_some_and(|def| def.handler.reads_chimney()) {
        stats.chimney = has_chimney_at(store, matched.core);
    }
    if stats != m.stats {
        m.stats = stats;
    }
    let capabilities = fold_capabilities(store, &matched.matched, &matched.slots);
    if capabilities != m.capabilities {
        m.capabilities = capabilities;
    }
    store
        .block_entities_mut()
        .insert(anchor, BlockEntity::Multiblock(m));
}

/// Revalidate every registered multiblock instance whose shell region could
/// contain the edited position, in a single store. The per-instance test is
/// O(1) arithmetic ([`pos_within_extent`]); only instances actually in range
/// re-run their shape match. Returns how many instances were re-matched so
/// the world's 2c test hook can count them.
pub(super) fn revalidate_machines_around<B: BlockStore>(store: &mut B, pos: B::Pos) -> usize {
    let anchors: Vec<B::Pos> = store
        .block_entities()
        .iter()
        .filter(|(anchor, entity)| {
            let BlockEntity::Multiblock(m) = entity else {
                return false;
            };
            let extent = m.kind.edit_region(store, **anchor);
            pos_within_extent(store, pos, **anchor, extent)
        })
        .map(|(anchor, _)| *anchor)
        .collect();
    for &anchor in &anchors {
        revalidate_machine_at(store, anchor);
    }
    anchors.len()
}

impl BlockRead for World {
    type Pos = BlockPos;

    fn get_block(&self, pos: BlockPos) -> BlockId {
        self.get_block_at(pos)
    }

    fn offset(&self, pos: BlockPos, d: (i32, i32, i32)) -> Option<BlockPos> {
        pos.offset(d.0, d.1, d.2)
    }

    fn cell_delta(&self, from: BlockPos, to: BlockPos) -> Option<(i32, i32, i32)> {
        super::block_store::planetary_delta(from, to)
    }

    fn block_entities(&self) -> &HashMap<BlockPos, BlockEntity> {
        self.installations.entries()
    }

    fn reg(&self) -> &Arc<Registry> {
        &self.reg
    }

    fn to_world(&self, pos: BlockPos) -> Option<BlockPos> {
        Some(pos)
    }

    fn open_sky_above(&self, core: BlockPos) -> bool {
        core.offset(0, 3, 0)
            .is_some_and(|above| self.light_at_pos(above).1 == 15)
    }

    fn weather_at(&self, at: BlockPos) -> LocalWeatherSample {
        self.weather_at_surface(at.surface())
    }
}

impl BlockStore for World {
    fn block_entities_mut(&mut self) -> &mut HashMap<BlockPos, BlockEntity> {
        self.installations.entries_mut()
    }

    fn swap_block_keep_entity(&mut self, pos: BlockPos, block_name: &str) {
        self.swap_block_keep_entity_at(pos, block_name);
    }

    fn material_ledger(&mut self) -> Option<&mut crate::materials::MaterialLedger> {
        self.material_ledger.as_mut()
    }

    fn push_drop_at(&mut self, at: BlockPos, stack: ItemStack) {
        self.push_drop_at(at, stack);
    }
}

mod archaeology;
mod clamps;
mod falling;
mod feeding;
mod firing;
mod legacy_coordinates;
mod modules;
mod workstations;
