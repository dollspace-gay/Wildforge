//! Read contracts shared by authority, replicas, physics, and presentation.

use std::sync::Arc;

use super::World;
use super::local_structure::LocalStructure;
use crate::chunk::{CHUNK_Y, Chunk, ChunkPos};
use crate::planet::{BlockPos, SurfacePos};
use crate::registry::{AIR, BlockId, Registry};

/// Resident voxel observations with no generation, save, or mutation capability.
/// Registry handles may be cloned into immutable mesh jobs, so the borrowed Arc
/// is intentional. Missing terrain retains the established air/open-sky policy.
pub trait TerrainRead {
    fn registry(&self) -> &Arc<Registry>;
    fn chunk(&self, position: ChunkPos) -> Option<&Chunk>;
    fn is_hidden(&self, position: BlockPos) -> bool;
    fn hidden_in_chunk(&self, position: ChunkPos) -> Vec<BlockPos>;

    /// Dry head/feet cells with visible solid ground below.
    fn standable_at(&self, surface: crate::planet::SurfacePos, y: i32) -> bool {
        // Fluid is not solid, so a seabed column used to read as
        // "standable" and players were dropped on the ocean floor.
        // Somewhere to stand means dry air for the body, too.
        let clear = |b: BlockId| !self.registry().is_solid(b) && !self.registry().is_fluid(b);
        let at = |height: i32| {
            crate::planet::BlockPos::new(surface.face(), surface.u(), height as u8, surface.v())
                .expect("standable height is inside the vertical shell")
        };
        let stands = |height: i32| {
            let pos = at(height);
            self.registry().is_solid(self.get_block_at(pos)) && !self.is_hidden(pos)
        };
        stands(y - 1) && clear(self.get_block_at(at(y))) && clear(self.get_block_at(at(y + 1)))
    }

    fn has_chunk(&self, position: ChunkPos) -> bool {
        self.chunk(position).is_some()
    }

    fn get_block_at(&self, position: BlockPos) -> BlockId {
        let (x, y, z) = position.local();
        self.chunk(position.chunk())
            .map_or(AIR, |chunk| chunk.get(x, y, z))
    }

    fn get_meta_at(&self, position: BlockPos) -> u8 {
        let (x, y, z) = position.local();
        self.chunk(position.chunk())
            .map_or(0, |chunk| chunk.meta(x, y, z))
    }

    fn get_water_salt_at(&self, position: BlockPos) -> u16 {
        let (x, y, z) = position.local();
        self.chunk(position.chunk())
            .map_or(0, |chunk| chunk.water_salt(x, y, z))
    }

    fn water_mass_at(&self, position: BlockPos) -> Option<crate::planet_atlas::ReservoirMass> {
        let volume = self.registry().water_volume(self.get_block_at(position))?;
        Some(crate::planet_atlas::ReservoirMass {
            water_hu: u64::from(volume) * crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL,
            salt_mass: u64::from(self.get_water_salt_at(position)),
        })
    }

    fn get_soil_salinity_at(&self, position: BlockPos) -> u8 {
        let (x, y, z) = position.local();
        self.chunk(position.chunk())
            .map_or(0, |chunk| chunk.soil_salinity(x, y, z))
    }

    fn can_sift_salvage_at(&self, position: BlockPos) -> bool {
        let block = self.registry().block(self.get_block_at(position));
        block.brush.is_none()
            && block.interaction.is_none()
            && block.hardness.is_some()
            && block.material_class == crate::registry::MaterialClass::TransformativeFinite
    }

    fn fertility_at_pos(&self, position: BlockPos) -> u8 {
        if self
            .registry()
            .block(self.get_block_at(position))
            .fert_tiles
            .is_some()
        {
            super::soil::fert_of(self.get_meta_at(position))
        } else {
            0
        }
    }

    fn light_at_pos(&self, position: BlockPos) -> (u8, u8) {
        let (x, y, z) = position.local();
        self.chunk(position.chunk())
            .map_or((0, 15), |chunk| chunk.light_intensity(x, y, z))
    }

    fn light_rgb_at_pos(&self, position: BlockPos) -> ([u8; 3], u8) {
        let (x, y, z) = position.local();
        self.chunk(position.chunk())
            .map_or(([0; 3], 15), |chunk| chunk.light(x, y, z))
    }

    fn surface_height_at(&self, surface: SurfacePos) -> i32 {
        for y in (0..CHUNK_Y as i32).rev() {
            let position = BlockPos::new(surface.face(), surface.u(), y as u8, surface.v())
                .expect("surface column and height are validated");
            if self.registry().is_solid(self.get_block_at(position)) && !self.is_hidden(position) {
                return y;
            }
        }
        0
    }

    fn is_open_water_at(&self, surface: SurfacePos) -> bool {
        self.surface_height_at(surface) < crate::chunk::SEA_LEVEL - 1
            && BlockPos::new(
                surface.face(),
                surface.u(),
                (crate::chunk::SEA_LEVEL - 1) as u8,
                surface.v(),
            )
            .is_ok_and(|position| self.registry().is_water(self.get_block_at(position)))
    }

    #[cfg(test)]
    fn get_block(&self, x: i32, y: i32, z: i32) -> BlockId {
        BlockPos::of_world(x, y, z).map_or(AIR, |position| self.get_block_at(position))
    }
}

/// Structure selection needs a spatial scene in addition to resident terrain.
/// Ordinary collision/meshing consumers only require TerrainRead.
pub trait SceneRead: TerrainRead {
    fn local_structures(&self) -> &[LocalStructure];
}

impl TerrainRead for World {
    fn registry(&self) -> &Arc<Registry> {
        &self.reg
    }
    fn chunk(&self, position: ChunkPos) -> Option<&Chunk> {
        World::chunk(self, position)
    }
    fn is_hidden(&self, position: BlockPos) -> bool {
        World::is_hidden(self, position)
    }
    fn hidden_in_chunk(&self, position: ChunkPos) -> Vec<BlockPos> {
        World::hidden_in_chunk(self, position)
    }
}

impl SceneRead for World {
    fn local_structures(&self) -> &[LocalStructure] {
        World::local_structures(self)
    }
}
