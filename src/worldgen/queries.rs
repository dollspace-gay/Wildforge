//! Stable generator query API forwarding to its immutable geography owner.

use super::{Biome, Climate, Generator, Geography, Province, ProvinceKey};
use crate::planet::SurfacePos;
#[cfg(test)]
use super::Tectonics;

impl Generator {
    pub(crate) fn geography(&self) -> &Geography { &self.geography }

    pub fn pluton_at_surface(&self, pos: SurfacePos) -> bool { self.geography.pluton_at_surface(pos) }
    pub fn prospect_at(&self, pos: SurfacePos) -> super::ProspectReading { self.geography.prospect_at(pos) }
    pub fn pipe_at(&self, pos: crate::chunk::ChunkPos) -> Option<(usize, usize, bool)> {
        self.geography.pipe_at(pos)
    }
    pub fn geode_at(&self, pos: crate::chunk::ChunkPos) -> Option<(usize, usize, i32, i32)> {
        self.geography.geode_at(pos)
    }

    #[cfg(test)]
    pub const PROVINCE_CELLS: u8 = Geography::PROVINCE_CELLS;

    pub fn climate_at(&self, pos: SurfacePos) -> Climate { self.geography.climate_at(pos) }
    pub fn biome_at(&self, pos: SurfacePos) -> Biome { self.geography.biome_at(pos) }
    pub fn province_at(&self, pos: SurfacePos) -> Province { self.geography.province_at(pos) }
    pub fn province_offset(&self, key: ProvinceKey, du: i32, dv: i32) -> ProvinceKey {
        self.geography.province_offset(key, du, dv)
    }
    pub fn province_center_at(&self, key: ProvinceKey) -> SurfacePos {
        self.geography.province_center_at(key)
    }
    pub fn province_keys_near(&self, pos: SurfacePos, radius_blocks: f64) -> Vec<ProvinceKey> {
        self.geography.province_keys_near(pos, radius_blocks)
    }
    pub fn heart_biome_at(&self, pos: SurfacePos) -> Biome { self.geography.heart_biome_at(pos) }
    pub fn heart_nearness_at(&self, pos: SurfacePos) -> f32 { self.geography.heart_nearness_at(pos) }
    pub(crate) fn plate_relief(&self, climate: &Climate) -> f32 {
        self.geography.plate_relief(climate)
    }
    pub(super) fn base_offset_at(&self, pos: SurfacePos, climate: &Climate) -> f32 {
        self.geography.base_offset_at(pos, climate)
    }
    pub(super) fn is_badlands(climate: &Climate) -> bool { Geography::is_badlands(climate) }

    #[cfg(test)]
    pub fn climate(&self, x: i32, z: i32) -> Climate { self.geography.climate(x, z) }
    #[cfg(test)]
    pub fn biome(&self, x: i32, z: i32) -> Biome { self.geography.biome(x, z) }
    #[cfg(test)]
    pub fn biome_from_at(&self, x: i32, z: i32, climate: &Climate) -> Biome {
        self.geography.biome_from_at(x, z, climate)
    }
    #[cfg(test)]
    pub fn tectonics(&self, x: i32, z: i32) -> Tectonics { self.geography.tectonics(x, z) }
    #[cfg(test)]
    pub fn province(&self, x: i32, z: i32) -> Province { self.geography.province(x, z) }
    #[cfg(test)]
    pub(super) fn base_offset(&self, x: i32, z: i32, climate: &Climate) -> f32 {
        self.geography.base_offset(x, z, climate)
    }
}
