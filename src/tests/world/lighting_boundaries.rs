//! Complete light-field invariants for boundary edits and content replacement.

use super::*;
use crate::chunk::{CHUNK_X, CHUNK_Z};
use crate::planet::{BlockPos, Face};

fn light_fields(world: &impl TerrainRead, positions: &[ChunkPos]) -> Vec<([u8; 3], u8)> {
    let mut fields = Vec::with_capacity(positions.len() * CHUNK_X * CHUNK_Y * CHUNK_Z);
    for &position in positions {
        let chunk = world.chunk(position).expect("fixture chunk is resident");
        for x in 0..CHUNK_X {
            for z in 0..CHUNK_Z {
                for y in 0..CHUNK_Y {
                    fields.push(chunk.light(x, y, z));
                }
            }
        }
    }
    fields
}

#[test]
fn removal_restores_complete_fields_at_every_rotated_seam_and_deep_wall() {
    let registry = base_reg();
    let torch = b(&registry, "base:torch");
    let mut cases: Vec<_> = directed_planet_seams()
        .into_iter()
        .map(|seam| (block_pos(seam.source, 100), block_pos(seam.across, 100)))
        .collect();
    // Deep's outward horizontal step reads its own stored edge light.
    let deep = BlockPos::new(Face::Deep, 0, 100, 320).unwrap();
    cases.push((deep, deep));

    for (case, (source, across)) in cases.into_iter().enumerate() {
        let mut world = World::new(
            43,
            tmp_dir(&format!("full-light-seam-{case}")),
            registry.clone(),
        );
        let mut positions = vec![source.chunk(), across.chunk()];
        positions.sort_unstable();
        positions.dedup();
        world.insert_empty_chunks_for_test(positions.iter().copied());
        for &position in &positions {
            world.relight_and_cascade(position);
        }
        let before = light_fields(&world, &positions);
        assert!(before.iter().all(|(rgb, _)| *rgb == [0; 3]));
        world.set_block_at(source, torch);
        assert!(world.light_rgb_at_pos(across).0[0] > 0);
        world.set_block_at(source, AIR);
        assert_eq!(light_fields(&world, &positions), before, "case {case}");

        // An unchanged solve must not dirty a mesh or mark voxel data modified.
        for &position in &positions {
            let chunk = world.chunks_mut().get_mut(&position).unwrap();
            chunk.dirty = false;
            chunk.modified = false;
        }
        world.relight_and_cascade(source.chunk());
        assert_eq!(light_fields(&world, &positions), before);
        for position in positions {
            let chunk = world.chunk(position).unwrap();
            assert!(!chunk.dirty && !chunk.modified);
        }
    }
}

#[test]
fn a_late_neighbor_preserves_the_complete_existing_field_and_drains_on_removal() {
    let registry = base_reg();
    let mut world = World::new(43, tmp_dir("full-light-late-neighbor"), registry.clone());
    let source = bp(15, 100, 8);
    let across = bp(16, 100, 8);
    world.insert_empty_chunks_for_test([source.chunk()]);
    world.relight_and_cascade(source.chunk());
    let dark_source = light_fields(&world, &[source.chunk()]);
    world.set_block_at(source, b(&registry, "base:torch"));
    let solo = light_fields(&world, &[source.chunk()]);
    assert!(!world.has_chunk(across.chunk()));

    world.insert_empty_chunks_for_test([across.chunk()]);
    world.relight_and_cascade(across.chunk());
    assert_eq!(light_fields(&world, &[source.chunk()]), solo);
    assert_eq!(
        world.light_rgb_at_pos(across).0,
        registry
            .block(b(&registry, "base:torch"))
            .light_rgb
            .map(|v| v.saturating_sub(1)),
    );
    world.set_block_at(source, AIR);
    assert_eq!(light_fields(&world, &[source.chunk()]), dark_source);
    assert_eq!(light_fields(&world, &[across.chunk()]), dark_source);

    // A missing target cannot reuse an earlier chunk's border snapshot.
    let before = light_fields(&world, &[source.chunk(), across.chunk()]);
    world.relight_and_cascade(tchunk(20, 20));
    assert_eq!(
        light_fields(&world, &[source.chunk(), across.chunk()]),
        before
    );
}

#[test]
fn tinted_water_seam_fields_match_legacy_and_settled_wire_after_edits() {
    let registry = base_reg();
    let mut world = World::new(43, tmp_dir("full-light-wire-corridor"), registry.clone());
    let positions = [tchunk(0, 0), tchunk(1, 0)];
    world.insert_empty_chunks_for_test(positions);
    let stone = b(&registry, "base:stone");
    // A one-cell-wide sealed corridor; there is no alternate path around glass.
    for position in positions {
        world
            .chunks_mut()
            .get_mut(&position)
            .unwrap()
            .raw_mut()
            .fill(stone.0);
    }
    for x in 14..20 {
        let position = bp(x, 100, 8);
        let (lx, y, lz) = position.local();
        world
            .chunks_mut()
            .get_mut(&position.chunk())
            .unwrap()
            .set(lx, y, lz, AIR);
    }
    world.set_block_at(bp(15, 100, 8), b(&registry, "base:red_glass"));
    world.set_block_at(bp(16, 100, 8), registry.water_ids[0]);
    world.relight_and_cascade(positions[0]);
    world.relight_and_cascade(positions[1]);
    let dark = light_fields(&world, &positions);
    assert!(dark.iter().all(|light| *light == ([0; 3], 0)));
    let source = bp(14, 100, 8);
    let torch = b(&registry, "base:torch");
    world.set_block_at(source, torch);
    assert_eq!(
        world.light_rgb_at_pos(bp(17, 100, 8)),
        (
            [registry.block(torch).light_rgb[0].saturating_sub(4), 0, 0],
            0
        ),
    );

    let remap: Vec<_> = (0..registry.blocks.len())
        .map(|id| registry::BlockId(id as u16))
        .collect();
    let mut replicas = Vec::new();
    for settled in [false, true] {
        let chunks: Vec<_> = positions
            .iter()
            .map(|&position| {
                let chunk = world.chunk(position).unwrap();
                let bytes = if settled {
                    crate::world::encode_stream_chunk(chunk)
                } else {
                    crate::world::encode_chunk_for_test(chunk)
                };
                (position, bytes)
            })
            .collect();
        let mut replica = ReplicaWorld::new(43, registry.clone(), 0.0);
        replica.insert_remote_chunks(
            chunks
                .iter()
                .map(|(position, bytes)| (*position, bytes.as_slice())),
            &remap,
        );
        assert_eq!(
            light_fields(&replica, &positions),
            light_fields(&world, &positions)
        );
        replicas.push(replica);
    }
    world.set_block_at(source, AIR);
    assert_eq!(light_fields(&world, &positions), dark);
    for mut replica in replicas {
        replica.apply_remote_block_states([(source, AIR, 0, 0, 0)]);
        assert_eq!(light_fields(&replica, &positions), dark);
    }
}

#[test]
fn replacement_registry_rebuilds_complete_fields_without_previous_emission() {
    let root = tmp_dir("full-light-registry");
    let content = root.join("lamp");
    std::fs::create_dir_all(&content).unwrap();
    std::fs::write(content.join("mod.toml"), "id = \"lamp\"\nworld_api = 2\n").unwrap();
    let write_lamp = |level: u8, color: [u8; 3]| {
        std::fs::write(content.join("blocks.toml"), format!(
            "[[block]]\nid = \"fixture\"\ntexture = \"@stone\"\nopaque = false\nlight = {level}\nlight_color = [{}, {}, {}]\n",
            color[0], color[1], color[2],
        )).unwrap();
    };
    write_lamp(14, [1, 0, 0]);
    let old = Arc::new(registry::load(&root));
    let positions = [tchunk(0, 0), tchunk(1, 0)];
    let source = bp(15, 100, 8);
    let mut world = World::new(43, tmp_dir("full-light-old-registry"), old.clone());
    world.insert_empty_chunks_for_test(positions);
    world.set_block_at(source, b(&old, "lamp:fixture"));
    assert_eq!(world.light_rgb_at_pos(bp(16, 100, 8)).0, [13, 0, 0]);

    write_lamp(9, [0, 0, 1]);
    let new = Arc::new(registry::load(&root));
    world.replace_registry(new.clone());
    world.relight_and_cascade(source.chunk());
    let mut fresh = World::new(43, tmp_dir("full-light-new-registry"), new.clone());
    fresh.insert_empty_chunks_for_test(positions);
    fresh.set_block_at(source, b(&new, "lamp:fixture"));
    assert_eq!(world.light_rgb_at_pos(bp(16, 100, 8)).0, [0, 0, 8]);
    assert_eq!(
        light_fields(&world, &positions),
        light_fields(&fresh, &positions)
    );
}

#[test]
fn a_uniform_full_height_water_column_keeps_exact_sky_through_saturation() {
    let registry = base_reg();
    let mut world = World::new(43, tmp_dir("full-light-water-saturation"), registry.clone());
    let position = tchunk(0, 0);
    world.insert_empty_chunks_for_test([position]);
    world
        .chunks_mut()
        .get_mut(&position)
        .unwrap()
        .raw_mut()
        .fill(registry.water_ids[0].0);
    world.relight_and_cascade(position);
    let chunk = world.chunk(position).unwrap();
    for x in 0..CHUNK_X {
        for z in 0..CHUNK_Z {
            for y in 0..CHUNK_Y {
                // The sky-column rule loses one per water cell, including
                // the top at y255, then remains zero all the way to y0.
                let depth = CHUNK_Y - 1 - y;
                let expected = 14u8.saturating_sub(depth.min(15) as u8);
                assert_eq!(chunk.light(x, y, z), ([0; 3], expected), "{x},{y},{z}");
            }
        }
    }
}

#[test]
fn an_overhang_water_shaft_reopens_to_the_same_complete_sky_field() {
    let registry = base_reg();
    let mut world = World::new(43, tmp_dir("full-light-overhang-shaft"), registry.clone());
    let position = tchunk(0, 0);
    world.insert_empty_chunks_for_test([position]);
    let stone = b(&registry, "base:stone");
    // Unequal roof heights surround a sealed room with a water-filled shaft.
    // Initial sky is 15 outside, zero inside, and becomes a descending water
    // column plus horizontal/vertical flood when the roof opens.
    let chunk = world.chunks_mut().get_mut(&position).unwrap();
    for x in 6..=10 {
        for z in 6..=10 {
            for y in 195..=200 {
                if x == 6 || x == 10 || z == 6 || z == 10 || y == 195 || y == 200 {
                    chunk.set(x, y, z, stone);
                }
            }
        }
    }
    for y in 196..200 {
        chunk.set(8, y, 8, registry.water_ids[0]);
    }
    world.relight_and_cascade(position);
    let sealed = light_fields(&world, &[position]);
    assert_eq!(world.light_at(8, 196, 8).1, 0);
    world.set_block_at(bp(8, 200, 8), AIR);
    assert_eq!(world.light_at(8, 199, 8).1, 14);
    assert_eq!(world.light_at(8, 196, 8).1, 11);
    assert_eq!(world.light_at(9, 199, 8).1, 13);
    assert_eq!(world.light_at(9, 196, 8).1, 10);
    let open = light_fields(&world, &[position]);
    world.set_block_at(bp(8, 200, 8), stone);
    assert_eq!(light_fields(&world, &[position]), sealed);
    world.set_block_at(bp(8, 200, 8), AIR);
    assert_eq!(light_fields(&world, &[position]), open);
}
