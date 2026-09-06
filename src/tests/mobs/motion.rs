//! Motion scenarios.

use super::*;

#[test]
fn mob_settles_on_ground_and_flees_from_damage() {
    let reg = base_reg();
    let mut w = test_world("mobphys");
    let si = reg.animal_id("base:deer").unwrap();
    let def = reg.animals[si].clone();
    // Flat pad well above any terrain, high in the air.
    let stone = reg.block_id("base:stone").unwrap();
    for x in -6..=6 {
        for z in -6..=6 {
            w.set_block(x, 180, z, stone);
            for y in 181..=186 {
                w.set_block(x, y, z, AIR);
            }
        }
    }
    let mut m = crate::mobs::Mob::new(si, Vec3::new(0.5, 184.0, 0.5), 0.0);
    m.health = def.health;
    let mut rng = 7u32;
    for _ in 0..120 {
        m.tick(
            &w,
            &def,
            &[crate::server::PlayerCtx {
                id: 0,
                pos: ep(Vec3::new(100.0, 181.0, 100.0)),
                spawn: ep(Vec3::ZERO),
                attackable: true,
                aggro_mod: 0.0,
                quiet_charm: None,
            }],
            1.0 / 60.0,
            &mut rng,
            &mut Vec::new(),
        );
    }
    assert!(m.on_ground, "gravity settles the mob");
    assert!(
        (m.pos.y - 181.0).abs() < 0.3,
        "standing on the pad, got y={}",
        m.pos.y
    );

    // Damage from the east: it panics away, gaining distance from the threat.
    let threat = m.pos.translated(Vec3::new(2.0, 0.0, 0.0)).unwrap().pos;
    m.hurt(&def, 4.0, None, threat);
    assert_eq!(m.state, crate::mobs::MobState::Flee);
    assert!(m.health < def.health);
    let d0 = m.pos.distance_to(threat);
    for _ in 0..90 {
        m.tick(
            &w,
            &def,
            &[crate::server::PlayerCtx {
                id: 0,
                pos: ep(Vec3::new(100.0, 181.0, 100.0)),
                spawn: ep(Vec3::ZERO),
                attackable: true,
                aggro_mod: 0.0,
                quiet_charm: None,
            }],
            1.0 / 60.0,
            &mut rng,
            &mut Vec::new(),
        );
    }
    let d1 = (m.pos - threat).length();
    assert!(d1 > d0 + 1.0, "fled from the threat ({d0:.1} -> {d1:.1})");
    // Panic subsides back to idle within the flee timer.
    for _ in 0..400 {
        m.tick(
            &w,
            &def,
            &[crate::server::PlayerCtx {
                id: 0,
                pos: ep(Vec3::new(100.0, 181.0, 100.0)),
                spawn: ep(Vec3::ZERO),
                attackable: true,
                aggro_mod: 0.0,
                quiet_charm: None,
            }],
            1.0 / 60.0,
            &mut rng,
            &mut Vec::new(),
        );
    }
    assert_ne!(m.state, crate::mobs::MobState::Flee, "calmed down");
}

#[test]
fn skittish_flees_players_bold_does_not() {
    let reg = base_reg();
    let w = test_world("mobskit");
    let deer_i = reg.animal_id("base:deer").unwrap();
    let boar_i = reg.animal_id("base:boar").unwrap();
    let deer_def = reg.animals[deer_i].clone();
    let boar_def = reg.animals[boar_i].clone();
    let pos = Vec3::new(0.5, 120.0, 0.5);
    let player = pos + Vec3::new(4.0, 0.0, 0.0); // within deer flee_range (10)
    let mut deer = crate::mobs::Mob::new(deer_i, pos, 0.0);
    let mut boar = crate::mobs::Mob::new(boar_i, pos, 0.0);
    let mut rng = 3u32;
    deer.tick(
        &w,
        &deer_def,
        &[crate::server::PlayerCtx {
            id: 0,
            pos: ep(player),
            spawn: ep(Vec3::ZERO),
            attackable: true,
            aggro_mod: 0.0,
            quiet_charm: None,
        }],
        1.0 / 60.0,
        &mut rng,
        &mut Vec::new(),
    );
    boar.tick(
        &w,
        &boar_def,
        &[crate::server::PlayerCtx {
            id: 0,
            pos: ep(player),
            spawn: ep(Vec3::ZERO),
            attackable: true,
            aggro_mod: 0.0,
            quiet_charm: None,
        }],
        1.0 / 60.0,
        &mut rng,
        &mut Vec::new(),
    );
    assert_eq!(deer.state, crate::mobs::MobState::Flee, "deer spooks");
    assert_ne!(boar.state, crate::mobs::MobState::Flee, "boar doesn't care");
}

#[test]
fn wrathful_country_frays_the_wilds_nerves() {
    // Capability E12: at tier 3 the deer startles from half the distance —
    // a player inside the calm-country radius no longer spooks it.
    let reg = base_reg();
    let deer_i = reg.animal_id("base:deer").unwrap();
    let deer_def = reg.animals[deer_i].clone();
    let pos = Vec3::new(0.5, 120.0, 0.5);
    let player = pos + Vec3::new(7.5, 0.0, 0.0); // inside calm flee_range 10, outside tier-3's 5
    let player_ctx = crate::server::PlayerCtx {
        id: 0,
        pos: ep(player),
        spawn: ep(Vec3::ZERO),
        attackable: true,
        aggro_mod: 0.0,
        quiet_charm: None,
    };
    let mut rng = 3u32;
    // Calm country: the deer bolts.
    let w = test_world("e12-calm");
    let mut deer = crate::mobs::Mob::new(deer_i, pos, 0.0);
    deer.tick(
        &w,
        &deer_def,
        &[player_ctx],
        1.0 / 60.0,
        &mut rng,
        &mut Vec::new(),
    );
    assert_eq!(deer.state, crate::mobs::MobState::Flee);
    // Wrathful country: same geometry, frayed nerves hold.
    let mut w = test_world("e12-wrath");
    w.ire = 100.0;
    assert_eq!(w.ire_tier(), 3);
    let mut deer = crate::mobs::Mob::new(deer_i, pos, 0.0);
    deer.tick(
        &w,
        &deer_def,
        &[player_ctx],
        1.0 / 60.0,
        &mut rng,
        &mut Vec::new(),
    );
    assert_ne!(
        deer.state,
        crate::mobs::MobState::Flee,
        "wrathful country shortens the flight distance"
    );
}

#[test]
fn mob_ray_hit_works() {
    let reg = base_reg();
    let si = reg.animal_id("base:deer").unwrap();
    let def = &reg.animals[si];
    let m = crate::mobs::Mob::new(si, Vec3::new(10.0, 64.0, 10.0), 0.0);
    let origin = Vec3::new(10.0, 64.5, 6.0);
    let t = m
        .ray_hit(def, origin, Vec3::Z, 8.0)
        .expect("aimed ray hits");
    assert!(t > 2.0 && t < 5.0, "hit distance sane: {t}");
    assert!(
        m.ray_hit(def, origin, -Vec3::Z, 8.0).is_none(),
        "away ray misses"
    );
    assert!(
        m.ray_hit(def, origin, Vec3::Z, 2.0).is_none(),
        "out of reach"
    );
}

#[test]
fn mobs_freeze_in_unloaded_chunks_and_unstick_when_buried() {
    let reg = base_reg();
    let mut w = test_world("mobfreeze"); // chunks -2..=2 are loaded
    let si = reg.animal_id("base:deer").unwrap();
    // Regression: mobs outside loaded chunks used to fall through the
    // unloaded (all-air) world, then get buried when the chunk streamed in.
    let far_i = w.mob_count(); // test_world seeds natural wildlife too
    let mut far = crate::mobs::Mob::new(si, Vec3::new(500.5, 80.0, 500.5), 0.0);
    far.health = 10.0;
    w.spawn_mob(far);
    let mut rng = 1u32;
    for _ in 0..60 {
        w.tick_mobs(
            &[crate::server::PlayerCtx {
                id: 0,
                pos: ep(Vec3::ZERO),
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
    assert_eq!(
        w.mobs()[far_i].pos.y,
        80.0,
        "frozen, not falling, outside loaded chunks"
    );

    // A mob already wedged inside solid ground pops up to the surface.
    let stone = reg.block_id("base:stone").unwrap();
    for y in 100..=110 {
        for x in 0..4 {
            for z in 0..4 {
                w.set_block(x, y, z, stone);
            }
        }
    }
    let buried_i = w.mob_count();
    let mut buried = crate::mobs::Mob::new(si, Vec3::new(1.5, 104.0, 1.5), 0.0);
    buried.health = 10.0;
    w.spawn_mob(buried);
    w.tick_mobs(
        &[crate::server::PlayerCtx {
            id: 0,
            pos: ep(Vec3::new(60.0, 80.0, 60.0)),
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
        w.mobs()[buried_i].pos.y >= 110.5,
        "unstuck above the stone, got y={}",
        w.mobs()[buried_i].pos.y
    );
}
