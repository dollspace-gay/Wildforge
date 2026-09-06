//! Transfers scenarios.

use super::*;

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
