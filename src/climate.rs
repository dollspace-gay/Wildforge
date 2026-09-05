//! Immutable climate observations shared by terrain generation and replicas.

use noise::{NoiseFn, Perlin};

use crate::planet::{PLANET_RADIUS, SurfacePos, surface_to_unit};

/// The atlas-free temperature field. Sampling this value cannot generate a
/// chunk or change weather; a replica uses it only until host observations arrive.
pub(crate) struct TemperatureField {
    noise: Perlin,
}

impl TemperatureField {
    pub(crate) fn new(seed: u32) -> Self {
        Self { noise: Perlin::new(seed.wrapping_add(4)) }
    }

    pub(crate) fn sample(&self, position: SurfacePos) -> f32 {
        let unit = surface_to_unit(position.center());
        let latitude_heat = 1.0 - 2.0 * unit.y.abs() as f32;
        (latitude_heat * 0.82
            + surface_noise(&self.noise, position, 2_300.0, [2.7, -4.9, 8.3]) * 0.34)
            .clamp(-1.0, 1.0)
    }

    #[cfg(test)]
    pub(crate) fn planar_sample(&self, x: f64, z: f64) -> f32 {
        self.noise.get([x * 0.0004, z * 0.0004]) as f32
    }
}

pub(crate) fn surface_noise(
    noise: &Perlin,
    position: SurfacePos,
    scale: f64,
    offset: [f64; 3],
) -> f32 {
    let point = surface_to_unit(position.center()) * (PLANET_RADIUS / scale);
    noise.get([point.x + offset[0], point.y + offset[1], point.z + offset[2]]) as f32
}

/// Preserve the latitude-scaled seasonal anomaly used by atlas-free worlds.
pub(crate) fn seasonal_temperature(field: f32, position: SurfacePos, day: f64) -> f32 {
    let latitude = crate::planet_atlas::latitude_longitude(surface_to_unit(position.center())).0;
    let phase = std::f64::consts::TAU * day / f64::from(crate::planet_atlas::YEAR_DAYS);
    let delta = (phase.sin() * latitude.sin() * 14.0) as f32;
    field * 22.0 + 8.0 + delta
}
