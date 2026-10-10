//! Exact optical input, inactive RGB, and invalid-identity solver scenarios.

use super::*;
use crate::chunk::{CHUNK_CELLS, Chunk};
use crate::planet::Face;
use crate::registry::{AIR, BlockId};
use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

fn registry() -> Arc<Registry> {
    Arc::new(crate::registry::load(Path::new(
        "/nonexistent-light-palette-mods",
    )))
}

fn position() -> ChunkPos {
    ChunkPos::new(Face::PosZ, 256, 256).unwrap()
}

fn full_field(chunk: &Chunk) -> Vec<([u8; 3], u8)> {
    let mut result = Vec::with_capacity(CHUNK_CELLS);
    for x in 0..CHUNK_X {
        for z in 0..CHUNK_Z {
            for y in 0..CHUNK_Y {
                result.push(chunk.light(x, y, z));
            }
        }
    }
    result
}

#[test]
fn inactive_rgb_clears_stale_color_but_level_one_emitters_and_level_two_borders_survive() {
    let mut registry = registry();
    let pos = position();
    let neighbor = pos.offset(-1, 0);
    let mut store = TerrainStore::default();
    let mut target = Chunk::new();
    target.set_light(&vec![[7, 9, 11]; CHUNK_CELLS], &vec![5; CHUNK_CELLS]);
    target.dirty = false;
    let mut adjacent = Chunk::new();
    adjacent.set_light(&vec![[1; 3]; CHUNK_CELLS], &vec![0; CHUNK_CELLS]);
    store.resident.insert(pos, target);
    store.resident.insert(neighbor, adjacent);

    // The registry contains brighter lights, but this resident chunk has no
    // emitters and every border color is 1. The original seed rejects all 1s.
    assert!(store.relight_chunk(&registry, pos));
    let chunk = store.resident.get_mut(&pos).unwrap();
    assert!(chunk.dirty && !chunk.modified);
    assert!(full_field(chunk).iter().all(|light| *light == ([0; 3], 15)));
    chunk.dirty = false;
    assert!(!store.relight_chunk(&registry, pos));
    assert!(!store.resident.get(&pos).unwrap().dirty);

    // Level 1 is a real emitter even though it cannot propagate. Keep the
    // same Arc identity while changing its definitions between solver calls.
    let torch = registry.block_id("base:torch").unwrap();
    let definition = &mut Arc::get_mut(&mut registry).unwrap().blocks[usize::from(torch.0)];
    definition.light_emit = 1;
    definition.light_rgb = [0, 1, 0];
    store.resident.get_mut(&pos).unwrap().set(8, 100, 8, torch);
    assert!(store.relight_chunk(&registry, pos));
    let chunk = store.resident.get(&pos).unwrap();
    for x in 0..CHUNK_X {
        for z in 0..CHUNK_Z {
            for y in 0..CHUNK_Y {
                let rgb = if (x, y, z) == (8, 100, 8) {
                    [0, 1, 0]
                } else {
                    [0; 3]
                };
                assert_eq!(chunk.light(x, y, z), (rgb, 15));
            }
        }
    }

    // A single active color in current border input also forbids the skip.
    store.resident.get_mut(&pos).unwrap().set(8, 100, 8, AIR);
    store
        .resident
        .get_mut(&neighbor)
        .unwrap()
        .set_light(&vec![[0, 0, 2]; CHUNK_CELLS], &vec![15; CHUNK_CELLS]);
    assert!(store.relight_chunk(&registry, pos));
    let chunk = store.resident.get(&pos).unwrap();
    for x in 0..CHUNK_X {
        for z in 0..CHUNK_Z {
            for y in 0..CHUNK_Y {
                assert_eq!(
                    chunk.light(x, y, z),
                    (if x == 0 { [0, 0, 1] } else { [0; 3] }, 15)
                );
            }
        }
    }
}

#[test]
fn fresh_cascades_observe_in_place_emission_filter_water_and_opacity_changes() {
    let mut registry = registry();
    let stone = registry.block_id("base:stone").unwrap();
    let torch = registry.block_id("base:torch").unwrap();
    let glass = registry.block_id("base:red_glass").unwrap();
    let water = registry.water_ids[0];
    let definitions = &mut Arc::get_mut(&mut registry).unwrap().blocks;
    definitions[usize::from(torch.0)].light_emit = 6;
    definitions[usize::from(torch.0)].light_rgb = [6, 4, 2];
    let pos = position();
    let mut store = TerrainStore::default();
    let mut chunk = Chunk::new();
    chunk.raw_mut().fill(stone.0);
    for x in 3..=8 {
        chunk.set(x, 100, 8, AIR);
    }
    chunk.set(3, 100, 8, torch);
    chunk.set(4, 100, 8, glass);
    chunk.set(5, 100, 8, water);
    store.resident.insert(pos, chunk);

    let check = |store: &TerrainStore, corridor: [[u8; 3]; 6]| {
        let chunk = store.resident.get(&pos).unwrap();
        for x in 0..CHUNK_X {
            for z in 0..CHUNK_Z {
                for y in 0..CHUNK_Y {
                    let rgb = if (3..=8).contains(&x) && (y, z) == (100, 8) {
                        corridor[x - 3]
                    } else {
                        [0; 3]
                    };
                    assert_eq!(chunk.light(x, y, z), (rgb, 0), "{x},{y},{z}");
                }
            }
        }
        assert!(!chunk.modified);
    };
    store.relight_chunks_and_cascade(&registry, [pos]);
    check(
        &store,
        [
            [6, 4, 2],
            [5, 0, 0],
            [3, 0, 0],
            [2, 0, 0],
            [1, 0, 0],
            [0; 3],
        ],
    );

    let definitions = &mut Arc::get_mut(&mut registry).unwrap().blocks;
    definitions[usize::from(torch.0)].light_rgb = [0, 6, 0];
    definitions[usize::from(glass.0)].light_filter = [false, true, true];
    definitions[usize::from(water.0)].water_level = None;
    store.relight_chunks_and_cascade(&registry, [pos]);
    check(
        &store,
        [
            [0, 6, 0],
            [0, 5, 0],
            [0, 4, 0],
            [0, 3, 0],
            [0, 2, 0],
            [0, 1, 0],
        ],
    );

    Arc::get_mut(&mut registry).unwrap().blocks[usize::from(glass.0)].opaque = true;
    store.relight_chunks_and_cascade(&registry, [pos]);
    check(&store, [[0, 6, 0], [0; 3], [0; 3], [0; 3], [0; 3], [0; 3]]);
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        payload
            .downcast_ref::<&str>()
            .expect("bounds panic has a text payload")
            .to_string()
    }
}

#[test]
fn invalid_numeric_identity_keeps_the_registry_bounds_panic_without_resident_writes() {
    let registry = registry();
    let invalid = BlockId(u16::MAX);
    let expected =
        panic_message(catch_unwind(AssertUnwindSafe(|| registry.block(invalid))).unwrap_err());
    assert!(expected.contains("index out of bounds"));
    let pos = position();
    let mut store = TerrainStore::default();
    let mut chunk = Chunk::new();
    chunk.set(4, 100, 5, invalid);
    chunk.set_light(&vec![[7, 5, 3]; CHUNK_CELLS], &vec![11; CHUNK_CELLS]);
    chunk.dirty = false;
    chunk.modified = true;
    let before = full_field(&chunk);
    store.resident.insert(pos, chunk);
    let actual =
        catch_unwind(AssertUnwindSafe(|| store.relight_chunk(&registry, pos))).unwrap_err();
    assert_eq!(panic_message(actual), expected);
    let actual = catch_unwind(AssertUnwindSafe(|| {
        store.relight_chunks_and_cascade(&registry, [pos])
    }))
    .unwrap_err();
    assert_eq!(panic_message(actual), expected);
    let chunk = store.resident.get(&pos).unwrap();
    assert_eq!(full_field(chunk), before);
    assert_eq!(chunk.get(4, 100, 5), invalid);
    assert!(!chunk.dirty && chunk.modified);
}

#[test]
fn cache_budget_fallback_preserves_complete_fields_and_fresh_air_definitions() {
    let mut registry = registry();
    let positions = [position(), position().offset(2, 0)];
    let mut store = TerrainStore::default();
    for pos in positions {
        let mut chunk = Chunk::new();
        chunk.set_light(&vec![[7, 5, 3]; CHUNK_CELLS], &vec![4; CHUNK_CELLS]);
        chunk.dirty = false;
        store.resident.insert(pos, chunk);
    }

    let palette = cell_palette(&registry);
    let limit =
        LocalInput::new(store.resident.get(&positions[0]).unwrap(), &palette).allocated_bytes();
    // Exercise the exact-capacity admission boundary without retaining 64 MiB
    // in this focused scenario. The second equally sized input must overflow.
    let mut cache = InputCache::new(limit);
    for (index, pos) in positions.into_iter().enumerate() {
        let prepared = cache.prepare(pos, store.resident.get(&pos).unwrap(), &palette);
        assert_eq!(matches!(&prepared, input::Prepared::Cached(_)), index == 0);
        assert!(store.relight_chunk_with_input(prepared.as_ref(), pos));
        assert!(prepared.as_ref().initial_sky.iter().all(|&sky| sky == 15));
        assert!(prepared.as_ref().frontier.is_empty());
        let chunk = store.resident.get(&pos).unwrap();
        assert!(full_field(chunk).iter().all(|light| *light == ([0; 3], 15)));
        assert!(chunk.dirty && !chunk.modified);
    }
    assert_eq!(cache.cached_bytes(), limit);
    for (index, pos) in positions.into_iter().enumerate() {
        let prepared = cache.prepare(pos, store.resident.get(&pos).unwrap(), &palette);
        assert_eq!(matches!(&prepared, input::Prepared::Cached(_)), index == 0);
        assert!(!store.relight_chunk_with_input(prepared.as_ref(), pos));
    }
    assert_eq!(cache.cached_bytes(), limit);
    drop(cache);

    // A new cascade must prepare from the same Arc's current definitions,
    // rather than retaining the previous all-air grid across calls.
    Arc::get_mut(&mut registry).unwrap().blocks[usize::from(AIR.0)].opaque = true;
    store.relight_chunks_and_cascade(&registry, positions);
    for pos in positions {
        let chunk = store.resident.get(&pos).unwrap();
        assert!(full_field(chunk).iter().all(|light| *light == ([0; 3], 0)));
        assert!(chunk.dirty && !chunk.modified);
    }
}

#[test]
fn inactive_rgb_clears_final_cell_dense_blue_and_preserves_modified_flag() {
    let registry = registry();
    let pos = position();
    for modified in [false, true] {
        let mut store = TerrainStore::default();
        let mut chunk = Chunk::new();
        chunk.set_light(&vec![[0; 3]; CHUNK_CELLS], &vec![15; CHUNK_CELLS]);
        chunk.light_block_raw_mut()[CHUNK_CELLS - 1] = [0, 0, 1];
        chunk.dirty = false;
        chunk.modified = modified;
        assert_eq!(chunk.heap_bytes(), CHUNK_CELLS * 3);
        store.resident.insert(pos, chunk);

        assert!(store.relight_chunk(&registry, pos));
        let chunk = store.resident.get(&pos).unwrap();
        assert!(full_field(chunk).iter().all(|light| *light == ([0; 3], 15)));
        assert!(chunk.dirty);
        assert_eq!(chunk.modified, modified);
        assert_eq!(chunk.heap_bytes(), 0);
    }
}

#[test]
fn inactive_rgb_keeps_matching_dense_planes_and_flags() {
    let registry = registry();
    let pos = position();
    for (dirty, modified) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut store = TerrainStore::default();
        let mut chunk = Chunk::new();
        chunk.light_block_raw_mut().fill([0; 3]);
        chunk.light_sky_raw_mut().fill(15);
        chunk.dirty = dirty;
        chunk.modified = modified;
        let heap = chunk.heap_bytes();
        assert_eq!(heap, CHUNK_CELLS * 4);
        store.resident.insert(pos, chunk);

        assert!(!store.relight_chunk(&registry, pos));
        let chunk = store.resident.get(&pos).unwrap();
        assert!(full_field(chunk).iter().all(|light| *light == ([0; 3], 15)));
        assert_eq!((chunk.dirty, chunk.modified), (dirty, modified));
        assert_eq!(chunk.heap_bytes(), heap);
    }
}

#[test]
fn uniform_block_light_replaces_nonuniform_sky_without_touching_flags() {
    let mut chunk = Chunk::new();
    chunk.dirty = false;
    chunk.modified = true;
    let sky: Vec<_> = (0..CHUNK_CELLS).map(|index| (index % 16) as u8).collect();
    assert!(chunk.replace_uniform_block_light([0; 3], &sky));
    assert!(!chunk.dirty && chunk.modified);
    assert_eq!(chunk.heap_bytes(), CHUNK_CELLS);
    for (index, light) in full_field(&chunk).into_iter().enumerate() {
        assert_eq!(light, ([0; 3], (index % 16) as u8));
    }

    assert!(!chunk.replace_uniform_block_light([0; 3], &sky));
    assert!(!chunk.dirty && chunk.modified);
    assert_eq!(chunk.heap_bytes(), CHUNK_CELLS);
    for (index, light) in full_field(&chunk).into_iter().enumerate() {
        assert_eq!(light, ([0; 3], (index % 16) as u8));
    }
}
