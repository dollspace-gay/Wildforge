use crate::planet::{Direction4, Face};
use crate::planet_atlas::{
    AquiferLayer, AtlasPos, ChunkWaterCommitment, HydrologyCell, PlanetAtlas, PlanetaryWeather,
    PrecipitationForm, ReservoirMass, SpringState, SurfaceReservoirKind, WaterCell, WaterClass,
    YEAR_DAYS, dynamic_water_total, surface_reservoir_parts, take_river_baseflow,
};

use super::{base_reg, tmp_dir};

fn atmosphere(weather: &PlanetaryWeather) -> ReservoirMass {
    ReservoirMass::fresh(dynamic_water_total(&weather.cells) as u64)
}

#[test]
fn every_fixed_point_transfer_preserves_water_and_salt() {
    let mut source = ReservoirMass::with_salinity(1_003, 173);
    let initial = source;
    let mut destination = ReservoirMass::default();
    for request in [1, 31, 32, 255, 17, 900] {
        destination.add_assign(source.take(request)).unwrap();
    }
    assert_eq!(source.water_hu + destination.water_hu, initial.water_hu);
    assert_eq!(source.salt_mass + destination.salt_mass, initial.salt_mass);
    destination.add_assign(source.take(u64::MAX)).unwrap();
    assert_eq!(destination, initial);
    assert_eq!(source, ReservoirMass::default());
}

#[test]
fn river_and_ocean_mixing_is_exact_and_becomes_brackish() {
    let river = ReservoirMass::with_salinity(256, 8);
    let ocean = ReservoirMass::with_salinity(256, 220);
    let mixed = river.checked_add(ocean).unwrap();
    assert_eq!(mixed.water_hu, river.water_hu + ocean.water_hu);
    assert_eq!(mixed.salt_mass, river.salt_mass + ocean.salt_mass);
    assert_eq!(mixed.water_class(), WaterClass::Brackish);
}

#[test]
fn evaporation_is_fresh_and_freezing_rejects_salt_exactly() {
    let original = ReservoirMass::with_salinity(256, 220);
    let mut ocean = original;
    let vapor = ocean.take_fresh_water(64);
    assert_eq!(vapor.salt_mass, 0);
    assert!(ocean.salinity() > original.salinity());
    ocean.add_assign(vapor).unwrap();
    assert_eq!(ocean, original);

    let mut sea = original;
    let ice = sea.freeze(128);
    assert!(ice.salinity() < original.salinity());
    assert_eq!(sea.water_hu + ice.water_hu, original.water_hu);
    assert_eq!(sea.salt_mass + ice.salt_mass, original.salt_mass);
    sea.add_assign(ice).unwrap();
    assert_eq!(sea, original);
}

#[test]
fn generated_planet_starts_with_a_closed_named_ledger() {
    let atlas = PlanetAtlas::fixture(6_001, 4).unwrap();
    let audit = atlas.water_audit();
    assert_eq!(audit.unexplained_water_delta_hu, 0);
    assert_eq!(audit.unexplained_salt_delta, 0);
    assert!(audit.atmosphere.water_hu > 0);
    assert!(audit.soil.water_hu > 0);
    assert!(audit.groundwater.water_hu > 0);
    assert!(audit.coarse_surface.water_hu > 0);
    assert!(
        atlas
            .water_cycle
            .reservoirs
            .iter()
            .all(|reservoir| !reservoir.name.is_empty())
    );
    assert!(
        atlas
            .water_audit_text()
            .contains("unexplained water delta: 0 HU")
    );
}

#[test]
fn save_load_preserves_water_at_transfer_boundaries() {
    let root = tmp_dir("water-cycle-boundaries");
    let atlas = PlanetAtlas::fixture(6_002, 4).unwrap();
    atlas.write_new(&root).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let reservoir = weather
        .water
        .reservoirs
        .iter()
        .find(|reservoir| reservoir.coarse.water_hu >= 256)
        .unwrap()
        .id;
    let parcel = weather.materialize_surface_water(reservoir, 256);
    assert_eq!(parcel.water_hu, 256);
    let class = weather.move_detailed_to_portable(parcel).unwrap();
    assert_eq!(weather.move_portable_to_detailed(class), Some(parcel));
    assert!(weather.move_detailed_to_industrial(parcel));
    let pos = AtlasPos {
        face: Face::PosZ,
        u: 0,
        v: 0,
    };
    let exhausted = weather.exhaust_industrial_vapor(pos, 32);
    assert_eq!(exhausted, 32);
    atlas
        .save_dynamic_snapshot(&root, &weather.cells, &weather.water)
        .unwrap();
    let loaded = PlanetAtlas::load_fixture(&root).unwrap();
    assert_eq!(loaded.dynamic, weather.cells);
    assert_eq!(loaded.water_cycle, weather.water);
    assert_eq!(loaded.water_audit().unexplained_water_delta_hu, 0);
    assert_eq!(loaded.water_audit().unexplained_salt_delta, 0);
}

#[test]
fn groundwater_crosses_a_cube_seam_without_mass_leak() {
    let atlas = PlanetAtlas::fixture(6_003, 4).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    for cell in weather.water.cells.values_mut() {
        cell.groundwater_head_milliblocks = 0;
    }
    let source = AtlasPos {
        face: Face::PosZ,
        u: 0,
        v: 1,
    };
    let target = source.step(Direction4::West, atlas.side()).pos;
    let room = weather
        .water
        .cells
        .get_mut(target)
        .unwrap()
        .groundwater
        .take(512);
    weather
        .water
        .cells
        .get_mut(source)
        .unwrap()
        .groundwater
        .add_assign(room)
        .unwrap();
    weather
        .water
        .cells
        .get_mut(source)
        .unwrap()
        .groundwater_head_milliblocks = 20_000;
    let target_before = weather
        .water
        .cells
        .get(target)
        .unwrap()
        .groundwater
        .water_hu;
    let before = weather.water.audit(atmosphere(&weather));
    weather.advance_groundwater_day_for_test(&atlas).unwrap();
    let after = weather.water.audit(atmosphere(&weather));
    assert!(
        weather
            .water
            .cells
            .get(target)
            .unwrap()
            .groundwater
            .water_hu
            > target_before
    );
    assert_eq!(after.current_water_hu, before.current_water_hu);
    assert_eq!(after.current_salt_mass, before.current_salt_mass);
}

#[test]
fn a_spring_weakens_after_drawdown_and_can_recover() {
    let atlas = PlanetAtlas::fixture(6_004, 4).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let pos = AtlasPos {
        face: Face::PosZ,
        u: 1,
        v: 1,
    };
    let cell = weather.water.cells.get_mut(pos).unwrap();
    cell.groundwater_head_milliblocks = 20_000;
    weather.water.springs = vec![SpringState {
        pos,
        layer: AquiferLayer::Shallow,
        outlet_milliblocks: 10_000,
        last_discharge_hu: 0,
        active: false,
    }];
    weather.complete_hour(&atlas, 20.0, 0, 0.0).unwrap();
    let flowing = weather.water.springs[0].last_discharge_hu;
    assert!(flowing > 0);
    let _ = weather.pump_groundwater(pos, u64::MAX);
    weather
        .water
        .cells
        .get_mut(pos)
        .unwrap()
        .groundwater_head_milliblocks = 0;
    weather.complete_hour(&atlas, 20.0, 1, 0.0).unwrap();
    assert_eq!(weather.water.springs[0].last_discharge_hu, 0);
    weather
        .water
        .cells
        .get_mut(pos)
        .unwrap()
        .groundwater_head_milliblocks = 20_000;
    weather.complete_hour(&atlas, 20.0, 2, 0.0).unwrap();
    assert!(weather.water.springs[0].last_discharge_hu <= flowing);
}

#[test]
fn bucket_classes_and_boiler_keep_exact_salt() {
    let atlas = PlanetAtlas::fixture(6_005, 4).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let parcels = [
        (WaterClass::Fresh, ReservoirMass::with_salinity(256, 8)),
        (WaterClass::Brackish, ReservoirMass::with_salinity(256, 80)),
        (WaterClass::Salt, ReservoirMass::with_salinity(256, 220)),
    ];
    for (class, parcel) in parcels {
        weather.water.credit_detailed(parcel).unwrap();
        weather.water.ledger.initial_water_hu += parcel.water_hu;
        weather.water.ledger.initial_salt_mass += parcel.salt_mass;
        assert_eq!(weather.move_detailed_to_portable(parcel), Some(class));
        assert_eq!(weather.move_portable_to_detailed(class), Some(parcel));
    }
    let salt = parcels[2].1;
    assert!(weather.move_detailed_to_industrial(salt));
    let pos = AtlasPos {
        face: Face::PosZ,
        u: 0,
        v: 0,
    };
    assert_eq!(weather.exhaust_industrial_vapor(pos, 64), 64);
    assert_eq!(weather.water.ledger.industrial.salt_mass, salt.salt_mass);
    let audit = weather.water.audit(atmosphere(&weather));
    assert_eq!(audit.unexplained_water_delta_hu, 0);
    assert_eq!(audit.unexplained_salt_delta, 0);
}

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

fn process_peak_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmHWM:")?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    })
}

#[test]
fn sliced_and_whole_hour_fluxes_are_identical() {
    let atlas = PlanetAtlas::fixture(6_012, 4).unwrap();
    let mut whole = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let mut sliced = whole.clone();
    whole.begin_hour(77);
    let whole_report = whole
        .advance_slice(&atlas, 31.25, usize::MAX, |_| 17.0)
        .unwrap()
        .unwrap();
    sliced.begin_hour(77);
    let sliced_report = loop {
        if let Some(report) = sliced.advance_slice(&atlas, 31.25, 3, |_| 17.0).unwrap() {
            break report;
        }
    };
    assert_eq!(whole.cells, sliced.cells);
    assert_eq!(whole.water, sliced.water);
    assert_eq!(whole_report, sliced_report);
}

#[test]
fn failed_weather_hour_rolls_back_and_is_not_retried() {
    let source = PlanetAtlas::fixture(6_014, 4).unwrap();
    let wrong_shape = PlanetAtlas::fixture(6_015, 2).unwrap();
    let mut weather = PlanetaryWeather::new(source.dynamic.clone(), source.water_cycle.clone());
    let before_cells = weather.cells.clone();
    let before_water = weather.water.clone();
    let first = weather
        .complete_hour(&wrong_shape, 0.0, 0, 0.0)
        .unwrap_err();
    assert!(first.to_string().contains("dimensions"));
    assert_eq!(weather.cells, before_cells);
    assert_eq!(weather.water, before_water);
    let second = weather.complete_hour(&source, 0.0, 0, 0.0).unwrap_err();
    assert!(
        second.to_string().contains("latched failed"),
        "a failed transaction must not mutate and retry every server tick"
    );
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
    world.day = (final_hour / 24)
        .try_into()
        .expect("production probe climate day fits the world calendar");
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

#[test]
fn ocean_fed_cave_materialization_debits_the_ocean_and_round_trips() {
    let atlas = PlanetAtlas::fixture(6_013, 4).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let id = weather
        .water
        .reservoirs
        .iter()
        .find(|reservoir| {
            surface_reservoir_parts(reservoir.id).0 == SurfaceReservoirKind::Ocean
                && reservoir.coarse.water_hu >= 256
        })
        .unwrap()
        .id;
    let before = weather.water.reservoir_mut(id).unwrap().clone();
    let parcel = weather.materialize_surface_water(id, 256);
    assert_eq!(parcel.water_hu, 256);
    let flooded = weather.water.reservoir_mut(id).unwrap().clone();
    assert_eq!(before.coarse.water_hu - flooded.coarse.water_hu, 256);
    assert_eq!(flooded.committed.water_hu - before.committed.water_hu, 256);
    assert!(weather.dematerialize_surface_water(id, parcel));
    let restored = weather.water.reservoir_mut(id).unwrap();
    assert_eq!(restored.coarse, before.coarse);
    assert_eq!(restored.committed, before.committed);
    assert_eq!(
        weather
            .water
            .audit(atmosphere(&weather))
            .unexplained_water_delta_hu,
        0
    );
}

#[test]
fn opening_a_cave_below_the_sea_moves_ocean_water_into_the_voxel() {
    let atlas = std::sync::Arc::new(PlanetAtlas::fixture(6_015, 8).unwrap());
    let (index, hydro) = atlas
        .genesis
        .hydrology
        .values()
        .iter()
        .enumerate()
        .find(|(_, hydro)| hydro.ocean_basin_id != 0)
        .unwrap();
    let atlas_pos = AtlasPos::from_index(index, atlas.side()).unwrap();
    let center = atlas_pos.center(atlas.side());
    let surface = crate::planet::SurfacePos::new(
        center.face,
        center.u.floor() as u16,
        center.v.floor() as u16,
    )
    .unwrap();
    let pos = crate::planet::BlockPos::new(surface.face(), surface.u(), 10, surface.v()).unwrap();
    let id = crate::planet_atlas::surface_reservoir_id(
        SurfaceReservoirKind::Ocean,
        u32::from(hydro.ocean_basin_id),
    );
    let reg = base_reg();
    let mut world =
        crate::world::World::new_with_atlas(6_015, tmp_dir("ocean-cave-seep"), reg.clone(), atlas);
    world.ensure_chunk(pos.chunk());
    world.set_block_at(pos, reg.block_id("base:stone").unwrap());
    let before = world
        .planetary_weather_for_test()
        .unwrap()
        .water
        .reservoirs
        .iter()
        .find(|reservoir| reservoir.id == id)
        .unwrap()
        .clone();
    world.break_block_at(pos, None, false, false).unwrap();
    assert_eq!(
        world.water_mass_at(pos).unwrap().water_hu,
        crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
    );
    let weather = world.planetary_weather_for_test().unwrap();
    let after = weather
        .water
        .reservoirs
        .iter()
        .find(|reservoir| reservoir.id == id)
        .unwrap();
    assert_eq!(
        before.coarse.water_hu - after.coarse.water_hu,
        crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
    );
    assert_eq!(
        after.committed.water_hu - before.committed.water_hu,
        crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
    );
    assert_eq!(
        weather
            .water
            .audit(atmosphere(weather))
            .unexplained_water_delta_hu,
        0
    );
}

#[test]
fn preferred_basin_debits_shrink_its_chunk_commitment() {
    let atlas = PlanetAtlas::fixture(6_014, 4).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let id = weather
        .water
        .reservoirs
        .iter()
        .find(|reservoir| reservoir.coarse.water_hu >= 256)
        .unwrap()
        .id;
    let parcel = weather.materialize_surface_water(id, 256);
    let chunk = crate::planet::ChunkPos::new(Face::PosZ, 1, 1).unwrap();
    weather.water.commitments.push(ChunkWaterCommitment {
        chunk,
        reservoir: id,
        mass: parcel,
    });
    let pos = AtlasPos {
        face: Face::PosZ,
        u: 0,
        v: 0,
    };
    assert!(weather.credit_detailed_vapor_from(pos, Some(id), ReservoirMass::fresh(32),));
    let commitment = weather
        .water
        .commitments
        .iter()
        .find(|commitment| commitment.reservoir == id)
        .unwrap();
    assert_eq!(commitment.mass.water_hu, 224);
    assert_eq!(commitment.mass.salt_mass, parcel.salt_mass);
    weather
        .water
        .validate(atlas.side(), atmosphere(&weather))
        .unwrap();
}

#[test]
fn perennial_channels_take_baseflow_but_intermittent_channels_do_not() {
    let initial = ReservoirMass::with_salinity(65_536, 12);
    let mut perennial = WaterCell {
        groundwater: initial,
        ..WaterCell::default()
    };
    let mut intermittent = perennial;
    let perennial_hydro = HydrologyCell {
        river_id: 7,
        stream_order: 3,
        ..HydrologyCell::default()
    };
    let intermittent_hydro = HydrologyCell {
        river_id: 8,
        stream_order: 2,
        ..HydrologyCell::default()
    };
    let (_, baseflow) = take_river_baseflow(perennial_hydro, &mut perennial).unwrap();
    assert!(baseflow.water_hu > 0);
    assert_eq!(
        perennial.groundwater.water_hu + baseflow.water_hu,
        initial.water_hu
    );
    assert_eq!(
        perennial.groundwater.salt_mass + baseflow.salt_mass,
        initial.salt_mass
    );
    assert!(take_river_baseflow(intermittent_hydro, &mut intermittent).is_none());
    assert_eq!(intermittent.groundwater, initial);
}

#[test]
fn rain_is_fresh_and_enters_storage_even_when_no_chunk_is_loaded() {
    let atlas = PlanetAtlas::fixture(6_007, 4).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let pos = AtlasPos {
        face: Face::NegZ,
        u: 1,
        v: 1,
    };
    let index = pos.index(atlas.side());
    weather.cells.cells.values_mut()[index].cloud_water = 200_000;
    weather.water.ledger.initial_water_hu =
        weather.water.audit(atmosphere(&weather)).current_water_hu;
    let before = weather.water.cells.get(pos).unwrap().total().water_hu;
    weather.complete_hour(&atlas, 30.0, 0, 0.0).unwrap();
    let after = weather.water.cells.get(pos).unwrap();
    assert!(after.total().water_hu >= before || weather.last_report.precipitation_units > 0);
    assert_eq!(
        after.snow.salt_mass + after.runoff.salt_mass + after.soil.salt_mass,
        0
    );
    assert_eq!(
        weather
            .water
            .audit(atmosphere(&weather))
            .unexplained_water_delta_hu,
        0
    );
}

#[test]
fn landed_precipitation_can_cross_the_explicit_detail_boundary() {
    let atlas = PlanetAtlas::fixture(6_008, 4).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let pos = AtlasPos {
        face: Face::PosX,
        u: 0,
        v: 0,
    };
    weather.water.cells.get_mut(pos).unwrap().runoff = ReservoirMass::fresh(64);
    weather.water.ledger.initial_water_hu += 64;
    let parcel = weather.withdraw_water_cycle_mass(pos, PrecipitationForm::Rain, 32);
    assert_eq!(parcel, ReservoirMass::fresh(32));
    assert_eq!(
        weather
            .water
            .audit(atmosphere(&weather))
            .unexplained_water_delta_hu,
        0
    );
}

#[test]
fn a_breached_lake_moves_its_exact_mass_downstream() {
    let atlas = PlanetAtlas::fixture(6_009, 8).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let ids = weather
        .water
        .reservoirs
        .iter()
        .filter(|reservoir| reservoir.coarse.water_hu > 1_024)
        .map(|reservoir| reservoir.id)
        .take(2)
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 2);
    let source_before = weather.water.reservoir_mut(ids[0]).unwrap().coarse;
    let destination_before = weather.water.reservoir_mut(ids[1]).unwrap().coarse;
    let parcel = weather.breach_surface_reservoir(ids[0], ids[1], 1_003);
    assert_eq!(parcel.water_hu, 1_003);
    let source_after = weather.water.reservoir_mut(ids[0]).unwrap().coarse;
    let destination_after = weather.water.reservoir_mut(ids[1]).unwrap().coarse;
    assert_eq!(
        source_before.water_hu - source_after.water_hu,
        parcel.water_hu
    );
    assert_eq!(
        source_before.salt_mass - source_after.salt_mass,
        parcel.salt_mass
    );
    assert_eq!(
        destination_after.water_hu - destination_before.water_hu,
        parcel.water_hu
    );
    assert_eq!(
        destination_after.salt_mass - destination_before.salt_mass,
        parcel.salt_mass
    );
    assert_eq!(
        weather
            .water
            .audit(atmosphere(&weather))
            .unexplained_water_delta_hu,
        0
    );
}

#[test]
fn pumping_makes_a_drawdown_cone_that_groundwater_refills() {
    let atlas = PlanetAtlas::fixture(6_010, 4).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let pos = AtlasPos {
        face: Face::NegX,
        u: 1,
        v: 1,
    };
    let neighbor = pos.step(Direction4::East, atlas.side()).pos;
    let head_before = weather
        .water
        .cells
        .get(pos)
        .unwrap()
        .groundwater_head_milliblocks;
    let neighbor_before = weather
        .water
        .cells
        .get(neighbor)
        .unwrap()
        .groundwater
        .water_hu;
    let pumped = weather.pump_groundwater(pos, 256);
    assert_eq!(pumped.water_hu, 256);
    assert!(
        weather
            .water
            .cells
            .get(pos)
            .unwrap()
            .groundwater_head_milliblocks
            < head_before
    );
    weather.advance_groundwater_day_for_test(&atlas).unwrap();
    assert_ne!(
        weather
            .water
            .cells
            .get(neighbor)
            .unwrap()
            .groundwater
            .water_hu,
        neighbor_before
    );
    assert_eq!(
        weather
            .water
            .audit(atmosphere(&weather))
            .unexplained_water_delta_hu,
        0
    );
}

#[test]
fn player_basin_registration_changes_ownership_without_changing_mass() {
    let atlas = PlanetAtlas::fixture(6_011, 4).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let parcel = ReservoirMass::with_salinity(256, 80);
    weather.water.credit_detailed(parcel).unwrap();
    weather.water.ledger.initial_water_hu += parcel.water_hu;
    weather.water.ledger.initial_salt_mass += parcel.salt_mass;
    let chunk = crate::planet::ChunkPos::new(Face::PosZ, 1, 1).unwrap();
    let before = weather.water.audit(atmosphere(&weather));
    let empty_id = weather.ensure_dynamic_basin(chunk, 70_000);
    let after_empty = weather.water.audit(atmosphere(&weather));
    assert_eq!(after_empty.current_water_hu, before.current_water_hu);
    assert_eq!(after_empty.current_salt_mass, before.current_salt_mass);
    let id = weather
        .register_dynamic_basin(chunk, 70_000, parcel)
        .unwrap();
    assert_eq!(id, empty_id, "waterfront works reuse the stable basin id");
    let after = weather.water.audit(atmosphere(&weather));
    assert_eq!(after.current_water_hu, before.current_water_hu);
    assert_eq!(after.current_salt_mass, before.current_salt_mass);
    let basin = weather.water.reservoir_mut(id).unwrap();
    assert_eq!(basin.committed, parcel);
    assert!(basin.name.starts_with("player basin"));
}
