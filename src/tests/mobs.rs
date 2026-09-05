//! Wildlife, hostile mobs, breeding, persistence, and projectile behavior.

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
fn wildlife_seeds_matching_biomes_only() {
    let reg = base_reg();
    let mut w = test_world_with("mobseed", reg.clone());
    // Sweep a wide area; every spawned mob must belong to its chunk's biome.
    for cx in -12..12 {
        for cz in -12..12 {
            w.ensure_chunk(tchunk(cx, cz));
        }
    }
    for m in w.mobs() {
        let def = &reg.animals[m.species];
        // The dark rolls independently of the surface: bats belong
        // to whatever chunk has a cave, not to its biome.
        if def.biomes.iter().any(|b| b == "underground") {
            continue;
        }
        // The group roll uses the chunk-center biome; members may scatter a
        // few blocks over a biome edge, which is fine.
        let cp = ChunkPos::of_world(m.pos.x.floor() as i32, m.pos.z.floor() as i32);
        let country = w
            .generator
            .biome(cp.centered_u() * 16 + 8, cp.centered_v() * 16 + 8)
            .name()
            .to_lowercase();
        // The water rolls on its own key, so salt-water natives are
        // checked against the sea and not against the coast behind it.
        let ocean = "ocean".to_string();
        let ok = def.biomes.contains(&country)
            || (def.biomes.contains(&ocean)
                && w.is_open_water(cp.centered_u() * 16 + 8, cp.centered_v() * 16 + 8));
        assert!(ok, "{} rolled in {country} chunk", def.name);
        assert!(m.health > 0.0, "spawned alive");
    }
    assert!(w.mob_count() <= crate::world::MOB_CAP);
}

#[test]
fn mob_persistence_round_trips_and_skips_unknown() {
    let reg = base_reg();
    let dir = tmp_dir("mobsave");
    let mut w = World::new(11, dir.clone(), reg.clone());
    let si = reg.animal_id("base:goat").unwrap();
    let mut m = crate::mobs::Mob::new(si, Vec3::new(3.5, 90.0, -2.5), 1.25);
    m.health = 7.0;
    w.spawn_mob(m);
    save_world(&mut w);
    // Unknown species entries (removed mod) skip cleanly on load.
    let extra = "\n[[mob]]\nspecies = \"gone:wolf\"\nface = 4\nu = 4096.0\ny = 80.0\nv = 4096.0\nyaw = 0\nhealth = 5\n";
    let path = dir.join("animals.toml");
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str(extra);
    std::fs::write(&path, text).unwrap();

    let w2 = World::load_or_create(dir, reg.clone()).unwrap();
    assert_eq!(w2.mob_count(), 1, "goat loaded, unknown skipped");
    let g = &w2.mobs()[0];
    assert_eq!(g.species, si);
    assert_eq!(g.health, 7.0);
    assert!((g.pos - Vec3::new(3.5, 90.0, -2.5)).length() < 0.01);
    assert!((g.yaw - 1.25).abs() < 0.01);
}

#[test]
fn npc_companion_persistence_round_trips() {
    let reg = base_reg();
    let dir = tmp_dir("npcsave");
    let mut w = World::new(13, dir.clone(), reg.clone());
    // Spawn the elder via the normal seam; save and reload.
    let def_idx = reg.npc_id("base:elder").expect("base elder registers");
    let at = ep(Vec3::new(3.5, 90.0, -2.5));
    let mob_id = w.spawn_npc_at(def_idx, at).expect("npc fits caps");
    assert!(
        w.npc_by_mob(mob_id).is_some(),
        "instance exists after spawn"
    );
    save_world(&mut w);

    let w2 = World::load_or_create(dir, reg.clone()).unwrap();
    assert_eq!(w2.npc_count(), 1, "npc instance restored on load");
    let npc = w2.npcs().first().expect("restored instance");
    assert_eq!(
        npc.mob_id, mob_id,
        "instance links to the same companion mob"
    );
    assert_eq!(npc.def, def_idx);
    assert_eq!(npc.dialogue.as_deref(), Some("base:elder"));
    let companion = w2
        .mobs()
        .iter()
        .find(|m| m.id == npc.mob_id)
        .expect("companion mob restored");
    assert_eq!(companion.species, reg.npcs[def_idx].species);
    assert!((companion.pos - Vec3::new(3.5, 90.0, -2.5)).length() < 0.01);
}

#[test]
fn wildlife_seed_marks_persist() {
    let reg = base_reg();
    let dir = tmp_dir("mobmark");
    let mut w = World::new(5, dir.clone(), reg.clone());
    w.ensure_chunk(tchunk(0, 0));
    let first = w
        .mobs()
        .iter()
        .filter(|m| {
            let def = &reg.animals[m.species];
            !def.hostile && !def.movement_swim
        })
        .count();
    save_world(&mut w);
    // Reload: regenerating the same chunk must NOT reroll wildlife.
    let mut w2 = World::load_or_create(dir, reg).unwrap();
    w2.ensure_chunk(tchunk(0, 0));
    assert_eq!(
        w2.mob_count(),
        first,
        "seeded mark survives; saved land wildlife is not duplicated on revisit"
    );
}

#[test]
fn mod_can_add_species() {
    let root = tmp_dir("modanimal");
    let dir = root.join("fauna");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"fauna\"\nworld_api = 2\n").unwrap();
    std::fs::write(
        dir.join("animals.toml"),
        r#"
[[animal]]
id = "shadow_cat"
biomes = ["forest"]
health = 6
tex = "@deer"
drops = [{ item = "base:hide", min = 1, max = 1 }]
"#,
    )
    .unwrap();
    let reg = registry::load(&root);
    let si = reg
        .animal_id("fauna:shadow_cat")
        .expect("mod species registers");
    let def = &reg.animals[si];
    assert!(!def.model.is_empty(), "default model filled in");
    assert_eq!(def.drops.len(), 1, "drop resolved to base:hide");
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
fn a_hostile_mob_pursues_across_every_planet_seam() {
    use crate::planet::{BlockPos, Direction4, EntityPos, FACE_BLOCKS, Face, SurfacePos};

    fn fixture_set(world: &mut World, pos: BlockPos, block: crate::registry::BlockId) {
        let (x, y, z) = pos.local();
        world
            .chunks_mut()
            .get_mut(&pos.chunk())
            .expect("the pursuit fixture installs every touched chunk")
            .set(x, y, z, block);
    }

    let reg = base_reg();
    let mut world = World::new(50, tmp_dir("planet-mob-all-seams"), reg.clone());
    let stone = b(&reg, "base:stone");
    let species = reg.animal_id("base:thornling").unwrap();
    let def = reg.animals[species].clone();
    let side = f32::from(FACE_BLOCKS);

    for (edge_index, (face, direction)) in Face::ALL
        .into_iter()
        .flat_map(|face| Direction4::ALL.map(move |direction| (face, direction)))
        .enumerate()
    {
        let varying = 360 + edge_index as u16 * 300;
        let (u, v, chase, cell_u, cell_v, du, dv, pu, pv) = match direction {
            Direction4::East => (
                side - 1.2,
                f32::from(varying) + 0.5,
                Vec3::X,
                i32::from(FACE_BLOCKS) - 1,
                i32::from(varying),
                1,
                0,
                0,
                1,
            ),
            Direction4::North => (
                f32::from(varying) + 0.5,
                side - 1.2,
                Vec3::Z,
                i32::from(varying),
                i32::from(FACE_BLOCKS) - 1,
                0,
                1,
                1,
                0,
            ),
            Direction4::West => (
                1.2,
                f32::from(varying) + 0.5,
                Vec3::NEG_X,
                0,
                i32::from(varying),
                -1,
                0,
                0,
                1,
            ),
            Direction4::South => (
                f32::from(varying) + 0.5,
                1.2,
                Vec3::NEG_Z,
                i32::from(varying),
                0,
                0,
                -1,
                1,
                0,
            ),
        };
        let mut lane = Vec::new();
        for along in -3..=4 {
            for across in -1..=1 {
                lane.push(
                    SurfacePos::canonicalized(
                        face,
                        cell_u + along * du + across * pu,
                        cell_v + along * dv + across * pv,
                    )
                    .unwrap(),
                );
            }
        }
        let chunks: std::collections::BTreeSet<_> = lane
            .iter()
            .map(|surface| crate::planet::ChunkPos::from_surface(*surface))
            .filter(|chunk| !world.has_chunk(*chunk))
            .collect();
        world.insert_empty_chunks_for_test(chunks);
        for surface in lane {
            fixture_set(
                &mut world,
                BlockPos::new(surface.face(), surface.u(), 99, surface.v()).unwrap(),
                stone,
            );
            for y in 100..=103 {
                fixture_set(
                    &mut world,
                    BlockPos::new(surface.face(), surface.u(), y, surface.v()).unwrap(),
                    AIR,
                );
            }
        }

        let start = EntityPos::new(face, u, 100.0, v).unwrap();
        let player = start.translated(chase * 3.0).unwrap().pos;
        assert_ne!(face, player.face());
        let players = [crate::server::PlayerCtx {
            id: 0,
            pos: player,
            spawn: player,
            attackable: true,
            aggro_mod: 0.0,
            quiet_charm: None,
        }];
        let mut mob = crate::mobs::Mob::new_at(species, start, 0.0);
        mob.health = def.health;
        mob.on_ground = true;
        let mut rng = edge_index as u32 + 1;
        let mut events = Vec::new();
        let mut crossed = false;
        for _ in 0..120 {
            mob.tick(&world, &def, &players, 0.05, &mut rng, &mut events);
            crossed |= mob.pos.face() != face;
            if crossed && mob.pos.distance_to(player) < 1.5 {
                break;
            }
        }
        assert!(
            crossed,
            "thornling did not pursue across {face:?} {direction:?}; stopped at {:?}",
            mob.pos
        );
        assert!(mob.pos.is_canonical());
    }
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
    w.mobs_mut().retain(|m| !reg.animals[m.species].hostile);
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
    w.clock = 0.25 * f64::from(crate::server::DAY_LENGTH);
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
fn mob_ids_stamped_unique_and_yaw_lerps_short_arc() {
    // Interpolation turns the short way around the circle.
    let y = crate::mobs::lerp_yaw(0.1, std::f32::consts::TAU - 0.1, 0.5);
    assert!(
        y.abs() < 0.01 || (y - std::f32::consts::TAU).abs() < 0.01,
        "short arc, got {y}"
    );

    let reg = base_reg();
    let mut w = test_world_with("mobids", reg.clone());
    let wild = reg
        .animals
        .iter()
        .position(|a| !a.hostile)
        .expect("wildlife exists");
    let sy = w.surface_height(4, 4) as f32 + 1.0;
    for i in 0..3 {
        let mut m = crate::mobs::Mob::new(wild, Vec3::new(4.5 + i as f32, sy, 4.5), 0.0);
        m.health = 5.0;
        w.spawn_mob(m);
    }
    let mut rng = 1u32;
    w.tick_mobs(&[], 1.0, 0.05, &mut rng);
    assert!(
        w.mobs().iter().all(|m| m.id > 0),
        "every mob stamped with an id"
    );
    let mut ids: Vec<u32> = w.mobs().iter().map(|m| m.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), w.mob_count(), "ids unique");
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

#[test]
fn saddlebag_cargo_survives_save_and_load() {
    let reg = base_reg();
    let dir = tmp_dir("packmule");
    let deer_i = reg.animal_id("base:deer").unwrap();
    let salt = it(&reg, "base:salt_crystal");
    {
        let mut w = World::new(42, dir.clone(), reg.clone());
        let mut m = crate::mobs::Mob::new(deer_i, Vec3::new(8.5, 220.0, 8.5), 0.0);
        m.health = reg.animals[deer_i].health;
        m.tamed = true;
        m.tame_fed = 4;
        m.tame_need = 4;
        let mut cargo: Box<[Option<ItemStack>; 12]> = Default::default();
        cargo[0] = Some(ItemStack::new(&reg, salt, 30));
        cargo[11] = Some(ItemStack::new(&reg, salt, 2));
        m.cargo = Some(cargo);
        w.spawn_mob(m);
        save_world(&mut w);
    }
    let w = World::load_or_create(dir, reg.clone()).unwrap();
    let m = w
        .mobs()
        .iter()
        .find(|m| m.tamed)
        .expect("the pack deer came back");
    let cargo = m.cargo.as_ref().expect("with its saddlebags");
    assert_eq!(cargo[0].unwrap().count, 30, "the salt rode through");
    assert_eq!(cargo[11].unwrap().count, 2);
    assert_eq!(m.tame_need, 4, "taming state persists");
}

#[test]
fn boats_float_carry_cargo_and_wreck_into_salvage() {
    let reg = base_reg();
    let boat_i = reg.animal_id("base:boat").unwrap();
    assert!(reg.animals[boat_i].vehicle, "the boat is a vehicle");
    assert!(reg.animals[boat_i].carrier, "and takes saddlebags");
    let mut w = test_world("boatfloat");
    // A deep water column: stone floor, six water cells.
    let stone = w.reg.block_id("base:stone").unwrap();
    for x in 6..=10 {
        for z in 6..=10 {
            w.set_block(x, 150, z, stone);
            for y in 151..=156 {
                w.set_block(x, y, z, w.reg.water_block(0));
            }
            for y in 157..200 {
                w.set_block(x, y, z, AIR);
            }
        }
    }
    let mut boat = crate::mobs::Mob::new(boat_i, Vec3::new(8.5, 158.0, 8.5), 0.0);
    boat.health = reg.animals[boat_i].health;
    boat.tamed = true;
    let def = &reg.animals[boat_i];
    let players = [crate::server::PlayerCtx {
        id: 0,
        pos: ep(Vec3::new(50.0, 160.0, 50.0)),
        spawn: ep(Vec3::ZERO),
        attackable: true,
        aggro_mod: 0.0,
        quiet_charm: None,
    }];
    let mut rng = 9u32;
    let mut events = Vec::new();
    for _ in 0..200 {
        boat.tick(&w, def, &players, 0.05, &mut rng, &mut events);
    }
    assert!(
        boat.pos.y > 154.5,
        "the hull bobbed to the surface ({})",
        boat.pos.y
    );
    // A wrecked boat spills its pack — the sweep logic reads cargo,
    // and the drop table returns the hull as lumber.
    assert_eq!(reg.animals[boat_i].drops[0].0, it(&reg, "base:boat"));
}

#[test]
fn the_watcher_warns_stands_down_or_graduates() {
    let reg = base_reg();
    let mut w = test_world("watcher");
    // Aggrieved country far from spawn protections.
    for _ in 0..12 {
        w.add_ire_at(500, 500, 1.0);
    }
    w.ire = 30.0;
    let thorn = reg
        .animals
        .iter()
        .position(|a| a.hostile)
        .expect("a warden species");
    let mut m = crate::mobs::Mob::new(thorn, Vec3::new(500.5, 220.0, 500.5), 0.0);
    m.health = 10.0;
    m.watcher = true;
    m.watch_baseline = w.regional_ire_at(500, 500);
    w.spawn_mob(m);
    // A watcher does not hunt, whatever the provocation.
    let def = &reg.animals[thorn];
    let players = [crate::server::PlayerCtx {
        id: 0,
        pos: ep(Vec3::new(504.5, 220.0, 500.5)),
        spawn: ep(Vec3::ZERO),
        attackable: true,
        aggro_mod: 0.0,
        quiet_charm: None,
    }];
    let mut rng = 7u32;
    let mut events = Vec::new();
    let stub = test_world("watcher-stub");
    {
        let mob = w.mob_mut(w.mob_count() - 1).unwrap();
        for _ in 0..40 {
            mob.tick(&stub, def, &players, 0.1, &mut rng, &mut events);
        }
    }
    // (ticked against a stub world only for physics; the state gate
    // is what we assert)
    let mob = w.mob(w.mob_count() - 1).unwrap();
    assert_ne!(
        mob.state,
        crate::mobs::MobState::Hunt,
        "watchers never hunt"
    );
    assert!(mob.watch_timer > 3.5, "the vigil is timed");
    // Mend the ground: the watcher melts away without a corpse.
    for _ in 0..8 {
        w.plant_ire_at(500, 500, 1.0);
    }
    let before = w.mob_count();
    w.grade_watchers();
    assert_eq!(w.mob_count(), before - 1, "answered, it leaves");
    assert!(
        w.whispers.iter().any(|l| l.contains("melts")),
        "and says so"
    );
}
