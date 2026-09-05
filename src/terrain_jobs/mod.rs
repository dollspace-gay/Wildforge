//! Shared cold-terrain preparation, independent of GPU and wire delivery.
//!
//! Workers only load or generate chunks. Callers commit prepared results on
//! the authoritative thread through `World::adopt_prepared`.

mod policy;
mod queue;
mod result;
#[cfg(test)]
mod tests;
mod workers;

pub(crate) use policy::WorkerPolicy;
pub(crate) use queue::Priority;
pub(crate) use result::{ChunkOrigin, PreparedChunk};

use std::collections::HashMap;
use std::io;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use crate::chunk::ChunkPos;
use crate::planet_atlas::PlanetAtlas;
use crate::registry::Registry;
use crate::world::ChunkLoader;
use queue::WorkQueue;
use result::{Completion, GenerationId, TerrainFailure};

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

/// Owns terrain requests and completions for one world session.
pub(crate) struct TerrainJobs {
    queue: Arc<(Mutex<WorkQueue>, Condvar)>,
    ready: Receiver<Completion>,
    generation: GenerationId,
    workers: Vec<JoinHandle<()>>,
    failures: HashMap<ChunkPos, TerrainFailure>,
}

impl TerrainJobs {
    pub(crate) fn new(context: TerrainContext, policy: WorkerPolicy) -> Self {
        let queue = Arc::new((Mutex::new(WorkQueue::default()), Condvar::new()));
        let (sender, ready) = channel();
        let mut jobs = Self {
            queue,
            ready,
            generation: GenerationId::new(),
            workers: Vec::new(),
            failures: HashMap::new(),
        };
        // Construct the owner before spawning: if spawning unwinds, Drop
        // still stops and joins every worker that was already started.
        workers::start(
            &context,
            policy,
            &jobs.queue,
            &sender,
            &jobs.generation,
            &mut jobs.workers,
        );
        jobs
    }

    /// Deduplicate requests and promote entry terrain within the supplied cap.
    pub(crate) fn request(&mut self, position: ChunkPos, priority: Priority, limit: usize) {
        if self.failures.contains_key(&position) {
            return;
        }
        let (lock, wake) = &*self.queue;
        if let Ok(mut queue) = lock.lock()
            && queue.request(position, priority, limit)
        {
            wake.notify_one();
        }
    }

    /// Remove obsolete queued work; running preparation remains pure.
    pub(crate) fn cancel_queued(&mut self, wanted: &impl Fn(ChunkPos) -> bool) {
        self.failures.retain(|position, _| wanted(*position));
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
    pub(crate) fn try_ready(&mut self) -> Option<Completion> {
        while let Ok(result) = self.ready.try_recv() {
            if let Some(result) = self.finish(result) {
                return Some(result);
            }
        }
        None
    }

    fn finish(&mut self, result: Completion) -> Option<Completion> {
        let (position, generation) = match &result {
            Ok(prepared) => (prepared.position, &prepared.generation),
            Err(failure) => (failure.position, &failure.generation),
        };
        if !generation.matches(&self.generation) {
            return None;
        }
        let (lock, _) = &*self.queue;
        let Ok(mut queue) = lock.lock() else {
            return None;
        };
        if queue.is_stopped() {
            return None;
        }
        queue.complete(position);
        if let Err(failure) = &result {
            self.failures.insert(position, failure.clone());
        }
        Some(result)
    }

    /// Failed positions stay suppressed while the caller retains interest in them.
    pub(crate) fn failures(&self) -> impl Iterator<Item = &TerrainFailure> {
        self.failures.values()
    }

    /// Cancel queued work, wait for running preparation, and discard results.
    ///
    /// Returns a worker failure only after all handles have been joined. The
    /// world and its persistence state are never mutated during shutdown.
    pub(crate) fn shutdown(&mut self) -> io::Result<()> {
        let (lock, wake) = &*self.queue;
        let poisoned = {
            let guard = lock.lock();
            let poisoned = guard.is_err();
            // Poisoned state is recovered only to stop and clear the queue;
            // no further terrain execution or adoption is allowed.
            let mut queue = guard.unwrap_or_else(std::sync::PoisonError::into_inner);
            queue.stop();
            wake.notify_all();
            poisoned
        };
        let mut panicked = 0;
        for worker in self.workers.drain(..) {
            if worker.join().is_err() {
                panicked += 1;
            }
        }
        while self.ready.try_recv().is_ok() {}
        if poisoned || panicked != 0 {
            return Err(io::Error::other(format!(
                "terrain shutdown: {panicked} worker panics; queue poisoned: {poisoned}"
            )));
        }
        Ok(())
    }
}

impl Drop for TerrainJobs {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            eprintln!("{error}");
        }
    }
}
