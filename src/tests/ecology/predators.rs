//! Predators scenarios.

use super::*;

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
        if evs.iter().any(
            |e| matches!(e, crate::mobs::MobEvent::HitPlayer { who: 0, dmg: d, .. } if *d >= 6.0),
        ) {
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
    w.set_calendar_day(3 * crate::world::SEASON_DAYS); // deep winter
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
            .any(|e| matches!(e, crate::mobs::MobEvent::HitPlayer { who: 0, .. }))
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
    wolf.hurt(&def, 4.0, None, ep(player));
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
                .any(|e| matches!(e, crate::mobs::MobEvent::HitPlayer { who: 0, .. })),
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
fn a_starving_wolf_prefers_available_prey_to_a_player() {
    let reg = base_reg();
    let mut world = World::new(42, tmp_dir("wolf-available-prey"), reg.clone());
    world.insert_empty_chunks_for_test([tchunk(0, 0)]);
    pad(&mut world, &reg, 0, 15, 0, 15, 100);
    world.set_calendar_day(3 * crate::world::SEASON_DAYS);
    let mut wolf = beast(&reg, "base:wolf", Vec3::new(6.5, 101.0, 8.5));
    wolf.belly = crate::mobs::BELLY_DESPERATE - 100.0;
    world.spawn_mob(wolf);
    world.spawn_mob(beast(&reg, "base:deer", Vec3::new(12.5, 101.0, 8.5)));
    let events = world.tick_mobs(&[ctx(Vec3::new(7.0, 101.0, 8.5))], 0.1, 0.05, &mut 17);
    let wolf = world
        .mobs()
        .iter()
        .find(|mob| mob.species == reg.animal_id("base:wolf").unwrap())
        .unwrap();
    assert!(
        !wolf.bold,
        "nearby animal prey must prevent desperation attacks on players"
    );
    assert_eq!(
        wolf.state,
        crate::mobs::MobState::Stalk,
        "the wolf should stalk its animal prey"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, crate::mobs::MobEvent::HitPlayer { .. })),
        "a player beside the wolf was bitten despite an available deer"
    );
}
