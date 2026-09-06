//! Normals scenarios.

use super::*;

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
