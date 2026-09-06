//! Soil scenarios.

use super::*;

#[test]
fn the_heap_ripens_into_compost() {
    let reg = base_reg();
    let mut w = test_world_with("heap", reg.clone());
    let h = w.surface_height(4, 4);
    let heap = b(&reg, "base:compost_heap");
    w.set_block_meta(4, h + 1, 4, heap, 0);
    // Greens go in item by item; mush counts triple.
    assert!(w.compost_fill(4, h + 1, 4, "base:spoiled_mush"));
    assert!(w.compost_fill(4, h + 1, 4, "base:wheat_seeds"));
    assert!(w.compost_fill(4, h + 1, 4, "base:spoiled_mush"));
    assert!(w.compost_fill(4, h + 1, 4, "base:spoiled_mush"));
    assert!(
        w.get_meta(4, h + 1, 4) >= soil::COMPOST_FULL,
        "the heap is full"
    );
    assert!(
        !w.compost_fill(4, h + 1, 4, "base:berry"),
        "a full heap takes no more"
    );
    // Stone is not compost.
    assert!(!w.compost_fill(4, h + 1, 4, "base:cobblestone"));
    // A bed of full heaps: any single cell waits days for its random
    // tick, so give the sampler a fair field and watch one turn.
    for x in 0..5 {
        for z in 0..5 {
            w.set_block_meta(8 + x, h + 1, 8 + z, heap, soil::COMPOST_FULL);
        }
    }
    let ready = b(&reg, "base:compost_heap_ready");
    let mut rng = 5u32;
    let mut turned = None;
    'outer: for _ in 0..12000 {
        w.random_tick(&mut rng);
        for x in 0..5 {
            for z in 0..5 {
                if w.get_block(8 + x, h + 1, 8 + z) == ready {
                    turned = Some((8 + x, h + 1, 8 + z));
                    break 'outer;
                }
            }
        }
    }
    let (rx, ry, rz) = turned.expect("a full heap cooked down");
    assert!(w.compost_take(rx, ry, rz), "the crumb comes out");
    assert_eq!(w.get_block(rx, ry, rz), heap);
    assert_eq!(w.get_meta(rx, ry, rz), 0, "fresh and empty again");
}

#[test]
fn the_fox_hunts_the_rabbit_and_leaves_a_carcass() {
    let reg = base_reg();
    let mut w = test_world_with("foxhunt", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 20, 0, 20, h);
    let mut fox = beast(&reg, "base:fox", glam::Vec3::new(6.5, h as f32 + 1.0, 8.5));
    fox.belly = -1.0;
    w.spawn_mob(fox);
    w.spawn_mob(beast(
        &reg,
        "base:rabbit",
        glam::Vec3::new(12.5, h as f32 + 1.0, 8.5),
    ));
    let ire_before = w.ire;
    let carcass_si = reg.animal_id("base:carcass").unwrap();
    let mut rng = 11u32;
    let mut fed = false;
    for _ in 0..4000 {
        w.tick_mobs(&[], 1.0, 0.05, &mut rng);
        if w.mobs().iter().any(|m| m.species == carcass_si) {
            fed = true;
            break;
        }
    }
    assert!(fed, "the fox made its kill and the kill left a carcass");
    assert_eq!(w.ire, ire_before, "predation moves the meter not at all");
}

#[test]
fn bats_roost_and_the_guano_gathers() {
    let reg = base_reg();
    let mut w = test_world_with("bats", reg.clone());
    // A cave box: floor, walls, and a roof to roost against.
    let stone = b(&reg, "base:stone");
    for x in 4..=12 {
        for z in 4..=12 {
            for y in 18..=26 {
                let shell = x == 4 || x == 12 || z == 4 || z == 12 || y == 18 || y == 26;
                w.set_block(x, y, z, if shell { stone } else { AIR });
            }
        }
    }
    let mut bat = beast(&reg, "base:bat", glam::Vec3::new(8.5, 21.0, 8.5));
    bat.belly = -1.0;
    w.spawn_mob(bat);
    let mut rng = 53u32;
    let mut gathered = false;
    for _ in 0..3000 {
        let evs = w.tick_mobs(&[], 0.0, 0.05, &mut rng);
        if evs
            .iter()
            .any(|e| matches!(e, crate::mobs::MobEvent::Dung(_, true)))
        {
            gathered = true;
            break;
        }
    }
    assert!(gathered, "the roost pays in guano");
    let bat_si = reg.animal_id("base:bat").unwrap();
    let bat = w.mobs().iter().find(|m| m.species == bat_si).expect("bat");
    assert!(
        bat.pos.y > 22.0,
        "the hover presses the bat toward the roof (y {:.1})",
        bat.pos.y
    );
    // And guano out-feeds dung: the cave's gift is the strong one.
    assert!(soil::fertilizer_value("base:guano") > soil::fertilizer_value("base:dung"));
}

#[test]
fn severed_leaves_rot_but_the_tree_keeps_its_crown() {
    let reg = base_reg();
    let mut w = test_world_with("rotting", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 24, 0, 24, h);
    let leaves = b(&reg, "base:leaves");
    let log = b(&reg, "base:log");
    // A severed canopy floating with no trunk anywhere near it...
    for x in 4..=8 {
        for z in 4..=8 {
            w.set_block(x, h + 6, z, leaves);
        }
    }
    // ...and an honest tree: trunk with its crown attached.
    for dy in 1..=3 {
        w.set_block(18, h + dy, 18, log);
    }
    for x in 17..=19 {
        for z in 17..=19 {
            w.set_block(x, h + 4, z, leaves);
        }
    }
    let mut rng = 59u32;
    for _ in 0..6000 {
        w.random_tick(&mut rng);
    }
    let severed = (4..=8)
        .flat_map(|x| (4..=8).map(move |z| (x, z)))
        .filter(|&(x, z)| w.get_block(x, h + 6, z) == leaves)
        .count();
    assert!(severed < 25, "the orphan canopy is rotting ({severed}/25)");
    let crowned = (17..=19)
        .flat_map(|x| (17..=19).map(move |z| (x, z)))
        .filter(|&(x, z)| w.get_block(x, h + 4, z) == leaves)
        .count();
    assert_eq!(crowned, 9, "the living tree keeps every leaf");
    // Some of what fell became litter on the ground below.
    let litter = b(&reg, "base:leaf_litter");
    let littered = (4..=8)
        .flat_map(|x| (4..=8).map(move |z| (x, z)))
        .filter(|&(x, z)| w.get_block(x, h + 1, z) == litter)
        .count();
    assert!(littered > 0, "the ground caught some of it");
}

#[test]
fn litter_fades_into_the_field() {
    let reg = base_reg();
    let mut w = test_world_with("litterfade", reg.clone());
    let h = w.surface_height(8, 8);
    let farm = b(&reg, "base:farmland");
    let litter = b(&reg, "base:leaf_litter");
    for x in 4..=12 {
        for z in 4..=12 {
            w.set_block_meta(x, h, z, farm, soil::soil_meta(10, 0));
            w.set_block(x, h + 1, z, litter);
            for dy in 2..4 {
                if w.get_block(x, h + dy, z) != AIR {
                    w.set_block(x, h + dy, z, AIR);
                }
            }
        }
    }
    let before: u32 = (4..=12)
        .flat_map(|x| (4..=12).map(move |z| (x, z)))
        .map(|(x, z)| soil::fert_of(w.get_meta(x, h, z)) as u32)
        .sum();
    let mut rng = 61u32;
    for _ in 0..4000 {
        w.random_tick(&mut rng);
    }
    let faded = (4..=12)
        .flat_map(|x| (4..=12).map(move |z| (x, z)))
        .filter(|&(x, z)| w.get_block(x, h + 1, z) == AIR)
        .count();
    assert!(faded > 0, "litter fades ({faded})");
    let after: u32 = (4..=12)
        .flat_map(|x| (4..=12).map(move |z| (x, z)))
        .map(|(x, z)| soil::fert_of(w.get_meta(x, h, z)) as u32)
        .sum();
    assert!(
        after > before,
        "and the field is richer ({before}->{after})"
    );
}

#[test]
fn fungi_creep_only_in_the_dark_and_damp() {
    let reg = base_reg();
    let mut w = test_world_with("fungi", reg.clone());
    let stone = b(&reg, "base:stone");
    let shroom = b(&reg, "base:wild_mushroom");
    // A sealed dark chamber deep underground with a water cell.
    for x in 4..=14 {
        for z in 4..=14 {
            for y in 20..=25 {
                let shell = x == 4 || x == 14 || z == 4 || z == 14 || y == 20 || y == 25;
                w.set_block(x, y, z, if shell { stone } else { AIR });
            }
        }
    }
    w.set_block(6, 21, 6, w.reg.water_block(0));
    // Four parents: any one cell's random-tick visits are days
    // apart, so give the sampler a fair colony.
    for (px, pz) in [(7, 7), (11, 7), (7, 11), (11, 11)] {
        w.set_block(px, 21, pz, shroom);
    }
    // A twin chamber lit by full sky: carve to the surface... simpler,
    // a surface mushroom on the open pad stays lonely.
    let h = w.surface_height(30, 30);
    for dx in -3..=3 {
        for dz in -3..=3 {
            w.set_block(30 + dx, h, 30 + dz, b(&reg, "base:grass"));
            for dy in 1..4 {
                if w.get_block(30 + dx, h + dy, 30 + dz) != AIR {
                    w.set_block(30 + dx, h + dy, 30 + dz, AIR);
                }
            }
        }
    }
    w.set_block(30, h + 1, 30, shroom);
    let count_dark = |w: &World| {
        (5..14)
            .flat_map(|x| (5..14).map(move |z| (x, z)))
            .flat_map(|(x, z)| (21..25).map(move |y| (x, y, z)))
            .filter(|&(x, y, z)| w.get_block(x, y, z) == shroom)
            .count()
    };
    let count_lit = |w: &World| {
        (27..=33)
            .flat_map(|x| (27..=33).map(move |z| (x, z)))
            .filter(|&(x, z)| w.get_block(x, h + 1, z) == shroom)
            .count()
    };
    let mut rng = 67u32;
    for _ in 0..14000 {
        w.random_tick(&mut rng);
        if count_dark(&w) >= 6 {
            break;
        }
    }
    assert!(count_dark(&w) > 4, "the dark chamber grows fungus");
    assert!(count_dark(&w) < 24, "and the crowd cap holds");
    assert_eq!(count_lit(&w), 1, "daylight grows nothing");
}

#[test]
fn the_lantern_fungus_lights_the_deep() {
    let reg = base_reg();
    let lf = b(&reg, "base:lantern_fungus");
    assert_eq!(reg.block(lf).light_emit, 6, "it glows");
    let mut w = World::new(42, tmp_dir("lanterns"), reg.clone());
    let mut found = 0;
    for cx in -8..8 {
        for cz in -8..8 {
            w.ensure_chunk(tchunk(cx, cz));
            for lx in 0..16 {
                for lz in 0..16 {
                    for y in 6..46 {
                        if w.get_block(cx * 16 + lx, y, cz * 16 + lz) == lf {
                            found += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(found > 0, "the deep grew its own light ({found})");
}

#[test]
fn a_fallen_warden_leaves_the_land_stirring() {
    let reg = base_reg();
    let mut w = test_world_with("wildfall", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 16, 0, 16, h);
    assert_eq!(w.bloom_at(8, 8), 0.0);
    w.wild_falls("base:dryad", 8, h + 1, 8);
    assert!(w.bloom_at(8, 8) > 0.0, "the death site banks bloom");
    assert_eq!(
        w.get_block(8, h + 1, 8),
        b(&reg, "base:oak_sapling"),
        "a dryad puts up a sapling where it stood"
    );
}
