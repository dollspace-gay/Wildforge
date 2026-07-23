//! World: chunk map with block access, fluid simulation, and versioned
//! persistence (save v2 with a per-world id palette; legacy v1 migrates).

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use glam::Vec3;

use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos, SEA_LEVEL};
use crate::inventory::ItemStack;
use crate::mobs::{Mob, MobEvent, ProjHit, Projectile};
use crate::registry::{AIR, BlockId, ItemId, Registry};
use crate::worldgen::Generator;

mod calendar;
mod chunks;
mod ecology;
mod entities;
mod fluids;
mod lighting;
mod machine_tick;
mod machines;
mod persistence;
mod storage;
mod substrate;
mod ticks;

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
    chunks: HashMap<ChunkPos, Chunk>,
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
    block_entities: HashMap<(i32, i32, i32), BlockEntity>,
    /// Items spilled by removed block entities, for the game loop to spawn.
    pending_drops: Vec<((i32, i32, i32), ItemStack)>,
    mobs: Vec<crate::mobs::Mob>,
    projectiles: Vec<Projectile>,
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
    remote: bool,
    /// Calendar day (increments at dawn, natural or slept-through).
    pub day: u32,
    /// Current weather + seconds remaining on it (the Server's machine
    /// drives this; it lives here so world.toml persistence is natural).
    pub weather: Weather,
    pub weather_timer: f32,
    /// Host mode: record block edits for broadcasting.
    log_edits: bool,
    edit_log: Vec<(i32, i32, i32, BlockId, u8)>,
    /// Gravity blocks currently airborne.
    falling: Vec<FallingBlock>,
    /// (guest id, stack) owed over the wire: mining drops, kill loot,
    /// recovered arrows, brush finds — full stacks so durability rides.
    pending_gives: Vec<(u32, ItemStack)>,
    /// Next stable mob id (host side; ids exist for the wire).
    next_mob_id: u32,
}

/// Ire tier names, index = tier.
pub const IRE_TIERS: [&str; 4] = ["CALM", "UNEASY", "PROVOKED", "WRATHFUL"];

/// Result of one authoritative block break. Presentation decides how a local
/// drop is animated; guest drops are queued directly by the host adapter.
pub struct BlockBreak {
    pub block: BlockId,
    pub drop: Option<ItemStack>,
}

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

    /// Switch between authoritative storage and guest snapshot mode.
    pub fn set_remote(&mut self, remote: bool) {
        self.remote = remote;
    }

    pub fn is_remote(&self) -> bool {
        self.remote
    }

    /// Enable or disable the authoritative block-edit journal.
    pub fn set_edit_logging(&mut self, enabled: bool) {
        self.log_edits = enabled;
    }

    pub fn edits(&self) -> &[(i32, i32, i32, BlockId, u8)] {
        &self.edit_log
    }

    pub fn take_edits(&mut self) -> Vec<(i32, i32, i32, BlockId, u8)> {
        std::mem::take(&mut self.edit_log)
    }

    pub fn queue_give(&mut self, owner: u32, stack: ItemStack) {
        self.pending_gives.push((owner, stack));
    }

    pub fn take_pending_gives(&mut self) -> Vec<(u32, ItemStack)> {
        std::mem::take(&mut self.pending_gives)
    }

    #[cfg(test)]
    pub(crate) fn pending_drops(&self) -> &[((i32, i32, i32), ItemStack)] {
        &self.pending_drops
    }

    pub fn clear_pending_drops(&mut self) {
        self.pending_drops.clear();
    }

    pub fn take_pending_drops(&mut self) -> Vec<((i32, i32, i32), ItemStack)> {
        std::mem::take(&mut self.pending_drops)
    }

    pub fn block_entity(&self, pos: &(i32, i32, i32)) -> Option<&BlockEntity> {
        self.block_entities.get(pos)
    }

    pub fn block_entity_mut(&mut self, pos: &(i32, i32, i32)) -> Option<&mut BlockEntity> {
        self.block_entities.get_mut(pos)
    }

    pub fn insert_block_entity(
        &mut self,
        pos: (i32, i32, i32),
        entity: BlockEntity,
    ) -> Option<BlockEntity> {
        self.block_entities.insert(pos, entity)
    }

    pub fn ensure_block_entity(
        &mut self,
        pos: (i32, i32, i32),
        default: BlockEntity,
    ) -> &mut BlockEntity {
        self.block_entities.entry(pos).or_insert(default)
    }

    pub fn has_block_entity(&self, pos: &(i32, i32, i32)) -> bool {
        self.block_entities.contains_key(pos)
    }

    pub fn block_entities(&self) -> impl Iterator<Item = (&(i32, i32, i32), &BlockEntity)> {
        self.block_entities.iter()
    }

    pub fn has_chunk(&self, pos: ChunkPos) -> bool {
        self.chunks.contains_key(&pos)
    }

    pub fn chunk(&self, pos: ChunkPos) -> Option<&Chunk> {
        self.chunks.get(&pos)
    }

    pub fn mark_chunk_dirty(&mut self, pos: ChunkPos) {
        if let Some(chunk) = self.chunks.get_mut(&pos) {
            chunk.dirty = true;
        }
    }

    pub fn chunks_outside(&self, center: ChunkPos, radius: i32) -> Vec<ChunkPos> {
        self.chunks
            .keys()
            .filter(|pos| (pos.x - center.x).abs() > radius || (pos.z - center.z).abs() > radius)
            .copied()
            .collect()
    }

    pub fn unload_chunk(&mut self, pos: ChunkPos) {
        self.chunks.remove(&pos);
    }

    pub fn dirty_chunks(&self) -> Vec<ChunkPos> {
        self.chunks
            .iter()
            .filter_map(|(pos, chunk)| chunk.dirty.then_some(*pos))
            .collect()
    }

    pub fn mark_chunk_meshed(&mut self, pos: ChunkPos) {
        if let Some(chunk) = self.chunks.get_mut(&pos) {
            chunk.dirty = false;
        }
    }

    #[cfg(test)]
    pub(crate) fn chunks(&self) -> &HashMap<ChunkPos, Chunk> {
        &self.chunks
    }

    #[cfg(test)]
    pub(crate) fn chunks_mut(&mut self) -> &mut HashMap<ChunkPos, Chunk> {
        &mut self.chunks
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

    /// Metadata byte at a world position (octant mask for sub-voxel blocks).
    pub fn get_meta(&self, x: i32, y: i32, z: i32) -> u8 {
        if y < 0 || y >= CHUNK_Y as i32 {
            return 0;
        }
        let pos = ChunkPos::of_world(x, z);
        match self.chunks.get(&pos) {
            Some(chunk) => chunk.meta(
                x.rem_euclid(CHUNK_X as i32) as usize,
                y as usize,
                z.rem_euclid(CHUNK_Z as i32) as usize,
            ),
            None => 0,
        }
    }

    pub fn break_block(
        &mut self,
        pos: (i32, i32, i32),
        tool: Option<ItemId>,
        award_drop: bool,
        affect_ire: bool,
    ) -> Option<BlockBreak> {
        let block = self.get_block(pos.0, pos.1, pos.2);
        if block == AIR || self.reg.block(block).hardness.is_none() {
            return None;
        }
        let drop = award_drop
            .then(|| self.reg.drops_for(block, tool))
            .flatten()
            .map(|(item, count)| ItemStack::new(&self.reg, item, count));
        if affect_ire {
            let cost = self.ire_for_block(block);
            self.add_ire(cost);
        }
        self.set_block(pos.0, pos.1, pos.2, AIR);
        Some(BlockBreak { block, drop })
    }

    pub fn place_block(&mut self, pos: (i32, i32, i32), block: BlockId) -> bool {
        if self.reg.blocks.get(block.0 as usize).is_none()
            || self.get_block(pos.0, pos.1, pos.2) != AIR
        {
            return false;
        }
        self.set_block(pos.0, pos.1, pos.2, block);
        true
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, b: BlockId) {
        let meta = if self.reg.block(b).sub_voxel { 0xff } else { 0 };
        self.set_block_meta(x, y, z, b, meta);
    }

    /// Set a block with an explicit metadata byte.
    pub fn set_block_meta(&mut self, x: i32, y: i32, z: i32, b: BlockId, meta: u8) {
        if y < 0 || y >= CHUNK_Y as i32 {
            return;
        }
        let pos = ChunkPos::of_world(x, z);
        let lx = x.rem_euclid(CHUNK_X as i32) as usize;
        let lz = z.rem_euclid(CHUNK_Z as i32) as usize;
        if let Some(c) = self.chunks.get_mut(&pos) {
            c.set(lx, y as usize, lz, b);
            c.set_meta(lx, y as usize, lz, meta);
            c.dirty = true;
            c.modified = true;
            if self.log_edits {
                self.edit_log.push((x, y, z, b, meta));
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
