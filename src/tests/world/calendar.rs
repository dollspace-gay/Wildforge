//! Calendar scenarios.

use super::*;

#[test]
fn crops_grow_on_farmland_via_random_ticks() {
    let reg = base_reg();
    let mut w = test_world_with("crops", reg.clone());
    let h = w.surface_height(4, 4);
    let farm = b(&reg, "base:farmland");
    let seed0 = b(&reg, "base:wheat_seeds");
    // A field on farmland grows; a control column on dirt doesn't.
    for x in 0..16 {
        for z in 0..16 {
            if (x, z) == (6, 6) {
                continue;
            }
            w.set_block(x, h, z, farm);
            w.set_block(x, h + 1, z, seed0);
        }
    }
    w.set_block(6, h, 6, b(&reg, "base:dirt"));
    w.set_block(6, h + 1, 6, seed0);
    let mut rng = 12345u32;
    for _ in 0..3000 {
        w.random_tick(&mut rng);
    }
    let mut advanced = 0;
    for x in 0..16 {
        for z in 0..16 {
            if (x, z) != (6, 6) && w.get_block(x, h + 1, z) != seed0 {
                advanced += 1;
            }
        }
    }
    let sample = w.weather_at_surface(bp(4, h + 1, 4).surface());
    assert!(
        advanced > 0,
        "farmland crops should advance (local temperature {:.2} C, season {})",
        sample.temperature_c,
        w.season_at_surface(bp(4, h + 1, 4).surface())
    );
    assert_eq!(w.get_block(6, h + 1, 6), seed0, "dirt crop must not grow");
    // Stage chain terminates at ripe (stage2) with a harvest def.
    let ripe = b(&reg, "base:wheat_seeds/stage2");
    assert!(reg.block(ripe).crop_next.is_none());
    let (item, _, becomes) = reg.block(ripe).harvest.expect("ripe wheat harvests");
    assert_eq!(item, it(&reg, "base:wheat"));
    assert_eq!(becomes, seed0);
    // Bushes regrow anywhere - but only in season (summer/autumn).
    w.set_calendar_day(crate::world::SEASON_DAYS); // summer
    let bare = b(&reg, "base:berry_bush");
    for x in 0..16 {
        for z in 8..11 {
            w.set_block(x, h + 3, z, bare);
        }
    }
    for _ in 0..30000 {
        w.random_tick(&mut rng);
    }
    let fruited = b(&reg, "base:berry_bush/stage1");
    let refruited = (0..16)
        .flat_map(|x| (8..11).map(move |z| (x, z)))
        .filter(|&(x, z)| w.get_block(x, h + 3, z) == fruited)
        .count();
    assert!(refruited > 0, "bushes should refruit anywhere");
    // Cross rendering flags.
    assert!(reg.block(seed0).cross);
    assert!(!reg.is_solid(seed0));
}

#[test]
fn random_ticks_budget_stamps_and_persist() {
    let reg = base_reg();
    let dir = tmp_dir("stamps");
    let mut w = World::new(42, dir.clone(), reg.clone());
    for x in 0..3 {
        for z in 0..3 {
            w.ensure_chunk(tchunk(x, z));
        }
    }
    w.set_simulation_clock(100.0);
    let mut rng = 7u32;
    let burst = w.random_tick(&mut rng);
    assert_eq!(burst, 9 * 256, "long-waited chunks catch up at the cap");
    let again = w.random_tick(&mut rng);
    assert_eq!(again, 9 * 8, "freshly stamped chunks take the floor burst");
    assert_eq!(w.chunk_stamp(0, 0), Some(100.0));
    save_world(&mut w);
    let w2 = World::load_or_create(dir, reg.clone()).unwrap();
    assert_eq!(w2.chunk_stamp(0, 0), Some(100.0), "stamps persist");
}

#[test]
fn random_ticks_visit_a_bounded_cohort() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("cohort"), reg.clone());
    for x in 0..9 {
        for z in 0..9 {
            w.ensure_chunk(tchunk(x, z));
        }
    }
    // 81 chunks loaded, all stamped at clock 0; K = 64 caps the visit.
    w.set_simulation_clock(5.0);
    let mut rng = 3u32;
    // Five seconds of waiting, at the world's sample rate; the 47
    // chunks already visited this clock fall back to the floor of 8.
    let waited = (5.0 * crate::world::RANDOM_TICKS_PER_CHUNK_SEC) as usize;
    assert_eq!(
        w.random_tick(&mut rng),
        64 * waited,
        "K chunks, elapsed-scaled"
    );
    assert_eq!(
        w.random_tick(&mut rng),
        17 * waited + 47 * 8,
        "oldest first"
    );
}

#[test]
fn server_ticks_at_fixed_rate_and_runs_the_world() {
    let reg = base_reg();
    let world = test_world("simsplit");
    let mut sv = crate::server::Server::new(world, 0.3, 42);
    let ctx = crate::server::PlayerCtx {
        id: 0,
        pos: ep(Vec3::new(8.0, 80.0, 8.0)),
        spawn: ep(Vec3::new(-500.0, 70.0, -500.0)),
        attackable: true,
        aggro_mod: 0.0,
        quiet_charm: None,
    };
    let t0 = sv.time_of_day;
    let mut evs = Vec::new();
    // 2 wall-seconds in odd chunks: the fixed tick must absorb it evenly.
    for _ in 0..120 {
        sv.advance(1.0 / 60.0, &[ctx], &mut evs);
    }
    let advanced = sv.time_of_day - t0;
    assert!(
        (advanced - 2.0 / crate::server::DAY_LENGTH).abs() < 0.0005,
        "clock advanced by the simulated time, got {advanced}"
    );
    // A hitch doesn't spiral the simulation.
    sv.advance(30.0, &[ctx], &mut evs);
    assert!(sv.time_of_day - t0 < 0.01, "hitch capped, not replayed");
    // Ire tier events flow through the server.
    sv.world.ire = 95.0;
    let mut evs2 = Vec::new();
    sv.advance(0.1, &[ctx], &mut evs2);
    assert!(
        evs2.iter()
            .any(|e| matches!(e, crate::server::SimEvent::IreTier { rose: true, .. })),
        "tier change surfaced as a SimEvent"
    );
    let _ = reg;
}

#[test]
fn calendar_advances_and_persists_without_a_global_weather_state() {
    let reg = base_reg();
    let w = World::new(42, tmp_dir("wx-day"), reg.clone());
    let mut sim = crate::server::Server::new(w, 0.999, 5);
    let mut ev = Vec::new();
    for _ in 0..40 {
        sim.advance(0.1, &[], &mut ev); // the hitch cap swallows big steps
    }
    assert_eq!(sim.world.day(), 1, "midnight rolls the calendar");
    sim.sleep_to_dawn();
    assert_eq!(sim.world.day(), 2, "sleeping skips into tomorrow");

    // Only the calendar rides world.toml. Local weather persists in the
    // dynamic atlas snapshot and has no world-wide enum to serialize.
    let dir = tmp_dir("wx-persist");
    let mut w = World::new(42, dir.clone(), reg.clone());
    let midsummer = crate::world::SEASON_DAYS + crate::world::SEASON_DAYS / 2;
    w.set_calendar_day(midsummer);
    save_world(&mut w);
    let w2 = World::load_or_create(dir, reg).unwrap();
    assert_eq!(w2.day(), midsummer);
    assert_eq!(w2.season(), 1, "a day and a half of seasons in is summer");
}

/// The calendar runs on DAY_LENGTH; growth, spoilage and recovery run
/// on the wall clock. Those are two clocks, and every one of these
/// pairs silently changes what it MEANS if only one of them is turned.
/// A crop that took four days to ripen taking two, a larder that fed
/// you through winter running out in autumn — neither shows up as a
/// failure anywhere, which is exactly why they are pinned here.
#[test]
fn the_calendar_and_the_wall_clock_stay_in_step() {
    use crate::server::DAY_LENGTH;
    use crate::world::{FRESHNESS_PER_SEC, RANDOM_TICKS_PER_CHUNK_SEC};
    let reg = base_reg();

    // A chunk gets the same random-tick visits per in-game day at any
    // day length: this is what makes a crop take N days rather than N
    // minutes. Growth, thaw, fungus and grass regrowth all ride it.
    let visits_per_day = RANDOM_TICKS_PER_CHUNK_SEC * DAY_LENGTH as f64;
    assert!(
        (visits_per_day - 9600.0).abs() < 1.0,
        "a chunk should see ~9600 random ticks a day, sees {visits_per_day:.0}"
    );

    // Food is authored in freshness points and spent on the wall
    // clock, so its shelf life in DAYS is the product of three
    // numbers. "Will this last the winter" has to keep its answer.
    for (item, want_days) in [("base:potato", 3.0f32), ("base:raw_venison", 1.5)] {
        let id = reg.item_id(item).unwrap_or_else(|| panic!("no {item}"));
        let days = reg.item(id).durability as f32 / FRESHNESS_PER_SEC / DAY_LENGTH;
        assert!(
            (days - want_days).abs() < 0.05,
            "{item} should keep {want_days} in-game days, keeps {days:.2}"
        );
    }

    // And a season is a season's worth of days, not a hardcoded 12.
    assert_eq!(crate::world::ROOT_DAYS, crate::world::SEASON_DAYS as f32);
    assert_eq!(
        crate::world::HEART_CUTTING_DAYS,
        crate::world::SEASON_DAYS as f32 / 2.0
    );
    assert_eq!(
        crate::world::BLOOM_EXHAUSTION,
        crate::world::SEASON_DAYS as f32
    );
}
