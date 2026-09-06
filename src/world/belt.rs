//! Belt transport (spec §2.2).
//!
//! A belt is a conveyor of items: cargo rides the belt and advances one
//! cell per unit of belt progress, drawn by the same quantized load-tier
//! machinery trains use. A belt cell's mass is the sum of the item stacks
//! riding it; its tier sets both the draw it demands of the shaft line and
//! the speed step it runs at. Back-pressure (a full cell ahead, a capped
//! mouth, or a full machine feed) stalls the belt and it draws nothing —
//! the same "set demand 0 when backed up" posture the reference game's
//! belts take, and the cheapest honest way to keep an idle line from
//! freeloading.
//!
//! ## Geometry
//!
//! Belt pieces classify by block id into straight (orientable in all four
//! directions), curve (four orientations), the North-climbing incline, a
//! switch (one of two exits, player-selected), a splitter (one input, two
//! outputs, alternating), and a merger (two inputs, one output) — the same
//! orientation-as-block-id convention rail uses (`BlockDef` has no
//! per-block orientation field). A cell's travel direction is stored on
//! its state: curves, splitters, and mergers need to know which way cargo
//! entered to pick the exit, exactly as a train queries `rail_exit`.
//!
//! ## Persistence
//!
//! Belt cargo is no longer transient: `[[belt]]` records in `entities.toml`
//! (capability E8) carry each cell's cargo, progress, entry direction, and
//! splitter phase across a save, so a reloaded line resumes where it left
//! off instead of starting empty.

use std::collections::{HashMap, VecDeque};

use super::*;
use crate::inventory::ItemStack;
use crate::planet::Direction4;
use crate::registry::{BlockId, Registry};

/// How many item stacks may ride a single belt cell before the belt
/// back-pressures (one stack per cell: items ride spaced, one per cell).
pub const BELT_CELL_CAPACITY: usize = 1;

/// How many loose items may sit at the belt's mouth before it stalls.
pub const MAX_LOOSE_ITEMS_AT_END: usize = 1;

/// The belt piece occupying a cell, classified by block id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeltKind {
    /// A straight run facing one of the four cardinals.
    Straight(Direction4),
    /// Connects two perpendicular directions; which pair is the orientation.
    Curve(CurveOrientation),
    /// Rises one block toward North over the run of the piece.
    Incline,
    /// A junction with two possible exits, one currently selected.
    Switch,
    /// One input (from the South cell), two exits (North and East),
    /// alternating which it uses.
    Splitter,
    /// Two inputs (from the South and West cells), one North exit.
    Merger,
}

/// Which perpendicular pair a [`BeltKind::Curve`] connects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveOrientation {
    NE,
    NW,
    SE,
    SW,
}

impl BeltKind {
    pub fn from_block(reg: &Registry, block: BlockId) -> Option<BeltKind> {
        use Direction4::{East, North, South, West};
        match reg.block(block).name.as_str() {
            "base:belt" => Some(BeltKind::Straight(North)),
            "base:belt_s" => Some(BeltKind::Straight(South)),
            "base:belt_e" => Some(BeltKind::Straight(East)),
            "base:belt_w" => Some(BeltKind::Straight(West)),
            "base:belt_curve_ne" => Some(BeltKind::Curve(CurveOrientation::NE)),
            "base:belt_curve_nw" => Some(BeltKind::Curve(CurveOrientation::NW)),
            "base:belt_curve_se" => Some(BeltKind::Curve(CurveOrientation::SE)),
            "base:belt_curve_sw" => Some(BeltKind::Curve(CurveOrientation::SW)),
            "base:belt_incline_n" => Some(BeltKind::Incline),
            "base:belt_switch" => Some(BeltKind::Switch),
            "base:belt_splitter" => Some(BeltKind::Splitter),
            "base:belt_merger" => Some(BeltKind::Merger),
            _ => None,
        }
    }

    /// The travel direction a freshly placed cell of this kind points in.
    /// Cargo placed directly onto a piece starts facing this way; cargo
    /// handed off from another belt carries its own travel direction.
    pub fn facing(self) -> Direction4 {
        match self {
            BeltKind::Straight(facing) => facing,
            _ => Direction4::North,
        }
    }
}

/// Runtime state of one belt cell. Persisted across saves (capability E8).
#[derive(Clone, Debug, PartialEq)]
pub struct BeltState {
    /// Item stacks riding this cell, front = the one about to leave.
    pub cargo: VecDeque<ItemStack>,
    /// 0.0 at this cell, 1.0 when the front item moves on.
    pub progress: f32,
    /// The direction cargo was traveling when it entered this cell. Curves,
    /// splitters, and mergers pick their exit from it.
    pub entry_dir: Direction4,
    /// Splitter alternation bit: which exit the next split item prefers.
    pub split_phase: bool,
}

impl BeltState {
    pub fn new() -> BeltState {
        BeltState {
            cargo: VecDeque::new(),
            progress: 0.0,
            entry_dir: Direction4::North,
            split_phase: false,
        }
    }
}

impl World {
    /// Feed an item stack onto the belt cell at `pos`. Returns false when
    /// the cell is not a belt or is already full. Creates the cell's state
    /// on first use, seeded with the piece's facing.
    #[allow(dead_code)]
    pub fn belt_insert_at(&mut self, pos: BlockPos, stack: ItemStack) -> bool {
        let Some(kind) = BeltKind::from_block(&self.reg, self.get_block_at(pos)) else {
            return false;
        };
        let state = self.belt_state.entry(pos).or_insert_with(BeltState::new);
        if state.cargo.len() >= BELT_CELL_CAPACITY {
            return false;
        }
        state.cargo.push_back(stack);
        state.entry_dir = kind.facing();
        true
    }

    /// The state of the belt cell at `pos`, if any (tests and tooling).
    #[allow(dead_code)]
    pub fn belt_cell_at(&self, pos: BlockPos) -> Option<&BeltState> {
        self.belt_state.get(&pos)
    }

    /// The direction cargo enters `pos` from, if a state is recorded there.
    #[allow(dead_code)]
    pub fn belt_entry_dir_at(&self, pos: BlockPos) -> Option<Direction4> {
        self.belt_state.get(&pos).map(|state| state.entry_dir)
    }

    /// The direction an item traveling `entered` continues in through the
    /// belt piece at `pos` — `None` when the piece does not connect from
    /// that entry (wrong orientation / dead end).
    fn belt_exit(&self, pos: BlockPos, kind: BeltKind, entered: Direction4) -> Option<Direction4> {
        use Direction4::{East, North, South, West};
        match kind {
            BeltKind::Straight(facing) => (entered == facing).then_some(facing),
            BeltKind::Curve(orientation) => belt_curve_exit(orientation, entered),
            BeltKind::Incline => match entered {
                North => Some(North),
                South => Some(South),
                _ => None,
            },
            BeltKind::Switch => {
                // Mainline runs North–South; the East branch is taken from a
                // North-bound approach only when selected.
                let selected = self.switch_selected(pos).unwrap_or(North);
                match entered {
                    North => Some(if selected == East { East } else { North }),
                    South => Some(South),
                    East => Some(North),
                    West => None,
                }
            }
            // The splitter's exit is chosen by `cell_exit` (alternating and
            // back-pressure aware); this connectivity view says the piece
            // takes its one input (from the South cell) toward the North.
            BeltKind::Splitter => (entered == North).then_some(North),
            BeltKind::Merger => match entered {
                North | East => Some(North),
                _ => None,
            },
        }
    }

    /// The cell reached by leaving `pos` in `exit`. A single-cell incline
    /// is a diagonal link: leaving North from an incline climbs one block,
    /// and a plain cell beside a ramp rejoins it from the high side to
    /// descend. An unbuildable neighbor is `None` (a stop).
    fn belt_next(&self, pos: BlockPos, exit: Direction4) -> Option<BlockPos> {
        use Direction4::{North, South};
        let (du, dv) = crate::world::rail::direction_offset(exit);
        let dy = match (
            BeltKind::from_block(&self.reg, self.get_block_at(pos)),
            exit,
        ) {
            (Some(BeltKind::Incline), North) => 1,
            _ => 0,
        };
        let via = pos.offset(du, dy, dv);
        if dy != 0 {
            // Leaving the ramp itself Northward: the climb IS the link.
            return via;
        }
        let Some(plain) = pos.offset(du, 0, dv) else {
            return via;
        };
        if BeltKind::from_block(&self.reg, self.get_block_at(plain)).is_some() {
            return Some(plain);
        }
        // Ramp rejoin (the descent half of the climb link): a cell leaving
        // into plain air still connects onto an incline sitting one block
        // up (North) or down (South) whose high/low partner is this cell.
        if (exit == North || exit == South)
            && let Some(ramp) = plain.offset(0, if exit == South { -1 } else { 1 }, 0)
            && matches!(
                BeltKind::from_block(&self.reg, self.get_block_at(ramp)),
                Some(BeltKind::Incline)
            )
            && self.belt_next(ramp, if exit == South { North } else { South }) == Some(pos)
        {
            return Some(ramp);
        }
        Some(plain)
    }

    /// The exit a cell with cargo may take this tick, or `None` to stall.
    /// Straight/curve/switch/merger cells take their one geometric exit and
    /// stall when it is blocked. A splitter prefers its phase exit and falls
    /// back to the other when that direction is blocked (the "two full
    /// outputs" case stalls).
    fn cell_exit(
        &self,
        states: &HashMap<BlockPos, BeltState>,
        pos: BlockPos,
        kind: BeltKind,
        entered: Direction4,
    ) -> Option<Direction4> {
        use Direction4::{East, North};
        match kind {
            BeltKind::Splitter => {
                let prefer = if states.get(&pos).is_some_and(|state| state.split_phase) {
                    East
                } else {
                    North
                };
                let other = if prefer == East { North } else { East };
                for exit in [prefer, other] {
                    if !self.belt_advance_blocked(states, pos, kind, exit) {
                        return Some(exit);
                    }
                }
                None
            }
            _ => {
                let exit = self.belt_exit(pos, kind, entered)?;
                if self.belt_advance_blocked(states, pos, kind, exit) {
                    return None;
                }
                Some(exit)
            }
        }
    }

    /// Whether the cell beyond `pos` along `exit` can accept another item:
    /// it is not a belt (mouth, machine feed) and is uncapped, or it is a
    /// belt that connects back from this entry and has room. `states` is the
    /// working copy of the belt map so the check sees hand-offs already made
    /// this tick.
    fn belt_advance_blocked(
        &self,
        states: &HashMap<BlockPos, BeltState>,
        pos: BlockPos,
        _kind: BeltKind,
        exit: Direction4,
    ) -> bool {
        let Some(next) = self.belt_next(pos, exit) else {
            return true;
        };
        match BeltKind::from_block(&self.reg, self.get_block_at(next)) {
            Some(next_kind) => {
                // A belt cell ahead: it must connect back from this entry
                // (wrong orientation is a dead end) and have room.
                if self.belt_exit(next, next_kind, exit).is_none() {
                    return true;
                }
                states
                    .get(&next)
                    .is_some_and(|state| state.cargo.len() >= BELT_CELL_CAPACITY)
            }
            None => {
                // A mouth. A machine mouth blocks when its charge is full;
                // otherwise a loose item cap counts the resting items there.
                if self.machine_feed_def(next).is_some() {
                    return self.machine_mouth_full(next);
                }
                let mouth_block = next;
                let above = next.offset(0, 1, 0).unwrap_or(next);
                self.population.loose_items()
                    .iter()
                    .filter(
                        |item| matches!(item.pos.block(), Some(pos) if pos == mouth_block || pos == above),
                    )
                    .count()
                    >= MAX_LOOSE_ITEMS_AT_END
            }
        }
    }

    /// Per-tick step for every belt cell carrying cargo. Each cell computes
    /// its tier from its own cargo mass, its draw against the delivered
    /// shaft rate (inclines cost half again while climbing), and its speed
    /// step; back-pressured cells stall and draw nothing. When the front
    /// item finishes a cell it advances to the next belt cell, feeds a
    /// machine mouth that accepts it, or is dropped as a loose item past
    /// the belt's mouth.
    pub(super) fn tick_belts(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        // Take the belt map out so a cell's advance can hand cargo to the
        // next cell without aliasing the map being iterated (the same
        // take-out/step/put-back shape `tick_rail_motion` uses).
        let mut states: HashMap<BlockPos, BeltState> = std::mem::take(&mut self.belt_state);
        // Farthest-along cells (largest +v) advance first, so a freed cell
        // is visible to the cell behind it in the same tick.
        let mut positions: Vec<BlockPos> = states
            .iter()
            .filter(|(_, state)| !state.cargo.is_empty())
            .map(|(pos, _)| *pos)
            .collect();
        positions.sort_by_key(|pos| std::cmp::Reverse(pos.v()));
        let mut drops: Vec<(BlockPos, ItemStack)> = Vec::new();
        for pos in positions {
            // Cells in chunks that are not loaded yet (restored from a save)
            // park their cargo rather than reading an unloaded chunk as air
            // and spilling it; they resume the moment the chunk loads.
            if !self.chunks.contains_key(&pos.chunk()) {
                continue;
            }
            let Some(kind) = BeltKind::from_block(&self.reg, self.get_block_at(pos)) else {
                // The belt under this cargo was removed or replaced: drop
                // the cargo as loose items rather than leaking it into the
                // void.
                let state = states.remove(&pos).expect("iterated above");
                for stack in state.cargo {
                    drops.push((pos, stack));
                }
                continue;
            };
            let entry_dir = states.get(&pos).expect("iterated above").entry_dir;
            let Some(exit) = self.cell_exit(&states, pos, kind, entry_dir) else {
                // Back-pressure (or a dead-end orientation): stall, demand
                // nothing.
                if let Some(state) = states.get_mut(&pos) {
                    state.progress = 0.0;
                }
                continue;
            };
            let tier = {
                let state = states.get(&pos).expect("iterated above");
                let mass = state
                    .cargo
                    .iter()
                    .map(|stack| crate::world::power_draw::item_stack_mass(&self.reg, *stack))
                    .sum::<f32>();
                crate::world::power_draw::load_tier_for_mass(mass)
            };
            let climbing = kind == BeltKind::Incline && exit == Direction4::North;
            let draw = crate::world::power_draw::incline_multiplier(
                if kind == BeltKind::Incline {
                    Some(crate::world::rail::RailKind::Incline)
                } else {
                    None
                },
                climbing,
            ) * crate::world::power_draw::load_tier_rate(tier);
            let delivered = self.power_at_pos(pos);
            let speed = crate::world::power_draw::effective_speed(
                tier,
                delivered,
                draw,
                crate::world::power_draw::BELT_SPEED,
            );
            if speed <= 0.0 {
                // Overloaded (or no power at all): the belt holds.
                if let Some(state) = states.get_mut(&pos) {
                    state.progress = 0.0;
                }
                continue;
            }
            let mut handoff: Vec<ItemStack> = Vec::new();
            {
                let state = states.get_mut(&pos).expect("iterated above");
                state.progress += speed * dt;
                while state.progress >= 1.0 && !state.cargo.is_empty() {
                    state.progress -= 1.0;
                    handoff.push(state.cargo.pop_front().expect("not empty"));
                }
            }
            let used_split = kind == BeltKind::Splitter && !handoff.is_empty();
            for stack in handoff {
                let Some(next) = self.belt_next(pos, exit) else {
                    drops.push((pos, stack));
                    continue;
                };
                if BeltKind::from_block(&self.reg, self.get_block_at(next)).is_some() {
                    let next_state = states.entry(next).or_insert_with(BeltState::new);
                    next_state.cargo.push_back(stack);
                    next_state.entry_dir = exit;
                } else {
                    let accepted = self.depot_accept(next, &stack);
                    let remainder = ItemStack {
                        count: stack.count - accepted,
                        ..stack
                    };
                    // A depot may accept only part of the cargo. Every
                    // remaining physical item must leave as cargo/loot,
                    // just as it does at an ordinary machine mouth.
                    if remainder.count > 0
                        && let Some(leftover) = self.machine_insert_at(next, remainder)
                    {
                        drops.push((next, leftover));
                    }
                }
            }
            if used_split && let Some(state) = states.get_mut(&pos) {
                state.split_phase = !state.split_phase;
            }
        }
        // Keep cargo cells, and keep splitter cells' phase alive so a drained
        // line resumes alternating instead of resetting.
        states.retain(|pos, state| {
            !state.cargo.is_empty()
                || (state.split_phase
                    && BeltKind::from_block(&self.reg, self.get_block_at(*pos))
                        == Some(BeltKind::Splitter))
        });
        self.belt_state = states;
        for (pos, stack) in drops {
            self.push_drop_at(pos, stack);
        }
    }
}

/// A curve's exit table. `entered` is the direction of travel on arrival.
fn belt_curve_exit(orientation: CurveOrientation, entered: Direction4) -> Option<Direction4> {
    use Direction4::{East, North, South, West};
    match (orientation, entered) {
        (CurveOrientation::NE, North) => Some(East),
        (CurveOrientation::NE, East) => Some(North),
        (CurveOrientation::NE, South) => Some(West),
        (CurveOrientation::NE, West) => Some(South),
        (CurveOrientation::NW, North) => Some(West),
        (CurveOrientation::NW, West) => Some(North),
        (CurveOrientation::NW, South) => Some(East),
        (CurveOrientation::NW, East) => Some(South),
        (CurveOrientation::SE, South) => Some(East),
        (CurveOrientation::SE, East) => Some(South),
        (CurveOrientation::SE, North) => Some(West),
        (CurveOrientation::SE, West) => Some(North),
        (CurveOrientation::SW, South) => Some(West),
        (CurveOrientation::SW, West) => Some(South),
        (CurveOrientation::SW, North) => Some(East),
        (CurveOrientation::SW, East) => Some(North),
    }
}
