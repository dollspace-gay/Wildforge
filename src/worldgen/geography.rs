//! Immutable geography queries with no content bindings or chunk output API.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use noise::Perlin;

use super::{ProvinceKey, ProvinceLabel, Spline, hash2};
use crate::planet::SurfacePos;
use crate::planet_atlas::PlanetAtlas;

mod climate;
mod provinces;
mod relief;
mod prospecting;
mod deposits;

/// Owns seeded climate and province classification, including the derived label
/// cache. Generation and guest observations share this algorithm without giving
/// a guest the terrain generator's content/material bindings or generate method.
pub(crate) struct Geography {
    atlas: Option<Arc<PlanetAtlas>>,
    province_cache: RwLock<HashMap<ProvinceKey, ProvinceLabel>>,
    cont: Perlin,
    ero: Perlin,
    ridge: Perlin,
    temperature: crate::climate::TemperatureField,
    moisture: Perlin,
    pub(super) detail: Perlin,
    pub(super) bandwarp: Perlin,
    pub(super) granite3d: Perlin,
    seed: u32,
    offset_base: Spline,
    mountain_amp: Spline,
}

impl Geography {
    pub(crate) fn new(seed: u32, atlas: Option<Arc<PlanetAtlas>>) -> Self {
        let p = |salt: u32| Perlin::new(seed.wrapping_add(salt));
        Self {
            atlas,
            province_cache: RwLock::new(HashMap::new()),
            cont: p(20),
            ero: p(21),
            ridge: p(22),
            temperature: crate::climate::TemperatureField::new(seed),
            moisture: p(5),
            detail: p(2),
            bandwarp: p(40),
            granite3d: p(42),
            seed,
            // Continental base height: ocean floor -> coast -> inland.
            offset_base: Spline::new(&[
                (-1.0, 38.0),
                (-0.45, 52.0),
                (-0.18, 62.0),
                (-0.05, 66.0),
                (0.2, 72.0),
                (0.6, 84.0),
                (1.0, 92.0),
            ]),
            // Mountain amplitude by erosion (low erosion = young peaks).
            mountain_amp: Spline::new(&[
                (-1.0, 130.0),
                (-0.6, 85.0),
                (-0.3, 38.0),
                (0.0, 14.0),
                (0.5, 5.0),
                (1.0, 0.0),
            ]),
        }
    }

    pub(super) fn chunk_hash(&self, salt: u32, pos: crate::chunk::ChunkPos) -> u32 {
        hash2(
            self.seed ^ salt ^ (pos.face() as u32).wrapping_mul(0x9e37_79b9),
            i32::from(pos.u()),
            i32::from(pos.v()),
        )
    }

    fn radial_noise_at(noise: &Perlin, pos: SurfacePos, y: f64, scale: f64, offset: [f64; 3]) -> f32 {
        crate::climate::radial_noise(noise, pos, y, scale, offset)
    }

    fn noise_at(noise: &Perlin, pos: SurfacePos, scale: f64, offset: [f64; 3]) -> f32 {
        crate::climate::surface_noise(noise, pos, scale, offset)
    }

    fn hash_surface(&self, salt: u32, pos: SurfacePos) -> u32 {
        crate::climate::surface_hash(self.seed, salt, pos)
    }
}
