use crate::chunk::SEA_LEVEL;
use crate::planet::{Direction4, Face, SurfacePos};
use crate::planet_atlas::{
    AXIAL_TILT_DEGREES, AtlasPos, CLIMATE_CONVERGENCE_TOLERANCE, CancellationToken, LocalWeather,
    PlanetAtlas, PlanetaryWeather, PrecipitationForm, ReservoirMass, YEAR_DAYS,
    daily_mean_insolation, day_length_hours, dynamic_water_total, generate_climate, local_season,
    solar_direction,
};
use crate::world::World;
use crate::world::{ReplicaWorld, ReplicationTarget};
use std::sync::Arc;

use super::{base_reg, tmp_dir};

fn climate(seed: u32, side: u16) -> PlanetAtlas {
    PlanetAtlas::fixture(seed, side).expect("climate fixture")
}

fn average(values: impl Iterator<Item = f64>) -> f64 {
    let values: Vec<f64> = values.collect();
    values.iter().sum::<f64>() / values.len().max(1) as f64
}

#[test]
fn astronomy_has_opposite_seasons_and_real_polar_day() {
    let tilt = AXIAL_TILT_DEGREES.to_radians();
    assert!((day_length_hours(0.0, 36.0) - 12.0).abs() < 1.0e-9);
    assert!(day_length_hours(60f64.to_radians(), 36.0) > 16.0);
    assert!(day_length_hours(60f64.to_radians(), 108.0) < 8.0);
    assert_eq!(day_length_hours(80f64.to_radians(), 36.0), 24.0);
    assert_eq!(day_length_hours(80f64.to_radians(), 108.0), 0.0);
    assert_eq!(local_season(40, tilt), 1);
    assert_eq!(local_season(40, -tilt), 3);
    assert_eq!(local_season(112, tilt), 3);
    assert_eq!(local_season(112, -tilt), 1);
    assert!((solar_direction(0.0, 0.25).length() - 1.0).abs() < 1.0e-12);
    assert_eq!(YEAR_DAYS, 144);
}

#[test]
fn runtime_sun_daylight_and_climate_insolation_share_one_geometry() {
    let atlas = Arc::new(climate(6_012, 8));
    let mut world = World::new_with_atlas(6_012, tmp_dir("climate-shared-sun"), base_reg(), atlas);
    world.set_calendar_day(36);
    world.set_simulation_clock(36.25 * f64::from(crate::server::DAY_LENGTH));
    let expected = solar_direction(36.25, 0.25);
    assert!(world.sun_direction().distance(expected) < 1.0e-12);
    for surface in [
        SurfacePos::new(Face::PosZ, 4_096, 4_096).unwrap(),
        SurfacePos::new(Face::PosY, 4_096, 4_096).unwrap(),
        SurfacePos::new(Face::NegX, 1, 4_096).unwrap(),
    ] {
        let elevation = expected.dot(crate::planet::surface_to_unit(surface.center())) as f32;
        assert_eq!(
            world.daylight_at_surface(surface),
            (elevation * 2.5 + 0.5).clamp(0.12, 1.0)
        );
    }

    // The analytic daily mean used by climate agrees with an integral of the
    // same sun vector used at runtime.
    for latitude in [0.0f64, 35f64.to_radians(), 70f64.to_radians()] {
        let unit = glam::DVec3::new(0.0, latitude.sin(), latitude.cos());
        let numerical = (0..2_400)
            .map(|step| {
                let time = (f64::from(step) + 0.5) / 2_400.0;
                solar_direction(36.0, time).dot(unit).max(0.0)
            })
            .sum::<f64>()
            / 2_400.0;
        assert!((numerical - daily_mean_insolation(latitude, 36.0)).abs() < 1.0e-5);
    }
}

#[test]
fn long_winter_adds_a_global_thermal_anomaly_without_stopping_the_orbit() {
    let atlas = Arc::new(climate(6_013, 8));
    let mut world = World::new_with_atlas(6_013, tmp_dir("climate-long-winter"), base_reg(), atlas);
    world.set_calendar_day(54);
    world.set_simulation_clock(54.25 * f64::from(crate::server::DAY_LENGTH));
    let northern = SurfacePos::new(Face::PosY, 4_096, 4_096).unwrap();
    let southern = SurfacePos::new(Face::NegY, 4_096, 4_096).unwrap();
    let ordinary = world.weather_at_surface(northern);
    let sun_before = world.sun_direction();

    world.set_long_winter_for_test(true);
    assert_eq!(world.season_at_surface(northern), 3);
    assert_eq!(world.season_at_surface(southern), 3);
    assert_eq!(world.sun_direction(), sun_before);
    let winter = world.weather_at_surface(northern);
    assert!((winter.temperature_c - (ordinary.temperature_c - 18.0)).abs() < 1.0e-6);
}

#[test]
fn cube_charts_have_equal_area_and_no_climate_seam_penalty() {
    let atlas = climate(1_337, 32);
    let mut area = [0.0f64; 6];
    let mut seam_delta = Vec::new();
    let mut interior_delta = Vec::new();
    for (pos, geometry) in atlas.genesis.geometry.iter() {
        area[pos.face.index()] += f64::from(geometry.physical_area);
        for direction in [Direction4::East, Direction4::North] {
            let stepped = pos.step(direction, atlas.side()).pos;
            let delta = (atlas.genesis.climate.get(pos).unwrap().mean_temperature
                - atlas.genesis.climate.get(stepped).unwrap().mean_temperature)
                .abs() as f64;
            if stepped.face == pos.face {
                interior_delta.push(delta);
            } else {
                seam_delta.push(delta);
            }
        }
    }
    let mean_area = area.iter().sum::<f64>() / 6.0;
    assert!(
        area.iter()
            .all(|face_area| ((face_area - mean_area) / mean_area).abs() < 1.0e-6),
        "cube faces must represent equal spherical area: {area:?}"
    );
    let seam = average(seam_delta.into_iter());
    let interior = average(interior_delta.into_iter());
    assert!(
        seam < interior * 1.8 + 0.15,
        "chart seams add a climate discontinuity: seam {seam:.3} C, interior {interior:.3} C"
    );
}

#[test]
fn circulation_bands_and_maritime_variation_hold_across_seeds() {
    for seed in [11, 57, 1_337] {
        let atlas = climate(seed, 16);
        let equatorial: Vec<f64> = atlas
            .genesis
            .geometry
            .iter()
            .filter_map(|(pos, geometry)| {
                (geometry.latitude_radians.abs() < 12f32.to_radians())
                    .then_some(f64::from(atlas.genesis.climate.get(pos)?.aridity))
            })
            .collect();
        let subtropical: Vec<f64> = atlas
            .genesis
            .geometry
            .iter()
            .filter_map(|(pos, geometry)| {
                ((23f32.to_radians()..33f32.to_radians())
                    .contains(&geometry.latitude_radians.abs()))
                .then_some(f64::from(atlas.genesis.climate.get(pos)?.aridity))
            })
            .collect();
        let wet = average(equatorial.iter().copied());
        let dry = average(subtropical.iter().copied());
        assert!(
            dry > wet * 1.04,
            "seed {seed}: equator {wet:.3}, subtropics {dry:.3}"
        );
        let spread = equatorial.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - equatorial.iter().copied().fold(f64::INFINITY, f64::min);
        assert!(
            spread > 0.08,
            "seed {seed}: latitude became a perfect stripe"
        );

        let maritime = average(atlas.genesis.terrain.iter().filter_map(|(pos, terrain)| {
            let climate = atlas.genesis.climate.get(pos)?;
            let latitude = atlas.genesis.geometry.get(pos)?.latitude_radians.abs();
            (terrain.eroded_elevation > SEA_LEVEL as f32
                && climate.continentality <= 0.4
                && (15f32.to_radians()..55f32.to_radians()).contains(&latitude))
            .then_some(f64::from(climate.mean_atmospheric_moisture))
        }));
        let interior = average(atlas.genesis.terrain.iter().filter_map(|(pos, terrain)| {
            let climate = atlas.genesis.climate.get(pos)?;
            let latitude = atlas.genesis.geometry.get(pos)?.latitude_radians.abs();
            (terrain.eroded_elevation > SEA_LEVEL as f32
                && climate.continentality >= 0.75
                && (15f32.to_radians()..55f32.to_radians()).contains(&latitude))
            .then_some(f64::from(climate.mean_atmospheric_moisture))
        }));
        assert!(
            maritime > interior,
            "seed {seed}: maritime {maritime:.2}, interior {interior:.2}"
        );
    }
}

#[test]
fn climate_normals_are_causal_not_independent_noise() {
    let atlas = climate(1_337, 16);
    assert!(
        atlas
            .manifest
            .climate_convergence_iterations
            .iter()
            .all(|iterations| *iterations >= 24)
    );
    assert!(f64::from(atlas.manifest.climate_max_residual) <= CLIMATE_CONVERGENCE_TOLERANCE);
    assert!(atlas.manifest.climate_moisture_budget_error < 1.0e-6);

    let equatorial = average(atlas.genesis.geometry.iter().filter_map(|(pos, geometry)| {
        (geometry.latitude_radians.abs() < 15f32.to_radians())
            .then_some(f64::from(atlas.genesis.climate.get(pos)?.mean_temperature))
    }));
    let polar = average(atlas.genesis.geometry.iter().filter_map(|(pos, geometry)| {
        (geometry.latitude_radians.abs() > 65f32.to_radians())
            .then_some(f64::from(atlas.genesis.climate.get(pos)?.mean_temperature))
    }));
    assert!(
        equatorial > polar + 22.0,
        "equator {equatorial:.1}, pole {polar:.1}"
    );

    let coastal_range = average(atlas.genesis.terrain.iter().filter_map(|(pos, terrain)| {
        let climate = atlas.genesis.climate.get(pos)?;
        (terrain.eroded_elevation > SEA_LEVEL as f32 && climate.continentality <= 0.25)
            .then_some(f64::from(climate.seasonality))
    }));
    let interior_range = average(atlas.genesis.terrain.iter().filter_map(|(pos, terrain)| {
        let climate = atlas.genesis.climate.get(pos)?;
        (terrain.eroded_elevation > SEA_LEVEL as f32 && climate.continentality >= 0.75)
            .then_some(f64::from(climate.seasonality))
    }));
    assert!(
        coastal_range < interior_range,
        "maritime {coastal_range:.2}, continental {interior_range:.2}"
    );

    let maritime_moisture = average(atlas.genesis.terrain.iter().filter_map(|(pos, terrain)| {
        let climate = atlas.genesis.climate.get(pos)?;
        let latitude = atlas.genesis.geometry.get(pos)?.latitude_radians.abs();
        (terrain.eroded_elevation > SEA_LEVEL as f32
            && climate.continentality <= 0.4
            && (15f32.to_radians()..50f32.to_radians()).contains(&latitude))
        .then_some(f64::from(climate.mean_atmospheric_moisture))
    }));
    let interior_moisture = average(atlas.genesis.terrain.iter().filter_map(|(pos, terrain)| {
        let climate = atlas.genesis.climate.get(pos)?;
        let latitude = atlas.genesis.geometry.get(pos)?.latitude_radians.abs();
        (terrain.eroded_elevation > SEA_LEVEL as f32
            && climate.continentality >= 0.8
            && (15f32.to_radians()..50f32.to_radians()).contains(&latitude))
        .then_some(f64::from(climate.mean_atmospheric_moisture))
    }));
    assert!(
        maritime_moisture > interior_moisture,
        "matched maritime moisture {maritime_moisture:.2}, interior {interior_moisture:.2}"
    );
    assert!(
        atlas
            .genesis
            .climate
            .values()
            .iter()
            .filter(|cell| cell.aridity > 1.0)
            .all(|cell| cell.mean_precipitation > 0.0)
    );
}

#[test]
fn elevation_cools_matched_latitudes() {
    let atlas = climate(2_441, 8);
    let mut raised = atlas.genesis.terrain.clone();
    let positions: Vec<_> = atlas
        .genesis
        .terrain
        .iter()
        .filter(|(_, terrain)| {
            terrain.eroded_elevation > SEA_LEVEL as f32 + 2.0 && terrain.eroded_elevation < 180.0
        })
        .map(|(pos, _)| pos)
        .take(12)
        .collect();
    assert!(positions.len() >= 8);
    for pos in &positions {
        raised.get_mut(*pos).unwrap().eroded_elevation += 40.0;
    }
    let (higher_climate, _) = generate_climate(
        2_441,
        atlas.side(),
        &atlas.genesis.geometry,
        &raised,
        &CancellationToken::default(),
    )
    .unwrap();
    for pos in positions {
        let base = atlas.genesis.climate.get(pos).unwrap().mean_temperature;
        let higher = higher_climate.get(pos).unwrap().mean_temperature;
        assert!(
            higher <= base - 0.30,
            "raising {pos:?} by 40 blocks changed {base:.2} C to {higher:.2} C"
        );
    }
}

#[test]
fn circulation_builds_rain_shadows_and_dry_subtropics() {
    let atlas = climate(1_337, 32);
    let mut windward = Vec::new();
    let mut leeward = Vec::new();
    for (pos, terrain) in atlas.genesis.terrain.iter() {
        if terrain.eroded_elevation <= SEA_LEVEL as f32 {
            continue;
        }
        let summit = atlas.climate_downstream(pos, 54.0);
        let lee = atlas.climate_downstream(summit, 54.0);
        let summit_elevation = atlas.genesis.terrain.get(summit).unwrap().eroded_elevation;
        let lee_elevation = atlas.genesis.terrain.get(lee).unwrap().eroded_elevation;
        if summit_elevation > terrain.eroded_elevation + 8.0
            && lee_elevation < summit_elevation - 5.0
        {
            windward.push(
                atlas
                    .genesis
                    .climate
                    .get(pos)
                    .unwrap()
                    .seasonal_precipitation[1] as f64,
            );
            leeward.push(
                atlas
                    .genesis
                    .climate
                    .get(lee)
                    .unwrap()
                    .seasonal_precipitation[1] as f64,
            );
        }
    }
    assert!(
        windward.len() >= 12,
        "only {} mountain transects",
        windward.len()
    );
    let wet = windward.iter().sum::<f64>() / windward.len() as f64;
    let dry = leeward.iter().sum::<f64>() / leeward.len() as f64;
    assert!(wet > dry * 1.08, "windward {wet:.1}, leeward {dry:.1}");

    let equatorial_aridity =
        average(atlas.genesis.geometry.iter().filter_map(|(pos, geometry)| {
            (geometry.latitude_radians.abs() < 12f32.to_radians())
                .then_some(f64::from(atlas.genesis.climate.get(pos)?.aridity))
        }));
    let subtropical_aridity =
        average(atlas.genesis.geometry.iter().filter_map(|(pos, geometry)| {
            let latitude = geometry.latitude_radians.abs();
            ((22f32.to_radians()..34f32.to_radians()).contains(&latitude))
                .then_some(f64::from(atlas.genesis.climate.get(pos)?.aridity))
        }));
    assert!(
        subtropical_aridity > equatorial_aridity * 1.08,
        "equatorial aridity {equatorial_aridity:.2}, subtropical {subtropical_aridity:.2}"
    );
}

#[test]
fn ocean_currents_transport_heat_in_the_recorded_direction() {
    let atlas = climate(8_181, 16);
    let mut poleward = Vec::new();
    let mut equatorward = Vec::new();
    for (pos, terrain) in atlas.genesis.terrain.iter() {
        if terrain.eroded_elevation > SEA_LEVEL as f32 {
            continue;
        }
        let geometry = atlas.genesis.geometry.get(pos).unwrap();
        let latitude = f64::from(geometry.latitude_radians);
        if latitude.abs() < 15f64.to_radians() || latitude.abs() > 70f64.to_radians() {
            continue;
        }
        let unit = glam::DVec3::from_array(geometry.unit_direction.map(f64::from));
        let north = (crate::planet_atlas::rotation_axis()
            - unit * unit.dot(crate::planet_atlas::rotation_axis()))
        .normalize_or_zero();
        let climate = atlas.genesis.climate.get(pos).unwrap();
        let current = crate::planet_atlas::chart_vector(pos, atlas.side(), climate.ocean_current);
        if current.dot(north) * latitude.signum() > 0.15 {
            poleward.push(f64::from(climate.ocean_temperature_anomaly));
        } else if current.dot(north) * latitude.signum() < -0.15 {
            equatorward.push(f64::from(climate.ocean_temperature_anomaly));
        }
    }
    assert!(poleward.len() > 20 && equatorward.len() > 20);
    let warm = poleward.iter().sum::<f64>() / poleward.len() as f64;
    let cold = equatorward.iter().sum::<f64>() / equatorward.len() as f64;
    assert!(
        warm > cold + 1.0,
        "poleward {warm:.2} C, equatorward {cold:.2} C"
    );

    // The ocean signal must reach the land it flows past rather than remain a
    // decorative vector over water. Classify immediate coasts by the anomaly
    // in their neighboring ocean and compare the anomaly recorded on land.
    let mut warm_coasts = Vec::new();
    let mut cold_coasts = Vec::new();
    for (pos, terrain) in atlas.genesis.terrain.iter() {
        if terrain.eroded_elevation <= SEA_LEVEL as f32 {
            continue;
        }
        let adjacent_ocean: Vec<f32> = pos
            .neighbors8(atlas.side())
            .into_iter()
            .filter(|neighbor| {
                atlas
                    .genesis
                    .terrain
                    .get(*neighbor)
                    .is_some_and(|cell| cell.eroded_elevation <= SEA_LEVEL as f32)
            })
            .filter_map(|neighbor| {
                Some(
                    atlas
                        .genesis
                        .climate
                        .get(neighbor)?
                        .ocean_temperature_anomaly,
                )
            })
            .collect();
        if adjacent_ocean.is_empty() {
            continue;
        }
        let ocean_signal = adjacent_ocean.iter().sum::<f32>() / adjacent_ocean.len() as f32;
        let land_signal = atlas
            .genesis
            .climate
            .get(pos)
            .unwrap()
            .ocean_temperature_anomaly;
        if ocean_signal > 0.5 {
            warm_coasts.push(f64::from(land_signal));
        } else if ocean_signal < -0.5 {
            cold_coasts.push(f64::from(land_signal));
        }
    }
    assert!(warm_coasts.len() > 8 && cold_coasts.len() > 8);
    let warm_land = average(warm_coasts.into_iter());
    let cold_land = average(cold_coasts.into_iter());
    assert!(
        warm_land > cold_land + 0.3,
        "warm-current coast {warm_land:.2} C, cold-current coast {cold_land:.2} C"
    );
}

#[test]
fn weather_moves_locally_and_conserves_every_water_unit() {
    let atlas = climate(77, 8);
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let initial = weather
        .water
        .audit(ReservoirMass::fresh(
            dynamic_water_total(&weather.cells) as u64
        ))
        .current_water_hu;
    let calm = weather.complete_hour(&atlas, 12.0, 0, 0.0).unwrap();
    assert_eq!(calm.unexplained_water_drift, 0);
    let calm_storm_energy: u64 = weather
        .cells
        .cells
        .values()
        .iter()
        .map(|cell| u64::from(cell.storm_energy))
        .sum();

    let mut wrath = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let wrath_report = wrath.complete_hour(&atlas, 12.0, 0, 100.0).unwrap();
    let wrath_storm_energy: u64 = wrath
        .cells
        .cells
        .values()
        .iter()
        .map(|cell| u64::from(cell.storm_energy))
        .sum();
    assert_eq!(wrath_report.unexplained_water_drift, 0);
    assert_eq!(
        weather
            .water
            .audit(ReservoirMass::fresh(
                dynamic_water_total(&weather.cells) as u64
            ))
            .current_water_hu,
        initial
    );
    assert_eq!(
        wrath
            .water
            .audit(ReservoirMass::fresh(
                dynamic_water_total(&wrath.cells) as u64
            ))
            .current_water_hu,
        initial
    );
    assert!(wrath_storm_energy > calm_storm_energy);

    for hour in 1..=u64::from(YEAR_DAYS) * 24 * 3 {
        let report = weather
            .complete_hour(&atlas, hour as f64 / 24.0, hour, 25.0)
            .unwrap();
        assert_eq!(report.unexplained_water_drift, 0);
    }
    assert_eq!(
        weather
            .water
            .audit(ReservoirMass::fresh(
                dynamic_water_total(&weather.cells) as u64
            ))
            .current_water_hu,
        initial
    );
    assert!(
        weather
            .cells
            .cells
            .values()
            .iter()
            .any(|cell| { cell.precipitation_rate > 0 && cell.cloud_water < u32::MAX })
    );
}

#[test]
fn one_weather_hour_contains_simultaneous_clear_and_precipitating_regions() {
    let atlas = climate(1_337, 16);
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    weather.complete_hour(&atlas, 0.0, 0, 0.0).unwrap();

    let mut raining = Vec::new();
    let mut clear = Vec::new();
    for (pos, climate) in atlas.genesis.climate.iter() {
        let dynamic = *weather.cells.cells.get(pos).unwrap();
        match crate::planet_atlas::weather_sample(*climate, dynamic, 0.0, false).kind {
            LocalWeather::Precipitation | LocalWeather::Storm => raining.push(pos),
            LocalWeather::Clear => clear.push(pos),
            LocalWeather::Overcast => {}
        }
    }

    assert!(
        !raining.is_empty(),
        "the accepted hour has no precipitation"
    );
    assert!(
        !clear.is_empty(),
        "the accepted hour rains over the whole planet"
    );
    let rain = raining[0];
    let farthest_clear = clear
        .into_iter()
        .map(|pos| {
            crate::planet::geodesic_distance(rain.center(atlas.side()), pos.center(atlas.side()))
        })
        .fold(0.0f64, f64::max);
    assert!(
        farthest_clear > 2_000.0,
        "clear and raining regions are not geographically distinct: {farthest_clear:.1} blocks"
    );
}

#[test]
fn weather_slices_are_bounded_and_deterministic() {
    let atlas = climate(909, 8);
    let mut sliced = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let mut whole = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    sliced.begin_hour(0);
    let mut calls = 0usize;
    let report = loop {
        calls += 1;
        if let Some(report) = sliced.advance_slice(&atlas, 24.0, 7, |_| 35.0).unwrap() {
            break report;
        }
    };
    assert_eq!(calls, atlas.dynamic.cells.len().div_ceil(7));
    let whole_report = whole.complete_hour(&atlas, 24.0, 0, 35.0).unwrap();
    assert_eq!(report, whole_report);
    assert_eq!(sliced.cells, whole.cells);
}

#[test]
fn voxel_precipitation_withdraws_the_exact_coarse_reserve() {
    let atlas = climate(910, 8);
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let pos = AtlasPos {
        face: Face::NegX,
        u: 0,
        v: 0,
    };
    let cell = weather.water.cells.get_mut(pos).unwrap();
    cell.runoff = ReservoirMass::fresh(10);
    cell.snow = ReservoirMass::fresh(10);
    weather.water.ledger.initial_water_hu += 20;
    let before = weather
        .water
        .audit(ReservoirMass::fresh(
            dynamic_water_total(&weather.cells) as u64
        ))
        .current_water_hu;

    assert_eq!(
        weather.withdraw_water_cycle_transfer(pos, PrecipitationForm::Rain, 3),
        3
    );
    assert_eq!(
        weather
            .water
            .audit(ReservoirMass::fresh(
                dynamic_water_total(&weather.cells) as u64
            ))
            .current_water_hu,
        before
    );

    // A withdrawal from a cell whose sliced update is already in scratch
    // must survive the atomic grid swap and be accounted as an external
    // transfer, not reported as unexplained drift.
    weather.begin_hour(0);
    assert!(
        weather
            .advance_slice(&atlas, 0.0, 1, |_| 0.0)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        weather.withdraw_water_cycle_transfer(pos, PrecipitationForm::Snow, 4),
        4
    );
    let report = loop {
        if let Some(report) = weather
            .advance_slice(&atlas, 0.0, atlas.dynamic.cells.len(), |_| 0.0)
            .unwrap()
        {
            break report;
        }
    };
    assert_eq!(report.water_cycle_outflow_units, 7);
    assert_eq!(report.unexplained_water_drift, 0);
    assert_eq!(
        weather
            .water
            .audit(ReservoirMass::fresh(
                dynamic_water_total(&weather.cells) as u64
            ))
            .current_water_hu,
        before
    );
}

#[test]
fn weather_front_crosses_a_cube_face_seam() {
    let atlas = climate(908, 8);
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    for cell in weather.cells.cells.values_mut() {
        *cell = Default::default();
    }
    for face in Face::ALL {
        for v in 0..atlas.side() {
            for u in [0, atlas.side() - 1] {
                weather
                    .cells
                    .cells
                    .get_mut(AtlasPos { face, u, v })
                    .unwrap()
                    .cloud_water = 8_000;
            }
        }
    }
    weather.water.ledger.initial_water_hu = weather
        .water
        .audit(ReservoirMass::fresh(
            dynamic_water_total(&weather.cells) as u64
        ))
        .current_water_hu;
    let before = weather
        .water
        .audit(ReservoirMass::fresh(
            dynamic_water_total(&weather.cells) as u64
        ))
        .current_water_hu;
    weather.complete_hour(&atlas, 18.0, 0, 0.0).unwrap();
    assert_eq!(
        weather
            .water
            .audit(ReservoirMass::fresh(
                dynamic_water_total(&weather.cells) as u64
            ))
            .current_water_hu,
        before
    );
    let mut crossed = false;
    for face in Face::ALL {
        for v in 0..atlas.side() {
            let source = AtlasPos {
                face,
                u: atlas.side() - 1,
                v,
            };
            let east = source.step(Direction4::East, atlas.side()).pos;
            if east.face != face && weather.cells.cells.get(east).unwrap().cloud_water > 0 {
                crossed = true;
            }
        }
    }
    assert!(
        crossed,
        "a coherent cloud front must advect across a face seam"
    );
}

#[test]
fn precipitation_form_uses_the_local_thermal_column() {
    let atlas = climate(31, 8);
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    weather.complete_hour(&atlas, 54.0, 0, 20.0).unwrap();
    let equator = SurfacePos::new(Face::PosZ, 4_096, 4_096).unwrap();
    let north_pole = SurfacePos::new(Face::PosY, 4_096, 4_096).unwrap();
    let warm = weather.sample(&atlas, equator, 54.0, false);
    let cold = weather.sample(&atlas, north_pole, 108.0, false);
    if warm.kind.precipitating() {
        assert_eq!(warm.precipitation, PrecipitationForm::Rain);
    }
    if cold.kind.precipitating() {
        assert_eq!(cold.precipitation, PrecipitationForm::Snow);
    }
    assert_ne!(
        warm.temperature_c.total_cmp(&cold.temperature_c),
        std::cmp::Ordering::Equal
    );
    assert!(matches!(
        warm.kind,
        LocalWeather::Clear
            | LocalWeather::Overcast
            | LocalWeather::Precipitation
            | LocalWeather::Storm
    ));
}

#[test]
fn guest_weather_matches_the_host_across_a_face_seam() {
    let atlas = Arc::new(climate(404, 8));
    let reg = base_reg();
    let mut host = World::new_with_atlas(
        404,
        tmp_dir("climate-host-seam"),
        reg.clone(),
        atlas.clone(),
    );
    host.force_local_weather("storm");
    let source = SurfacePos::new(Face::PosZ, 8_191, 4_096).unwrap();
    let crossed = crate::planet::step4(source, Direction4::East).pos;
    let center = atlas.atlas_pos(source);
    let mut positions = vec![center];
    positions.extend(center.neighbors8(atlas.side()));
    positions.sort();
    positions.dedup();
    let cells = positions
        .into_iter()
        .map(|pos| {
            let center = pos.center(atlas.side());
            let surface = SurfacePos::new(
                center.face,
                center.u.floor() as u16,
                center.v.floor() as u16,
            )
            .unwrap();
            (pos, host.weather_at_surface(surface))
        })
        .collect();
    let mut guest = ReplicaWorld::new(404, reg, 0.0);
    guest.observations_mut().set_weather(atlas.side(), cells);
    assert_eq!(
        guest.weather_at_surface(source).kind,
        host.weather_at_surface(source).kind
    );
    assert_eq!(
        guest.weather_at_surface(crossed).kind,
        host.weather_at_surface(crossed).kind
    );
    assert_eq!(guest.weather_at_surface(crossed).kind, LocalWeather::Storm);
}

#[test]
fn local_weather_state_persists_and_round_trips() {
    let root = tmp_dir("climate-weather-persistence");
    let atlas = climate(505, 8);
    atlas.write_new(&root).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    for hour in 0..12 {
        weather
            .complete_hour(&atlas, hour as f64 / 24.0, hour, 60.0)
            .unwrap();
    }
    atlas
        .save_dynamic_snapshot(&root, &weather.cells, &weather.water)
        .unwrap();
    let loaded = PlanetAtlas::load_fixture(&root).unwrap();
    assert_eq!(loaded.dynamic, weather.cells);
    assert_eq!(loaded.water_cycle, weather.water);
}
