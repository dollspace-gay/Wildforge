//! Existing spawn qualification contracts.

use super::atlas_selection::fresh_water_distances;
use super::*;
use crate::planet_atlas::AtlasPos;
use crate::{
    chunk::{CHUNK_X, CHUNK_Z},
    planet::FACE_BLOCKS,
    planet_atlas::{PlanetAtlas, WaterBodyKind},
    world::World,
};
use std::{collections::HashMap, sync::Arc};

#[test]
fn freshwater_distance_crosses_cube_face_edges() {
    let atlas = PlanetAtlas::fixture(8_101, 4).unwrap();
    let distances = fresh_water_distances(&atlas);
    assert_eq!(distances.len(), atlas.genesis.hydrology.len());
    for face in Face::ALL {
        for u in 0..atlas.side() {
            for v in [0, atlas.side() - 1] {
                let pos = AtlasPos { face, u, v };
                for neighbor in pos.neighbors4(atlas.side()) {
                    let a = distances[pos.index(atlas.side())];
                    let b = distances[neighbor.index(atlas.side())];
                    if a != u16::MAX && b != u16::MAX {
                        assert!(a.abs_diff(b) <= 1);
                    }
                }
            }
        }
    }
}

#[test]
fn atlas_free_world_has_the_canonical_fallback_doorstep() {
    let reg = Arc::new(crate::registry::load(std::path::Path::new("mods")));
    let world = World::new(99, std::path::PathBuf::new(), reg);
    let spawn = world.qualified_spawn_surface().unwrap();
    assert_eq!(spawn.face(), Face::PosZ);
    assert_eq!(spawn.u(), FACE_BLOCKS / 2);
    assert_eq!(spawn.v(), FACE_BLOCKS / 2);
}

#[test]
fn voxel_trial_requires_a_safe_walkable_resource_doorstep() {
    let reg = Arc::new(crate::registry::load(std::path::Path::new("mods")));
    let atlas = Arc::new(PlanetAtlas::fixture(1_337, 64).unwrap());
    let atlas_pos = atlas
        .genesis
        .hydrology
        .iter()
        .find(|(pos, water)| {
            water.water_body == WaterBodyKind::Land
                && pos.u > 2
                && pos.v > 2
                && pos.u + 3 < atlas.side()
                && pos.v + 3 < atlas.side()
        })
        .map(|(pos, _)| pos)
        .unwrap();
    let point = atlas_pos.center(atlas.side());
    let center = SurfacePos::new(point.face, point.u as u16, point.v as u16).unwrap();
    let stone = reg.block_id("base:stone").unwrap();
    let dirt = reg.block_id("base:dirt").unwrap();
    let grass = reg.block_id("base:grass").unwrap();
    let log = reg.block_id("base:log").unwrap();
    let bush = reg.block_id("base:berry_bush").unwrap();
    let water = reg.water_for_volume(8);
    let positions = entry_chunks(center);
    let mut chunks = positions
        .iter()
        .map(|position| {
            let mut chunk = Chunk::new();
            for x in 0..CHUNK_X {
                for z in 0..CHUNK_Z {
                    chunk.set(x, 64, z, stone);
                    chunk.set(x, 65, z, dirt);
                    chunk.set(x, 66, z, grass);
                }
            }
            (*position, chunk)
        })
        .collect::<Vec<_>>();
    let center_chunk = ChunkPos::from_surface(center);
    let chunk = chunks
        .iter_mut()
        .find(|(position, _)| *position == center_chunk)
        .map(|(_, chunk)| chunk)
        .unwrap();
    chunk.set(5, 67, 5, log);
    chunk.set(6, 67, 5, water);
    chunk.set(7, 67, 5, bush);

    let qualification = qualify_trial_region(&reg, &atlas, center, &chunks).unwrap();
    assert!(qualification.verification.walkable_cells >= 32);
    assert!(qualification.verification.reachable_wood);
    assert!(qualification.verification.reachable_fresh_water);
    assert!(qualification.verification.reachable_soil);
    assert!(qualification.verification.reachable_stone);
    assert!(qualification.verification.reachable_plants);
}

#[test]
fn homeland_census_installs_three_redundant_observational_sites_once() {
    let root =
        std::env::temp_dir().join(format!("wildforge-discovery-spawn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let reg = Arc::new(crate::registry::load(std::path::Path::new("mods")));
    let mut world = World::new(99, root.clone(), reg.clone());
    world.discovery_state = Some(
        crate::discovery::DiscoveryState::load_or_initialize(&root, 99, reg.content_hash).unwrap(),
    );
    let spawn = world.qualified_spawn_surface().unwrap();
    let stone = reg.block_id("base:stone").unwrap();
    let dirt = reg.block_id("base:dirt").unwrap();
    let grass = reg.block_id("base:grass").unwrap();
    for position in entry_chunks(spawn) {
        let mut chunk = Chunk::new();
        for x in 0..CHUNK_X {
            for z in 0..CHUNK_Z {
                chunk.set(x, 64, z, stone);
                chunk.set(x, 65, z, dirt);
                chunk.set(x, 66, z, grass);
            }
        }
        world.adopt_generated(position, chunk);
    }
    let installed = world.ensure_spawn_discovery_sites(spawn).unwrap();
    assert_eq!(installed.len(), 3);
    assert!(
        world
            .ensure_spawn_discovery_sites(spawn)
            .unwrap()
            .is_empty()
    );
    let mut evidence = HashMap::<String, usize>::new();
    for (_, entity) in world.block_entities() {
        if let BlockEntity::Chest(chest) = entity {
            for stack in chest.slots.iter().flatten() {
                if let Some(class) = reg
                    .item(stack.item)
                    .discovery
                    .as_ref()
                    .and_then(|definition| definition.evidence_class.clone())
                {
                    *evidence.entry(class).or_default() += 1;
                }
            }
        }
    }
    for class in crate::discovery::EVIDENCE_CLASSES {
        assert!(
            evidence.get(class).copied().unwrap_or_default() >= 3,
            "foundational clue {class} is not independently redundant"
        );
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn discovery_census_routes_around_a_wet_nominal_site() {
    let root = std::env::temp_dir().join(format!(
        "wildforge-discovery-wet-bearing-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let reg = Arc::new(crate::registry::load(std::path::Path::new("mods")));
    let mut world = World::new(20260802, root.clone(), reg.clone());
    world.discovery_state = Some(
        crate::discovery::DiscoveryState::load_or_initialize(&root, 20260802, reg.content_hash)
            .unwrap(),
    );
    let spawn = world.qualified_spawn_surface().unwrap();
    let stone = reg.block_id("base:stone").unwrap();
    let dirt = reg.block_id("base:dirt").unwrap();
    let grass = reg.block_id("base:grass").unwrap();
    for position in entry_chunks(spawn) {
        let mut chunk = Chunk::new();
        for x in 0..CHUNK_X {
            for z in 0..CHUNK_Z {
                chunk.set(x, 64, z, stone);
                chunk.set(x, 65, z, dirt);
                chunk.set(x, 66, z, grass);
            }
        }
        world.adopt_generated(position, chunk);
    }
    // Remove every column the old five-point fixed-offset search tried
    // for its first outpost. The accepted homeland remains broadly dry.
    for (du, dv) in [(24, 0), (24, 8), (32, 0), (24, -8), (16, 0)] {
        let surface = SurfacePos::canonicalized(
            spawn.face(),
            i32::from(spawn.u()) + du,
            i32::from(spawn.v()) + dv,
        )
        .unwrap();
        for y in 64..=66 {
            let pos = BlockPos::new(surface.face(), surface.u(), y, surface.v()).unwrap();
            world.set_block_at(pos, AIR);
        }
    }

    assert_eq!(world.ensure_spawn_discovery_sites(spawn).unwrap().len(), 3);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn discovery_retrogen_preserves_worked_homeland_and_adds_explicit_remnants() {
    let root = std::env::temp_dir().join(format!(
        "wildforge-discovery-retrogen-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let reg = Arc::new(crate::registry::load(std::path::Path::new("mods")));
    let mut world = World::new(101, root.clone(), reg.clone());
    world.discovery_state = Some(
        crate::discovery::DiscoveryState::load_or_initialize(&root, 101, reg.content_hash).unwrap(),
    );
    let spawn = world.qualified_spawn_surface().unwrap();
    let stone = reg.block_id("base:stone").unwrap();
    let dirt = reg.block_id("base:dirt").unwrap();
    let grass = reg.block_id("base:grass").unwrap();
    let prepared = entry_chunks(spawn);
    for position in &prepared {
        let mut chunk = Chunk::new();
        for x in 0..CHUNK_X {
            for z in 0..CHUNK_Z {
                chunk.set(x, 64, z, stone);
                chunk.set(x, 65, z, dirt);
                chunk.set(x, 66, z, grass);
            }
        }
        world.adopt_generated(*position, chunk);
    }
    world.player_touched.extend(prepared);
    let sentinel = BlockPos::new(spawn.face(), spawn.u(), 67, spawn.v()).unwrap();
    let planks = reg.block_id("base:planks").unwrap();
    world.set_block_at(sentinel, planks);

    assert_eq!(world.ensure_spawn_discovery_sites(spawn).unwrap().len(), 3);
    assert_eq!(world.get_block_at(sentinel), planks);
    assert!(
        world
            .block_entities()
            .all(|(_, entity)| !matches!(entity, BlockEntity::Chest(_)))
    );
    let cracked = reg.block_id("base:cracked_masonry").unwrap();
    let remnants = world
        .chunks
        .iter()
        .map(|(_, chunk)| {
            (0..CHUNK_X)
                .flat_map(|x| (0..CHUNK_Z).map(move |z| (x, z)))
                .flat_map(|(x, z)| (67..72).map(move |y| (x, y, z)))
                .filter(|(x, y, z)| chunk.get(*x, *y, *z) == cracked)
                .count()
        })
        .sum::<usize>();
    assert_eq!(remnants, 3);
    assert!(
        world
            .ensure_spawn_discovery_sites(spawn)
            .unwrap()
            .is_empty()
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
#[ignore = "operator probe for WILDFORGE_PROBE_WORLD production atlas"]
fn production_spawn_selection_probe() {
    let root = std::env::var_os("WILDFORGE_PROBE_WORLD")
        .map(std::path::PathBuf::from)
        .expect("set WILDFORGE_PROBE_WORLD");
    let atlas = PlanetAtlas::load(&root).unwrap();
    let (candidates, diagnostics) = qualified_atlas_candidates(&atlas);
    eprintln!("{diagnostics}; candidates={}", candidates.len());
    assert!(!candidates.is_empty());
}
