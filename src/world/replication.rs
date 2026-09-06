//! Incoming replica mutations shared by guest protocol adapters.

use super::{BlockEntity, ReplicaObservations, TerrainRead};
use crate::chunk::ChunkPos;
use crate::planet::BlockPos;
use crate::registry::BlockId;

/// The bounded incoming surface used by GuestSession. Only ReplicaWorld
/// implements it. Read-only consumers depend on TerrainRead and never receive
/// this mutation capability; an authoritative World cannot accept host snapshots.
pub(crate) trait ReplicationTarget: TerrainRead {
    fn observations_mut(&mut self) -> &mut ReplicaObservations;
    fn receive_clock(&mut self, ire: f32, day: u32);
    fn receive_block_entity(&mut self, position: BlockPos, entity: BlockEntity);
    fn insert_remote_chunks<'a>(
        &mut self,
        chunks: impl IntoIterator<Item = (ChunkPos, &'a [u8])>,
        remap: &[BlockId],
    );
    fn apply_remote_block_states(
        &mut self,
        updates: impl IntoIterator<Item = (BlockPos, BlockId, u8, u16, u8)>,
    );
}
