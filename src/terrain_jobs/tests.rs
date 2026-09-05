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
            return result;
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
    assert!(!world.adopt_prepared(position, result.chunk, true));
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
