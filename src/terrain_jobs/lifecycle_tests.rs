//! Session cancellation, shutdown, startup, and live worker failure scenarios.

use super::{TestDirectory, jobs, receive};
use crate::chunk::ChunkPos;
use crate::planet::Face;
use crate::registry;
use crate::terrain_jobs::{Priority, TerrainContext, TerrainJobs, WorkerPolicy};
use crate::world::World;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[test]
fn old_session_completion_cannot_release_or_replace_new_session_work() {
    let root = TestDirectory::new();
    let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    let world = World::new(42, root.0.clone(), reg);
    let position = ChunkPos::new(Face::PosZ, 256, 256).unwrap();
    let mut old = jobs(&world, WorkerPolicy::Dedicated);
    old.request(position, Priority::Ordinary, 2);
    let late = receive(&mut old);
    old.shutdown().unwrap();

    let mut current = jobs(&world, WorkerPolicy::Dedicated);
    current.request(position, Priority::Ordinary, 2);
    assert!(current.finish(Ok(late)).is_none());
    assert_eq!(
        current.pending_count(),
        1,
        "old work cannot free a new slot"
    );
    assert_eq!(receive(&mut current).position, position);
    assert_eq!(current.pending_count(), 0);
}

#[test]
fn shutdown_and_drop_wait_for_running_workers_then_reject_late_work() {
    use std::sync::mpsc::channel;

    let root = TestDirectory::new();
    let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    let world = World::new(42, root.0.clone(), reg);
    for explicit in [true, false] {
        let mut pool = jobs(&world, WorkerPolicy::Dedicated);
        let position = ChunkPos::new(Face::PosZ, 256, 256).unwrap();
        pool.request(position, Priority::Ordinary, 2);
        let late = receive(&mut pool);
        // An owned probe holds a worker in progress until explicitly released.
        // This proves joining, without relying on terrain generation speed.
        let (release_tx, release_rx) = channel();
        pool.workers.push(std::thread::spawn(move || {
            release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        }));
        let queue = Arc::clone(&pool.queue);
        let (done_tx, done_rx) = channel();
        let owner = std::thread::spawn(move || {
            if explicit {
                pool.shutdown().unwrap();
                assert!(pool.workers.is_empty());
                assert_eq!(pool.pending_count(), 0);
                assert!(pool.finish(Ok(late)).is_none());
                pool.request(position, Priority::Entry, 2);
                assert_eq!(pool.pending_count(), 0);
                assert!(pool.try_ready().is_none());
                pool.shutdown().unwrap();
            } else {
                drop(pool);
            }
            done_tx.send(()).unwrap();
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while !queue.0.lock().unwrap().is_stopped() {
            assert!(Instant::now() < deadline, "shutdown did not begin");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(done_rx.try_recv().is_err(), "running worker was detached");
        release_tx.send(()).unwrap();
        done_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        owner.join().unwrap();
    }
}

#[test]
fn shutdown_reports_worker_panics_after_joining_every_handle() {
    let root = TestDirectory::new();
    let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    let world = World::new(42, root.0.clone(), reg);
    let mut pool = jobs(&world, WorkerPolicy::Dedicated);
    pool.workers.push(std::thread::spawn(|| {
        panic!("injected terrain worker failure");
    }));
    let error = pool.shutdown().unwrap_err();
    assert!(error.to_string().contains("1 worker panics"));
    assert!(pool.workers.is_empty());
    assert_eq!(pool.pending_count(), 0);
    assert!(pool.try_ready().is_none());
    pool.shutdown().unwrap();
}

#[test]
fn partial_startup_failure_joins_every_worker_already_started() {
    let root = TestDirectory::new();
    let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    let world = World::new(42, root.0.clone(), reg);
    let exited = Arc::new(AtomicUsize::new(0));
    let mut count = 0;
    let result = TerrainJobs::with_spawner(
        TerrainContext::new(
            world.seed,
            Arc::clone(&world.reg),
            None,
            world.chunk_loader(),
        ),
        WorkerPolicy::Dedicated,
        |name, task| {
            count += 1;
            if count == 2 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "injected spawn refusal",
                ));
            }
            let exited = Arc::clone(&exited);
            crate::terrain_jobs::workers::spawn(
                name,
                Box::new(move || {
                    task();
                    exited.fetch_add(1, Ordering::SeqCst);
                }),
            )
        },
    );
    let error = result.err().expect("second worker startup must fail");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(count, 2);
    assert_eq!(exited.load(Ordering::SeqCst), 1, "startup leaked a worker");
}

#[test]
fn a_live_panic_stops_sibling_workers_and_notifies_the_caller_once() {
    let root = TestDirectory::new();
    let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    let world = World::new(42, root.0.clone(), reg);
    let mut pool = jobs(&world, WorkerPolicy::Dedicated);
    let position = ChunkPos::new(Face::PosZ, 256, 256).unwrap();
    pool.request(position, Priority::Entry, 2);
    let late = receive(&mut pool);
    let queue = Arc::clone(&pool.queue);
    pool.workers.push(std::thread::spawn(move || {
        crate::terrain_jobs::workers::supervise(&queue, || {
            panic!("injected supervised terrain panic");
        });
    }));
    let deadline = Instant::now() + Duration::from_secs(10);
    let failure = loop {
        if let Some(error) = pool.take_failure_notification() {
            break error;
        }
        assert!(Instant::now() < deadline, "live panic was not observable");
        std::thread::yield_now();
    };
    assert!(
        failure
            .to_string()
            .contains("injected supervised terrain panic")
    );
    assert!(pool.take_failure_notification().is_none());
    assert!(
        pool.fatal_failure().is_some(),
        "notification must not erase failure state"
    );
    pool.request(position, Priority::Entry, 2);
    assert_eq!(pool.pending_count(), 0);
    assert!(pool.finish(Ok(late)).is_none());
    assert!(pool.shutdown().is_err());
    assert!(pool.workers.is_empty());
    pool.shutdown().unwrap();
}

#[test]
fn a_poisoned_queue_is_visible_and_all_waiters_exit_on_shutdown() {
    let root = TestDirectory::new();
    let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    let world = World::new(42, root.0.clone(), reg);
    let mut pool = jobs(&world, WorkerPolicy::Dedicated);
    let queue = Arc::clone(&pool.queue);
    std::thread::spawn(move || {
        let _guard = queue.0.lock().unwrap();
        panic!("injected queue poison");
    })
    .join()
    .unwrap_err();
    assert!(pool.fatal_failure().is_some());
    assert!(pool.shutdown().is_err());
    assert!(pool.workers.is_empty());
    assert!(pool.try_ready().is_none());
    pool.shutdown().unwrap();
}
