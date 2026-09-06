//! Feeding machines transaction coordination.

use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::world::BlockEntity;
use crate::world::MachineInstance;
use crate::world::World;
use crate::world::multiblock::MachineKind;

impl World {
    /// Resolve a qualified machine id to its kind. Base ships all four of
    /// its machines, so a missing lookup here degrades to kind 0 rather
    /// than panicking mid-save.
    pub(crate) fn machine_kind(&self, name: &str) -> MachineKind {
        self.reg.machine_kind(name).unwrap_or_default()
    }

    /// The belt-feed contract of the machine whose mouth occupies `pos`:
    /// the block's interaction resolves to a fire-handler machine (it
    /// accepts item-stack charge), and its shell is actually built. The
    /// count-based separator and the recipe-station workbench do not take
    /// belt feed and resolve to `None`.
    pub(crate) fn machine_feed_def(
        &self,
        pos: BlockPos,
    ) -> Option<(MachineKind, &crate::machines::MachineDef)> {
        let interaction = self
            .reg
            .block(self.get_block_at(pos))
            .interaction
            .as_deref()?;
        let kind = self.reg.machine_by_interaction(interaction)?;
        let def = self.reg.machine(kind)?;
        if !def.handler.has_fire() || def.charge_slots == 0 {
            return None;
        }
        kind.validate(self, pos)?;
        Some((kind, def))
    }

    /// Whether the machine mouth at `pos` has no free charge slot, so a
    /// belt feeding it must stall (back-pressure) rather than overflow.
    pub(crate) fn machine_mouth_full(&self, pos: BlockPos) -> bool {
        let Some((_, def)) = self.machine_feed_def(pos) else {
            return false;
        };
        match self.block_entity_at(&pos) {
            Some(BlockEntity::Multiblock(machine)) => !machine
                .charge
                .iter()
                .take(def.charge_slots as usize)
                .any(Option::is_none),
            // A built shell with no instance yet: the first belt feed gives
            // it one, and a fresh machine has empty charge slots.
            _ => false,
        }
    }

    /// Insert a stack into the machine mouth at `pos`'s charge slots,
    /// merging into a matching slot first then the first empty one. Returns
    /// the leftover (the full original stack) when the machine is not
    /// belt-fed or has no room, so the belt can drop it as a loose item.
    pub(crate) fn machine_insert_at(
        &mut self,
        pos: BlockPos,
        stack: ItemStack,
    ) -> Option<ItemStack> {
        // Scope the machine-feed lookup so its `&MachineDef` borrow of the
        // registry ends before the block entity is borrowed mutably below.
        let (kind, charge_slots) = match self.machine_feed_def(pos) {
            Some((kind, def)) => (kind, def.charge_slots as usize),
            None => return Some(stack),
        };
        // A built shell with no instance yet gets one — the belt is its
        // first touch, exactly as a player's first interaction would be.
        self.ensure_block_entity_at(
            pos,
            BlockEntity::Multiblock(MachineInstance {
                kind,
                ..Default::default()
            }),
        );
        let reg = self.reg.clone();
        let Some(BlockEntity::Multiblock(machine)) = self.block_entity_mut_at(&pos) else {
            return Some(stack);
        };
        for slot in machine.charge.iter_mut().take(charge_slots) {
            if let Some(existing) = slot
                && existing.can_merge(&reg, &stack)
            {
                let max = reg.item(stack.item).max_stack;
                let room = max.saturating_sub(existing.count);
                if room > 0 {
                    let add = stack.count.min(room);
                    existing.count += add;
                    let left = stack.count - add;
                    return if left == 0 {
                        None
                    } else {
                        Some(ItemStack {
                            count: left,
                            ..stack
                        })
                    };
                }
            }
        }
        for slot in machine.charge.iter_mut().take(charge_slots) {
            if slot.is_none() {
                *slot = Some(stack);
                return None;
            }
        }
        Some(stack)
    }
}
