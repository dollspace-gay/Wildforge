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
            w.ensure_chunk(ChunkPos { x: cx, z: cz });
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
fn lightning_strikes_only_what_was_always_wild() {
    let reg = base_reg();
    let mut w = test_world_with("bolt", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 16, 0, 16, h);
    let charred = b(&reg, "base:charred_soil");
    // Natural country takes the strike: char plus banked bloom.
    let hit = w.lightning_strike(8, 8);
    assert!(hit.is_some(), "the bolt lands on wild ground");
    let (sx, sy, sz) = hit.unwrap();
    assert_eq!(w.get_block(sx, sy, sz), charred, "the ground is scorched");
    assert!(w.bloom_at(8, 8) > 0.0, "and the bloom is banked");
    // Touched country is off the target list, absolutely.
    let mut wt = test_world_with("bolt-touched", reg.clone());
    let ht = wt.surface_height(8, 8);
    pad(&mut wt, &reg, 0, 16, 0, 16, ht);
    wt.player_touched.insert((0, 0));
    assert!(
        wt.lightning_strike(8, 8).is_none(),
        "the wild never touches what players built"
    );
    // And the scorch tills into the richest field in the game.
    let meta = crate::world::soil::soil_meta(crate::world::soil::FERT_MAX, 0);
    assert_eq!(
        crate::world::soil::fert_of(meta),
        crate::world::soil::FERT_MAX
    );
}

#[test]
fn the_bloom_erupts_and_burns_down() {
    let reg = base_reg();
    let mut w = test_world_with("bloom", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 24, 0, 24, h);
    // A parent log so the green tide has a species to seed.
    let log = b(&reg, "base:log");
    for dy in 1..=3 {
        w.set_block(12, h + dy, 12, log);
    }
    w.add_bloom(8, 8, 3.0);
    assert!(w.bloom_at(8, 8) > 0.0);
    let flowers = [b(&reg, "base:meadow_bloom"), b(&reg, "base:ember_poppy")];
    let mut rng = 71u32;
    let mut bloomed = 0;
    for _ in 0..8000 {
        w.random_tick(&mut rng);
        bloomed = (0..=24)
            .flat_map(|x| (0..=24).map(move |z| (x, z)))
            .filter(|&(x, z)| flowers.contains(&w.get_block(x, h + 1, z)))
            .count();
        if bloomed >= 3 {
            break;
        }
    }
    assert!(
        bloomed >= 3,
        "the charged cell erupts in flowers ({bloomed})"
    );
    // The charge burns down day by day and the ledger survives a save.
    let before = w.bloom_at(8, 8);
    w.tick_ire(1.0);
    assert!(w.bloom_at(8, 8) < before, "blooms fade");
    // Persistence round-trip.
    let dir = tmp_dir("bloom-save");
    let mut w2 = World::new(9, dir.clone(), reg.clone());
    w2.ensure_chunk(ChunkPos { x: 0, z: 0 });
    w2.add_bloom(40, 40, 2.5);
    w2.save_modified();
    let w3 = World::load_or_create(dir, reg.clone());
    assert!(w3.bloom_at(40, 40) > 2.0, "the bloom ledger persists");
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

/// The sea used to be empty. Fish shared the single per-chunk wildlife
/// slot with the landfolk and lost it to whatever was defined earlier
/// in the file — and in an ocean chunk that winner then failed to place
/// on dry ground, so the chunk stocked nothing at all.
#[test]
fn the_sea_keeps_its_own_life() {
    let reg = base_reg();
    let mut w = test_world_with("sea-roster", reg.clone());
    let mut open_water = 0;
    for cx in -14..14 {
        for cz in -14..14 {
            w.ensure_chunk(ChunkPos { x: cx, z: cz });
            if w.is_open_water(cx * 16 + 8, cz * 16 + 8) {
                open_water += 1;
            }
        }
    }
    assert!(open_water > 0, "this sweep has to contain some sea");
    let count = |name: &str| {
        let si = reg.animal_id(name).unwrap();
        w.mobs().iter().filter(|m| m.species == si).count()
    };
    // Salt water: its own natives, below the surface and over it.
    assert!(count("base:cod") > 0, "cod in the sea");
    assert!(count("base:mackerel") > 0, "mackerel in the sea");
    assert!(
        count("base:gull") > 0,
        "gulls over it — a flier needs no ground"
    );
    // Fresh water still stocks the country's own fish, so the new
    // roster did not simply replace the old one.
    assert!(count("base:trout") > 0, "trout still in cold fresh water");
    // Every fish is actually IN water, not flopping on a hill.
    for m in w.mobs() {
        if !reg.animals[m.species].movement_swim {
            continue;
        }
        let at = w.get_block(
            m.pos.x.floor() as i32,
            m.pos.y.floor() as i32,
            m.pos.z.floor() as i32,
        );
        assert!(
            reg.is_water(at),
            "{} spawned out of water at {:?}",
            reg.animals[m.species].name,
            m.pos
        );
    }
}

/// A drowned column is Ocean whatever its country grows. The country
/// itself keeps its culture — the heart of a coastal forest province
/// still knows it is a forest.
#[test]
fn open_water_reads_as_ocean_not_as_the_coast_behind_it() {
    use crate::worldgen::Biome;
    let reg = base_reg();
    let mut w = test_world_with("ocean-label", reg.clone());
    let (mut sea, mut land) = (None, None);
    for cx in -14..14 {
        for cz in -14..14 {
            w.ensure_chunk(ChunkPos { x: cx, z: cz });
            let (x, z) = (cx * 16 + 8, cz * 16 + 8);
            if w.is_open_water(x, z) {
                sea = sea.or(Some((x, z)));
            } else {
                land = land.or(Some((x, z)));
            }
        }
    }
    let (sx, sz) = sea.expect("some sea in this sweep");
    let (lx, lz) = land.expect("some land in this sweep");
    assert_eq!(w.biome_here(sx, sz), Biome::Ocean, "you are in the sea");
    assert_ne!(
        w.country_biome(sx, sz),
        Biome::Ocean,
        "the country it belongs to is still a country"
    );
    assert_eq!(
        w.biome_here(lx, lz),
        w.country_biome(lx, lz),
        "on dry ground the two agree"
    );
    // Nothing classifies AS ocean: it is not a climate, so no province
    // can be labelled with it and no centroid can claim it.
    for x in (-200..200).step_by(37) {
        assert_ne!(w.generator.biome(x, x * 3), Biome::Ocean);
    }
}

/// A flier's cruise height is measured from the ground *under it*.
/// Reading the world's surface height instead told a bat thirty blocks
/// underground to climb into daylight, so it spent its whole life
/// pressed into the cave roof — which from below looks exactly like a
/// bat hovering in one spot doing nothing.
#[test]
fn a_flier_cruises_over_the_floor_beneath_it_not_the_worlds_surface() {
    let reg = base_reg();
    let mut w = test_world_with("cave-cruise", reg.clone());
    let stone = b(&reg, "base:stone");
    // A tall hall, deep down: floor at 10, roof at 34, so the cruise
    // height falls comfortably inside it.
    for x in 2..=14 {
        for z in 2..=14 {
            for y in [10, 34] {
                w.set_block(x, y, z, stone);
            }
            for y in 11..34 {
                w.set_block(x, y, z, AIR);
            }
        }
    }
    let bat_si = reg.animal_id("base:bat").unwrap();
    w.spawn_mob(beast(&reg, "base:bat", glam::Vec3::new(8.5, 12.0, 8.5)));
    let mut rng = 61u32;
    for _ in 0..400 {
        w.tick_mobs(&[], 0.0, 0.05, &mut rng);
    }
    let bat = w.mobs().iter().find(|m| m.species == bat_si).expect("bat");
    assert!(
        bat.pos.y > 14.0,
        "it climbed off the cave floor (y {:.1})",
        bat.pos.y
    );
    assert!(
        bat.pos.y < 32.0,
        "and did not grind itself against the roof (y {:.1})",
        bat.pos.y
    );
}

/// Wings do not loiter. Before this a flier idled in mid-air for
/// seconds at a time, wandered at a grazer's amble, and read open water
/// as a landfolk's wall — so a gull over the sea turned back at once
/// and hung there. All three showed up as "it just hovers".
#[test]
fn a_flier_crosses_ground_instead_of_hanging_in_the_air() {
    let reg = base_reg();
    let mut w = test_world_with("gull-cruise", reg.clone());
    let water = reg.water_block(0);
    let stone = b(&reg, "base:stone");
    // A little sea to cross: the shoreline veto is what pinned it.
    for x in -8..=8 {
        for z in -8..=8 {
            w.set_block(x, SEA_LEVEL - 2, z, stone);
            w.set_block(x, SEA_LEVEL - 1, z, water);
            w.set_block(x, SEA_LEVEL, z, water);
        }
    }
    let gull_si = reg.animal_id("base:gull").unwrap();
    let start = glam::Vec3::new(0.5, SEA_LEVEL as f32 + 6.0, 0.5);
    w.spawn_mob(beast(&reg, "base:gull", start));
    let mut rng = 67u32;
    let mut travelled = 0.0f32;
    let mut last = start;
    for _ in 0..600 {
        w.tick_mobs(&[], 1.0, 0.05, &mut rng);
        let g = w.mobs().iter().find(|m| m.species == gull_si).unwrap();
        travelled += (g.pos - last).length();
        last = g.pos;
    }
    // Thirty seconds of flight. A cruising gull covers well over a
    // block a second; the old idle-heavy wander barely managed a third
    // of this even when it wasn't stalled at the water's edge.
    assert!(
        travelled > 40.0,
        "the gull actually flew somewhere ({travelled:.1} blocks in 30s)"
    );
}

/// The wings had no animation at all: only a box literally named "leg"
/// was ever rotated, so every bird in the game glided with its wings
/// nailed out flat.
#[test]
fn wings_beat_and_wingless_animals_hold_still() {
    let reg = base_reg();
    let lum = ([1.0f32; 3], 1.0f32);
    let frame = |name: &str, phase: f32| {
        let mut m = beast(&reg, name, glam::Vec3::ZERO);
        m.anim_phase = phase;
        let (mut v, mut i) = (Vec::new(), Vec::new());
        m.emit(&reg, lum, &mut v, &mut i);
        v.iter().map(|x| x.pos).collect::<Vec<_>>()
    };
    let down = frame("base:gull", 0.0);
    let up = frame("base:gull", std::f32::consts::FRAC_PI_2);
    assert_eq!(down.len(), up.len(), "same model, same box count");
    assert!(
        down.iter().zip(&up).any(|(a, b)| a != b),
        "the wing moved between the top and the bottom of the beat"
    );
    // A standing animal with no wings is identical at any phase, so
    // the difference above is the beat and not the gait.
    let a = frame("base:deer", 0.0);
    let b2 = frame("base:deer", std::f32::consts::FRAC_PI_2);
    assert_eq!(a, b2, "a standing deer does not animate");
}
