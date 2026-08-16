//! Chunk storage: 16x16 columns of blocks, 256 cells tall.
//!
//! Each per-voxel plane is stored uniform-or-dense. A chunk with
//! no torch in it holds one `[0,0,0]` instead of 65,536 of them, and a chunk
//! with no block state holds one zero instead of 65,536. Those two planes are
//! 256 KB of the 448 KB a chunk used to cost unconditionally, and the
//! overwhelming majority of chunks are uniform in both.

pub use crate::planet::ChunkPos;
use crate::registry::BlockId;

pub const CHUNK_X: usize = 16;
pub const CHUNK_Y: usize = 256;
pub const CHUNK_Z: usize = 16;
pub const SEA_LEVEL: i32 = 64;

/// Cells in one chunk.
pub const CHUNK_CELLS: usize = CHUNK_X * CHUNK_Y * CHUNK_Z;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HydrologyVolumeRecord {
    /// Stable encoded ocean/lake/river identifier from the immutable atlas.
    pub reservoir: u64,
    /// Continuous atlas allocation in hydro units (1/256 block).
    pub baseline_hu: u64,
    /// `baseline_hu - materialized_hu` retained in coarse storage.
    pub residual_hu: i64,
    /// Exact salt mass represented by the materialized water allocation.
    pub salt_mass: u64,
}

/// One per-voxel plane: a single value, or one value per cell.
///
/// Promotes to dense on the first write that disagrees with the uniform value,
/// and can be compacted back when a whole-plane rewrite (a relight, a load)
/// turns out uniform again.
#[derive(Clone)]
pub enum Plane<T> {
    Uniform(T),
    Dense(Box<[T]>),
}

impl<T: Copy + PartialEq> Plane<T> {
    pub fn uniform(value: T) -> Plane<T> {
        Plane::Uniform(value)
    }

    /// Build from a full plane of values, staying uniform when it is.
    pub fn from_slice(values: &[T]) -> Plane<T> {
        match values.first() {
            Some(first) if values.iter().all(|v| v == first) => Plane::Uniform(*first),
            Some(_) => Plane::Dense(values.to_vec().into_boxed_slice()),
            None => unreachable!("a plane is never empty"),
        }
    }

    #[inline]
    pub fn get(&self, i: usize) -> T {
        match self {
            Plane::Uniform(v) => *v,
            Plane::Dense(cells) => cells[i],
        }
    }

    #[inline]
    pub fn set(&mut self, i: usize, value: T) {
        match self {
            // Writing the value it already holds everywhere is free, and it is
            // the common case while worldgen fills air.
            Plane::Uniform(v) if *v == value => {}
            Plane::Uniform(_) => {
                self.densify()[i] = value;
            }
            Plane::Dense(cells) => cells[i] = value,
        }
    }

    /// Materialize and return the whole plane for writing.
    pub fn densify(&mut self) -> &mut [T] {
        if let Plane::Uniform(v) = *self {
            *self = Plane::Dense(vec![v; CHUNK_CELLS].into_boxed_slice());
        }
        match self {
            Plane::Dense(cells) => cells,
            Plane::Uniform(_) => unreachable!("just densified"),
        }
    }

    /// Shrink back to a single value when every cell agrees.
    pub fn compact(&mut self) {
        if let Plane::Dense(cells) = self
            && let Some(first) = cells.first().copied()
            && cells.iter().all(|v| *v == first)
        {
            *self = Plane::Uniform(first);
        }
    }

    /// Does this plane hold exactly these values?
    pub fn matches(&self, values: &[T]) -> bool {
        match self {
            Plane::Uniform(v) => values.iter().all(|other| other == v),
            Plane::Dense(cells) => cells.as_ref() == values,
        }
    }

    /// Heap bytes this plane occupies. What the whole exercise is about.
    #[cfg(test)]
    pub fn heap_bytes(&self) -> usize {
        match self {
            Plane::Uniform(_) => 0,
            Plane::Dense(cells) => std::mem::size_of_val(cells.as_ref()),
        }
    }

    /// Runs of equal values, in index order: `(value, length)`. The RLE codecs
    /// read through this so a uniform plane costs one step instead of 65,536.
    pub fn runs(&self) -> PlaneRuns<'_, T> {
        PlaneRuns { plane: self, at: 0 }
    }
}

pub struct PlaneRuns<'a, T> {
    plane: &'a Plane<T>,
    at: usize,
}

impl<T: Copy + PartialEq> Iterator for PlaneRuns<'_, T> {
    type Item = (T, usize);

    fn next(&mut self) -> Option<(T, usize)> {
        if self.at >= CHUNK_CELLS {
            return None;
        }
        match self.plane {
            Plane::Uniform(v) => {
                self.at = CHUNK_CELLS;
                Some((*v, CHUNK_CELLS))
            }
            Plane::Dense(cells) => {
                let value = cells[self.at];
                let mut run = 1;
                while self.at + run < CHUNK_CELLS && cells[self.at + run] == value {
                    run += 1;
                }
                self.at += run;
                Some((value, run))
            }
        }
    }
}

#[derive(Clone)]
pub struct Chunk {
    /// Indexed [x][z][y] flattened: (x * CHUNK_Z + z) * CHUNK_Y + y
    blocks: Plane<u16>,
    /// Per-voxel metadata byte, same indexing. Meaning is block-defined; for
    /// soil it is fertility plus a rotation stamp; for a compost heap the
    /// amount of greens in it. Zero for blocks that carry no state.
    /// Unlike the light planes this IS gameplay state and is saved.
    meta: Plane<u8>,
    /// Exact dissolved salt mass for water/ice in this voxel. Concentration
    /// metadata is derived from this and visible volume; u16 covers a full
    /// 256-HU cell at salinity 255.
    water_salt: Plane<u16>,
    /// Managed topsoil salinity. This is separate from dissolved water salt:
    /// irrigation can leave salt behind after its water drains or evaporates.
    soil_salinity: Plane<u8>,
    /// Torch/emitter light per channel (r,g,b), each 0..15, same indexing.
    /// Derived — never saved.
    light_block: Plane<[u8; 3]>,
    /// Sky light 0..15, scaled by the daylight uniform at render time.
    light_sky: Plane<u8>,
    hydrology_volumes: Vec<HydrologyVolumeRecord>,
    pub dirty: bool,    // needs remesh
    pub modified: bool, // differs from freshly generated terrain (needs save)
}

/// The immutable planes needed to build a chunk mesh on a worker thread.
/// Gameplay-only water, salinity, and hydrology state deliberately stay in
/// the authoritative chunk instead of being copied every time terrain is
/// remeshed.
#[derive(Clone)]
pub(crate) struct ChunkMeshSnapshot {
    blocks: Plane<u16>,
    meta: Plane<u8>,
    light_block: Plane<[u8; 3]>,
    light_sky: Plane<u8>,
}

impl ChunkMeshSnapshot {
    #[inline]
    pub(crate) fn get(&self, x: usize, y: usize, z: usize) -> BlockId {
        BlockId(self.blocks.get(Chunk::idx(x, y, z)))
    }

    /// Blank one cell to air (used by the mesher to hide settlement growth
    /// cells at capture time; spec 3.4).
    #[inline]
    pub(crate) fn blank(&mut self, x: usize, y: usize, z: usize) {
        let i = Chunk::idx(x, y, z);
        self.blocks.set(i, 0);
        self.meta.set(i, 0);
    }

    #[inline]
    pub(crate) fn meta(&self, x: usize, y: usize, z: usize) -> u8 {
        self.meta.get(Chunk::idx(x, y, z))
    }

    #[inline]
    pub(crate) fn light(&self, x: usize, y: usize, z: usize) -> ([u8; 3], u8) {
        let index = Chunk::idx(x, y, z);
        (self.light_block.get(index), self.light_sky.get(index))
    }
}

impl Chunk {
    pub fn new() -> Chunk {
        Chunk {
            blocks: Plane::uniform(0),
            meta: Plane::uniform(0),
            water_salt: Plane::uniform(0),
            soil_salinity: Plane::uniform(0),
            light_block: Plane::uniform([0; 3]),
            light_sky: Plane::uniform(0),
            hydrology_volumes: Vec::new(),
            dirty: true,
            modified: false,
        }
    }

    pub(crate) fn mesh_snapshot(&self) -> ChunkMeshSnapshot {
        ChunkMeshSnapshot {
            blocks: self.blocks.clone(),
            meta: self.meta.clone(),
            light_block: self.light_block.clone(),
            light_sky: self.light_sky.clone(),
        }
    }

    #[inline]
    fn idx(x: usize, y: usize, z: usize) -> usize {
        (x * CHUNK_Z + z) * CHUNK_Y + y
    }

    /// Per-channel block light (r,g,b) and sky light at a cell.
    #[inline]
    pub fn light(&self, x: usize, y: usize, z: usize) -> ([u8; 3], u8) {
        let i = Self::idx(x, y, z);
        (self.light_block.get(i), self.light_sky.get(i))
    }

    /// Scalar block-light intensity (brightest channel) and sky light — the
    /// value gameplay/spawn logic and light tests read.
    #[inline]
    pub fn light_intensity(&self, x: usize, y: usize, z: usize) -> (u8, u8) {
        let i = Self::idx(x, y, z);
        let c = self.light_block.get(i);
        (c[0].max(c[1]).max(c[2]), self.light_sky.get(i))
    }

    /// Does the stored light already equal this freshly computed field?
    pub fn light_matches(&self, block: &[[u8; 3]], sky: &[u8]) -> bool {
        self.light_block.matches(block) && self.light_sky.matches(sky)
    }

    /// Replace the whole light field, compacting a uniformly dark (or
    /// uniformly lit) result back down to a single value.
    pub fn set_light(&mut self, block: &[[u8; 3]], sky: &[u8]) {
        self.light_block = Plane::from_slice(block);
        self.light_sky = Plane::from_slice(sky);
    }

    /// Derived light runs for the multiplayer chunk codec. Disk chunks omit
    /// these planes and rebuild them; a live host already owns the exact
    /// settled field, so streaming it avoids repeating an expensive cascade
    /// on every guest.
    pub fn light_block_runs(&self) -> PlaneRuns<'_, [u8; 3]> {
        self.light_block.runs()
    }

    pub fn light_sky_runs(&self) -> PlaneRuns<'_, u8> {
        self.light_sky.runs()
    }

    pub fn light_block_raw_mut(&mut self) -> &mut [[u8; 3]] {
        self.light_block.densify()
    }

    pub fn light_sky_raw_mut(&mut self) -> &mut [u8] {
        self.light_sky.densify()
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> BlockId {
        BlockId(self.blocks.get(Self::idx(x, y, z)))
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, b: BlockId) {
        self.blocks.set(Self::idx(x, y, z), b.0);
    }

    /// Block runs in index order, for the RLE codecs.
    pub fn block_runs(&self) -> PlaneRuns<'_, u16> {
        self.blocks.runs()
    }

    /// Every block id in index order. Test-facing, and it materializes a
    /// uniform plane to do it — production code reads `get` or `block_runs`.
    #[cfg(test)]
    pub fn raw(&self) -> Vec<u16> {
        (0..CHUNK_CELLS).map(|i| self.blocks.get(i)).collect()
    }

    pub fn raw_mut(&mut self) -> &mut [u16] {
        self.blocks.densify()
    }

    /// Shrink any plane a whole-plane rewrite left uniform.
    pub fn compact(&mut self) {
        self.blocks.compact();
        self.meta.compact();
        self.water_salt.compact();
        self.soil_salinity.compact();
        self.light_block.compact();
        self.light_sky.compact();
    }

    /// Metadata byte at a cell (soil fertility, compost fill, ...).
    #[inline]
    pub fn meta(&self, x: usize, y: usize, z: usize) -> u8 {
        self.meta.get(Self::idx(x, y, z))
    }

    #[inline]
    pub fn set_meta(&mut self, x: usize, y: usize, z: usize, m: u8) {
        self.meta.set(Self::idx(x, y, z), m);
    }

    /// Metadata runs in index order, for the RLE codecs.
    pub fn meta_runs(&self) -> PlaneRuns<'_, u8> {
        self.meta.runs()
    }

    pub fn meta_raw_mut(&mut self) -> &mut [u8] {
        self.meta.densify()
    }

    #[inline]
    pub fn water_salt(&self, x: usize, y: usize, z: usize) -> u16 {
        self.water_salt.get(Self::idx(x, y, z))
    }

    #[inline]
    pub fn set_water_salt(&mut self, x: usize, y: usize, z: usize, salt_mass: u16) {
        self.water_salt.set(Self::idx(x, y, z), salt_mass);
    }

    pub fn water_salt_runs(&self) -> PlaneRuns<'_, u16> {
        self.water_salt.runs()
    }

    pub fn water_salt_raw_mut(&mut self) -> &mut [u16] {
        self.water_salt.densify()
    }

    #[inline]
    pub fn soil_salinity(&self, x: usize, y: usize, z: usize) -> u8 {
        self.soil_salinity.get(Self::idx(x, y, z))
    }

    #[inline]
    pub fn set_soil_salinity(&mut self, x: usize, y: usize, z: usize, salinity: u8) {
        self.soil_salinity.set(Self::idx(x, y, z), salinity);
    }

    pub fn soil_salinity_runs(&self) -> PlaneRuns<'_, u8> {
        self.soil_salinity.runs()
    }

    pub fn soil_salinity_raw_mut(&mut self) -> &mut [u8] {
        self.soil_salinity.densify()
    }

    pub fn hydrology_volumes(&self) -> &[HydrologyVolumeRecord] {
        &self.hydrology_volumes
    }

    pub fn set_hydrology_volumes(&mut self, records: Vec<HydrologyVolumeRecord>) {
        self.hydrology_volumes = records;
    }

    /// Heap bytes this chunk holds. A chunk of open air costs nothing here;
    /// a chunk of mixed terrain with a torch in it costs the lot.
    #[cfg(test)]
    pub fn heap_bytes(&self) -> usize {
        self.blocks.heap_bytes()
            + self.meta.heap_bytes()
            + self.water_salt.heap_bytes()
            + self.soil_salinity.heap_bytes()
            + self.light_block.heap_bytes()
            + self.light_sky.heap_bytes()
            + self.hydrology_volumes.capacity() * std::mem::size_of::<HydrologyVolumeRecord>()
    }
}
