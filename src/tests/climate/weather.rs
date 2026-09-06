//! Weather scenarios.

use super::*;

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
