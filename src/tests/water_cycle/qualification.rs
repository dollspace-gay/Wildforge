//! Qualification scenarios.

use super::*;

#[test]
fn sealed_planet_conserves_exactly_for_two_centuries() {
    // This is the headless long-run harness as well as the default two-century
    // gate. Qualification can select any positive duration without changing
    // the production weather/water algorithms exercised here:
    // WILDFORGE_STRESS_YEARS=10 cargo test ... -- --exact --nocapture
    let years = std::env::var("WILDFORGE_STRESS_YEARS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(200);
    let side = std::env::var("WILDFORGE_STRESS_ATLAS_SIDE")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(2);
    assert!(years > 0, "stress duration must be positive");
    let atlas = PlanetAtlas::fixture(6_006, side).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let before = weather.water.audit(atmosphere(&weather));
    let hours_per_year = u64::from(YEAR_DAYS) * 24;
    let hours = hours_per_year * years;
    let started = std::time::Instant::now();
    let mut annual_surface = Vec::new();
    let mut seasonal_surface = Vec::new();
    let mut annual_evaporation = 0_u64;
    let mut annual_precipitation = 0_u64;
    for hour in 0..hours {
        let report = weather
            .complete_hour(&atlas, hour as f64 / 24.0, hour, 0.0)
            .unwrap();
        annual_evaporation = annual_evaporation.saturating_add(report.evaporation_units);
        annual_precipitation = annual_precipitation.saturating_add(report.precipitation_units);
        if (hour + 1).is_multiple_of(u64::from(YEAR_DAYS) * 6) {
            let surface = weather
                .water
                .reservoirs
                .iter()
                .map(|reservoir| {
                    reservoir
                        .coarse
                        .water_hu
                        .saturating_add(reservoir.committed.water_hu)
                })
                .sum::<u64>();
            seasonal_surface.push(surface);
            if (hour + 1).is_multiple_of(hours_per_year) {
                annual_surface.push(surface);
                let year = (hour + 1) / hours_per_year;
                if matches!(year, 1 | 10 | 100) || year == years {
                    let audit = weather.water.audit(atmosphere(&weather));
                    let level_range = weather
                        .water
                        .reservoirs
                        .iter()
                        .map(|reservoir| reservoir.level_milliblocks)
                        .fold((i32::MAX, i32::MIN), |(low, high), level| {
                            (low.min(level), high.max(level))
                        });
                    let head_range = weather
                        .water
                        .cells
                        .values()
                        .iter()
                        .map(|cell| cell.groundwater_head_milliblocks)
                        .fold((i32::MAX, i32::MIN), |(low, high), head| {
                            (low.min(head), high.max(head))
                        });
                    let temperature_range = weather
                        .cells
                        .cells
                        .values()
                        .iter()
                        .map(|cell| cell.weather_temperature_anomaly)
                        .fold((i16::MAX, i16::MIN), |(low, high), value| {
                            (low.min(value), high.max(value))
                        });
                    let vegetation_range = weather
                        .cells
                        .cells
                        .values()
                        .iter()
                        .map(|cell| cell.vegetation_moisture_anomaly)
                        .fold((i32::MAX, i32::MIN), |(low, high), value| {
                            (low.min(value), high.max(value))
                        });
                    let habitat_cells = atlas
                        .genesis
                        .biomes
                        .values()
                        .iter()
                        .filter(|cell| cell.habitat_flags != 0)
                        .count();
                    eprintln!(
                        "planet-stress year={year} elapsed_s={:.3} cells={} water_hu={} salt={} unexplained_water={} unexplained_salt={} atmosphere_hu={} soil_hu={} snow_hu={} groundwater_hu={} runoff_hu={} surface_hu={} level_milliblocks={}..{} aquifer_head_milliblocks={}..{} annual_precipitation_hu={} annual_evaporation_hu={} temperature_anomaly_centi={}..{} vegetation_moisture_anomaly={}..{} habitat_cells={} countries={} peak_rss_kib={}",
                        started.elapsed().as_secs_f64(),
                        atlas.genesis.geometry.len(),
                        audit.current_water_hu,
                        audit.current_salt_mass,
                        audit.unexplained_water_delta_hu,
                        audit.unexplained_salt_delta,
                        audit.atmosphere.water_hu,
                        audit.soil.water_hu,
                        audit.snow.water_hu,
                        audit.groundwater.water_hu,
                        audit.runoff.water_hu,
                        audit.coarse_surface.water_hu + audit.voxel_surface.water_hu,
                        level_range.0,
                        level_range.1,
                        head_range.0,
                        head_range.1,
                        annual_precipitation,
                        annual_evaporation,
                        temperature_range.0,
                        temperature_range.1,
                        vegetation_range.0,
                        vegetation_range.1,
                        habitat_cells,
                        atlas.biomes.countries.len(),
                        process_peak_rss_kib().unwrap_or(0),
                    );
                }
                annual_evaporation = 0;
                annual_precipitation = 0;
            }
        }
    }
    let after = weather.water.audit(atmosphere(&weather));
    assert_eq!(after.current_water_hu, before.current_water_hu);
    assert_eq!(after.current_salt_mass, before.current_salt_mass);
    assert_eq!(after.unexplained_water_delta_hu, 0);
    assert_eq!(after.unexplained_salt_delta, 0);
    assert_eq!(
        weather.water.completed_groundwater_days,
        u64::from(YEAR_DAYS) * years
    );
    assert!(
        after.coarse_surface.water_hu > 0,
        "surface reservoirs survive"
    );
    assert!(after.groundwater.water_hu > 0, "aquifers survive");
    let rises = seasonal_surface.windows(2).any(|pair| pair[1] > pair[0]);
    let falls = seasonal_surface.windows(2).any(|pair| pair[1] < pair[0]);
    assert!(
        rises && falls,
        "surface storage follows a bounded seasonal cycle"
    );
    let annual_min = annual_surface.iter().copied().min().unwrap();
    let annual_max = annual_surface.iter().copied().max().unwrap();
    assert!(
        annual_max - annual_min <= annual_max / 1_000,
        "multi-century surface redistribution remains below 0.1%"
    );
    if annual_surface.len() >= 2 {
        let first_slope = annual_surface[0].saturating_sub(annual_surface[1]);
        let last = annual_surface.len() - 1;
        let last_slope = annual_surface[last - 1].saturating_sub(annual_surface[last]);
        assert!(
            last_slope <= first_slope.saturating_mul(2),
            "annual redistribution does not accelerate into a numerical runaway"
        );
    }
}

/// Live production-atlas regression for transport-capacity drift found during
/// the seed-20260801 dedicated smoke. The ordinary suite keeps compact
/// fixtures; qualification opts into the 393,216-cell save and crosses the
/// first twelve hours because later transport fields can reach capacities the
/// genesis hour does not.
#[test]
#[ignore = "requires WILDFORGE_PROBE_WORLD production save"]
fn production_weather_hours_conserve_after_materialized_spawn() {
    let root = std::env::var_os("WILDFORGE_PROBE_WORLD")
        .map(std::path::PathBuf::from)
        .expect("set WILDFORGE_PROBE_WORLD");
    let atlas = PlanetAtlas::load(&root).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let before = weather.water.audit(atmosphere(&weather));
    let first_hour = weather.completed_hours;
    for hour in first_hour..first_hour + 12 {
        let report = weather
            .complete_hour(&atlas, hour as f64 / 24.0, hour, 0.0)
            .unwrap_or_else(|error| panic!("production hour {hour}: {error}"));
        assert_eq!(report.unexplained_water_drift, 0, "hour {hour}");
    }
    let after = weather.water.audit(atmosphere(&weather));
    assert_eq!(after.current_water_hu, before.current_water_hu);
    assert_eq!(after.current_salt_mass, before.current_salt_mass);
    assert_eq!(after.unexplained_water_delta_hu, 0);
    assert_eq!(after.unexplained_salt_delta, 0);
}

/// The whole-hour probe above deliberately isolates the climate algorithm.
/// This companion reproduces dedicated operation: prepared voxel chunks keep
/// flowing, freezing, evaporating, and random-ticking while a sliced climate
/// transaction is in flight.
#[test]
#[ignore = "requires WILDFORGE_PROBE_WORLD production save"]
fn production_server_interleaving_conserves_weather() {
    let root = std::env::var_os("WILDFORGE_PROBE_WORLD")
        .map(std::path::PathBuf::from)
        .expect("set WILDFORGE_PROBE_WORLD");
    let reg = std::sync::Arc::new(crate::registry::load(std::path::Path::new("mods")));
    let mut world = crate::world::World::load_or_create(root, reg).unwrap();
    world.prepare_common_spawn(|_, _, _| {}).unwrap();
    let spawn = world.common_spawn().unwrap();
    let first_hour = world.planetary_weather_for_test().unwrap().completed_hours;
    let target = first_hour + 10;
    // Production probes may be rerun against a save whose climate clock has
    // already advanced well past day zero. Drive the authoritative calendar
    // just beyond the last hour this probe needs instead of hardcoding an
    // earlier time and then waiting for work the scheduler correctly refuses
    // to repeat.
    let final_hour = target - 1;
    world.set_calendar_day(
        (final_hour / 24)
            .try_into()
            .expect("production probe climate day fits the world calendar"),
    );
    let time_of_day = (final_hour % 24) as f32 / 24.0 + 0.001;
    let mut server = crate::server::Server::new(world, time_of_day, 0xd5ed);
    let before = server
        .world
        .planetary_weather_for_test()
        .unwrap()
        .water
        .audit(atmosphere(
            server.world.planetary_weather_for_test().unwrap(),
        ));
    let players = [crate::server::PlayerCtx {
        id: 1,
        pos: spawn,
        spawn,
        attackable: true,
        aggro_mod: 0.0,
        quiet_charm: None,
    }];
    let center = spawn.chunk().unwrap();
    let mut streaming = (-10..=10)
        .flat_map(|du| (-10..=10).map(move |dv| (du * du + dv * dv, center.offset(du, dv))))
        .filter(|(_, chunk)| chunk.distance(center) <= 10.0 * crate::chunk::CHUNK_X as f64 + 1.0)
        .collect::<Vec<_>>();
    streaming.sort_by_key(|(distance, _)| *distance);
    let mut streaming = std::collections::VecDeque::from(streaming);
    let mut events = Vec::new();
    for _ in 0..400 {
        // A real guest expands its wider view while the climate pass is
        // sliced. Chunk adoption materializes finite surface water and wakes
        // voxel flows, so it is part of the transaction-interleaving gate.
        for _ in 0..2 {
            if let Some((_, chunk)) = streaming.pop_front() {
                server.world.ensure_chunk(chunk);
            }
        }
        server.advance(0.25, &players, &mut events);
        if server
            .world
            .planetary_weather_for_test()
            .unwrap()
            .completed_hours
            >= target
        {
            break;
        }
    }
    let weather = server.world.planetary_weather_for_test().unwrap();
    assert_eq!(
        weather.completed_hours, target,
        "a sliced production hour latched or failed under ordinary voxel ticks"
    );
    let after = weather.water.audit(atmosphere(weather));
    assert_eq!(after.current_water_hu, before.current_water_hu);
    assert_eq!(after.current_salt_mass, before.current_salt_mass);
    assert_eq!(after.unexplained_water_delta_hu, 0);
    assert_eq!(after.unexplained_salt_delta, 0);
}
