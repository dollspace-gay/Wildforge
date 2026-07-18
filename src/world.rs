//! World: chunk map with block access, fluid simulation, and versioned
//! persistence (save v2 with a per-world id palette; legacy v1 migrates).

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos};
use crate::registry::{AIR, BlockId, Registry};
use crate::worldgen::Generator;

pub struct World {
    pub chunks: HashMap<ChunkPos, Chunk>,
    pub generator: Generator,
    pub reg: Arc<Registry>,
    #[allow(dead_code)]
    pub seed: u32,
    save_dir: PathBuf,
    /// stored-id -> runtime-id remap for chunks loaded from disk.
    load_remap: Vec<BlockId>,
    water_queue: VecDeque<(i32, i32, i32)>,
    water_queued: HashSet<(i32, i32, i32)>,
}

impl World {
    pub fn new(seed: u32, save_dir: PathBuf, reg: Arc<Registry>) -> World {
        World {
            chunks: HashMap::new(),
            generator: Generator::new(seed, &reg),
            reg,
            seed,
            save_dir,
            load_remap: Vec::new(),
            water_queue: VecDeque::new(),
            water_queued: HashSet::new(),
        }
    }

    /// Load a world from disk (reads seed + palette) or create a fresh one.
    pub fn load_or_create(save_dir: PathBuf, reg: Arc<Registry>) -> World {
        let seed_file = save_dir.join("seed");
        let seed = fs::read_to_string(&seed_file)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or_else(|| {
                let s = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as u32)
                    .unwrap_or(1337);
                let _ = fs::create_dir_all(&save_dir);
                let _ = fs::write(&seed_file, s.to_string());
                s
            });
        let mut w = World::new(seed, save_dir, reg);
        w.load_remap = w.read_palette_remap();
        w
    }

    /// Map every stored numeric id to a current runtime id via string names.
    fn read_palette_remap(&self) -> Vec<BlockId> {
        let Ok(text) = fs::read_to_string(self.save_dir.join("palette")) else {
            return Vec::new();
        };
        let mut remap = Vec::new();
        for line in text.lines() {
            let Some((num, name)) = line.split_once(' ') else { continue };
            let Ok(num) = num.parse::<usize>() else { continue };
            if remap.len() <= num {
                remap.resize(num + 1, self.reg.unknown_block);
            }
            remap[num] = self.reg.block_id(name.trim()).unwrap_or(self.reg.unknown_block);
        }
        remap
    }

    /// Write the current registry as this world's palette (runtime ids are
    /// stored ids from now on).
    fn write_palette(&self) {
        let mut out = String::new();
        for (i, b) in self.reg.blocks.iter().enumerate() {
            out.push_str(&format!("{i} {}\n", b.name));
        }
        let _ = fs::write(self.save_dir.join("palette"), out);
    }

    fn chunk_file(&self, pos: ChunkPos) -> PathBuf {
        self.save_dir.join(format!("c.{}.{}.wfc", pos.x, pos.z))
    }

    pub fn ensure_chunk(&mut self, pos: ChunkPos) -> bool {
        if self.chunks.contains_key(&pos) {
            return false;
        }
        let chunk = self
            .try_load_chunk(pos)
            .unwrap_or_else(|| self.generator.generate(pos, &self.reg));
        self.chunks.insert(pos, chunk);
        true
    }

    fn try_load_chunk(&self, pos: ChunkPos) -> Option<Chunk> {
        let data = fs::read(self.chunk_file(pos)).ok()?;
        let mut chunk = Chunk::new();
        let out = chunk.raw_mut();
        let mut o = 0;

        if !data.starts_with(b"WFC3") {
            return None; // pre-256-height save: regenerate
        }
        // (count u16, id u16) pairs, remapped through the palette.
        let mut i = 4;
        while i + 4 <= data.len() && o < out.len() {
            let count = u16::from_le_bytes([data[i], data[i + 1]]) as usize;
            let stored = u16::from_le_bytes([data[i + 2], data[i + 3]]) as usize;
            let id = self
                .load_remap
                .get(stored)
                .copied()
                .unwrap_or(self.reg.unknown_block);
            let end = (o + count).min(out.len());
            out[o..end].fill(id.0);
            o = end;
            i += 4;
        }
        if o != out.len() {
            return None; // corrupt; regenerate
        }
        chunk.dirty = true;
        chunk.modified = true;
        Some(chunk)
    }

    fn save_chunk(&self, pos: ChunkPos, chunk: &Chunk) -> std::io::Result<()> {
        let raw = chunk.raw();
        let mut buf: Vec<u8> = Vec::with_capacity(4096);
        buf.extend_from_slice(b"WFC3");
        let mut i = 0;
        while i < raw.len() {
            let b = raw[i];
            let mut run = 1usize;
            while i + run < raw.len() && raw[i + run] == b && run < u16::MAX as usize {
                run += 1;
            }
            buf.extend_from_slice(&(run as u16).to_le_bytes());
            buf.extend_from_slice(&b.to_le_bytes());
            i += run;
        }
        let mut f = fs::File::create(self.chunk_file(pos))?;
        f.write_all(&buf)
    }

    pub fn save_modified(&self) {
        let _ = fs::create_dir_all(&self.save_dir);
        self.write_palette();
        for (pos, chunk) in &self.chunks {
            if chunk.modified {
                let _ = self.save_chunk(*pos, chunk);
            }
        }
    }

    /// Remap all in-memory chunks from an old registry to the current one
    /// (used by hot reload). Unknown blocks become the placeholder.
    pub fn remap_from(&mut self, old: &Registry) {
        let map: Vec<BlockId> = old
            .blocks
            .iter()
            .map(|b| self.reg.block_id(&b.name).unwrap_or(self.reg.unknown_block))
            .collect();
        for chunk in self.chunks.values_mut() {
            for cell in chunk.raw_mut() {
                *cell = map.get(*cell as usize).copied().unwrap_or(self.reg.unknown_block).0;
            }
            chunk.dirty = true;
        }
        self.load_remap = self.read_palette_remap();
    }

    pub fn get_block(&self, x: i32, y: i32, z: i32) -> BlockId {
        if y < 0 || y >= CHUNK_Y as i32 {
            return AIR;
        }
        let pos = ChunkPos::of_world(x, z);
        match self.chunks.get(&pos) {
            Some(c) => c.get(
                x.rem_euclid(CHUNK_X as i32) as usize,
                y as usize,
                z.rem_euclid(CHUNK_Z as i32) as usize,
            ),
            None => AIR,
        }
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, b: BlockId) {
        if y < 0 || y >= CHUNK_Y as i32 {
            return;
        }
        let pos = ChunkPos::of_world(x, z);
        let lx = x.rem_euclid(CHUNK_X as i32) as usize;
        let lz = z.rem_euclid(CHUNK_Z as i32) as usize;
        if let Some(c) = self.chunks.get_mut(&pos) {
            c.set(lx, y as usize, lz, b);
            c.dirty = true;
            c.modified = true;
        }
        let mut touch = |dx: i32, dz: i32| {
            let np = ChunkPos { x: pos.x + dx, z: pos.z + dz };
            if let Some(c) = self.chunks.get_mut(&np) {
                c.dirty = true;
            }
        };
        if lx == 0 {
            touch(-1, 0);
        } else if lx == CHUNK_X - 1 {
            touch(1, 0);
        }
        if lz == 0 {
            touch(0, -1);
        } else if lz == CHUNK_Z - 1 {
            touch(0, 1);
        }
        self.wake_water(x, y, z);
    }

    pub fn save_dir_for_saving(&self) -> PathBuf {
        self.save_dir.clone()
    }

    #[cfg(test)]
    pub fn save_dir_for_test(&self) -> PathBuf {
        self.save_dir.clone()
    }

    // ---------------- fluids ----------------

    fn schedule_water(&mut self, x: i32, y: i32, z: i32) {
        if self.water_queued.insert((x, y, z)) {
            self.water_queue.push_back((x, y, z));
        }
    }

    pub fn wake_water(&mut self, x: i32, y: i32, z: i32) {
        for (dx, dy, dz) in [(0, 0, 0), (1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
            self.schedule_water(x + dx, y + dy, z + dz);
        }
    }

    fn desired_flow(&self, x: i32, y: i32, z: i32) -> Option<u8> {
        let reg = &self.reg;
        if reg.is_water(self.get_block(x, y + 1, z)) {
            return Some(1);
        }
        let mut best: Option<u8> = None;
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let n = self.get_block(x + dx, y, z + dz);
            if let Some(l) = reg.water_level(n) {
                if reg.is_solid(self.get_block(x + dx, y - 1, z + dz)) {
                    best = Some(best.map_or(l, |b| b.min(l)));
                }
            }
        }
        match best {
            Some(l) if l < 7 => Some(l + 1),
            _ => None,
        }
    }

    pub fn tick_water(&mut self, budget: usize) -> bool {
        let mut changed = false;
        for _ in 0..budget {
            let Some(pos) = self.water_queue.pop_front() else { break };
            self.water_queued.remove(&pos);
            let (x, y, z) = pos;
            let b = self.get_block(x, y, z);
            let level = self.reg.water_level(b);

            match level {
                Some(0) => {
                    if self.reg.is_air(self.get_block(x, y - 1, z)) {
                        let f = self.reg.water_block(1);
                        self.set_block(x, y - 1, z, f);
                        changed = true;
                    } else {
                        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                            if self.reg.is_air(self.get_block(x + dx, y, z + dz))
                                && self.desired_flow(x + dx, y, z + dz).is_some()
                            {
                                self.schedule_water(x + dx, y, z + dz);
                            }
                        }
                    }
                }
                Some(l) => match self.desired_flow(x, y, z) {
                    Some(want) if want != l => {
                        let f = self.reg.water_block(want);
                        self.set_block(x, y, z, f);
                        changed = true;
                    }
                    None => {
                        self.set_block(x, y, z, AIR);
                        changed = true;
                    }
                    _ => {
                        if self.reg.is_air(self.get_block(x, y - 1, z)) {
                            let f = self.reg.water_block(1);
                            self.set_block(x, y - 1, z, f);
                            changed = true;
                        } else if l < 7 {
                            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                                if self.reg.is_air(self.get_block(x + dx, y, z + dz))
                                    && self.desired_flow(x + dx, y, z + dz).is_some()
                                {
                                    self.schedule_water(x + dx, y, z + dz);
                                }
                            }
                        }
                    }
                },
                None if self.reg.is_air(b) => {
                    if let Some(want) = self.desired_flow(x, y, z) {
                        let f = self.reg.water_block(want);
                        self.set_block(x, y, z, f);
                        changed = true;
                    }
                }
                None => {}
            }
        }
        changed
    }

    /// Y of the highest solid block in a column (for spawn placement).
    pub fn surface_height(&self, x: i32, z: i32) -> i32 {
        for y in (0..CHUNK_Y as i32).rev() {
            if self.reg.is_solid(self.get_block(x, y, z)) {
                return y;
            }
        }
        0
    }
}
