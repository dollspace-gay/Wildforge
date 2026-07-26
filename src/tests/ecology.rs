//! The belly: hunger drives grazing, raids, dung, herds, compost.

use super::*;
use crate::world::soil;

/// A flat grass pad centered near the origin, cleared overhead.
fn pad(w: &mut World, reg: &Registry, x0: i32, x1: i32, z0: i32, z1: i32, h: i32) {
    let grass = b(reg, "base:grass");
    for x in x0..=x1 {
        for z in z0..=z1 {
            w.set_block(x, h, z, grass);
            for dy in 1..5 {
                if w.get_block(x, h + dy, z) != AIR {
                    w.set_block(x, h + dy, z, AIR);
                }
            }
        }
    }
}

fn hungry_deer(reg: &Registry, at: glam::Vec3) -> crate::mobs::Mob {
    let si = reg.animal_id("base:deer").expect("deer exists");
    let mut m = crate::mobs::Mob::new(si, at, 0.0);
    m.health = reg.animals[si].health;
    m.belly = -1.0;
    m
}

#[test]
fn the_hungry_deer_raids_the_unfenced_field() {
    let reg = base_reg();
    let mut w = test_world_with("raid", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 24, 0, 24, h);
    let farm = b(&reg, "base:farmland");
    let ripe = b(&reg, "base:wheat_seeds/stage2");
    for x in 14..=16 {
        for z in 6..=10 {
            w.set_block_meta(x, h, z, farm, soil::soil_meta(40, 0));
            w.set_block(x, h + 1, z, ripe);
        }
    }
    w.spawn_mob(hungry_deer(&reg, glam::Vec3::new(6.5, h as f32 + 1.0, 8.5)));
    let mut sim = crate::server::Server::new(w, 0.3, 7);
    let mut evs = Vec::new();
    let mut bitten = false;
    for _ in 0..4000 {
        sim.advance(0.05, &[], &mut evs);
        bitten = (14..=16)
            .flat_map(|x| (6..=10).map(move |z| (x, z)))
            .any(|(x, z)| sim.world.get_block(x, h + 1, z) == b(&reg, "base:wheat_seeds"));
        if bitten {
            break;
        }
    }
    assert!(
        bitten,
        "the deer walked to the field and ate the ripest thing in reach"
    );
}

#[test]
fn the_fence_defeats_the_raid() {
    let reg = base_reg();
    let mut w = test_world_with("fenced", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 24, 0, 24, h);
    let farm = b(&reg, "base:farmland");
    let ripe = b(&reg, "base:wheat_seeds/stage2");
    let stone = b(&reg, "base:stone");
    for x in 14..=16 {
        for z in 6..=10 {
            w.set_block_meta(x, h, z, farm, soil::soil_meta(40, 0));
            w.set_block(x, h + 1, z, ripe);
        }
    }
    // A one-high wall around the field: the raid never climbs.
    for x in 12..=18 {
        for z in 4..=12 {
            if x == 12 || x == 18 || z == 4 || z == 12 {
                w.set_block(x, h + 1, z, stone);
            }
        }
    }
    w.spawn_mob(hungry_deer(&reg, glam::Vec3::new(6.5, h as f32 + 1.0, 8.5)));
    let mut sim = crate::server::Server::new(w, 0.3, 7);
    let mut evs = Vec::new();
    for _ in 0..4000 {
        sim.advance(0.05, &[], &mut evs);
    }
    let standing = (14..=16)
        .flat_map(|x| (6..=10).map(move |z| (x, z)))
        .filter(|&(x, z)| sim.world.get_block(x, h + 1, z) == ripe)
        .count();
    assert_eq!(standing, 15, "every stalk stands behind the wall");
}

#[test]
fn grazing_turns_grass_and_digestion_pays_the_soil() {
    let reg = base_reg();
    let mut w = test_world_with("graze", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 20, 0, 20, h);
    w.spawn_mob(hungry_deer(
        &reg,
        glam::Vec3::new(10.5, h as f32 + 1.0, 10.5),
    ));
    let mut sim = crate::server::Server::new(w, 0.3, 7);
    let mut evs = Vec::new();
    let dirt = b(&reg, "base:dirt");
    let dung = it(&reg, "base:dung");
    let mut grazed = false;
    let mut dunged = false;
    for _ in 0..6000 {
        sim.advance(0.05, &[], &mut evs);
        if !grazed {
            grazed = (0..=20)
                .flat_map(|x| (0..=20).map(move |z| (x, z)))
                .any(|(x, z)| sim.world.get_block(x, h, z) == dirt);
        }
        if sim
            .world
            .take_pending_drops()
            .iter()
            .any(|(_, s)| s.item == dung)
        {
            dunged = true;
        }
        if grazed && dunged {
            break;
        }
    }
    assert!(grazed, "the deer cropped the grass down to dirt");
    assert!(dunged, "digestion finished where it always does");
    // And the dung is worth soil: the fertilizer loop closes.
    let farm = b(&reg, "base:farmland");
    sim.world
        .set_block_meta(2, h, 2, farm, soil::soil_meta(10, 1));
    let v = soil::fertilizer_value("base:dung");
    assert!(v > 0);
    assert!(sim.world.feed_soil(2, h, 2, v));
    assert_eq!(soil::fert_of(sim.world.get_meta(2, h, 2)), 10 + v);
}

#[test]
fn herds_lean_homeward() {
    let reg = base_reg();
    let mut w = test_world_with("herd", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 31, 0, 31, h);
    // A walled arena in tended country: the trio can't scatter off
    // the pad, so what the measurement sees is the pull, not luck.
    let stone = b(&reg, "base:stone");
    for x in 0..=31 {
        for z in 0..=31 {
            if x == 0 || x == 31 || z == 0 || z == 31 {
                w.set_block(x, h + 1, z, stone);
            }
        }
    }
    for cx in -1..=1 {
        for cz in -1..=1 {
            w.player_touched.insert((cx, cz));
        }
    }
    let si = reg.animal_id("base:deer").unwrap();
    for x in [3.5f32, 15.5, 27.5] {
        let mut m = crate::mobs::Mob::new(si, glam::Vec3::new(x, h as f32 + 1.0, 15.5), 0.0);
        m.health = reg.animals[si].health;
        m.belly = 9999.0; // sated: today is for walking
        m.tamed = true; // the tag that finds our trio among strangers
        w.spawn_mob(m);
    }
    // Seeding and repop add strangers mid-run; the tame flag finds
    // our trio no matter who else wanders in.
    let spread = |w: &World| -> f32 {
        let trio: Vec<glam::Vec3> = w.mobs().iter().filter(|m| m.tamed).map(|m| m.pos).collect();
        let mut s = 0.0f32;
        for i in 0..trio.len() {
            for j in (i + 1)..trio.len() {
                s += (trio[i] - trio[j]).length();
            }
        }
        s / 3.0
    };
    let before = spread(&w);
    let mut rng = 2024u32;
    for _ in 0..4800 {
        w.tick_mobs(&[], 1.0, 0.05, &mut rng);
    }
    let after = spread(&w);
    assert!(
        after < before,
        "the herd drifts together ({before:.1} -> {after:.1})"
    );
}

#[test]
fn a_calm_deer_minds_the_pen_but_panic_leaps() {
    let reg = base_reg();
    let mut w = test_world_with("pen", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 16, 0, 16, h);
    let plank = b(&reg, "base:planks");
    // A one-high plank pen in tended country.
    for x in 6..=10 {
        for z in 6..=10 {
            if x == 6 || x == 10 || z == 6 || z == 10 {
                w.set_block(x, h + 1, z, plank);
            }
        }
    }
    for cx in -1..=1 {
        for cz in -1..=1 {
            w.player_touched.insert((cx, cz));
        }
    }
    let si = reg.animal_id("base:deer").unwrap();
    let mut m = crate::mobs::Mob::new(si, glam::Vec3::new(8.5, h as f32 + 1.0, 8.5), 0.0);
    m.health = reg.animals[si].health;
    m.tamed = true;
    m.belly = 9999.0;
    m.tamed = true;
    w.spawn_mob(m);
    let mut rng = 77u32;
    for _ in 0..2400 {
        w.tick_mobs(&[], 1.0, 0.05, &mut rng);
    }
    let deer = w.mobs().iter().find(|m| m.tamed).expect("the deer");
    assert!(
        (7.0..10.0).contains(&deer.pos.x) && (7.0..10.0).contains(&deer.pos.z),
        "the calm deer stayed penned (at {:.1} {:.1})",
        deer.pos.x,
        deer.pos.z
    );
}

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

fn ctx(pos: glam::Vec3) -> crate::server::PlayerCtx {
    crate::server::PlayerCtx {
        id: 0,
        pos,
        spawn: glam::Vec3::ZERO,
        attackable: true,
        aggro_mod: 0.0,
    }
}

fn beast(reg: &Registry, name: &str, at: glam::Vec3) -> crate::mobs::Mob {
    let si = reg.animal_id(name).expect("species exists");
    let mut m = crate::mobs::Mob::new(si, at, 0.0);
    m.health = reg.animals[si].health;
    m
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
fn the_polar_bear_needs_no_reason() {
    let reg = base_reg();
    let mut w = test_world_with("bear", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 24, 0, 24, h);
    let bear_si = reg.animal_id("base:polar_bear").unwrap();
    w.spawn_mob(beast(
        &reg,
        "base:polar_bear",
        glam::Vec3::new(8.5, h as f32 + 1.0, 8.5),
    ));
    let player = glam::Vec3::new(18.5, h as f32 + 1.0, 8.5);
    // High noon, full health, nothing provoked: it simply comes.
    let mut rng = 13u32;
    let mut mauled = false;
    for _ in 0..3000 {
        let evs = w.tick_mobs(&[ctx(player)], 1.0, 0.05, &mut rng);
        if evs
            .iter()
            .any(|e| matches!(e, crate::mobs::MobEvent::HitPlayer(0, d, _) if *d >= 6.0))
        {
            mauled = true;
            break;
        }
    }
    assert!(
        mauled,
        "the polar bear charged an unprovoked player at noon"
    );
    // And it is wildlife, not warden: daylight never dissolves it.
    assert!(
        w.mobs().iter().any(|m| m.species == bear_si),
        "the bear persists in full daylight"
    );
}

#[test]
fn the_desperate_winter_wolf_sizes_you_up_and_breaks_off() {
    let reg = base_reg();
    let mut w = test_world_with("wolf", reg.clone());
    w.day = 3 * crate::world::SEASON_DAYS; // deep winter
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 20, 0, 20, h);
    let wolf_si = reg.animal_id("base:wolf").unwrap();
    let mut wolf = beast(&reg, "base:wolf", glam::Vec3::new(6.5, h as f32 + 1.0, 8.5));
    wolf.belly = -999.0; // long past empty
    w.spawn_mob(wolf);
    let player = glam::Vec3::new(13.5, h as f32 + 1.0, 8.5);
    let mut rng = 17u32;
    let mut bitten = false;
    for _ in 0..3000 {
        let evs = w.tick_mobs(&[ctx(player)], 0.1, 0.05, &mut rng);
        if evs
            .iter()
            .any(|e| matches!(e, crate::mobs::MobEvent::HitPlayer(0, _, _)))
        {
            bitten = true;
            break;
        }
    }
    assert!(
        bitten,
        "a desperate wolf on a winter night tries the player"
    );
    // Wounded, it breaks off: hungry, not suicidal.
    let def = reg.animals[wolf_si].clone();
    let wolf = w
        .mobs_mut()
        .iter_mut()
        .find(|m| m.species == wolf_si)
        .expect("the wolf");
    wolf.hurt(&def, 4.0, player);
    assert_eq!(
        wolf.state,
        crate::mobs::MobState::Flee,
        "a wounded wolf remembers it has options"
    );
    // A sated wolf on a summer day has no interest at all.
    let mut w2 = test_world_with("wolf-sated", reg.clone());
    let h2 = w2.surface_height(8, 8);
    pad(&mut w2, &reg, 0, 20, 0, 20, h2);
    let mut calm = beast(
        &reg,
        "base:wolf",
        glam::Vec3::new(6.5, h2 as f32 + 1.0, 8.5),
    );
    calm.belly = 9999.0;
    w2.spawn_mob(calm);
    let player2 = glam::Vec3::new(13.5, h2 as f32 + 1.0, 8.5);
    let mut rng2 = 19u32;
    for _ in 0..1200 {
        let evs = w2.tick_mobs(&[ctx(player2)], 1.0, 0.05, &mut rng2);
        assert!(
            !evs.iter()
                .any(|e| matches!(e, crate::mobs::MobEvent::HitPlayer(0, _, _))),
            "a sated summer wolf ignores everyone"
        );
    }
}

#[test]
fn the_vulture_beats_the_clock_and_rot_feeds_the_field() {
    let reg = base_reg();
    let mut w = test_world_with("vulture", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 20, 0, 20, h);
    let carcass_si = reg.animal_id("base:carcass").unwrap();
    let mut c = beast(
        &reg,
        "base:carcass",
        glam::Vec3::new(10.5, h as f32 + 1.0, 10.5),
    );
    c.rot = 9999.0; // the bird must beat a clock that hasn't run out
    w.spawn_mob(c);
    let mut v = beast(
        &reg,
        "base:vulture",
        glam::Vec3::new(14.5, h as f32 + 3.0, 10.5),
    );
    v.belly = -1.0;
    w.spawn_mob(v);
    let mut rng = 23u32;
    let mut cleaned = false;
    for _ in 0..4000 {
        w.tick_mobs(&[], 1.0, 0.05, &mut rng);
        if !w.mobs().iter().any(|m| m.species == carcass_si) {
            cleaned = true;
            break;
        }
    }
    assert!(cleaned, "the vulture found the carcass and left nothing");
    // Untouched rot pays the soil below instead.
    let mut w2 = test_world_with("rot", reg.clone());
    let h2 = w2.surface_height(8, 8);
    let farm = b(&reg, "base:farmland");
    w2.set_block_meta(8, h2, 8, farm, soil::soil_meta(10, 0));
    for dy in 1..4 {
        if w2.get_block(8, h2 + dy, 8) != AIR {
            w2.set_block(8, h2 + dy, 8, AIR);
        }
    }
    let mut c2 = beast(
        &reg,
        "base:carcass",
        glam::Vec3::new(8.5, h2 as f32 + 1.0, 8.5),
    );
    c2.rot = 0.5;
    w2.spawn_mob(c2);
    let mut rng2 = 29u32;
    for _ in 0..40 {
        w2.tick_mobs(&[], 1.0, 0.05, &mut rng2);
    }
    assert!(
        !w2.mobs().iter().any(|m| m.species == carcass_si),
        "the ground took the rest"
    );
    assert_eq!(
        soil::fert_of(w2.get_meta(8, h2, 8)),
        18,
        "and the field is richer for it"
    );
}

#[test]
fn fish_stay_wet_and_the_rod_takes_the_real_one_first() {
    let reg = base_reg();
    let mut w = test_world_with("pond", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 16, 0, 16, h);
    // A dug pond, three deep, filled with source water.
    let water = reg.water_block(0);
    let stone = b(&reg, "base:stone");
    for x in 5..=11 {
        for z in 5..=11 {
            for dy in 1..=3 {
                w.set_block(
                    x,
                    h - dy,
                    z,
                    if x == 5 || x == 11 || z == 5 || z == 11 {
                        stone
                    } else {
                        water
                    },
                );
            }
            w.set_block(x, h, z, AIR);
        }
    }
    // Seal the floor.
    for x in 5..=11 {
        for z in 5..=11 {
            w.set_block(x, h - 4, z, stone);
        }
    }
    let trout_si = reg.animal_id("base:trout").unwrap();
    let mut f = beast(
        &reg,
        "base:trout",
        glam::Vec3::new(8.5, h as f32 - 2.5, 8.5),
    );
    f.tamed = true; // the tag that finds our fish among strangers
    w.spawn_mob(f);
    let mut rng = 41u32;
    for _ in 0..1200 {
        w.tick_mobs(
            &[ctx(glam::Vec3::new(8.5, h as f32 + 1.0, 2.5))],
            1.0,
            0.05,
            &mut rng,
        );
    }
    let fish = w
        .mobs()
        .iter()
        .find(|m| m.tamed && m.species == trout_si)
        .expect("the trout persists near a player");
    let cell = (
        fish.pos.x.floor() as i32,
        (fish.pos.y + 0.2).floor() as i32,
        fish.pos.z.floor() as i32,
    );
    assert!(
        reg.is_water(w.get_block(cell.0, cell.1, cell.2)),
        "a minute later the trout is still swimming (at {:?})",
        fish.pos
    );
    // The rod: the real fish comes out before any luck table.
    let hooked = w.catch_fish_near(glam::Vec3::new(8.5, h as f32 - 2.0, 8.5), 6.0);
    assert_eq!(hooked, Some(trout_si), "the strike lands the trout");
    assert!(
        !w.mobs().iter().any(|m| m.tamed && m.species == trout_si),
        "and the water is emptier for it"
    );
    // Empty water gives the rod nothing real.
    assert_eq!(
        w.catch_fish_near(glam::Vec3::new(8.5, h as f32 - 2.0, 8.5), 6.0),
        None
    );
}

#[test]
fn the_heron_works_the_shallows() {
    let reg = base_reg();
    let mut w = test_world_with("heron", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 16, 0, 16, h);
    let water = reg.water_block(0);
    let stone = b(&reg, "base:stone");
    for x in 6..=10 {
        for z in 6..=10 {
            w.set_block(
                x,
                h - 1,
                z,
                if x == 6 || x == 10 || z == 6 || z == 10 {
                    stone
                } else {
                    water
                },
            );
            w.set_block(x, h, z, AIR);
        }
    }
    let trout_si = reg.animal_id("base:trout").unwrap();
    w.spawn_mob(beast(
        &reg,
        "base:trout",
        glam::Vec3::new(8.5, h as f32 - 0.5, 8.5),
    ));
    let mut hb = beast(
        &reg,
        "base:heron",
        glam::Vec3::new(12.5, h as f32 + 3.0, 8.5),
    );
    hb.belly = -1.0;
    w.spawn_mob(hb);
    let mut rng = 43u32;
    let mut taken = false;
    for _ in 0..3000 {
        w.tick_mobs(&[], 1.0, 0.05, &mut rng);
        if !w.mobs().iter().any(|m| m.species == trout_si) {
            taken = true;
            break;
        }
    }
    assert!(taken, "the heron speared the trout");
}

#[test]
fn the_crab_pinches_what_bothers_it() {
    let reg = base_reg();
    let mut w = test_world_with("crab", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 16, 0, 16, h);
    w.spawn_mob(beast(
        &reg,
        "base:crab",
        glam::Vec3::new(8.5, h as f32 + 1.0, 8.5),
    ));
    let player = glam::Vec3::new(9.3, h as f32 + 1.0, 8.5);
    let mut rng = 47u32;
    let mut pinched = false;
    for _ in 0..1200 {
        let evs = w.tick_mobs(&[ctx(player)], 1.0, 0.05, &mut rng);
        if evs
            .iter()
            .any(|e| matches!(e, crate::mobs::MobEvent::HitPlayer(0, d, _) if *d <= 1.5))
        {
            pinched = true;
            break;
        }
    }
    assert!(pinched, "stand on a crab, get pinched");
}
