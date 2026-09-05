//! Exercise real saved/generated results from both caller policies.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::{ChunkOrigin, PreparedChunk, Priority, TerrainContext, TerrainJobs, WorkerPolicy};
use crate::chunk::ChunkPos;
use crate::planet::{BlockPos, Face};
use crate::registry;
use crate::world::World;

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "wildforge-terrain-jobs-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn jobs(world: &World, policy: WorkerPolicy) -> TerrainJobs {
    TerrainJobs::new(
        TerrainContext::new(
            world.seed,
            Arc::clone(&world.reg),
            world.planet_atlas(),
            world.chunk_loader(),
        ),
        policy,
    )
}

fn receive(jobs: &mut TerrainJobs) -> PreparedChunk {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(result) = jobs.try_ready() {
            return result.unwrap();
        }
        assert!(
            Instant::now() < deadline,
            "terrain worker failed to complete"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn both_policies_prepare_the_same_generated_terrain_and_preserve_later_edits() {
    let root = TestDirectory::new();
    let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    let mut world = World::new(42, root.0.clone(), reg);
    let position = ChunkPos::new(Face::PosZ, 257, 257).unwrap();
    let expected = world.generator.generate(position, &world.reg);
    for policy in [WorkerPolicy::Interactive, WorkerPolicy::Dedicated] {
        let mut pool = jobs(&world, policy);
        pool.request(position, Priority::Ordinary, 2);
        pool.request(position, Priority::Entry, 2);
        assert_eq!(pool.pending_count(), 1, "duplicate callers share one job");
        let result = receive(&mut pool);
        assert_eq!(result.origin, ChunkOrigin::Generated);
        assert_eq!(result.chunk.raw(), expected.raw());
        assert_eq!(pool.pending_count(), 0);
    }
    let mut pool = jobs(&world, WorkerPolicy::Dedicated);
    pool.request(position, Priority::Entry, 2);
    let result = receive(&mut pool);
    world.ensure_chunk(position);
    let edit = BlockPos::new(Face::PosZ, 257 * 16 + 4, 200, 257 * 16 + 4).unwrap();
    let stone = world.reg.block_id("base:stone").unwrap();
    world.set_block_at(edit, stone);
    assert!(!world.adopt_prepared_at_revision(position, result.chunk, true, &result.revision));
    assert_eq!(
        world.get_block_at(edit),
        stone,
        "late terrain must not erase the edit"
    );
}

#[test]
fn workers_return_saved_terrain_with_saved_provenance() {
    let root = TestDirectory::new();
    let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    let mut world = World::new(42, root.0.clone(), reg);
    let position = ChunkPos::new(Face::PosZ, 256, 256).unwrap();
    world.ensure_chunk(position);
    let edit = BlockPos::new(Face::PosZ, 4098, 200, 4098).unwrap();
    let stone = world.reg.block_id("base:stone").unwrap();
    world.set_block_at(edit, stone);
    let report = world.save_modified();
    assert!(report.is_ok(), "{}", report.summary());
    for policy in [WorkerPolicy::Interactive, WorkerPolicy::Dedicated] {
        let mut pool = jobs(&world, policy);
        pool.request(position, Priority::Ordinary, 2);
        let result = receive(&mut pool);
        assert_eq!(result.origin, ChunkOrigin::Saved);
        assert_eq!(result.chunk.get(2, 200, 2), stone);
    }
}

#[test]
fn saved_and_generated_results_cannot_erase_edits_after_save_and_unload() {
    let root = TestDirectory::new();
    let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    let position = ChunkPos::new(Face::PosZ, 257, 257).unwrap();
    let edit = BlockPos::new(Face::PosZ, 257 * 16 + 4, 200, 257 * 16 + 4).unwrap();
    for (index, policy) in [WorkerPolicy::Interactive, WorkerPolicy::Dedicated]
        .into_iter()
        .enumerate()
    {
        for saved in [false, true] {
            let mut world = World::new(
                42,
                root.0.join(format!("late-{index}-{saved}")),
                Arc::clone(&reg),
            );
            if saved {
                assert!(world.ensure_chunk(position));
                world.set_block_at(edit, reg.block_id("base:stone").unwrap());
            }
            assert!(world.save_modified().is_ok());
            world.unload_chunk(position);
            let mut pool = jobs(&world, policy);
            pool.request(position, Priority::Entry, 2);
            let prepared = receive(&mut pool);
            assert_eq!(prepared.is_fresh(), !saved);
            assert!(world.ensure_chunk(position));
            let planks = reg.block_id("base:planks").unwrap();
            world.set_block_at(edit, planks);
            let (report, released) = world.evict_chunks(vec![position]);
            assert!(report.failures.is_empty());
            assert_eq!(released, [position]);
            assert!(!world.has_chunk(position));
            let fresh = prepared.is_fresh();
            assert!(!world.adopt_prepared_at_revision(
                position,
                prepared.chunk,
                fresh,
                &prepared.revision,
            ));
            assert!(
                !world.has_chunk(position),
                "stale result must have no adoption effects"
            );
            assert_eq!(pool.pending_count(), 0);
            pool.request(position, Priority::Ordinary, 2);
            let latest = receive(&mut pool);
            assert_eq!(latest.origin, ChunkOrigin::Saved);
            assert_eq!(latest.chunk.get(4, 200, 4), planks);
            assert!(world.adopt_prepared_at_revision(
                position,
                latest.chunk,
                false,
                &latest.revision,
            ));
            assert_eq!(world.get_block_at(edit), planks);
        }
    }
}

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
fn authoritative_adoption_preserves_material_and_water_accounting_exactly_once() {
    let root = TestDirectory::new();
    let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    let atlas = Arc::new(crate::planet_atlas::PlanetAtlas::fixture(42, 8).unwrap());
    let position = ChunkPos::new(Face::PosZ, 257, 257).unwrap();
    for (index, policy) in [WorkerPolicy::Interactive, WorkerPolicy::Dedicated]
        .into_iter()
        .enumerate()
    {
        let mut synchronous = World::new_with_atlas(
            42,
            root.0.join(format!("sync-{index}")),
            Arc::clone(&reg),
            Arc::clone(&atlas),
        );
        let mut asynchronous = World::new_with_atlas(
            42,
            root.0.join(format!("worker-{index}")),
            Arc::clone(&reg),
            Arc::clone(&atlas),
        );
        let mut pool = jobs(&asynchronous, policy);
        pool.request(position, Priority::Ordinary, 2);
        let result = receive(&mut pool);
        let duplicate = result.chunk.clone();
        assert!(result.is_fresh());
        assert!(asynchronous.adopt_prepared_at_revision(
            position,
            result.chunk,
            true,
            &result.revision,
        ));
        assert!(synchronous.ensure_chunk(position));
        let materials = asynchronous.material_ledger.as_ref().unwrap().audit();
        let water = asynchronous.live_water_audit().unwrap();
        assert!(materials.is_balanced());
        assert_eq!(water.unexplained_water_delta_hu, 0);
        assert_eq!(water.unexplained_salt_delta, 0);
        assert_eq!(
            materials,
            synchronous.material_ledger.as_ref().unwrap().audit()
        );
        assert_eq!(Some(water), synchronous.live_water_audit());
        assert!(!asynchronous.adopt_prepared_at_revision(
            position,
            duplicate,
            true,
            &result.revision,
        ));
        assert_eq!(
            materials,
            asynchronous.material_ledger.as_ref().unwrap().audit()
        );
        assert_eq!(Some(water), asynchronous.live_water_audit());
    }
}
