//! Native thread boundaries shared by specialized background work owners.

mod operation;
mod snapshots;
pub(crate) use operation::{Operation, OperationUpdate, Progress};
pub(crate) use snapshots::SnapshotJobs;

use std::io;
use std::thread::JoinHandle;

/// Boxed at thread creation to allow local, deterministic startup-failure probes.
pub(crate) type Task = Box<dyn FnOnce() + Send + 'static>;

pub(crate) fn spawn(name: String, task: Task) -> io::Result<JoinHandle<()>> {
    std::thread::Builder::new().name(name).spawn(task)
}

/// Stop an owner after a worker panic instead of silently losing a worker.
///
/// Callers must discard this worker's state after failure. The unwind boundary
/// is for immutable preparation tasks, never authoritative simulation mutation.
pub(crate) fn run_guarded(name: &str, task: impl FnOnce() -> io::Result<()>) -> io::Result<()> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(task)) {
        Ok(result) => result,
        Err(payload) => Err(panic_error(name, payload.as_ref())),
    }
}

fn panic_error(name: &str, payload: &(dyn std::any::Any + Send)) -> io::Error {
    let detail = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic payload");
    io::Error::other(format!("{name} worker panicked: {detail}"))
}
