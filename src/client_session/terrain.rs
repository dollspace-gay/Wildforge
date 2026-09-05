//! Paced terrain and block updates share one incoming world lifetime.

use std::collections::VecDeque;

use super::palette::ContentMap;
use crate::chunk::ChunkPos;
use crate::planet::BlockPos;
use crate::world::World;

#[derive(Default)]
pub(super) struct TerrainInbox {
    chunks: VecDeque<(ChunkPos, Vec<u8>)>,
    blocks: Vec<(BlockPos, u16, u8, u16, u8)>,
}

impl TerrainInbox {
    pub(super) fn contains(&self, position: ChunkPos) -> bool {
        self.chunks.iter().any(|(queued, _)| *queued == position)
    }

    pub(super) fn chunk(&mut self, position: ChunkPos, bytes: Vec<u8>) {
        self.chunks.push_back((position, bytes));
    }

    pub(super) fn block(
        &mut self,
        position: BlockPos,
        block: u16,
        metadata: u8,
        salt_mass: u16,
        soil_salinity: u8,
    ) {
        self.blocks
            .push((position, block, metadata, salt_mass, soil_salinity));
    }

    /// Return attempted positions so consumers can retire requests, even on a
    /// rejected payload. Only the replica can establish successful residency.
    pub(super) fn apply(
        &mut self,
        world: &mut World,
        content: &ContentMap,
        budget: usize,
    ) -> Vec<ChunkPos> {
        let chunks: Vec<_> = self.chunks.drain(..self.chunks.len().min(budget)).collect();
        if !chunks.is_empty() {
            debug_assert!(world.is_remote(), "guest terrain needs a replica");
            world.insert_remote_chunks(
                chunks
                    .iter()
                    .map(|(position, bytes)| (*position, bytes.as_slice())),
                content.blocks(),
            );
        }
        if !self.blocks.is_empty() {
            world.apply_remote_block_states(
                self.blocks
                    .drain(..)
                    .map(|(pos, id, meta, salt, soil)| (pos, content.block(id), meta, salt, soil)),
            );
        }
        chunks.into_iter().map(|(position, _)| position).collect()
    }
}
