//! Depots calendar transaction coordination.

use crate::planet::BlockPos;
use crate::world::BlockEntity;
use crate::world::World;

impl World {
    /// The delivery contract of the depot at `pos` (capability E13): how
    /// many units of `item` the bound settlement still wants staged, and
    /// the reputation per unit. `None` when the cell is not a depot,
    /// nothing is bound, or the item is not one of its needs.
    pub fn depot_need_at(
        &self,
        pos: BlockPos,
        item: crate::registry::ItemId,
    ) -> Option<(u32, u32)> {
        let Some(BlockEntity::Depot(d)) = self.block_entity_at(&pos) else {
            return None;
        };
        let def = self.reg.settlements.iter().find(|s| s.id == d.settlement)?;
        let need = def.needs.iter().find(|need| need.item == item)?;
        // Staged stock counts against the appetite: a depot full of iron
        // has no more use for iron.
        let staged: u32 = d
            .storage
            .iter()
            .flatten()
            .filter(|stack| stack.item == item)
            .map(|stack| stack.count)
            .sum();
        let wanted = 64u32.saturating_sub(staged.min(64));
        (wanted > 0).then_some((wanted, need.rep_per_unit))
    }

    /// A belt's offer to the depot at `pos`: how many units of this stack
    /// the settlement currently needs (capability E13 belt port).
    pub fn depot_accept(&mut self, pos: BlockPos, stack: &crate::inventory::ItemStack) -> u32 {
        self.depot_deposit(pos, stack)
    }

    /// Transfer a player's held goods into a depot. Solo and networked play
    /// share this boundary: refused goods leave both inventories unchanged,
    /// and the accepted physical stack is debited from its exact source slot.
    pub(crate) fn deliver_to_depot(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
        slot: usize,
    ) -> Option<(String, crate::registry::ItemId, u32, u32)> {
        let held = inventory.slots.get(slot).copied().flatten()?;
        let (_, rep_per_unit) = self.depot_need_at(pos, held.item)?;
        let Some(BlockEntity::Depot(depot)) = self.block_entity_at(&pos) else {
            return None;
        };
        let settlement = depot.settlement.clone();
        let accepted = self.depot_deposit(pos, &held);
        if accepted == 0 {
            return None;
        }
        inventory.slots[slot] = (held.count > accepted).then_some(crate::inventory::ItemStack {
            count: held.count - accepted,
            ..held
        });
        Some((settlement, held.item, accepted, rep_per_unit))
    }

    /// Deposit the needed portion of `stack` into the depot at `pos`,
    /// respecting staging capacity. Returns how many units were accepted.
    pub fn depot_deposit(&mut self, pos: BlockPos, stack: &crate::inventory::ItemStack) -> u32 {
        let Some((wanted, _)) = self.depot_need_at(pos, stack.item) else {
            return 0;
        };
        let max_stack = self.reg.item(stack.item).max_stack;
        let Some(BlockEntity::Depot(d)) = self.block_entity_mut_at(&pos) else {
            return 0;
        };
        let offered = stack.count.min(wanted);
        let mut left = offered;
        // Top up part-stacks first.
        for slot in d.storage.iter_mut().flatten() {
            if slot.item == stack.item
                && slot.arcane_id == stack.arcane_id
                && slot.durability == stack.durability
            {
                let take = left.min(max_stack.saturating_sub(slot.count));
                slot.count += take;
                left -= take;
                if left == 0 {
                    break;
                }
            }
        }
        if left > 0 {
            for slot in d.storage.iter_mut() {
                if slot.is_none() {
                    let take = left.min(max_stack);
                    if take > 0 {
                        *slot = Some(crate::inventory::ItemStack {
                            count: take,
                            ..*stack
                        });
                        left -= take;
                    }
                    if left == 0 {
                        break;
                    }
                }
            }
        }
        offered - left
    }
}
