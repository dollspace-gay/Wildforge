//! Persistence scenarios.

use super::*;

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
