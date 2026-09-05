//! Session identity and provenance attached to every prepared terrain result.

use std::sync::Arc;

use crate::chunk::{Chunk, ChunkPos};

/// Allocation identity cannot be reused while any old completion retains it.
#[derive(Clone, Debug)]
pub(super) struct GenerationId(Arc<()>);

impl GenerationId {
    pub(super) fn new() -> Self {
        Self(Arc::new(()))
    }

    pub(super) fn matches(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// Whether preparation found a persisted chunk or generated new terrain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChunkOrigin {
    Saved,
    Generated,
}

/// A pure worker result awaiting an authoritative adoption decision.
pub(crate) struct PreparedChunk {
    pub(crate) position: ChunkPos,
    pub(crate) chunk: Chunk,
    pub(crate) origin: ChunkOrigin,
    pub(super) generation: GenerationId,
}

impl PreparedChunk {
    pub(crate) fn is_fresh(&self) -> bool {
        self.origin == ChunkOrigin::Generated
    }
}
