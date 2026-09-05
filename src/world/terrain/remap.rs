//! Resident content-ID remapping without generation or persistence policy.

use super::TerrainStore;
use crate::registry::{BlockId, Registry};

impl TerrainStore {
    pub(in crate::world) fn remap_from(&mut self, old: &Registry, registry: &Registry) {
        let map: Vec<BlockId> = old.blocks.iter()
            .map(|block| registry.block_id(&block.name).unwrap_or(registry.unknown_block))
            .collect();
        for chunk in self.resident.values_mut() {
            for cell in chunk.raw_mut() {
                *cell = map.get(*cell as usize).copied().unwrap_or(registry.unknown_block).0;
            }
            // Removed definitions may collapse many IDs onto one placeholder.
            chunk.compact();
            chunk.dirty = true;
        }
    }
}
