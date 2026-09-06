//! Basins scenarios.

use super::*;

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
