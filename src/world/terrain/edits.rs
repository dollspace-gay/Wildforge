//! Shared resident voxel writes and support/light classification.

use super::TerrainStore;
use crate::chunk::{CHUNK_X, CHUNK_Z};
use crate::planet::BlockPos;
use crate::registry::{AIR, BlockId, Registry};

impl TerrainStore {
    /// Write only an already resident cell. The coordinator owns logging,
    /// conservation, support consequences, and when the light solve happens.
    pub(in crate::world) fn write_state(
        &mut self,
        position: BlockPos,
        block: BlockId,
        metadata: u8,
        salt_mass: u16,
        soil_salinity: u8,
    ) -> Option<BlockId> {
        let (x, y, z) = position.local();
        let chunk = self.resident.get_mut(&position.chunk())?;
        let old = chunk.get(x, y, z);
        chunk.set(x, y, z, block);
        chunk.set_meta(x, y, z, metadata);
        chunk.set_water_salt(x, y, z, salt_mass);
        chunk.set_soil_salinity(x, y, z, soil_salinity);
        chunk.dirty = true;
        chunk.modified = true;
        Some(old)
    }

    pub(in crate::world) fn dirty_edit_neighbors(&mut self, position: BlockPos) {
        let (x, _, z) = position.local();
        let chunk = position.chunk();
        if x == 0 {
            self.mark_dirty(chunk.offset(-1, 0), true);
        } else if x == CHUNK_X - 1 {
            self.mark_dirty(chunk.offset(1, 0), true);
        }
        if z == 0 {
            self.mark_dirty(chunk.offset(0, -1), true);
        } else if z == CHUNK_Z - 1 {
            self.mark_dirty(chunk.offset(0, 1), true);
        }
    }

    /// Plants, torches, and layers use the same support rule in authority and
    /// replica application. Only authority turns the result into physical drops.
    pub(in crate::world) fn unsupported_above(
        &self,
        registry: &Registry,
        position: BlockPos,
        block: BlockId,
    ) -> Option<(BlockPos, BlockId)> {
        if registry.is_solid(block) {
            return None;
        }
        let above = position.offset(0, 1, 0)?;
        let (x, y, z) = above.local();
        let above_block = self
            .resident
            .get(&above.chunk())
            .map_or(AIR, |chunk| chunk.get(x, y, z));
        let definition = registry.block(above_block);
        (above_block != AIR
            && !definition.floats
            && (definition.cross || definition.height.is_some()))
        .then_some((above, above_block))
    }
}
