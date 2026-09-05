//! Resident chunk storage and derived spatial work, independent of authority.

use std::collections::HashMap;

use crate::chunk::{CHUNK_X, Chunk, ChunkPos};

mod lighting;
mod network;

/// Owns resident voxel planes, mesh dirtiness, and derived light. Generation,
/// persisted palettes, ledgers, and save policy belong to the authoritative
/// coordinator. Mutation access is confined to world-domain coordinators.
#[derive(Default)]
pub(super) struct TerrainStore {
    resident: HashMap<ChunkPos, Chunk>,
}

impl std::fmt::Debug for TerrainStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("TerrainStore")
            .field("resident_chunks", &self.resident.len()).finish()
    }
}

impl TerrainStore {
    pub(super) fn get(&self, position: &ChunkPos) -> Option<&Chunk> {
        self.resident.get(position)
    }

    pub(super) fn get_mut(&mut self, position: &ChunkPos) -> Option<&mut Chunk> {
        self.resident.get_mut(position)
    }

    pub(super) fn contains_key(&self, position: &ChunkPos) -> bool {
        self.resident.contains_key(position)
    }

    pub(super) fn insert(&mut self, position: ChunkPos, chunk: Chunk) -> Option<Chunk> {
        self.resident.insert(position, chunk)
    }

    pub(super) fn remove(&mut self, position: &ChunkPos) -> Option<Chunk> {
        self.resident.remove(position)
    }

    pub(super) fn len(&self) -> usize { self.resident.len() }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&ChunkPos, &Chunk)> {
        self.resident.iter()
    }

    pub(super) fn keys(&self) -> impl Iterator<Item = &ChunkPos> {
        self.resident.keys()
    }

    pub(super) fn values_mut(&mut self) -> impl Iterator<Item = &mut Chunk> {
        self.resident.values_mut()
    }

    pub(super) fn mark_dirty(&mut self, position: ChunkPos, dirty: bool) {
        if let Some(chunk) = self.resident.get_mut(&position) { chunk.dirty = dirty; }
    }

    pub(super) fn mark_all_dirty(&mut self) {
        for chunk in self.resident.values_mut() { chunk.dirty = true; }
    }

    pub(super) fn dirty_positions(&self) -> Vec<ChunkPos> {
        self.resident.iter().filter_map(|(position, chunk)| chunk.dirty.then_some(*position)).collect()
    }

    pub(super) fn outside(&self, centers: &[ChunkPos], radius: i32) -> Vec<ChunkPos> {
        self.resident.keys().filter(|position| {
            !centers.iter().any(|center| position.distance(*center) <= f64::from(radius * CHUNK_X as i32))
        }).copied().collect()
    }

    #[cfg(test)]
    pub(super) fn clear(&mut self) { self.resident.clear(); }

    #[cfg(test)]
    pub(super) fn test_map(&self) -> &HashMap<ChunkPos, Chunk> { &self.resident }

    #[cfg(test)]
    pub(super) fn test_map_mut(&mut self) -> &mut HashMap<ChunkPos, Chunk> { &mut self.resident }
}
