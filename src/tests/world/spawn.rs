//! Spawn scenarios.

use super::*;

#[test]
fn settle_spawn_frees_buried_and_dug_out_spawns() {
    let reg = base_reg();
    let mut w = test_world_with("settlespawn", reg.clone());
    let stone = b(&reg, "base:stone");
    let h = w.surface_height(4, 4);

    // A valid standing spot comes back untouched (bedroll exactness).
    let stand = Vec3::new(4.5, (h + 1) as f32 + 0.2, 4.5);
    assert_eq!(w.settle_spawn(stand), stand);

    // Built over: a hill grows across the spawn cells. The player
    // must come out of the solid, standing on real ground.
    for y in (h + 1)..=(h + 6) {
        w.set_block(4, y, 4, stone);
    }
    let freed = w.settle_spawn(stand);
    let fy = freed.y.floor() as i32;
    assert!(!reg.is_solid(w.get_block(4, fy, 4)), "feet clear");
    assert!(!reg.is_solid(w.get_block(4, fy + 1, 4)), "head clear");
    assert!(reg.is_solid(w.get_block(4, fy - 1, 4)), "ground underfoot");

    // Dug out: the floor under a high spawn is mined away — settle
    // down to the first real floor instead of leaving a free-fall.
    let (px, pz) = (10, 10);
    let ph = w.surface_height(px, pz);
    w.set_block(px, ph + 8, pz, stone);
    let perch = Vec3::new(px as f32 + 0.5, (ph + 9) as f32 + 0.2, pz as f32 + 0.5);
    assert_eq!(w.settle_spawn(perch), perch, "platform stand is valid");
    w.set_block(px, ph + 8, pz, AIR);
    let landed = w.settle_spawn(perch);
    let ly = landed.y.floor() as i32;
    assert!(ly < ph + 9, "came down off the vanished platform");
    assert!(
        reg.is_solid(w.get_block(px, ly - 1, pz)),
        "onto real ground"
    );

    // A column sealed solid from bedrock to sky: walk to a neighbor
    // column rather than teleporting into the fill.
    let (qx, qz) = (20, 20);
    let qh = w.surface_height(qx, qz);
    for y in 1..crate::chunk::CHUNK_Y as i32 - 1 {
        w.set_block(qx, y, qz, stone);
    }
    let sealed = Vec3::new(qx as f32 + 0.5, (qh + 1) as f32 + 0.2, qz as f32 + 0.5);
    let moved = w.settle_spawn(sealed);
    let (mx, mz) = (moved.x.floor() as i32, moved.z.floor() as i32);
    let my = moved.y.floor() as i32;
    assert!((mx, mz) != (qx, qz), "left the sealed column");
    assert!(!reg.is_solid(w.get_block(mx, my, mz)), "feet clear");
    assert!(
        reg.is_solid(w.get_block(mx, my - 1, mz)),
        "ground underfoot"
    );
}

#[test]
fn free_position_rescues_embedded_but_leaves_air_and_water_alone() {
    let reg = base_reg();
    let mut w = test_world_with("freepos", reg.clone());
    let stone = b(&reg, "base:stone");
    let h = w.surface_height(4, 4);

    // Embedded in a hill: rescued to clear cells.
    for y in (h + 1)..=(h + 5) {
        w.set_block(4, y, 4, stone);
    }
    let buried = Vec3::new(4.5, (h + 2) as f32 + 0.2, 4.5);
    let freed = w.free_position(buried);
    let fy = freed.y.floor() as i32;
    assert!(!reg.is_solid(w.get_block(4, fy, 4)), "feet freed");
    assert!(!reg.is_solid(w.get_block(4, fy + 1, 4)), "head freed");

    // A legitimate mid-air save is not touched (physics owns falling).
    let midair = Vec3::new(10.5, (h + 20) as f32, 10.5);
    assert_eq!(w.free_position(midair), midair);

    // A swimmer stays floating where they saved: build a water shaft
    // and confirm no teleport to its floor.
    for y in (h + 1)..=(h + 4) {
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            w.set_block(15 + dx, y, 15 + dz, stone);
        }
    }
    for y in (h + 1)..=(h + 4) {
        w.set_block(15, y, 15, reg.water_block(0));
    }
    let swimming = Vec3::new(15.5, (h + 3) as f32 + 0.5, 15.5);
    assert_eq!(w.free_position(swimming), swimming, "swimmers float on");
}

#[test]
fn nobody_spawns_in_the_water_the_sky_or_a_wall() {
    let reg = base_reg();
    let mut w = test_world_with("spawn-safe", reg.clone());
    let stone = b(&reg, "base:stone");
    let water = reg.water_block(0);
    let check = |w: &World, p: Vec3| {
        let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        assert!(
            reg.is_solid(w.get_block(x, y - 1, z)),
            "solid ground underfoot at {p:?}"
        );
        for dy in 0..2 {
            let b = w.get_block(x, y + dy, z);
            assert!(!reg.is_solid(b), "body not inside a wall at {p:?}");
            assert!(!reg.is_fluid(b), "body not in fluid at {p:?}");
        }
        assert!(y > SEA_LEVEL, "above the tideline at {p:?}");
    };
    // Ordinary ground: taken as-is.
    let p = w.safe_spawn(4, 4);
    check(&w, p);
    // A drowned column: the search walks ashore instead of standing
    // the player on the seabed. (This is the bug — fluid is not solid,
    // so a seabed column used to read as somewhere to stand.)
    for x in 0..10 {
        for z in 0..10 {
            for y in (SEA_LEVEL - 6)..=SEA_LEVEL {
                w.set_block(x, y, z, water);
            }
            for y in SEA_LEVEL + 1..SEA_LEVEL + 5 {
                w.set_block(x, y, z, AIR);
            }
            w.set_block(x, SEA_LEVEL - 7, z, stone);
        }
    }
    let p = w.safe_spawn(5, 5);
    check(&w, p);
}

#[test]
fn open_ocean_gets_an_island_rather_than_a_drowning() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("spawn-isle"), reg.clone());
    let water = reg.water_block(0);
    let stone = b(&reg, "base:stone");
    let center = 'search: {
        for face in crate::planet::Face::ALL {
            for cu in (1..crate::planet::FACE_CHUNKS - 1).step_by(11) {
                for cv in (1..crate::planet::FACE_CHUNKS - 1).step_by(11) {
                    let pos = crate::planet::SurfacePos::new(
                        face,
                        cu * crate::chunk::CHUNK_X as u16 + 8,
                        cv * crate::chunk::CHUNK_Z as u16 + 8,
                    )
                    .unwrap();
                    if w.generator.surface_estimate_at(pos) >= SEA_LEVEL - 4 {
                        continue;
                    }
                    ensure_surface_neighborhood(&mut w, pos, 1);
                    if w.is_open_water_at(pos) {
                        break 'search pos;
                    }
                }
            }
        }
        panic!("seed 42 has no sampled deep ocean");
    };

    // A small patch of open sea: seabed just down, water to the
    // tideline. Kept tight on purpose — every water cell set here
    // wakes the fluid sim, and a big test sea starves the whole
    // parallel suite.
    for du in -9..=9 {
        for dv in -9..=9 {
            let surface = surface_offset(center, du, dv);
            for y in (SEA_LEVEL - 2)..=(SEA_LEVEL + 4) {
                w.set_block_at(
                    block_pos(surface, y),
                    if y <= SEA_LEVEL { water } else { AIR },
                );
            }
            w.set_block_at(block_pos(surface, SEA_LEVEL - 3), stone);
        }
    }
    let crest = w.raise_castaway_isle_at(center);
    assert!(crest > SEA_LEVEL, "landfall rises out of the water");
    // Sand, not a plinth of whatever was underneath.
    assert_eq!(
        block_at(&w, center, crest),
        b(&reg, "base:sand"),
        "a little sand island"
    );
    // Dry overhead, so a castaway is actually standing in air.
    for dy in 1..=2 {
        assert!(
            !reg.is_fluid(block_at(&w, center, crest + dy)),
            "the island is dry at +{dy}"
        );
    }
    // It shelves back into the sea rather than dropping off a tower.
    let rim = w.surface_height_at(surface_offset(center, 4, 0));
    assert!(
        rim < crest && rim >= SEA_LEVEL - 1,
        "the rim shelves ({rim} vs crest {crest})"
    );
    // And it is an island, not a continent: nothing was raised
    // beyond its shore.
    let sand = b(&reg, "base:sand");
    let beyond = surface_offset(center, 8, 0);
    assert!(
        (SEA_LEVEL - 2..=SEA_LEVEL + 3).all(|y| block_at(&w, beyond, y) != sand),
        "no landfill beyond the island's shore"
    );
}

#[test]
#[ignore = "dev tool: times the spawn search"]
fn dev_time_safe_spawn() {
    let reg = base_reg();
    let mut w = test_world_with("spawn-timing", reg.clone());
    let t = std::time::Instant::now();
    let p = w.safe_spawn(0, 0);
    eprintln!("safe_spawn(0,0) = {p:?} in {:?}", t.elapsed());
    eprintln!("surface at 0,0 = {}", w.surface_height(0, 0));
    eprintln!("estimate at 0,0 = {}", w.generator.surface_estimate(0, 0));
}
