//! Chunk I/O failures must survive worker, authoritative-save, and guest boundaries.

use std::io::ErrorKind;
use std::path::Path;
use std::time::{Duration, Instant};

use super::{base_reg, ep, tchunk, tmp_dir};
use crate::chunk::ChunkPos;
use crate::net::{Client, RefusalCode, S2C};
use crate::terrain_jobs::{Priority, TerrainContext, TerrainJobs, WorkerPolicy};
use crate::world::{ChunkRead, World, region};

fn pool(world: &World, policy: WorkerPolicy) -> TerrainJobs {
    TerrainJobs::new(
        TerrainContext::new(world.seed, world.planet_atlas(), world.chunk_loader()),
        policy,
    )
    .unwrap()
}

fn failure(pool: &mut TerrainJobs) -> (ChunkPos, ErrorKind) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match pool.try_ready() {
            Some(Err(failure)) => return (failure.position, failure.source.kind()),
            Some(Ok(_)) => panic!("unreadable terrain was replaced by generated content"),
            None => {}
        }
        assert!(
            Instant::now() < deadline,
            "terrain failure was never delivered"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn corrupt_header(root: &Path, position: ChunkPos) -> std::path::PathBuf {
    let path = region::region_path(root, position);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"damaged region header").unwrap();
    path
}

#[test]
fn missing_corrupt_and_unreadable_saves_remain_distinct_through_both_worker_policies() {
    for (index, policy) in [WorkerPolicy::Interactive, WorkerPolicy::Dedicated]
        .into_iter()
        .enumerate()
    {
        let root = tmp_dir(&format!("terrain-io-{index}"));
        let mut world = World::new(42, root.clone(), base_reg());
        let position = tchunk(0, 0);
        assert!(matches!(
            world.chunk_loader().load(position).unwrap(),
            ChunkRead::Missing
        ));
        let path = corrupt_header(&root, position);
        let before = std::fs::read(&path).unwrap();
        let mut jobs = pool(&world, policy);
        jobs.request(position, Priority::Entry, 2);
        assert_eq!(failure(&mut jobs), (position, ErrorKind::InvalidData));
        assert_eq!(jobs.pending_count(), 0);
        assert_eq!(jobs.failures().count(), 1);
        jobs.request(position, Priority::Ordinary, 2);
        assert_eq!(
            jobs.pending_count(),
            0,
            "failed interest cannot create an endless retry loop"
        );
        assert!(!world.ensure_chunk(position));
        let generated = world.generator.generate(position, &world.reg);
        assert!(!world.adopt_generated(position, generated));
        assert!(!world.has_chunk(position));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        jobs.shutdown().unwrap();

        // NotADirectory is reproducible even when tests run with root privileges.
        let other_root = root.join("unreadable");
        std::fs::create_dir_all(&other_root).unwrap();
        std::fs::write(other_root.join(position.face().name()), b"not a directory").unwrap();
        let other = World::new(42, other_root, base_reg());
        let expected = other.chunk_loader().load(position).err().unwrap().kind();
        assert_ne!(expected, ErrorKind::InvalidData);
        assert_ne!(expected, ErrorKind::NotFound);
        let mut jobs = pool(&other, policy);
        jobs.request(position, Priority::Entry, 2);
        assert_eq!(failure(&mut jobs), (position, expected));
    }
}

#[test]
fn failed_region_write_retains_dirty_edits_for_a_successful_retry() {
    let root = tmp_dir("terrain-dirty-retry");
    let mut world = World::new(42, root.clone(), base_reg());
    let position = tchunk(0, 0);
    world.ensure_chunk(position);
    let stone = world.reg.block_id("base:stone").unwrap();
    world.set_block(2, 200, 2, stone);
    assert!(world.save_modified().is_ok());
    let path = region::region_path(&root, position);
    let original = std::fs::read(&path).unwrap();
    let planks = world.reg.block_id("base:planks").unwrap();
    world.set_block(2, 200, 2, planks);
    std::fs::write(&path, b"damaged").unwrap();
    let failed = world.save_modified();
    assert!(!failed.is_ok());
    assert!(world.chunks()[&position].modified);
    assert_eq!(world.get_block(2, 200, 2), planks);
    assert_eq!(std::fs::read(&path).unwrap(), b"damaged");
    std::fs::write(&path, original).unwrap();
    let saved = world.save_modified();
    assert!(saved.is_ok(), "{}", saved.summary());
    assert!(!world.chunks()[&position].modified);
    let ChunkRead::Present(reloaded) = world.chunk_loader().load(position).unwrap() else {
        panic!("retried save did not yield stored terrain");
    };
    assert_eq!(reloaded.get(2, 200, 2), planks);
}

#[test]
fn host_refuses_entry_with_a_read_error_instead_of_waiting_for_missing_terrain() {
    refused_terrain("terrain-error-admission", false);
}

#[test]
fn host_refuses_entry_when_a_terrain_worker_panics() {
    refused_terrain("terrain-panic-admission", true);
}

fn refused_terrain(label: &str, worker_panic: bool) {
    let root = tmp_dir(label);
    let world = World::new(42, root.clone(), base_reg());
    let spawn = ep(glam::Vec3::new(
        8.5,
        world.surface_height(8, 8) as f32 + 1.0,
        8.5,
    ));
    let position = spawn.chunk().unwrap();
    let damaged = if worker_panic {
        None
    } else {
        let path = corrupt_header(&root, position);
        let original = std::fs::read(&path).unwrap();
        Some((path, original))
    };
    let mut sim = crate::server::Server::new(world, 0.3, 7);
    let mut session = crate::mp::HostSession::start_on("terrain-error".into(), 0).unwrap();
    session.fresh_spawn = Some(spawn);
    if worker_panic {
        session.panic_terrain_worker_for_test(&sim);
    }
    let identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir(&format!("{label}-client")))
            .unwrap();
    let address = format!("127.0.0.1:{}", session.net.port).parse().unwrap();
    let mut client = Client::connect(address, "Fern".into(), 0, 0, &identity, None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let refusal = loop {
        let events = session.pump(&mut sim, None, 0.05);
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, crate::mp::HostFx::Joined(_)))
        );
        if let Some(refusal) = client.poll().into_iter().find_map(|message| match message {
            S2C::Refused(refusal) => Some(refusal),
            S2C::EntryAccepted => panic!("guest entered an unreadable spawn region"),
            _ => None,
        }) {
            break refusal;
        }
        assert!(
            Instant::now() < deadline,
            "guest never received the terrain error"
        );
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(refusal.code, RefusalCode::Server);
    assert!(refusal.detail.contains("terrain"));
    assert!(!sim.world.has_chunk(position));
    if let Some((path, original)) = damaged {
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}
