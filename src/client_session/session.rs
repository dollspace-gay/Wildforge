//! One owner resets protocol interpretation and queued work at world boundaries.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use super::admission::{Admission, AdmissionError, PresentationRequirement};
use super::palette::ContentMap;
use super::replica::EntitySnapshots;
use super::terrain::TerrainInbox;
use super::transfer::{self, TransferError};
use crate::chunk::ChunkPos;
use crate::entity::ItemEntity;
use crate::mobs::{Mob, Projectile};
use crate::net::{BoltSnap, FallSnap, LooseItemSnap, MobSnap, PlayerSnap, Snapshot};
use crate::planet::{BlockPos, EntityPos};
use crate::registry::{BlockId, Registry};
use crate::world::{FallingBlock, World};

pub(crate) struct GuestSession {
    content: ContentMap,
    entities: EntitySnapshots,
    admission: Admission,
    terrain: TerrainInbox,
    roster: HashMap<u32, crate::net::PlayerPresence>,
}

impl GuestSession {
    pub(crate) fn new(
        registry: Arc<Registry>,
        requirement: PresentationRequirement,
        now: Instant,
    ) -> Self {
        Self {
            content: ContentMap::empty(registry),
            entities: EntitySnapshots::default(),
            admission: Admission::new(requirement, now),
            terrain: TerrainInbox::default(),
            roster: HashMap::new(),
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
        self.entities = EntitySnapshots::default();
        self.terrain = TerrainInbox::default();
        self.roster.clear();
        self.admission.begin(world_name, spawn, now);
    }

    pub(crate) fn install_content(
        &mut self,
        cache: &std::path::Path,
        files: Vec<(String, Vec<u8>)>,
    ) -> Result<Arc<Registry>, TransferError> {
        let result = if self.admission.accepts_content() {
            transfer::install(cache, files)
        } else {
            Err(TransferError::UnexpectedPhase)
        };
        if result.is_err() {
            self.close();
        }
        result
    }

    pub(crate) fn set_roster(&mut self, players: Vec<crate::net::PlayerPresence>) {
        if self.admission.receives_world() {
            self.roster = players.into_iter().map(|presence| (presence.id, presence)).collect();
        }
    }

    pub(crate) fn roster(&self) -> &HashMap<u32, crate::net::PlayerPresence> {
        &self.roster
    }

    pub(crate) fn joined(&mut self, presence: crate::net::PlayerPresence) {
        if self.admission.receives_world() {
            self.roster.insert(presence.id, presence);
        }
    }

    pub(crate) fn left(&mut self, id: u32) -> Option<crate::net::PlayerPresence> {
        self.roster.remove(&id)
    }

    pub(crate) fn content(&self) -> &ContentMap {
        &self.content
    }

    pub(crate) fn rebind_content(&mut self, registry: Arc<Registry>) {
        self.content.rebind(registry);
    }

    fn entity_input(&mut self) -> Option<(&mut EntitySnapshots, &ContentMap)> {
        self.admission
            .receives_world()
            .then_some((&mut self.entities, &self.content))
    }

    pub(crate) fn players(&mut self, part: Snapshot<PlayerSnap>) -> Option<Vec<PlayerSnap>> {
        self.entity_input()?.0.players(part)
    }

    pub(crate) fn mobs(&mut self, part: Snapshot<MobSnap>) -> Option<Vec<Mob>> {
        let (entities, content) = self.entity_input()?;
        entities.mobs(part, content.registry())
    }

    pub(crate) fn bolts(&mut self, part: Snapshot<BoltSnap>) -> Option<Vec<Projectile>> {
        self.entity_input()?.0.bolts(part)
    }

    pub(crate) fn loose_items(&mut self, part: Snapshot<LooseItemSnap>) -> Option<Vec<ItemEntity>> {
        let (entities, content) = self.entity_input()?;
        entities.loose_items(part, content)
    }

    pub(crate) fn falling(&mut self, part: Snapshot<FallSnap>) -> Option<Vec<FallingBlock>> {
        let (entities, content) = self.entity_input()?;
        entities.falling(part, content)
    }

    /// Apply shared world-domain messages synchronously. The caller receives
    /// only messages that still need its presentation or control policy.
    pub(crate) fn apply_world_message(
        &self,
        message: crate::net::S2C,
        world: &mut World,
        time_of_day: &mut f32,
    ) -> Option<crate::net::S2C> {
        super::events::apply_world(message, world, time_of_day, self.admission.receives_world())
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
        self.roster.clear();
        self.entities = EntitySnapshots::default();
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
