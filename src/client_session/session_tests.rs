//! Cross-owner lifetime and decoding contracts use real replica storage.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use super::GuestSession;
use crate::client_session::{ContentMap, PresentationRequirement};
use crate::net::Snapshot;
use crate::planet::{EntityPos, Face};
use crate::registry::{self, Registry};
use crate::world::World;

fn reg() -> Arc<Registry> {
    Arc::new(registry::load(Path::new("/nonexistent-mods-dir")))
}

fn spawn() -> EntityPos {
    EntityPos::new(Face::PosZ, 100.0, 80.0, 100.0).unwrap()
}

fn content(reg: &Arc<Registry>) -> ContentMap {
    ContentMap::new(
        Arc::clone(reg),
        reg.blocks.iter().map(|b| b.name.clone()).collect(),
        reg.items.iter().map(|i| i.name.clone()).collect(),
    )
}

fn session(reg: &Arc<Registry>, requirement: PresentationRequirement) -> GuestSession {
    let mut session = GuestSession::new(Arc::clone(reg), requirement, Instant::now());
    session.begin(content(reg), "fixture".into(), spawn(), Instant::now());
    session
}

fn replica(reg: &Arc<Registry>) -> World {
    // Remote storage never saves. No test creates or reads terrain at this path.
    let directory =
        std::env::temp_dir().join(format!("wildforge-session-fixture-{}", std::process::id()));
    assert!(!directory.exists());
    let mut world = World::new(42, directory, Arc::clone(reg));
    world.set_remote(true);
    world
}

fn chunk_bytes(reg: &Arc<Registry>) -> Vec<u8> {
    let mut source = replica(reg);
    let center = spawn().chunk().unwrap();
    source.insert_empty_chunks_for_test([center]);
    source.chunk_rle(center).unwrap()
}

#[test]
fn welcome_replaces_maps_receivers_and_both_kinds_of_queued_terrain() {
    let reg = reg();
    let mut session = session(&reg, PresentationRequirement::TerrainOnly);
    let center = spawn().chunk().unwrap();
    let block = spawn().block().unwrap();
    let stone = reg.block_id("base:stone").unwrap();
    session.queue_chunk(center, chunk_bytes(&reg));
    session.queue_block(block, stone.0, 0, 0, 0);
    assert!(session.has_queued_chunk(center));
    assert!(
        session
            .snapshots()
            .players(Snapshot::whole(90, vec![]))
            .is_some()
    );
    session.begin(
        ContentMap::empty(Arc::clone(&reg)),
        "replacement".into(),
        spawn(),
        Instant::now(),
    );
    let mut next_world = replica(&reg);
    next_world.insert_empty_chunks_for_test([center]);
    assert!(!session.has_queued_chunk(center));
    assert!(session.apply_terrain(&mut next_world, 8).is_empty());
    assert_eq!(next_world.get_block_at(block), registry::AIR);
    assert_eq!(session.content().item(stone.0), None);
    assert!(
        session
            .snapshots()
            .players(Snapshot::whole(0, vec![]))
            .is_some()
    );
    assert!(!session.take_ready());
}

#[test]
fn both_presentation_policies_keep_paced_decoding_and_real_residency() {
    let reg = reg();
    let center = spawn().chunk().unwrap();
    let neighbor = center.offset(1, 0);
    let bytes = chunk_bytes(&reg);
    for requirement in [
        PresentationRequirement::TerrainOnly,
        PresentationRequirement::FirstFrame,
    ] {
        let mut session = session(&reg, requirement);
        let mut world = replica(&reg);
        session
            .manifest(spawn(), vec![center, neighbor], &world)
            .unwrap();
        session.queue_chunk(center, bytes.clone());
        session.queue_chunk(neighbor, bytes.clone());
        assert!(session.apply_terrain(&mut world, 0).is_empty());
        assert_eq!(session.apply_terrain(&mut world, 1), vec![center]);
        assert!(world.has_chunk(center));
        assert!(!world.has_chunk(neighbor));
        assert!(session.has_queued_chunk(neighbor));
        assert!(!session.take_ready());
        assert_eq!(session.apply_terrain(&mut world, 1), vec![neighbor]);
        assert!(world.has_chunk(neighbor));
        if requirement == PresentationRequirement::FirstFrame {
            assert!(!session.take_ready());
            session.frame_ready().unwrap();
        }
        assert!(session.take_ready());
        assert!(!session.take_ready());
        assert_eq!(session.accepted().unwrap(), "fixture");
    }
}

#[test]
fn chunk_then_block_batch_preserves_the_later_authoritative_edit() {
    let reg = reg();
    let center = spawn().chunk().unwrap();
    let block = spawn().block().unwrap();
    let stone = reg.block_id("base:stone").unwrap();
    let mut session = session(&reg, PresentationRequirement::TerrainOnly);
    let mut world = replica(&reg);
    session.queue_chunk(center, chunk_bytes(&reg));
    session.queue_block(block, stone.0, 0, 0, 0);
    session.apply_terrain(&mut world, 2);
    assert_eq!(world.get_block_at(block), stone);
}

#[test]
fn content_reload_resolves_queued_wire_blocks_against_the_replacement_registry() {
    let reg = reg();
    let center = spawn().chunk().unwrap();
    let block = spawn().block().unwrap();
    let stone = reg.block_id("base:stone").unwrap();
    let dirt = reg.block_id("base:dirt").unwrap();
    let mut session = session(&reg, PresentationRequirement::TerrainOnly);
    session.queue_block(block, stone.0, 0, 0, 0);
    let mut changed = (*reg).clone();
    changed
        .blocks
        .swap(usize::from(stone.0), usize::from(dirt.0));
    changed.block_by_name.insert("base:stone".into(), dirt);
    changed.block_by_name.insert("base:dirt".into(), stone);
    let changed = Arc::new(changed);
    session.rebind_content(Arc::clone(&changed));
    let mut world = replica(&changed);
    world.insert_empty_chunks_for_test([center]);
    session.apply_terrain(&mut world, 2);
    assert_eq!(world.get_block_at(block), dirt);
    assert_eq!(changed.block(world.get_block_at(block)).name, "base:stone");
}

#[test]
fn invalid_manifest_discards_queued_mutations_and_later_chunks_cannot_reopen_it() {
    let reg = reg();
    let center = spawn().chunk().unwrap();
    let block = spawn().block().unwrap();
    let stone = reg.block_id("base:stone").unwrap();
    let mut session = session(&reg, PresentationRequirement::TerrainOnly);
    let mut world = replica(&reg);
    world.insert_empty_chunks_for_test([center]);
    session.queue_chunk(center, chunk_bytes(&reg));
    session.queue_block(block, stone.0, 0, 0, 0);
    assert!(session.manifest(spawn(), vec![], &world).is_err());
    session.queue_chunk(center, chunk_bytes(&reg));
    session.queue_block(block, stone.0, 0, 0, 0);
    assert!(session.apply_terrain(&mut world, 8).is_empty());
    assert_eq!(world.get_block_at(block), registry::AIR);
    assert!(!session.take_ready());
    assert!(session.admission().is_closed());
}

#[test]
fn malformed_payload_is_attempted_without_satisfying_entry() {
    let reg = reg();
    let center = spawn().chunk().unwrap();
    let mut session = session(&reg, PresentationRequirement::TerrainOnly);
    let mut world = replica(&reg);
    session.manifest(spawn(), vec![center], &world).unwrap();
    session.queue_chunk(center, b"WFC9".to_vec());
    assert_eq!(session.apply_terrain(&mut world, 2), vec![center]);
    assert!(!world.has_chunk(center));
    assert!(!session.take_ready());
}
