//! Context changes retire failures, pending encodings, and cached numeric IDs.

use super::{HostChunkState, TerrainContext};
use crate::chunk::{Chunk, ChunkPos};
use crate::planet::Face;
use crate::registry;
use crate::server::Server;
use crate::world::World;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn world() -> (Self, World) {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "wildforge-host-context-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
        let world = World::new(42, root.clone(), reg);
        (Self(root), world)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn context(world: &World) -> TerrainContext {
    TerrainContext::new(world.seed, world.planet_atlas(), world.chunk_loader())
}

fn encoded(
    state: &mut HostChunkState,
    server: &mut Server,
    position: ChunkPos,
    chunk: &Chunk,
) -> Arc<Vec<u8>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let jobs = state.as_mut().unwrap();
        jobs.drain_into(server, &|_| true);
        if let Some(bytes) = jobs.encoded_or_enqueue(position, chunk) {
            return bytes;
        }
        assert!(Instant::now() < deadline, "encoding was not delivered");
        std::thread::yield_now();
    }
}

#[test]
fn context_change_discards_cached_and_in_flight_encodings() {
    let (_root, world) = Fixture::world();
    let mut server = Server::new(world, 0.3, 1);
    let mut state = HostChunkState::default();
    state.ensure_context(context(&server.world));
    let position = ChunkPos::new(Face::PosZ, 256, 256).unwrap();
    let mut chunk = Chunk::new();
    chunk.set(1, 200, 1, server.world.reg.block_id("base:stone").unwrap());
    let old = encoded(&mut state, &mut server, position, &chunk);
    assert_eq!(*old, crate::world::encode_stream_chunk(&chunk));
    assert!(
        state
            .as_mut()
            .unwrap()
            .encoded_or_enqueue(position.offset(1, 0), &chunk)
            .is_none()
    );

    server.world.reg = Arc::new((*server.world.reg).clone());
    state.ensure_context(context(&server.world));
    let jobs = state.as_ref().unwrap();
    assert!(jobs.encoded_cache.is_empty());
    assert!(jobs.encoding.is_empty());
    assert!(jobs.revisions.is_empty());
    chunk.set(1, 200, 1, server.world.reg.block_id("base:log").unwrap());
    let current = encoded(&mut state, &mut server, position, &chunk);
    assert_eq!(*current, crate::world::encode_stream_chunk(&chunk));
    assert_ne!(current, old);
}

#[test]
fn startup_failure_stays_with_its_context_and_does_not_retry_every_pump() {
    let (_root, mut world) = Fixture::world();
    let mut state = HostChunkState::default();
    let mut attempts = 0;
    for _ in 0..3 {
        state.ensure_with(context(&world), |_| {
            attempts += 1;
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "fixture host startup denied",
            ))
        });
    }
    assert_eq!(attempts, 1);
    assert!(state.as_ref().is_none());
    assert_eq!(
        state.take_failure_notification().unwrap().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert!(state.take_failure_notification().is_none());
    assert!(state.failure().is_some());
    world.reg = Arc::new((*world.reg).clone());
    state.ensure_context(context(&world));
    assert!(state.as_ref().is_some());
    assert!(state.failure().is_none());
}

#[test]
fn worker_failure_is_retained_until_the_context_changes() {
    let (_root, mut world) = Fixture::world();
    let mut state = HostChunkState::default();
    state.ensure_context(context(&world));
    state.as_mut().unwrap().panic_worker_for_test();
    assert!(state.take_failure_notification().is_some());
    state.ensure_context(context(&world));
    assert!(state.failure().is_some());
    assert!(state.take_failure_notification().is_none());
    world.reg = Arc::new((*world.reg).clone());
    state.ensure_context(context(&world));
    assert!(state.failure().is_none());
}
