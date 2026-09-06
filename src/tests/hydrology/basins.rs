//! Basins scenarios.

use super::*;

#[test]
fn oceans_lakes_sills_salinity_and_hypsometry_are_closed() {
    let atlas = atlas();
    let ocean_cells: u64 = atlas
        .hydrology
        .oceans
        .iter()
        .map(|ocean| u64::from(ocean.cell_count))
        .sum();
    let ocean_share = ocean_cells as f64 / atlas.genesis.hydrology.len() as f64;
    assert!((0.35..=0.80).contains(&ocean_share));
    let ocean_area: f64 = atlas.hydrology.oceans.iter().map(|ocean| ocean.area).sum();
    let dominant = atlas
        .hydrology
        .oceans
        .iter()
        .find(|ocean| ocean.id == atlas.hydrology.dominant_ocean_id)
        .unwrap();
    assert!(dominant.area / ocean_area >= 0.90);
    assert!(atlas.hydrology.oceans.iter().all(|ocean| {
        ocean.salinity >= 192
            && ocean
                .volume_elevation_curve
                .windows(2)
                .all(|pair| pair[1].volume_units >= pair[0].volume_units)
    }));
    assert!(atlas.hydrology.lakes.len() >= 3);
    assert!(
        atlas
            .hydrology
            .lakes
            .iter()
            .any(|lake| lake.outlet.is_some())
    );
    assert!(atlas.hydrology.lakes.iter().any(|lake| {
        lake.outlet.is_none() && (lake.salinity >= 64 || lake.class == LakeClass::SeasonalPlaya)
    }));
    for lake in &atlas.hydrology.lakes {
        assert!(lake.cell_count > 0 && lake.catchment_area > 0.0);
        assert!(!lake.volume_elevation_curve.is_empty());
        assert!(lake.volume_elevation_curve.windows(2).all(|pair| {
            pair[1].elevation >= pair[0].elevation && pair[1].volume_units >= pair[0].volume_units
        }));
        assert!(lake.surface_elevation <= lake.spill_elevation + 0.001);
        assert!(
            (lake.baseline_inflow - lake.baseline_evaporation - lake.baseline_outflow).abs()
                <= lake.baseline_inflow.max(1.0) * 1.0e-8
        );
        if let Some(outlet) = lake.outlet {
            assert!(lake.sink.neighbors8(atlas.side()).contains(&outlet) || lake.cell_count > 1);
        }
        match lake.class {
            LakeClass::ThroughFlowFresh => {
                assert!(lake.outlet.is_some() && lake.salinity < 64)
            }
            LakeClass::TerminalFresh => {
                assert!(lake.outlet.is_none() && lake.salinity < 64)
            }
            LakeClass::SalineTerminal => {
                assert!(lake.outlet.is_none() && lake.salinity >= 64)
            }
            LakeClass::SeasonalPlaya => {
                assert!(lake.outlet.is_none() && lake.baseline_volume_units == 0)
            }
            LakeClass::Rift | LakeClass::VolcanicCrater | LakeClass::GlacialAlpine => {}
        }
    }
    assert!(atlas.hydrology.lakes.iter().any(|lake| {
        lake.outlet.is_none()
            && matches!(
                lake.class,
                LakeClass::SalineTerminal | LakeClass::SeasonalPlaya
            )
            && atlas
                .genesis
                .climate
                .get(lake.sink)
                .is_some_and(|climate| climate.aridity > 1.0)
    }));
}

#[test]
fn terminal_lake_concentration_precipitates_salt_without_losing_it() {
    let atlas = atlas();
    let lake = atlas
        .hydrology
        .lakes
        .iter()
        .find(|lake| {
            matches!(
                lake.class,
                LakeClass::TerminalFresh | LakeClass::SalineTerminal | LakeClass::SeasonalPlaya
            )
        })
        .expect("the qualification atlas has a terminal lake");
    let id = surface_reservoir_id(SurfaceReservoirKind::Lake, lake.id);
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    if weather.water.reservoir_mut(id).unwrap().coarse.water_hu == 0 {
        let source = weather
            .water
            .reservoirs
            .iter()
            .find(|reservoir| reservoir.id != id && reservoir.coarse.water_hu >= 4_096)
            .unwrap()
            .id;
        assert_eq!(
            weather.breach_surface_reservoir(source, id, 4_096).water_hu,
            4_096
        );
    }
    let reservoir = weather.water.reservoir_mut(id).unwrap();
    let saturated = reservoir.coarse.water_hu.saturating_mul(250);
    let added = saturated.saturating_sub(reservoir.coarse.salt_mass);
    reservoir.coarse.salt_mass = saturated;
    weather.water.ledger.initial_salt_mass =
        weather.water.ledger.initial_salt_mass.saturating_add(added);
    let before = weather.water.audit(ReservoirMass::fresh(
        dynamic_water_total(&weather.cells) as u64
    ));
    weather.complete_hour(atlas, 20.0, 0, 0.0).unwrap();
    let after = weather.water.audit(ReservoirMass::fresh(
        dynamic_water_total(&weather.cells) as u64
    ));
    assert!(
        after.precipitated_salt_mass > before.precipitated_salt_mass,
        "supersaturated terminal water leaves an audited salt precipitate"
    );
    assert_eq!(after.current_salt_mass, before.current_salt_mass);
    assert_eq!(after.unexplained_salt_delta, 0);
}
