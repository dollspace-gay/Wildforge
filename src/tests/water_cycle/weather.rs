//! Weather scenarios.

use super::*;

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
