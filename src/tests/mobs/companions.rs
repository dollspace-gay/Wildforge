//! Companions scenarios.

use super::*;

#[test]
fn breeding_makes_babies_that_grow() {
    let reg = base_reg();
    let mut w = test_world("breed");
    let deer_i = reg.animal_id("base:deer").unwrap();
    w.ire = 20.0;
    let before = w.mob_count();
    for x in [4.5f32, 6.5] {
        let mut m = crate::mobs::Mob::new(deer_i, Vec3::new(x, 220.0, 4.5), 0.0);
        m.health = 10.0;
        m.fed = true;
        w.spawn_mob(m);
    }
    let mut rng = 3u32;
    let events = w.tick_mobs(
        &[crate::server::PlayerCtx {
            id: 0,
            pos: ep(Vec3::new(200.0, 80.0, 200.0)),
            spawn: ep(Vec3::ZERO),
            attackable: true,
            aggro_mod: 0.0,
            quiet_charm: None,
        }],
        1.0,
        1.0 / 60.0,
        &mut rng,
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, crate::mobs::MobEvent::Bred)),
        "birth event"
    );
    assert_eq!(w.mob_count(), before + 3, "two parents + one baby");
    let baby = w
        .mobs()
        .iter()
        .find(|m| m.growth < 1.0)
        .expect("a baby exists");
    assert!(baby.growth < 0.1);
    assert!((w.ire - 19.0).abs() < 0.01, "a birth refunds 1 ire");
    let parents_fed = w.mobs().iter().filter(|m| m.fed).count();
    assert_eq!(parents_fed, 0, "parents spent their meal");
    // Growth advances with time; babies persist through saves.
    let baby_growth = baby.growth;
    for _ in 0..120 {
        w.tick_mobs(
            &[crate::server::PlayerCtx {
                id: 0,
                pos: ep(Vec3::new(200.0, 80.0, 200.0)),
                spawn: ep(Vec3::ZERO),
                attackable: true,
                aggro_mod: 0.0,
                quiet_charm: None,
            }],
            1.0,
            1.0 / 60.0,
            &mut rng,
        );
    }
    let baby2 = w
        .mobs()
        .iter()
        .find(|m| m.growth < 1.0)
        .expect("still young");
    assert!(baby2.growth > baby_growth, "babies grow");
    // No immediate re-breeding: cooldown holds.
    let n_now = w.mob_count();
    let ev2 = w.tick_mobs(
        &[crate::server::PlayerCtx {
            id: 0,
            pos: ep(Vec3::new(200.0, 80.0, 200.0)),
            spawn: ep(Vec3::ZERO),
            attackable: true,
            aggro_mod: 0.0,
            quiet_charm: None,
        }],
        1.0,
        1.0 / 60.0,
        &mut rng,
    );
    assert!(!ev2.iter().any(|e| matches!(e, crate::mobs::MobEvent::Bred)));
    assert_eq!(w.mob_count(), n_now);
}

#[test]
fn feeding_tames_and_tamed_animals_stand_their_ground() {
    let reg = base_reg();
    let deer_i = reg.animal_id("base:deer").unwrap();
    let mut wild = crate::mobs::Mob::new(deer_i, Vec3::new(8.5, 220.0, 8.5), 0.0);
    wild.id = 7;
    wild.health = 10.0;
    // Trust lands after the rolled number of meals (3-5).
    let mut meals = 0;
    while !wild.tamed {
        wild.feed_tame();
        meals += 1;
        assert!(meals <= 5, "taming lands within five meals");
    }
    assert!(meals >= 3, "taming takes at least three meals ({meals})");
    // A tamed deer holds its ground beside a player; a wild one bolts.
    let w = test_world("tame-flee");
    let def = &reg.animals[deer_i];
    let player = [crate::server::PlayerCtx {
        id: 0,
        pos: ep(Vec3::new(9.5, 220.0, 8.5)),
        spawn: ep(Vec3::ZERO),
        attackable: true,
        aggro_mod: 0.0,
        quiet_charm: None,
    }];
    let mut rng = 3u32;
    let mut events = Vec::new();
    wild.tick(&w, def, &player, 0.05, &mut rng, &mut events);
    assert_ne!(wild.state, crate::mobs::MobState::Flee, "tamed deer trusts");
    let mut skittish = crate::mobs::Mob::new(deer_i, Vec3::new(8.5, 220.0, 8.5), 0.0);
    skittish.health = 10.0;
    skittish.tick(&w, def, &player, 0.05, &mut rng, &mut events);
    assert_eq!(
        skittish.state,
        crate::mobs::MobState::Flee,
        "wild deer bolts"
    );
}

#[test]
fn led_animals_follow_and_leads_snap_at_range() {
    let reg = base_reg();
    let deer_i = reg.animal_id("base:deer").unwrap();
    let mut w = test_world("lead-follow");
    let stone = reg.block_id("base:stone").unwrap();
    for x in 2..=40 {
        for z in 6..=10 {
            w.set_block(x, 219, z, stone);
        }
    }
    let def = &reg.animals[deer_i];
    let mut m = crate::mobs::Mob::new(deer_i, Vec3::new(8.5, 220.0, 8.5), 0.0);
    m.health = 10.0;
    m.tamed = true;
    m.led_by = Some(0);
    let mut rng = 5u32;
    let mut events = Vec::new();
    let handler = |x: f32| {
        [crate::server::PlayerCtx {
            id: 0,
            pos: ep(Vec3::new(x, 220.0, 8.5)),
            spawn: ep(Vec3::ZERO),
            attackable: true,
            aggro_mod: 0.0,
            quiet_charm: None,
        }]
    };
    // Handler 7 blocks east: the deer walks after them.
    let x0 = m.pos.x;
    for _ in 0..30 {
        m.tick(&w, def, &handler(15.5), 0.05, &mut rng, &mut events);
    }
    assert!(
        m.pos.x > x0 + 0.2,
        "the deer follows its lead ({})",
        m.pos.x
    );
    assert!(m.led_by.is_some(), "the lead holds at range 7");
    // Handler far beyond the lead's reach: it snaps and drops.
    m.tick(&w, def, &handler(60.0), 0.05, &mut rng, &mut events);
    assert!(m.led_by.is_none(), "the lead snapped");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, crate::mobs::MobEvent::LeadSnapped(_))),
        "the snap dropped the strip"
    );
}
