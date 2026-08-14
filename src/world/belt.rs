//! Minimal belt segments (spec §2.2, scoped).
//!
//! A belt is a straight, single-direction conveyor of items: cargo rides
//! the belt and advances one cell per unit of belt progress, drawn by the
//! same quantized load-tier machinery trains use. A belt cell's mass is the
//! sum of the item stacks riding it; its tier sets both the draw it demands
//! of the shaft line and the speed step it runs at. Back-pressure (a full
//! cell ahead, or a capped mouth) stalls the belt and it draws nothing —
//! the same "set demand 0 when backed up" posture the reference game's
//! belts take, and the cheapest honest way to keep an idle line from
//! freeloading.
//!
//! ## Scoped
//!
//! Straight segments only; travel direction is fixed at North (+v), the
//! same axis the rail incline climbs. Curves, inclines, switches, and
//! orientation are explicitly a later belt phase — the tier machinery is
//! shared (`power_draw`), the geometry is not. Belt cargo is transient
//! runtime state, exactly like `RailState`: a reloaded belt is empty until
//! it is fed again.

use std::collections::{HashMap, VecDeque};

use super::*;
use crate::inventory::ItemStack;
use crate::registry::{BlockId, Registry};

/// How many item stacks may ride a single belt cell before the belt
/// back-pressures (one stack per cell: items ride spaced, one per cell).
pub const BELT_CELL_CAPACITY: usize = 1;

/// How many loose items may sit at the belt's mouth before it stalls.
pub const MAX_LOOSE_ITEMS_AT_END: usize = 1;

/// The belt piece occupying a cell, classified by block id. One variant
/// this phase; geometry variants will extend the enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeltKind {
    Straight,
}

impl BeltKind {
    pub fn from_block(reg: &Registry, block: BlockId) -> Option<BeltKind> {
        match reg.block(block).name.as_str() {
            "base:belt" => Some(BeltKind::Straight),
            _ => None,
        }
    }
}

/// Runtime state of one belt cell. Mirrors `RailState`'s role for trains:
/// transient, not persisted.
#[derive(Clone, Debug, PartialEq)]
pub struct BeltState {
    /// Item stacks riding this cell, front = the one about to leave.
    pub cargo: VecDeque<ItemStack>,
    /// 0.0 at this cell, 1.0 when the front item moves on.
    pub progress: f32,
}

impl BeltState {
    pub fn new() -> BeltState {
        BeltState {
            cargo: VecDeque::new(),
            progress: 0.0,
        }
    }
}

impl World {
    /// Feed an item stack onto the belt cell at `pos`. Returns false when
    /// the cell is not a belt or is already full. Creates the cell's state
    /// on first use; the belt's travel direction is North (+v).
    #[allow(dead_code)]
    pub fn belt_insert_at(&mut self, pos: BlockPos, stack: ItemStack) -> bool {
        if BeltKind::from_block(&self.reg, self.get_block_at(pos)).is_none() {
            return false;
        }
        let state = self.belt_state.entry(pos).or_insert_with(BeltState::new);
        if state.cargo.len() >= BELT_CELL_CAPACITY {
            return false;
        }
        state.cargo.push_back(stack);
        true
    }

    /// The state of the belt cell at `pos`, if any (tests and tooling).
    #[allow(dead_code)]
    pub fn belt_cell_at(&self, pos: BlockPos) -> Option<&BeltState> {
        self.belt_state.get(&pos)
    }

    /// Whether the cell beyond `pos` along the belt's travel axis (North)
    /// can accept another item: it is not a belt (mouth) and is uncapped,
    /// or it is a belt with room. `states` is the working copy of the belt
    /// map so the check sees hand-offs already made this tick.
    fn belt_advance_blocked(&self, states: &HashMap<BlockPos, BeltState>, pos: BlockPos) -> bool {
        let Some(next) = pos.offset(0, 0, 1) else {
            return true;
        };
        if BeltKind::from_block(&self.reg, self.get_block_at(next)).is_some() {
            // A belt cell ahead: blocked when it has no room.
            return states
                .get(&next)
                .is_some_and(|state| state.cargo.len() >= BELT_CELL_CAPACITY);
        }
        // A mouth: blocked when too many loose items wait there. A resting
        // item settles one cell above the ground it rests on, so a mouth
        // item is found in the mouth cell or the cell directly above it.
        let mouth_block = next;
        let above = next.offset(0, 1, 0).unwrap_or(next);
        self.loose_items
            .iter()
            .filter(
                |item| matches!(item.pos.block(), Some(pos) if pos == mouth_block || pos == above),
            )
            .count()
            >= MAX_LOOSE_ITEMS_AT_END
    }

    /// Per-tick step for every belt cell carrying cargo. Each cell computes
    /// its tier from its own cargo mass, its draw against the delivered
    /// shaft rate, and its speed step; back-pressured cells stall and draw
    /// nothing. When the front item finishes a cell it advances to the next
    /// belt cell, or is dropped as a loose item past the belt's mouth.
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
            if self.get_block_at(pos) == AIR {
                // The belt under this cargo was removed: drop the cargo as
                // loose items rather than leaking it into the void.
                let state = states.remove(&pos).expect("iterated above");
                for stack in state.cargo {
                    drops.push((pos, stack));
                }
                continue;
            }
            if self.belt_advance_blocked(&states, pos) {
                // Back-pressure: stall, demand nothing.
                if let Some(state) = states.get_mut(&pos) {
                    state.progress = 0.0;
                }
                continue;
            }
            let tier = {
                let state = states.get(&pos).expect("iterated above");
                let mass = state
                    .cargo
                    .iter()
                    .map(|stack| crate::world::power_draw::item_stack_mass(&self.reg, *stack))
                    .sum::<f32>();
                crate::world::power_draw::load_tier_for_mass(mass)
            };
            let draw = crate::world::power_draw::load_tier_rate(tier);
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
            for stack in handoff {
                let Some(next) = pos.offset(0, 0, 1) else {
                    drops.push((pos, stack));
                    continue;
                };
                if BeltKind::from_block(&self.reg, self.get_block_at(next)).is_some() {
                    states
                        .entry(next)
                        .or_insert_with(BeltState::new)
                        .cargo
                        .push_back(stack);
                } else {
                    // The mouth: the item leaves the belt as a loose item.
                    drops.push((next, stack));
                }
            }
        }
        states.retain(|_, state| !state.cargo.is_empty());
        self.belt_state = states;
        for (pos, stack) in drops {
            self.push_drop_at(pos, stack);
        }
    }
}
