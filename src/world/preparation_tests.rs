//! Real generation parity, cancellation, and refused entry I/O scenarios.

use super::*;
use crate::planet::Face;
use crate::world::World;
use std::path::PathBuf;

fn registry() -> Arc<Registry> {
    Arc::new(crate::registry::load(std::path::Path::new(
        "/nonexistent-mods-dir",
    )))
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "wildforge-preparation-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn owned_trials_equal_direct_generation_and_remain_sorted() {
    let reg = registry();
    let atlas = Arc::new(PlanetAtlas::fixture(42, 8).unwrap());
    let generator = Generator::with_atlas(42, &reg, Arc::clone(&atlas));
    let positions = [
        ChunkPos::new(Face::PosZ, 257, 257).unwrap(),
        ChunkPos::new(Face::PosZ, 256, 256).unwrap(),
        ChunkPos::new(Face::PosZ, 257, 256).unwrap(),
    ];
    let mut completed = Vec::new();
    let generated = generate_trial_region(
        42,
        Arc::clone(&reg),
        atlas,
        &positions,
        &CancellationToken::default(),
        |done, total| completed.push((done, total)),
    )
    .unwrap();
    assert_eq!(completed, [(1, 3), (2, 3), (3, 3)]);
    assert!(generated.windows(2).all(|pair| pair[0].0 < pair[1].0));
    for (position, chunk) in generated {
        assert_eq!(chunk.raw(), generator.generate(position, &reg).raw());
    }
}

#[test]
fn cancellation_during_a_trial_discards_the_batch() {
    let cancel = CancellationToken::default();
    let positions: Vec<_> = (250..259)
        .map(|x| ChunkPos::new(Face::PosZ, x, 257).unwrap())
        .collect();
    let mut completed = 0;
    let error = generate_trial_region(
        42,
        registry(),
        Arc::new(PlanetAtlas::fixture(42, 8).unwrap()),
        &positions,
        &cancel,
        |_, _| {
            completed += 1;
            cancel.cancel();
        },
    )
    .err()
    .expect("cancelled batch must not return terrain");
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(completed, 1);
}

#[test]
fn already_cancelled_entry_does_not_publish_a_world_or_prepare_a_spawn() {
    let root = TestDirectory::new();
    let path = root.0.join("unpublished");
    let cancel = CancellationToken::default();
    cancel.cancel();
    let reg = registry();
    let error = World::load_or_create_cancellable(path.clone(), Arc::clone(&reg), &cancel)
        .err()
        .expect("cancelled creation must fail");
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert!(!path.exists());
    let mut world = World::new(42, path.clone(), reg);
    let error = world
        .prepare_common_spawn_cancellable(&cancel, |_, _, _| {
            panic!("cancelled preparation must not start")
        })
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert!(!path.exists());
}

#[test]
fn synchronous_entry_preserves_read_failure_and_does_not_generate_over_it() {
    let root = TestDirectory::new();
    let position = ChunkPos::new(Face::PosZ, 256, 256).unwrap();
    let path = crate::world::region::region_path(&root.0, position);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"corrupted entry region").unwrap();
    let mut world = World::new(42, root.0.clone(), registry());
    assert_eq!(
        world.try_ensure_chunk(position).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert!(!world.has_chunk(position));
    assert_eq!(std::fs::read(path).unwrap(), b"corrupted entry region");
}
