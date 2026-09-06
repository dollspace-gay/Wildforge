//! Water residency scenarios.

use super::*;

#[test]
fn reconcile_catches_up_an_absent_chunk() {
    let reg = base_reg();
    let dir = tmp_dir("reconcile").join("world");
    crate::world::create_world_fixture_atomic(
        &dir,
        42,
        "survival",
        8,
        &crate::planet_atlas::CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    let mut w = World::load_or_create(dir.clone(), reg.clone()).unwrap();
    let anchor = crate::planet::Face::ALL
        .into_iter()
        .flat_map(|face| {
            (128u16..crate::planet::FACE_BLOCKS)
                .step_by(256)
                .flat_map(move |u| {
                    (128u16..crate::planet::FACE_BLOCKS)
                        .step_by(256)
                        .map(move |v| crate::planet::SurfacePos::new(face, u, v).unwrap())
                })
        })
        .find(|&pos| {
            let latitude = w.latitude_at_surface(pos);
            let (summer_day, winter_day) = if latitude < 0.0 {
                (3 * crate::world::SEASON_DAYS, crate::world::SEASON_DAYS)
            } else {
                (crate::world::SEASON_DAYS, 3 * crate::world::SEASON_DAYS)
            };
            w.temperature_at_surface_on_day(pos, f64::from(winter_day)) < -0.5
                && w.temperature_at_surface_on_day(pos, f64::from(summer_day)) > 8.0
                && w.soil_moisture_at_surface(pos) > 0.35
                && w.generator.surface_estimate_at(pos) > SEA_LEVEL + 2
        })
        .expect("seasonally freezing agricultural country");
    ensure_surface_neighborhood(&mut w, anchor, 1);
    let b = |n: &str| reg.block_id(n).unwrap();
    let y = 200;
    // A supported sky-open pool (the shelf the live winter test uses)
    // and a farmland strip about to miss three growing seasons.
    let pool: Vec<_> = (0..8)
        .map(|du| block_pos(surface_offset(anchor, du, 4), y + 1))
        .collect();
    let crops: Vec<_> = (0..8)
        .map(|du| block_pos(surface_offset(anchor, du, -4), y + 1))
        .collect();
    for (&water, &crop) in pool.iter().zip(&crops) {
        w.set_block_at(water.offset(0, -1, 0).unwrap(), b("base:planks"));
        w.set_block_at(water, reg.water_block(0));
        w.set_block_at(crop.offset(0, -1, 0).unwrap(), b("base:farmland"));
        w.set_block_at(crop, b("base:wheat_seeds"));
    }
    save_world(&mut w);

    // Reopen the world more than a year later, at the start of local winter.
    let mut w2 = World::load_or_create(dir, reg.clone()).unwrap();
    w2.set_calendar_day(local_season_day(&w2, anchor, 3) + crate::planet_atlas::YEAR_DAYS);
    w2.set_simulation_clock(w2.day() as f64 * 600.0);
    ensure_surface_neighborhood(&mut w2, anchor, 1);
    let iced = pool
        .iter()
        .filter(|&&pos| w2.get_block_at(pos) == b("base:ice"))
        .count();
    let winter_weather = w2.weather_at_surface(anchor);
    assert!(
        iced >= 6,
        "the pool froze while you were away ({iced}/8 at {:.2} C, season {}, t {:.3}, latitude {:.3}, day {})",
        winter_weather.temperature_c,
        w2.season_at_surface(anchor),
        w2.generator.climate_at(anchor).t,
        w2.latitude_at_surface(anchor),
        w2.day()
    );
    let grown = crops
        .iter()
        .filter(|&&pos| w2.get_block_at(pos) != b("base:wheat_seeds"))
        .count();
    assert!(grown > 0, "crops advanced over the missed seasons");
}

#[test]
fn stale_saved_water_wakes_on_load() {
    // Water saved in an unstable pose (a full cube whose sides face
    // air — what pre-seal worldgen left behind) must resume settling
    // when its chunk returns from disk, not hang frozen forever.
    let reg = base_reg();
    let dir = tmp_dir("stalewake");
    let y = 180;
    {
        let mut w = World::new(7, dir.clone(), reg.clone());
        w.ensure_chunk(tchunk(0, 0));
        // A 3x3 stone shelf holding one exposed full water cube.
        for x in 3..=5 {
            for z in 3..=5 {
                w.set_block(x, y, z, b(&reg, "base:stone"));
            }
        }
        w.set_block(4, y + 1, 4, reg.water_block(0));
        // Unload without ticking: the save captures it mid-flow, and
        // this world's pending queues die with it.
        w.unload_chunk(tchunk(0, 0));
    }
    let mut w = World::new(7, dir, reg.clone());
    w.ensure_chunk(tchunk(0, 0));
    let mut quiet = false;
    for _ in 0..200 {
        if !w.tick_water(10_000) {
            quiet = true;
            break;
        }
    }
    assert!(quiet, "reloaded water settles");
    assert!(
        reg.water_volume(w.get_block(4, y + 1, 4)).unwrap_or(0) < 8,
        "the stranded cube spread instead of hanging as a square"
    );
}

#[test]
fn saved_mid_drain_pools_resume_leveling() {
    // Quit the game mid-drain and the queues die with the session;
    // the load sweep must notice the head cliff at the gap and set
    // the pools leveling again.
    let reg = base_reg();
    let dir = tmp_dir("utube-resume");
    let y = 200;
    {
        let mut w = World::new(7, dir.clone(), reg.clone());
        w.ensure_chunk(tchunk(0, 0));
        let stone = b(&reg, "base:stone");
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
        w.set_block(3, y + 1, 1, AIR);
        w.tick_water(50); // a few strokes of the pour, then quit
        w.unload_chunk(tchunk(0, 0));
    }
    let mut w = World::new(7, dir, reg.clone());
    w.ensure_chunk(tchunk(0, 0));
    let mut quiet = false;
    for _ in 0..4000 {
        if !w.tick_water(10_000) {
            quiet = true;
            break;
        }
    }
    assert!(quiet, "the reloaded pools settle");
    let head = |x: i32, z: i32| -> i64 {
        let mut h = 0;
        for yy in y + 1..=y + 5 {
            if let Some(v) = reg.water_volume(w.get_block(x, yy, z)) {
                h = yy as i64 * 8 + v as i64;
            }
        }
        h
    };
    let (a, b2) = (head(1, 1), head(5, 2));
    assert!(
        (a - b2).abs() <= 2,
        "reload resumes the leveling (heads {a} vs {b2})"
    );
}
