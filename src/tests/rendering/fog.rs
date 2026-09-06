//! Fog scenarios.

#[test]
fn above_water_fog_is_monotonic_and_reaches_directional_sky() {
    let range = 184.0;
    let mut previous = 0.0;
    for step in 0..=200 {
        let distance = range * step as f32 / 200.0;
        let factor = crate::sky::atmospheric_fog_factor(distance, range);
        assert!(
            factor + f32::EPSILON >= previous,
            "fog retreated at {distance}"
        );
        previous = factor;
    }
    assert_eq!(crate::sky::atmospheric_fog_factor(range * 0.89, range), 0.0);
    assert_eq!(crate::sky::atmospheric_fog_factor(range, range), 1.0);

    let params = crate::sky::SkyParams {
        sun_dir: glam::Vec3::new(0.7, 0.5, 0.2).normalize(),
        up: glam::Vec3::Y,
        gloom: 0.0,
        overcast: glam::Vec3::splat(0.6),
        moon_fill: glam::Vec3::ZERO,
    };
    let toward_sun = crate::sky::radiance(params.sun_dir, &params);
    let away = crate::sky::radiance(-params.sun_dir, &params);
    assert_ne!(
        toward_sun, away,
        "above-water far color must remain directional"
    );
    let terrain = glam::Vec3::new(0.2, 0.3, 0.4);
    assert_eq!(
        terrain.lerp(toward_sun, crate::sky::atmospheric_fog_factor(range, range)),
        toward_sun
    );

    let shader = crate::shader::WORLD;
    assert!(shader.contains("smoothstep(u.cam.w * 0.90, u.cam.w * 1.0, dist)"));
    assert!(shader.contains("select(sky_radiance(rd), u.sky.rgb, u.misc.x > 0.5)"));
}

#[test]
fn fog_distance_is_cube_face_invariant() {
    use glam::{Quat, Vec3};
    let radius = crate::planet::PLANET_RADIUS as f32;
    let camera = Vec3::new(radius + 72.0, 17.0, -9.0);
    let world = Vec3::new(radius + 68.0, 133.0, 21.0);
    let expected = crate::sky::planetary_fog_distance(camera, world);
    for rotation in [
        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        Quat::from_rotation_z(std::f32::consts::PI),
    ] {
        let actual = crate::sky::planetary_fog_distance(rotation * camera, rotation * world);
        assert!(
            (actual - expected).abs() < 0.10,
            "cube-face rotation changed fog: {expected} -> {actual}"
        );
    }
}
