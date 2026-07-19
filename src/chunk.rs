//! Chunk storage: 16x16 column of blocks, 128 tall (early-alpha Minecraft dimensions).

use crate::registry::BlockId;

pub const CHUNK_X: usize = 16;
pub const CHUNK_Y: usize = 256;
pub const CHUNK_Z: usize = 16;
pub const SEA_LEVEL: i32 = 64;

pub struct Chunk {
    /// Indexed [x][z][y] flattened: (x * CHUNK_Z + z) * CHUNK_Y + y
    blocks: Vec<u16>,
    /// Torch/emitter light 0..15, same indexing. Derived — never saved.
    light_block: Vec<u8>,
    /// Sky light 0..15, scaled by the daylight uniform at render time.
    light_sky: Vec<u8>,
    pub dirty: bool,    // needs remesh
    pub modified: bool, // differs from freshly generated terrain (needs save)
}

impl Chunk {
    pub fn new() -> Chunk {
        let n = CHUNK_X * CHUNK_Y * CHUNK_Z;
        Chunk {
            blocks: vec![0; n],
            light_block: vec![0; n],
            light_sky: vec![0; n],
            dirty: true,
            modified: false,
        }
    }

    #[inline]
    fn idx(x: usize, y: usize, z: usize) -> usize {
        (x * CHUNK_Z + z) * CHUNK_Y + y
    }

    #[inline]
    pub fn light(&self, x: usize, y: usize, z: usize) -> (u8, u8) {
        let i = Self::idx(x, y, z);
        (self.light_block[i], self.light_sky[i])
    }

    #[inline]
    pub fn light_raw_mut(&mut self) -> (&mut [u8], &mut [u8]) {
        (&mut self.light_block, &mut self.light_sky)
    }

    pub fn light_raw(&self) -> (&[u8], &[u8]) {
        (&self.light_block, &self.light_sky)
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> BlockId {
        BlockId(self.blocks[Self::idx(x, y, z)])
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, b: BlockId) {
        self.blocks[Self::idx(x, y, z)] = b.0;
    }

    pub fn raw(&self) -> &[u16] {
        &self.blocks
    }

    pub fn raw_mut(&mut self) -> &mut [u16] {
        &mut self.blocks
    }
}

/// Chunk coordinate (world block x = cx * 16 + local x).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub fn of_world(wx: i32, wz: i32) -> ChunkPos {
        ChunkPos {
            x: wx.div_euclid(CHUNK_X as i32),
            z: wz.div_euclid(CHUNK_Z as i32),
        }
    }
}
