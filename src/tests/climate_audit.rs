//! The live weather audit must count the same reservoirs as the atlas audit.

use crate::planet_atlas::{PlanetAtlas, PlanetaryWeather};

#[test]
fn live_weather_audit_counts_cloud_water_as_well_as_vapor() {
    let atlas = PlanetAtlas::fixture(42, 8).unwrap();
    let clouds: u64 = atlas
        .dynamic
        .cells
        .values()
        .iter()
        .map(|cell| u64::from(cell.cloud_water))
        .sum();
    assert!(clouds > 0, "regression fixture must contain cloud water");
    let expected = atlas.water_audit();
    assert_eq!(expected.unexplained_water_delta_hu, 0);
    let weather = PlanetaryWeather::new(atlas.dynamic, atlas.water_cycle);
    assert_eq!(weather.water_audit(), expected);
}
