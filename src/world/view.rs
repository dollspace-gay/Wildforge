//! Bounded scene observations over independent authoritative and replica owners.

use std::sync::Arc;

use crate::chunk::{Chunk, ChunkPos};
use crate::planet::BlockPos;
use crate::registry::Registry;
use super::{ReplicaWorld, SceneRead, TerrainRead, World};
use super::local_structure::LocalStructure;

mod environment;
mod entities;
mod machines;
mod items;

#[derive(Clone, Copy)]
enum Source<'a> {
    Authority(&'a World),
    Replica(&'a ReplicaWorld),
}

/// A scene can be inspected without obtaining the authoritative World, a
/// mutable replica, or a persistence/generation API. Its source stays private.
#[derive(Clone, Copy)]
pub(crate) struct WorldView<'a> {
    source: Source<'a>,
}

impl World {
    pub(crate) fn view(&self) -> WorldView<'_> { WorldView { source: Source::Authority(self) } }
}

impl ReplicaWorld {
    pub(crate) fn view(&self) -> WorldView<'_> { WorldView { source: Source::Replica(self) } }
}

impl WorldView<'_> {
    pub(crate) fn is_remote(&self) -> bool { matches!(self.source, Source::Replica(_)) }

    pub(crate) fn chunk_count(&self) -> usize {
        match self.source {
            Source::Authority(world) => world.chunk_count(),
            Source::Replica(world) => world.chunk_count(),
        }
    }

    pub(crate) fn dirty_chunks(&self) -> Vec<ChunkPos> {
        match self.source {
            Source::Authority(world) => world.dirty_chunks(),
            Source::Replica(world) => world.dirty_chunks(),
        }
    }

    pub(crate) fn chunks_outside_all(&self, centers: &[ChunkPos], radius: i32) -> Vec<ChunkPos> {
        match self.source {
            Source::Authority(world) => world.chunks_outside_all(centers, radius),
            Source::Replica(world) => world.chunks_outside_all(centers, radius),
        }
    }
}

impl TerrainRead for WorldView<'_> {
    fn registry(&self) -> &Arc<Registry> {
        match self.source {
            Source::Authority(world) => world.registry(),
            Source::Replica(world) => world.registry(),
        }
    }
    fn chunk(&self, position: ChunkPos) -> Option<&Chunk> {
        match self.source {
            Source::Authority(world) => world.chunk(position),
            Source::Replica(world) => world.chunk(position),
        }
    }
    fn is_hidden(&self, position: BlockPos) -> bool {
        match self.source {
            Source::Authority(world) => world.is_hidden(position),
            Source::Replica(world) => world.is_hidden(position),
        }
    }
    fn hidden_in_chunk(&self, position: ChunkPos) -> Vec<BlockPos> {
        match self.source {
            Source::Authority(world) => world.hidden_in_chunk(position),
            Source::Replica(world) => world.hidden_in_chunk(position),
        }
    }
}

impl SceneRead for WorldView<'_> {
    fn local_structures(&self) -> &[LocalStructure] { self.structures() }
}
