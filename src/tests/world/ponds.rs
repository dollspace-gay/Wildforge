//! Ponds scenarios.

use super::*;

#[test]
fn magma_pools_the_deep_chambers() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("magma"), reg.clone());
    let mut lava_cells = 0;
    // Deep magma is a sparse planetary field. Sample the same finite
    // hydrology/geology neighborhoods used by the generator census instead
    // of assuming the old flat origin happens to cut through a chamber.
    for (center, _) in find_water_features(&w.generator, 4) {
        let chunk = crate::planet::ChunkPos::from_surface(center);
        for du in -2..=2 {
            for dv in -2..=2 {
                let sample = chunk.offset(du, dv);
                w.ensure_chunk(sample);
                for lx in 0..crate::chunk::CHUNK_X as u16 {
                    for lz in 0..crate::chunk::CHUNK_Z as u16 {
                        let surface = crate::planet::SurfacePos::new(
                            sample.face(),
                            sample.u() * crate::chunk::CHUNK_X as u16 + lx,
                            sample.v() * crate::chunk::CHUNK_Z as u16 + lz,
                        )
                        .unwrap();
                        lava_cells += (1..12)
                            .filter(|&y| reg.is_lava(block_at(&w, surface, y)))
                            .count();
                    }
                }
            }
        }
        if lava_cells > 20 {
            break;
        }
    }
    assert!(
        lava_cells > 20,
        "deep cheese chambers pool magma ({lava_cells})"
    );
}

#[test]
fn breached_pool_pours_over_the_edge() {
    // Break a dam and the pool should empty through the gap: the drop
    // rule pushes even the last unit over an edge (the moved water
    // falls, so it can never slosh back), leaving at most a thin
    // glaze on cells with no path to a fall.
    let reg = base_reg();
    let mut w = test_world_with("breach", reg.clone());
    let stone = b(&reg, "base:stone");
    let y = 200;
    // 7x7 sky platform, 5x5 wall ring, 3x3 pool of full water.
    for x in 0..7 {
        for z in 0..7 {
            w.set_block(x, y, z, stone);
        }
    }
    for x in 1..6 {
        for z in 1..6 {
            if x == 1 || x == 5 || z == 1 || z == 5 {
                w.set_block(x, y + 1, z, stone);
            }
        }
    }
    for x in 2..5 {
        for z in 2..5 {
            w.set_block(x, y + 1, z, reg.water_block(0));
        }
    }
    while w.tick_water(10_000) {}
    w.set_block(3, y + 1, 1, AIR); // the breach
    let mut quiet = false;
    for _ in 0..2000 {
        if !w.tick_water(10_000) {
            quiet = true;
            break;
        }
    }
    assert!(quiet, "the breached pool settles");
    let total = |w: &World| -> u32 {
        let mut sum = 0u32;
        for x in -1..8 {
            for z in -1..8 {
                sum += reg.water_volume(w.get_block(x, y + 1, z)).unwrap_or(0) as u32;
            }
        }
        sum
    };
    // 72 units started; the pour takes the majority with it, leaving
    // only the shallow hysteresis gradient behind.
    let left = total(&w);
    assert!(
        left <= 36,
        "pool mostly drained through the breach ({left} left)"
    );
    // The apron ring borders the platform rim on every side: the drop
    // rule empties it completely — even the last unit goes over.
    for x in 0..7 {
        assert_eq!(
            reg.water_volume(w.get_block(x, y + 1, 0)),
            None,
            "apron cell ({x},0) drained dry"
        );
    }
    // Then the sun finishes the job: the residue is shallow and open,
    // so it draws down and dries through — the basin empties fully.
    w.set_calendar_day(crate::world::SEASON_DAYS); // summer
    let mut rng = 5u32;
    for _ in 0..40_000 {
        w.random_tick(&mut rng);
        w.tick_water(1_000);
    }
    assert_eq!(total(&w), 0, "the breached basin dries out completely");
}

#[test]
fn pools_level_through_a_submerged_gap() {
    // Two wells share a wall; the breach sits at the bottom layer,
    // below both surfaces once the low side backs up. Equalization
    // alone stalls there (every layer at the gap is full) — pressure
    // has to carry the difference, and the surfaces must meet.
    let reg = base_reg();
    let mut w = test_world_with("utube", reg.clone());
    let stone = b(&reg, "base:stone");
    let y = 200;
    // Solid 7x4 block from y..y+5, wells carved at x 1..=2 and 4..=5.
    for x in 0..7 {
        for z in 0..4 {
            for yy in y..=y + 5 {
                w.set_block(x, yy, z, stone);
            }
        }
    }
    for z in 1..=2 {
        for x in [1, 2, 4, 5] {
            for yy in y + 1..=y + 5 {
                w.set_block(x, yy, z, AIR);
            }
        }
    }
    // A holds four blocks of water, B one.
    for z in 1..=2 {
        for x in [1, 2] {
            for yy in y + 1..=y + 4 {
                w.set_block(x, yy, z, reg.water_block(0));
            }
        }
        for x in [4, 5] {
            w.set_block(x, y + 1, z, reg.water_block(0));
        }
    }
    while w.tick_water(10_000) {}
    // Knock one wall block out of the bottom layer. The wall above
    // the hole stays: this link is a sealed-top pipe mouth.
    w.set_block(3, y + 1, 1, AIR);
    let mut quiet = false;
    for _ in 0..4000 {
        if !w.tick_water(10_000) {
            quiet = true;
            break;
        }
    }
    assert!(quiet, "the linked pools settle");
    // Column heads across both wells and the gap must agree within
    // the 2-unit hysteresis: the surfaces have met.
    let head = |w: &World, x: i32, z: i32| -> i64 {
        let mut h = 0;
        for yy in y + 1..=y + 5 {
            if let Some(v) = reg.water_volume(w.get_block(x, yy, z)) {
                h = yy as i64 * 8 + v as i64;
            }
        }
        h
    };
    let cols: Vec<i64> = [
        (1, 1),
        (2, 1),
        (1, 2),
        (2, 2),
        (4, 1),
        (5, 1),
        (4, 2),
        (5, 2),
    ]
    .iter()
    .map(|&(x, z)| head(&w, x, z))
    .collect();
    let (lo, hi) = (*cols.iter().min().unwrap(), *cols.iter().max().unwrap());
    assert!(hi > 0, "water survived");
    assert!(hi - lo <= 2, "pressure levels the pools (heads {lo}..{hi})");
    // And the low side genuinely rose: from one block to over two.
    assert!(
        head(&w, 5, 2) >= (y as i64 + 2) * 8,
        "the far well rose ({})",
        head(&w, 5, 2)
    );
}
