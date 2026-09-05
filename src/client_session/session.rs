//! One owner resets protocol interpretation and queued work at world boundaries.

use std::sync::Arc;
use std::time::Instant;

use super::admission::{Admission, AdmissionError, PresentationRequirement};
use super::palette::ContentMap;
use super::snapshots::Snapshots;
use super::terrain::TerrainInbox;
use crate::chunk::ChunkPos;
use crate::planet::{BlockPos, EntityPos};
use crate::registry::{BlockId, Registry};
use crate::world::World;

pub(crate) struct GuestSession {
    content: ContentMap,
    snapshots: Snapshots,
    admission: Admission,
    terrain: TerrainInbox,
}

impl GuestSession {
    pub(crate) fn new(
        registry: Arc<Registry>,
        requirement: PresentationRequirement,
        now: Instant,
    ) -> Self {
        Self {
            content: ContentMap::empty(registry),
            snapshots: Snapshots::default(),
            admission: Admission::new(requirement, now),
            terrain: TerrainInbox::default(),
        }
    }

    /// Welcome replaces every receiver and queued mutation together. Nothing
    /// interpreted against the previous palette can reach the replacement world.
    pub(crate) fn begin(
        &mut self,
        content: ContentMap,
        world_name: String,
        spawn: EntityPos,
        now: Instant,
    ) {
        self.content = content;
        self.snapshots = Snapshots::default();
        self.terrain = TerrainInbox::default();
        self.admission.begin(world_name, spawn, now);
    }

    pub(crate) fn content(&self) -> &ContentMap {
        &self.content
    }

    pub(crate) fn rebind_content(&mut self, registry: Arc<Registry>) {
        self.content.rebind(registry);
    }

    pub(crate) fn snapshots(&mut self) -> &mut Snapshots {
        &mut self.snapshots
    }

    pub(crate) fn admission(&self) -> &Admission {
        &self.admission
    }

    pub(crate) fn manifest(
        &mut self,
        spawn: EntityPos,
        required: Vec<ChunkPos>,
        world: &World,
    ) -> Result<(), AdmissionError> {
        let result = self
            .admission
            .manifest(spawn, required, |pos| world.has_chunk(pos));
        if result.is_err() {
            self.close();
        }
        result
    }

    pub(crate) fn accepted(&mut self) -> Result<String, AdmissionError> {
        let result = self.admission.accepted();
        if result.is_err() {
            self.close();
        }
        result
    }

    pub(crate) fn frame_ready(&mut self) -> Result<(), AdmissionError> {
        let result = self.admission.frame_ready();
        if result.is_err() {
            self.close();
        }
        result
    }

    pub(crate) fn take_ready(&mut self) -> bool {
        self.admission.take_ready()
    }

    pub(crate) fn note_activity(&mut self, now: Instant) {
        self.admission.note_activity(now);
    }

    pub(crate) fn close(&mut self) {
        self.admission.close();
        self.terrain = TerrainInbox::default();
        self.snapshots = Snapshots::default();
    }

    pub(crate) fn has_queued_chunk(&self, position: ChunkPos) -> bool {
        self.terrain.contains(position)
    }

    pub(crate) fn queue_chunk(&mut self, position: ChunkPos, bytes: Vec<u8>) {
        if self.admission.receives_world() {
            self.terrain.chunk(position, bytes);
        }
    }

    pub(crate) fn queue_block(
        &mut self,
        position: BlockPos,
        wire_id: u16,
        metadata: u8,
        salt_mass: u16,
        soil_salinity: u8,
    ) -> BlockId {
        let local = self.content.block(wire_id);
        if self.admission.receives_world() {
            self.terrain
                .block(position, wire_id, metadata, salt_mass, soil_salinity);
        }
        local
    }

    pub(crate) fn apply_terrain(&mut self, world: &mut World, budget: usize) -> Vec<ChunkPos> {
        let attempted = self.terrain.apply(world, &self.content, budget);
        for &position in &attempted {
            if world.has_chunk(position) {
                self.admission.resident(position);
            }
        }
        attempted
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
