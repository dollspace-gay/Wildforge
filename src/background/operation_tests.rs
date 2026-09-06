//! Handshakes exercise completion, cancellation races, and joined failure paths.

use super::{Operation, OperationUpdate};
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(10);

fn finished<P, T>(operation: &mut Operation<P, T>) -> io::Result<T> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(OperationUpdate::Finished(result)) = operation.poll() {
            return result;
        }
        assert!(Instant::now() < deadline, "operation did not finish");
        std::thread::yield_now();
    }
}

#[test]
fn progress_is_coalesced_and_terminal_result_is_delivered_once() {
    let (sent, received) = channel();
    let (release, resume) = channel();
    let mut operation = Operation::spawn(
        "progress",
        || {},
        move |progress| {
            for value in 0..10_000 {
                progress.publish(value)?;
            }
            sent.send(()).unwrap();
            resume.recv_timeout(TIMEOUT).unwrap();
            Ok(42)
        },
    )
    .unwrap();
    received.recv_timeout(TIMEOUT).unwrap();
    assert!(matches!(
        operation.poll(),
        Some(OperationUpdate::Progress(9_999))
    ));
    assert!(operation.poll().is_none());
    release.send(()).unwrap();
    assert_eq!(finished(&mut operation).unwrap(), 42);
    assert!(operation.poll().is_none());
    operation.shutdown().unwrap();
}

#[test]
fn worker_errors_and_panics_are_terminal_observable_results() {
    let mut operation = Operation::<(), ()>::spawn(
        "read",
        || {},
        |_| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "fixture read denied",
            ))
        },
    )
    .unwrap();
    let error = finished(&mut operation).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(error.to_string(), "fixture read denied");
    assert!(operation.poll().is_none());

    let mut operation =
        Operation::<(), ()>::spawn("entry", || {}, |_| panic!("fixture preparation panic"))
            .unwrap();
    assert_eq!(
        finished(&mut operation).unwrap_err().to_string(),
        "entry worker panicked: fixture preparation panic"
    );
    assert!(operation.poll().is_none());
}

#[test]
fn shutdown_and_drop_cancel_once_and_wait_for_worker_exit() {
    for explicit in [false, true] {
        let (cancel, cancelled) = channel();
        let (release, resume) = channel();
        let exited = Arc::new(AtomicBool::new(false));
        let worker_exited = Arc::clone(&exited);
        let mut operation = Operation::<(), ()>::spawn(
            "join",
            move || {
                cancel.send(()).unwrap();
            },
            move |_| {
                resume.recv_timeout(TIMEOUT).unwrap();
                worker_exited.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
        .unwrap();
        let (done, completed) = channel();
        let owner = std::thread::spawn(move || {
            if explicit {
                operation.shutdown().unwrap();
                operation.shutdown().unwrap();
            }
            drop(operation);
            done.send(()).unwrap();
        });
        cancelled.recv_timeout(TIMEOUT).unwrap();
        assert!(!exited.load(Ordering::SeqCst));
        assert!(completed.try_recv().is_err());
        release.send(()).unwrap();
        completed.recv_timeout(TIMEOUT).unwrap();
        owner.join().unwrap();
        assert!(exited.load(Ordering::SeqCst));
        assert!(cancelled.try_recv().is_err());
    }
}

#[test]
fn successful_publication_is_not_rewritten_as_cancellation() {
    let mut operation = Operation::<(), u32>::spawn("published", || {}, |_| Ok(17)).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while !operation.worker.as_ref().unwrap().is_finished() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    operation.cancel();
    operation.cancel();
    assert!(operation.cancellation_requested());
    assert_eq!(finished(&mut operation).unwrap(), 17);
}

#[test]
fn poisoned_progress_stops_and_joins_the_worker() {
    let (release, resume) = channel();
    let mut operation = Operation::<(), ()>::spawn(
        "poison",
        move || {
            release.send(()).unwrap();
        },
        move |_| {
            resume.recv_timeout(TIMEOUT).unwrap();
            Ok(())
        },
    )
    .unwrap();
    let progress = Arc::clone(&operation.progress);
    assert!(
        std::thread::spawn(move || {
            let _guard = progress.lock().unwrap();
            panic!("fixture poison");
        })
        .join()
        .is_err()
    );
    assert_eq!(
        finished(&mut operation).unwrap_err().to_string(),
        "operation progress poisoned"
    );
    assert!(operation.worker.is_none());
}
