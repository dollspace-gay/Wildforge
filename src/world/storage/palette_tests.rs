//! Stored bindings survive registry changes without rewriting cold terrain.

use super::Palette;
use crate::chunk::{Chunk, ChunkPos};
use crate::planet::Face;
use crate::registry::{self, BlockId, Registry};
use crate::world::World;
use crate::world::storage::{ChunkRead, encode_chunk, encode_stream_chunk};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

static REGISTRY: LazyLock<Arc<Registry>> =
    LazyLock::new(|| Arc::new(registry::load(Path::new("/nonexistent-mods-dir"))));
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "wildforge-palette-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn changed_registry() -> Arc<Registry> {
    let mut reg = (**REGISTRY).clone();
    let stone = reg.block_id("base:stone").unwrap();
    let log = reg.block_id("base:log").unwrap();
    reg.blocks.swap(usize::from(stone.0), usize::from(log.0));
    reg.block_by_name.insert("base:stone".into(), log);
    reg.block_by_name.insert("base:log".into(), stone);
    let mut added = reg.blocks[0].clone();
    added.name = "fixture:added".into();
    let id = BlockId(reg.blocks.len() as u16);
    reg.blocks.push(added);
    reg.block_by_name.insert("fixture:added".into(), id);
    Arc::new(reg)
}

fn position(x: u16) -> ChunkPos {
    ChunkPos::new(Face::PosZ, 257 + x, 257).unwrap()
}

fn edited_chunk(reg: &Registry, added: bool) -> Chunk {
    let mut chunk = Chunk::new();
    chunk.set(0, 100, 0, reg.block_id("base:stone").unwrap());
    chunk.set(1, 100, 0, reg.block_id("base:log").unwrap());
    if added {
        chunk.set(2, 100, 0, reg.block_id("fixture:added").unwrap());
    }
    chunk.modified = true;
    chunk
}

fn read(world: &World, position: ChunkPos) -> Chunk {
    let ChunkRead::Present(chunk) = world.try_load_chunk(position).unwrap() else {
        panic!("saved edited chunk was not decoded");
    };
    chunk
}

#[test]
fn parser_preserves_sparse_ids_and_rejects_malformed_tables() {
    let palette = Palette::parse("0 base:air\n65535 removed:block\n")
        .unwrap()
        .unwrap();
    assert_eq!(palette.names.len(), 65536);
    assert!(palette.names[1].is_none());
    for bad in [
        "65536 block",
        "999999999999999999999999 block",
        "-1 block",
        "text block",
        "1",
        "0 base:air\n0 base:stone",
    ] {
        assert_eq!(
            Palette::parse(bad).unwrap_err().kind(),
            ErrorKind::InvalidData
        );
    }
    assert!(Palette::parse(" \n\t").unwrap().is_none());
}

#[test]
fn palette_read_is_bounded_and_preserves_io_errors() {
    let dir = Directory::new();
    let path = dir.0.join("palette");
    assert!(Palette::read(&path).unwrap().is_none());
    std::fs::File::create(&path)
        .unwrap()
        .set_len(super::MAX_BYTES + 1)
        .unwrap();
    assert_eq!(
        Palette::read(&path).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    std::fs::write(&path, [255]).unwrap();
    assert_eq!(
        Palette::read(&path).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(Palette::read(&path).is_err());
}

#[test]
fn a_full_palette_refuses_extension_without_partial_bindings() {
    let mut palette = Palette::parse("65535 retained:block\n").unwrap().unwrap();
    let before = palette.text();
    assert_eq!(
        palette.bind(&REGISTRY).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(palette.text(), before);
}

#[test]
fn reordering_and_extending_content_preserves_unloaded_chunks_and_wire_ids() {
    let dir = Directory::new();
    let mut world = World::new(42, dir.0.clone(), Arc::clone(&REGISTRY));
    for pos in [position(0), position(1)] {
        world.chunks.insert(pos, edited_chunk(&REGISTRY, false));
        world.save_chunk(pos).unwrap();
    }
    let original_palette = std::fs::read_to_string(dir.0.join("palette")).unwrap();
    let cold_before = world.region_store.read(position(1)).unwrap().0.unwrap();
    world.chunks.clear();
    let changed = changed_registry();
    world.reg = Arc::clone(&changed);
    world.remap_from(&REGISTRY);
    let chunk = edited_chunk(&changed, true);
    let wire_before = encode_stream_chunk(&chunk);
    world.chunks.insert(position(0), chunk);
    world.save_chunk(position(0)).unwrap();
    assert_eq!(
        encode_stream_chunk(world.chunk(position(0)).unwrap()),
        wire_before
    );
    let palette = std::fs::read_to_string(dir.0.join("palette")).unwrap();
    assert!(palette.starts_with(&original_palette));
    assert_eq!(
        world.region_store.read(position(1)).unwrap().0.unwrap(),
        cold_before
    );
    drop(world);

    let reopened = World::new(42, dir.0.clone(), changed.clone());
    for pos in [position(0), position(1)] {
        let chunk = read(&reopened, pos);
        assert_eq!(
            chunk.get(0, 100, 0),
            changed.block_id("base:stone").unwrap()
        );
        assert_eq!(chunk.get(1, 100, 0), changed.block_id("base:log").unwrap());
        assert!(
            !chunk.modified,
            "a registry reorder does not make saved bytes dirty"
        );
    }
    assert_eq!(
        read(&reopened, position(0)).get(2, 100, 0),
        changed.block_id("fixture:added").unwrap()
    );
    let stored = reopened.region_store.read(position(0)).unwrap().0.unwrap();
    assert!(stored.starts_with(b"WFC8"));
    assert!(wire_before.starts_with(b"WFC9"));
    assert_ne!(
        &stored[4..],
        &wire_before[4..stored.len()],
        "disk IDs differ from live wire IDs"
    );
}

#[test]
fn removal_and_restoration_keeps_an_unedited_stored_name() {
    let dir = Directory::new();
    let changed = changed_registry();
    let mut world = World::new(42, dir.0.clone(), Arc::clone(&changed));
    world
        .chunks
        .insert(position(0), edited_chunk(&changed, true));
    world.save_chunk(position(0)).unwrap();
    let before = std::fs::read(dir.0.join("palette")).unwrap();
    let removed = World::new(42, dir.0.clone(), Arc::clone(&REGISTRY));
    let unknown = read(&removed, position(0));
    assert_eq!(unknown.get(2, 100, 0), REGISTRY.unknown_block);
    assert!(!unknown.modified);
    removed.palette.publish(&removed.reg).unwrap();
    assert_eq!(std::fs::read(dir.0.join("palette")).unwrap(), before);
    let restored = World::new(42, dir.0.clone(), Arc::clone(&changed));
    assert_eq!(
        read(&restored, position(0)).get(2, 100, 0),
        changed.block_id("fixture:added").unwrap()
    );
}

#[test]
fn failed_direct_save_never_publishes_a_chunk_before_its_palette() {
    let dir = Directory::new();
    let mut world = World::new(42, dir.0.clone(), Arc::clone(&REGISTRY));
    world
        .chunks
        .insert(position(0), edited_chunk(&REGISTRY, false));
    std::fs::create_dir(dir.0.join("palette")).unwrap();
    assert!(world.save_chunk_if_modified(position(0)).is_err());
    assert!(world.region_store.read(position(0)).unwrap().0.is_none());
    std::fs::remove_dir(dir.0.join("palette")).unwrap();
    assert!(world.save_chunk_if_modified(position(0)).unwrap());
    assert_eq!(
        read(&world, position(0)).get(0, 100, 0),
        REGISTRY.block_id("base:stone").unwrap()
    );
}

#[test]
fn malformed_palette_is_not_permission_to_generate_or_replace_saved_data() {
    let dir = Directory::new();
    let malformed = b"70000 invalid:id\n";
    std::fs::write(dir.0.join("palette"), malformed).unwrap();
    assert_eq!(
        World::load_or_create(dir.0.clone(), Arc::clone(&REGISTRY))
            .err()
            .unwrap()
            .kind(),
        ErrorKind::InvalidData
    );
    assert!(
        !dir.0.join("world.toml").exists(),
        "entry refuses damage before publishing a world"
    );
    let mut world = World::new(42, dir.0.clone(), Arc::clone(&REGISTRY));
    assert_eq!(
        world.try_ensure_chunk(position(0)).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert!(world.chunk(position(0)).is_none());
    let save = world.save_modified();
    assert!(
        save.failures
            .iter()
            .any(|failure| failure.component == "block palette")
    );
    assert_eq!(std::fs::read(dir.0.join("palette")).unwrap(), malformed);
    assert!(world.region_store.read(position(0)).unwrap().0.is_none());
}

#[test]
fn invalid_runtime_ids_cannot_replace_an_existing_chunk() {
    let dir = Directory::new();
    let mut world = World::new(42, dir.0.clone(), Arc::clone(&REGISTRY));
    world
        .chunks
        .insert(position(0), edited_chunk(&REGISTRY, false));
    world.save_chunk(position(0)).unwrap();
    let before = world.region_store.read(position(0)).unwrap().0.unwrap();
    world
        .chunks
        .get_mut(&position(0))
        .unwrap()
        .set(0, 100, 0, BlockId(u16::MAX));
    assert_eq!(
        world.save_chunk(position(0)).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(
        world.region_store.read(position(0)).unwrap().0.unwrap(),
        before
    );
}

#[test]
fn identity_palette_preserves_existing_disk_bytes() {
    let dir = Directory::new();
    let mut world = World::new(42, dir.0.clone(), Arc::clone(&REGISTRY));
    let chunk = edited_chunk(&REGISTRY, false);
    let original = encode_chunk(&chunk);
    world.chunks.insert(position(0), chunk);
    world.save_chunk(position(0)).unwrap();
    assert_eq!(
        world.region_store.read(position(0)).unwrap().0.unwrap(),
        original
    );
}

#[test]
fn names_that_cannot_round_trip_are_rejected_before_extending_a_palette() {
    let mut reg = (**REGISTRY).clone();
    let mut palette = Palette::default();
    palette.bind(&reg).unwrap();
    let before = palette.text();
    for invalid in ["", "bad\nname", "bad\rname", " leading", "trailing "] {
        reg.blocks[0].name = invalid.into();
        assert_eq!(
            palette.bind(&reg).unwrap_err().kind(),
            ErrorKind::InvalidData
        );
        assert_eq!(palette.text(), before);
    }
}
