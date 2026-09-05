//! Deterministic spherical sites, rotations, and nearest-site lookup.

use glam::DVec3;
use crate::planet_atlas::mix64;

pub(super) fn dvec(array: [f32; 3]) -> DVec3 {
    DVec3::new(
        f64::from(array[0]),
        f64::from(array[1]),
        f64::from(array[2]),
    )
}

pub(super) fn arr(value: DVec3) -> [f32; 3] {
    [value.x as f32, value.y as f32, value.z as f32]
}

pub(super) fn unit_from_hash(seed: u32, salt: u64) -> DVec3 {
    let a = mix64(u64::from(seed) ^ salt);
    let b = mix64(a ^ 0x9e37_79b9_7f4a_7c15);
    let z = (a as f64 / u64::MAX as f64) * 2.0 - 1.0;
    let angle = (b as f64 / u64::MAX as f64) * std::f64::consts::TAU;
    let radius = (1.0 - z * z).max(0.0).sqrt();
    DVec3::new(radius * angle.cos(), z, radius * angle.sin())
}

pub(super) fn rotate(vector: DVec3, axis: DVec3, angle: f64) -> DVec3 {
    let axis = axis.normalize();
    (vector * angle.cos()
        + axis.cross(vector) * angle.sin()
        + axis * axis.dot(vector) * (1.0 - angle.cos()))
    .normalize()
}

pub(super) fn fibonacci_sites(count: usize, seed: u32, salt: u64, jitter: f64) -> Vec<DVec3> {
    let rotation_axis = unit_from_hash(seed, salt ^ 0x726f_7461_7465);
    let rotation_angle = (mix64(u64::from(seed) ^ salt ^ 0x0061_6e67_6c65) as f64
        / u64::MAX as f64)
        * std::f64::consts::TAU;
    let phase = (mix64(u64::from(seed) ^ salt) as f64 / u64::MAX as f64) * std::f64::consts::TAU;
    let golden = std::f64::consts::PI * (3.0 - 5.0f64.sqrt());
    (0..count)
        .map(|index| {
            let y = 1.0 - 2.0 * (index as f64 + 0.5) / count as f64;
            let radius = (1.0 - y * y).sqrt();
            let theta = phase + golden * index as f64;
            let mut site = DVec3::new(radius * theta.cos(), y, radius * theta.sin());
            site = rotate(site, rotation_axis, rotation_angle);
            let random = unit_from_hash(seed, salt ^ index as u64 ^ 0x6a69_7474_6572);
            let tangent = (random - site * random.dot(site)).normalize_or_zero();
            (site * jitter.cos() + tangent * jitter.sin()).normalize()
        })
        .collect()
}

pub(super) fn nearest_two(sites: &[DVec3], point: DVec3) -> (usize, usize, f64, f64) {
    let mut best = (usize::MAX, f64::NEG_INFINITY);
    let mut second = (usize::MAX, f64::NEG_INFINITY);
    for (index, site) in sites.iter().enumerate() {
        let dot = site.dot(point);
        if dot > best.1 {
            second = best;
            best = (index, dot);
        } else if dot > second.1 {
            second = (index, dot);
        }
    }
    (best.0, second.0, best.1, second.1)
}
