//! Shared cold-terrain preparation, independent of GPU and wire delivery.
//!
//! Workers only load or generate chunks. Callers commit prepared results on
//! the authoritative thread through `World::adopt_prepared`.

mod policy;
mod queue;
#[cfg(test)]
mod tests;
mod workers;

pub(crate) use policy::WorkerPolicy;
pub(crate) use queue::Priority;

use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Condvar, Mutex};

use crate::chunk::{Chunk, ChunkPos};
use crate::planet_atlas::PlanetAtlas;
use crate::registry::Registry;
use crate::world::ChunkLoader;
use queue::WorkQueue;

/// Immutable inputs shared by every worker in one world session.
#[derive(Clone)]
pub(crate) struct TerrainContext {
    seed: u32,
    registry: Arc<Registry>,
    atlas: Option<Arc<PlanetAtlas>>,
    loader: ChunkLoader,
}

impl TerrainContext {
    pub(crate) fn new(
        seed: u32,
        registry: Arc<Registry>,
        atlas: Option<Arc<PlanetAtlas>>,
        loader: ChunkLoader,
    ) -> Self {
        Self {
            seed,
            registry,
            atlas,
            loader,
        }
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
}

impl PreparedChunk {
    pub(crate) fn is_fresh(&self) -> bool {
        self.origin == ChunkOrigin::Generated
    }
}

/// Owns terrain requests and completions for one world session.
pub(crate) struct TerrainJobs {
    queue: Arc<(Mutex<WorkQueue>, Condvar)>,
    ready: Receiver<PreparedChunk>,
}

impl TerrainJobs {
    pub(crate) fn new(context: TerrainContext, policy: WorkerPolicy) -> Self {
        let queue = Arc::new((Mutex::new(WorkQueue::default()), Condvar::new()));
        let (sender, ready) = channel();
        workers::start(&context, policy, &queue, &sender);
        Self { queue, ready }
    }

    /// Deduplicate requests and promote entry terrain within the supplied cap.
    pub(crate) fn request(&mut self, position: ChunkPos, priority: Priority, limit: usize) {
        let (lock, wake) = &*self.queue;
        if let Ok(mut queue) = lock.lock()
            && queue.request(position, priority, limit)
        {
            wake.notify_one();
        }
    }

    /// Remove obsolete queued work; running preparation remains pure.
    pub(crate) fn cancel_queued(&mut self, wanted: &impl Fn(ChunkPos) -> bool) {
        let (lock, _) = &*self.queue;
        if let Ok(mut queue) = lock.lock() {
            queue.cancel_queued(wanted);
        }
    }

    /// Queued, running, and ready work still occupying the caller's budget.
    pub(crate) fn pending_count(&self) -> usize {
        let (lock, _) = &*self.queue;
        lock.lock().map(|queue| queue.pending_count()).unwrap_or(0)
    }

    /// Complete bookkeeping before returning a result to its adopting caller.
    pub(crate) fn try_ready(&mut self) -> Option<PreparedChunk> {
        let result = self.ready.try_recv().ok()?;
        let (lock, _) = &*self.queue;
        if let Ok(mut queue) = lock.lock() {
            queue.complete(result.position);
        }
        Some(result)
    }
}

impl Drop for TerrainJobs {
    fn drop(&mut self) {
        let (lock, wake) = &*self.queue;
        if let Ok(mut queue) = lock.lock() {
            queue.stop();
            wake.notify_all();
        }
    }
}
