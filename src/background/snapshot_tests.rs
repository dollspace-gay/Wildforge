//! Actual processor and ownership scenarios, controlled by explicit handshakes.

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::SnapshotJobs;

fn receive(pool: &mut SnapshotJobs<u32, u32>) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(value) = pool.try_ready() {
            return value;
        }
        assert!(Instant::now() < deadline, "snapshot did not complete");
        std::thread::yield_now();
    }
}

#[test]
fn fifo_cancellation_and_budget_cover_queued_running_and_ready_work() {
    let (release, resume) = channel();
    let worker_release = Mutex::new(resume);
    let (started_tx, started) = channel();
    let mut pool = SnapshotJobs::new("fifo", 1, 3, move |value| {
        if value == 1 {
            started_tx.send(()).unwrap();
            worker_release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
        }
        value * 2
    })
    .unwrap();
    assert!(pool.request(1));
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(pool.request(2));
    assert!(pool.request(3));
    assert!(!pool.request(4));
    assert_eq!(pool.cancel_queued(|value| *value != 2), [2]);
    assert_eq!(pool.pending_count(), 2);
    assert!(pool.request(4));
    release.send(()).unwrap();
    assert_eq!(receive(&mut pool), 2);
    assert_eq!(pool.pending_count(), 2);
    assert_eq!(receive(&mut pool), 6);
    assert_eq!(receive(&mut pool), 8);
    assert_eq!(pool.pending_count(), 0);
    pool.shutdown().unwrap();
}

#[test]
fn shutdown_and_drop_cancel_queued_inputs_and_join_the_running_processor() {
    for explicit in [false, true] {
        let (release, resume) = channel();
        let worker_release = Mutex::new(resume);
        let count = Arc::new(AtomicUsize::new(0));
        let processed = Arc::clone(&count);
        let (started_tx, started) = channel();
        let mut pool = SnapshotJobs::new("shutdown", 1, 2, move |value: u32| {
            started_tx.send(()).unwrap();
            worker_release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
            processed.fetch_add(1, Ordering::SeqCst);
            value
        })
        .unwrap();
        assert!(pool.request(1));
        started.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(pool.request(2));
        let queue = Arc::clone(&pool.queue);
        let (done_tx, done) = channel();
        let owner = std::thread::spawn(move || {
            if explicit {
                pool.shutdown().unwrap();
                assert!(pool.workers.is_empty());
                assert!(pool.try_ready().is_none());
                assert!(!pool.request(3));
                assert_eq!(pool.pending_count(), 0);
                pool.shutdown().unwrap();
            } else {
                drop(pool);
            }
            done_tx.send(()).unwrap();
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while !queue.0.lock().unwrap().stopped {
            assert!(Instant::now() < deadline, "shutdown did not begin");
            std::thread::yield_now();
        }
        assert!(done.try_recv().is_err(), "running processor was detached");
        release.send(()).unwrap();
        done.recv_timeout(Duration::from_secs(10)).unwrap();
        owner.join().unwrap();
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "cancelled input was processed"
        );
    }
}

#[test]
fn a_processor_panic_stops_the_pool_and_preserves_its_cause() {
    let mut pool = SnapshotJobs::new("panic-probe", 2, 2, |_: u32| -> u32 {
        panic!("broken immutable processor");
    })
    .unwrap();
    assert!(pool.request(1));
    let deadline = Instant::now() + Duration::from_secs(10);
    while pool.failure().is_none() {
        assert!(Instant::now() < deadline, "processor panic was lost");
        std::thread::yield_now();
    }
    let error = pool.failure().unwrap();
    assert!(
        error
            .to_string()
            .contains("panic-probe worker panicked: broken immutable processor")
    );
    assert!(!pool.request(2));
    assert_eq!(pool.pending_count(), 0);
    assert!(pool.try_ready().is_none());
    assert!(pool.shutdown().is_err());
    assert!(pool.workers.is_empty());
    pool.shutdown().unwrap();
}

#[test]
fn partial_startup_failure_joins_the_first_worker_before_returning() {
    let exited = Arc::new(AtomicUsize::new(0));
    let mut count = 0;
    let result = SnapshotJobs::with_spawner(
        "startup",
        2,
        2,
        |value: u32| value,
        |name, task| {
            count += 1;
            if count == 2 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "injected startup refusal",
                ));
            }
            let exited = Arc::clone(&exited);
            crate::background::spawn(
                name,
                Box::new(move || {
                    task();
                    exited.fetch_add(1, Ordering::SeqCst);
                }),
            )
        },
    );
    assert_eq!(
        result.err().unwrap().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(count, 2);
    assert_eq!(exited.load(Ordering::SeqCst), 1);
}

#[test]
fn poisoned_state_stops_waiters_and_is_reported_without_running_more_work() {
    let mut pool = SnapshotJobs::new("poison", 2, 2, |value: u32| value).unwrap();
    let queue = Arc::clone(&pool.queue);
    std::thread::spawn(move || {
        let _guard = queue.0.lock().unwrap();
        panic!("injected snapshot queue poison");
    })
    .join()
    .unwrap_err();
    assert!(pool.failure().is_some());
    assert!(!pool.request(1));
    assert!(pool.shutdown().is_err());
    assert!(pool.workers.is_empty());
}

#[test]
fn zero_workers_or_capacity_are_rejected_at_startup() {
    for (workers, limit) in [(0, 1), (1, 0)] {
        let result = SnapshotJobs::new("invalid", workers, limit, |value: u32| value);
        assert_eq!(result.err().unwrap().kind(), io::ErrorKind::InvalidInput);
    }
}
