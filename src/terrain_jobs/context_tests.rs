//! Real worker results must retain the content and decoding context they used.

use super::{TestDirectory, jobs, receive};
use crate::chunk::ChunkPos;
use crate::planet::Face;
use crate::registry;
use crate::terrain_jobs::{ChunkOrigin, Priority, TerrainContext, WorkerPolicy};
use crate::world::World;
use std::io;
use std::path::Path;
use std::sync::Arc;

fn context(world: &World) -> TerrainContext {
    TerrainContext::new(world.seed, world.planet_atlas(), world.chunk_loader())
}

#[test]
fn first_palette_publication_invalidates_prepared_terrain_without_a_chunk_write() {
    let root = TestDirectory::new();
    let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    let mut world = World::new(42, root.0.clone(), reg);
    let position = ChunkPos::new(Face::PosZ, 257, 257).unwrap();
    let mut pool = jobs(&world, WorkerPolicy::Interactive);
    pool.request(position, Priority::Ordinary, 2);
    let old = receive(&mut pool);
    let save = world.save_modified();
    assert!(save.is_ok(), "{}", save.summary());
    assert!(!world.adopt_prepared_at_revision(position, old.chunk, true, &old.revision));
    pool.reconfigure(context(&world), WorkerPolicy::Interactive);
    pool.request(position, Priority::Ordinary, 2);
    let fresh = receive(&mut pool);
    assert!(world.adopt_prepared_at_revision(position, fresh.chunk, true, &fresh.revision));
}

#[test]
fn a_reader_created_before_the_first_save_keeps_edits_and_rejects_stale_adoption() {
    let root = TestDirectory::new();
    let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    let mut world = World::new(42, root.0.clone(), reg);
    let position = ChunkPos::new(Face::PosZ, 257, 257).unwrap();
    let mut pool = jobs(&world, WorkerPolicy::Dedicated);
    world.ensure_chunk(position);
    let edit = crate::planet::BlockPos::new(Face::PosZ, 257 * 16 + 2, 200, 257 * 16 + 2).unwrap();
    let stone = world.reg.block_id("base:stone").unwrap();
    let value = if world.get_block_at(edit) == stone {
        registry::AIR
    } else {
        stone
    };
    world.set_block_at(edit, value);
    let before = world.chunk(position).unwrap().raw().to_vec();
    let save = world.save_modified();
    assert!(save.is_ok(), "{}", save.summary());
    let (save, evicted) = world.evict_chunks(vec![position]);
    assert!(save.is_ok(), "{}", save.summary());
    assert_eq!(evicted, [position]);
    pool.request(position, Priority::Entry, 2);
    let old = receive(&mut pool);
    assert_eq!(old.origin, ChunkOrigin::Saved);
    assert_eq!(old.chunk.raw(), before);
    assert!(!world.adopt_prepared_at_revision(position, old.chunk, true, &old.revision));
    pool.reconfigure(context(&world), WorkerPolicy::Dedicated);
    pool.request(position, Priority::Entry, 2);
    let current = receive(&mut pool);
    assert_eq!(current.origin, ChunkOrigin::Saved);
    assert_eq!(current.chunk.raw(), before);
    assert!(world.adopt_prepared_at_revision(position, current.chunk, false, &current.revision));
}

#[test]
fn reload_rejects_prepared_ids_and_old_queue_completions() {
    let root = TestDirectory::new();
    let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    let mut world = World::new(42, root.0.clone(), Arc::clone(&reg));
    let position = ChunkPos::new(Face::PosZ, 257, 257).unwrap();
    let mut pool = jobs(&world, WorkerPolicy::Interactive);
    pool.request(position, Priority::Ordinary, 2);
    let old = receive(&mut pool);
    pool.request(position, Priority::Ordinary, 2);
    let late = receive(&mut pool);
    world.reg = Arc::new((*reg).clone());
    world.remap_from(&reg);
    assert!(!world.adopt_prepared_at_revision(position, old.chunk, true, &old.revision));
    pool.reconfigure(context(&world), WorkerPolicy::Interactive);
    pool.request(position, Priority::Ordinary, 2);
    assert!(pool.finish(Ok(late)).is_none());
    assert_eq!(
        pool.pending_count(),
        1,
        "an old result cannot release a current slot"
    );
    let current = receive(&mut pool);
    assert!(world.adopt_prepared_at_revision(position, current.chunk, true, &current.revision));
}

#[test]
fn a_failed_restart_is_retained_until_a_different_context_is_requested() {
    let root = TestDirectory::new();
    let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    let mut world = World::new(42, root.0.clone(), reg);
    let mut pool = jobs(&world, WorkerPolicy::Interactive);
    world.reg = Arc::new((*world.reg).clone());
    let requested = context(&world);
    let mut attempts = 0;
    for _ in 0..2 {
        pool.reconfigure_with_spawner(requested.clone(), WorkerPolicy::Interactive, |_, _| {
            attempts += 1;
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "fixture restart denied",
            ))
        });
    }
    assert_eq!(attempts, 1);
    assert!(pool.context().matches(&requested));
    assert!(pool.workers.is_empty());
    assert_eq!(
        pool.take_failure_notification().unwrap().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert!(pool.take_failure_notification().is_none());
    assert_eq!(pool.pending_count(), 0);
    world.reg = Arc::new((*world.reg).clone());
    pool.reconfigure(context(&world), WorkerPolicy::Interactive);
    assert!(pool.fatal_failure().is_none());
    let position = ChunkPos::new(Face::PosZ, 257, 257).unwrap();
    pool.request(position, Priority::Ordinary, 2);
    assert_eq!(receive(&mut pool).position, position);
}
