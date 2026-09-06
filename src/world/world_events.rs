//! World events coordinator for the authoritative world.

use super::{BlockEntity, BlockId, ItemStack, SignState, World};

impl World {
    /// Enable or disable the authoritative block-edit journal.
    pub fn set_edit_logging(&mut self, enabled: bool) {
        self.log_edits = enabled;
    }

    pub fn edits(&self) -> &[(crate::planet::BlockPos, BlockId, u8, u16, u8)] {
        &self.edit_log
    }

    pub fn take_edits(&mut self) -> Vec<(crate::planet::BlockPos, BlockId, u8, u16, u8)> {
        std::mem::take(&mut self.edit_log)
    }

    pub fn queue_give(&mut self, owner: u32, stack: ItemStack) {
        self.pending_gives.push((owner, stack));
    }

    pub fn take_pending_gives(&mut self) -> Vec<(u32, ItemStack)> {
        std::mem::take(&mut self.pending_gives)
    }

    #[cfg(test)]
    pub(crate) fn pending_drops(&self) -> &[(crate::planet::BlockPos, ItemStack)] {
        &self.pending_drops
    }

    #[cfg(test)]
    pub fn clear_pending_drops(&mut self) {
        let pending = std::mem::take(&mut self.pending_drops);
        for (at, stack) in pending {
            self.retire_arcane_stack_at(at, stack, "pending drop administratively cleared");
        }
    }

    /// Every sign and waystone with its text (world rendering, join sync).
    pub fn sign_texts(&self) -> impl Iterator<Item = (crate::planet::BlockPos, &SignState)> {
        self.installations.iter().filter_map(|(&p, e)| match e {
            BlockEntity::Sign(s) => Some((p, s)),
            _ => None,
        })
    }

    pub fn push_drop_at(&mut self, at: crate::planet::BlockPos, stack: ItemStack) {
        self.pending_drops.push((at, stack));
    }

    pub fn take_pending_drops(&mut self) -> Vec<(crate::planet::BlockPos, ItemStack)> {
        std::mem::take(&mut self.pending_drops)
    }
}
