//! Firing machines transaction coordination.

#[cfg(test)]
use super::check_glassworks_at;
use super::check_machine_at;
use super::check_stall_at;
use super::light_machine_at;
use crate::planet::BlockPos;
use crate::world::World;

impl World {
    // ---------------- steelworks ----------------

    /// Validate the bloomery multiblock at this mouth: a hollow 1x1
    /// core beside the mouth wrapped in a 3-wide, 3-tall firebrick
    /// ring (23 firebrick + the mouth), open on top. Returns the core.
    #[cfg(test)]
    pub fn check_bloomery_at(&self, pos: BlockPos) -> Option<BlockPos> {
        check_machine_at(self, "base:bloomery", pos)
    }

    /// Validate the forge: the firebrick stack with a forge mouth,
    /// PLUS a chimney (three more courses of firebrick ring around an
    /// open flue above the stack — rain never reaches the fire) and a
    /// stone anvil within three blocks of the mouth. A building, not
    /// a block: the workshop is the capital (economy plan, leg 2).
    #[cfg(test)]
    pub fn check_forge_at(&self, pos: BlockPos) -> Option<BlockPos> {
        check_machine_at(self, "base:forge", pos)
    }

    /// Light a charged forge. Errors name what's missing. The generic
    /// [`light_machine_at`] is the data-driven path; this base shortcut is
    /// kept for the test helpers.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn light_forge_at(&mut self, pos: BlockPos) -> Result<(), &'static str> {
        let kind = self.machine_kind("base:forge");
        let matched = kind
            .validate(self, pos)
            .ok_or("the forge wants its stack, chimney, and anvil")?;
        light_machine_at(self, pos, kind, matched)
    }

    /// A kiln whose stack carries the chimney is a GLASSWORKS: the
    /// draft doubles what each fuel fires, and weather means nothing
    /// (economy plan, leg 2 — same capital rule as the forge).
    #[cfg(test)]
    pub fn check_glassworks_at(&self, pos: BlockPos) -> Option<BlockPos> {
        check_glassworks_at(self, pos)
    }

    /// The same stack with a separator in its mouth splits the mixed
    /// rare-earth powder instead (mechanization stage 6).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn check_separator_at(&self, pos: BlockPos) -> Option<BlockPos> {
        check_machine_at(self, "base:separator", pos)
    }

    /// The same stack with a kiln in its mouth fires glass instead.
    #[cfg(test)]
    pub fn check_kiln_at(&self, pos: BlockPos) -> Option<BlockPos> {
        check_machine_at(self, "base:kiln", pos)
    }

    /// Validate a market stall at its counter: two log posts (two
    /// tall) flanking the counter along either axis, bridged by a
    /// three-wide awning of solid or glass at post-top height. A
    /// stall trades only while it stands (trade & travel, stage 3).
    pub fn check_stall_at(&self, pos: BlockPos) -> bool {
        check_stall_at(self, pos)
    }

    /// Light a charged bloomery. Errors name what's missing.
    pub fn light_bloomery_at(&mut self, pos: BlockPos) -> Result<(), &'static str> {
        let kind = self.machine_kind("base:bloomery");
        let matched = kind.validate(self, pos).ok_or("the stack is breached")?;
        light_machine_at(self, pos, kind, matched)
    }

    /// Light a charged kiln. Errors name what's missing.
    pub fn light_kiln_at(&mut self, pos: BlockPos) -> Result<(), &'static str> {
        let kind = self.machine_kind("base:kiln");
        let matched = kind.validate(self, pos).ok_or("the stack is breached")?;
        light_machine_at(self, pos, kind, matched)
    }

    /// Swap a block without invalidating the machine living there.
    pub(in crate::world) fn swap_block_keep_entity_at(&mut self, pos: BlockPos, to: &str) {
        let Some(to) = self.reg.block_id(to) else {
            return;
        };
        let e = self.installations.remove(&pos);
        self.set_block_at(pos, to);
        if let Some(e) = e {
            self.installations.insert(pos, e);
        }
    }
}
