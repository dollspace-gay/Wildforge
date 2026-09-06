//! Persistence scenarios.

use super::*;

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
