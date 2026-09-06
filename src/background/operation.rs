//! One cancellable operation with coalesced progress and a joined terminal result.

use std::io;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// Intermediate status is bounded to its newest value; completion is never dropped.
pub(crate) struct Progress<P>(Arc<Mutex<Option<P>>>);

impl<P> Progress<P> {
    pub(crate) fn publish(&self, value: P) -> io::Result<()> {
        let previous = self
            .0
            .lock()
            .map_err(|_| io::Error::other("operation progress poisoned"))?
            .replace(value);
        // Do not run an arbitrary value's destructor while holding the lock.
        drop(previous);
        Ok(())
    }
}

pub(crate) enum OperationUpdate<P, T> {
    Progress(P),
    Finished(io::Result<T>),
}

/// The owner retains cancellation and joins the worker on completion or Drop.
pub(crate) struct Operation<P, T> {
    name: &'static str,
    progress: Arc<Mutex<Option<P>>>,
    worker: Option<JoinHandle<io::Result<T>>>,
    cancel: Box<dyn Fn() + Send>,
    cancelled: bool,
}

impl<P: Send + 'static, T: Send + 'static> Operation<P, T> {
    pub(crate) fn spawn(
        name: &'static str,
        cancel: impl Fn() + Send + 'static,
        work: impl FnOnce(Progress<P>) -> io::Result<T> + Send + 'static,
    ) -> io::Result<Self> {
        let progress = Arc::new(Mutex::new(None));
        let sender = Progress(Arc::clone(&progress));
        let worker = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || work(sender))?;
        Ok(Self {
            name,
            progress,
            worker: Some(worker),
            cancel: Box::new(cancel),
            cancelled: false,
        })
    }
}

impl<P, T> Operation<P, T> {
    pub(crate) fn cancel(&mut self) {
        if !self.cancelled {
            self.cancelled = true;
            (self.cancel)();
        }
    }

    pub(crate) fn cancellation_requested(&self) -> bool {
        self.cancelled
    }

    pub(crate) fn poll(&mut self) -> Option<OperationUpdate<P, T>> {
        self.worker.as_ref()?;
        let progress = self
            .progress
            .lock()
            .map(|mut progress| progress.take())
            .map_err(|_| io::Error::other("operation progress poisoned"));
        match progress {
            Ok(Some(progress)) => return Some(OperationUpdate::Progress(progress)),
            Ok(None) => {}
            Err(error) => {
                self.cancel();
                let joined = self.join();
                return Some(OperationUpdate::Finished(Err(joined
                    .err()
                    .unwrap_or(error))));
            }
        }
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            return Some(OperationUpdate::Finished(self.join()));
        }
        None
    }

    pub(crate) fn shutdown(&mut self) -> io::Result<()> {
        if self.worker.is_none() {
            return Ok(());
        }
        self.cancel();
        self.join().map(drop)
    }

    fn join(&mut self) -> io::Result<T> {
        let worker = self
            .worker
            .take()
            .ok_or_else(|| io::Error::other("operation already joined"))?;
        match worker.join() {
            Ok(result) => result,
            Err(payload) => Err(super::panic_error(self.name, payload.as_ref())),
        }
    }
}

impl<P, T> Drop for Operation<P, T> {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            eprintln!("{} stopped: {error}", self.name);
        }
    }
}

#[cfg(test)]
#[path = "operation_tests.rs"]
mod tests;
