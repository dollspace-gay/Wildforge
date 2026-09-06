//! Legacy coordinates machines transaction coordination.

use crate::planet::BlockPos;
use crate::inventory::ItemStack;
use crate::world::World;
use super::check_glassworks_at;
use super::check_stall_at;

impl World {
    // Positive-Z adapters exist only for the pre-topology fixture suite.
    #[cfg(test)]
    pub fn check_bloomery(&self, x: i32, y: i32, z: i32) -> Option<BlockPos> {
        BlockPos::of_world(x, y, z).and_then(|pos| self.check_bloomery_at(pos))
    }

    #[cfg(test)]
    pub fn check_forge(&self, x: i32, y: i32, z: i32) -> Option<BlockPos> {
        BlockPos::of_world(x, y, z).and_then(|pos| self.check_forge_at(pos))
    }

    #[cfg(test)]
    pub fn check_glassworks(&self, x: i32, y: i32, z: i32) -> Option<BlockPos> {
        BlockPos::of_world(x, y, z).and_then(|pos| self.check_glassworks_at(pos))
    }

    #[cfg(test)]
    pub fn check_separator(&self, x: i32, y: i32, z: i32) -> Option<BlockPos> {
        BlockPos::of_world(x, y, z).and_then(|pos| self.check_separator_at(pos))
    }

    #[cfg(test)]
    pub fn check_kiln(&self, x: i32, y: i32, z: i32) -> Option<BlockPos> {
        BlockPos::of_world(x, y, z).and_then(|pos| self.check_kiln_at(pos))
    }

    #[cfg(test)]
    pub fn check_stall(&self, x: i32, y: i32, z: i32) -> bool {
        BlockPos::of_world(x, y, z).is_some_and(|pos| self.check_stall_at(pos))
    }

    #[cfg(test)]
    pub fn light_bloomery(&mut self, x: i32, y: i32, z: i32) -> Result<(), &'static str> {
        self.light_bloomery_at(BlockPos::of_world(x, y, z).ok_or("outside the world")?)
    }

    #[cfg(test)]
    pub fn light_forge(&mut self, x: i32, y: i32, z: i32) -> Result<(), &'static str> {
        self.light_forge_at(BlockPos::of_world(x, y, z).ok_or("outside the world")?)
    }

    #[cfg(test)]
    pub fn light_kiln(&mut self, x: i32, y: i32, z: i32) -> Result<(), &'static str> {
        self.light_kiln_at(BlockPos::of_world(x, y, z).ok_or("outside the world")?)
    }

    #[cfg(test)]
    pub fn try_light_clamp(&mut self, x: i32, y: i32, z: i32) -> Result<usize, &'static str> {
        self.try_light_clamp_at(BlockPos::of_world(x, y, z).ok_or("outside the world")?)
    }

    #[cfg(test)]
    pub fn anvil_put(&mut self, pos: (i32, i32, i32), stack: ItemStack) -> bool {
        BlockPos::of_world(pos.0, pos.1, pos.2).is_some_and(|at| self.anvil_put_at(at, stack))
    }

    #[cfg(test)]
    pub fn anvil_take(&mut self, pos: (i32, i32, i32)) -> Option<ItemStack> {
        BlockPos::of_world(pos.0, pos.1, pos.2).and_then(|at| self.anvil_take_at(at))
    }

    #[cfg(test)]
    pub fn anvil_strike(&mut self, pos: (i32, i32, i32)) -> Option<ItemStack> {
        BlockPos::of_world(pos.0, pos.1, pos.2).and_then(|at| self.anvil_strike_at(at))
    }

    #[cfg(test)]
    pub fn brush_block(&mut self, x: i32, y: i32, z: i32, rng: &mut u32) -> Option<ItemStack> {
        BlockPos::of_world(x, y, z).and_then(|pos| self.brush_block_at(pos, rng))
    }
}
