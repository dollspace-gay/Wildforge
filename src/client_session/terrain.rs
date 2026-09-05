//! Paced terrain and block updates share one incoming world lifetime.

use std::collections::VecDeque;

use super::palette::ContentMap;
use crate::chunk::ChunkPos;
use crate::planet::BlockPos;
use crate::world::ReplicationTarget;

#[derive(Default)]
pub(super) struct TerrainInbox {
    pending: VecDeque<Mutation>,
}

enum Mutation {
    Chunk(ChunkPos, Vec<u8>),
    Block(BlockPos, u16, u8, u16, u8),
}

impl TerrainInbox {
    pub(super) fn contains(&self, position: ChunkPos) -> bool {
        self.pending
            .iter()
            .any(|mutation| matches!(mutation, Mutation::Chunk(queued, _) if *queued == position))
    }

    pub(super) fn chunk(&mut self, position: ChunkPos, bytes: Vec<u8>) {
        self.pending.push_back(Mutation::Chunk(position, bytes));
    }

    pub(super) fn block(
        &mut self,
        position: BlockPos,
        block: u16,
        metadata: u8,
        salt_mass: u16,
        soil_salinity: u8,
    ) {
        self.pending.push_back(Mutation::Block(
            position,
            block,
            metadata,
            salt_mass,
            soil_salinity,
        ));
    }

    /// Return attempted positions so consumers can retire requests, even on a
    /// rejected payload. Only the replica can establish successful residency.
    pub(super) fn apply(
        &mut self,
        world: &mut impl ReplicationTarget,
        content: &ContentMap,
        budget: usize,
    ) -> Vec<ChunkPos> {
        let mut remaining = budget;
        let mut attempted = Vec::new();
        let mut chunks = Vec::new();
        let mut blocks = Vec::new();
        while let Some(next) = self.pending.front() {
            if matches!(next, Mutation::Chunk(..)) && remaining == 0 {
                break;
            }
            let Some(mutation) = self.pending.pop_front() else {
                break;
            };
            match mutation {
                Mutation::Chunk(position, bytes) => {
                    // A later snapshot must follow earlier edits, including
                    // when a poll interleaves several updates of one chunk.
                    if !blocks.is_empty() {
                        apply_batch(world, content, &mut chunks, &mut blocks);
                    }
                    attempted.push(position);
                    chunks.push((position, bytes));
                    remaining -= 1;
                }
                Mutation::Block(pos, id, meta, salt, soil) => {
                    blocks.push((pos, id, meta, salt, soil));
                }
            }
        }
        apply_batch(world, content, &mut chunks, &mut blocks);
        attempted
    }
}

/// Consecutive snapshots and their following edits retain batched lighting.
/// Edits behind a paced snapshot remain queued with it until a later pump.
fn apply_batch(
    world: &mut impl ReplicationTarget,
    content: &ContentMap,
    chunks: &mut Vec<(ChunkPos, Vec<u8>)>,
    blocks: &mut Vec<(BlockPos, u16, u8, u16, u8)>,
) {
    if !chunks.is_empty() {
        world.insert_remote_chunks(
            chunks.iter().map(|(pos, bytes)| (*pos, bytes.as_slice())),
            content.blocks(),
        );
        chunks.clear();
    }
    if !blocks.is_empty() {
        world.apply_remote_block_states(
            blocks
                .drain(..)
                .map(|(pos, id, meta, salt, soil)| (pos, content.block(id), meta, salt, soil)),
        );
    }
}
