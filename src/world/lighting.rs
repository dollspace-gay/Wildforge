//! RGB block light, skylight, and cross-chunk relight cascades.

use super::{TerrainRead, World};
#[cfg(test)]
use crate::chunk::CHUNK_Y;
use crate::chunk::ChunkPos;
use crate::planet::BlockPos;

impl World {
    /// (block-light intensity, sky light) at a canonical planetary cell.
    pub fn light_at_pos(&self, pos: BlockPos) -> (u8, u8) {
        TerrainRead::light_at_pos(self, pos)
    }

    /// (block-light intensity, sky light) at a world position. Unloaded chunks
    /// read as open sky so the world's edge doesn't render black.
    #[cfg(test)]
    pub fn light_at(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        if y < 0 {
            return (0, 0);
        }
        if y >= CHUNK_Y as i32 {
            return (0, 15);
        }
        BlockPos::of_world(x, y, z).map_or((0, 15), |pos| self.light_at_pos(pos))
    }

    /// (block-light r,g,b, sky light) at a canonical planetary cell.
    #[cfg(test)]
    pub fn light_rgb_at_pos(&self, pos: BlockPos) -> ([u8; 3], u8) {
        TerrainRead::light_rgb_at_pos(self, pos)
    }

    /// (block-light r,g,b, sky light) at a world position — the full colored
    /// signal the mesher bakes into vertices.
    #[cfg(test)]
    pub fn light_rgb_at(&self, x: i32, y: i32, z: i32) -> ([u8; 3], u8) {
        if y < 0 {
            return ([0; 3], 0);
        }
        if y >= CHUNK_Y as i32 {
            return ([0; 3], 15);
        }
        BlockPos::of_world(x, y, z).map_or(([0; 3], 15), |pos| self.light_rgb_at_pos(pos))
    }

    /// Settle a changed chunk and loaded neighbors through the shared cascade.
    /// The store preserves the 18-visit cap, above the full 15-level light range.
    pub fn relight_and_cascade(&mut self, position: ChunkPos) {
        self.relight_chunks_and_cascade([position]);
    }

    pub(super) fn relight_chunks_and_cascade(
        &mut self,
        positions: impl IntoIterator<Item = ChunkPos>,
    ) {
        self.chunks.relight_chunks_and_cascade(&self.reg, positions);
    }
}
