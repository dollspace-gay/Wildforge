//! Voxel access coordinator for the authoritative world.

use super::{AIR, BlockId, CHUNK_Y, Chunk, ChunkPos, HashMap, TerrainRead, World};

impl World {
    pub fn dirty_chunks(&self) -> Vec<ChunkPos> {
        self.chunks.dirty_positions()
    }

    pub fn mark_chunk_meshed(&mut self, pos: ChunkPos) {
        self.chunks.mark_dirty(pos, false);
    }

    #[cfg(test)]
    pub(crate) fn chunks(&self) -> &HashMap<ChunkPos, Chunk> {
        self.chunks.test_map()
    }

    #[cfg(test)]
    pub(crate) fn chunks_mut(&mut self) -> &mut HashMap<ChunkPos, Chunk> {
        self.chunks.test_map_mut()
    }

    #[cfg(test)]
    pub(crate) fn planetary_weather_for_test(
        &self,
    ) -> Option<&crate::planet_atlas::PlanetaryWeather> {
        self.weather_state.live()
    }

    #[cfg(test)]
    pub(crate) fn planetary_weather_for_test_mut(
        &mut self,
    ) -> Option<&mut crate::planet_atlas::PlanetaryWeather> {
        self.weather_state.live_mut()
    }

    pub fn get_block_at(&self, pos: crate::planet::BlockPos) -> BlockId {
        TerrainRead::get_block_at(self, pos)
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> BlockId {
        crate::planet::BlockPos::of_world(x, y, z)
            .map(|pos| self.get_block_at(pos))
            .unwrap_or(AIR)
    }

    /// Metadata byte at a world position (octant mask for sub-voxel blocks).
    pub fn get_meta_at(&self, pos: crate::planet::BlockPos) -> u8 {
        TerrainRead::get_meta_at(self, pos)
    }

    pub fn get_water_salt_at(&self, pos: crate::planet::BlockPos) -> u16 {
        TerrainRead::get_water_salt_at(self, pos)
    }

    pub fn get_soil_salinity_at(&self, pos: crate::planet::BlockPos) -> u8 {
        TerrainRead::get_soil_salinity_at(self, pos)
    }

    pub fn water_mass_at(
        &self,
        pos: crate::planet::BlockPos,
    ) -> Option<crate::planet_atlas::ReservoirMass> {
        TerrainRead::water_mass_at(self, pos)
    }

    #[cfg(test)]
    pub fn live_water_audit(&self) -> Option<crate::planet_atlas::WaterAudit> {
        self.weather_state.live()
            .map(crate::planet_atlas::PlanetaryWeather::water_audit)
    }

    #[cfg(test)]
    pub fn ecology_soil_water_hu_at(&self, surface: crate::planet::SurfacePos) -> Option<u64> {
        let atlas = self.planet_atlas.as_ref()?;
        self.weather_state.live()
            .map(|weather| weather.ecology_soil_water_hu(atlas.atlas_pos(surface)))
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub fn get_meta(&self, x: i32, y: i32, z: i32) -> u8 {
        crate::planet::BlockPos::of_world(x, y, z)
            .map(|pos| self.get_meta_at(pos))
            .unwrap_or(0)
    }

    /// Y of the highest solid block in a column (for spawn placement).
    #[cfg(test)]
    pub fn surface_height(&self, x: i32, z: i32) -> i32 {
        let surface = crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
            .expect("legacy surface query is within the bounded porting window");
        self.surface_height_at(surface)
    }

    pub fn surface_height_at(&self, surface: crate::planet::SurfacePos) -> i32 {
        TerrainRead::surface_height_at(self, surface)
    }

    /// How many chunks carry a random-tick stamp.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn stamp_count(&self) -> usize {
        self.last_random.len()
    }

    /// The headroom a flier standing at `y` actually has: the first
    /// solid at or below it, and the first solid above it. Bounded so a
    /// mob in open sky or a sealed shaft costs a fixed scan.
    pub fn air_column_at(&self, pos: crate::planet::EntityPos, y: i32) -> (i32, i32) {
        let surface = crate::planet::SurfacePos::new(
            pos.face(),
            pos.u().floor() as u16,
            pos.v().floor() as u16,
        )
        .expect("canonical entity has a valid surface cell");
        let at = |height: i32| {
            crate::planet::BlockPos::new(surface.face(), surface.u(), height as u8, surface.v())
                .expect("air column height is inside the shell")
        };
        let solid = |height: i32| {
            let pos = at(height);
            self.reg.is_solid(self.get_block_at(pos)) && !self.is_hidden(pos)
        };
        let floor = (y - 64).max(0)..=y;
        let floor = floor
            .rev()
            .find(|&fy| solid(fy))
            // An ungenerated/open column has no trustworthy ground. A
            // floater should hold station until terrain is resident, not
            // interpret missing data as a sixty-block abyss.
            .unwrap_or((y - 2).max(0));
        let ceil = ((y + 1)..=(y + 40).min(CHUNK_Y as i32 - 1))
            .find(|&cy| solid(cy))
            .unwrap_or(CHUNK_Y as i32);
        (floor, ceil)
    }

    /// Can a player body stand with its feet in cell y? Feet and
    /// head clear of solids, solid ground directly underfoot.
    pub(super) fn standable_at(&self, surface: crate::planet::SurfacePos, y: i32) -> bool {
        TerrainRead::standable_at(self, surface, y)
    }
}
