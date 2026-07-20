//! World: chunk map with block access, fluid simulation, and versioned
//! persistence (save v2 with a per-world id palette; legacy v1 migrates).

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos, SEA_LEVEL};
use crate::inventory::ItemStack;
use crate::mobs::{Mob, MobEvent, ProjHit, Projectile};
use crate::registry::{AIR, BlockId, Registry};
use crate::worldgen::Generator;

/// Per-block persistent state for interactive machines.
// Chest dwarfs the others; entity counts are tiny, so boxing would
// only add indirection.
#[allow(clippy::large_enum_variant)]
pub enum BlockEntity {
    Furnace(FurnaceState),
    Chest(ChestState),
    Offering(OfferingState),
    Bloomery(BloomeryState),
    Clamp(ClampState),
    Anvil(AnvilState),
    Kiln(KilnState),
}

/// The steelworks stack: a batch of charge + fuel, fired for half a
/// day inside a validated firebrick shell.
#[derive(Default)]
pub struct BloomeryState {
    pub charge: [Option<ItemStack>; 4],
    pub fuel: [Option<ItemStack>; 4],
    pub lit: bool,
    /// Seconds fired so far (out of BLOOMERY_FIRE_SECS).
    pub progress: f32,
    /// Hollow core cell of the validated stack (set on lighting).
    pub core: (i32, i32, i32),
}

/// The glass kiln: sand + one powder + charcoal, fired hot and fast.
#[derive(Default)]
pub struct KilnState {
    pub sand: [Option<ItemStack>; 4],
    pub powder: Option<ItemStack>,
    pub fuel: [Option<ItemStack>; 4],
    pub lit: bool,
    pub progress: f32,
    pub core: (i32, i32, i32),
}

/// A quarter-day of white heat per glass batch.
pub const KILN_FIRE_SECS: f32 = 150.0;

/// A covered log pile smoldering into charcoal.
pub struct ClampState {
    pub logs: Vec<(i32, i32, i32)>,
    /// Seconds remaining until the whole pile converts.
    pub timer: f32,
}

/// A bloom resting on the anvil, part-way worked.
#[derive(Default)]
pub struct AnvilState {
    pub bloom: Option<ItemStack>,
    pub strikes: u32,
}

/// A gravity block mid-fall (host-simulated; guests get snapshots).
#[derive(Clone, Copy)]
pub struct FallingBlock {
    pub pos: glam::Vec3,
    pub vel: f32,
    pub block: BlockId,
}

/// Half an in-game day of fire per batch.
pub const BLOOMERY_FIRE_SECS: f32 = 300.0;
/// Seconds of smolder per log in a charcoal clamp.
pub const CLAMP_SECS_PER_LOG: f32 = 300.0;

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
        ChestState {
            slots: [None; CHEST_SLOTS],
            wild_owned: false,
        }
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
    let (seed, mode, ire, _, _) = read_world_meta_full(dir);
    (seed, mode, ire)
}

/// Full metadata: (seed, mode, ire, day, weather).
pub fn read_world_meta_full(dir: &std::path::Path) -> (Option<u32>, String, f32, u32, Weather) {
    if let Ok(t) = fs::read_to_string(dir.join("world.toml")) {
        let mut seed = None;
        let mut mode = "survival".to_string();
        let mut ire = 0.0f32;
        let mut day = 0u32;
        let mut weather = Weather::Clear;
        for l in t.lines() {
            if let Some(v) = l.strip_prefix("seed = ") {
                seed = v.trim().parse().ok();
            } else if let Some(v) = l.strip_prefix("mode = ") {
                mode = v.trim().trim_matches('"').to_string();
            } else if let Some(v) = l.strip_prefix("ire = ") {
                ire = v.trim().parse().unwrap_or(0.0);
            } else if let Some(v) = l.strip_prefix("day = ") {
                day = v.trim().parse().unwrap_or(0);
            } else if let Some(v) = l.strip_prefix("weather = ") {
                weather = Weather::from_name(v.trim().trim_matches('"'));
            }
        }
        (seed, mode, ire.clamp(0.0, 100.0), day, weather)
    } else {
        let seed = fs::read_to_string(dir.join("seed"))
            .ok()
            .and_then(|s| s.trim().parse().ok());
        (seed, "survival".to_string(), 0.0, 0, Weather::Clear)
    }
}

pub fn write_world_meta(dir: &std::path::Path, seed: u32, mode: &str, ire: f32) {
    write_world_meta_full(dir, seed, mode, ire, 0, Weather::Clear);
}

pub fn write_world_meta_full(
    dir: &std::path::Path,
    seed: u32,
    mode: &str,
    ire: f32,
    day: u32,
    weather: Weather,
) {
    let _ = fs::create_dir_all(dir);
    let _ = fs::write(
        dir.join("world.toml"),
        format!(
            "seed = {seed}\nmode = \"{mode}\"\nire = {ire:.2}\nday = {day}\nweather = \"{}\"\n",
            weather.name()
        ),
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
    /// Absolute sim-time in seconds (day * DAY_LENGTH + time-of-day),
    /// mirrored from the Server every tick so chunk load and random
    /// ticks share one clock.
    pub clock: f64,
    /// When each chunk last took its random ticks (persisted, so the
    /// world can live on while a chunk is away).
    last_random: HashMap<(i32, i32), f64>,
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
    /// Calendar day (increments at dawn, natural or slept-through).
    pub day: u32,
    /// Current weather + seconds remaining on it (the Server's machine
    /// drives this; it lives here so world.toml persistence is natural).
    pub weather: Weather,
    pub weather_timer: f32,
    /// Host mode: record block edits for broadcasting.
    pub log_edits: bool,
    pub edit_log: Vec<(i32, i32, i32, BlockId)>,
    /// Gravity blocks currently airborne.
    pub falling: Vec<FallingBlock>,
    /// (guest id, stack) owed over the wire: mining drops, kill loot,
    /// recovered arrows, brush finds — full stacks so durability rides.
    pub pending_gives: Vec<(u32, ItemStack)>,
    /// Next stable mob id (host side; ids exist for the wire).
    next_mob_id: u32,
}

/// Ire tier names, index = tier.
pub const IRE_TIERS: [&str; 4] = ["CALM", "UNEASY", "PROVOKED", "WRATHFUL"];

/// Small-λ Poisson draw (Knuth's product method) on the sim's LCG
/// stream — how many of the ticks a chunk missed actually landed.
fn poisson(lambda: f64, r: &mut u32) -> u32 {
    if lambda <= 0.0 {
        return 0;
    }
    let l = (-lambda).exp();
    let mut k = 0u32;
    let mut p = 1.0f64;
    loop {
        *r = r.wrapping_mul(1664525).wrapping_add(1013904223);
        p *= (*r >> 8) as f64 / (1 << 24) as f64;
        if p <= l || k > 64 {
            return k;
        }
        k += 1;
    }
}

// ---------------- calendar & weather ----------------

/// In-game days per season; four seasons make a 48-day year.
pub const SEASON_DAYS: u32 = 12;
pub const SEASONS: [&str; 4] = ["SPRING", "SUMMER", "AUTUMN", "WINTER"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Weather {
    Clear,
    Overcast,
    /// Rain or snow, decided per column by climate + season.
    Precip,
    Storm,
}

impl Weather {
    pub fn as_u8(self) -> u8 {
        match self {
            Weather::Clear => 0,
            Weather::Overcast => 1,
            Weather::Precip => 2,
            Weather::Storm => 3,
        }
    }
    pub fn from_u8(v: u8) -> Weather {
        match v {
            1 => Weather::Overcast,
            2 => Weather::Precip,
            3 => Weather::Storm,
            _ => Weather::Clear,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Weather::Clear => "clear",
            Weather::Overcast => "overcast",
            Weather::Precip => "precip",
            Weather::Storm => "storm",
        }
    }
    pub fn from_name(s: &str) -> Weather {
        match s {
            "overcast" => Weather::Overcast,
            "precip" | "rain" | "snow" => Weather::Precip,
            "storm" => Weather::Storm,
            _ => Weather::Clear,
        }
    }
    /// Anything falling from the sky right now?
    pub fn precipitating(self) -> bool {
        matches!(self, Weather::Precip | Weather::Storm)
    }
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
            clock: 0.0,
            last_random: HashMap::new(),
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
            day: 0,
            weather: Weather::Clear,
            weather_timer: 0.0,
            log_edits: false,
            edit_log: Vec::new(),
            falling: Vec::new(),
            pending_gives: Vec::new(),
            next_mob_id: 1,
        }
    }

    /// 0 spring, 1 summer, 2 autumn, 3 winter.
    pub fn season(&self) -> usize {
        ((self.day / SEASON_DAYS) % 4) as usize
    }

    /// 0..1 through the current season.
    pub fn season_progress(&self) -> f32 {
        (self.day % SEASON_DAYS) as f32 / SEASON_DAYS as f32
    }

    /// Does precipitation fall as snow in this column? The threshold
    /// relaxes in winter so taiga and cold-temperate lands whiten.
    pub fn snows_at(&self, x: i32, z: i32) -> bool {
        let t = self.generator.climate(x, z).t;
        t < if self.season() == 3 { -0.05 } else { -0.35 }
    }

    /// Deserts stay dry: overcast skies, nothing falls.
    pub fn rains_at(&self, x: i32, z: i32) -> bool {
        let c = self.generator.climate(x, z);
        !(c.t > 0.6 && c.h < -0.5)
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
        // The wild breathes easier when the land drinks.
        let decay = if self.weather.precipitating() {
            5.0
        } else {
            4.0
        };
        self.add_ire(-decay * day_frac);
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
        let per = if [
            "base:heartwood",
            "base:living_wood",
            "base:ember",
            "base:frost_shard",
        ]
        .contains(&d.name.as_str())
        {
            2.0
        } else if d.name.ends_with("_sapling")
            || d.name.contains("raw_")
            || d.name.contains("cooked_")
        {
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
            let BlockEntity::Offering(o) = e else {
                continue;
            };
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
        let leaf_at = |w: &mut World, lx: i32, ly: i32, lz: i32| {
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
        let (seed, mode, ire, day, weather) = read_world_meta_full(&save_dir);
        let seed = seed.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as u32)
                .unwrap_or(1337)
        });
        write_world_meta_full(&save_dir, seed, &mode, ire, day, weather);
        let mut w = World::new(seed, save_dir, reg);
        w.mode = mode;
        w.ire = ire;
        w.day = day;
        w.weather = weather;
        w.clock = day as f64 * crate::server::DAY_LENGTH as f64;
        w.load_remap = w.read_palette_remap();
        w.load_entities();
        w.load_mobs();
        w.load_stamps();
        w
    }

    /// Per-chunk random-tick stamps: compact (x, z, time) triples.
    fn load_stamps(&mut self) {
        let Ok(buf) = fs::read(self.save_dir.join("stamps")) else {
            return;
        };
        for rec in buf.chunks_exact(16) {
            let x = i32::from_le_bytes(rec[0..4].try_into().unwrap());
            let z = i32::from_le_bytes(rec[4..8].try_into().unwrap());
            let t = f64::from_le_bytes(rec[8..16].try_into().unwrap());
            self.last_random.insert((x, z), t);
        }
    }

    fn save_stamps(&self) {
        let mut buf = Vec::with_capacity(self.last_random.len() * 16);
        for ((x, z), t) in &self.last_random {
            buf.extend_from_slice(&x.to_le_bytes());
            buf.extend_from_slice(&z.to_le_bytes());
            buf.extend_from_slice(&t.to_le_bytes());
        }
        let _ = fs::write(self.save_dir.join("stamps"), buf);
    }

    /// Map every stored numeric id to a current runtime id via string names.
    fn read_palette_remap(&self) -> Vec<BlockId> {
        let Ok(text) = fs::read_to_string(self.save_dir.join("palette")) else {
            return Vec::new();
        };
        let mut remap = Vec::new();
        for line in text.lines() {
            let Some((num, name)) = line.split_once(' ') else {
                continue;
            };
            let Ok(num) = num.parse::<usize>() else {
                continue;
            };
            if remap.len() <= num {
                remap.resize(num + 1, self.reg.unknown_block);
            }
            remap[num] = self
                .reg
                .block_id(name.trim())
                .unwrap_or(self.reg.unknown_block);
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
        let mut chunk = loaded.unwrap_or_else(|| self.generator.generate(pos, &self.reg));
        // The floor reseals on load: any hole in the bedrock (a
        // creative dig, an old bug) heals when the chunk comes back.
        // Idempotent — set() doesn't mark the chunk modified.
        if let Some(root) = self.reg.block_id("base:bedrock") {
            for lx in 0..CHUNK_X {
                for lz in 0..CHUNK_Z {
                    if chunk.get(lx, 0, lz) != root {
                        chunk.set(lx, 0, lz, root);
                    }
                }
            }
        }
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
        // A chunk seen for the first time is up to date; one loaded
        // from disk keeps its old stamp (the gap below reads it).
        let stamp = self.last_random.get(&(pos.x, pos.z)).copied();
        self.last_random.entry((pos.x, pos.z)).or_insert(self.clock);
        self.wake_seams(pos);
        self.relight_and_cascade(pos);
        // The world lived while this chunk was away: catch it up.
        if let Some(stamp) = stamp {
            let gap = self.clock - stamp;
            if gap > 60.0 {
                self.reconcile_chunk(pos, gap);
                self.last_random.insert((pos.x, pos.z), self.clock);
            }
        }
        true
    }

    // ---------------- ruins ----------------

    /// Deterministic per-chunk structure roll (at most one per chunk).
    fn seed_structures(&mut self, pos: ChunkPos) {
        let reg = self.reg.clone();
        let (cx, cz) = (pos.x * CHUNK_X as i32, pos.z * CHUNK_Z as i32);
        let biome = self.generator.biome(cx + 8, cz + 8).name().to_lowercase();
        for (si, st) in reg.structures.iter().enumerate() {
            if !st.biomes.contains(&biome) {
                continue;
            }
            let h = self.mob_hash(pos.x, pos.z, 9000 + si as u32);
            if !h.is_multiple_of(st.rarity) {
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
        let Some(st) = reg.structures.get(si).cloned() else {
            return;
        };
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
                                let mut state = ChestState {
                                    wild_owned: true,
                                    ..Default::default()
                                };
                                if let Some(table) = &st.loot {
                                    let n = 3 + (rng % 3) as usize;
                                    for (i, stck) in self
                                        .roll_loot(table, n as u32, &mut rng)
                                        .into_iter()
                                        .enumerate()
                                    {
                                        if i < CHEST_SLOTS {
                                            // Scatter through the chest.
                                            let slot = (i * 7 + (rng % 5) as usize) % CHEST_SLOTS;
                                            state.slots[slot] = Some(stck);
                                        }
                                    }
                                }
                                self.block_entities
                                    .insert((x, y, z), BlockEntity::Chest(state));
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
        if st.buried.is_some()
            && let Some(cob) = reg.block_id("base:cobblestone")
        {
            let hx = x0 + 1;
            let hz = z0 + 1;
            let sy = self.surface_height(hx, hz);
            self.set_block(hx, sy + 1, hz, cob);
            self.set_block(hx, sy + 2, hz, cob);
        }
    }

    /// Weighted rolls from a loot table.
    pub fn roll_loot(&self, table: &str, rolls: u32, rng: &mut u32) -> Vec<ItemStack> {
        let Some(entries) = self.reg.loots.get(table) else {
            return Vec::new();
        };
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

    // ---------------- gravity blocks ----------------

    /// Lift a block out of the grid and into the air (atomically: the
    /// cell empties in the same call, so it can't be duped).
    fn detach(&mut self, x: i32, y: i32, z: i32, b: BlockId) {
        self.set_block(x, y, z, AIR);
        self.falling.push(FallingBlock {
            pos: glam::Vec3::new(x as f32, y as f32, z as f32),
            vel: 0.0,
            block: b,
        });
    }

    /// Advance airborne blocks; landings re-plant (popping any plant or
    /// layer they crush) and re-trigger the cell above the launch site
    /// through the normal edit cascade.
    pub fn tick_falling(&mut self, dt: f32) {
        if self.falling.is_empty() {
            return;
        }
        // Landings apply immediately so a stacked column settles one on
        // top of the other instead of racing into the same cell.
        let mut fallen = std::mem::take(&mut self.falling);
        let mut still = Vec::with_capacity(fallen.len());
        for mut f in fallen.drain(..) {
            f.vel = (f.vel + 20.0 * dt).min(30.0);
            f.pos.y -= f.vel * dt;
            let (x, z) = (f.pos.x.floor() as i32, f.pos.z.floor() as i32);
            let below = f.pos.y.floor() as i32;
            if below < 0 {
                continue; // out of the world (should be impossible)
            }
            if !self.reg.is_solid(self.get_block(x, below, z)) {
                still.push(f);
                continue;
            }
            // Land on the first free cell above the obstruction - a
            // second sand in the same column stacks instead of popping.
            let mut y = below + 1;
            while y < CHUNK_Y as i32 - 1 && self.reg.is_solid(self.get_block(x, y, z)) {
                y += 1;
            }
            let b = f.block;
            let cur = self.get_block(x, y, z);
            if cur != AIR {
                // Crushed: the plant/layer pops as its drop first.
                if let Some((item, n)) = self.reg.block(cur).drops {
                    let reg = self.reg.clone();
                    self.pending_drops
                        .push(((x, y, z), ItemStack::new(&reg, item, n)));
                }
            }
            self.set_block(x, y, z, b);
        }
        // Landings may have detached more (rare); keep both sets.
        self.falling.extend(still);
    }

    /// Land every airborne block instantly (world save/quit).
    pub fn settle_falling(&mut self) {
        while !self.falling.is_empty() {
            self.tick_falling(0.5);
        }
    }

    // ---------------- steelworks ----------------

    /// Validate the bloomery multiblock at this mouth: a hollow 1x1
    /// core beside the mouth wrapped in a 3-wide, 3-tall firebrick
    /// ring (23 firebrick + the mouth), open on top. Returns the core.
    pub fn check_bloomery(&self, x: i32, y: i32, z: i32) -> Option<(i32, i32, i32)> {
        let mouth = [
            self.reg.block_id("base:bloomery"),
            self.reg.block_id("base:bloomery_lit"),
        ];
        self.check_stack(x, y, z, &mouth)
    }

    /// The same stack with a kiln in its mouth fires glass instead.
    pub fn check_kiln(&self, x: i32, y: i32, z: i32) -> Option<(i32, i32, i32)> {
        let mouth = [
            self.reg.block_id("base:kiln"),
            self.reg.block_id("base:kiln_lit"),
        ];
        self.check_stack(x, y, z, &mouth)
    }

    /// The shared shell scan: the stack is the stack; the mouth block
    /// decides the craft.
    fn check_stack(
        &self,
        x: i32,
        y: i32,
        z: i32,
        mouth: &[Option<BlockId>; 2],
    ) -> Option<(i32, i32, i32)> {
        let fb = self.reg.block_id("base:firebrick")?;
        'dirs: for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (cx, cz) = (x + dx, z + dz);
            for ly in 0..3 {
                if self.get_block(cx, y + ly, cz) != AIR {
                    continue 'dirs;
                }
                for rx in -1..=1 {
                    for rz in -1..=1 {
                        if rx == 0 && rz == 0 {
                            continue;
                        }
                        let (bx, bz) = (cx + rx, cz + rz);
                        let b = self.get_block(bx, y + ly, bz);
                        if bx == x && bz == z && ly == 0 {
                            if !mouth.contains(&Some(b)) {
                                continue 'dirs;
                            }
                        } else if b != fb {
                            continue 'dirs;
                        }
                    }
                }
            }
            return Some((cx, y, cz));
        }
        None
    }

    /// Light a charged bloomery. Errors name what's missing.
    pub fn light_bloomery(&mut self, x: i32, y: i32, z: i32) -> Result<(), &'static str> {
        let core = self
            .check_bloomery(x, y, z)
            .ok_or("the stack is breached")?;
        let Some(BlockEntity::Bloomery(b)) = self.block_entities.get_mut(&(x, y, z)) else {
            return Err("nothing charged");
        };
        if b.lit {
            return Err("already firing");
        }
        let n_charge: u32 = b.charge.iter().flatten().map(|s| s.count).sum();
        let n_fuel: u32 = b.fuel.iter().flatten().map(|s| s.count).sum();
        if n_charge < 2 || n_fuel < 2 {
            return Err("needs at least 2 charge and 2 charcoal");
        }
        b.lit = true;
        b.progress = 0.0;
        b.core = core;
        self.swap_block_keep_entity(x, y, z, "base:bloomery_lit");
        Ok(())
    }

    /// Light a charged kiln. Errors name what's missing.
    pub fn light_kiln(&mut self, x: i32, y: i32, z: i32) -> Result<(), &'static str> {
        let core = self.check_kiln(x, y, z).ok_or("the stack is breached")?;
        let Some(BlockEntity::Kiln(k)) = self.block_entities.get_mut(&(x, y, z)) else {
            return Err("nothing charged");
        };
        if k.lit {
            return Err("already firing");
        }
        let n_sand: u32 = k.sand.iter().flatten().map(|s| s.count).sum();
        let n_fuel: u32 = k.fuel.iter().flatten().map(|s| s.count).sum();
        if n_sand < 2 || n_fuel < 2 {
            return Err("needs at least 2 sand and 2 charcoal");
        }
        k.lit = true;
        k.progress = 0.0;
        k.core = core;
        self.swap_block_keep_entity(x, y, z, "base:kiln_lit");
        Ok(())
    }

    /// Fire every lit kiln: shared shell/weather rules, glass out.
    fn tick_kilns(&mut self, dt: f32) {
        let keys: Vec<(i32, i32, i32)> = self
            .block_entities
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Kiln(k) if k.lit))
            .map(|(k, _)| *k)
            .collect();
        for pos in keys {
            let Some(BlockEntity::Kiln(mut k)) = self.block_entities.remove(&pos) else {
                continue;
            };
            let (x, y, z) = pos;
            if self.check_kiln(x, y, z).is_none() {
                k.lit = false;
                k.progress = 0.0;
                self.swap_block_keep_entity(x, y, z, "base:kiln");
                self.block_entities.insert(pos, BlockEntity::Kiln(k));
                continue;
            }
            let unroofed = self.light_at(k.core.0, y + 3, k.core.2).1 == 15;
            let wet = self.weather.precipitating() && self.rains_at(x, z) && unroofed;
            if wet && self.weather == Weather::Storm {
                k.lit = false;
                k.progress = 0.0;
                self.swap_block_keep_entity(x, y, z, "base:kiln");
                self.block_entities.insert(pos, BlockEntity::Kiln(k));
                continue;
            }
            k.progress += dt * if wet { 0.5 } else { 1.0 };
            if k.progress >= KILN_FIRE_SECS {
                if let Some((_, fuel_item, clear)) = self.reg.kiln_base {
                    let n_sand: u32 = k.sand.iter().flatten().map(|s| s.count).sum();
                    let n_fuel: u32 = k.fuel.iter().flatten().map(|s| s.count).sum();
                    let pairs = n_sand.min(n_fuel) / 2;
                    let out_n = pairs * 2;
                    // One powder colors the whole batch.
                    let colored = k.powder.as_ref().and_then(|p| {
                        self.reg
                            .kiln
                            .iter()
                            .find(|(pw, _)| *pw == p.item)
                            .map(|(_, g)| *g)
                    });
                    let out_item = colored.unwrap_or(clear);
                    if colored.is_some()
                        && let Some(p) = &mut k.powder
                    {
                        p.count -= 1;
                        if p.count == 0 {
                            k.powder = None;
                        }
                    }
                    let eat = |slots: &mut [Option<ItemStack>; 4], mut n: u32| {
                        for s in slots.iter_mut() {
                            if n == 0 {
                                break;
                            }
                            if let Some(st) = s {
                                let take = st.count.min(n);
                                n -= take;
                                st.count -= take;
                                if st.count == 0 {
                                    *s = None;
                                }
                            }
                        }
                    };
                    eat(&mut k.sand, pairs * 2);
                    eat(&mut k.fuel, pairs * 2);
                    let _ = fuel_item;
                    if out_n > 0 {
                        let reg = self.reg.clone();
                        let mut out = ItemStack::new(&reg, out_item, 1);
                        out.count = out_n;
                        for s in k.sand.iter_mut() {
                            if s.is_none() {
                                *s = Some(out);
                                break;
                            }
                        }
                    }
                }
                k.lit = false;
                k.progress = 0.0;
                self.swap_block_keep_entity(x, y, z, "base:kiln");
            }
            self.block_entities.insert(pos, BlockEntity::Kiln(k));
        }
    }

    /// Swap a block without invalidating the machine living there.
    fn swap_block_keep_entity(&mut self, x: i32, y: i32, z: i32, to: &str) {
        let Some(to) = self.reg.block_id(to) else {
            return;
        };
        let e = self.block_entities.remove(&(x, y, z));
        self.set_block(x, y, z, to);
        if let Some(e) = e {
            self.block_entities.insert((x, y, z), e);
        }
    }

    /// Flood-fill a covered log pile from the clicked log and light it.
    /// Exactly one face (the lighting face) may be exposed.
    pub fn try_light_clamp(&mut self, x: i32, y: i32, z: i32) -> Result<usize, &'static str> {
        let logs_tag = self.reg.tags.get("base:logs").cloned().unwrap_or_default();
        let is_log = |w: &World, p: (i32, i32, i32)| {
            let b = w.get_block(p.0, p.1, p.2);
            w.reg
                .item_id(&w.reg.block(b).name)
                .is_some_and(|i| logs_tag.contains(&i))
        };
        if !is_log(self, (x, y, z)) {
            return Err("light a log");
        }
        let mut set = vec![(x, y, z)];
        let mut queue = vec![(x, y, z)];
        while let Some(p) = queue.pop() {
            for d in [
                (1, 0, 0),
                (-1, 0, 0),
                (0, 1, 0),
                (0, -1, 0),
                (0, 0, 1),
                (0, 0, -1),
            ] {
                let n = (p.0 + d.0, p.1 + d.1, p.2 + d.2);
                if !set.contains(&n) && is_log(self, n) {
                    set.push(n);
                    if set.len() > 8 {
                        return Err("the pile is too big to smolder (8 logs at most)");
                    }
                    queue.push(n);
                }
            }
        }
        if set.len() < 2 {
            return Err("a clamp needs at least 2 logs");
        }
        let mut exposed = 0;
        for p in &set {
            for d in [
                (1, 0, 0),
                (-1, 0, 0),
                (0, 1, 0),
                (0, -1, 0),
                (0, 0, 1),
                (0, 0, -1),
            ] {
                let n = (p.0 + d.0, p.1 + d.1, p.2 + d.2);
                if set.contains(&n) {
                    continue;
                }
                if !self.reg.is_solid(self.get_block(n.0, n.1, n.2)) {
                    exposed += 1;
                }
            }
        }
        if exposed > 1 {
            return Err("cover the pile with earth (one face open)");
        }
        let n = set.len();
        self.block_entities.insert(
            (x, y, z),
            BlockEntity::Clamp(ClampState {
                logs: set,
                timer: n as f32 * CLAMP_SECS_PER_LOG,
            }),
        );
        Ok(n)
    }

    /// The station kind ("anvil"/"quern") of the block at pos.
    fn station_at(&self, pos: (i32, i32, i32)) -> Option<String> {
        self.reg
            .block(self.get_block(pos.0, pos.1, pos.2))
            .interaction
            .clone()
    }

    /// Rest a workable item on a station (one at a time). Only items
    /// this station's worked-table accepts may rest.
    pub fn anvil_put(&mut self, pos: (i32, i32, i32), stack: ItemStack) -> bool {
        let Some(st) = self.station_at(pos) else {
            return false;
        };
        if !self
            .reg
            .worked
            .iter()
            .any(|w| w.input == stack.item && w.station == st)
        {
            return false;
        }
        let e = self
            .block_entities
            .entry(pos)
            .or_insert_with(|| BlockEntity::Anvil(Default::default()));
        if let BlockEntity::Anvil(a) = e
            && a.bloom.is_none()
        {
            a.bloom = Some(ItemStack { count: 1, ..stack });
            a.strikes = 0;
            return true;
        }
        false
    }

    pub fn anvil_take(&mut self, pos: (i32, i32, i32)) -> Option<ItemStack> {
        if let Some(BlockEntity::Anvil(a)) = self.block_entities.get_mut(&pos) {
            a.strikes = 0;
            return a.bloom.take();
        }
        None
    }

    /// One hammer strike; finishing the work returns the output.
    pub fn anvil_strike(&mut self, pos: (i32, i32, i32)) -> Option<ItemStack> {
        let reg = self.reg.clone();
        let st = self.station_at(pos)?;
        if let Some(BlockEntity::Anvil(a)) = self.block_entities.get_mut(&pos)
            && let Some(b) = a.bloom
            && let Some(def) = reg
                .worked
                .iter()
                .find(|w| w.input == b.item && w.station == st)
        {
            a.strikes += 1;
            if a.strikes >= def.strikes {
                a.bloom = None;
                a.strikes = 0;
                let mut out = ItemStack::new(&reg, def.output, 1);
                out.count = def.count;
                return Some(out);
            }
        }
        None
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
            if def.hostile || !def.biomes.contains(&biome) {
                continue;
            }
            let roll = self.mob_hash(pos.x, pos.z, 7000 + si as u32);
            if !roll.is_multiple_of(def.rarity) {
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
        // Stamp stable ids on anything new (spawns, births, loaded saves).
        for m in &mut self.mobs {
            if m.id == 0 {
                m.id = self.next_mob_id;
                self.next_mob_id += 1;
            }
        }
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
            let Some(def) = reg.animals.get(m.species) else {
                return false;
            };
            if m.pos.y < -20.0 {
                return false; // fell out of the world somehow
            }
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
        // Husbandry: two fed adults of a species near each other bear
        // young - but not in winter; spring is the birthing season.
        let winter = self.season() == 3;
        let mut births: Vec<(usize, usize)> = Vec::new();
        for i in 0..mobs.len() {
            if winter || births.iter().any(|&(a, b)| a == i || b == i) {
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
                events.push(MobEvent::Bred);
            }
        }
        self.mobs = mobs;

        // Repopulation: overhunted wildlife slowly recovers, away from the
        // player and only under the local cap.
        // Spring teems, winter starves: the repop clock runs at double
        // or half speed with the season.
        self.repop_timer += dt
            * match self.season() {
                0 => 2.0,
                3 => 0.5,
                _ => 1.0,
            };
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
                        .filter(|(_, d)| !d.hostile && d.biomes.contains(&biome))
                        .map(|(i, _)| i)
                        .collect();
                    if let Some(&si) = eligible.get(((r >> 20) as usize) % eligible.len().max(1)) {
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
                        let stack = ItemStack::new(&self.reg, it, 1);
                        self.pending_gives.push((p.owner, stack));
                    } else {
                        let back = p.pos - p.vel * dt * 2.0;
                        drops.push((
                            (
                                back.x.floor() as i32,
                                back.y.floor() as i32,
                                back.z.floor() as i32,
                            ),
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
            if let Some(m) = self.mobs.get_mut(i)
                && let Some(def) = reg.animals.get(m.species)
            {
                m.hurt(def, d, from);
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
        let mut budget = [2usize, 6, 10, 14][tier];
        if self.weather == Weather::Storm && tier >= 2 {
            budget += 1; // dark skies are cover
        }
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
        let roll = |rng: &mut u32| {
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
                        (self.reg.is_solid(ground) && a1 == AIR && a2 == AIR).then_some((i, y))
                    } else if d.biomes.contains(&biome) && surface_y > SEA_LEVEL {
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
                    reg.animals
                        .get(m.species)
                        .is_some_and(|d| d.name.ends_with("wrathwood"))
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
            let Some(def) = self.reg.animals.get(m.species) else {
                continue;
            };
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
        if let Ok(text) = fs::read_to_string(self.mobs_path())
            && let Ok(f) = toml::from_str::<FileT>(&text)
        {
            for t in f.mob {
                // Unknown species (mod removed) skip cleanly.
                let Some(si) = self.reg.animal_id(&t.species) else {
                    continue;
                };
                let mut m = Mob::new(si, glam::Vec3::new(t.pos[0], t.pos[1], t.pos[2]), t.yaw);
                m.health = t.health.min(self.reg.animals[si].health);
                m.fed = t.fed;
                m.growth = t.growth.clamp(0.05, 1.0);
                self.mobs.push(m);
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
            let n = ChunkPos {
                x: pos.x + dx,
                z: pos.z + dz,
            };
            if let Some(c) = self.chunks.get_mut(&n) {
                c.dirty = true;
            }
        }
    }

    fn save_chunk(&self, pos: ChunkPos) -> std::io::Result<()> {
        let buf = self.chunk_rle(pos).unwrap_or_default();
        let mut f = fs::File::create(self.chunk_file(pos))?;
        f.write_all(&buf)
    }

    pub fn save_modified(&self) {
        if self.remote {
            return; // the host owns the world
        }
        let _ = fs::create_dir_all(&self.save_dir);
        write_world_meta_full(
            &self.save_dir,
            self.seed,
            &self.mode,
            self.ire,
            self.day,
            self.weather,
        );
        self.write_palette();
        self.save_entities();
        self.save_mobs();
        self.save_stamps();
        for (pos, chunk) in &self.chunks {
            if chunk.modified {
                let _ = self.save_chunk(*pos);
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
                *cell = map
                    .get(*cell as usize)
                    .copied()
                    .unwrap_or(self.reg.unknown_block)
                    .0;
            }
            chunk.dirty = true;
        }
        self.load_remap = self.read_palette_remap();
    }

    // ---------------- lighting ----------------

    /// (block-light intensity, sky light) at a world position. Unloaded chunks
    /// read as open sky so the world's edge doesn't render black.
    pub fn light_at(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        if y < 0 {
            return (0, 0);
        }
        if y >= CHUNK_Y as i32 {
            return (0, 15);
        }
        match self.chunks.get(&ChunkPos::of_world(x, z)) {
            Some(c) => c.light_intensity(
                x.rem_euclid(CHUNK_X as i32) as usize,
                y as usize,
                z.rem_euclid(CHUNK_Z as i32) as usize,
            ),
            None => (0, 15),
        }
    }

    /// (block-light r,g,b, sky light) at a world position — the full colored
    /// signal the mesher bakes into vertices.
    pub fn light_rgb_at(&self, x: i32, y: i32, z: i32) -> ([u8; 3], u8) {
        if y < 0 {
            return ([0; 3], 0);
        }
        if y >= CHUNK_Y as i32 {
            return ([0; 3], 15);
        }
        match self.chunks.get(&ChunkPos::of_world(x, z)) {
            Some(c) => c.light(
                x.rem_euclid(CHUNK_X as i32) as usize,
                y as usize,
                z.rem_euclid(CHUNK_Z as i32) as usize,
            ),
            None => ([0; 3], 15),
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
        let Some(chunk) = self.chunks.get(&pos) else {
            return false;
        };

        // Per-cell properties, resolved once.
        #[derive(Clone, Copy)]
        struct Cell {
            opaque: bool,
            cost: u8,          // propagation cost: 1, or 2 through water
            emit: [u8; 3],     // per-channel emission
            filter: [bool; 3], // stained glass gates channels
        }
        let mut cells = vec![
            Cell {
                opaque: false,
                cost: 1,
                emit: [0; 3],
                filter: [true; 3],
            };
            NX * NY * NZ
        ];
        for x in 0..NX {
            for z in 0..NZ {
                for y in 0..NY {
                    let d = reg.block(chunk.get(x, y, z));
                    cells[idx(x, y, z)] = Cell {
                        opaque: d.opaque,
                        cost: if d.water_level.is_some() { 2 } else { 1 },
                        emit: d.light_rgb,
                        filter: d.light_filter,
                    };
                }
            }
        }

        let mut ls = vec![0u8; NX * NY * NZ];
        let mut sky_q: VecDeque<(usize, usize, usize)> = VecDeque::new();

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
        // Border seeds from loaded neighbors (light crosses chunk seams).
        // `chan` selects the channel: None = sky, Some(c) = block channel c.
        let seed = |grid: &mut [u8],
                    q: &mut VecDeque<(usize, usize, usize)>,
                    cells: &[Cell],
                    chan: Option<usize>| {
            for (dx, dz, edge_x, edge_z) in [
                (-1i32, 0i32, 0usize, usize::MAX),
                (1, 0, NX - 1, usize::MAX),
                (0, -1, usize::MAX, 0usize),
                (0, 1, usize::MAX, NZ - 1),
            ] {
                let npos = ChunkPos {
                    x: pos.x + dx,
                    z: pos.z + dz,
                };
                let Some(nc) = self.chunks.get(&npos) else {
                    continue;
                };
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
                        let v = match chan {
                            None => nls,
                            Some(c) => nlb[c],
                        };
                        if v < 2 {
                            continue;
                        }
                        let c = cells[idx(ox, y, oz)];
                        if c.opaque || chan.is_some_and(|ch| !c.filter[ch]) {
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

        // BFS relax (single channel; run once per light channel).
        let bfs = |grid: &mut [u8],
                   q: &mut VecDeque<(usize, usize, usize)>,
                   cells: &[Cell],
                   chan: Option<usize>| {
            while let Some((x, y, z)) = q.pop_front() {
                let v = grid[idx(x, y, z)];
                if v < 2 {
                    continue;
                }
                let mut relax = |nx: usize, ny: usize, nz: usize| {
                    let c = cells[idx(nx, ny, nz)];
                    // Stained glass is opaque to the channels it blocks.
                    if c.opaque || chan.is_some_and(|ch| !c.filter[ch]) {
                        return;
                    }
                    let nv = v.saturating_sub(c.cost);
                    if nv > grid[idx(nx, ny, nz)] {
                        grid[idx(nx, ny, nz)] = nv;
                        q.push_back((nx, ny, nz));
                    }
                };
                if x > 0 {
                    relax(x - 1, y, z);
                }
                if x < NX - 1 {
                    relax(x + 1, y, z);
                }
                if y > 0 {
                    relax(x, y - 1, z);
                }
                if y < NY - 1 {
                    relax(x, y + 1, z);
                }
                if z > 0 {
                    relax(x, y, z - 1);
                }
                if z < NZ - 1 {
                    relax(x, y, z + 1);
                }
            }
        };

        // Sky: one channel.
        seed(&mut ls, &mut sky_q, &cells, None);
        bfs(&mut ls, &mut sky_q, &cells, None);

        // Block light: independent flood per color channel, packed into rgb.
        let mut lb = vec![[0u8; 3]; NX * NY * NZ];
        // `ch` selects a lane of per-cell arrays and parameterizes seed();
        // there is no slice to enumerate here.
        #[allow(clippy::needless_range_loop)]
        for ch in 0..3 {
            let mut grid = vec![0u8; NX * NY * NZ];
            let mut q: VecDeque<(usize, usize, usize)> = VecDeque::new();
            for i in 0..cells.len() {
                let e = cells[i].emit[ch];
                if e > 0 {
                    grid[i] = e;
                    let y = i % NY;
                    let xz = i / NY;
                    q.push_back((xz / NZ, y, xz % NZ));
                }
            }
            seed(&mut grid, &mut q, &cells, Some(ch));
            bfs(&mut grid, &mut q, &cells, Some(ch));
            for i in 0..grid.len() {
                lb[i][ch] = grid[i];
            }
        }

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
                    let n = ChunkPos {
                        x: p.x + dx,
                        z: p.z + dz,
                    };
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
            let np = ChunkPos {
                x: pos.x + dx,
                z: pos.z + dz,
            };
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
        // Gravity blocks detach when their support vanishes — and a
        // gravity block placed over nothing starts falling immediately.
        // Guests never simulate this; the host's BlockSet echoes land.
        if !self.remote {
            if !self.reg.is_solid(b) && y + 1 < CHUNK_Y as i32 {
                let above = self.get_block(x, y + 1, z);
                if self.reg.block(above).falls {
                    self.detach(x, y + 1, z, above);
                }
            }
            if self.reg.block(b).falls && y > 0 && !self.reg.is_solid(self.get_block(x, y - 1, z)) {
                self.detach(x, y, z, b);
            }
        }
        // Cross blocks (torches, plants) and thin slabs (snow layers)
        // pop off when their support vanishes.
        if !self.reg.is_solid(b) && y + 1 < CHUNK_Y as i32 {
            let above = self.get_block(x, y + 1, z);
            let ad = self.reg.block(above);
            if above != AIR && (ad.cross || ad.height.is_some()) {
                if let Some((item, n)) = self.reg.block(above).drops {
                    let reg = self.reg.clone();
                    self.pending_drops
                        .push(((x, y + 1, z), ItemStack::new(&reg, item, n)));
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
                BlockEntity::Bloomery(b) => b.charge.into_iter().chain(b.fuel).flatten().collect(),
                BlockEntity::Clamp(_) => Vec::new(), // the burn dies with it
                BlockEntity::Anvil(a) => a.bloom.into_iter().collect(),
                BlockEntity::Kiln(k) => k
                    .sand
                    .into_iter()
                    .chain(k.fuel)
                    .chain([k.powder])
                    .flatten()
                    .collect(),
            };
            for s in spilled {
                self.pending_drops.push(((x, y, z), s));
            }
        }
    }

    /// Random ticks: crops advance a stage when conditions hold.
    /// Random ticks at constant cost: visit the K oldest-stamped
    /// chunks with a sample burst scaled by how long each waited —
    /// one mechanism for "far corner of a big view distance" and
    /// "just came back". Returns samples drawn (for tests).
    pub fn random_tick(&mut self, rng: &mut u32) -> usize {
        const K: usize = 64;
        let reg = self.reg.clone();
        let farmland = reg.block_id("base:farmland");
        let ice = reg.block_id("base:ice");
        let snow_layer = reg.block_id("base:snow_layer");
        let snow_trod = reg.block_id("base:snow_layer_trod");
        let season = self.season();
        let mut order: Vec<(f64, ChunkPos)> = self
            .chunks
            .keys()
            .map(|p| {
                (
                    self.last_random
                        .get(&(p.x, p.z))
                        .copied()
                        .unwrap_or(self.clock),
                    *p,
                )
            })
            .collect();
        order.sort_unstable_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then((a.1.x, a.1.z).cmp(&(b.1.x, b.1.z)))
        });
        order.truncate(K);
        let mut samples = 0;
        let mut changes = Vec::new();
        let mut saplings: Vec<(i32, i32, i32, String, u32)> = Vec::new();
        for (stamp, pos) in order {
            let elapsed = (self.clock - stamp).max(0.0);
            // 8 samples per half-second of waiting, floor 8, cap 256.
            let n = ((elapsed * 16.0) as usize).clamp(8, 256);
            self.last_random.insert((pos.x, pos.z), self.clock);
            samples += n;
            for _ in 0..n {
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
                    let soil_ok =
                        d.crop_any_soil || farmland == Some(self.get_block(wx, y - 1, wz));
                    // The calendar gates growth. Bushes fruit in summer
                    // and autumn; crops slow through the year and stop
                    // in winter - unless roofed and torchlit (a
                    // greenhouse, emergent from the light rules).
                    let mult = if d.crop_any_soil {
                        if season == 1 || season == 2 { 1.0 } else { 0.0 }
                    } else {
                        match season {
                            0 => 1.25,
                            1 => 1.0,
                            2 => 0.75,
                            _ => 0.0,
                        }
                    };
                    let mult = if mult == 0.0 && !d.crop_any_soil {
                        let (bl, sl) = self.light_at(wx, y, wz);
                        if sl < 15 && bl >= 10 {
                            0.5 // dark roof + torchlight
                        } else if sl == 15
                            && (1..=16)
                                .any(|dy| self.reg.block(self.get_block(wx, y + dy, wz)).glass)
                        {
                            0.75 // a glass roof is a greenhouse
                        } else {
                            0.0
                        }
                    } else {
                        mult
                    };
                    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    if soil_ok
                        && mult > 0.0
                        && ((*rng >> 8) as f32 / (1 << 24) as f32) < d.crop_chance * mult
                    {
                        changes.push((wx, y, wz, next));
                    }
                    continue;
                }
                // Winter freezes exposed still water outside the warm
                // belts; spring gives the lakes back.
                let sky_open = self.light_at(wx, y + 1, wz).1 == 15;
                if d.water_level == Some(0)
                    && season == 3
                    && sky_open
                    && self.get_block(wx, y + 1, wz) == AIR
                    && self.generator.climate(wx, wz).t < 0.35
                {
                    if let Some(ice) = ice {
                        changes.push((wx, y, wz, ice));
                    }
                    continue;
                }
                if Some(b) == ice
                    && (season == 0 || season == 1)
                    && sky_open
                    && self.generator.climate(wx, wz).t > -0.35
                {
                    changes.push((wx, y, wz, self.reg.water_block(0)));
                    continue;
                }
                // Snow layers melt under bright light or a warm season
                // (footprints melt with them).
                if Some(b) == snow_layer || Some(b) == snow_trod {
                    let (bl, _) = self.light_at(wx, y, wz);
                    let warm = season != 3 && self.generator.climate(wx, wz).t > -0.35;
                    if bl >= 12 || warm {
                        changes.push((wx, y, wz, AIR));
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
        samples
    }

    /// The last random-tick stamp for a chunk.
    #[cfg(test)]
    pub fn chunk_stamp(&self, x: i32, z: i32) -> Option<f64> {
        self.last_random.get(&(x, z)).copied()
    }

    /// A chunk returning after an absence catches up in one sweep:
    /// phase rules apply wholesale — it's winter, so the pond you
    /// left liquid is simply frozen when you arrive — and crops
    /// advance by a Poisson draw over the random ticks they missed,
    /// integrated across the seasons the absence spanned. The water
    /// cycle is deliberately not reconciled (it nets roughly zero
    /// over a season); light-gated cases judge the plot as it stands
    /// today — an accepted approximation.
    fn reconcile_chunk(&mut self, pos: ChunkPos, elapsed: f64) {
        let reg = self.reg.clone();
        let ice = reg.block_id("base:ice");
        let snow_layer = reg.block_id("base:snow_layer");
        let snow_trod = reg.block_id("base:snow_layer_trod");
        let farmland = reg.block_id("base:farmland");
        let season = self.season();
        let day_len = crate::server::DAY_LENGTH as f64;
        // Sudden wholesale phase changes want a real absence behind
        // them; short gaps stay with the gradual burst mechanism.
        let phase = elapsed >= 2.0 * day_len;
        // Expected random-tick visits per block per in-game day.
        let ticks_per_day = 16.0 * day_len / (CHUNK_X * CHUNK_Z * CHUNK_Y) as f64;
        // Deterministic per-(world, chunk, day) randomness.
        let mut r = self
            .seed
            .wrapping_mul(31)
            .wrapping_add(pos.x as u32)
            .wrapping_mul(31)
            .wrapping_add(pos.z as u32)
            .wrapping_mul(31)
            .wrapping_add(self.day);
        // Seasons of the missed days, capped at two years back —
        // beyond that the expectations saturate anyway.
        let end_day = (self.clock / day_len) as i64;
        let start_day = ((self.clock - elapsed) / day_len) as i64;
        let days: Vec<usize> = (start_day..end_day)
            .rev()
            .take(96)
            .map(|d| ((d.max(0) as u32 / SEASON_DAYS) % 4) as usize)
            .collect();

        // One pass over the chunk collects the cells the rules touch.
        let mut interesting: Vec<(i32, i32, i32, BlockId)> = Vec::new();
        if let Some(c) = self.chunks.get(&pos) {
            for lx in 0..CHUNK_X {
                for lz in 0..CHUNK_Z {
                    for y in 1..CHUNK_Y {
                        let b = c.get(lx, y, lz);
                        if b == AIR {
                            continue;
                        }
                        let d = reg.block(b);
                        if d.crop_next.is_some()
                            || d.sapling.is_some()
                            || d.water_level == Some(0)
                            || Some(b) == ice
                            || Some(b) == snow_layer
                            || Some(b) == snow_trod
                        {
                            interesting.push((
                                pos.x * CHUNK_X as i32 + lx as i32,
                                y as i32,
                                pos.z * CHUNK_Z as i32 + lz as i32,
                                b,
                            ));
                        }
                    }
                }
            }
        }

        let mut changes = Vec::new();
        let mut grow: Vec<(i32, i32, i32, u32)> = Vec::new();
        let mut refunds = 0u32;
        for (wx, y, wz, b) in interesting {
            let d = reg.block(b);
            if d.sapling.is_some() {
                let e = days.len() as f64 * ticks_per_day * 0.02;
                if poisson(e, &mut r) > 0 {
                    r = r.wrapping_mul(1664525).wrapping_add(1013904223);
                    grow.push((wx, y, wz, r));
                }
                continue;
            }
            if d.crop_next.is_some() {
                let soil_ok = d.crop_any_soil || farmland == Some(self.get_block(wx, y - 1, wz));
                if !soil_ok {
                    continue;
                }
                let mut sum = 0.0;
                for &s in &days {
                    let mult = if d.crop_any_soil {
                        if s == 1 || s == 2 { 1.0 } else { 0.0 }
                    } else {
                        match s {
                            0 => 1.25,
                            1 => 1.0,
                            2 => 0.75,
                            _ => 0.0,
                        }
                    };
                    let mult = if mult == 0.0 && !d.crop_any_soil {
                        let (bl, sl) = self.light_at(wx, y, wz);
                        if sl < 15 && bl >= 10 {
                            0.5
                        } else if sl == 15
                            && (1..=16).any(|dy| reg.block(self.get_block(wx, y + dy, wz)).glass)
                        {
                            0.75
                        } else {
                            0.0
                        }
                    } else {
                        mult
                    };
                    sum += mult;
                }
                let k = poisson(ticks_per_day * d.crop_chance as f64 * sum, &mut r);
                if k > 0 {
                    let mut cur = b;
                    for _ in 0..k {
                        match reg.block(cur).crop_next {
                            Some(n) => cur = n,
                            None => break,
                        }
                    }
                    if cur != b {
                        // Reaching the final stage refunds ire, as a
                        // live random tick would have.
                        if reg.block(cur).crop_next.is_none() {
                            refunds += 1;
                        }
                        changes.push((wx, y, wz, cur));
                    }
                }
                continue;
            }
            if !phase {
                continue;
            }
            let sky_open = self.light_at(wx, y + 1, wz).1 == 15;
            if d.water_level == Some(0)
                && season == 3
                && sky_open
                && self.get_block(wx, y + 1, wz) == AIR
                && self.generator.climate(wx, wz).t < 0.35
            {
                if let Some(ice) = ice {
                    changes.push((wx, y, wz, ice));
                }
                continue;
            }
            if Some(b) == ice
                && (season == 0 || season == 1)
                && sky_open
                && self.generator.climate(wx, wz).t > -0.35
            {
                changes.push((wx, y, wz, reg.water_block(0)));
                continue;
            }
            if Some(b) == snow_layer || Some(b) == snow_trod {
                let (bl, _) = self.light_at(wx, y, wz);
                let warm = season != 3 && self.generator.climate(wx, wz).t > -0.35;
                if bl >= 12 || warm {
                    changes.push((wx, y, wz, AIR));
                }
            }
        }
        // Batched apply: a frozen lake is many cells — one relight,
        // not one per cell.
        let any = !changes.is_empty();
        for (x, y, z, nb) in changes {
            let (lx, lz) = (
                x.rem_euclid(CHUNK_X as i32) as usize,
                z.rem_euclid(CHUNK_Z as i32) as usize,
            );
            if let Some(c) = self.chunks.get_mut(&pos) {
                c.set(lx, y as usize, lz, nb);
                c.dirty = true;
                c.modified = true;
                if self.log_edits {
                    self.edit_log.push((x, y, z, nb));
                }
            }
            self.wake_water(x, y, z);
        }
        if any {
            self.relight_and_cascade(pos);
        }
        for _ in 0..refunds {
            self.plant_ire(0.5);
        }
        for (x, y, z, rnd) in grow {
            self.try_grow_sapling(x, y, z, rnd);
        }
    }

    /// A footstep through a snow layer presses it into a trodden
    /// print — a real edit: logged, broadcast, persisted, and it melts
    /// like any layer. History written in the ground.
    pub fn tread(&mut self, x: i32, y: i32, z: i32) {
        if self.remote {
            return; // guests' prints are stamped by the host
        }
        let (Some(layer), Some(trod)) = (
            self.reg.block_id("base:snow_layer"),
            self.reg.block_id("base:snow_layer_trod"),
        ) else {
            return;
        };
        if self.get_block(x, y, z) == layer {
            self.set_block(x, y, z, trod);
        }
    }

    /// One flake of consequence: lay a snow layer on this column's
    /// surface if the storm is cold here and the sky can reach it.
    pub fn settle_snow(&mut self, x: i32, z: i32) {
        if !self.snows_at(x, z) {
            return;
        }
        let Some(layer) = self.reg.block_id("base:snow_layer") else {
            return;
        };
        let y = self.surface_height(x, z);
        if y <= SEA_LEVEL || y + 1 >= CHUNK_Y as i32 - 1 {
            return;
        }
        if self.get_block(x, y + 1, z) != AIR || self.light_at(x, y + 1, z).1 != 15 {
            return;
        }
        self.set_block(x, y + 1, z, layer);
    }

    /// Attempt to mature the sapling at this position. On success the
    /// tree is built and the wild refunds -2 ire, bypassing the daily
    /// planting cap (it took days — it IS the slow path).
    pub fn try_grow_sapling(&mut self, x: i32, y: i32, z: i32, rnd: u32) -> bool {
        let b = self.get_block(x, y, z);
        let Some(species) = self.reg.block(b).sapling.clone() else {
            return false;
        };
        if self.grow_tree(x, y, z, &species, rnd) {
            self.add_ire(-2.0);
            true
        } else {
            false
        }
    }

    /// Advance machines. Returns true if any visible state changed.
    /// Fire every lit bloomery: validate the shell, let the weather
    /// slow or douse an unroofed stack, and cash the batch when done.
    fn tick_bloomeries(&mut self, dt: f32) {
        let keys: Vec<(i32, i32, i32)> = self
            .block_entities
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Bloomery(b) if b.lit))
            .map(|(k, _)| *k)
            .collect();
        for pos in keys {
            let Some(BlockEntity::Bloomery(mut b)) = self.block_entities.remove(&pos) else {
                continue;
            };
            let (x, y, z) = pos;
            if self.check_bloomery(x, y, z).is_none() {
                // Breached mid-fire: the heat escapes, the charge survives.
                b.lit = false;
                b.progress = 0.0;
                self.swap_block_keep_entity(x, y, z, "base:bloomery");
                self.block_entities.insert(pos, BlockEntity::Bloomery(b));
                continue;
            }
            // An unroofed stack fights the rain and loses to a storm.
            let unroofed = self.light_at(b.core.0, y + 3, b.core.2).1 == 15;
            let wet = self.weather.precipitating() && self.rains_at(x, z) && unroofed;
            if wet && self.weather == Weather::Storm {
                b.lit = false;
                b.progress = 0.0;
                self.swap_block_keep_entity(x, y, z, "base:bloomery");
                self.block_entities.insert(pos, BlockEntity::Bloomery(b));
                continue;
            }
            b.progress += dt * if wet { 0.5 } else { 1.0 };
            if b.progress >= BLOOMERY_FIRE_SECS {
                // Cash the batch: 2 charge + 2 fuel per bloom, +2 bonus
                // blooms on a full 8+8 firing.
                let chain = self.reg.bloomery.first().cloned();
                if let Some(chain) = chain {
                    let n_charge: u32 = b.charge.iter().flatten().map(|s| s.count).sum();
                    let n_fuel: u32 = b.fuel.iter().flatten().map(|s| s.count).sum();
                    let units = n_charge.min(n_fuel) / 2;
                    let blooms = units + if units >= 4 { 2 } else { 0 };
                    let eat = |slots: &mut [Option<ItemStack>; 4], mut n: u32| {
                        for s in slots.iter_mut() {
                            if n == 0 {
                                break;
                            }
                            if let Some(st) = s {
                                let take = st.count.min(n);
                                n -= take;
                                st.count -= take;
                                if st.count == 0 {
                                    *s = None;
                                }
                            }
                        }
                    };
                    eat(&mut b.charge, units * 2);
                    eat(&mut b.fuel, units * 2);
                    let reg = self.reg.clone();
                    let mut out = ItemStack::new(&reg, chain.bloom, blooms.max(1));
                    out.count = blooms.max(1);
                    // Blooms land in the first empty charge slot.
                    for s in b.charge.iter_mut() {
                        if s.is_none() {
                            *s = Some(out);
                            break;
                        }
                    }
                }
                b.lit = false;
                b.progress = 0.0;
                self.swap_block_keep_entity(x, y, z, "base:bloomery");
            }
            self.block_entities.insert(pos, BlockEntity::Bloomery(b));
        }
    }

    /// Smolder every clamp; venting burns the exposed log away.
    fn tick_clamps(&mut self, dt: f32) {
        let keys: Vec<(i32, i32, i32)> = self
            .block_entities
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Clamp(_)))
            .map(|(k, _)| *k)
            .collect();
        let logs_tag = self.reg.tags.get("base:logs").cloned().unwrap_or_default();
        for pos in keys {
            let Some(BlockEntity::Clamp(mut c)) = self.block_entities.remove(&pos) else {
                continue;
            };
            // Logs that stopped being logs (mined) leave the pile.
            c.logs.retain(|&(x, y, z)| {
                let b = self.get_block(x, y, z);
                self.reg
                    .item_id(&self.reg.block(b).name)
                    .is_some_and(|i| logs_tag.contains(&i))
            });
            // A newly exposed log burns to nothing.
            let mut vented: Option<(i32, i32, i32)> = None;
            let mut exposed = 0;
            'scan: for p in &c.logs {
                for d in [
                    (1, 0, 0),
                    (-1, 0, 0),
                    (0, 1, 0),
                    (0, -1, 0),
                    (0, 0, 1),
                    (0, 0, -1),
                ] {
                    let n = (p.0 + d.0, p.1 + d.1, p.2 + d.2);
                    if c.logs.contains(&n) {
                        continue;
                    }
                    if !self.reg.is_solid(self.get_block(n.0, n.1, n.2)) {
                        exposed += 1;
                        if exposed > 1 {
                            vented = Some(*p);
                            break 'scan;
                        }
                    }
                }
            }
            if let Some(p) = vented {
                self.set_block(p.0, p.1, p.2, AIR);
                c.logs.retain(|l| *l != p);
                c.timer -= CLAMP_SECS_PER_LOG;
            }
            if c.logs.is_empty() {
                continue; // the pile is gone; so is the burn
            }
            c.timer -= dt;
            if c.timer <= 0.0 {
                if let Some(cc) = self.reg.block_id("base:charcoal_block") {
                    for p in c.logs.clone() {
                        self.set_block(p.0, p.1, p.2, cc);
                    }
                }
                continue; // done; entity retires
            }
            self.block_entities.insert(pos, BlockEntity::Clamp(c));
        }
    }

    pub fn tick_entities(&mut self, dt: f32) {
        self.tick_bloomeries(dt);
        self.tick_kilns(dt);
        self.tick_clamps(dt);
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
                if let Some(fs) = f.fuel
                    && let Some((burn, speed)) = reg.fuel_value(fs.item)
                {
                    f.burn_left = burn;
                    f.burn_total = burn;
                    f.burn_speed = speed;
                    let left = fs.count - 1;
                    f.fuel = if left > 0 {
                        Some(ItemStack { count: left, ..fs })
                    } else {
                        None
                    };
                    self.ire = (self.ire + 0.1).min(100.0);
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
                            f.input = if left > 0 {
                                Some(ItemStack { count: left, ..inp })
                            } else {
                                None
                            };
                        }
                        f.output = Some(match f.output {
                            Some(o) => ItemStack {
                                count: o.count + 1,
                                ..o
                            },
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
                                self.reg.item(s.item).name,
                                s.count,
                                s.durability
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
                                self.reg.item(st.item).name,
                                st.count,
                                st.durability
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
                                self.reg.item(st.item).name,
                                st.count,
                                st.durability
                            );
                        }
                    }
                    let _ = writeln!(out);
                }
                BlockEntity::Bloomery(b) => {
                    let _ = writeln!(
                        out,
                        "[[bloomery]]\npos = [{x}, {y}, {z}]\nlit = {}\nprogress = {}\ncore = [{}, {}, {}]",
                        b.lit, b.progress, b.core.0, b.core.1, b.core.2
                    );
                    for (i, st) in b.charge.iter().chain(b.fuel.iter()).enumerate() {
                        if let Some(st) = st {
                            let _ = writeln!(
                                out,
                                "[[bloomery.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}",
                                self.reg.item(st.item).name,
                                st.count,
                                st.durability
                            );
                        }
                    }
                    let _ = writeln!(out);
                }
                BlockEntity::Clamp(c) => {
                    let logs: Vec<String> = c
                        .logs
                        .iter()
                        .map(|(a, b2, c2)| format!("[{a}, {b2}, {c2}]"))
                        .collect();
                    let _ = writeln!(
                        out,
                        "[[clamp]]\npos = [{x}, {y}, {z}]\ntimer = {}\nlogs = [{}]\n",
                        c.timer,
                        logs.join(", ")
                    );
                }
                BlockEntity::Kiln(k) => {
                    let _ = writeln!(
                        out,
                        "[[kiln]]\npos = [{x}, {y}, {z}]\nlit = {}\nprogress = {}\ncore = [{}, {}, {}]",
                        k.lit, k.progress, k.core.0, k.core.1, k.core.2
                    );
                    let all: Vec<&Option<ItemStack>> = k
                        .sand
                        .iter()
                        .chain([&k.powder])
                        .chain(k.fuel.iter())
                        .collect();
                    for (i, st) in all.into_iter().enumerate() {
                        if let Some(st) = st {
                            let _ = writeln!(
                                out,
                                "[[kiln.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}",
                                self.reg.item(st.item).name,
                                st.count,
                                st.durability
                            );
                        }
                    }
                    let _ = writeln!(out);
                }
                BlockEntity::Anvil(a) => {
                    let _ = writeln!(
                        out,
                        "[[anvil]]\npos = [{x}, {y}, {z}]\nstrikes = {}",
                        a.strikes
                    );
                    if let Some(st) = &a.bloom {
                        let _ = writeln!(
                            out,
                            "bloom = {{ item = \"{}\", count = {}, durability = {} }}",
                            self.reg.item(st.item).name,
                            st.count,
                            st.durability
                        );
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
        struct BloomeryT {
            pos: [i32; 3],
            #[serde(default)]
            lit: bool,
            #[serde(default)]
            progress: f32,
            #[serde(default)]
            core: Option<[i32; 3]>,
            #[serde(default)]
            slot: Vec<ChestSlotT>,
        }
        #[derive(Deserialize)]
        struct ClampT {
            pos: [i32; 3],
            timer: f32,
            #[serde(default)]
            logs: Vec<[i32; 3]>,
        }
        #[derive(Deserialize)]
        struct AnvilT {
            pos: [i32; 3],
            #[serde(default)]
            strikes: u32,
            #[serde(default)]
            bloom: Option<SlotT>,
        }
        #[derive(Deserialize)]
        struct FileT {
            #[serde(default)]
            furnace: Vec<FurnaceT>,
            #[serde(default)]
            chest: Vec<ChestT>,
            #[serde(default)]
            offering: Vec<ChestT>,
            #[serde(default)]
            bloomery: Vec<BloomeryT>,
            #[serde(default)]
            clamp: Vec<ClampT>,
            #[serde(default)]
            anvil: Vec<AnvilT>,
            #[serde(default)]
            kiln: Vec<BloomeryT>,
        }
        let Ok(text) = fs::read_to_string(self.entities_path()) else {
            return;
        };
        let Ok(parsed) = toml::from_str::<FileT>(&text) else {
            return;
        };
        let conv = |s: Option<SlotT>| -> Option<ItemStack> {
            let s = s?;
            let item = self.reg.item_id(&s.item)?;
            Some(ItemStack {
                item,
                count: s.count,
                durability: s.durability,
            })
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
            let mut state = ChestState {
                wild_owned: ch.wild_owned,
                ..Default::default()
            };
            for sl in ch.slot {
                if sl.index < CHEST_SLOTS
                    && let Some(item) = self.reg.item_id(&sl.item)
                {
                    state.slots[sl.index] = Some(ItemStack {
                        item,
                        count: sl.count,
                        durability: sl.durability,
                    });
                }
            }
            self.block_entities
                .insert((ch.pos[0], ch.pos[1], ch.pos[2]), BlockEntity::Chest(state));
        }
        for of in parsed.offering {
            let mut state = OfferingState::default();
            for sl in of.slot {
                if sl.index < 3
                    && let Some(item) = self.reg.item_id(&sl.item)
                {
                    state.slots[sl.index] = Some(ItemStack {
                        item,
                        count: sl.count,
                        durability: sl.durability,
                    });
                }
            }
            self.block_entities.insert(
                (of.pos[0], of.pos[1], of.pos[2]),
                BlockEntity::Offering(state),
            );
        }
        for bl in parsed.bloomery {
            let mut state = BloomeryState {
                lit: bl.lit,
                progress: bl.progress,
                core: bl.core.map(|c| (c[0], c[1], c[2])).unwrap_or_default(),
                ..Default::default()
            };
            for sl in bl.slot {
                if sl.index < 8
                    && let Some(item) = self.reg.item_id(&sl.item)
                {
                    let st = Some(ItemStack {
                        item,
                        count: sl.count,
                        durability: sl.durability,
                    });
                    if sl.index < 4 {
                        state.charge[sl.index] = st;
                    } else {
                        state.fuel[sl.index - 4] = st;
                    }
                }
            }
            self.block_entities.insert(
                (bl.pos[0], bl.pos[1], bl.pos[2]),
                BlockEntity::Bloomery(state),
            );
        }
        for cl in parsed.clamp {
            self.block_entities.insert(
                (cl.pos[0], cl.pos[1], cl.pos[2]),
                BlockEntity::Clamp(ClampState {
                    logs: cl.logs.iter().map(|l| (l[0], l[1], l[2])).collect(),
                    timer: cl.timer,
                }),
            );
        }
        for kl in parsed.kiln {
            let mut state = KilnState {
                lit: kl.lit,
                progress: kl.progress,
                core: kl.core.map(|c| (c[0], c[1], c[2])).unwrap_or_default(),
                ..Default::default()
            };
            for sl in kl.slot {
                if sl.index < 9
                    && let Some(item) = self.reg.item_id(&sl.item)
                {
                    let st = Some(ItemStack {
                        item,
                        count: sl.count,
                        durability: sl.durability,
                    });
                    match sl.index {
                        0..=3 => state.sand[sl.index] = st,
                        4 => state.powder = st,
                        _ => state.fuel[sl.index - 5] = st,
                    }
                }
            }
            self.block_entities
                .insert((kl.pos[0], kl.pos[1], kl.pos[2]), BlockEntity::Kiln(state));
        }
        for an in parsed.anvil {
            self.block_entities.insert(
                (an.pos[0], an.pos[1], an.pos[2]),
                BlockEntity::Anvil(AnvilState {
                    bloom: conv(an.bloom),
                    strikes: an.strikes,
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
        for (dx, dy, dz) in [
            (0, 0, 0),
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ] {
            self.schedule_water(x + dx, y + dy, z + dz);
        }
    }

    /// Wake water across a fresh chunk's seams: flow deferred at the
    /// edge of the generated world resumes here. Only genuine
    /// differentials queue — a flat ocean seam schedules nothing.
    fn wake_seams(&mut self, pos: ChunkPos) {
        let mut wake = Vec::new();
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let np = ChunkPos {
                x: pos.x + dx,
                z: pos.z + dz,
            };
            if !self.chunks.contains_key(&np) {
                continue;
            }
            let n = if dx != 0 { CHUNK_Z } else { CHUNK_X } as i32;
            for i in 0..n {
                // World coords of the facing cells on each side.
                let (ax, az) = if dx != 0 {
                    (
                        pos.x * CHUNK_X as i32 + if dx == 1 { CHUNK_X as i32 - 1 } else { 0 },
                        pos.z * CHUNK_Z as i32 + i,
                    )
                } else {
                    (
                        pos.x * CHUNK_X as i32 + i,
                        pos.z * CHUNK_Z as i32 + if dz == 1 { CHUNK_Z as i32 - 1 } else { 0 },
                    )
                };
                let (bx, bz) = (ax + dx, az + dz);
                for y in 1..CHUNK_Y as i32 {
                    if let (Some(a), Some(b)) = (
                        self.flow_potential(ax, y, az),
                        self.flow_potential(bx, y, bz),
                    ) && a.abs_diff(b) >= 2
                    {
                        wake.push(if a > b { (ax, y, az) } else { (bx, y, bz) });
                    }
                }
            }
        }
        for (x, y, z) in wake {
            self.schedule_water(x, y, z);
        }
    }

    /// Volume for flow comparisons: water carries its units, air can
    /// receive (0), anything else opts out of flow entirely.
    fn flow_potential(&self, x: i32, y: i32, z: i32) -> Option<u8> {
        let b = self.get_block(x, y, z);
        if self.reg.is_air(b) {
            Some(0)
        } else {
            self.reg.water_volume(b)
        }
    }

    /// Finite water (docs/water-and-ticks-plan.md): each level encodes
    /// volume — level 0 is 8 units, level 7 a 1-unit film. On wake a
    /// cell falls as far as it can, then equalizes toward its lowest
    /// horizontal neighbor with a 2-unit hysteresis so the queue always
    /// quiesces. Volume moves; it is never created or destroyed. Flow
    /// toward ungenerated chunks defers (set_block there silently
    /// drops the write) — `wake_seams` resumes it when the neighbor
    /// generates.
    pub fn tick_water(&mut self, budget: usize) -> bool {
        let mut changed = false;
        for _ in 0..budget {
            let Some(pos) = self.water_queue.pop_front() else {
                break;
            };
            self.water_queued.remove(&pos);
            let (x, y, z) = pos;
            let Some(v) = self.reg.water_volume(self.get_block(x, y, z)) else {
                continue;
            };
            // Fall first, greedily (below is always in our own chunk).
            if y > 0
                && let Some(nv) = self.flow_potential(x, y - 1, z)
                && nv < 8
            {
                let t = v.min(8 - nv);
                self.set_block(x, y - 1, z, self.reg.water_for_volume(nv + t));
                self.set_block(x, y, z, self.reg.water_for_volume(v - t));
                changed = true;
                continue;
            }
            // Equalize toward the lowest loaded horizontal neighbor.
            let mut best: Option<(i32, i32, u8)> = None;
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, nz) = (x + dx, z + dz);
                if !self.chunks.contains_key(&ChunkPos::of_world(nx, nz)) {
                    continue; // the world's edge: defer, don't spill
                }
                if let Some(nv) = self.flow_potential(nx, y, nz)
                    && best.is_none_or(|(_, _, b)| nv < b)
                {
                    best = Some((nx, nz, nv));
                }
            }
            if let Some((nx, nz, nv)) = best
                && v >= nv + 2
            {
                let t = ((v - nv) / 2).max(1);
                self.set_block(nx, y, nz, self.reg.water_for_volume(nv + t));
                self.set_block(x, y, z, self.reg.water_for_volume(v - t));
                changed = true;
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
