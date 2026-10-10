//! Immutable optical input preparation and a bounded single-cascade cache.

use crate::chunk::{CHUNK_CELLS, CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos};
use crate::registry::{BlockDef, Registry};
use std::collections::{HashMap, VecDeque, hash_map::Entry};
use std::mem::size_of;

pub(super) const CACHE_BYTES: usize = 64 * 1024 * 1024;

/// Optical properties shared by every voxel carrying the same current ID.
#[derive(Clone, Copy)]
pub(super) struct Cell {
    pub(super) opaque: bool,
    pub(super) cost: u8,          // propagation cost: 1, or 2 through water
    pub(super) emit: [u8; 3],     // per-channel emission
    pub(super) filter: [bool; 3], // stained glass gates channels
}

impl From<&BlockDef> for Cell {
    fn from(definition: &BlockDef) -> Self {
        Self {
            opaque: definition.opaque,
            cost: if definition.water_level.is_some() {
                2
            } else {
                1
            },
            emit: definition.light_rgb,
            filter: definition.light_filter,
        }
    }
}

pub(super) fn cell_palette(registry: &Registry) -> Vec<Cell> {
    registry.blocks.iter().map(Cell::from).collect()
}

/// These inputs depend only on block IDs and current definitions. Solving
/// changes resident light/dirty state, which cannot invalidate them.
pub(super) struct LocalInput {
    pub(super) cells: Vec<Cell>,
    pub(super) has_emission: bool,
    pub(super) initial_sky: Vec<u8>,
    pub(super) frontier: VecDeque<(usize, usize, usize)>,
}

impl LocalInput {
    pub(super) fn new(chunk: &Chunk, palette: &[Cell]) -> Self {
        const NX: usize = CHUNK_X;
        const NY: usize = CHUNK_Y;
        const NZ: usize = CHUNK_Z;
        let idx = |x: usize, y: usize, z: usize| (x * NZ + z) * NY + y;

        // block_runs visits the raw x/z/y index order, with positive lengths
        // summing to CHUNK_CELLS. Extend with actual entries, avoiding a default
        // fill, repeated Plane::get, and any resident-plane materialization.
        // Direct indexing keeps Registry::block's first invalid-ID panic.
        let mut cells = Vec::with_capacity(CHUNK_CELLS);
        let mut has_emission = false;
        for (id, count) in chunk.block_runs() {
            let cell = palette[usize::from(id)];
            has_emission |= cell.emit != [0; 3];
            cells.extend(std::iter::repeat_n(cell, count));
        }
        debug_assert_eq!(
            cells.len(),
            CHUNK_CELLS,
            "invariant: block runs cover one chunk"
        );

        let mut ls = vec![0u8; NX * NY * NZ];
        let mut sky_q: VecDeque<(usize, usize, usize)> = VecDeque::new();
        let mut highest_dim = [None; NX * NZ];

        // Sky columns: full light straight down to the first opaque block,
        // dimming through water.
        for x in 0..NX {
            for z in 0..NZ {
                let mut v = 15u8;
                for y in (0..NY).rev() {
                    let c = cells[idx(x, y, z)];
                    if c.opaque {
                        highest_dim[x * NZ + z].get_or_insert(y);
                        break; // rest of the column stays 0
                    }
                    if c.cost > 1 {
                        v = v.saturating_sub(1);
                    }
                    ls[idx(x, y, z)] = v;
                    if v < 15 {
                        highest_dim[x * NZ + z].get_or_insert(y);
                    }
                    if v == 0 {
                        break;
                    }
                }
            }
        }

        // Initial columns cannot improve vertically: sky never rises downward,
        // water loses one in the scan but costs two in BFS, and opaque cells
        // refuse light. Queue only horizontal improvement sources, in the
        // original x/z/descending-y order. Above every adjacent column's first
        // dim cell, all destinations are already 15, so skip that whole range.
        // Later border/BFS improvements still enqueue their cells normally.
        for x in 0..NX {
            for z in 0..NZ {
                let mut cutoff: Option<usize> = None;
                let mut include = |nx: usize, nz: usize| {
                    if let Some(y) = highest_dim[nx * NZ + nz] {
                        cutoff = Some(cutoff.map_or(y, |previous| previous.max(y)));
                    }
                };
                if x > 0 {
                    include(x - 1, z);
                }
                if x < NX - 1 {
                    include(x + 1, z);
                }
                if z > 0 {
                    include(x, z - 1);
                }
                if z < NZ - 1 {
                    include(x, z + 1);
                }
                let Some(cutoff) = cutoff else {
                    continue;
                };
                for y in (0..=cutoff).rev() {
                    let v = ls[idx(x, y, z)];
                    if v < 2 {
                        continue;
                    }
                    let improves = |nx: usize, nz: usize| {
                        let i = idx(nx, y, nz);
                        let c = cells[i];
                        !c.opaque && v.saturating_sub(c.cost) > ls[i]
                    };
                    if x > 0 && improves(x - 1, z)
                        || x < NX - 1 && improves(x + 1, z)
                        || z > 0 && improves(x, z - 1)
                        || z < NZ - 1 && improves(x, z + 1)
                    {
                        sky_q.push_back((x, y, z));
                    }
                }
            }
        }
        Self {
            cells,
            has_emission,
            initial_sky: ls,
            frontier: sky_q,
        }
    }

    pub(super) fn allocated_bytes(&self) -> usize {
        self.cells.capacity() * size_of::<Cell>()
            + self.initial_sky.capacity() * size_of::<u8>()
            + self.frontier.capacity() * size_of::<(usize, usize, usize)>()
    }
}

pub(super) enum Prepared<'a> {
    Cached(&'a LocalInput),
    Uncached(LocalInput),
}

impl AsRef<LocalInput> for Prepared<'_> {
    fn as_ref(&self) -> &LocalInput {
        match self {
            Self::Cached(input) => input,
            Self::Uncached(input) => input,
        }
    }
}

/// Owned by one cascade, with no retained registry/resident references.
/// The limit counts cached Vec/VecDeque capacities, excluding map metadata,
/// allocator overhead, solve buffers, and one current uncached input.
pub(super) struct InputCache {
    entries: HashMap<ChunkPos, LocalInput>,
    allocated: usize,
    limit: usize,
}

impl InputCache {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            entries: HashMap::new(),
            allocated: 0,
            limit,
        }
    }

    pub(super) fn prepare<'a>(
        &'a mut self,
        position: ChunkPos,
        chunk: &Chunk,
        palette: &[Cell],
    ) -> Prepared<'a> {
        match self.entries.entry(position) {
            Entry::Occupied(entry) => Prepared::Cached(entry.into_mut()),
            Entry::Vacant(entry) => {
                let input = LocalInput::new(chunk, palette);
                let bytes = input.allocated_bytes();
                if bytes <= self.limit.saturating_sub(self.allocated) {
                    self.allocated += bytes;
                    Prepared::Cached(entry.insert(input))
                } else {
                    Prepared::Uncached(input)
                }
            }
        }
    }

    #[cfg(test)]
    pub(super) fn cached_bytes(&self) -> usize {
        self.allocated
    }
}
