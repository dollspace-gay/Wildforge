//! Astronomy scenarios.

use super::*;

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
