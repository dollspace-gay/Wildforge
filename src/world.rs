//! World: chunk map with block access, fluid simulation, and versioned
//! persistence (save v2 with a per-world id palette; legacy v1 migrates).

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos, SEA_LEVEL};
use crate::mobs::{Mob, MobEvent, ProjHit, Projectile};
use crate::inventory::ItemStack;
use crate::registry::{AIR, BlockId, Registry};
use crate::worldgen::Generator;

/// Per-block persistent state for interactive machines.
pub enum BlockEntity {
    Furnace(FurnaceState),
    Chest(ChestState),
    Offering(OfferingState),
}

#[derive(Default)]
pub struct OfferingState {
    pub slots: [Option<ItemStack>; 3],
}

pub const CHEST_SLOTS: usize = 27;

pub struct ChestState {
    pub slots: [Option<ItemStack>; CHEST_SLOTS],
    /// A ruin's chest: first opening costs 1 ire (the wild keeps its
    /// trophies).
    pub wild_owned: bool,
}

impl Default for ChestState {
    fn default() -> ChestState {
        ChestState { slots: [None; CHEST_SLOTS], wild_owned: false }
    }
}

#[derive(Default)]
pub struct FurnaceState {
    pub input: Option<ItemStack>,
    pub fuel: Option<ItemStack>,
    pub output: Option<ItemStack>,
    pub progress: f32,
    pub burn_left: f32,
    pub burn_total: f32,
    /// Smelt-speed multiplier of the currently burning fuel (embers 2x).
    pub burn_speed: f32,
}

/// (seed, mode, ire) from world.toml, falling back to the legacy seed file.
pub fn read_world_meta(dir: &std::path::Path) -> (Option<u32>, String, f32) {
    if let Ok(t) = fs::read_to_string(dir.join("world.toml")) {
        let mut seed = None;
        let mut mode = "survival".to_string();
        let mut ire = 0.0f32;
        for l in t.lines() {
            if let Some(v) = l.strip_prefix("seed = ") {
                seed = v.trim().parse().ok();
            } else if let Some(v) = l.strip_prefix("mode = ") {
                mode = v.trim().trim_matches('"').to_string();
            } else if let Some(v) = l.strip_prefix("ire = ") {
                ire = v.trim().parse().unwrap_or(0.0);
            }
        }
        (seed, mode, ire.clamp(0.0, 100.0))
    } else {
        let seed = fs::read_to_string(dir.join("seed")).ok().and_then(|s| s.trim().parse().ok());
        (seed, "survival".to_string(), 0.0)
    }
}

pub fn write_world_meta(dir: &std::path::Path, seed: u32, mode: &str, ire: f32) {
    let _ = fs::create_dir_all(dir);
    let _ = fs::write(
        dir.join("world.toml"),
        format!("seed = {seed}\nmode = \"{mode}\"\nire = {ire:.2}\n"),
    );
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
            if let (Some(seed), _, _) = read_world_meta(&e.path()) {
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
    pub projectiles: Vec<Projectile>,
    hostile_spawn_timer: f32,
    /// Chunks whose wildlife roll already happened (persisted).
    mob_seeded: HashSet<(i32, i32)>,
    repop_timer: f32,
    /// Game mode string, persisted in world.toml alongside seed/ire.
    pub mode: String,
    /// The wild's ire 0..100 — reciprocity meter driving hostile spawns.
    pub ire: f32,
    /// How much ire planting has already refunded today (daily cap).
    plant_ire_today: f32,
    /// Fraction of the current day elapsed (for decay + cap reset).
    day_progress: f32,
    /// Guest mode: chunks come only from the network, never generated.
    pub remote: bool,
    /// Host mode: record block edits for broadcasting.
    pub log_edits: bool,
    pub edit_log: Vec<(i32, i32, i32, BlockId)>,
    /// (projectile owner, item) — guests' recovered arrows, etc.
    pub pending_gives: Vec<(u32, crate::registry::ItemId)>,
}

/// Ire tier names, index = tier.
pub const IRE_TIERS: [&str; 4] = ["CALM", "UNEASY", "PROVOKED", "WRATHFUL"];

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
            projectiles: Vec::new(),
            hostile_spawn_timer: 0.0,
            mob_seeded: HashSet::new(),
            repop_timer: 0.0,
            mode: "survival".into(),
            ire: 0.0,
            plant_ire_today: 0.0,
            day_progress: 0.0,
            remote: false,
            log_edits: false,
            edit_log: Vec::new(),
            pending_gives: Vec::new(),
        }
    }

    // ---------------- ire (reciprocity) ----------------

    pub fn ire_tier(&self) -> usize {
        match self.ire {
            x if x < 20.0 => 0,
            x if x < 50.0 => 1,
            x if x < 80.0 => 2,
            _ => 3,
        }
    }

    pub fn add_ire(&mut self, amt: f32) {
        self.ire = (self.ire + amt).clamp(0.0, 100.0);
    }

    /// Planting refunds ire, capped per day — mending stays slower than
    /// taking; a clearcut can't be laundered with a seed drawer.
    pub fn plant_ire(&mut self, amt: f32) {
        let room = (8.0 - self.plant_ire_today).max(0.0);
        let refund = amt.min(room);
        if refund > 0.0 {
            self.plant_ire_today += refund;
            self.add_ire(-refund);
        }
    }

    /// Advance ire time by a fraction of a day: passive decay (-4/day)
    /// and the daily reset of the planting cap. Returns true at dawn
    /// (day rollover) — the moment offerings are accepted.
    pub fn tick_ire(&mut self, day_frac: f32) -> bool {
        self.add_ire(-4.0 * day_frac);
        self.day_progress += day_frac;
        if self.day_progress >= 1.0 {
            self.day_progress -= 1.0;
            self.plant_ire_today = 0.0;
            return true;
        }
        false
    }

    /// What the wild values: its own materials most, then life given.
    pub fn offering_value(&self, s: &ItemStack) -> f32 {
        let d = self.reg.item(s.item);
        let per = if ["base:heartwood", "base:living_wood", "base:ember", "base:frost_shard"]
            .contains(&d.name.as_str())
        {
            2.0
        } else if d.name.ends_with("_sapling") {
            1.0
        } else if d.name.contains("raw_") || d.name.contains("cooked_") {
            1.0
        } else if let Some(f) = &d.food {
            f.hunger * 0.25
        } else {
            0.25
        };
        per * s.count as f32
    }

    /// Dawn: the wild takes everything left on offering stones. Items are
    /// consumed regardless; the refund is capped at 10 per dawn.
    pub fn accept_offerings(&mut self) -> f32 {
        let mut taken: Vec<ItemStack> = Vec::new();
        for e in self.block_entities.values_mut() {
            let BlockEntity::Offering(o) = e else { continue };
            for slot in o.slots.iter_mut() {
                if let Some(s) = slot.take() {
                    taken.push(s);
                }
            }
        }
        if taken.is_empty() {
            return 0.0;
        }
        let value: f32 = taken.iter().map(|s| self.offering_value(s)).sum();
        let refund = value.min(10.0);
        self.add_ire(-refund);
        refund
    }

    /// Grow a planted sapling into a full tree, mirroring the worldgen
    /// shapes. Returns false (sapling stays) if the trunk is blocked.
    pub fn grow_tree(&mut self, x: i32, y: i32, z: i32, species: &str, rnd: u32) -> bool {
        let reg = self.reg.clone();
        let ids = |l: &str, f: &str| Some((reg.block_id(l)?, reg.block_id(f)?));
        let Some((log, leaf)) = (match species {
            "birch" => ids("base:birch_log", "base:birch_leaves"),
            "spruce" => ids("base:spruce_log", "base:spruce_leaves"),
            "jungle" => ids("base:jungle_log", "base:jungle_leaves"),
            "acacia" => ids("base:acacia_log", "base:acacia_leaves"),
            _ => ids("base:log", "base:leaves"),
        }) else {
            return false;
        };
        let trunk_h = match species {
            "acacia" => 1,
            "spruce" => 5 + (rnd % 3) as i32,
            "jungle" => 6 + (rnd % 3) as i32,
            _ => 4 + (rnd % 3) as i32,
        };
        // Clearance: the trunk column (above the sapling cell) must be open.
        for dy in 1..=trunk_h + 1 {
            if self.get_block(x, y + dy, z) != AIR {
                return false;
            }
        }
        let mut leaf_at = |w: &mut World, lx: i32, ly: i32, lz: i32| {
            if ly > 0 && ly < CHUNK_Y as i32 && w.get_block(lx, ly, lz) == AIR {
                w.set_block(lx, ly, lz, leaf);
            }
        };
        for dy in 0..trunk_h {
            self.set_block(x, y + dy, z, log);
        }
        let top = y + trunk_h;
        match species {
            "acacia" => {
                for dx in -1..=1 {
                    for dz in -1..=1 {
                        leaf_at(self, x + dx, top, z + dz);
                    }
                }
            }
            "spruce" => {
                for (dy, r) in [(-3i32, 2i32), (-2, 1), (-1, 2), (0, 1), (1, 1)] {
                    for dx in -r..=r {
                        for dz in -r..=r {
                            if dx.abs() == r && dz.abs() == r && r > 1 {
                                continue;
                            }
                            if dx == 0 && dz == 0 && dy < 0 {
                                continue;
                            }
                            leaf_at(self, x + dx, top + dy, z + dz);
                        }
                    }
                }
                leaf_at(self, x, top + 2, z);
            }
            _ => {
                let big: i32 = if species == "jungle" { 3 } else { 2 };
                for (dy, r) in [(-2i32, big), (-1, big), (0, 1), (1, 1)] {
                    for dx in -r..=r {
                        for dz in -r..=r {
                            if dx == 0 && dz == 0 && dy < 0 {
                                continue;
                            }
                            leaf_at(self, x + dx, top + dy, z + dz);
                        }
                    }
                }
            }
        }
        true
    }

    /// Ire cost of breaking a block, by what it is.
    pub fn ire_for_block(&self, b: BlockId) -> f32 {
        let name = &self.reg.block(b).name;
        if name.ends_with("_log") || name.ends_with(":log") {
            0.3
        } else if name.contains("ore") {
            0.4
        } else if name.ends_with("stone") && !name.contains("cobble") {
            0.05
        } else if name.contains("leaves") || name.ends_with("dirt") || name.ends_with("grass") {
            0.02
        } else {
            0.0
        }
    }

    /// Load a world from disk (reads seed + palette) or create a fresh one.
    pub fn load_or_create(save_dir: PathBuf, reg: Arc<Registry>) -> World {
        let (seed, mode, ire) = read_world_meta(&save_dir);
        let seed = seed.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as u32)
                .unwrap_or(1337)
        });
        write_world_meta(&save_dir, seed, &mode, ire);
        let mut w = World::new(seed, save_dir, reg);
        w.mode = mode;
        w.ire = ire;
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
        if self.remote {
            return false; // guests receive chunks, they don't make them
        }
        let loaded = self.try_load_chunk(pos);
        let fresh = loaded.is_none();
        let chunk = loaded.unwrap_or_else(|| self.generator.generate(pos, &self.reg));
        self.chunks.insert(pos, chunk);
        // Ruins place once, at first generation; placement marks the chunk
        // modified so it saves and never regenerates.
        if fresh {
            self.seed_structures(pos);
        }
        // Wildlife rolls once per chunk, ever (the mark persists with the
        // world so hunted animals stay gone across sessions).
        if self.mob_seeded.insert((pos.x, pos.z)) {
            self.seed_wildlife(pos);
        }
        self.relight_and_cascade(pos);
        true
    }

    // ---------------- ruins ----------------

    /// Deterministic per-chunk structure roll (at most one per chunk).
    fn seed_structures(&mut self, pos: ChunkPos) {
        let reg = self.reg.clone();
        let (cx, cz) = (pos.x * CHUNK_X as i32, pos.z * CHUNK_Z as i32);
        let biome = self.generator.biome(cx + 8, cz + 8).name().to_lowercase();
        for (si, st) in reg.structures.iter().enumerate() {
            if !st.biomes.iter().any(|b| *b == biome) {
                continue;
            }
            let h = self.mob_hash(pos.x, pos.z, 9000 + si as u32);
            if h % st.rarity != 0 {
                continue;
            }
            let w = st.layers[0].first().map(|r| r.len()).unwrap_or(0) as i32;
            let d = st.layers[0].len() as i32;
            if w == 0 || w > 14 || d > 14 {
                continue;
            }
            let ox = cx + 1 + ((h >> 8) as i32).rem_euclid((15 - w).max(1));
            let oz = cz + 1 + ((h >> 16) as i32).rem_euclid((15 - d).max(1));
            let surface = self.surface_height(ox + w / 2, oz + d / 2);
            if surface <= SEA_LEVEL + 1 || surface >= CHUNK_Y as i32 - 24 {
                continue;
            }
            let y0 = match st.buried {
                None => surface,
                Some((min, max)) => {
                    let depth = min + (h >> 4).rem_euclid((max - min + 1) as u32) as i32;
                    (surface - depth).max(6)
                }
            };
            self.place_structure(si, ox, y0, oz, h);
            break;
        }
    }

    /// Stamp a structure template into the world (clipped writes via
    /// set_block; chests get rolled loot and belong to the wild).
    pub fn place_structure(&mut self, si: usize, x0: i32, y0: i32, z0: i32, seed: u32) {
        let reg = self.reg.clone();
        let Some(st) = reg.structures.get(si).cloned() else { return };
        let chest_block = reg.block_id("base:chest");
        let mut rng = seed ^ 0x5f37_59df;
        for (ly, layer) in st.layers.iter().enumerate() {
            for (lz, row) in layer.iter().enumerate() {
                for (lx, ch) in row.chars().enumerate() {
                    let (x, y, z) = (x0 + lx as i32, y0 + ly as i32, z0 + lz as i32);
                    match ch {
                        '.' => {}
                        '~' => self.set_block(x, y, z, AIR),
                        'C' => {
                            if let Some(cb) = chest_block {
                                self.set_block(x, y, z, cb);
                                let mut state = ChestState { wild_owned: true, ..Default::default() };
                                if let Some(table) = &st.loot {
                                    let n = 3 + (rng % 3) as usize;
                                    for (i, stck) in
                                        self.roll_loot(table, n as u32, &mut rng).into_iter().enumerate()
                                    {
                                        if i < CHEST_SLOTS {
                                            // Scatter through the chest.
                                            let slot = (i * 7 + (rng % 5) as usize) % CHEST_SLOTS;
                                            state.slots[slot] = Some(stck);
                                        }
                                    }
                                }
                                self.block_entities.insert((x, y, z), BlockEntity::Chest(state));
                            }
                        }
                        c => {
                            if let Some(b) = st.palette.get(&c) {
                                self.set_block(x, y, z, *b);
                            }
                        }
                    }
                }
            }
        }
        // Buried ruins leave a hint on the surface: a chimney stub.
        if st.buried.is_some() {
            if let Some(cob) = reg.block_id("base:cobblestone") {
                let hx = x0 + 1;
                let hz = z0 + 1;
                let sy = self.surface_height(hx, hz);
                self.set_block(hx, sy + 1, hz, cob);
                self.set_block(hx, sy + 2, hz, cob);
            }
        }
    }

    /// Weighted rolls from a loot table.
    pub fn roll_loot(&self, table: &str, rolls: u32, rng: &mut u32) -> Vec<ItemStack> {
        let Some(entries) = self.reg.loots.get(table) else { return Vec::new() };
        let total: u32 = entries.iter().map(|e| e.weight).sum();
        if total == 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        for _ in 0..rolls {
            *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            let mut pick = (*rng >> 8) % total;
            for e in entries {
                if pick < e.weight {
                    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    let span = e.count.1.saturating_sub(e.count.0) + 1;
                    let n = e.count.0 + (*rng >> 8) % span;
                    let mut stack = ItemStack::new(&self.reg, e.item, n.max(1));
                    if let Some(frac) = e.durability_frac {
                        let max = self.reg.item(e.item).durability;
                        if max > 0 {
                            stack.durability = ((max as f32 * frac) as u32).max(1);
                        }
                    }
                    out.push(stack);
                    break;
                }
                pick -= e.weight;
            }
        }
        out
    }

    /// Archaeology: sweep a remnant block — it yields its artifact once
    /// and becomes plain. Returns what was found.
    pub fn brush_block(&mut self, x: i32, y: i32, z: i32, rng: &mut u32) -> Option<ItemStack> {
        let b = self.get_block(x, y, z);
        let (table, becomes) = self.reg.block(b).brush.clone()?;
        let mut items = self.roll_loot(&table, 1, rng);
        self.set_block(x, y, z, becomes);
        items.pop()
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
            // Wildlife only — wardens come and go with the spawner.
            if def.hostile || !def.biomes.iter().any(|b| *b == biome) {
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
    /// Returns events (player hits, projectile casts) for the game loop.
    pub fn tick_mobs(
        &mut self,
        players: &[crate::server::PlayerCtx],
        daylight: f32,
        dt: f32,
        rng: &mut u32,
    ) -> Vec<MobEvent> {
        let player = players.first().map(|p| p.pos).unwrap_or(glam::Vec3::ZERO);
        let reg = self.reg.clone();
        let mut events = Vec::new();
        let mut mobs = std::mem::take(&mut self.mobs);
        for m in &mut mobs {
            // Frozen until its chunk streams in: an unloaded chunk reads as
            // air, and ticking against it drops the mob through the world.
            let cp = ChunkPos::of_world(m.pos.x.floor() as i32, m.pos.z.floor() as i32);
            if !self.chunks.contains_key(&cp) {
                continue;
            }
            if let Some(def) = reg.animals.get(m.species) {
                m.unstick(self, def);
                m.tick(self, def, players, dt, rng, &mut events);
            }
        }
        // Wardens are expressions of the wild, not creatures: they dissolve
        // in daylight (sky-lit cells only — torchlight never banishes them)
        // and when the player leaves them far behind.
        mobs.retain(|m| {
            let Some(def) = reg.animals.get(m.species) else { return false };
            if !def.hostile {
                return true;
            }
            let near = players
                .iter()
                .map(|p| (m.pos - p.pos).length_squared())
                .fold(f32::INFINITY, f32::min);
            if near > 80.0 * 80.0 {
                return false;
            }
            let (_, sl) = self.light_at(
                m.pos.x.floor() as i32,
                (m.pos.y + 0.5).floor() as i32,
                m.pos.z.floor() as i32,
            );
            sl as f32 * daylight < 7.0
        });
        // Husbandry: two fed adults of a species near each other bear young.
        let mut births: Vec<(usize, usize)> = Vec::new();
        for i in 0..mobs.len() {
            if births.iter().any(|&(a, b)| a == i || b == i) {
                continue;
            }
            if !mobs[i].fed || mobs[i].growth < 1.0 {
                continue;
            }
            for j in (i + 1)..mobs.len() {
                if mobs[j].species == mobs[i].species
                    && mobs[j].fed
                    && mobs[j].growth >= 1.0
                    && (mobs[i].pos - mobs[j].pos).length_squared() < 16.0
                {
                    births.push((i, j));
                    break;
                }
            }
        }
        for (i, j) in births {
            let mid = (mobs[i].pos + mobs[j].pos) * 0.5;
            mobs[i].fed = false;
            mobs[j].fed = false;
            mobs[i].breed_cd = 300.0;
            mobs[j].breed_cd = 300.0;
            if mobs.len() < MOB_CAP {
                let mut baby = Mob::new(mobs[i].species, mid, 0.0);
                baby.health = reg.animals[mobs[i].species].health;
                baby.growth = 0.05;
                mobs.push(baby);
                // Life returned to the world.
                self.ire = (self.ire - 1.0).max(0.0);
                events.push(MobEvent::Bred(mid));
            }
        }
        self.mobs = mobs;

        // Repopulation: overhunted wildlife slowly recovers, away from the
        // player and only under the local cap.
        self.repop_timer += dt;
        if self.repop_timer >= 8.0 {
            self.repop_timer = 0.0;
            let near = self
                .mobs
                .iter()
                .filter(|m| (m.pos - player).length_squared() < 96.0 * 96.0)
                .count();
            if near < 40 && !reg.animals.is_empty() {
                *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                let r = *rng;
                // A ring 32-72 blocks out at a random angle.
                let ang = (r % 1024) as f32 / 1024.0 * std::f32::consts::TAU;
                let dist = 32.0 + ((r >> 10) % 40) as f32;
                let x = (player.x + ang.sin() * dist).floor() as i32;
                let z = (player.z + ang.cos() * dist).floor() as i32;
                let cp = ChunkPos::of_world(x, z);
                if self.chunks.contains_key(&cp) {
                    let biome = self.generator.biome(x, z).name().to_lowercase();
                    // Wildlife only — wardens have their own spawner.
                    let eligible: Vec<usize> = reg
                        .animals
                        .iter()
                        .enumerate()
                        .filter(|(_, d)| !d.hostile && d.biomes.iter().any(|b| *b == biome))
                        .map(|(i, _)| i)
                        .collect();
                    if let Some(&si) = eligible.get(((r >> 20) as usize) % eligible.len().max(1))
                    {
                        self.try_spawn(si, x, z, (r >> 8) as f32 / (u32::MAX >> 8) as f32);
                    }
                }
            }
        }
        events
    }

    /// Advance all bolts and arrows; returns (player index, damage) hits.
    /// Player arrows strike mobs through the normal hurt path and stick
    /// into blocks as recoverable item drops.
    pub fn tick_projectiles(
        &mut self,
        players: &[crate::server::PlayerCtx],
        dt: f32,
    ) -> Vec<(usize, f32)> {
        let mut dmg: Vec<(usize, f32)> = Vec::new();
        let mut mob_hits: Vec<(usize, f32, glam::Vec3)> = Vec::new();
        let mut drops: Vec<((i32, i32, i32), crate::registry::ItemId)> = Vec::new();
        let mut projectiles = std::mem::take(&mut self.projectiles);
        projectiles.retain_mut(|p| match p.tick(self, players, dt) {
            ProjHit::None => true,
            ProjHit::Expired => false,
            ProjHit::Player(i) => {
                dmg.push((i, p.damage));
                false
            }
            ProjHit::Mob(i) => {
                mob_hits.push((i, p.damage, p.pos - p.vel * dt));
                false
            }
            ProjHit::Block => {
                if let Some(it) = p.drop_item {
                    if p.owner != 0 {
                        // A guest's arrow: hand it back over the wire.
                        self.pending_gives.push((p.owner, it));
                    } else {
                        let back = p.pos - p.vel * dt * 2.0;
                        drops.push((
                            (back.x.floor() as i32, back.y.floor() as i32, back.z.floor() as i32),
                            it,
                        ));
                    }
                }
                false
            }
        });
        self.projectiles = projectiles;
        let reg = self.reg.clone();
        for (i, d, from) in mob_hits {
            if let Some(m) = self.mobs.get_mut(i) {
                if let Some(def) = reg.animals.get(m.species) {
                    m.hurt(def, d, from);
                }
            }
        }
        for (pos, it) in drops {
            self.pending_drops.push((pos, ItemStack::new(&reg, it, 1)));
        }
        dmg
    }

    /// Ire-driven warden spawner: territorial lurkers roll into the dark
    /// ring around the player. Never near the world spawn, never in light.
    pub fn tick_hostile_spawns(
        &mut self,
        player: glam::Vec3,
        world_spawn: glam::Vec3,
        daylight: f32,
        dt: f32,
        rng: &mut u32,
    ) {
        self.hostile_spawn_timer += dt;
        if self.hostile_spawn_timer < 4.0 {
            return;
        }
        self.hostile_spawn_timer = 0.0;
        let reg = self.reg.clone();
        let tier = self.ire_tier();
        let budget = [2usize, 6, 10, 14][tier];
        let near_hostiles = self
            .mobs
            .iter()
            .filter(|m| {
                reg.animals.get(m.species).is_some_and(|d| d.hostile)
                    && (m.pos - player).length_squared() < 96.0 * 96.0
            })
            .count();
        if near_hostiles >= budget || self.mobs.len() >= MOB_CAP {
            return;
        }
        let mut roll = |rng: &mut u32| {
            *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            *rng >> 8
        };
        for _ in 0..6 {
            let r = roll(rng);
            let ang = (r % 1024) as f32 / 1024.0 * std::f32::consts::TAU;
            let dist = 24.0 + ((r >> 10) % 32) as f32;
            let x = (player.x + ang.sin() * dist).floor() as i32;
            let z = (player.z + ang.cos() * dist).floor() as i32;
            if !self.chunks.contains_key(&ChunkPos::of_world(x, z)) {
                continue;
            }
            let dxs = x as f32 - world_spawn.x;
            let dzs = z as f32 - world_spawn.z;
            if dxs * dxs + dzs * dzs < 16.0 * 16.0 {
                continue;
            }
            let biome = self.generator.biome(x, z).name().to_lowercase();
            // Split the roster: surface wardens spawn at the surface, the
            // deep's own ("underground" biome tag) in caves below.
            let surface_y = self.surface_height(x, z);
            let candidates: Vec<(usize, i32)> = reg
                .animals
                .iter()
                .enumerate()
                .filter(|(_, d)| d.hostile && self.ire >= d.ire_min)
                .filter_map(|(i, d)| {
                    if d.biomes.iter().any(|b| b == "underground") {
                        // A random depth with a 2-tall air pocket.
                        let y = 6 + (roll(rng) % (surface_y.max(12) as u32 - 6)) as i32;
                        let ground = self.get_block(x, y - 1, z);
                        let a1 = self.get_block(x, y, z);
                        let a2 = self.get_block(x, y + 1, z);
                        (self.reg.is_solid(ground) && a1 == AIR && a2 == AIR)
                            .then_some((i, y))
                    } else if d.biomes.iter().any(|b| *b == biome) && surface_y > SEA_LEVEL {
                        Some((i, surface_y + 1))
                    } else {
                        None
                    }
                })
                .collect();
            if candidates.is_empty() {
                continue;
            }
            let (si, y) = candidates[(roll(rng) as usize) % candidates.len()];
            let def = &reg.animals[si];
            // Only one wrathwood walks at a time.
            if def.name.ends_with("wrathwood")
                && self.mobs.iter().any(|m| {
                    reg.animals.get(m.species).is_some_and(|d| d.name.ends_with("wrathwood"))
                })
            {
                continue;
            }
            let (bl, sl) = self.light_at(x, y, z);
            let eff = (bl as f32).max(sl as f32 * daylight);
            if eff >= def.spawn_light_max as f32 {
                continue;
            }
            let mut m = Mob::new(
                si,
                glam::Vec3::new(x as f32 + 0.5, y as f32 + 0.05, z as f32 + 0.5),
                (roll(rng) % 1024) as f32 / 1024.0 * std::f32::consts::TAU,
            );
            m.health = def.health;
            self.mobs.push(m);
            return; // one spawn per cycle
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
            if def.hostile {
                continue; // wardens dissolve on save — never persisted
            }
            let _ = writeln!(
                out,
                "[[mob]]\nspecies = \"{}\"\npos = [{}, {}, {}]\nyaw = {}\nhealth = {}\nfed = {}\ngrowth = {}\n",
                def.name, m.pos.x, m.pos.y, m.pos.z, m.yaw, m.health, m.fed, m.growth
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
        fn one() -> f32 {
            1.0
        }
        #[derive(Deserialize)]
        struct MobT {
            species: String,
            pos: [f32; 3],
            yaw: f32,
            health: f32,
            #[serde(default)]
            fed: bool,
            #[serde(default = "one")]
            growth: f32,
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
                    m.fed = t.fed;
                    m.growth = t.growth.clamp(0.05, 1.0);
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

    /// WFC3 RLE bytes for a chunk — the save format, reused as the wire
    /// format for multiplayer chunk streaming.
    pub fn chunk_rle(&self, pos: ChunkPos) -> Option<Vec<u8>> {
        let chunk = self.chunks.get(&pos)?;
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
        Some(buf)
    }

    /// Insert a network-streamed chunk, remapping host block ids to
    /// local ones. Relights and marks for remesh.
    pub fn insert_remote_chunk(&mut self, pos: ChunkPos, rle: &[u8], remap: &[BlockId]) {
        if !rle.starts_with(b"WFC3") {
            return;
        }
        let mut chunk = Chunk::new();
        let out = chunk.raw_mut();
        let mut o = 0;
        let mut i = 4;
        while i + 4 <= rle.len() && o < out.len() {
            let count = u16::from_le_bytes([rle[i], rle[i + 1]]) as usize;
            let stored = u16::from_le_bytes([rle[i + 2], rle[i + 3]]) as usize;
            let id = remap.get(stored).copied().unwrap_or(self.reg.unknown_block);
            let end = (o + count).min(out.len());
            out[o..end].fill(id.0);
            o = end;
            i += 4;
        }
        chunk.dirty = true;
        self.chunks.insert(pos, chunk);
        self.relight_and_cascade(pos);
        // Neighbors need remeshing for the new border faces.
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let n = ChunkPos { x: pos.x + dx, z: pos.z + dz };
            if let Some(c) = self.chunks.get_mut(&n) {
                c.dirty = true;
            }
        }
    }

    fn save_chunk(&self, pos: ChunkPos, chunk: &Chunk) -> std::io::Result<()> {
        let buf = self.chunk_rle(pos).unwrap_or_default();
        let mut f = fs::File::create(self.chunk_file(pos))?;
        f.write_all(&buf)
    }

    pub fn save_modified(&self) {
        if self.remote {
            return; // the host owns the world
        }
        let _ = fs::create_dir_all(&self.save_dir);
        write_world_meta(&self.save_dir, self.seed, &self.mode, self.ire);
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

    // ---------------- lighting ----------------

    /// (block light, sky light) at a world position. Unloaded chunks read
    /// as open sky so the world's edge doesn't render black.
    pub fn light_at(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        if y < 0 {
            return (0, 0);
        }
        if y >= CHUNK_Y as i32 {
            return (0, 15);
        }
        match self.chunks.get(&ChunkPos::of_world(x, z)) {
            Some(c) => c.light(
                x.rem_euclid(CHUNK_X as i32) as usize,
                y as usize,
                z.rem_euclid(CHUNK_Z as i32) as usize,
            ),
            None => (0, 15),
        }
    }

    /// Recompute both light channels for one chunk from scratch: sky column
    /// scan, then BFS from emitters and lit cells, seeded across chunk
    /// borders from loaded neighbors. Returns true if any value changed.
    fn relight_chunk(&mut self, pos: ChunkPos) -> bool {
        const NX: usize = CHUNK_X;
        const NY: usize = CHUNK_Y;
        const NZ: usize = CHUNK_Z;
        let idx = |x: usize, y: usize, z: usize| (x * NZ + z) * NY + y;
        let reg = self.reg.clone();
        let Some(chunk) = self.chunks.get(&pos) else { return false };

        // Per-cell properties, resolved once.
        #[derive(Clone, Copy)]
        struct Cell {
            opaque: bool,
            cost: u8, // propagation cost: 1, or 2 through water
            emit: u8,
        }
        let mut cells = vec![Cell { opaque: false, cost: 1, emit: 0 }; NX * NY * NZ];
        for x in 0..NX {
            for z in 0..NZ {
                for y in 0..NY {
                    let d = reg.block(chunk.get(x, y, z));
                    cells[idx(x, y, z)] = Cell {
                        opaque: d.opaque,
                        cost: if d.water_level.is_some() { 2 } else { 1 },
                        emit: d.light_emit,
                    };
                }
            }
        }

        let mut lb = vec![0u8; NX * NY * NZ];
        let mut ls = vec![0u8; NX * NY * NZ];
        let mut sky_q: VecDeque<(usize, usize, usize)> = VecDeque::new();
        let mut blk_q: VecDeque<(usize, usize, usize)> = VecDeque::new();

        // Sky columns: full light straight down to the first opaque block,
        // dimming through water.
        for x in 0..NX {
            for z in 0..NZ {
                let mut v = 15u8;
                for y in (0..NY).rev() {
                    let c = cells[idx(x, y, z)];
                    if c.opaque {
                        break; // rest of the column stays 0
                    }
                    if c.cost > 1 {
                        v = v.saturating_sub(1);
                    }
                    ls[idx(x, y, z)] = v;
                    if v >= 2 {
                        sky_q.push_back((x, y, z));
                    }
                    if v == 0 {
                        break;
                    }
                }
            }
        }
        // Emitters.
        for x in 0..NX {
            for z in 0..NZ {
                for y in 0..NY {
                    let e = cells[idx(x, y, z)].emit;
                    if e > 0 {
                        lb[idx(x, y, z)] = e;
                        blk_q.push_back((x, y, z));
                    }
                }
            }
        }
        // Border seeds from loaded neighbors (light crosses chunk seams).
        let mut seed = |grid: &mut Vec<u8>,
                        q: &mut VecDeque<(usize, usize, usize)>,
                        sky: bool| {
            for (dx, dz, edge_x, edge_z) in [
                (-1i32, 0i32, 0usize, usize::MAX),
                (1, 0, NX - 1, usize::MAX),
                (0, -1, usize::MAX, 0usize),
                (0, 1, usize::MAX, NZ - 1),
            ] {
                let npos = ChunkPos { x: pos.x + dx, z: pos.z + dz };
                let Some(nc) = self.chunks.get(&npos) else { continue };
                // The neighbor's cell touching our edge cell.
                let (nb_x, nb_z) = (
                    if dx == -1 { NX - 1 } else { 0 },
                    if dz == -1 { NZ - 1 } else { 0 },
                );
                for t in 0..(if edge_x == usize::MAX { NX } else { NZ }) {
                    for y in 0..NY {
                        let (ox, oz, nx, nz) = if edge_x != usize::MAX {
                            (edge_x, t, nb_x, t)
                        } else {
                            (t, edge_z, t, nb_z)
                        };
                        let (nlb, nls) = nc.light(nx, y, nz);
                        let v = if sky { nls } else { nlb };
                        if v < 2 {
                            continue;
                        }
                        let c = cells[idx(ox, y, oz)];
                        if c.opaque {
                            continue;
                        }
                        let nv = v.saturating_sub(c.cost);
                        if nv > grid[idx(ox, y, oz)] {
                            grid[idx(ox, y, oz)] = nv;
                            q.push_back((ox, y, oz));
                        }
                    }
                }
            }
        };
        seed(&mut ls, &mut sky_q, true);
        seed(&mut lb, &mut blk_q, false);

        // BFS relax (shared by both channels).
        let bfs = |grid: &mut Vec<u8>, q: &mut VecDeque<(usize, usize, usize)>| {
            while let Some((x, y, z)) = q.pop_front() {
                let v = grid[idx(x, y, z)];
                if v < 2 {
                    continue;
                }
                let mut relax = |nx: usize, ny: usize, nz: usize| {
                    let c = cells[idx(nx, ny, nz)];
                    if c.opaque {
                        return;
                    }
                    let nv = v.saturating_sub(c.cost);
                    if nv > grid[idx(nx, ny, nz)] {
                        grid[idx(nx, ny, nz)] = nv;
                        q.push_back((nx, ny, nz));
                    }
                };
                if x > 0 { relax(x - 1, y, z); }
                if x < NX - 1 { relax(x + 1, y, z); }
                if y > 0 { relax(x, y - 1, z); }
                if y < NY - 1 { relax(x, y + 1, z); }
                if z > 0 { relax(x, y, z - 1); }
                if z < NZ - 1 { relax(x, y, z + 1); }
            }
        };
        bfs(&mut lb, &mut blk_q);
        bfs(&mut ls, &mut sky_q);

        let chunk = self.chunks.get_mut(&pos).unwrap();
        let (old_b, old_s) = chunk.light_raw();
        if old_b == lb.as_slice() && old_s == ls.as_slice() {
            return false;
        }
        let (dst_b, dst_s) = chunk.light_raw_mut();
        dst_b.copy_from_slice(&lb);
        dst_s.copy_from_slice(&ls);
        chunk.dirty = true;
        true
    }

    /// Relight a chunk and let changes ripple to loaded neighbors until the
    /// light field settles (light reaches at most ~1 chunk, so this is
    /// short); every changed chunk is marked for remesh.
    pub fn relight_and_cascade(&mut self, start: ChunkPos) {
        let mut queue = VecDeque::from([start]);
        let mut visits: HashMap<(i32, i32), u32> = HashMap::new();
        while let Some(p) = queue.pop_front() {
            let v = visits.entry((p.x, p.z)).or_insert(0);
            if *v >= 4 {
                continue; // safety cap; converges long before this
            }
            *v += 1;
            if !self.chunks.contains_key(&p) {
                continue;
            }
            if self.relight_chunk(p) {
                for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let n = ChunkPos { x: p.x + dx, z: p.z + dz };
                    if self.chunks.contains_key(&n) {
                        queue.push_back(n);
                    }
                }
            }
        }
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
            if self.log_edits {
                self.edit_log.push((x, y, z, b));
            }
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
        // Cross blocks (torches, plants) pop off when their support vanishes.
        if !self.reg.is_solid(b) && y + 1 < CHUNK_Y as i32 {
            let above = self.get_block(x, y + 1, z);
            if above != AIR && self.reg.block(above).cross {
                if let Some((item, n)) = self.reg.block(above).drops {
                    let reg = self.reg.clone();
                    self.pending_drops.push(((x, y + 1, z), ItemStack::new(&reg, item, n)));
                }
                self.set_block(x, y + 1, z, AIR);
            }
        }
        self.relight_and_cascade(pos);
        // A changed block invalidates any machine state living there.
        if let Some(e) = self.block_entities.remove(&(x, y, z)) {
            let spilled: Vec<ItemStack> = match e {
                BlockEntity::Furnace(f) => {
                    [f.input, f.fuel, f.output].into_iter().flatten().collect()
                }
                BlockEntity::Chest(c) => c.slots.into_iter().flatten().collect(),
                BlockEntity::Offering(o) => o.slots.into_iter().flatten().collect(),
            };
            for s in spilled {
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
        let mut saplings: Vec<(i32, i32, i32, String, u32)> = Vec::new();
        for pos in keys {
            for _ in 0..8 {
                *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                let r = *rng >> 8;
                let (lx, lz) = ((r % 16) as i32, ((r >> 4) % 16) as i32);
                let y = ((r >> 8) % CHUNK_Y as u32) as i32;
                let (wx, wz) = (pos.x * 16 + lx, pos.z * 16 + lz);
                let b = self.get_block(wx, y, wz);
                let d = reg.block(b);
                if let Some(species) = &d.sapling {
                    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    if ((*rng >> 8) as f32 / (1 << 24) as f32) < 0.02 {
                        saplings.push((wx, y, wz, species.clone(), *rng));
                    }
                    continue;
                }
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
            // A crop reaching its final stage refunds ire (capped daily).
            if self.reg.block(b).crop_next.is_none() {
                self.plant_ire(0.5);
            }
            self.set_block(x, y, z, b);
        }
        for (x, y, z, _species, rnd) in saplings {
            self.try_grow_sapling(x, y, z, rnd);
        }
    }

    /// Attempt to mature the sapling at this position. On success the
    /// tree is built and the wild refunds -2 ire, bypassing the daily
    /// planting cap (it took days — it IS the slow path).
    pub fn try_grow_sapling(&mut self, x: i32, y: i32, z: i32, rnd: u32) -> bool {
        let b = self.get_block(x, y, z);
        let Some(species) = self.reg.block(b).sapling.clone() else { return false };
        if self.grow_tree(x, y, z, &species, rnd) {
            self.add_ire(-2.0);
            true
        } else {
            false
        }
    }

    /// Advance machines. Returns true if any visible state changed.
    pub fn tick_entities(&mut self, dt: f32) {
        let reg = self.reg.clone();
        for e in self.block_entities.values_mut() {
            let BlockEntity::Furnace(f) = e else { continue };
            let smelt = f.input.and_then(|s| reg.smelt_for(s.item)).cloned();
            let output_ok = |f: &FurnaceState, out: crate::registry::ItemId| match f.output {
                None => true,
                Some(o) => o.item == out && o.count < reg.item(out).max_stack,
            };
            let can_smelt = smelt.as_ref().is_some_and(|s| output_ok(f, s.output));

            if f.burn_left <= 0.0 && can_smelt {
                // Light more fuel (the forge feeds the wild's ire).
                if let Some(fs) = f.fuel {
                    if let Some((burn, speed)) = reg.fuel_value(fs.item) {
                        f.burn_left = burn;
                        f.burn_total = burn;
                        f.burn_speed = speed;
                        let left = fs.count - 1;
                        f.fuel = if left > 0 { Some(ItemStack { count: left, ..fs }) } else { None };
                        self.ire = (self.ire + 0.1).min(100.0);
                    }
                }
            }
            if f.burn_left > 0.0 {
                f.burn_left = (f.burn_left - dt).max(0.0);
                if can_smelt {
                    let s = smelt.as_ref().unwrap();
                    f.progress += dt * f.burn_speed.max(1.0);
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
            match e {
                BlockEntity::Furnace(f) => {
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
                    let _ = writeln!(
                        out,
                        "progress = {}\nburn_left = {}\nburn_total = {}\nburn_speed = {}\n",
                        f.progress, f.burn_left, f.burn_total, f.burn_speed
                    );
                }
                BlockEntity::Chest(c) => {
                    let _ = writeln!(out, "[[chest]]\npos = [{x}, {y}, {z}]");
                    if c.wild_owned {
                        let _ = writeln!(out, "wild_owned = true");
                    }
                    for (i, st) in c.slots.iter().enumerate() {
                        if let Some(st) = st {
                            let _ = writeln!(
                                out,
                                "[[chest.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}",
                                self.reg.item(st.item).name, st.count, st.durability
                            );
                        }
                    }
                    let _ = writeln!(out);
                }
                BlockEntity::Offering(o) => {
                    let _ = writeln!(out, "[[offering]]\npos = [{x}, {y}, {z}]");
                    for (i, st) in o.slots.iter().enumerate() {
                        if let Some(st) = st {
                            let _ = writeln!(
                                out,
                                "[[offering.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}",
                                self.reg.item(st.item).name, st.count, st.durability
                            );
                        }
                    }
                    let _ = writeln!(out);
                }
            }
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
            #[serde(default)]
            burn_speed: f32,
        }
        #[derive(Deserialize)]
        struct ChestSlotT {
            index: usize,
            item: String,
            count: u32,
            durability: u32,
        }
        #[derive(Deserialize)]
        struct ChestT {
            pos: [i32; 3],
            #[serde(default)]
            wild_owned: bool,
            #[serde(default)]
            slot: Vec<ChestSlotT>,
        }
        #[derive(Deserialize)]
        struct FileT {
            #[serde(default)]
            furnace: Vec<FurnaceT>,
            #[serde(default)]
            chest: Vec<ChestT>,
            #[serde(default)]
            offering: Vec<ChestT>,
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
                    burn_speed: fu.burn_speed.max(1.0),
                }),
            );
        }
        for ch in parsed.chest {
            let mut state = ChestState { wild_owned: ch.wild_owned, ..Default::default() };
            for sl in ch.slot {
                if sl.index < CHEST_SLOTS {
                    if let Some(item) = self.reg.item_id(&sl.item) {
                        state.slots[sl.index] =
                            Some(ItemStack { item, count: sl.count, durability: sl.durability });
                    }
                }
            }
            self.block_entities
                .insert((ch.pos[0], ch.pos[1], ch.pos[2]), BlockEntity::Chest(state));
        }
        for of in parsed.offering {
            let mut state = OfferingState::default();
            for sl in of.slot {
                if sl.index < 3 {
                    if let Some(item) = self.reg.item_id(&sl.item) {
                        state.slots[sl.index] =
                            Some(ItemStack { item, count: sl.count, durability: sl.durability });
                    }
                }
            }
            self.block_entities
                .insert((of.pos[0], of.pos[1], of.pos[2]), BlockEntity::Offering(state));
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
