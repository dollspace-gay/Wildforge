//! Hostiles scenarios.

use super::*;

#[test]
fn warden_hunts_strikes_and_caster_fires() {
    let reg = base_reg();
    let mut w = test_world("wardenhunt");
    let stone = reg.block_id("base:stone").unwrap();
    for x in -4..12 {
        for z in -4..12 {
            w.set_block(x, 150, z, stone);
            for y in 151..156 {
                w.set_block(x, y, z, AIR);
            }
        }
    }
    let ti = reg.animal_id("base:thornling").unwrap();
    let def = reg.animals[ti].clone();
    assert!(def.hostile && def.attack > 0.0);
    let player = Vec3::new(1.5, 151.0, 1.5);
    let mut m = crate::mobs::Mob::new(ti, Vec3::new(6.5, 151.0, 1.5), 0.0);
    m.health = def.health;
    let mut rng = 5u32;
    let mut events = Vec::new();
    m.tick(
        &w,
        &def,
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
        &mut events,
    );
    assert_eq!(m.state, crate::mobs::MobState::Hunt, "aggro within range");
    // Walk it onto the player: contact damage fires once, then cools down.
    m.pos = ep(player + Vec3::new(0.8, 0.0, 0.0));
    for _ in 0..30 {
        m.tick(
            &w,
            &def,
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
            &mut events,
        );
        m.pos = ep(player + Vec3::new(0.8, 0.0, 0.0));
    }
    let hits = events
        .iter()
        .filter(|e| matches!(e, crate::mobs::MobEvent::HitPlayer { .. }))
        .count();
    assert_eq!(hits, 1, "swing cooldown limits contact damage");
    // Creative players are invisible to the wild.
    let mut calm = crate::mobs::Mob::new(ti, Vec3::new(6.5, 151.0, 1.5), 0.0);
    calm.health = def.health;
    let mut ev2 = Vec::new();
    calm.tick(
        &w,
        &def,
        &[crate::server::PlayerCtx {
            id: 0,
            pos: ep(player),
            spawn: ep(Vec3::ZERO),
            attackable: false,
            aggro_mod: 0.0,
            quiet_charm: None,
        }],
        1.0 / 60.0,
        &mut rng,
        &mut ev2,
    );
    assert_ne!(
        calm.state,
        crate::mobs::MobState::Hunt,
        "no aggro when unattackable"
    );

    // Dryad: holds range and lobs a thorn bolt.
    let di = reg.animal_id("base:dryad").unwrap();
    let ddef = reg.animals[di].clone();
    let mut d = crate::mobs::Mob::new(di, Vec3::new(9.5, 151.0, 1.5), 0.0);
    d.health = ddef.health;
    let mut ev3 = Vec::new();
    for _ in 0..90 {
        d.tick(
            &w,
            &ddef,
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
            &mut ev3,
        );
    }
    assert!(
        ev3.iter()
            .any(|e| matches!(e, crate::mobs::MobEvent::Cast(_))),
        "caster fired"
    );
}

#[test]
fn floaters_hover_and_projectiles_collide() {
    let reg = base_reg();
    let mut w = test_world("floaty");
    let ei = reg.animal_id("base:emberkin").unwrap();
    let def = reg.animals[ei].clone();
    assert!(def.movement_float && def.emissive);
    let gy = w.surface_height(4, 4);
    let mut m = crate::mobs::Mob::new(ei, Vec3::new(4.5, gy as f32 + 3.0, 4.5), 0.0);
    m.health = def.health;
    let mut rng = 9u32;
    let far = Vec3::new(200.0, 80.0, 200.0);
    for _ in 0..240 {
        m.tick(
            &w,
            &def,
            &[crate::server::PlayerCtx {
                id: 0,
                pos: ep(far),
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
    let (under, ceiling) = w.air_column_at(m.pos, m.pos.y.floor() as i32);
    assert!(
        m.pos.y > under as f32 + 0.8 && m.pos.y < ceiling as f32,
        "wisp hovers in its current air column (y={} floor={under} ceiling={ceiling})",
        m.pos.y,
    );
    let _ = gy;

    // Projectile into a wall dies; into the player connects.
    let stone = reg.block_id("base:stone").unwrap();
    w.set_block(10, 200, 10, stone);
    let mut p = crate::mobs::Projectile {
        stable_id: 0,
        pos: ep(Vec3::new(10.5, 200.5, 7.0)),
        vel: Vec3::new(0.0, 0.0, 20.0),
        tile: 0,
        damage: 3.0,
        damage_type: None,
        age: 0.0,
        from_player: false,
        drop_item: None,
        preparation_payload: None,
        owner: 0,
    };
    let mut outcome = crate::mobs::ProjHit::None;
    for _ in 0..60 {
        outcome = p.tick(
            &w,
            &[crate::server::PlayerCtx {
                id: 0,
                pos: ep(far),
                spawn: ep(Vec3::ZERO),
                attackable: true,
                aggro_mod: 0.0,
                quiet_charm: None,
            }],
            1.0 / 30.0,
        );
        if !matches!(outcome, crate::mobs::ProjHit::None) {
            break;
        }
    }
    assert!(
        matches!(outcome, crate::mobs::ProjHit::Block),
        "bolt stopped by the wall"
    );
    w.spawn_projectile(crate::mobs::Projectile {
        stable_id: 0,
        pos: ep(Vec3::new(4.5, 120.9, 2.0)),
        vel: Vec3::new(0.0, 0.0, 12.0),
        tile: 0,
        damage: 3.0,
        damage_type: None,
        age: 0.0,
        from_player: false,
        drop_item: None,
        preparation_payload: None,
        owner: 0,
    });
    let mut dmg = 0.0;
    for _ in 0..60 {
        dmg += w
            .tick_projectiles(
                &[crate::server::PlayerCtx {
                    id: 0,
                    pos: ep(Vec3::new(4.5, 120.0, 4.5)),
                    spawn: ep(Vec3::ZERO),
                    attackable: true,
                    aggro_mod: 0.0,
                    quiet_charm: None,
                }],
                1.0 / 30.0,
            )
            .iter()
            .map(|(_, d, _)| d)
            .sum::<f32>();
    }
    assert_eq!(dmg, 3.0, "bolt connected with the player");
}

#[test]
fn spawner_respects_darkness_ire_and_tiers() {
    let reg = base_reg();
    let mut w = test_world("wardenspawn");
    let player = ep(Vec3::new(8.0, (w.surface_height(8, 8) + 1) as f32, 8.0));
    let world_spawn = ep(Vec3::new(-500.0, 70.0, -500.0)); // far away, no exclusion
    let mut rng = 77u32;
    // Daytime: surface spawns are impossible (only underground wardens may
    // appear, if a cave pocket is found).
    for _ in 0..200 {
        w.tick_hostile_spawns(player, world_spawn, 1.0, 5.0, &mut rng);
    }
    for m in w.mobs() {
        let d = &reg.animals[m.species];
        if d.hostile {
            assert!(
                d.biomes.iter().any(|b| b == "underground"),
                "daytime surface spawn of {}",
                d.name
            );
        }
    }
    // Night at Calm: spawns only ire_min = 0 wardens, within the ring.
    w.replace_mobs(
        w.mobs()
            .iter()
            .filter(|m| !reg.animals[m.species].hostile)
            .cloned()
            .collect(),
    );
    for _ in 0..300 {
        w.tick_hostile_spawns(player, world_spawn, 0.12, 5.0, &mut rng);
    }
    let hostiles: Vec<_> = w
        .mobs()
        .iter()
        .filter(|m| reg.animals[m.species].hostile)
        .collect();
    assert!(
        hostiles.len() <= 2,
        "calm budget respected: {}",
        hostiles.len()
    );
    for m in &hostiles {
        let d = &reg.animals[m.species];
        assert_eq!(d.ire_min, 0.0, "no provoked-tier wardens at calm");
        let dist = m.pos.distance_to(player);
        assert!((20.0..90.0).contains(&dist), "ring distance {dist}");
    }
    // Wrathful: higher budget, elites allowed.
    w.ire = 95.0;
    for _ in 0..300 {
        w.tick_hostile_spawns(player, world_spawn, 0.12, 5.0, &mut rng);
    }
    let n = w
        .mobs()
        .iter()
        .filter(|m| reg.animals[m.species].hostile)
        .count();
    assert!(n > 2, "wrathful nights are busier: {n}");
}

#[test]
fn wardens_dissolve_at_dawn_and_never_save() {
    let reg = base_reg();
    let dir = tmp_dir("wardensave");
    let mut w = World::new(21, dir.clone(), reg.clone());
    w.ensure_chunk(tchunk(0, 0));
    let ti = reg.animal_id("base:thornling").unwrap();
    let deer_i = reg.animal_id("base:deer").unwrap();
    let y = w.surface_height(4, 4) as f32 + 1.0;
    for (si, x) in [(ti, 4.5f32), (deer_i, 6.5)] {
        let mut m = crate::mobs::Mob::new(si, Vec3::new(x, y, 4.5), 0.0);
        m.health = reg.animals[si].health;
        w.spawn_mob(m);
    }
    // Never persisted.
    save_world(&mut w);
    let w2 = World::load_or_create(dir, reg.clone()).unwrap();
    assert!(
        w2.mobs().iter().all(|mob| mob.species != ti),
        "wardens never survive a save"
    );
    assert!(
        w2.mobs().iter().any(|mob| mob.species == deer_i),
        "ordinary wildlife survives"
    );
    // Dawn dissolve: full daylight on an open surface removes the warden.
    w.set_simulation_clock(0.25 * f64::from(crate::server::DAY_LENGTH));
    let player = Vec3::new(5.0, y, 5.0);
    let mut rng = 3u32;
    w.tick_mobs(
        &[crate::server::PlayerCtx {
            id: 0,
            pos: ep(player),
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
        !w.mobs().iter().any(|m| reg.animals[m.species].hostile),
        "warden dissolved in daylight"
    );
    assert!(
        w.mobs().iter().any(|m| m.species == deer_i),
        "the deer does not dissolve"
    );
}

#[test]
fn warden_current_balances_manifestation_dissolution_death_drops_and_heart_death() {
    let reg = base_reg();
    let dir = tmp_dir("warden-current-lifecycle");
    let atlas = std::sync::Arc::new(crate::planet_atlas::PlanetAtlas::fixture(8_799, 16).unwrap());
    atlas.write_new(&dir).unwrap();
    let country = atlas.biomes.countries.first().expect("fixture country");
    let point = country.heart_site.center(atlas.side());
    let surface =
        crate::planet::SurfacePos::new(point.face, point.u.floor() as u16, point.v.floor() as u16)
            .unwrap();
    let mut world = World::new_with_atlas(8_799, dir, reg.clone(), atlas.clone());
    world.ensure_chunk(ChunkPos::from_surface(surface));
    let y = world.surface_height_at(surface) + 1;
    let pos = crate::planet::EntityPos::new(
        surface.face(),
        f32::from(surface.u()) + 0.5,
        y as f32,
        f32::from(surface.v()) + 0.5,
    )
    .unwrap();
    let species = reg.animal_id("base:thornling").unwrap();
    let capacity = reg.animals[species].arcane.as_ref().unwrap().capacity;
    let heart = crate::arcane::ArcaneOwner::Heart(country.id);
    let heart_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .account(&heart)
        .unwrap()
        .current
        .total();

    let manifest = |world: &mut World| {
        let before = world.mob_count();
        let mut mob = crate::mobs::Mob::new_at(species, pos, 0.0);
        mob.health = reg.animals[species].health;
        world.spawn_mob(mob);
        (world.mob_count() > before).then(|| world.mobs().last().unwrap().id)
    };
    let first = manifest(&mut world).expect("heart funded first manifestation");
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert_eq!(
        ledger
            .account(&crate::arcane::ArcaneOwner::Mob(u64::from(first)))
            .unwrap()
            .current
            .total(),
        capacity
    );
    assert_eq!(
        ledger.account(&heart).unwrap().current.total(),
        heart_before - capacity
    );

    // With no nearby player, the ordinary retirement path dissolves the
    // temporary manifestation and returns its full loan.
    let mut rng = 91u32;
    world.tick_mobs(&[], 1.0, 1.0 / 60.0, &mut rng);
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert!(
        ledger
            .account(&crate::arcane::ArcaneOwner::Mob(u64::from(first)))
            .is_none()
    );
    assert_eq!(
        ledger.account(&heart).unwrap().current.total(),
        heart_before
    );

    let second = manifest(&mut world).expect("heart funded second manifestation");
    let mob = world.mob_by_id_mut(second).unwrap();
    mob.health = 0.0;
    mob.growth = 1.0;
    let deaths = world.settle_dead_mobs(&mut rng);
    assert_eq!(deaths.len(), 1);
    let drops = world.take_pending_drops();
    assert!(!drops.is_empty());
    assert!(drops.iter().all(|(_, stack)| stack.arcane_id != 0));
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert!(
        ledger
            .account(&crate::arcane::ArcaneOwner::Mob(u64::from(second)))
            .is_none()
    );
    for (_, stack) in &drops {
        assert!(
            ledger
                .account(&crate::arcane::ArcaneOwner::Item(stack.arcane_id))
                .is_some()
        );
    }
    assert!(ledger.audit().unwrap().is_balanced());

    // The generated heart's death freezes the remaining reserve. A new
    // manifestation is rejected instead of silently borrowing from Deep.
    let province = world.generator.province_at(surface).key;
    assert!(world.heart_at_surface(surface).is_some());
    world.set_heart_stage(province, 0);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .heart_frozen(country.id)
    );
    let count = world.mob_count();
    assert!(manifest(&mut world).is_none());
    assert_eq!(world.mob_count(), count);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .is_balanced()
    );
}
