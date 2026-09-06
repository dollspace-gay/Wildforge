//! Shared cold-terrain preparation, independent of GPU and wire delivery.
//!
//! Workers only load or generate chunks. Callers commit prepared results on
//! the authoritative thread through `World::adopt_prepared_at_revision`.

mod policy;
mod queue;
mod result;
#[cfg(test)]
mod tests;
mod workers;

pub(crate) use policy::WorkerPolicy;
pub(crate) use queue::Priority;
pub(crate) use result::{ChunkOrigin, PreparedChunk, TerrainFailure};

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
use result::{Completion, GenerationId};

/// Immutable inputs shared by every worker in one world session.
#[derive(Clone)]
pub(crate) struct TerrainContext {
    seed: u32,
    registry: Arc<Registry>,
    atlas: Option<Arc<PlanetAtlas>>,
    loader: ChunkLoader,
}

impl TerrainContext {
    pub(crate) fn new(seed: u32, atlas: Option<Arc<PlanetAtlas>>, loader: ChunkLoader) -> Self {
        Self {
            seed,
            registry: Arc::clone(loader.registry()),
            atlas,
            loader,
        }
    }

    pub(crate) fn matches(&self, other: &Self) -> bool {
        self.seed == other.seed
            && self.loader.matches(&other.loader)
            && match (&self.atlas, &other.atlas) {
                (Some(left), Some(right)) => Arc::ptr_eq(left, right),
                (None, None) => true,
                _ => false,
            }
    }
}

/// Owns terrain requests and completions for one world session.
pub(crate) struct TerrainJobs {
    context: TerrainContext,
    queue: Arc<(Mutex<WorkQueue>, Condvar)>,
    ready: Receiver<Completion>,
    generation: GenerationId,
    workers: Vec<JoinHandle<()>>,
    failures: HashMap<ChunkPos, TerrainFailure>,
    failure_reported: bool,
    closed: bool,
}

impl TerrainJobs {
    pub(crate) fn new(context: TerrainContext, policy: WorkerPolicy) -> io::Result<Self> {
        Self::with_spawner(context, policy, workers::spawn)
    }

    fn with_spawner(
        context: TerrainContext,
        policy: WorkerPolicy,
        mut spawn: impl FnMut(String, workers::Task) -> io::Result<JoinHandle<()>>,
    ) -> io::Result<Self> {
        let (mut jobs, sender) = Self::empty(context, WorkQueue::default());
        // Construct the owner before spawning: if spawning unwinds, Drop
        // still stops and joins every worker that was already started.
        workers::start(
            &jobs.context,
            policy,
            &jobs.queue,
            &sender,
            &jobs.generation,
            &mut jobs.workers,
            &mut spawn,
        )?;
        Ok(jobs)
    }

    fn empty(
        context: TerrainContext,
        queue: WorkQueue,
    ) -> (Self, std::sync::mpsc::Sender<Completion>) {
        let queue = Arc::new((Mutex::new(queue), Condvar::new()));
        let (sender, ready) = channel();
        let jobs = Self {
            context,
            queue,
            ready,
            generation: GenerationId::new(),
            workers: Vec::new(),
            failures: HashMap::new(),
            failure_reported: false,
            closed: false,
        };
        (jobs, sender)
    }

    pub(crate) fn context(&self) -> &TerrainContext {
        &self.context
    }

    /// Join the obsolete pool before starting the new one. A failed restart
    /// retains the requested context and its error, so callers do not retry
    /// every frame or fall back to synchronous generation.
    pub(crate) fn reconfigure(&mut self, context: TerrainContext, policy: WorkerPolicy) {
        self.reconfigure_with_spawner(context, policy, workers::spawn);
    }

    fn reconfigure_with_spawner(
        &mut self,
        context: TerrainContext,
        policy: WorkerPolicy,
        spawn: impl FnMut(String, workers::Task) -> io::Result<JoinHandle<()>>,
    ) {
        if self.context.matches(&context) {
            return;
        }
        if let Err(error) = self.shutdown() {
            eprintln!("terrain context retired: {error}");
        }
        match Self::with_spawner(context.clone(), policy, spawn) {
            Ok(jobs) => *self = jobs,
            Err(error) => {
                let mut queue = WorkQueue::default();
                queue.fail(error);
                *self = Self::empty(context, queue).0;
            }
        }
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

    /// A worker-wide failure stops this session's queue and remains observable.
    pub(crate) fn fatal_failure(&self) -> Option<Arc<io::Error>> {
        match self.queue.0.lock() {
            Ok(queue) => queue.failure(),
            Err(poisoned) => {
                let mut queue = poisoned.into_inner();
                queue.fail(io::Error::other("terrain work queue poisoned"));
                self.queue.1.notify_all();
                queue.failure()
            }
        }
    }

    /// UI notification is one-shot; failure state itself remains retained.
    pub(crate) fn take_failure_notification(&mut self) -> Option<Arc<io::Error>> {
        if self.failure_reported {
            return None;
        }
        let failure = self.fatal_failure()?;
        self.failure_reported = true;
        Some(failure)
    }

    /// Stop accepting work and cancel pending requests before joining workers.
    pub(crate) fn stop(&self) {
        let (lock, wake) = &*self.queue;
        lock.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stop();
        wake.notify_all();
    }

    #[cfg(test)]
    pub(crate) fn panic_worker_for_test(&mut self) {
        let queue = Arc::clone(&self.queue);
        let (finished, done) = std::sync::mpsc::channel();
        self.workers.push(std::thread::spawn(move || {
            workers::supervise(&queue, || panic!("injected terrain worker failure"));
            finished.send(()).unwrap();
        }));
        done.recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
    }

    /// Cancel queued work, wait for running preparation, and discard results.
    ///
    /// Returns a worker failure only after all handles have been joined. The
    /// world and its persistence state are never mutated during shutdown.
    pub(crate) fn shutdown(&mut self) -> io::Result<()> {
        if self.closed {
            return Ok(());
        }
        self.stop();
        let poisoned = self.queue.0.is_poisoned();
        let mut panicked = 0;
        for worker in self.workers.drain(..) {
            if worker.join().is_err() {
                panicked += 1;
            }
        }
        while self.ready.try_recv().is_ok() {}
        self.closed = true;
        if poisoned || panicked != 0 {
            return Err(io::Error::other(format!(
                "terrain shutdown: {panicked} worker panics; queue poisoned: {poisoned}"
            )));
        }
        if let Some(failure) = self.fatal_failure() {
            return Err(io::Error::other(format!(
                "terrain shutdown after worker failure: {failure}"
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
