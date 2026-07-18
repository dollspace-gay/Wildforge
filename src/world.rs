//! World: chunk map with block access, fluid simulation, and versioned
//! persistence (save v2 with a per-world id palette; legacy v1 migrates).

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos, SEA_LEVEL};
use crate::mobs::Mob;
use crate::inventory::ItemStack;
use crate::registry::{AIR, BlockId, Registry};
use crate::worldgen::Generator;

/// Per-block persistent state for interactive machines.
pub enum BlockEntity {
    Furnace(FurnaceState),
}

#[derive(Default)]
pub struct FurnaceState {
    pub input: Option<ItemStack>,
    pub fuel: Option<ItemStack>,
    pub output: Option<ItemStack>,
    pub progress: f32,
    pub burn_left: f32,
    pub burn_total: f32,
}

/// (seed, mode) from world.toml, falling back to the legacy seed file.
pub fn read_world_meta(dir: &std::path::Path) -> (Option<u32>, String) {
    if let Ok(t) = fs::read_to_string(dir.join("world.toml")) {
        let mut seed = None;
        let mut mode = "survival".to_string();
        for l in t.lines() {
            if let Some(v) = l.strip_prefix("seed = ") {
                seed = v.trim().parse().ok();
            } else if let Some(v) = l.strip_prefix("mode = ") {
                mode = v.trim().trim_matches('"').to_string();
            }
        }
        (seed, mode)
    } else {
        let seed = fs::read_to_string(dir.join("seed")).ok().and_then(|s| s.trim().parse().ok());
        (seed, "survival".to_string())
    }
}

pub fn write_world_meta(dir: &std::path::Path, seed: u32, mode: &str) {
    let _ = fs::create_dir_all(dir);
    let _ = fs::write(dir.join("world.toml"), format!("seed = {seed}\nmode = \"{mode}\"\n"));
}

/// List worlds under `dir`: (name, seed), sorted. Reads world.toml with the
/// legacy `seed`-file fallback, same as read_world_meta.
pub fn list_worlds(dir: &std::path::Path) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || !e.path().is_dir() {
                continue;
            }
            if let (Some(seed), _) = read_world_meta(&e.path()) {
                out.push((name, seed));
            }
        }
    }
    out.sort();
    out
}

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
    pub block_entities: HashMap<(i32, i32, i32), BlockEntity>,
    /// Items spilled by removed block entities, for the game loop to spawn.
    pub pending_drops: Vec<((i32, i32, i32), ItemStack)>,
    pub mobs: Vec<crate::mobs::Mob>,
    /// Chunks whose wildlife roll already happened (persisted).
    mob_seeded: HashSet<(i32, i32)>,
    repop_timer: f32,
}

/// Hard cap on living mobs — memory/perf backstop, far above natural density.
pub const MOB_CAP: usize = 200;

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
            block_entities: HashMap::new(),
            pending_drops: Vec::new(),
            mobs: Vec::new(),
            mob_seeded: HashSet::new(),
            repop_timer: 0.0,
        }
    }

    /// Load a world from disk (reads seed + palette) or create a fresh one.
    pub fn load_or_create(save_dir: PathBuf, reg: Arc<Registry>) -> World {
        let (seed, mode) = read_world_meta(&save_dir);
        let seed = seed.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as u32)
                .unwrap_or(1337)
        });
        write_world_meta(&save_dir, seed, &mode);
        let mut w = World::new(seed, save_dir, reg);
        w.load_remap = w.read_palette_remap();
        w.load_entities();
        w.load_mobs();
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
        // Wildlife rolls once per chunk, ever (the mark persists with the
        // world so hunted animals stay gone across sessions).
        if self.mob_seeded.insert((pos.x, pos.z)) {
            self.seed_wildlife(pos);
        }
        true
    }

    // ---------------- wildlife ----------------

    fn mob_hash(&self, x: i32, z: i32, salt: u32) -> u32 {
        let mut h = (x as u32).wrapping_mul(0x85eb_ca6b)
            ^ (z as u32).wrapping_mul(0xc2b2_ae35)
            ^ self.seed.wrapping_mul(0x9e37_79b9)
            ^ salt.wrapping_mul(0x2708_92cd);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2c1b_3c6d);
        h ^= h >> 12;
        h
    }

    /// Deterministic per-chunk wildlife roll: at most one species' group.
    fn seed_wildlife(&mut self, pos: ChunkPos) {
        if self.mobs.len() >= MOB_CAP {
            return;
        }
        let reg = self.reg.clone();
        let (cx, cz) = (pos.x * CHUNK_X as i32, pos.z * CHUNK_Z as i32);
        let biome = self.generator.biome(cx + 8, cz + 8).name().to_lowercase();
        for (si, def) in reg.animals.iter().enumerate() {
            if !def.biomes.iter().any(|b| *b == biome) {
                continue;
            }
            let roll = self.mob_hash(pos.x, pos.z, 7000 + si as u32);
            if roll % def.rarity != 0 {
                continue;
            }
            let span = def.group[1].saturating_sub(def.group[0]) + 1;
            let n = def.group[0] + (roll >> 8) % span;
            for i in 0..n {
                let h = self.mob_hash(pos.x, pos.z, 7100 + si as u32 * 31 + i);
                let lx = (h % CHUNK_X as u32) as i32;
                let lz = ((h >> 8) % CHUNK_Z as u32) as i32;
                self.try_spawn(si, cx + lx, cz + lz, (h >> 16) as f32 / 65535.0);
            }
            break; // one species per chunk keeps groups readable
        }
    }

    /// Spawn on dry solid ground at the surface; silently skips bad spots.
    fn try_spawn(&mut self, species: usize, x: i32, z: i32, yaw01: f32) -> bool {
        if self.mobs.len() >= MOB_CAP {
            return false;
        }
        let y = self.surface_height(x, z);
        if y <= SEA_LEVEL {
            return false;
        }
        let ground = self.get_block(x, y, z);
        if !self.reg.is_solid(ground) {
            return false;
        }
        let mut m = Mob::new(
            species,
            glam::Vec3::new(x as f32 + 0.5, y as f32 + 1.05, z as f32 + 0.5),
            yaw01 * std::f32::consts::TAU,
        );
        m.health = self.reg.animals[species].health;
        self.mobs.push(m);
        true
    }

    /// Tick AI/physics for all mobs, plus the slow repopulation roll.
    pub fn tick_mobs(&mut self, player: glam::Vec3, dt: f32, rng: &mut u32) {
        let reg = self.reg.clone();
        let mut mobs = std::mem::take(&mut self.mobs);
        for m in &mut mobs {
            if let Some(def) = reg.animals.get(m.species) {
                m.tick(self, def, player, dt, rng);
            }
        }
        self.mobs = mobs;

        // Repopulation: overhunted wildlife slowly recovers, away from the
        // player and only under the local cap.
        self.repop_timer += dt;
        if self.repop_timer < 8.0 {
            return;
        }
        self.repop_timer = 0.0;
        let near = self
            .mobs
            .iter()
            .filter(|m| (m.pos - player).length_squared() < 96.0 * 96.0)
            .count();
        if near >= 40 || reg.animals.is_empty() {
            return;
        }
        *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
        let r = *rng;
        // A ring 32-72 blocks out at a random angle.
        let ang = (r % 1024) as f32 / 1024.0 * std::f32::consts::TAU;
        let dist = 32.0 + ((r >> 10) % 40) as f32;
        let x = (player.x + ang.sin() * dist).floor() as i32;
        let z = (player.z + ang.cos() * dist).floor() as i32;
        let cp = ChunkPos::of_world(x, z);
        if !self.chunks.contains_key(&cp) {
            return;
        }
        let biome = self.generator.biome(x, z).name().to_lowercase();
        let eligible: Vec<usize> = reg
            .animals
            .iter()
            .enumerate()
            .filter(|(_, d)| d.biomes.iter().any(|b| *b == biome))
            .map(|(i, _)| i)
            .collect();
        if let Some(&si) = eligible.get(((r >> 20) as usize) % eligible.len().max(1)) {
            self.try_spawn(si, x, z, (r >> 8) as f32 / (u32::MAX >> 8) as f32);
        }
    }

    fn mobs_path(&self) -> PathBuf {
        self.save_dir.join("animals.toml")
    }

    fn save_mobs(&self) {
        use std::fmt::Write as _;
        let mut out = String::new();
        for m in &self.mobs {
            let Some(def) = self.reg.animals.get(m.species) else { continue };
            let _ = writeln!(
                out,
                "[[mob]]\nspecies = \"{}\"\npos = [{}, {}, {}]\nyaw = {}\nhealth = {}\n",
                def.name, m.pos.x, m.pos.y, m.pos.z, m.yaw, m.health
            );
        }
        if out.is_empty() {
            let _ = fs::remove_file(self.mobs_path());
        } else {
            let _ = fs::write(self.mobs_path(), out);
        }
        // Seeded-chunk marks: compact binary pairs.
        let mut buf = Vec::with_capacity(self.mob_seeded.len() * 8);
        for (x, z) in &self.mob_seeded {
            buf.extend_from_slice(&x.to_le_bytes());
            buf.extend_from_slice(&z.to_le_bytes());
        }
        let _ = fs::write(self.save_dir.join("aseeded"), buf);
    }

    fn load_mobs(&mut self) {
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct MobT {
            species: String,
            pos: [f32; 3],
            yaw: f32,
            health: f32,
        }
        #[derive(Deserialize)]
        struct FileT {
            #[serde(default)]
            mob: Vec<MobT>,
        }
        if let Ok(text) = fs::read_to_string(self.mobs_path()) {
            if let Ok(f) = toml::from_str::<FileT>(&text) {
                for t in f.mob {
                    // Unknown species (mod removed) skip cleanly.
                    let Some(si) = self.reg.animal_id(&t.species) else { continue };
                    let mut m =
                        Mob::new(si, glam::Vec3::new(t.pos[0], t.pos[1], t.pos[2]), t.yaw);
                    m.health = t.health.min(self.reg.animals[si].health);
                    self.mobs.push(m);
                }
            }
        }
        if let Ok(data) = fs::read(self.save_dir.join("aseeded")) {
            for p in data.chunks_exact(8) {
                let x = i32::from_le_bytes([p[0], p[1], p[2], p[3]]);
                let z = i32::from_le_bytes([p[4], p[5], p[6], p[7]]);
                self.mob_seeded.insert((x, z));
            }
        }
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
        self.save_entities();
        self.save_mobs();
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
        // A changed block invalidates any machine state living there.
        if let Some(e) = self.block_entities.remove(&(x, y, z)) {
            let BlockEntity::Furnace(f) = e;
            for s in [f.input, f.fuel, f.output].into_iter().flatten() {
                self.pending_drops.push(((x, y, z), s));
            }
        }
    }

    /// Random ticks: crops advance a stage when conditions hold.
    pub fn random_tick(&mut self, rng: &mut u32) {
        let reg = self.reg.clone();
        let farmland = reg.block_id("base:farmland");
        let keys: Vec<ChunkPos> = self.chunks.keys().copied().collect();
        let mut changes = Vec::new();
        for pos in keys {
            for _ in 0..8 {
                *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                let r = *rng >> 8;
                let (lx, lz) = ((r % 16) as i32, ((r >> 4) % 16) as i32);
                let y = ((r >> 8) % CHUNK_Y as u32) as i32;
                let (wx, wz) = (pos.x * 16 + lx, pos.z * 16 + lz);
                let b = self.get_block(wx, y, wz);
                let d = reg.block(b);
                if let Some(next) = d.crop_next {
                    let soil_ok = d.crop_any_soil
                        || farmland == Some(self.get_block(wx, y - 1, wz));
                    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    if soil_ok && ((*rng >> 8) as f32 / (1 << 24) as f32) < d.crop_chance {
                        changes.push((wx, y, wz, next));
                    }
                }
            }
        }
        for (x, y, z, b) in changes {
            self.set_block(x, y, z, b);
        }
    }

    /// Advance machines. Returns true if any visible state changed.
    pub fn tick_entities(&mut self, dt: f32) {
        let reg = self.reg.clone();
        for e in self.block_entities.values_mut() {
            let BlockEntity::Furnace(f) = e;
            let smelt = f.input.and_then(|s| reg.smelt_for(s.item)).cloned();
            let output_ok = |f: &FurnaceState, out: crate::registry::ItemId| match f.output {
                None => true,
                Some(o) => o.item == out && o.count < reg.item(out).max_stack,
            };
            let can_smelt = smelt.as_ref().is_some_and(|s| output_ok(f, s.output));

            if f.burn_left <= 0.0 && can_smelt {
                // Light more fuel.
                if let Some(fs) = f.fuel {
                    if let Some(burn) = reg.fuel_value(fs.item) {
                        f.burn_left = burn;
                        f.burn_total = burn;
                        let left = fs.count - 1;
                        f.fuel = if left > 0 { Some(ItemStack { count: left, ..fs }) } else { None };
                    }
                }
            }
            if f.burn_left > 0.0 {
                f.burn_left = (f.burn_left - dt).max(0.0);
                if can_smelt {
                    let s = smelt.as_ref().unwrap();
                    f.progress += dt;
                    if f.progress >= s.time {
                        f.progress = 0.0;
                        // Consume one input, emit output.
                        if let Some(inp) = f.input {
                            let left = inp.count - 1;
                            f.input =
                                if left > 0 { Some(ItemStack { count: left, ..inp }) } else { None };
                        }
                        f.output = Some(match f.output {
                            Some(o) => ItemStack { count: o.count + 1, ..o },
                            None => ItemStack::new(&reg, s.output, 1),
                        });
                    }
                } else {
                    f.progress = 0.0;
                }
            } else if f.progress > 0.0 {
                f.progress = (f.progress - dt * 2.0).max(0.0);
            }
        }
    }

    // ---- block entity persistence (by item name, mod-change safe) ----

    fn entities_path(&self) -> PathBuf {
        self.save_dir.join("entities.toml")
    }

    fn save_entities(&self) {
        use std::fmt::Write as _;
        let mut out = String::new();
        for ((x, y, z), e) in &self.block_entities {
            let BlockEntity::Furnace(f) = e;
            let _ = writeln!(out, "[[furnace]]\npos = [{x}, {y}, {z}]");
            let mut slot = |k: &str, s: &Option<ItemStack>| {
                if let Some(s) = s {
                    let _ = writeln!(
                        out,
                        "{k} = {{ item = \"{}\", count = {}, durability = {} }}",
                        self.reg.item(s.item).name, s.count, s.durability
                    );
                }
            };
            slot("input", &f.input);
            slot("fuel", &f.fuel);
            slot("output", &f.output);
            let _ = writeln!(out, "progress = {}\nburn_left = {}\nburn_total = {}\n",
                f.progress, f.burn_left, f.burn_total);
        }
        if out.is_empty() {
            let _ = fs::remove_file(self.entities_path());
        } else {
            let _ = fs::write(self.entities_path(), out);
        }
    }

    fn load_entities(&mut self) {
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct SlotT {
            item: String,
            count: u32,
            durability: u32,
        }
        #[derive(Deserialize)]
        struct FurnaceT {
            pos: [i32; 3],
            input: Option<SlotT>,
            fuel: Option<SlotT>,
            output: Option<SlotT>,
            #[serde(default)]
            progress: f32,
            #[serde(default)]
            burn_left: f32,
            #[serde(default)]
            burn_total: f32,
        }
        #[derive(Deserialize)]
        struct FileT {
            #[serde(default)]
            furnace: Vec<FurnaceT>,
        }
        let Ok(text) = fs::read_to_string(self.entities_path()) else { return };
        let Ok(parsed) = toml::from_str::<FileT>(&text) else { return };
        let conv = |s: Option<SlotT>| -> Option<ItemStack> {
            let s = s?;
            let item = self.reg.item_id(&s.item)?;
            Some(ItemStack { item, count: s.count, durability: s.durability })
        };
        for fu in parsed.furnace {
            self.block_entities.insert(
                (fu.pos[0], fu.pos[1], fu.pos[2]),
                BlockEntity::Furnace(FurnaceState {
                    input: conv(fu.input),
                    fuel: conv(fu.fuel),
                    output: conv(fu.output),
                    progress: fu.progress,
                    burn_left: fu.burn_left,
                    burn_total: fu.burn_total,
                }),
            );
        }
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
