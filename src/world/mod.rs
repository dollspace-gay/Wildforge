//! World: chunk map with block access, fluid simulation, and versioned
//! persistence (save v2 with a per-world id palette; legacy v1 migrates).

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(test)]
use glam::Vec3;

use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos, SEA_LEVEL};
use crate::inventory::ItemStack;
use crate::mobs::{Mob, MobEvent, ProjHit, Projectile};
use crate::planet::BlockPos;
use crate::registry::{AIR, BlockId, ItemId, Registry};
use crate::worldgen::Generator;

mod calendar;
mod chunks;
mod ecology;
mod entities;
mod fire;
mod fluids;
mod hearts;
mod lighting;
mod machine_tick;
mod machines;
pub(crate) mod multiblock;
mod persistence;
mod power;
#[cfg_attr(test, allow(unused))]
pub(crate) mod region;
mod storage;

pub use hearts::ROOT_READY_FRAC;
#[cfg(test)]
pub use hearts::{HEART_CUTTING_DAYS, HEART_DEATH_STRAIN, HEART_SICKEN_STRAIN, ROOT_DAYS};
pub use hearts::{Heart, heart_block_name, heart_form, heart_height, seed_nature, seed_of_form};
pub use machines::{station_powered, worked_table_for};
pub mod soil;
mod spawn;
pub(crate) use spawn::player_entry_chunks;
pub(crate) use storage::{ChunkLoader, encode_stream_chunk};
mod ticks;

/// Materialized water conditions used by fish and later aquatic biomes. Depth
/// and salinity come from the live voxel column; temperature comes from local
/// weather; discharge remains the immutable atlas's broad-flow prior.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AquaticHabitat {
    pub depth_blocks: u8,
    pub temperature_c: f32,
    pub discharge: f32,
    pub salinity: u8,
}

/// One persistence component that did not reach durable storage.
#[derive(Debug)]
pub struct SaveFailure {
    pub component: String,
    pub path: PathBuf,
    pub error: std::io::Error,
}

impl SaveFailure {
    pub(super) fn new(component: impl Into<String>, path: PathBuf, error: std::io::Error) -> Self {
        Self {
            component: component.into(),
            path,
            error,
        }
    }
}

impl fmt::Display for SaveFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}): {}",
            self.component,
            self.path.display(),
            self.error
        )
    }
}

/// Complete result of one world-save attempt.
///
/// Independent components continue after a failure so operators get one
/// useful report instead of discovering errors one five-minute retry at a
/// time. Dirty chunks are cleared only for writes that landed.
#[derive(Debug, Default)]
#[must_use = "world save failures must be reported or handled"]
pub struct SaveReport {
    pub chunks_saved: usize,
    pub failures: Vec<SaveFailure>,
}

impl SaveReport {
    pub fn is_ok(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn summary(&self) -> String {
        if self.is_ok() {
            return format!("{} dirty chunks written", self.chunks_saved);
        }
        let shown = self
            .failures
            .iter()
            .take(3)
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        let more = self.failures.len().saturating_sub(3);
        if more == 0 {
            shown
        } else {
            format!("{shown}; and {more} more")
        }
    }

    pub(super) fn record(
        &mut self,
        component: impl Into<String>,
        path: PathBuf,
        result: std::io::Result<()>,
    ) -> bool {
        match result {
            Ok(()) => true,
            Err(error) => {
                self.failures.push(SaveFailure::new(component, path, error));
                false
            }
        }
    }

    pub(super) fn extend(&mut self, failures: impl IntoIterator<Item = SaveFailure>) {
        self.failures.extend(failures);
    }
}

/// Result of one residency sweep.
#[derive(Debug, Default)]
#[must_use = "residency save failures must be reported or handled"]
pub struct ResidencyReport {
    pub released: usize,
    pub retained_dirty: usize,
    pub failures: Vec<SaveFailure>,
}

impl ResidencyReport {
    pub fn is_ok(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn summary(&self) -> String {
        if self.is_ok() {
            return format!("released {} chunks", self.released);
        }
        let shown = self
            .failures
            .iter()
            .take(3)
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        let more = self.failures.len().saturating_sub(3);
        let suffix = if more == 0 {
            String::new()
        } else {
            format!("; and {more} more")
        };
        format!(
            "released {}, retained {} dirty: {shown}{suffix}",
            self.released, self.retained_dirty
        )
    }
}

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
    Forge(BloomeryState),
    /// Three short lines on a post (a waystone uses line 0 as its name).
    Sign(SignState),
    /// A market stall counter: goods, a price, and the owner's till.
    Stall(StallState),
    /// A smoking rack: raw cuts curing over a live torch.
    Smoker(SmokerState),
    /// A steam firebox: banked fire and exact boiler water.
    Steam(SteamState),
    /// A rare-earth separator: powder in, neodymium and cerium out.
    Separator(SeparatorState),
}

#[derive(Default)]
pub struct SeparatorState {
    pub powder: u32,
    pub fuel: u32,
    pub nd: u32,
    pub ce: u32,
    pub progress: f32,
}

/// The world's year stops when this many countries are dead AND
/// they are this share of every country anyone has seen.
pub const LONG_WINTER_MIN_DEAD: usize = 3;
pub const LONG_WINTER_FRAC: f32 = 0.5;

/// A cell will give this much bloom, all told, before the ground has
/// nothing left to give. Tending pays it back. A season's worth, so
/// the exploit it closes stays closed at any calendar length.
pub const BLOOM_EXHAUSTION: f32 = SEASON_DAYS as f32;

/// Freshness a food stack loses per real second, carried or stored.
/// Food ages on the wall clock while the calendar runs on DAY_LENGTH,
/// so these two have to move together or the larder silently changes
/// meaning: at half a point a second and a 1200 s day, an 1800-point
/// potato still keeps three in-game days, exactly as it did at a
/// point a second and a 600 s day. "Will this last the winter" is a
/// question about days, so it is answered in days.
pub const FRESHNESS_PER_SEC: f32 = 0.5;

/// Random-tick samples per chunk per real second: growth, spread,
/// thaw, decay. Crops are planted and waited on in DAYS, so this
/// moves opposite DAY_LENGTH — 8 a second across a 1200 s day is the
/// same 9600 visits a chunk got from 16 across a 600 s one.
pub const RANDOM_TICKS_PER_CHUNK_SEC: f64 = 8.0;

/// Seconds per separator batch (1 powder + 1 fuel -> 1 Nd + 2 Ce).
pub const SEPARATE_SECS: f32 = 45.0;
/// How far a running generator's field reaches (lamps, the quern).
pub const ELEC_RADIUS: i32 = 6;

#[derive(Default)]
pub struct SteamState {
    /// Seconds of fire banked (coal fed by hand at the door).
    pub fuel: f32,
    /// Exact boiler reservoir; evaporation leaves dissolved salt here.
    pub water: crate::planet_atlas::ReservoirMass,
    /// Numerator remainder for HU consumption at 256 HU / 15 seconds.
    pub steam_numerator_remainder: u64,
}

/// One full water cell banks this many seconds of steam.
pub const STEAM_SECS_PER_WATER: f32 = 15.0;
/// The firebox holds at most this much banked fire (seconds).
pub const STEAM_FUEL_CAP: f32 = 1800.0;
/// What a running engine delivers to its shaft line.
pub const STEAM_RATE: f32 = 1.4;

#[derive(Default)]
pub struct SmokerState {
    pub meat: [Option<ItemStack>; 4],
    pub progress: f32,
}

/// A rack-load of cuts cures in four minutes over a steady flame.
pub const SMOKE_SECS: f32 = 240.0;

#[derive(Default)]
pub struct StallState {
    /// The seller (server-shaped PlayerId bytes; zero = unclaimed).
    pub owner: [u8; 16],
    pub owner_name: String,
    pub goods: [Option<ItemStack>; 6],
    /// Price per item sold: any stack is a legal price (barter-native).
    pub price: Option<ItemStack>,
    pub till: [Option<ItemStack>; 6],
}

#[derive(Default, Clone)]
pub struct SignState {
    pub lines: [String; 3],
}

/// The steelworks stack: a batch of charge + fuel, fired for half a
/// day inside a validated firebrick shell.
#[derive(Default)]
pub struct BloomeryState {
    pub charge: [Option<ItemStack>; 4],
    pub fuel: [Option<ItemStack>; 4],
    /// Fractional, already-recovered stock waiting to add up to ordinary
    /// recipe units. Forge instances use it; bloomeries leave it empty.
    pub reclaim: crate::registry::MaterialVector,
    pub lit: bool,
    /// Seconds fired so far (out of BLOOMERY_FIRE_SECS).
    pub progress: f32,
    /// Hollow core cell of the validated stack (set on lighting).
    pub core: Option<BlockPos>,
}

/// The glass kiln: sand + one powder + charcoal, fired hot and fast.
#[derive(Default)]
pub struct KilnState {
    pub sand: [Option<ItemStack>; 4],
    pub powder: Option<ItemStack>,
    pub fuel: [Option<ItemStack>; 4],
    pub lit: bool,
    pub progress: f32,
    pub core: Option<BlockPos>,
}

/// Two and a half minutes of white heat per glass batch. Deliberately
/// left on the wall clock when the day doubled: how long a player
/// stands waiting on a kiln is a question about patience, not about
/// the calendar.
pub const KILN_FIRE_SECS: f32 = 150.0;

/// A covered log pile smoldering into charcoal.
pub struct ClampState {
    pub logs: Vec<BlockPos>,
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
    pub pos: crate::planet::EntityPos,
    pub vel: f32,
    pub block: BlockId,
}

/// Five minutes of fire per batch, and the forge's two is still the
/// upgrade worth building. On the wall clock for the same reason the
/// kiln is.
pub const BLOOMERY_FIRE_SECS: f32 = 300.0;
/// The forge runs hotter and shorter than the open stack — capital
/// pays for itself in wall-clock too (economy plan, leg 2).
pub const FORGE_FIRE_SECS: f32 = 120.0;
/// One fuel smelts this many charge items in a forge (a furnace
/// burns roughly one per item — the chimney draft earns its keep).
pub const FORGE_ITEMS_PER_FUEL: u32 = 2;
/// Seconds of smolder per log in a charcoal clamp.
pub const CLAMP_SECS_PER_LOG: f32 = 300.0;
/// Shaft-seconds a powered station banks per strike (a hand strike
/// is a 2 s channel; rate scales this, it never skips it).
pub const STATION_STRIKE_SECS: f32 = 2.0;
/// The helve hammer strikes at half a smith's pace — and all day.
pub const HELVE_STRIKE_SECS: f32 = 4.0;
/// Seconds per pump stroke (one water cell lifted per stroke).
pub const PUMP_STROKE_SECS: f32 = 2.0;
/// How deep a pump's suction column reaches.
pub const PUMP_REACH: i32 = 24;

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

/// Stable save-format identifiers for the finite planetary world.
pub const WORLD_TOPOLOGY: &str = "cube_sphere_v1";
// Version 10 makes visible biome terrain obey the atlas's zonal climate and
// requires physically backed water before applying riparian vegetation.
pub const WORLD_GENERATOR_VERSION: u32 = 10;
const MIN_SUPPORTED_WORLD_GENERATOR_VERSION: u32 = 9;

#[derive(Clone, Debug)]
pub struct WorldMeta {
    pub seed: u32,
    pub mode: String,
    pub ire: f32,
    pub day: u32,
}

fn meta_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (found, value) = line.split_once('=')?;
        (found.trim() == key).then(|| value.trim().trim_matches('"'))
    })
}

fn invalid_world_meta(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}

/// Read and validate the planetary save header.
///
/// `Ok(None)` means the path has never contained a world. A legacy seed file
/// or a `world.toml` without the exact topology contract is an error: planar
/// coordinates must never be silently reinterpreted as planetary ones.
pub fn load_world_meta(dir: &std::path::Path) -> std::io::Result<Option<WorldMeta>> {
    let path = dir.join("world.toml");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if dir.join("seed").exists() {
                return Err(invalid_world_meta(
                    "legacy flat Wildforge world: this build only opens cube_sphere_v1 worlds",
                ));
            }
            return Ok(None);
        }
        Err(error) => return Err(error),
    };

    let topology = meta_value(&text, "topology").ok_or_else(|| {
        invalid_world_meta(
            "flat or unversioned Wildforge world: missing topology = \"cube_sphere_v1\"",
        )
    })?;
    if topology != WORLD_TOPOLOGY {
        return Err(invalid_world_meta(format!(
            "unsupported world topology {topology:?}; expected {WORLD_TOPOLOGY:?}"
        )));
    }

    let face_blocks = meta_value(&text, "face_blocks")
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| invalid_world_meta("planetary world is missing a valid face_blocks"))?;
    if face_blocks != crate::planet::FACE_BLOCKS {
        return Err(invalid_world_meta(format!(
            "unsupported planetary face size {face_blocks}; expected {}",
            crate::planet::FACE_BLOCKS
        )));
    }

    let world_height = meta_value(&text, "world_height")
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| invalid_world_meta("planetary world is missing a valid world_height"))?;
    if world_height != CHUNK_Y as u16 {
        return Err(invalid_world_meta(format!(
            "unsupported world height {world_height}; expected {CHUNK_Y}"
        )));
    }

    let radius = meta_value(&text, "planet_radius")
        .and_then(|value| value.parse::<f64>().ok())
        .ok_or_else(|| invalid_world_meta("planetary world is missing a valid planet_radius"))?;
    if (radius - crate::planet::PLANET_RADIUS).abs() > 0.001 {
        return Err(invalid_world_meta(format!(
            "unsupported planet radius {radius}; expected {:.6}",
            crate::planet::PLANET_RADIUS
        )));
    }

    let generator_version = meta_value(&text, "generator_version")
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| invalid_world_meta("planetary world is missing a generator_version"))?;
    if !(MIN_SUPPORTED_WORLD_GENERATOR_VERSION..=WORLD_GENERATOR_VERSION)
        .contains(&generator_version)
    {
        return Err(invalid_world_meta(format!(
            "unsupported generator version {generator_version}; supported {MIN_SUPPORTED_WORLD_GENERATOR_VERSION}..={WORLD_GENERATOR_VERSION}"
        )));
    }

    let seed = meta_value(&text, "seed")
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| invalid_world_meta("world metadata is missing a valid seed"))?;
    let mode = meta_value(&text, "mode").unwrap_or("survival").to_string();
    let ire = meta_value(&text, "ire")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0)
        .clamp(0.0, 100.0);
    let day = meta_value(&text, "day")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    Ok(Some(WorldMeta {
        seed,
        mode,
        ire,
        day,
    }))
}

/// (seed, mode, ire) from a validated planetary `world.toml`.
pub fn read_world_meta(dir: &std::path::Path) -> (Option<u32>, String, f32) {
    let (seed, mode, ire, _) = read_world_meta_full(dir);
    (seed, mode, ire)
}

/// Full metadata: (seed, mode, ire, day). Dynamic local weather lives in the
/// atlas snapshot, never in a world-wide metadata field.
pub fn read_world_meta_full(dir: &std::path::Path) -> (Option<u32>, String, f32, u32) {
    match load_world_meta(dir) {
        Ok(Some(meta)) => (Some(meta.seed), meta.mode, meta.ire, meta.day),
        Ok(None) | Err(_) => (None, "survival".to_string(), 0.0, 0),
    }
}

pub fn write_world_meta(
    dir: &std::path::Path,
    seed: u32,
    mode: &str,
    ire: f32,
) -> std::io::Result<()> {
    write_world_meta_full(dir, seed, mode, ire, 0)
}

pub fn write_world_meta_full(
    dir: &std::path::Path,
    seed: u32,
    mode: &str,
    ire: f32,
    day: u32,
) -> std::io::Result<()> {
    let text = format!(
        "topology = \"{WORLD_TOPOLOGY}\"\nface_blocks = {}\nworld_height = {CHUNK_Y}\nplanet_radius = {:.6}\ngenerator_version = {WORLD_GENERATOR_VERSION}\nseed = {seed}\nmode = \"{mode}\"\nire = {ire:.2}\nday = {day}\n",
        crate::planet::FACE_BLOCKS,
        crate::planet::PLANET_RADIUS,
    );
    crate::identity::atomic_write(&dir.join("world.toml"), text.as_bytes(), false)
}

/// Create a complete production world off to the side and publish it with a
/// single directory rename. The browser therefore never sees `world.toml`
/// without both its committed immutable atlas and qualified homeland.
pub enum WorldCreationProgress {
    Atlas(crate::planet_atlas::AtlasProgress),
    Homeland {
        stage: String,
        completed: usize,
        total: usize,
    },
}

type HomelandPreparation<'a> = (Arc<Registry>, &'a mut dyn FnMut(WorldCreationProgress));

pub fn create_world_atomic(
    destination: &std::path::Path,
    seed: u32,
    mode: &str,
    content_hash: u64,
    reg: Arc<Registry>,
    cancel: &crate::planet_atlas::CancellationToken,
    mut progress: impl FnMut(WorldCreationProgress),
) -> std::io::Result<()> {
    if destination.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("world path already exists: {}", destination.display()),
        ));
    }
    let atlas = crate::planet_atlas::PlanetAtlas::generate(
        seed,
        content_hash,
        crate::planet_atlas::AtlasConfig::production(),
        cancel,
        |atlas| progress(WorldCreationProgress::Atlas(atlas)),
    )
    .map_err(std::io::Error::other)?;
    publish_created_world(
        destination,
        seed,
        mode,
        atlas,
        cancel,
        Some((reg, &mut progress)),
    )
}

#[cfg(test)]
pub fn create_world_fixture_atomic(
    destination: &std::path::Path,
    seed: u32,
    mode: &str,
    side: u16,
    cancel: &crate::planet_atlas::CancellationToken,
    progress: impl FnMut(crate::planet_atlas::AtlasProgress),
) -> std::io::Result<()> {
    create_world_atomic_with_config(
        destination,
        seed,
        mode,
        0,
        crate::planet_atlas::AtlasConfig::fixture(side),
        cancel,
        progress,
    )
}

#[cfg(test)]
fn create_world_atomic_with_config(
    destination: &std::path::Path,
    seed: u32,
    mode: &str,
    content_hash: u64,
    config: crate::planet_atlas::AtlasConfig,
    cancel: &crate::planet_atlas::CancellationToken,
    progress: impl FnMut(crate::planet_atlas::AtlasProgress),
) -> std::io::Result<()> {
    if destination.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("world path already exists: {}", destination.display()),
        ));
    }
    let atlas =
        crate::planet_atlas::PlanetAtlas::generate(seed, content_hash, config, cancel, progress)
            .map_err(std::io::Error::other)?;
    publish_created_world(destination, seed, mode, atlas, cancel, None)
}

fn publish_created_world(
    destination: &std::path::Path,
    seed: u32,
    mode: &str,
    atlas: crate::planet_atlas::PlanetAtlas,
    cancel: &crate::planet_atlas::CancellationToken,
    mut homeland: Option<HomelandPreparation<'_>>,
) -> std::io::Result<()> {
    if cancel.is_cancelled() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "planet creation cancelled",
        ));
    }
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    fs::create_dir_all(parent)?;
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| std::io::Error::other("invalid world path"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".{name}.creating.{}.{}", std::process::id(), stamp));
    fs::create_dir(&temporary)?;
    let result = (|| {
        atlas.write_new(&temporary).map_err(std::io::Error::other)?;
        if cancel.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "planet creation cancelled",
            ));
        }
        write_world_meta(&temporary, seed, mode, 0.0)?;
        if let Some((reg, progress)) = &mut homeland {
            let mut world = World::load_or_create(temporary.clone(), Arc::clone(reg))?;
            world.prepare_common_spawn(|stage, completed, total| {
                progress(WorldCreationProgress::Homeland {
                    stage: stage.to_owned(),
                    completed,
                    total,
                });
            })?;
            if cancel.is_cancelled() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "planet creation cancelled during homeland preparation",
                ));
            }
        }
        crate::persist::publish_new_directory(&temporary, destination).map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!("could not publish the completed world directory: {error}"),
            )
        })?;
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}

/// List compatible planetary worlds under `dir`: (name, seed), sorted.
pub fn list_worlds(dir: &std::path::Path) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || !e.path().is_dir() {
                continue;
            }
            if let Ok(Some(meta)) = load_world_meta(&e.path())
                && crate::planet_atlas::PlanetAtlas::is_committed(&e.path())
            {
                out.push((name, meta.seed));
            }
        }
    }
    out.sort();
    out
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldBrowserEntry {
    pub name: String,
    pub playable: bool,
    pub status: String,
}

/// Cheap title-screen inspection. Full checksums and bounded file decoding
/// remain in `PlanetAtlas::load` on the cancellable entry worker; browsing a
/// save must not synchronously load roughly 180 MiB of planet state.
pub fn inspect_worlds(dir: &std::path::Path) -> Vec<WorldBrowserEntry> {
    let mut entries = Vec::new();
    let Ok(read_dir) = fs::read_dir(dir) else {
        return entries;
    };
    for entry in read_dir.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || !entry.path().is_dir() {
            continue;
        }
        let status = match load_world_meta(&entry.path()) {
            Err(error) => WorldBrowserEntry {
                name,
                playable: false,
                status: format!("INCOMPATIBLE: {error}"),
            },
            Ok(None) => WorldBrowserEntry {
                name,
                playable: false,
                status: "INCOMPLETE: NO PLANET METADATA".into(),
            },
            Ok(Some(_)) if !crate::planet_atlas::PlanetAtlas::is_committed(&entry.path()) => {
                WorldBrowserEntry {
                    name,
                    playable: false,
                    status: "INCOMPLETE OR CORRUPT PLANET ATLAS".into(),
                }
            }
            Ok(Some(_)) => {
                let manifest = fs::read_to_string(
                    crate::planet_atlas::PlanetAtlas::planet_dir(&entry.path())
                        .join("manifest.toml"),
                )
                .unwrap_or_default();
                let atlas = meta_value(&manifest, "format_version").unwrap_or("?");
                let content = meta_value(&manifest, "content_hash")
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(|value| format!("{value:016x}"))
                    .unwrap_or_else(|| "unknown".into());
                WorldBrowserEntry {
                    name,
                    playable: true,
                    status: format!(
                        "READY  GENERATOR {WORLD_GENERATOR_VERSION}  ATLAS {atlas}  CONTENT {}",
                        &content[..content.len().min(8)]
                    ),
                }
            }
        };
        entries.push(status);
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// One 256×256-cell regional ledger tile on a particular cube face.
///
/// The face is part of the identity: coordinates at the same `(u, v)` on two
/// charts are unrelated countries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RegionCell {
    pub face: crate::planet::Face,
    pub u: u8,
    pub v: u8,
}

impl RegionCell {
    pub const BLOCKS: u16 = 256;

    pub const fn from_surface(surface: crate::planet::SurfacePos) -> Self {
        Self {
            face: surface.face(),
            u: (surface.u() / Self::BLOCKS) as u8,
            v: (surface.v() / Self::BLOCKS) as u8,
        }
    }
}

pub struct World {
    chunks: HashMap<ChunkPos, Chunk>,
    pub generator: Generator,
    planet_atlas: Option<Arc<crate::planet_atlas::PlanetAtlas>>,
    planetary_weather: Option<crate::planet_atlas::PlanetaryWeather>,
    /// Persisted qualified doorstep, populated only after preparation or
    /// successful manifest revalidation.
    common_spawn: Option<crate::planet::EntityPos>,
    /// Exact finite-material manifest and movement ledger. Guests do not own
    /// one; the authoritative host persists it beside the atlas.
    pub(crate) material_ledger: Option<crate::materials::MaterialLedger>,
    /// Atlas-free unit fixtures can request a local condition explicitly.
    /// Production worlds never consult this: their weather is atlas state.
    weather_override: Option<crate::planet_atlas::LocalWeatherSample>,
    remote_weather_side: u16,
    remote_weather: HashMap<crate::planet_atlas::AtlasPos, crate::planet_atlas::LocalWeatherSample>,
    pub reg: Arc<Registry>,
    #[allow(dead_code)]
    pub seed: u32,
    save_dir: PathBuf,
    /// stored-id -> runtime-id remap for chunks loaded from disk.
    load_remap: Vec<BlockId>,
    /// The saved palette no longer matches this registry, so every chunk
    /// that loads has to be rewritten in current ids before the palette on
    /// disk is replaced. Cleared by the first full save of the session.
    palette_stale: bool,
    water_queue: VecDeque<crate::planet::BlockPos>,
    water_queued: HashSet<crate::planet::BlockPos>,
    lava_queue: VecDeque<crate::planet::BlockPos>,
    lava_queued: HashSet<crate::planet::BlockPos>,
    fire_queue: VecDeque<crate::planet::BlockPos>,
    fire_queued: HashSet<crate::planet::BlockPos>,
    /// True while a fluid tick runs: air<->fluid relights batch into
    /// pending_relight (one per chunk per tick) instead of cascading
    /// per moved cell.
    fluid_batch: bool,
    pending_relight: HashSet<ChunkPos>,
    /// Authored bulk edits preserve ordinary block-edit behavior while
    /// settling lighting once per touched chunk.
    edit_relight_batch: bool,
    /// Accumulator for the food-freshness sweep (containers).
    perish_accum: f32,
    /// Seconds of work banked per powered station (transient: a
    /// partial strike is honest to lose across a save).
    station_work: HashMap<BlockPos, f32>,
    /// The land's memory: per-256-block-cell standing (±20), charged
    /// by taking, credited by tending, fading over days.
    pub(crate) regional_ire: HashMap<RegionCell, f32>,
    /// Lines the wild wants spoken (drained by the game as toasts).
    pub whispers: Vec<String>,
    /// Days each cell has held deeply blessed (session-scoped; the
    /// reseed clock restarts on load — the wild forgives the patient).
    blessed_streak: HashMap<RegionCell, u32>,
    /// Chunks a player's hands have edited (placed or broken blocks):
    /// the green tide never seeds ground people made their own.
    pub(crate) player_touched: HashSet<ChunkPos>,
    /// Chunks containing a generated ruin/shrine. Persisted separately from
    /// the chunk's save-dirty bit so palette remaps never make retrogen
    /// mistake a structure for untouched host rock (or vice versa).
    structure_chunks: HashSet<ChunkPos>,
    /// Bloom ledger: days of post-wrath eruption left per 256-cell.
    pub(crate) bloom: HashMap<RegionCell, f32>,
    /// The spirits of the land, keyed by province.
    pub(crate) hearts: HashMap<crate::worldgen::ProvinceKey, Heart>,
    /// How much bloom a cell has already been given without being
    /// tended back — the ground's willingness, spent.
    pub(crate) bloom_spent: HashMap<RegionCell, f32>,
    /// The year has stopped: too many countries have no spirit left.
    pub long_winter: bool,
    /// Absolute sim-time in seconds (day * DAY_LENGTH + time-of-day),
    /// mirrored from the Server every tick so chunk load and random
    /// ticks share one clock.
    pub clock: f64,
    /// When each chunk last took its random ticks (persisted, so the
    /// world can live on while a chunk is away).
    last_random: HashMap<ChunkPos, f64>,
    block_entities: HashMap<crate::planet::BlockPos, BlockEntity>,
    /// Items spilled by removed block entities, for the game loop to spawn.
    pending_drops: Vec<(crate::planet::BlockPos, ItemStack)>,
    mobs: Vec<crate::mobs::Mob>,
    projectiles: Vec<Projectile>,
    hostile_spawn_timer: f32,
    /// Chunks whose wildlife roll already happened (persisted).
    mob_seeded: HashSet<ChunkPos>,
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
    /// Host mode: record block edits for broadcasting.
    log_edits: bool,
    edit_log: Vec<(crate::planet::BlockPos, BlockId, u8, u16, u8)>,
    /// Gravity blocks currently airborne.
    falling: Vec<FallingBlock>,
    /// (guest id, stack) owed over the wire: mining drops, kill loot,
    /// recovered arrows, brush finds — full stacks so durability rides.
    pending_gives: Vec<(u32, ItemStack)>,
    /// Next stable mob id (host side; ids exist for the wire).
    next_mob_id: u32,
    #[cfg(test)]
    save_fail_chunks: HashSet<ChunkPos>,
}

/// Ire tier names, index = tier.
pub const IRE_TIERS: [&str; 4] = ["CALM", "UNEASY", "PROVOKED", "WRATHFUL"];

/// Length of a full lunar cycle (new -> full -> new), in calendar days.
pub const LUNAR_DAYS: u32 = 8;

/// A named lunar phase band. The moon's lit fraction is continuous
/// (`World::moon_illumination`); these eight discrete bands are what game
/// systems and the UI gate on, because a named phase is far more predictable
/// for the player than a raw float.
///
/// INFRASTRUCTURE HOOK: nothing keys off the moon yet. The phase is exposed
/// deterministically (a pure function of the persisted calendar day, so every
/// client and replay agrees) so later work can hang mechanics off it — hostile
/// spawns, rituals once a magic system exists, or resources that can only be
/// harvested (or gain special effects) on a given phase. Read it; don't wire
/// gameplay to it here.
// Hook API: the named-phase surface is exposed for future mechanics and isn't
// consumed inside the engine yet.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MoonPhase {
    New,
    WaxingCrescent,
    FirstQuarter,
    WaxingGibbous,
    Full,
    WaningGibbous,
    LastQuarter,
    WaningCrescent,
}

#[allow(dead_code)]
impl MoonPhase {
    /// The eight bands in cycle order, index = day-into-cycle.
    const ORDER: [MoonPhase; 8] = [
        MoonPhase::New,
        MoonPhase::WaxingCrescent,
        MoonPhase::FirstQuarter,
        MoonPhase::WaxingGibbous,
        MoonPhase::Full,
        MoonPhase::WaningGibbous,
        MoonPhase::LastQuarter,
        MoonPhase::WaningCrescent,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MoonPhase::New => "NEW MOON",
            MoonPhase::WaxingCrescent => "WAXING CRESCENT",
            MoonPhase::FirstQuarter => "FIRST QUARTER",
            MoonPhase::WaxingGibbous => "WAXING GIBBOUS",
            MoonPhase::Full => "FULL MOON",
            MoonPhase::WaningGibbous => "WANING GIBBOUS",
            MoonPhase::LastQuarter => "LAST QUARTER",
            MoonPhase::WaningCrescent => "WANING CRESCENT",
        }
    }
}

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

/// In-game days per season; four seasons make a 144-day year.
///
/// At a 20-minute day that is 12 hours of play per season and two
/// full days per year. A season is meant to be lived in: you plant
/// into one, tend through it, and meet winter with what you put by.
/// Everything below tuned as "a season" is derived from this rather
/// than written out again, so this is the one number to turn.
pub const SEASON_DAYS: u32 = 36;
pub const SEASONS: [&str; 4] = ["SPRING", "SUMMER", "AUTUMN", "WINTER"];

/// Hard cap on living mobs — memory/perf backstop, far above natural density.
pub const MOB_CAP: usize = 320;

impl World {
    /// Move material carried by physically consumed stacks into the explicit
    /// sink. Callers remove the gameplay objects; this keeps eating,
    /// offerings, composting, and spoilage from becoming hidden deletion
    /// paths for ingredients such as finite salt.
    pub fn record_consumed_stacks(
        &mut self,
        stacks: impl IntoIterator<Item = ItemStack>,
    ) -> std::io::Result<()> {
        let mut total = crate::registry::MaterialVector::new();
        for stack in stacks {
            for (material, units) in crate::materials::stack_materials(&self.reg, stack) {
                let stored = total.entry(material).or_default();
                *stored = stored.saturating_add(units);
            }
        }
        let Some(ledger) = &mut self.material_ledger else {
            return Ok(());
        };
        ledger.record_consumption(&total)
    }

    /// Position in the lunar cycle, 0..1 (0 = new moon, 0.5 = full moon). A
    /// pure, deterministic function of the persisted calendar `day`, so every
    /// client and every replay agrees. Constant across a given day (it steps at
    /// dawn), so "tonight is a full moon" is a fixed, plannable fact.
    pub fn moon_cycle(&self) -> f32 {
        (self.day % LUNAR_DAYS) as f32 / LUNAR_DAYS as f32
    }

    /// Illuminated fraction of the moon, 0..1 (0 = new/dark, 1 = full/bright).
    /// Drives moonlight strength and the disc's lit sliver.
    pub fn moon_illumination(&self) -> f32 {
        0.5 * (1.0 - (self.moon_cycle() * std::f32::consts::TAU).cos())
    }

    /// The named phase band for the current day — the discrete signal game
    /// systems should gate on. See [`MoonPhase`]. (Hook API; unused in-engine.)
    #[allow(dead_code)]
    pub fn moon_phase(&self) -> MoonPhase {
        MoonPhase::ORDER[(self.day % LUNAR_DAYS) as usize]
    }

    pub fn new(seed: u32, save_dir: PathBuf, reg: Arc<Registry>) -> World {
        Self::new_with_optional_atlas(seed, save_dir, reg, None)
    }

    pub fn new_with_atlas(
        seed: u32,
        save_dir: PathBuf,
        reg: Arc<Registry>,
        atlas: Arc<crate::planet_atlas::PlanetAtlas>,
    ) -> World {
        Self::new_with_optional_atlas(seed, save_dir, reg, Some(atlas))
    }

    fn new_with_optional_atlas(
        seed: u32,
        save_dir: PathBuf,
        reg: Arc<Registry>,
        planet_atlas: Option<Arc<crate::planet_atlas::PlanetAtlas>>,
    ) -> World {
        let planetary_weather = planet_atlas.as_ref().map(|atlas| {
            crate::planet_atlas::PlanetaryWeather::new(
                atlas.dynamic.clone(),
                atlas.water_cycle.clone(),
            )
        });
        let generator = planet_atlas.as_ref().map_or_else(
            || Generator::new(seed, &reg),
            |atlas| Generator::with_atlas(seed, &reg, atlas.clone()),
        );
        let material_ledger = planet_atlas.as_ref().and_then(|atlas| {
            crate::materials::MaterialLedger::load_or_initialize(&save_dir, atlas, &reg)
                .map_err(|error| {
                    eprintln!("materials: could not open ledger: {error}");
                    error
                })
                .ok()
        });
        World {
            chunks: HashMap::new(),
            generator,
            planet_atlas,
            planetary_weather,
            common_spawn: None,
            material_ledger,
            weather_override: None,
            remote_weather_side: 0,
            remote_weather: HashMap::new(),
            reg,
            seed,
            save_dir,
            load_remap: Vec::new(),
            // A world with no save behind it has no palette on disk
            // either, so the first save owes one. `load_or_create`
            // replaces this with the answer for an existing save.
            palette_stale: true,
            water_queue: VecDeque::new(),
            water_queued: HashSet::new(),
            lava_queue: VecDeque::new(),
            lava_queued: HashSet::new(),
            fire_queue: VecDeque::new(),
            fire_queued: HashSet::new(),
            fluid_batch: false,
            pending_relight: HashSet::new(),
            edit_relight_batch: false,
            clock: 0.0,
            last_random: HashMap::new(),
            block_entities: HashMap::new(),
            pending_drops: Vec::new(),
            perish_accum: 0.0,
            station_work: HashMap::new(),
            regional_ire: HashMap::new(),
            whispers: Vec::new(),
            blessed_streak: HashMap::new(),
            player_touched: HashSet::new(),
            structure_chunks: HashSet::new(),
            bloom: HashMap::new(),
            hearts: HashMap::new(),
            bloom_spent: HashMap::new(),
            long_winter: false,
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
            log_edits: false,
            edit_log: Vec::new(),
            falling: Vec::new(),
            pending_gives: Vec::new(),
            next_mob_id: 1,
            #[cfg(test)]
            save_fail_chunks: HashSet::new(),
        }
    }

    pub fn planet_atlas(&self) -> Option<Arc<crate::planet_atlas::PlanetAtlas>> {
        self.planet_atlas.clone()
    }

    pub fn common_spawn(&self) -> Option<crate::planet::EntityPos> {
        self.common_spawn
    }

    pub fn set_remote_weather(
        &mut self,
        side: u16,
        cells: Vec<(
            crate::planet_atlas::AtlasPos,
            crate::planet_atlas::LocalWeatherSample,
        )>,
    ) {
        self.remote_weather_side = side;
        self.remote_weather.clear();
        self.remote_weather.extend(cells);
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

    pub fn edits(&self) -> &[(crate::planet::BlockPos, BlockId, u8, u16, u8)] {
        &self.edit_log
    }

    pub fn take_edits(&mut self) -> Vec<(crate::planet::BlockPos, BlockId, u8, u16, u8)> {
        std::mem::take(&mut self.edit_log)
    }

    pub fn queue_give(&mut self, owner: u32, stack: ItemStack) {
        self.pending_gives.push((owner, stack));
    }

    pub fn take_pending_gives(&mut self) -> Vec<(u32, ItemStack)> {
        std::mem::take(&mut self.pending_gives)
    }

    #[cfg(test)]
    pub(crate) fn pending_drops(&self) -> &[(crate::planet::BlockPos, ItemStack)] {
        &self.pending_drops
    }

    pub fn clear_pending_drops(&mut self) {
        self.pending_drops.clear();
    }

    /// Every sign and waystone with its text (world rendering, join sync).
    pub fn sign_texts(&self) -> impl Iterator<Item = (crate::planet::BlockPos, &SignState)> {
        self.block_entities.iter().filter_map(|(&p, e)| match e {
            BlockEntity::Sign(s) => Some((p, s)),
            _ => None,
        })
    }

    pub fn push_drop_at(&mut self, at: crate::planet::BlockPos, stack: ItemStack) {
        self.pending_drops.push((at, stack));
    }

    pub fn take_pending_drops(&mut self) -> Vec<(crate::planet::BlockPos, ItemStack)> {
        std::mem::take(&mut self.pending_drops)
    }

    #[cfg(test)]
    pub fn block_entity(&self, pos: &(i32, i32, i32)) -> Option<&BlockEntity> {
        crate::planet::BlockPos::of_world(pos.0, pos.1, pos.2)
            .and_then(|pos| self.block_entities.get(&pos))
    }

    #[cfg(test)]
    pub fn block_entity_mut(&mut self, pos: &(i32, i32, i32)) -> Option<&mut BlockEntity> {
        let pos = crate::planet::BlockPos::of_world(pos.0, pos.1, pos.2)?;
        self.block_entities.get_mut(&pos)
    }

    pub fn block_entity_at(&self, pos: &crate::planet::BlockPos) -> Option<&BlockEntity> {
        self.block_entities.get(pos)
    }

    pub fn block_entity_mut_at(
        &mut self,
        pos: &crate::planet::BlockPos,
    ) -> Option<&mut BlockEntity> {
        self.block_entities.get_mut(pos)
    }

    #[cfg(test)]
    pub fn insert_block_entity(
        &mut self,
        pos: (i32, i32, i32),
        entity: BlockEntity,
    ) -> Option<BlockEntity> {
        let pos = crate::planet::BlockPos::of_world(pos.0, pos.1, pos.2)?;
        self.block_entities.insert(pos, entity)
    }

    pub fn insert_block_entity_at(
        &mut self,
        pos: crate::planet::BlockPos,
        entity: BlockEntity,
    ) -> Option<BlockEntity> {
        self.block_entities.insert(pos, entity)
    }

    /// Insert a development-authored machine/container while keeping every
    /// finite stack in its buffers visible to the material ledger.
    pub fn insert_block_entity_authored_at(
        &mut self,
        pos: crate::planet::BlockPos,
        entity: BlockEntity,
        source: &str,
    ) -> Option<BlockEntity> {
        let old = self.block_entities.remove(&pos);
        if let Some(previous) = old.as_ref()
            && let Err(error) = self.record_admin_block_entity_deletion(previous)
        {
            eprintln!("materials: authored block-entity replacement failed: {error}");
        }
        if let Err(error) = self.record_external_block_entity_contents(&entity, source) {
            eprintln!("materials: authored block-entity source failed: {error}");
        }
        self.block_entities.insert(pos, entity);
        old
    }

    pub fn ensure_block_entity_at(
        &mut self,
        pos: crate::planet::BlockPos,
        default: BlockEntity,
    ) -> &mut BlockEntity {
        self.block_entities.entry(pos).or_insert(default)
    }

    #[cfg(test)]
    pub fn has_block_entity(&self, pos: &(i32, i32, i32)) -> bool {
        crate::planet::BlockPos::of_world(pos.0, pos.1, pos.2)
            .is_some_and(|pos| self.block_entities.contains_key(&pos))
    }

    pub fn block_entities(&self) -> impl Iterator<Item = (&crate::planet::BlockPos, &BlockEntity)> {
        self.block_entities.iter()
    }

    pub fn has_chunk(&self, pos: ChunkPos) -> bool {
        self.chunks.contains_key(&pos)
    }

    /// Mark every loaded chunk for remesh. Used when something outside the
    /// world changes what a mesh should look like — switching texture packs
    /// can change which atlas slot a face draws, and that lives in the uvs.
    pub fn mark_all_chunks_dirty(&mut self) {
        for c in self.chunks.values_mut() {
            c.dirty = true;
        }
    }

    pub fn chunk(&self, pos: ChunkPos) -> Option<&Chunk> {
        self.chunks.get(&pos)
    }

    pub fn mark_chunk_dirty(&mut self, pos: ChunkPos) {
        if let Some(chunk) = self.chunks.get_mut(&pos) {
            chunk.dirty = true;
        }
    }

    /// Chunks outside `radius` of every one of `centers`.
    ///
    /// Chunk residency is the world's business, not the client's. It used to
    /// live only in the client's streaming path, which meant the dedicated
    /// server — the deployment that actually needs it — never evicted
    /// anything and grew by 448 KB for every chunk any guest ever walked
    /// through. With no centers at all nothing is resident: an empty server
    /// holds no world.
    pub fn chunks_outside_all(&self, centers: &[ChunkPos], radius: i32) -> Vec<ChunkPos> {
        self.chunks
            .keys()
            .filter(|pos| {
                !centers
                    .iter()
                    .any(|center| pos.distance(*center) <= f64::from(radius * CHUNK_X as i32))
            })
            .copied()
            .collect()
    }

    /// Save and drop every chunk no longer near any of `centers`.
    ///
    /// Returns what left and what had to stay. Saving as a chunk departs is
    /// the incremental save: there is no short autosave timer, so this is how
    /// most of the world reaches disk.
    pub fn retain_chunks(&mut self, centers: &[ChunkPos], radius: i32) -> ResidencyReport {
        let far = self.chunks_outside_all(centers, radius);
        self.evict_chunks(far).0
    }

    /// Save and unload this exact set, returning both the report and the
    /// positions that actually left. Renderer-owning clients use the latter
    /// to release their matching GPU and light-cache state.
    pub fn evict_chunks(&mut self, candidates: Vec<ChunkPos>) -> (ResidencyReport, Vec<ChunkPos>) {
        let mut report = ResidencyReport::default();
        let mut released = Vec::new();
        if candidates.is_empty() {
            return (report, released);
        }
        self.settle_falling();
        for pos in candidates {
            match self.save_chunk_if_modified(pos) {
                Ok(_) => {
                    self.unload_chunk(pos);
                    report.released += 1;
                    released.push(pos);
                }
                Err(error) => {
                    // The in-memory chunk is the newest copy. Keep it dirty
                    // and resident so the next residency sweep can retry.
                    report.retained_dirty += 1;
                    report.failures.push(SaveFailure::new(
                        format!("chunk {pos:?}"),
                        region::region_path(&self.save_dir, pos),
                        error,
                    ));
                }
            }
        }
        (report, released)
    }

    pub fn unload_chunk(&mut self, pos: ChunkPos) {
        self.chunks.remove(&pos);
    }

    /// How many chunks are resident. The number a long-running server has to
    /// keep bounded.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    #[cfg(test)]
    pub fn fail_chunk_save_for_test(&mut self, pos: ChunkPos, fail: bool) {
        if fail {
            self.save_fail_chunks.insert(pos);
        } else {
            self.save_fail_chunks.remove(&pos);
        }
    }

    #[cfg(test)]
    pub fn mark_structure_chunk_for_test(&mut self, pos: ChunkPos) {
        self.structure_chunks.insert(pos);
        if let Some(chunk) = self.chunks.get_mut(&pos) {
            chunk.modified = true;
        }
    }

    #[cfg(test)]
    /// Entries held across the land's decaying ledgers.
    ///
    /// These are keyed per 256-block cell, persisted, and rewritten whole on
    /// every save, so they are only bounded because each decays to nothing
    /// and drops its entry when it gets there.
    pub fn ledger_len(&self) -> usize {
        self.regional_ire.len() + self.bloom.len() + self.blessed_streak.len()
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

    #[cfg(test)]
    pub(crate) fn planetary_weather_for_test(
        &self,
    ) -> Option<&crate::planet_atlas::PlanetaryWeather> {
        self.planetary_weather.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn planetary_weather_for_test_mut(
        &mut self,
    ) -> Option<&mut crate::planet_atlas::PlanetaryWeather> {
        self.planetary_weather.as_mut()
    }

    pub fn get_block_at(&self, pos: crate::planet::BlockPos) -> BlockId {
        let (x, y, z) = pos.local();
        match self.chunks.get(&pos.chunk()) {
            Some(chunk) => chunk.get(x, y, z),
            None => AIR,
        }
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
        let (x, y, z) = pos.local();
        match self.chunks.get(&pos.chunk()) {
            Some(chunk) => chunk.meta(x, y, z),
            None => 0,
        }
    }

    pub fn get_water_salt_at(&self, pos: crate::planet::BlockPos) -> u16 {
        let (x, y, z) = pos.local();
        self.chunks
            .get(&pos.chunk())
            .map_or(0, |chunk| chunk.water_salt(x, y, z))
    }

    pub fn get_soil_salinity_at(&self, pos: crate::planet::BlockPos) -> u8 {
        let (x, y, z) = pos.local();
        self.chunks
            .get(&pos.chunk())
            .map_or(0, |chunk| chunk.soil_salinity(x, y, z))
    }

    pub fn water_mass_at(
        &self,
        pos: crate::planet::BlockPos,
    ) -> Option<crate::planet_atlas::ReservoirMass> {
        let volume = self.reg.water_volume(self.get_block_at(pos))?;
        Some(crate::planet_atlas::ReservoirMass {
            water_hu: u64::from(volume) * crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL,
            salt_mass: u64::from(self.get_water_salt_at(pos)),
        })
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub fn get_meta(&self, x: i32, y: i32, z: i32) -> u8 {
        crate::planet::BlockPos::of_world(x, y, z)
            .map(|pos| self.get_meta_at(pos))
            .unwrap_or(0)
    }

    pub fn break_block_at(
        &mut self,
        pos: BlockPos,
        tool: Option<ItemId>,
        award_drop: bool,
        affect_ire: bool,
    ) -> Option<BlockBreak> {
        let block = self.get_block_at(pos);
        if block == AIR || self.reg.block(block).hardness.is_none() {
            return None;
        }
        let drop = award_drop
            .then(|| self.reg.drops_for(block, tool))
            .flatten()
            .map(|(item, count)| {
                let item = if tool.is_some() {
                    self.reg.block(block).dismantles_to.unwrap_or(item)
                } else {
                    item
                };
                ItemStack::new(&self.reg, item, count)
            });
        let material_operation = {
            let definition = self.reg.block(block);
            if let Some(ledger) = self.material_ledger.as_mut() {
                match ledger.begin_break(pos, &definition.name, &definition.materials) {
                    Ok(operation) => operation,
                    Err(error) => {
                        eprintln!("materials: block break cancelled at {pos:?}: {error}");
                        return None;
                    }
                }
            } else {
                None
            }
        };
        self.player_touched.insert(pos.chunk());
        if affect_ire {
            let cost = self.ire_for_block(block);
            self.add_ire_at_surface(pos.surface(), cost);
        }
        let was_heart = self.reg.block(block).name.starts_with("base:heart_");
        self.set_block_at(pos, AIR);
        if let Some(operation) = material_operation {
            self.complete_material_operation(&operation);
            if !award_drop
                && let Some(ledger) = &mut self.material_ledger
                && let Err(error) = ledger.record_admin_deletion(&operation.materials)
            {
                eprintln!("materials: could not record creative/admin deletion: {error}");
            }
            if award_drop
                && drop.is_none()
                && let Some(ledger) = &mut self.material_ledger
                && let Err(error) =
                    ledger.bury_materials(pos, &operation.materials, "destructive block breaking")
            {
                eprintln!("materials: could not move destructive breakage to salvage: {error}");
            }
        }
        self.register_player_waterwork_at(pos);
        self.seep_into_excavation_at(pos);
        if was_heart {
            self.heart_struck_at(pos);
        }
        Some(BlockBreak { block, drop })
    }

    #[cfg(test)]
    pub fn place_block(&mut self, pos: (i32, i32, i32), block: BlockId) -> bool {
        let Some(block_pos) = BlockPos::of_world(pos.0, pos.1, pos.2) else {
            return false;
        };
        self.place_block_at(block_pos, block)
    }

    pub fn place_block_at(&mut self, pos: BlockPos, block: BlockId) -> bool {
        if self.reg.blocks.get(block.0 as usize).is_none()
            || !self.reg.is_replaceable(self.get_block_at(pos))
        {
            return false;
        }
        let material_operation = {
            let before = self.reg.block(self.get_block_at(pos)).name.clone();
            let definition = self.reg.block(block);
            if let Some(ledger) = self.material_ledger.as_mut() {
                match ledger.begin_place(pos, &before, &definition.name, &definition.materials) {
                    Ok(operation) => operation,
                    Err(error) => {
                        eprintln!("materials: block placement cancelled at {pos:?}: {error}");
                        return false;
                    }
                }
            } else {
                None
            }
        };
        self.player_touched.insert(pos.chunk());
        // Soil arrives prepared. A block that carries fertility placed
        // at zero is dead ground that LOOKS tilled — it grows nothing
        // and it counts for nothing, which is a trap in either mode and
        // was the reason a hand-laid field around a dead heart did
        // absolutely nothing. Placed farmland is freshly-turned soil.
        if self.reg.block(block).fert_tiles.is_some() {
            let meta = soil::soil_meta(soil::FERT_TILL_GRASS, 0);
            self.set_block_meta_at(pos, block, meta);
            self.initialize_tilled_soil_at(pos);
        } else {
            self.set_block_at(pos, block);
        }
        if let Some(operation) = material_operation {
            self.complete_material_operation(&operation);
        }
        if self.reg.is_solid(block) {
            self.register_player_waterwork_at(pos);
        }
        // Power sources carry a marker entity from birth so the
        // station sweep finds them without scanning the world.
        match self.reg.block(block).interaction.as_deref() {
            Some("wheel" | "sail" | "pump" | "generator") => {
                self.block_entities
                    .entry(pos)
                    .or_insert_with(|| BlockEntity::Anvil(Default::default()));
            }
            Some("firebox") => {
                self.block_entities
                    .entry(pos)
                    .or_insert_with(|| BlockEntity::Steam(Default::default()));
            }
            Some("separator") => {
                self.block_entities
                    .entry(pos)
                    .or_insert_with(|| BlockEntity::Separator(Default::default()));
            }
            _ => {}
        }
        true
    }

    fn complete_material_operation(&mut self, operation: &crate::materials::MaterialOperation) {
        // The voxel lands first. If the process stops after this write, the
        // pending operation replays exactly once on load. If the chunk write
        // fails, leave the journal unapplied: disk still owns the old voxel.
        if let Err(error) = self.save_chunk(operation.pos.chunk()) {
            eprintln!(
                "materials: tracked chunk write failed at {:?}; journal retained: {error}",
                operation.pos
            );
            return;
        }
        let Some(ledger) = &mut self.material_ledger else {
            return;
        };
        if let Err(error) = ledger.apply_operation(operation) {
            eprintln!(
                "materials: ledger operation {} failed: {error}",
                operation.id
            );
            return;
        }
        if let Err(error) = ledger.finish_operation() {
            eprintln!(
                "materials: operation {} committed but journal cleanup failed: {error}",
                operation.id
            );
        }
    }

    #[cfg(test)]
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, b: BlockId) {
        self.set_block_meta(x, y, z, b, 0);
    }

    /// Planetary block mutation used by topology-aware simulation walks.
    pub fn set_block_at(&mut self, pos: crate::planet::BlockPos, block: BlockId) {
        self.set_block_meta_at(pos, block, 0);
    }

    /// A mod/script-authored edit is an explicit source/sink, never an
    /// untracked shortcut around finite extraction. This deliberately marks
    /// the chunk touched so later retrogen cannot overwrite the authored cell.
    pub fn set_block_authored_at(
        &mut self,
        pos: crate::planet::BlockPos,
        block: BlockId,
        source: &str,
    ) {
        let old = self.get_block_at(pos);
        if old == block {
            return;
        }
        self.player_touched.insert(pos.chunk());
        let mut before = self.reg.block(old).name.clone();
        let old_materials = self.reg.block(old).materials.clone();
        if !old_materials.is_empty() {
            if self.break_block_at(pos, None, false, false).is_none() {
                self.set_block_at(pos, AIR);
                if let Some(ledger) = &mut self.material_ledger
                    && let Err(error) = ledger.record_admin_deletion(&old_materials)
                {
                    eprintln!("materials: authored block deletion failed: {error}");
                }
            }
            before = self.reg.block(AIR).name.clone();
        }
        // When the old block is nonmaterial, keep it in place until the
        // authored-placement journal exists. The eventual chunk write then
        // commits that replacement and the material addition together.
        let materials = self.reg.block(block).materials.clone();
        let material_operation = if materials.is_empty() {
            None
        } else if let Some(ledger) = &mut self.material_ledger {
            match ledger.begin_authored_place(
                pos,
                &before,
                &self.reg.block(block).name,
                &materials,
                source,
            ) {
                Ok(operation) => operation,
                Err(error) => {
                    eprintln!("materials: could not journal authored block at {pos:?}: {error}");
                    return;
                }
            }
        } else {
            None
        };
        self.set_block_at(pos, block);
        if let Some(operation) = material_operation {
            self.complete_material_operation(&operation);
        }
    }

    /// Account for a stack introduced by an explicit development/admin path.
    pub fn record_external_stack(&mut self, stack: ItemStack, source: &str) -> std::io::Result<()> {
        let reg = self.reg.clone();
        let Some(ledger) = &mut self.material_ledger else {
            return Ok(());
        };
        ledger.record_external_stack(&reg, stack, source)
    }

    /// Account for a stack overwritten by an explicit development/admin path.
    pub fn record_admin_stack_deletion(&mut self, stack: ItemStack) -> std::io::Result<()> {
        let reg = self.reg.clone();
        let Some(ledger) = &mut self.material_ledger else {
            return Ok(());
        };
        ledger.record_admin_stack_deletion(&reg, stack)
    }

    fn block_entity_stacks(entity: &BlockEntity) -> Vec<ItemStack> {
        let mut stacks = Vec::new();
        let mut add = |slots: &[Option<ItemStack>]| {
            stacks.extend(slots.iter().flatten().copied());
        };
        match entity {
            BlockEntity::Furnace(state) => {
                add(&[state.input, state.fuel, state.output]);
            }
            BlockEntity::Chest(state) => add(&state.slots),
            BlockEntity::Offering(state) => add(&state.slots),
            BlockEntity::Bloomery(state) | BlockEntity::Forge(state) => {
                add(&state.charge);
                add(&state.fuel);
            }
            BlockEntity::Anvil(state) => add(&[state.bloom]),
            BlockEntity::Kiln(state) => {
                add(&state.sand);
                add(&[state.powder]);
                add(&state.fuel);
            }
            BlockEntity::Stall(state) => {
                add(&state.goods);
                add(&[state.price]);
                add(&state.till);
            }
            BlockEntity::Smoker(state) => add(&state.meat),
            BlockEntity::Clamp(_)
            | BlockEntity::Sign(_)
            | BlockEntity::Steam(_)
            | BlockEntity::Separator(_) => {}
        }
        stacks
    }

    /// Account for all physical contents represented by a block entity,
    /// including separator counters and a forge's fractional secondary stock.
    pub fn record_external_block_entity_contents(
        &mut self,
        entity: &BlockEntity,
        source: &str,
    ) -> std::io::Result<()> {
        for stack in Self::block_entity_stacks(entity) {
            self.record_external_stack(stack, source)?;
        }
        if let BlockEntity::Bloomery(state) | BlockEntity::Forge(state) = entity {
            let Some(ledger) = &mut self.material_ledger else {
                return Ok(());
            };
            ledger.record_external_materials(&state.reclaim, true, source)?;
        }
        if let BlockEntity::Separator(state) = entity {
            let counts = [
                ("base:rare_earth_powder", state.powder),
                ("base:charcoal", state.fuel),
                ("base:neodymium", state.nd),
                ("base:cerium", state.ce),
            ];
            for (name, count) in counts {
                if count != 0
                    && let Some(item) = self.reg.item_id(name)
                {
                    self.record_external_stack(ItemStack::new(&self.reg, item, count), source)?;
                }
            }
        }
        Ok(())
    }

    fn record_admin_block_entity_deletion(&mut self, entity: &BlockEntity) -> std::io::Result<()> {
        for stack in Self::block_entity_stacks(entity) {
            self.record_admin_stack_deletion(stack)?;
        }
        if let BlockEntity::Bloomery(state) | BlockEntity::Forge(state) = entity {
            let Some(ledger) = &mut self.material_ledger else {
                return Ok(());
            };
            ledger.record_admin_secondary_deletion(&state.reclaim)?;
        }
        if let BlockEntity::Separator(state) = entity {
            let counts = [
                ("base:rare_earth_powder", state.powder),
                ("base:charcoal", state.fuel),
                ("base:neodymium", state.nd),
                ("base:cerium", state.ce),
            ];
            for (name, count) in counts {
                if count != 0
                    && let Some(item) = self.reg.item_id(name)
                {
                    self.record_admin_stack_deletion(ItemStack::new(&self.reg, item, count))?;
                }
            }
        }
        Ok(())
    }

    /// Low-level typed mutation. Subsystems that already own a canonical
    /// address never convert it back through a planar tuple.
    pub fn set_block_meta_at(&mut self, pos: BlockPos, block: BlockId, meta: u8) {
        self.set_block_water_at(pos, block, meta, 0);
    }

    /// Low-level block update carrying exact dissolved salt. Ordinary block
    /// edits call this with zero; water transfers must provide the parcel's
    /// mass and derive concentration metadata from it.
    pub fn set_block_water_at(&mut self, pos: BlockPos, block: BlockId, meta: u8, salt_mass: u16) {
        let soil_salinity = if self.reg.block(block).fert_tiles.is_some() {
            self.get_soil_salinity_at(pos)
        } else {
            0
        };
        self.set_block_state_at(pos, block, meta, salt_mass, soil_salinity);
    }

    /// Exact authoritative voxel state used by chunk replication. Soil salt
    /// is independent of dissolved water salt and must survive crop/meta edits.
    pub fn set_block_state_at(
        &mut self,
        pos: BlockPos,
        block: BlockId,
        meta: u8,
        salt_mass: u16,
        soil_salinity: u8,
    ) {
        let chunk_pos = pos.chunk();
        let (x, y, z) = pos.local();
        let old = self.get_block_at(pos);
        if let Some(chunk) = self.chunks.get_mut(&chunk_pos) {
            chunk.set(x, y, z, block);
            chunk.set_meta(x, y, z, meta);
            chunk.set_water_salt(x, y, z, salt_mass);
            chunk.set_soil_salinity(x, y, z, soil_salinity);
            chunk.dirty = true;
            chunk.modified = true;
            if self.log_edits {
                self.edit_log
                    .push((pos, block, meta, salt_mass, soil_salinity));
            }
        } else {
            return;
        }
        if x == 0 {
            self.mark_chunk_dirty(chunk_pos.offset(-1, 0));
        } else if x == CHUNK_X - 1 {
            self.mark_chunk_dirty(chunk_pos.offset(1, 0));
        }
        if z == 0 {
            self.mark_chunk_dirty(chunk_pos.offset(0, -1));
        } else if z == CHUNK_Z - 1 {
            self.mark_chunk_dirty(chunk_pos.offset(0, 1));
        }
        self.wake_water_at(pos);

        // Gravity blocks detach when support vanishes, and a newly placed
        // gravity block over air starts falling. All neighbor addressing goes
        // through BlockPos::offset so this works identically at face seams.
        if !self.remote {
            if !self.reg.is_solid(block)
                && let Some(above) = pos.offset(0, 1, 0)
            {
                let above_block = self.get_block_at(above);
                if self.reg.block(above_block).falls {
                    self.detach_at(above, above_block);
                }
            }
            if self.reg.block(block).falls
                && let Some(below) = pos.offset(0, -1, 0)
                && !self.reg.is_solid(self.get_block_at(below))
            {
                self.detach_at(pos, block);
                return;
            }
        }

        // Plants, torches, and layers pop when the support underneath changes.
        if !self.reg.is_solid(block)
            && let Some(above) = pos.offset(0, 1, 0)
        {
            let above_block = self.get_block_at(above);
            let above_def = self.reg.block(above_block);
            if above_block != AIR
                && !above_def.floats
                && (above_def.cross || above_def.height.is_some())
            {
                if let Some((item, count)) = above_def.drops {
                    let reg = self.reg.clone();
                    self.push_drop_at(above, ItemStack::new(&reg, item, count));
                }
                self.set_block_at(above, AIR);
            }
        }

        let fluid_level_only = self.reg.is_fluid(old)
            && self.reg.is_fluid(block)
            && self.reg.is_lava(old) == self.reg.is_lava(block);
        let front_move = self.fluid_batch
            && (self.reg.is_fluid(old) || self.reg.is_fluid(block))
            && (self.reg.is_fluid(old) || self.reg.is_air(old))
            && (self.reg.is_fluid(block) || self.reg.is_air(block));
        let bulk_edit = self.edit_relight_batch && !fluid_level_only;
        if front_move || bulk_edit {
            self.pending_relight.insert(chunk_pos);
        } else if !fluid_level_only {
            self.relight_and_cascade(chunk_pos);
        }

        // A changed block invalidates any machine state living there and
        // returns its inventory at that exact planetary address.
        if let Some(entity) = self.block_entities.remove(&pos) {
            let spilled: Vec<ItemStack> = match entity {
                BlockEntity::Furnace(f) => {
                    [f.input, f.fuel, f.output].into_iter().flatten().collect()
                }
                BlockEntity::Chest(c) => c.slots.into_iter().flatten().collect(),
                BlockEntity::Offering(o) => o.slots.into_iter().flatten().collect(),
                BlockEntity::Bloomery(b) => b.charge.into_iter().chain(b.fuel).flatten().collect(),
                BlockEntity::Forge(f) => {
                    if !f.reclaim.is_empty()
                        && let Some(ledger) = &mut self.material_ledger
                        && let Err(error) = ledger.bury_materials(
                            pos,
                            &f.reclaim,
                            "forge dismantled with fractional recovered stock",
                        )
                    {
                        eprintln!("materials: forge stock salvage failed: {error}");
                    }
                    f.charge.into_iter().chain(f.fuel).flatten().collect()
                }
                BlockEntity::Sign(_) => Vec::new(),
                BlockEntity::Stall(st) => st
                    .goods
                    .into_iter()
                    .chain(st.till)
                    .chain([st.price])
                    .flatten()
                    .collect(),
                BlockEntity::Smoker(sm) => sm.meat.into_iter().flatten().collect(),
                BlockEntity::Clamp(_) => Vec::new(),
                BlockEntity::Anvil(a) => a.bloom.into_iter().collect(),
                BlockEntity::Steam(_) => Vec::new(),
                BlockEntity::Separator(separator) => {
                    let mut out = Vec::new();
                    let mut push = |name: &str, count: u32| {
                        if count > 0
                            && let Some(item) = self.reg.item_id(name)
                        {
                            let mut stack = ItemStack::new(&self.reg, item, 1);
                            stack.count = count;
                            out.push(stack);
                        }
                    };
                    push("base:rare_earth_powder", separator.powder);
                    push("base:charcoal", separator.fuel);
                    push("base:neodymium", separator.nd);
                    push("base:cerium", separator.ce);
                    out
                }
                BlockEntity::Kiln(k) => k
                    .sand
                    .into_iter()
                    .chain(k.fuel)
                    .chain([k.powder])
                    .flatten()
                    .collect(),
            };
            for stack in spilled {
                self.push_drop_at(pos, stack);
            }
        }
    }

    pub fn set_soil_salinity_at(&mut self, pos: BlockPos, salinity: u8) {
        let block = self.get_block_at(pos);
        if self.reg.block(block).fert_tiles.is_none() {
            return;
        }
        self.set_block_state_at(
            pos,
            block,
            self.get_meta_at(pos),
            self.get_water_salt_at(pos),
            salinity,
        );
    }

    /// Apply an authored group of edits with normal logging, support checks,
    /// and fluid wakeups, but settle lighting only once per touched chunk.
    pub(crate) fn edit_batch(&mut self, edit: impl FnOnce(&mut Self)) {
        assert!(
            !self.fluid_batch && !self.edit_relight_batch && self.pending_relight.is_empty(),
            "block-edit batches cannot nest"
        );
        self.edit_relight_batch = true;
        edit(self);
        self.edit_relight_batch = false;
        let starts = std::mem::take(&mut self.pending_relight);
        self.relight_chunks_and_cascade(starts);
    }

    #[cfg(test)]
    pub(crate) fn edit_fixture_for_test(&mut self, edit: impl FnOnce(&mut Self)) {
        self.edit_batch(edit);
    }

    /// Convenience wrapper for block-only fixtures.
    #[cfg(test)]
    pub(crate) fn set_blocks_for_test(
        &mut self,
        edits: impl IntoIterator<Item = (i32, i32, i32, BlockId)>,
    ) {
        self.edit_fixture_for_test(|world| {
            for (x, y, z, block) in edits {
                world.set_block(x, y, z, block);
            }
        });
    }

    /// Install blank authoritative chunks for protocol fixtures that exercise
    /// a hand-built stage rather than terrain generation.
    #[cfg(test)]
    pub(crate) fn insert_empty_chunks_for_test(
        &mut self,
        positions: impl IntoIterator<Item = ChunkPos>,
    ) {
        let bedrock = self
            .reg
            .block_id("base:bedrock")
            .expect("base test registry has bedrock");
        for pos in positions {
            let mut chunk = Chunk::new();
            for x in 0..CHUNK_X {
                for z in 0..CHUNK_Z {
                    chunk.set(x, 0, z, bedrock);
                }
            }
            assert!(
                self.chunks.insert(pos, chunk).is_none(),
                "test fixture inserted chunk {pos:?} twice"
            );
        }
    }

    /// Set a block with an explicit metadata byte.
    #[cfg(test)]
    #[doc(hidden)]
    pub fn set_block_meta(&mut self, x: i32, y: i32, z: i32, b: BlockId, meta: u8) {
        if let Some(pos) = BlockPos::of_world(x, y, z) {
            self.set_block_meta_at(pos, b, meta);
        }
    }

    /// Y of the highest solid block in a column (for spawn placement).
    #[cfg(test)]
    pub fn surface_height(&self, x: i32, z: i32) -> i32 {
        let surface = crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
            .expect("legacy surface query is within the bounded porting window");
        self.surface_height_at(surface)
    }

    pub fn surface_height_at(&self, surface: crate::planet::SurfacePos) -> i32 {
        for y in (0..CHUNK_Y as i32).rev() {
            let pos =
                crate::planet::BlockPos::new(surface.face(), surface.u(), y as u8, surface.v())
                    .expect("surface column and height are validated");
            if self.reg.is_solid(self.get_block_at(pos)) {
                return y;
            }
        }
        0
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
        let floor = (y - 64).max(0)..=y;
        let floor = floor
            .rev()
            .find(|&fy| self.reg.is_solid(self.get_block_at(at(fy))))
            // An ungenerated/open column has no trustworthy ground. A
            // floater should hold station until terrain is resident, not
            // interpret missing data as a sixty-block abyss.
            .unwrap_or((y - 2).max(0));
        let ceil = ((y + 1)..=(y + 40).min(CHUNK_Y as i32 - 1))
            .find(|&cy| self.reg.is_solid(self.get_block_at(at(cy))))
            .unwrap_or(CHUNK_Y as i32);
        (floor, ceil)
    }

    /// Can a player body stand with its feet in cell y? Feet and
    /// head clear of solids, solid ground directly underfoot.
    fn standable_at(&self, surface: crate::planet::SurfacePos, y: i32) -> bool {
        // Fluid is not solid, so a seabed column used to read as
        // "standable" and players were dropped on the ocean floor.
        // Somewhere to stand means dry air for the body, too.
        let clear = |b: BlockId| !self.reg.is_solid(b) && !self.reg.is_fluid(b);
        let at = |height: i32| {
            crate::planet::BlockPos::new(surface.face(), surface.u(), height as u8, surface.v())
                .expect("standable height is inside the vertical shell")
        };
        self.reg.is_solid(self.get_block_at(at(y - 1)))
            && clear(self.get_block_at(at(y)))
            && clear(self.get_block_at(at(y + 1)))
    }

    /// Somewhere a player can be put down: dry, solid-footed, and
    /// above the tideline. Searches outward from the asked column and,
    /// finding nothing but open water, raises a small sand island
    /// rather than dropping anyone into the sea.
    #[cfg(test)]
    pub fn safe_spawn(&mut self, x: i32, z: i32) -> Vec3 {
        let surface = crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
            .expect("legacy spawn query is inside PosZ");
        self.safe_spawn_at(surface).local()
    }

    pub fn safe_spawn_at(&mut self, wanted: crate::planet::SurfacePos) -> crate::planet::EntityPos {
        let stands_dry = |w: &mut World,
                          surface: crate::planet::SurfacePos|
         -> Option<crate::planet::EntityPos> {
            w.ensure_chunk(ChunkPos::from_surface(surface));
            let h = w.surface_height_at(surface);
            let feet = h + 1;
            (h > SEA_LEVEL && w.standable_at(surface, feet)).then(|| {
                crate::planet::EntityPos::new(
                    surface.face(),
                    surface.u() as f32 + 0.5,
                    feet as f32 + 0.2,
                    surface.v() as f32 + 0.5,
                )
                .expect("surface cell center is a canonical entity position")
            })
        };
        if let Some(p) = stands_dry(self, wanted) {
            return p;
        }
        // Rings outward: near land first, so a coastal spawn walks
        // ashore instead of conjuring ground it didn't need. The ring
        // is probed with the generator's cheap height estimate, and
        // only a column that looks dry is actually generated — this
        // search used to build hundreds of chunks per join.
        for r in (4..=96).step_by(4) {
            for (dx, dz) in [
                (r, 0),
                (-r, 0),
                (0, r),
                (0, -r),
                (r, r),
                (-r, -r),
                (r, -r),
                (-r, r),
            ] {
                let Ok(surface) = crate::planet::SurfacePos::canonicalized(
                    wanted.face(),
                    wanted.u() as i32 + dx,
                    wanted.v() as i32 + dz,
                ) else {
                    continue;
                };
                if self.generator.surface_estimate_at(surface) <= SEA_LEVEL + 1 {
                    continue;
                }
                if let Some(p) = stands_dry(self, surface) {
                    return p;
                }
            }
        }
        // Open ocean in every direction: make landfall.
        let top = self.raise_castaway_isle_at(wanted);
        crate::planet::EntityPos::new(
            wanted.face(),
            wanted.u() as f32 + 0.5,
            top as f32 + 1.2,
            wanted.v() as f32 + 0.5,
        )
        .expect("surface cell center is a canonical entity position")
    }

    /// Raise a small sand island for a castaway spawn: a low dome up
    /// out of the water with its own patch of dry ground. Returns the
    /// height of the ground at its center.
    pub fn raise_castaway_isle_at(&mut self, center: crate::planet::SurfacePos) -> i32 {
        const R: i32 = 5;
        let sand = self.reg.block_id("base:sand").unwrap_or(AIR);
        let crest = SEA_LEVEL + 2;
        for dx in -R..=R {
            for dz in -R..=R {
                let d2 = dx * dx + dz * dz;
                if d2 > R * R {
                    continue;
                }
                let surface = crate::planet::SurfacePos::canonicalized(
                    center.face(),
                    center.u() as i32 + dx,
                    center.v() as i32 + dz,
                )
                .expect("castaway island radius crosses at most one face edge");
                self.ensure_chunk(ChunkPos::from_surface(surface));
                let at = |y: i32| {
                    crate::planet::BlockPos::new(surface.face(), surface.u(), y as u8, surface.v())
                        .expect("castaway island height is inside the shell")
                };
                // A dome: full height at the middle, shelving into the
                // water at the rim.
                let top = crest - (d2 as f32 / 6.0).round() as i32;
                let floor = (1..=SEA_LEVEL)
                    .rev()
                    .find(|&y| self.reg.is_solid(self.get_block_at(at(y))))
                    .unwrap_or(1);
                for y in floor + 1..=top {
                    self.set_block_at(at(y), sand);
                }
                // Dry it out overhead, so the island is actually air.
                for y in top + 1..=SEA_LEVEL + 4 {
                    if self.reg.is_fluid(self.get_block_at(at(y))) {
                        self.set_block_at(at(y), AIR);
                    }
                }
            }
        }
        crest
    }

    /// Resolve a requested standing position into one a body can
    /// actually occupy. Saved spawns go stale — a bedroll gets built
    /// over, terrain regenerates under an old save — and a player
    /// placed inside a hill is simply stuck. A valid spot returns
    /// unchanged; otherwise the nearest clear opening in the column
    /// wins (downward on ties, matching the old come-to-ground rule),
    /// then a ring of neighbor columns, then the column surface.
    #[cfg(test)]
    pub fn settle_spawn(&mut self, want: Vec3) -> Vec3 {
        crate::planet::EntityPos::from_local(crate::planet::Face::PosZ, want)
            .map(|want| self.settle_spawn_at(want).local())
            .unwrap_or(want)
    }

    pub fn settle_spawn_at(&mut self, want: crate::planet::EntityPos) -> crate::planet::EntityPos {
        let surface = crate::planet::SurfacePos::new(
            want.face(),
            want.u().floor() as u16,
            want.v().floor() as u16,
        )
        .expect("canonical entity has a valid surface cell");
        self.ensure_chunk(ChunkPos::from_surface(surface));
        let feet = (want.y.floor() as i32).clamp(1, CHUNK_Y as i32 - 2);
        if self.standable_at(surface, feet) {
            return want;
        }
        for d in 1..CHUNK_Y as i32 {
            for y in [feet - d, feet + d] {
                if y >= 1 && y < CHUNK_Y as i32 - 1 && self.standable_at(surface, y) {
                    return crate::planet::EntityPos::new(
                        want.face(),
                        want.u(),
                        y as f32 + 0.2,
                        want.v(),
                    )
                    .expect("settled height preserves a canonical surface position");
                }
            }
        }
        // The column offers nothing (filled sky-to-bedrock): walk
        // outward for the nearest column with open ground.
        for r in 1..=8i32 {
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dz.abs() != r {
                        continue;
                    }
                    let neighbor = crate::planet::SurfacePos::canonicalized(
                        surface.face(),
                        surface.u() as i32 + dx,
                        surface.v() as i32 + dz,
                    )
                    .expect("spawn rescue radius crosses at most one face edge");
                    self.ensure_chunk(ChunkPos::from_surface(neighbor));
                    let y = self.surface_height_at(neighbor) + 1;
                    if y < CHUNK_Y as i32 - 1 && self.standable_at(neighbor, y) {
                        return crate::planet::EntityPos::new(
                            neighbor.face(),
                            neighbor.u() as f32 + 0.5,
                            y as f32 + 0.2,
                            neighbor.v() as f32 + 0.5,
                        )
                        .expect("neighbor cell center is canonical");
                    }
                }
            }
        }
        // Last resort: on top of whatever this column calls surface.
        let y = self.surface_height_at(surface) + 1;
        crate::planet::EntityPos::new(want.face(), want.u(), y as f32 + 0.2, want.v())
            .expect("settled height preserves a canonical surface position")
    }

    /// Free a restored *position* only if it is embedded in solid.
    /// Unlike `settle_spawn`, a legitimate mid-air or mid-swim save
    /// passes through untouched — physics owns falling and floating;
    /// this only rescues a body inside a hill.
    #[cfg(test)]
    pub fn free_position(&mut self, pos: Vec3) -> Vec3 {
        crate::planet::EntityPos::from_local(crate::planet::Face::PosZ, pos)
            .map(|pos| self.free_position_at(pos).local())
            .unwrap_or(pos)
    }

    pub fn free_position_at(&mut self, pos: crate::planet::EntityPos) -> crate::planet::EntityPos {
        let surface = crate::planet::SurfacePos::new(
            pos.face(),
            pos.u().floor() as u16,
            pos.v().floor() as u16,
        )
        .expect("canonical entity has a valid surface cell");
        self.ensure_chunk(ChunkPos::from_surface(surface));
        let feet = (pos.y.floor() as i32).clamp(1, CHUNK_Y as i32 - 2);
        let block = |y: i32| {
            crate::planet::BlockPos::new(surface.face(), surface.u(), y as u8, surface.v())
                .expect("rescued height is inside the shell")
        };
        let embedded = self.reg.is_solid(self.get_block_at(block(feet)))
            || self.reg.is_solid(self.get_block_at(block(feet + 1)));
        if embedded {
            self.settle_spawn_at(pos)
        } else {
            pos
        }
    }
}
