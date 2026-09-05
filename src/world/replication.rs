//! Incoming replica mutations shared by guest protocol adapters.

use crate::chunk::ChunkPos;
use crate::planet::BlockPos;
use crate::registry::BlockId;
use super::{BlockEntity, ReplicaObservations, TerrainRead, World};

/// The bounded incoming surface used by GuestSession. World implements this
/// temporarily while the graphical adapter moves to ReplicaWorld; read-only
/// consumers depend on TerrainRead and never receive this mutation capability.
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

impl ReplicationTarget for World {
    fn observations_mut(&mut self) -> &mut ReplicaObservations {
        debug_assert!(self.is_remote(), "observations require a guest replica");
        &mut self.replica_observations
    }

    fn receive_clock(&mut self, ire: f32, day: u32) {
        debug_assert!(self.is_remote(), "replica time requires a guest");
        self.ire = ire;
        self.day = day;
    }

    fn receive_block_entity(&mut self, position: BlockPos, entity: BlockEntity) {
        debug_assert!(self.is_remote(), "replicated block entities require a guest");
        self.insert_block_entity_at(position, entity);
    }

    fn insert_remote_chunks<'a>(
        &mut self,
        chunks: impl IntoIterator<Item = (ChunkPos, &'a [u8])>,
        remap: &[BlockId],
    ) {
        debug_assert!(self.is_remote(), "guest terrain needs a replica");
        World::insert_remote_chunks(self, chunks, remap);
    }

    fn apply_remote_block_states(
        &mut self,
        updates: impl IntoIterator<Item = (BlockPos, BlockId, u8, u16, u8)>,
    ) {
        World::apply_remote_block_states(self, updates);
    }
}
