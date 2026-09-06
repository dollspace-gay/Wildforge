//! Chunk access storage transaction coordination.

use crate::chunk::ChunkPos;
use crate::world::ChunkLoader;
use crate::world::ChunkRead;
use crate::world::World;
use std::sync::Arc;

impl World {
    pub(crate) fn chunk_loader(&self) -> ChunkLoader {
        ChunkLoader {
            store: self.region_store.clone(),
            palette: self.palette.snapshot(&self.reg),
            reg: Arc::clone(&self.reg),
        }
    }

    pub(in crate::world) fn try_load_chunk(&self, pos: ChunkPos) -> std::io::Result<ChunkRead> {
        self.chunk_loader().load(pos)
    }

    /// Planetary WFC8 block/metadata/water-salt/soil-salt RLE and HU reservoir
    /// residuals for disk. Derived light is deliberately omitted from saves.
    #[cfg(test)]
    pub fn chunk_rle(&self, pos: ChunkPos) -> Option<Vec<u8>> {
        let chunk = self.chunks.get(&pos)?;
        Some(crate::world::storage::encode_chunk(chunk))
    }
}
