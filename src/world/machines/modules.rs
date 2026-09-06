//! Modules machines transaction coordination.

use super::slot_of_instance_at;
use crate::planet::BlockPos;
use crate::registry::BlockId;
use crate::world::World;
use crate::world::multiblock::modules_in_category;

impl World {
    /// Find the `(anchor, category)` of the instance whose matched shell
    /// has `pos` as a module-slot cell. The O(1) `edit_region` test gates
    /// every candidate before its (more expensive) shape re-match.
    pub(super) fn slot_of_instance_at(&self, pos: BlockPos) -> Option<(BlockPos, &'static str)> {
        slot_of_instance_at(self, pos)
    }

    /// Swap the module installed in a slot cell in place (spec Part 1.3).
    /// Only a real slot cell of a registered frame may be swapped, the
    /// replacement must belong to the slot's catalog, and the swap is a
    /// plain block edit: the machine's `BlockEntity` at the anchor is
    /// untouched, and the 2c edit hook re-folds the frame's stats and
    /// capabilities immediately.
    pub fn swap_slot_module_at(
        &mut self,
        pos: BlockPos,
        category: &'static str,
        replacement: BlockId,
    ) -> Result<(), &'static str> {
        let (_, found) = self.slot_of_instance_at(pos).ok_or("no module slot here")?;
        if found != category {
            return Err("this slot takes a different module category");
        }
        if !modules_in_category(&self.reg, category).contains(&replacement) {
            return Err("that is not a module of this slot's category");
        }
        if self.get_block_at(pos) == replacement {
            return Ok(());
        }
        self.set_block_at(pos, replacement);
        Ok(())
    }
}
