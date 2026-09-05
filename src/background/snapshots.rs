//! Bounded FIFO processing of immutable snapshots, including worker lifetimes.

use std::collections::VecDeque;
use std::io;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;

use super::{Task, run_guarded, spawn};

struct Queue<I> {
    requests: VecDeque<I>,
    pending: usize,
    stopped: bool,
    failure: Option<Arc<io::Error>>,
}

impl<I> Queue<I> {
    fn stop(&mut self) {
        self.stopped = true;
        self.requests.clear();
        self.pending = 0;
    }

    fn fail(&mut self, error: io::Error) {
        self.stop();
        self.failure.get_or_insert_with(|| Arc::new(error));
    }
}

type Shared<I> = Arc<(Mutex<Queue<I>>, Condvar)>;

fn state<I>(shared: &Shared<I>) -> MutexGuard<'_, Queue<I>> {
    match shared.0.lock() {
        Ok(state) => state,
        Err(poisoned) => {
            let mut state = poisoned.into_inner();
            state.fail(io::Error::other("snapshot work queue poisoned"));
            shared.1.notify_all();
            state
        }
    }
}

/// One session's FIFO snapshot workers; limits include queued/running/ready work.
pub(crate) struct SnapshotJobs<I, O> {
    name: &'static str,
    queue: Shared<I>,
    ready: Receiver<O>,
    workers: Vec<JoinHandle<()>>,
    limit: usize,
    closed: bool,
}

impl<I: Send + 'static, O: Send + 'static> SnapshotJobs<I, O> {
    pub(crate) fn new(
        name: &'static str,
        workers: usize,
        limit: usize,
        process: impl Fn(I) -> O + Send + Sync + 'static,
    ) -> io::Result<Self> {
        Self::with_spawner(name, workers, limit, process, spawn)
    }

    fn with_spawner(
        name: &'static str,
        workers: usize,
        limit: usize,
        process: impl Fn(I) -> O + Send + Sync + 'static,
        mut spawn: impl FnMut(String, Task) -> io::Result<JoinHandle<()>>,
    ) -> io::Result<Self> {
        if workers == 0 || limit == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "snapshot workers and limit must be positive",
            ));
        }
        let (send, ready) = channel();
        let mut owner = Self {
            name,
            queue: Arc::new((
                Mutex::new(Queue {
                    requests: VecDeque::new(),
                    pending: 0,
                    stopped: false,
                    failure: None,
                }),
                Condvar::new(),
            )),
            ready,
            workers: Vec::new(),
            limit,
            closed: false,
        };
        let process = Arc::new(process);
        for index in 0..workers {
            let queue = Arc::clone(&owner.queue);
            let send = send.clone();
            let process = Arc::clone(&process);
            owner.workers.push(spawn(
                format!("{name}-{index}"),
                Box::new(move || {
                    if let Err(error) = run_guarded(name, || run(&queue, send, &*process)) {
                        state(&queue).fail(error);
                        queue.1.notify_all();
                    }
                }),
            )?);
        }
        Ok(owner)
    }

    /// False means stopped or full; callers retain their normal retry policy.
    pub(crate) fn request(&mut self, input: I) -> bool {
        let mut state = state(&self.queue);
        if state.stopped || state.pending >= self.limit {
            return false;
        }
        state.requests.push_back(input);
        state.pending += 1;
        self.queue.1.notify_one();
        true
    }

    pub(crate) fn try_ready(&mut self) -> Option<O> {
        while let Ok(output) = self.ready.try_recv() {
            let mut state = state(&self.queue);
            if state.stopped {
                continue;
            }
            state.pending -= 1;
            return Some(output);
        }
        None
    }

    /// Return cancelled requests so adapters can release their deduplication keys.
    pub(crate) fn cancel_queued(&mut self, wanted: impl Fn(&I) -> bool) -> Vec<I> {
        let mut state = state(&self.queue);
        let mut cancelled = Vec::new();
        for input in std::mem::take(&mut state.requests) {
            if wanted(&input) {
                state.requests.push_back(input);
            } else {
                state.pending -= 1;
                cancelled.push(input);
            }
        }
        cancelled
    }

    pub(crate) fn pending_count(&self) -> usize {
        state(&self.queue).pending
    }
}

impl<I, O> SnapshotJobs<I, O> {
    pub(crate) fn failure(&self) -> Option<Arc<io::Error>> {
        state(&self.queue).failure.clone()
    }

    pub(crate) fn stop(&self) {
        state(&self.queue).stop();
        self.queue.1.notify_all();
    }

    /// Cancel queued work and wait for active processors before discarding output.
    pub(crate) fn shutdown(&mut self) -> io::Result<()> {
        if self.closed {
            return Ok(());
        }
        self.stop();
        let mut panics = 0;
        for worker in self.workers.drain(..) {
            if worker.join().is_err() {
                panics += 1;
            }
        }
        while self.ready.try_recv().is_ok() {}
        self.closed = true;
        if panics != 0 {
            return Err(io::Error::other(format!(
                "{} shutdown: {panics} worker panics",
                self.name
            )));
        }
        if let Some(error) = self.failure() {
            return Err(io::Error::other(format!(
                "{} shutdown after failure: {error}",
                self.name
            )));
        }
        Ok(())
    }
}

impl<I, O> Drop for SnapshotJobs<I, O> {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            eprintln!("{error}");
        }
    }
}

fn run<I, O>(queue: &Shared<I>, ready: Sender<O>, process: &impl Fn(I) -> O) -> io::Result<()> {
    loop {
        let input = {
            let mut state = state(queue);
            loop {
                if state.stopped {
                    return Ok(());
                }
                if let Some(input) = state.requests.pop_front() {
                    break input;
                }
                state = queue
                    .1
                    .wait(state)
                    .map_err(|_| io::Error::other("snapshot queue poisoned while waiting"))?;
            }
        };
        let output = process(input);
        let state = state(queue);
        if state.stopped || ready.send(output).is_err() {
            return Ok(());
        }
    }
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
