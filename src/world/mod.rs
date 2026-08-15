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
use crate::entity::ItemEntity;
use crate::inventory::ItemStack;
use crate::mobs::{Mob, MobEvent, ProjHit, Projectile};
use crate::planet::BlockPos;
use crate::registry::{AIR, BlockId, ItemId, Registry};
use crate::worldgen::Generator;

/// Dropped-item ids occupy a high, signed-64-safe namespace so they cannot
/// collide with ordinary projectile ids and still round-trip through TOML.
const LOOSE_ITEM_ID_BASE: u64 = 1u64 << 62;

mod alchemy;
mod calendar;
mod chunks;
mod discovery;
pub(crate) use discovery::ObservationTarget;
mod dross;
mod ecology;
pub use ecology::SettledMobDeath;
pub(crate) mod belt;
mod entities;
mod fire;
mod fluids;
mod hearts;
mod implements;
mod lighting;
mod machine_tick;
pub(crate) mod machines;
pub(crate) mod multiblock;
mod persistence;
mod pieces;
mod power;
pub(crate) mod power_draw;
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
pub(crate) mod local_structure;
pub(crate) mod rail;
pub(crate) mod template;
mod ticks;
mod workings;

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
    /// A validated multiblock shell (bloomery/forge/kiln/separator):
    /// one generic representation carrying the shape ID and the state
    /// any furnace-style machine needs (spec Part 2.1 groundwork).
    Multiblock(MachineInstance),
    Clamp(ClampState),
    Anvil(AnvilState),
    /// Three short lines on a post (a waystone uses line 0 as its name).
    Sign(SignState),
    /// A market stall counter: goods, a price, and the owner's till.
    Stall(StallState),
    /// A smoking rack: raw cuts curing over a live torch.
    Smoker(SmokerState),
    /// A steam firebox: banked fire and exact boiler water.
    Steam(SteamState),
    /// A placed shared library. The records themselves remain signed in the
    /// discovery state; this block owns the physical object that indexes them.
    SurveyFolio(SurveyFolioState),
    /// A controlled trial holds its sample and calibrated reference in world
    /// custody. Experiments read these bays without consuming either.
    DiscoveryApparatus(DiscoveryApparatusState),
    /// Physical mounts and completed output of a binding frame. Components
    /// remain ordinary item stacks while the frame holds them.
    BindingFrame(BindingFrameState),
    /// The placed shell owns exactly one stable vessel item identity.
    ChargeVessel(ChargeVesselState),
    /// A rail switch: which exit is currently selected. The rest of the rail
    /// piece is ordinary block data (spec Part 2.2, scoped).
    Switch(SwitchState),
}

/// One mutable rail-switch block: the currently-selected exit. `None` on a
/// switch block (no entity) reads as the straight default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchState {
    pub selected: crate::planet::Direction4,
}

impl Default for SwitchState {
    fn default() -> Self {
        Self {
            selected: crate::planet::Direction4::North,
        }
    }
}

#[derive(Default)]
pub struct SurveyFolioState {
    pub object_id: u64,
}

#[derive(Default)]
pub struct DiscoveryApparatusState {
    pub sample: Option<ItemStack>,
    pub reference: Option<ItemStack>,
}

#[derive(Default)]
pub struct BindingFrameState {
    pub body: Option<ItemStack>,
    pub reservoir: Option<ItemStack>,
    pub focus: Option<ItemStack>,
    pub binding: Option<ItemStack>,
    pub output: Option<ItemStack>,
    pub revision: u64,
}

impl BindingFrameState {
    pub fn mounts(&self) -> [Option<ItemStack>; 4] {
        [self.body, self.reservoir, self.focus, self.binding]
    }

    pub fn is_empty(&self) -> bool {
        self.mounts().into_iter().all(|stack| stack.is_none()) && self.output.is_none()
    }
}

#[derive(Default)]
pub struct ChargeVesselState {
    pub vessel: Option<ItemStack>,
    pub damage: u16,
    pub revision: u64,
}

#[derive(Default)]
pub struct MachineInstance {
    /// Which registered machine this is — its shape ID.
    pub kind: crate::world::multiblock::MachineKind,
    /// Whether the fire is lit.
    pub lit: bool,
    /// Seconds fired so far.
    pub progress: f32,
    /// Hollow core cell of the validated shell (set on lighting).
    pub core: Option<BlockPos>,
    /// Four primary input slots (bloomery/forge charge, kiln sand).
    pub charge: [Option<ItemStack>; 4],
    /// A single reagent slot (the kiln's pigment).
    pub reagent: Option<ItemStack>,
    /// Four fuel slots.
    pub fuel: [Option<ItemStack>; 4],
    /// Fractional, already-recovered stock waiting to add up to ordinary
    /// recipe units. Forge instances use it; bloomeries leave it empty.
    pub reclaim: crate::registry::MaterialVector,
    /// Separator input/output counts (rare-earth powder and charcoal in;
    /// neodymium and cerium out).
    pub powder: u32,
    pub separator_fuel: u32,
    pub neodymium: u32,
    pub cerium: u32,
    /// Folded Pattern A stats of the validated shell (spec Part 2.1).
    /// Recomputed on revalidation, never per tick.
    pub stats: crate::world::multiblock::EffectiveStats,
    /// Qualitative capabilities the installed slot modules grant their
    /// frame (spec Part 1.3). Folded on revalidation alongside stats;
    /// distinct from stats because these change what a structure can do,
    /// not how well it does it.
    pub capabilities: crate::world::multiblock::Capabilities,
}

/// The world's year stops when this many countries are dead AND they are
/// this share of every country anyone has seen.
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

/// Two and a half minutes of white heat per glass batch. Deliberately
/// left on the wall clock when the day doubled: how long a player
/// stands waiting on a kiln is a question about patience, not about
/// the calendar.
pub const KILN_FIRE_SECS: f32 = 150.0;

/// How far a running generator's field reaches (lamps, the quern).
pub const ELEC_RADIUS: i32 = 6;

#[derive(Default)]
pub struct SteamState {
    /// Seconds of fire banked (coal fed by hand at the door).
    pub fuel: f32,
    /// Exact boiler reservoir; evaporation leaves dissolved salt here.
    pub water: crate::planet_atlas::ReservoirMass,
    /// Physical draft latch operated by hand or a bounded Nudge. This belongs
    /// to the embodied firebox, not generic voxel metadata, whose bits have
    /// unrelated meanings for other blocks and must reset on visual swaps.
    pub draft_closed: bool,
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

/// The glass kiln's fire duration.
#[derive(Default)]
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
    Arcane(crate::arcane_geography::ArcaneGeographyProgress),
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
            let geography =
                crate::arcane_geography::ArcaneGeography::generate(&atlas, reg, cancel, |arcane| {
                    progress(WorldCreationProgress::Arcane(arcane))
                })
                .map_err(std::io::Error::other)?;
            geography
                .write_new(&temporary)
                .map_err(std::io::Error::other)?;
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

/// Publish a complete development qualification world from an already
/// committed production atlas. This is deliberately narrower than ordinary
/// world creation: it exists so read-only visual locators can select one
/// immutable atlas and the capture harness can materialize that exact world
/// without regenerating or silently substituting a different planet.
pub(crate) fn create_qualification_world_from_atlas(
    destination: &std::path::Path,
    atlas: crate::planet_atlas::PlanetAtlas,
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
    let seed = atlas.manifest.seed;
    publish_created_world(
        destination,
        seed,
        "survival",
        atlas,
        cancel,
        Some((reg, &mut progress)),
    )
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
    /// Exact finite-Current accounts. Like materials, this exists only on the
    /// authoritative simulation; guests receive bounded local observations.
    pub(crate) arcane_ledger: Option<crate::arcane::ArcaneLedger>,
    /// Compact exact planetary subledger and its causal immutable controls.
    pub(crate) arcane_geography: Option<crate::arcane_geography::ArcaneGeography>,
    /// Signed physical observation records and archaeological provenance.
    /// Guests hold only bounded summaries; the host owns the signing key and
    /// object census beside the world's other finite ledgers.
    pub(crate) discovery_state: Option<crate::discovery::DiscoveryState>,
    /// Physical construction, wear, and provenance for stable charge-bearing
    /// implements. Exact Current remains in `arcane_ledger`.
    pub(crate) implements_state: Option<crate::implements::ImplementsState>,
    /// Host-owned multi-tick magical transactions. Each active id has exact
    /// Current custody in `ArcaneOwner::Working` and survives unload/restart.
    pub(crate) workings_state: Option<crate::workings::WorkingsState>,
    /// Exact apparatus, batch, vessel, and timed preparation state. The host
    /// owns this sidecar; guests receive only bounded station/status cues.
    pub(crate) alchemy_state: Option<crate::alchemy::AlchemyState>,
    /// Sparse exact temperature/dross carriers for detailed water touched by
    /// conservative magical transfer. Ordinary untouched water derives its
    /// baseline temperature from weather and costs no per-voxel allocation.
    pub(crate) water_carriers: Option<crate::workings::WaterCarrierState>,
    /// Last host tick at which each actor received environmental exposure.
    /// Bodily burden is persisted with player profiles; this cadence cache is
    /// deliberately transient and cannot mint or delete environmental dross.
    dross_exposure_tick: HashMap<[u8; 16], u64>,
    /// Atlas-free unit fixtures can request a local condition explicitly.
    /// Production worlds never consult this: their weather is atlas state.
    weather_override: Option<crate::planet_atlas::LocalWeatherSample>,
    remote_weather_side: u16,
    remote_weather: HashMap<crate::planet_atlas::AtlasPos, crate::planet_atlas::LocalWeatherSample>,
    remote_arcane_cue: [u8; 2],
    /// Guest-safe categorical base resonance: 0 is unclear, 1..=6 follows
    /// `arcane::BASE_RESONANCES`. The authoritative mixture never crosses the
    /// wire.
    remote_arcane_dominant: u8,
    remote_arcane_ecology: Option<(String, bool)>,
    /// Exact charge only for opaque item ids the host says this guest may
    /// inspect. Replaced as a bounded snapshot; never populated from clients.
    remote_arcane_items: HashMap<u64, u64>,
    remote_implements: HashMap<u64, crate::implements::ImplementPublicState>,
    remote_apparatus: HashMap<crate::planet::BlockPos, crate::implements::ApparatusCue>,
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
    /// Multiblock revalidations triggered by block edits since construction.
    /// Test-only: proves the 2c edit hook is scoped, not global.
    #[cfg(test)]
    multiblock_revalidations: usize,
    /// Seconds of work banked per powered station (transient: a
    /// partial strike is honest to lose across a save).
    station_work: HashMap<BlockPos, f32>,
    /// Belt cells carrying cargo (spec §2.2): transient runtime state,
    /// exactly like `RailState` — a reloaded belt is empty until fed again.
    belt_state: HashMap<crate::planet::BlockPos, crate::world::belt::BeltState>,
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
    /// Flag-gated feature positions (spec 2.5): world position -> gate index
    /// into the registry. Sealed blocks placed by `feature:<id>` markers;
    /// locked until the player's KV flag reads the gate's `value`. Persisted
    /// so a save never depends on regeneration to know what is sealed.
    gated: HashMap<BlockPos, usize>,
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
    /// Named, world-shared structural templates (spec Part 1.4). Structure
    /// only — no `BlockEntity` contents — so the library is persistable and
    /// duplication-safe with a single TOML sidecar.
    templates: Vec<crate::world::template::Template>,
    /// Active ghost overlays: the world cells a player still has to place,
    /// keyed absolutely and mapped to the required block name.
    pending_fills: Vec<crate::world::template::PendingFill>,
    /// Spawned local structures (spec Part 1.1, scoped): self-contained
    /// block stores that exist off the chunk grid, each with its own static
    /// world transform. Persisted to `local_structures.toml`.
    local_structures: Vec<crate::world::local_structure::LocalStructure>,
    /// Allocator for [`World::local_structures`] ids, advanced on every
    /// spawn so ids stay unique across a session.
    next_local_structure_id: u64,
    /// Items spilled by removed block entities, for the game loop to spawn.
    pending_drops: Vec<(crate::planet::BlockPos, ItemStack)>,
    mobs: Vec<crate::mobs::Mob>,
    projectiles: Vec<Projectile>,
    next_projectile_id: u64,
    /// Ordinary dropped items are host-owned entities, not renderer-local
    /// decorations. This makes pickup, collision, persistence, replication,
    /// and Nudge share one authority.
    loose_items: Vec<ItemEntity>,
    next_loose_item_id: u64,
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
    /// Live NPCs (spec Part 3.1). Each links a companion Mob (by stable id)
    /// to its authoring def + patrol walker. Persisted via the mob save path.
    npcs: Vec<crate::npc::NpcInstance>,
    /// Next stable mob id (host side; ids exist for the wire).
    next_mob_id: u32,
    #[cfg(test)]
    save_fail_chunks: HashSet<ChunkPos>,
    #[cfg(test)]
    fail_loose_item_save: bool,
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

fn cancel_unapplied_material_operation(
    ledger: Option<&crate::materials::MaterialLedger>,
    operation: &Option<crate::materials::MaterialOperation>,
) {
    if operation.is_some()
        && let Some(ledger) = ledger
        && let Err(error) = ledger.finish_operation()
    {
        eprintln!("materials: could not cancel unapplied block operation: {error}");
    }
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

/// Cap on live NPCs. Authored NPCs are rare and placed deliberately (a
/// `spawn:npc:<id>` marker or a mod's `on_world_start`), so a small budget
/// is the right shape — it stops a bad script from flooding the world.
pub const NPC_CAP: usize = 24;

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
        Self::new_with_optional_atlas(seed, save_dir, reg, None, true)
    }

    #[cfg(test)]
    pub fn new_with_atlas(
        seed: u32,
        save_dir: PathBuf,
        reg: Arc<Registry>,
        atlas: Arc<crate::planet_atlas::PlanetAtlas>,
    ) -> World {
        Self::new_with_optional_atlas(seed, save_dir, reg, Some(atlas), true)
    }

    fn new_with_preloaded_atlas(
        seed: u32,
        save_dir: PathBuf,
        reg: Arc<Registry>,
        atlas: Arc<crate::planet_atlas::PlanetAtlas>,
    ) -> World {
        Self::new_with_optional_atlas(seed, save_dir, reg, Some(atlas), false)
    }

    fn new_with_optional_atlas(
        seed: u32,
        save_dir: PathBuf,
        reg: Arc<Registry>,
        planet_atlas: Option<Arc<crate::planet_atlas::PlanetAtlas>>,
        load_authority: bool,
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
        let authority_atlas = if load_authority {
            planet_atlas.as_ref()
        } else {
            None
        };
        let material_ledger = authority_atlas.and_then(|atlas| {
            crate::materials::MaterialLedger::load_or_initialize(&save_dir, atlas, &reg)
                .map_err(|error| {
                    eprintln!("materials: could not open ledger: {error}");
                    error
                })
                .ok()
        });
        let arcane_geography = authority_atlas.and_then(|atlas| {
            #[cfg(test)]
            let opened =
                crate::arcane_geography::ArcaneGeography::load_or_generate(&save_dir, atlas, &reg);
            #[cfg(not(test))]
            let opened = crate::arcane_geography::ArcaneGeography::load(&save_dir, atlas);
            let mut geography = opened
                .map_err(|error| {
                    eprintln!("arcane geography: could not open subledger: {error}");
                    error
                })
                .ok()?;
            if let Err(error) =
                crate::arcane_ecology::reconcile_content(atlas, &reg, &mut geography)
            {
                eprintln!("arcane ecology: could not reconcile content: {error}");
                return None;
            }
            Some(geography)
        });
        let geography_current = arcane_geography
            .as_ref()
            .and_then(|geography| geography.custody_current().ok());
        let arcane_ledger = authority_atlas
            .zip(geography_current)
            .and_then(|(atlas, current)| {
                crate::arcane::ArcaneLedger::load_or_initialize_with_geography(
                    &save_dir, atlas, &reg, current,
                )
                .map_err(|error| {
                    eprintln!("arcane: could not open ledger: {error}");
                    error
                })
                .ok()
            });
        let discovery_state = authority_atlas.and_then(|_| {
            crate::discovery::DiscoveryState::load_or_initialize(&save_dir, seed, reg.content_hash)
                .map_err(|error| {
                    eprintln!("discovery: could not open knowledge state: {error}");
                    error
                })
                .ok()
        });
        let implements_state = authority_atlas.and_then(|_| {
            crate::implements::ImplementsState::load_or_initialize(&save_dir, reg.content_hash)
                .map_err(|error| {
                    eprintln!("implements: could not open state: {error}");
                    error
                })
                .ok()
        });
        let workings_state = authority_atlas.and_then(|_| {
            crate::workings::WorkingsState::load_or_initialize(
                &save_dir,
                reg.content_hash,
                &reg.workings,
            )
            .map_err(|error| {
                eprintln!("workings: could not open transaction state: {error}");
                error
            })
            .ok()
        });
        let alchemy_state = authority_atlas.and_then(|_| {
            crate::alchemy::AlchemyState::load_or_initialize(&save_dir, reg.content_hash)
                .map_err(|error| {
                    eprintln!("alchemy: could not open state: {error}");
                    error
                })
                .ok()
        });
        let water_carriers = authority_atlas.and_then(|_| {
            crate::workings::WaterCarrierState::load_or_initialize(&save_dir)
                .map_err(|error| {
                    eprintln!("workings: could not open detailed water carriers: {error}");
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
            arcane_ledger,
            arcane_geography,
            discovery_state,
            implements_state,
            workings_state,
            alchemy_state,
            water_carriers,
            dross_exposure_tick: HashMap::new(),
            weather_override: None,
            remote_weather_side: 0,
            remote_weather: HashMap::new(),
            remote_arcane_cue: [0; 2],
            remote_arcane_dominant: 0,
            remote_arcane_ecology: None,
            remote_arcane_items: HashMap::new(),
            remote_implements: HashMap::new(),
            remote_apparatus: HashMap::new(),
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
            templates: Vec::new(),
            pending_fills: Vec::new(),
            local_structures: Vec::new(),
            next_local_structure_id: 0,
            pending_drops: Vec::new(),
            perish_accum: 0.0,
            station_work: HashMap::new(),
            belt_state: HashMap::new(),
            regional_ire: HashMap::new(),
            whispers: Vec::new(),
            blessed_streak: HashMap::new(),
            player_touched: HashSet::new(),
            structure_chunks: HashSet::new(),
            gated: HashMap::new(),
            bloom: HashMap::new(),
            hearts: HashMap::new(),
            bloom_spent: HashMap::new(),
            long_winter: false,
            mobs: Vec::new(),
            projectiles: Vec::new(),
            next_projectile_id: 1,
            loose_items: Vec::new(),
            next_loose_item_id: LOOSE_ITEM_ID_BASE,
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
            npcs: Vec::new(),
            next_mob_id: 1,
            #[cfg(test)]
            multiblock_revalidations: 0,
            #[cfg(test)]
            save_fail_chunks: HashSet::new(),
            #[cfg(test)]
            fail_loose_item_save: false,
        }
    }

    pub fn planet_atlas(&self) -> Option<Arc<crate::planet_atlas::PlanetAtlas>> {
        self.planet_atlas.clone()
    }

    /// Test-only: how many instances the 2c edit hook has revalidated.
    #[cfg(test)]
    pub(crate) fn multiblock_revalidations(&self) -> usize {
        self.multiblock_revalidations
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

    pub fn set_remote_arcane_cue(
        &mut self,
        bands: [u8; 2],
        dominant: u8,
        ecology: Option<(String, bool)>,
    ) {
        self.remote_arcane_cue = bands.map(|band| band.min(4));
        self.remote_arcane_dominant = dominant.min(crate::arcane::BASE_RESONANCES.len() as u8);
        self.remote_arcane_ecology = ecology.map(|(text, damped)| {
            let mut text = text;
            text.truncate(240);
            (text, damped)
        });
    }

    pub fn remote_arcane_cue(&self) -> [u8; 2] {
        self.remote_arcane_cue
    }

    pub fn remote_arcane_dominant(&self) -> u8 {
        self.remote_arcane_dominant
    }

    pub fn arcane_cue_at(&self, region: crate::planet_atlas::AtlasPos) -> [u8; 2] {
        let geographic = self
            .arcane_geography
            .as_ref()
            .map_or([0; 2], |geography| geography.local_bands(region));
        let sparse = self
            .arcane_ledger
            .as_ref()
            .map_or([0; 2], |ledger| ledger.local_bands(region));
        [geographic[0].max(sparse[0]), geographic[1].max(sparse[1])]
    }

    /// Complete ordinary-player perception packet. Strength and dross remain
    /// coarse bands, while dominant resonance is a category rather than an
    /// exact mixture or amount.
    pub fn arcane_sensory_cue_at(&self, region: crate::planet_atlas::AtlasPos) -> ([u8; 2], u8) {
        let bands = self.arcane_cue_at(region);
        let dominant = self
            .arcane_geography
            .as_ref()
            .map_or(0, |geography| geography.local_dominant_resonance(region));
        (bands, dominant)
    }

    pub fn arcane_survey_at(
        &self,
        region: crate::planet_atlas::AtlasPos,
        tuning_lens: bool,
    ) -> Option<crate::arcane_geography::ArcaneSurvey> {
        self.arcane_geography
            .as_ref()
            .map(|geography| geography.survey(region, tuning_lens))
    }

    pub fn tick_arcane_geography(&mut self, budget: usize) -> std::io::Result<usize> {
        let Some(geography) = &mut self.arcane_geography else {
            return Ok(0);
        };
        let before = geography.dynamic.completed_steps;
        let processed = geography
            .advance_toward(self.clock.max(0.0) as u64, budget)
            .map_err(std::io::Error::other)?;
        let advanced = geography.dynamic.completed_steps != before;
        if advanced {
            self.pressure_wards_from_arcane_environment();
        }
        Ok(processed)
    }

    /// Once per authoritative Current transport step, translate actual local
    /// wakes and dross custody into ward pressure. The ward consumes its own
    /// supply; it neither deletes the environmental load nor edits Ire.
    fn pressure_wards_from_arcane_environment(&mut self) {
        let Some(atlas) = self.planet_atlas.as_ref() else {
            return;
        };
        let controllers = self
            .workings_state
            .as_ref()
            .into_iter()
            .flat_map(|state| state.active.values())
            .filter_map(|transaction| {
                if transaction.phase != crate::workings::WorkingPhase::Active {
                    return None;
                }
                match &transaction.effect {
                    crate::workings::WorkingEffect::Ward { controller, .. } => Some(*controller),
                    _ => None,
                }
            })
            .collect::<Vec<_>>();
        let pressures = controllers
            .into_iter()
            .filter_map(|controller| {
                let region = atlas.atlas_pos(controller.surface());
                let geography = self.arcane_geography.as_ref()?;
                let cell = geography
                    .dynamic
                    .cells
                    .get(region.index(geography.manifest.side))?;
                let wake = if cell.wake_id != 0 {
                    geography
                        .dynamic
                        .wakes
                        .iter()
                        .find(|wake| wake.id == u64::from(cell.wake_id))
                        .map(|wake| {
                            wake.charge
                                .into_iter()
                                .chain(wake.dross)
                                .map(u64::from)
                                .sum::<u64>()
                        })
                        .unwrap_or_default()
                } else {
                    0
                };
                let sparse_dross = self.arcane_ledger.as_ref().map_or(0, |ledger| {
                    [
                        crate::arcane::DrossMedium::Soil,
                        crate::arcane::DrossMedium::Water,
                        crate::arcane::DrossMedium::Air,
                    ]
                    .into_iter()
                    .filter_map(|medium| {
                        ledger.account(&crate::arcane::ArcaneOwner::Dross { region, medium })
                    })
                    .fold(0u64, |sum, account| {
                        sum.saturating_add(account.current.total())
                    })
                });
                Some((
                    controller,
                    wake,
                    cell.dross_total().saturating_add(sparse_dross),
                ))
            })
            .collect::<Vec<_>>();
        let bounded = |units: u64| {
            if units == 0 {
                0
            } else {
                units.div_ceil(64).clamp(1, 32)
            }
        };
        for (controller, wake, dross) in pressures {
            let wake = bounded(wake);
            if wake != 0 {
                self.resist_supernatural_pressure_at(controller, "wake", wake);
            }
            let dross = bounded(dross);
            if dross != 0 {
                self.resist_supernatural_pressure_at(controller, "dross", dross);
            }
        }
    }

    /// Sliced whole-planet magical succession. Residency never enters the
    /// decision: loaded blocks are a view of these persistent sites.
    pub fn tick_arcane_ecology(&mut self, budget: usize) -> std::io::Result<usize> {
        let Some(atlas) = self.planet_atlas.as_ref().cloned() else {
            return Ok(0);
        };
        let Some(geography) = self.arcane_geography.as_ref() else {
            return Ok(0);
        };
        let site_positions = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .filter_map(|site| site.surface().map(|surface| (site.atlas_pos, surface)))
            .collect::<Vec<_>>();
        let positions = site_positions
            .iter()
            .map(|(pos, _)| *pos)
            .collect::<std::collections::BTreeSet<_>>();
        let storming = site_positions
            .iter()
            .filter(|(_, surface)| {
                self.weather_at_surface(*surface).kind == crate::planet_atlas::LocalWeather::Storm
            })
            .map(|(pos, _)| *pos)
            .collect::<std::collections::BTreeSet<_>>();
        let crystal_candidates = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .filter(|site| {
                self.reg
                    .arcane_ecology
                    .get(&site.content_id)
                    .is_some_and(|definition| {
                        definition.kind == crate::registry::ArcaneEcologyKind::Crystal
                    })
            })
            .filter_map(|site| {
                Some((
                    site.id,
                    site.content_id.clone(),
                    site.materialized_y,
                    site.surface()?,
                    site.block_pos(),
                ))
            })
            .collect::<Vec<_>>();
        let blocked_crystal_sites = crystal_candidates
            .into_iter()
            .filter_map(|(id, content_id, materialized_y, surface, block_pos)| {
                let chunk = crate::planet::ChunkPos::from_surface(surface);
                let blocked = if materialized_y == 0 {
                    self.player_touched.contains(&chunk)
                } else if self.chunks.contains_key(&chunk) {
                    block_pos.is_some_and(|at| {
                        self.reg
                            .block_id(&content_id)
                            .is_none_or(|expected| self.get_block_at(at) != expected)
                    })
                } else {
                    false
                };
                blocked.then_some(id)
            })
            .collect::<std::collections::BTreeSet<_>>();
        let mut water_available = positions
            .into_iter()
            .map(|pos| {
                let available = self
                    .planetary_weather
                    .as_ref()
                    .map_or(u64::MAX / 4, |weather| weather.ecology_soil_water_hu(pos));
                (pos, available)
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut living_hearts = atlas
            .biomes
            .countries
            .iter()
            .map(|country| country.id)
            .collect::<std::collections::BTreeSet<_>>();
        for heart in self.hearts.values().filter(|heart| heart.stage == 0) {
            if let Some(country) = atlas.country_at(heart.pos.surface()) {
                living_hearts.remove(&country.id);
            }
        }
        let geography = self
            .arcane_geography
            .as_mut()
            .expect("ecology geography was checked above");
        let previous_completed_day = geography.dynamic.ecology.completed_days;
        let report = crate::arcane_ecology::advance_toward(
            geography,
            &atlas,
            &self.reg,
            u64::from(self.day),
            budget,
            crate::arcane_ecology::EcologyConditions::new(
                &mut water_available,
                &living_hearts,
                &storming,
                &blocked_crystal_sites,
            ),
        )
        .map_err(std::io::Error::other)?;
        let processed = report.processed;
        let completed_days = report.completed_days;
        let dross_harm = report.dross_harm;
        if let Some(weather) = &mut self.planetary_weather {
            for (pos, requested) in report.transpiration {
                let moved = weather.transpire_ecology(pos, requested);
                if moved != requested {
                    return Err(std::io::Error::other(format!(
                        "ecology water snapshot promised {requested} HU at {pos:?}, moved {moved}"
                    )));
                }
            }
        }
        // Dross arithmetic is not Ire. Only population losses explicitly
        // reported by succession count as habitat damage; source identity
        // remains in the separate bounded dross evidence mixture.
        for (pos, harmed) in dross_harm {
            let center = pos.center(atlas.side());
            let surface = crate::planet::SurfacePos::new(
                center.face,
                center
                    .u
                    .floor()
                    .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1)) as u16,
                center
                    .v
                    .floor()
                    .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1)) as u16,
            )
            .expect("atlas ecology position has a canonical surface center");
            self.add_ire_at_surface(surface, harmed.min(8) as f32 * 0.25);
        }
        if completed_days > previous_completed_day {
            self.refresh_loaded_arcane_ecology();
        }
        Ok(processed)
    }

    pub fn arcane_ecology_observation_at(
        &self,
        surface: crate::planet::SurfacePos,
    ) -> Option<crate::arcane_ecology::EcologyObservation> {
        self.arcane_geography.as_ref().and_then(|geography| {
            crate::arcane_ecology::observation_at(geography, &self.reg, surface)
        })
    }

    /// Persist a fire/explosion/wildlife loss before its representative voxel
    /// is removed. If the process stops after this linked commit, chunk
    /// reconciliation removes the stale block; if it stops before, neither
    /// state changed durably. Crystal charge therefore cannot rematerialize.
    pub(super) fn settle_arcane_ecology_destruction(
        &mut self,
        pos: crate::planet::BlockPos,
    ) -> Result<bool, String> {
        let Some(geography) = self.arcane_geography.as_mut() else {
            return Ok(false);
        };
        let Some(site_index) = geography.dynamic.ecology.sites.iter().position(|site| {
            site.block_pos() == Some(pos)
                && site.stage != crate::arcane_ecology::EcologyStage::Harvested
        }) else {
            return Ok(false);
        };
        let atlas_index = geography.dynamic.ecology.sites[site_index]
            .atlas_pos
            .index(geography.manifest.side);
        let old_site = geography.dynamic.ecology.sites[site_index].clone();
        let old_cell = geography.dynamic.cells[atlas_index];
        let old_sequence = geography.dynamic.ecology.event_sequence;
        let changed = crate::arcane_ecology::apply_destructive_loss(geography, &self.reg, pos)?;
        if !changed {
            return Ok(false);
        }
        let operation_id = geography.dynamic.ecology.event_sequence.max(1);
        let (manifest, files) =
            match geography.linked_dynamic_replacements(&self.save_dir, operation_id) {
                Ok(prepared) => prepared,
                Err(error) => {
                    geography.dynamic.ecology.sites[site_index] = old_site;
                    geography.dynamic.cells[atlas_index] = old_cell;
                    geography.dynamic.ecology.event_sequence = old_sequence;
                    return Err(error.to_string());
                }
            };
        let Some(ledger) = self.arcane_ledger.as_mut() else {
            geography.dynamic.ecology.sites[site_index] = old_site;
            geography.dynamic.cells[atlas_index] = old_cell;
            geography.dynamic.ecology.event_sequence = old_sequence;
            return Err("arcane ecology destruction requires the parent ledger".into());
        };
        if let Err(error) =
            ledger.commit_geography_state_linked("ecological biomass or crystal destroyed", files)
        {
            geography.dynamic.ecology.sites[site_index] = old_site;
            geography.dynamic.cells[atlas_index] = old_cell;
            geography.dynamic.ecology.event_sequence = old_sequence;
            return Err(error.to_string());
        }
        geography.accept_linked_manifest(manifest);
        Ok(true)
    }

    pub fn perceived_arcane_ecology_at(
        &self,
        surface: crate::planet::SurfacePos,
    ) -> Option<crate::arcane_ecology::EcologyObservation> {
        if self.remote {
            return self.remote_arcane_ecology.as_ref().map(|(text, damped)| {
                crate::arcane_ecology::EcologyObservation {
                    text: text.clone(),
                    damped: *damped,
                }
            });
        }
        self.arcane_ecology_observation_at(surface)
    }

    #[cfg(test)]
    pub fn set_remote_arcane_items(&mut self, charges: Vec<(u64, u64)>) {
        self.remote_arcane_items.clear();
        self.remote_arcane_items
            .extend(charges.into_iter().filter(|(id, _)| *id != 0));
    }

    pub fn set_remote_arcane_item(&mut self, id: u64, units: u64) {
        if id != 0 {
            self.remote_arcane_items.insert(id, units);
        }
    }

    pub fn inspectable_item_current(&self, id: u64) -> Option<u64> {
        if id == 0 {
            return None;
        }
        self.arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.item_clean_total(id))
            .or_else(|| self.remote_arcane_items.get(&id).copied())
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
        let pending = std::mem::take(&mut self.pending_drops);
        for (at, stack) in pending {
            self.retire_arcane_stack_at(at, stack, "pending drop administratively cleared");
        }
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

    /// Every destructive item path converges here. Unknown/removed charged
    /// content defaults to regional Dross, retaining both quantity and its
    /// saved content identity instead of deleting an uninspectable account.
    pub fn retire_arcane_stack_at(
        &mut self,
        at: crate::planet::BlockPos,
        stack: ItemStack,
        reason: &str,
    ) -> bool {
        if stack.arcane_id == 0 {
            return false;
        }
        if self
            .alchemy_state
            .as_ref()
            .is_some_and(|state| state.containers.contains_key(&stack.arcane_id))
        {
            if let Err(error) = self.destroy_preparation_container_at(at, stack, reason) {
                eprintln!("alchemy: destructive container settlement failed: {error}");
            }
            // The exact dose sidecar owns both its ingredient and vessel
            // vectors, even if settlement reported a recoverable error. Do
            // not let the generic item-material path double-count either.
            return true;
        }
        if self
            .implements_state
            .as_ref()
            .is_some_and(|state| state.instance(stack.arcane_id).is_some())
        {
            if let Err(error) = self.retire_implement_at(at, stack, reason) {
                eprintln!("implements: destructive item settlement failed: {error}");
            }
            return true;
        }
        let Some(atlas) = &self.planet_atlas else {
            return false;
        };
        let region = atlas.atlas_pos(at.surface());
        let heat_dispersal = reason.contains("lava") || reason.contains("burned in fire");
        let disposition = self
            .reg
            .items
            .get(stack.item.0 as usize)
            .and_then(|definition| definition.arcane.as_ref())
            .map_or(crate::registry::ArcaneDisposition::Dross, |arcane| {
                arcane.on_destroy
            });
        let Some(ledger) = &mut self.arcane_ledger else {
            return false;
        };
        if ledger.item_current_total(stack.arcane_id).is_none() {
            eprintln!(
                "arcane: discarded item {} names missing Current account {}",
                self.reg.item(stack.item).name,
                stack.arcane_id
            );
            return false;
        }
        let destination = match disposition {
            crate::registry::ArcaneDisposition::Ambient => {
                crate::arcane::ArcaneOwner::Ambient(region)
            }
            // A destructive "scar" disposition means severe environmental
            // dross, not permission to mint a bare Scar owner. Only the dross
            // manifestation coordinator may create a Scar(id), after the
            // persisted warning ladder and canonical-site checks.
            crate::registry::ArcaneDisposition::Dross
            | crate::registry::ArcaneDisposition::Scar => crate::arcane::ArcaneOwner::Dross {
                region,
                medium: if heat_dispersal {
                    crate::arcane::DrossMedium::Air
                } else {
                    crate::arcane::DrossMedium::Soil
                },
            },
        };
        if let Err(error) = ledger.move_all_item(stack.arcane_id, destination, reason) {
            eprintln!("arcane: destructive item transfer failed: {error}");
        }
        false
    }

    /// Charge newly discovered/generated content from the finite reserve of
    /// its country (or Deep where no country owns the site).
    pub fn bind_arcane_stack_at(
        &mut self,
        at: crate::planet::BlockPos,
        stack: &mut ItemStack,
        reason: &str,
    ) -> std::io::Result<()> {
        if stack.arcane_id != 0 {
            return Ok(());
        }
        let definition = self
            .reg
            .items
            .get(stack.item.0 as usize)
            .and_then(|item| item.arcane.clone());
        let sealed_dross = self
            .reg
            .item(stack.item)
            .discovery
            .as_ref()
            .and_then(|definition| definition.evidence_class.as_deref())
            == Some("sealed_dross_ampoule");
        let Some(definition) = definition else {
            return Ok(());
        };
        if stack.count != 1 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "charged content must be instantiated as single-item stacks",
            ));
        }
        let source = self
            .planet_atlas
            .as_ref()
            .and_then(|atlas| atlas.country_at(at.surface()).map(|country| country.id))
            .map(crate::arcane::ArcaneOwner::Heart)
            .unwrap_or(crate::arcane::ArcaneOwner::Deep);
        let content_id = self.reg.item(stack.item).name.clone();
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or_else(|| std::io::Error::other("charged content requires an arcane ledger"))?;
        stack.arcane_id = ledger
            .bind_new_item(source, &definition, &content_id, reason)
            .map_err(std::io::Error::other)?;
        if sealed_dross {
            ledger
                .move_all(
                    crate::arcane::ArcaneOwner::Item(stack.arcane_id),
                    crate::arcane::ArcaneOwner::ItemDross(stack.arcane_id),
                    "sealed archaeological dross containment",
                )
                .map_err(std::io::Error::other)?;
        }
        if self.reg.item(stack.item).charm_def.is_some() {
            self.ensure_charm_instance_at(at, stack, reason)
                .map_err(std::io::Error::other)?;
        }
        Ok(())
    }

    /// Roll a block's chance drop on the authoritative simulation stream and
    /// bind magical results before any local entity or network delivery can
    /// observe them.
    pub fn roll_bonus_drop_at(
        &mut self,
        at: crate::planet::BlockPos,
        block: BlockId,
        rng: &mut u32,
    ) -> Option<ItemStack> {
        let (item, chance) = self.reg.block(block).bonus_drop?;
        *rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let roll = (*rng >> 8) as f32 / (1 << 24) as f32;
        if roll >= chance {
            return None;
        }
        let mut stack = ItemStack::new(&self.reg, item, 1);
        if let Err(error) = self.bind_arcane_stack_at(at, &mut stack, "magical bonus harvest") {
            eprintln!("arcane: magical bonus drop cancelled: {error}");
            return None;
        }
        Some(stack)
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
    pub fn fail_loose_item_save_for_test(&mut self, fail: bool) {
        self.fail_loose_item_save = fail;
    }

    #[cfg(test)]
    pub fn mark_structure_chunk_for_test(&mut self, pos: ChunkPos) {
        self.structure_chunks.insert(pos);
        if let Some(chunk) = self.chunks.get_mut(&pos) {
            chunk.modified = true;
        }
    }

    /// Index of the gate sealing `pos`, if any (spec 2.5).
    pub fn gate_at(&self, pos: BlockPos) -> Option<usize> {
        self.gated.get(&pos).copied()
    }

    /// Place a gate's sealed block at `pos` and record it as gated. Used by
    /// the `feature:<id>` marker consumer; unknown gate ids never reach here.
    pub(crate) fn place_gate_at(&mut self, gate: usize, pos: BlockPos) {
        let reg = self.reg.clone();
        let Some(definition) = reg.gates.get(gate) else {
            return;
        };
        self.ensure_chunk(pos.chunk());
        self.set_block_at(pos, definition.block);
        self.gated.insert(pos, gate);
        if let Some(chunk) = self.chunks.get_mut(&pos.chunk()) {
            chunk.modified = true;
        }
    }
    /// Remove a position from the gated registry after it has been unlocked
    /// and replaced (so it never counts again).
    pub(crate) fn ungate_at(&mut self, pos: BlockPos) {
        self.gated.remove(&pos);
    }

    #[cfg(test)]
    /// Whether a position is sealed by a gate (for gate tests).
    pub fn is_gated_for_test(&self, pos: BlockPos) -> bool {
        self.gated.contains_key(&pos)
    }

    #[cfg(test)]
    /// Whether this chunk is claimed by a structure or piece assembly.
    pub fn is_structure_chunk_for_test(&self, pos: ChunkPos) -> bool {
        self.structure_chunks.contains(&pos)
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
    pub fn live_water_audit(&self) -> Option<crate::planet_atlas::WaterAudit> {
        self.planetary_weather
            .as_ref()
            .map(crate::planet_atlas::PlanetaryWeather::water_audit)
    }

    #[cfg(test)]
    pub fn ecology_soil_water_hu_at(&self, surface: crate::planet::SurfacePos) -> Option<u64> {
        let atlas = self.planet_atlas.as_ref()?;
        self.planetary_weather
            .as_ref()
            .map(|weather| weather.ecology_soil_water_hu(atlas.atlas_pos(surface)))
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
        // Spec 2.5: a sealed gate is unbreakable while its position is gated
        // and the gate def requires it. This is the world-level backstop —
        // scripts, commands, and remote hosts cannot bypass the seal even if
        // the UI layer is bypassed. (The interact path opens gates.)
        if let Some(gate) = self.gated.get(&pos).copied()
            && self
                .reg
                .gates
                .get(gate)
                .is_some_and(|g| g.unbreakable_when_locked)
        {
            return None;
        }
        let block = self.get_block_at(pos);
        if block == AIR || self.reg.block(block).hardness.is_none() {
            return None;
        }
        let tool_tier = tool
            .and_then(|item| self.reg.item(item).tool.map(|(_, _, tier)| tier))
            .unwrap_or(0);
        let block_definition = self.reg.block(block);
        let held_tool_kind = tool.and_then(|item| self.reg.item(item).tool.map(|tool| tool.0));
        let alchemy_apparatus = matches!(
            block_definition.interaction.as_deref(),
            Some("alchemy_mortar" | "alchemy_basin" | "alchemy_alembic" | "alchemy_filter")
        );
        let release_alchemy_installation = if alchemy_apparatus {
            let installed = self
                .alchemy_state
                .as_ref()
                .and_then(|state| state.apparatus.get(&pos));
            if installed.is_some_and(|apparatus| {
                apparatus.batch.is_some()
                    || !apparatus.residue_materials.is_empty()
                    || apparatus.filter_burden != 0
                    || apparatus.filter_medium.is_some()
            }) || self
                .alchemy_state
                .as_ref()
                .is_some_and(|state| state.ordinary_jobs.contains_key(&pos))
            {
                // A pick swing cannot orphan conserved liquid, residue,
                // filter, or a timed carrier job. The player must drain and
                // clean it first.
                return None;
            }
            installed.is_some()
        } else {
            false
        };
        let breaking_binding_frame =
            block_definition.interaction.as_deref() == Some("binding_frame");
        let controlled_frame_break = award_drop
            && held_tool_kind == block_definition.tool
            && (!block_definition.requires_tool || tool_tier >= block_definition.min_tier);
        if block_definition.interaction.as_deref() == Some("charge_vessel")
            && (held_tool_kind != block_definition.tool
                || block_definition.requires_tool && tool_tier < block_definition.min_tier)
        {
            // A placed vessel is an embodied container, not a free inventory
            // pickup. The correct dismantling tool returns its exact physical
            // item/identity through the block-entity spill path; bare hands or
            // an undersized tool leave it in place.
            return None;
        }
        let ecology_plan = self.arcane_geography.as_ref().and_then(|geography| {
            crate::arcane_ecology::plan_harvest(geography, &self.reg, pos, tool_tier)
        });
        let ecology_site_present = self.arcane_geography.as_ref().is_some_and(|geography| {
            crate::arcane_ecology::owns_materialized_block(geography, pos)
        });
        let dross_scar_present = self.owns_dross_scar_block(pos);
        // A recovering plant or crystal is real persistent state, not an
        // ordinary loot block. In particular, a second host command must not
        // bypass its harvest cooldown merely because no new plan is ready.
        if award_drop && ecology_site_present && ecology_plan.is_none() {
            return None;
        }
        if ecology_plan.as_ref().is_some_and(|plan| {
            plan.water_hu != 0
                && self.planetary_weather.as_ref().is_some_and(|weather| {
                    let Some(atlas) = self.planet_atlas.as_ref() else {
                        return true;
                    };
                    weather.ecology_soil_water_hu(atlas.atlas_pos(pos.surface())) < plan.water_hu
                })
        }) {
            return None;
        }
        let is_finite_resonant_mineral =
            self.reg
                .block(block)
                .arcane_ecology
                .as_ref()
                .is_some_and(|definition| {
                    definition.kind == crate::registry::ArcaneEcologyKind::FiniteMineral
                });
        let mineral_plan = self
            .reg
            .block(block)
            .arcane_ecology
            .as_ref()
            .filter(|_| is_finite_resonant_mineral)
            .and_then(|definition| {
                self.arcane_geography.as_ref().and_then(|geography| {
                    self.planet_atlas.as_ref().and_then(|atlas| {
                        crate::arcane_ecology::plan_finite_mineral_harvest(
                            geography, atlas, pos, definition,
                        )
                    })
                })
            });
        let mut drop = award_drop
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
        let mut dross_scar_handled = false;
        if dross_scar_present {
            let settled = if award_drop {
                if let Some(stack) = drop.as_mut() {
                    self.excavate_dross_scar(pos, stack)
                } else {
                    self.release_dross_scar(pos)
                }
            } else {
                self.release_dross_scar(pos)
            };
            if let Err(error) = settled {
                eprintln!("dross: scar break cancelled at {pos:?}: {error}");
                cancel_unapplied_material_operation(
                    self.material_ledger.as_ref(),
                    &material_operation,
                );
                return None;
            }
            dross_scar_handled = true;
        }
        if breaking_binding_frame
            && let Err(error) = self.settle_binding_frame_break_at(pos, controlled_frame_break)
        {
            eprintln!("implements: binding-frame break cancelled at {pos:?}: {error}");
            cancel_unapplied_material_operation(self.material_ledger.as_ref(), &material_operation);
            return None;
        }
        let mut arcane_harvest_handled =
            ecology_site_present || is_finite_resonant_mineral || dross_scar_handled;
        let mut leaves_bud = false;
        if award_drop {
            if let (Some(plan), Some(stack)) = (ecology_plan.as_ref(), drop.as_mut()) {
                let Some(geography) = self.arcane_geography.as_mut() else {
                    cancel_unapplied_material_operation(
                        self.material_ledger.as_ref(),
                        &material_operation,
                    );
                    return None;
                };
                let Some(site_index) = geography
                    .dynamic
                    .ecology
                    .sites
                    .iter()
                    .position(|site| site.id == plan.site_id)
                else {
                    cancel_unapplied_material_operation(
                        self.material_ledger.as_ref(),
                        &material_operation,
                    );
                    return None;
                };
                let old_site = geography.dynamic.ecology.sites[site_index].clone();
                let old_sequence = geography.dynamic.ecology.event_sequence;
                let old_exported = geography.dynamic.ecology.exported;
                let old_external_imported = geography.dynamic.dross_state.external_imported;
                if let Err(error) = crate::arcane_ecology::apply_harvest(
                    geography,
                    &self.reg,
                    plan,
                    u64::from(self.day),
                ) {
                    eprintln!("arcane ecology: site changed during harvest at {pos:?}: {error}");
                    cancel_unapplied_material_operation(
                        self.material_ledger.as_ref(),
                        &material_operation,
                    );
                    return None;
                }
                let operation_id = geography.dynamic.ecology.event_sequence.max(1);
                let (manifest, files) = match geography
                    .linked_dynamic_replacements(&self.save_dir, operation_id)
                {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        geography.dynamic.ecology.sites[site_index] = old_site;
                        geography.dynamic.ecology.event_sequence = old_sequence;
                        geography.dynamic.ecology.exported = old_exported;
                        geography.dynamic.dross_state.external_imported = old_external_imported;
                        eprintln!("arcane ecology: could not stage harvest at {pos:?}: {error}");
                        cancel_unapplied_material_operation(
                            self.material_ledger.as_ref(),
                            &material_operation,
                        );
                        return None;
                    }
                };
                let Some(ledger) = self.arcane_ledger.as_mut() else {
                    geography.dynamic.ecology.sites[site_index] = old_site;
                    geography.dynamic.ecology.event_sequence = old_sequence;
                    geography.dynamic.ecology.exported = old_exported;
                    geography.dynamic.dross_state.external_imported = old_external_imported;
                    eprintln!("arcane ecology: harvest cancelled without a ledger");
                    cancel_unapplied_material_operation(
                        self.material_ledger.as_ref(),
                        &material_operation,
                    );
                    return None;
                };
                let charged = !plan.current.is_empty() || !plan.dross_current.is_empty();
                let committed = if charged {
                    ledger
                        .bind_new_item_exact_linked(
                            crate::arcane::ArcaneOwner::Geography,
                            plan.current.clone(),
                            plan.dross_current.clone(),
                            &plan.item_content,
                            "ecological harvest",
                            files,
                        )
                        .map(Some)
                } else {
                    ledger
                        .commit_geography_state_linked("uncharged ecological harvest", files)
                        .map(|()| None)
                };
                match committed {
                    Ok(item_id) => {
                        if let Some(item_id) = item_id {
                            stack.arcane_id = item_id;
                        }
                        geography.accept_linked_manifest(manifest);
                    }
                    Err(error) => {
                        geography.dynamic.ecology.sites[site_index] = old_site;
                        geography.dynamic.ecology.event_sequence = old_sequence;
                        geography.dynamic.ecology.exported = old_exported;
                        geography.dynamic.dross_state.external_imported = old_external_imported;
                        eprintln!("arcane ecology: harvest cancelled at {pos:?}: {error}");
                        cancel_unapplied_material_operation(
                            self.material_ledger.as_ref(),
                            &material_operation,
                        );
                        return None;
                    }
                }
                leaves_bud = plan.leaves_bud;
                if plan.water_hu != 0
                    && let (Some(weather), Some(atlas)) =
                        (self.planetary_weather.as_mut(), self.planet_atlas.as_ref())
                {
                    let moved = weather
                        .harvest_ecology_water(atlas.atlas_pos(pos.surface()), plan.water_hu);
                    debug_assert_eq!(moved, plan.water_hu, "prechecked dew water changed");
                }
            } else if let (Some(current), Some(stack)) = (mineral_plan.as_ref(), drop.as_mut()) {
                let content_id = self.reg.item(stack.item).name.clone();
                let (Some(geography), Some(atlas)) =
                    (self.arcane_geography.as_mut(), self.planet_atlas.as_ref())
                else {
                    cancel_unapplied_material_operation(
                        self.material_ledger.as_ref(),
                        &material_operation,
                    );
                    return None;
                };
                let atlas_index = atlas.atlas_pos(pos.surface()).index(atlas.side());
                let old_cell = geography.dynamic.cells[atlas_index];
                let old_sequence = geography.dynamic.ecology.event_sequence;
                let old_exported = geography.dynamic.ecology.exported;
                let old_external_imported = geography.dynamic.dross_state.external_imported;
                if let Err(error) = crate::arcane_ecology::apply_finite_mineral_harvest(
                    geography, atlas, pos, current,
                ) {
                    eprintln!("arcane ecology: mineral charge changed at {pos:?}: {error}");
                    cancel_unapplied_material_operation(
                        self.material_ledger.as_ref(),
                        &material_operation,
                    );
                    return None;
                }
                geography.dynamic.ecology.event_sequence = old_sequence.saturating_add(1);
                let operation_id = geography.dynamic.ecology.event_sequence.max(1);
                let (manifest, files) =
                    match geography.linked_dynamic_replacements(&self.save_dir, operation_id) {
                        Ok(prepared) => prepared,
                        Err(error) => {
                            geography.dynamic.cells[atlas_index] = old_cell;
                            geography.dynamic.ecology.event_sequence = old_sequence;
                            geography.dynamic.ecology.exported = old_exported;
                            geography.dynamic.dross_state.external_imported = old_external_imported;
                            eprintln!("arcane ecology: could not stage mineral harvest: {error}");
                            cancel_unapplied_material_operation(
                                self.material_ledger.as_ref(),
                                &material_operation,
                            );
                            return None;
                        }
                    };
                let Some(ledger) = self.arcane_ledger.as_mut() else {
                    geography.dynamic.cells[atlas_index] = old_cell;
                    geography.dynamic.ecology.event_sequence = old_sequence;
                    geography.dynamic.ecology.exported = old_exported;
                    geography.dynamic.dross_state.external_imported = old_external_imported;
                    cancel_unapplied_material_operation(
                        self.material_ledger.as_ref(),
                        &material_operation,
                    );
                    return None;
                };
                match ledger.bind_new_item_exact_linked(
                    crate::arcane::ArcaneOwner::Geography,
                    current.clone(),
                    crate::arcane::Current::default(),
                    &content_id,
                    "finite resonant mineral harvest",
                    files,
                ) {
                    Ok(id) => {
                        stack.arcane_id = id;
                        geography.accept_linked_manifest(manifest);
                    }
                    Err(error) => {
                        geography.dynamic.cells[atlas_index] = old_cell;
                        geography.dynamic.ecology.event_sequence = old_sequence;
                        geography.dynamic.ecology.exported = old_exported;
                        geography.dynamic.dross_state.external_imported = old_external_imported;
                        eprintln!("arcane ecology: mineral harvest cancelled at {pos:?}: {error}");
                        cancel_unapplied_material_operation(
                            self.material_ledger.as_ref(),
                            &material_operation,
                        );
                        return None;
                    }
                }
            }
        }
        if ecology_site_present && (!award_drop || drop.is_none()) {
            if let Err(error) = self.settle_arcane_ecology_destruction(pos) {
                eprintln!("arcane ecology: destructive loss cancelled at {pos:?}: {error}");
                cancel_unapplied_material_operation(
                    self.material_ledger.as_ref(),
                    &material_operation,
                );
                return None;
            }
            arcane_harvest_handled = true;
        }
        self.player_touched.insert(pos.chunk());
        if affect_ire {
            let mut cost = self.ire_for_block(block);
            if let Some(plan) = &ecology_plan {
                cost += if plan.destructive { 2.0 } else { 0.35 };
                if plan.protected {
                    cost *= 0.5;
                }
            }
            self.add_ire_at_surface(pos.surface(), cost);
        }
        let was_heart = self.reg.block(block).name.starts_with("base:heart_");
        if release_alchemy_installation && let Some(state) = &mut self.alchemy_state {
            // A clean empty installation has no conserved contents. Its
            // sidecar identity is released with the ordinary block edit.
            state.apparatus.remove(&pos);
        }
        self.set_block_at(pos, if leaves_bud { block } else { AIR });
        if let Some(stack) = &mut drop
            && self.reg.item(stack.item).arcane.is_some()
            && !arcane_harvest_handled
            && let Err(error) = self.bind_arcane_stack_at(pos, stack, "magical harvest")
        {
            eprintln!("arcane: magical harvest drop cancelled: {error}");
            drop = None;
        }
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
                self.block_entities.entry(pos).or_insert_with(|| {
                    BlockEntity::Multiblock(MachineInstance {
                        kind: crate::world::multiblock::MachineKind::Separator,
                        ..Default::default()
                    })
                });
            }
            Some("discovery_lab") => {
                self.block_entities
                    .entry(pos)
                    .or_insert_with(|| BlockEntity::DiscoveryApparatus(Default::default()));
            }
            Some("binding_frame") => {
                self.block_entities
                    .entry(pos)
                    .or_insert_with(|| BlockEntity::BindingFrame(Default::default()));
            }
            _ => {}
        }
        true
    }

    /// Place the block carried by one inventory instance. Charged placeables
    /// discharge into their declared local environmental reservoir when the
    /// physical item becomes a block; no item owner is left behind for a
    /// later save-recovery pass to clean up.
    pub fn place_item_block_at(&mut self, pos: BlockPos, stack: ItemStack) -> bool {
        let returns_ecology_water = self.reg.item(stack.item).name == "base:rainbell_dew";
        let Some(block) = self.reg.item(stack.item).places else {
            return false;
        };
        if !self.place_block_at(pos, block) {
            return false;
        }
        if self
            .reg
            .item(stack.item)
            .implement
            .as_ref()
            .is_some_and(|definition| {
                definition.kind == crate::implements::ImplementItemKind::ChargeVessel
            })
        {
            if stack.count != 1 {
                self.set_block_at(pos, AIR);
                return false;
            }
            self.block_entities.insert(
                pos,
                BlockEntity::ChargeVessel(ChargeVesselState {
                    vessel: Some(ItemStack { count: 1, ..stack }),
                    damage: 0,
                    revision: 0,
                }),
            );
            return true;
        }
        if self
            .reg
            .item(stack.item)
            .discovery
            .as_ref()
            .is_some_and(|definition| definition.kind == "survey_folio")
        {
            let mut physical = ItemStack { count: 1, ..stack };
            if let Err(error) = self.bind_discovery_stack_at(pos, &mut physical) {
                eprintln!("discovery: survey folio placement cancelled: {error}");
                self.set_block_at(pos, AIR);
                return false;
            }
            self.block_entities.insert(
                pos,
                BlockEntity::SurveyFolio(SurveyFolioState {
                    object_id: physical.arcane_id,
                }),
            );
            return true;
        }
        if let Some(definition) = self.reg.block(block).arcane_ecology.clone()
            && definition.kind != crate::registry::ArcaneEcologyKind::FiniteMineral
            && let (Some(atlas), Some(geography)) =
                (self.planet_atlas.as_ref(), self.arcane_geography.as_mut())
        {
            let content_id = &self.reg.block(block).name;
            let restored =
                crate::arcane_ecology::restore_with_seed(geography, &self.reg, content_id, pos);
            if !restored
                && let Err(error) = crate::arcane_ecology::register_cultivated(
                    geography,
                    atlas,
                    content_id,
                    pos,
                    &definition,
                )
            {
                eprintln!("arcane ecology: cultivation cancelled at {pos:?}: {error}");
                self.set_block_at(pos, AIR);
                return false;
            }
            if restored {
                self.plant_ire_at_surface(pos.surface(), 0.35);
            }
        }
        self.refresh_loaded_arcane_ecology();
        if returns_ecology_water
            && let (Some(atlas), Some(weather)) =
                (self.planet_atlas.as_ref(), self.planetary_weather.as_mut())
        {
            weather.return_ecology_water_to_soil(
                atlas.atlas_pos(pos.surface()),
                crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL,
            );
        }
        self.retire_arcane_stack_at(
            pos,
            ItemStack { count: 1, ..stack },
            "charged block placed into the environment",
        );
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

    pub(crate) fn block_entity_stacks(entity: &BlockEntity) -> Vec<ItemStack> {
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
            BlockEntity::Multiblock(state) => {
                add(&state.charge);
                add(&[state.reagent]);
                add(&state.fuel);
            }
            BlockEntity::Anvil(state) => add(&[state.bloom]),
            BlockEntity::Stall(state) => {
                add(&state.goods);
                add(&[state.price]);
                add(&state.till);
            }
            BlockEntity::Smoker(state) => add(&state.meat),
            BlockEntity::DiscoveryApparatus(state) => add(&[state.sample, state.reference]),
            BlockEntity::BindingFrame(state) => {
                add(&state.mounts());
                add(&[state.output]);
            }
            BlockEntity::ChargeVessel(state) => add(&[state.vessel]),
            BlockEntity::Clamp(_)
            | BlockEntity::Sign(_)
            | BlockEntity::Steam(_)
            | BlockEntity::SurveyFolio(_)
            | BlockEntity::Switch(_) => {}
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
        if let BlockEntity::Multiblock(state) = entity {
            let Some(ledger) = &mut self.material_ledger else {
                return Ok(());
            };
            ledger.record_external_materials(&state.reclaim, true, source)?;
            let counts = [
                ("base:rare_earth_powder", state.powder),
                ("base:charcoal", state.separator_fuel),
                ("base:neodymium", state.neodymium),
                ("base:cerium", state.cerium),
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
        if let BlockEntity::Multiblock(state) = entity {
            let Some(ledger) = &mut self.material_ledger else {
                return Ok(());
            };
            ledger.record_admin_secondary_deletion(&state.reclaim)?;
            let counts = [
                ("base:rare_earth_powder", state.powder),
                ("base:charcoal", state.separator_fuel),
                ("base:neodymium", state.neodymium),
                ("base:cerium", state.cerium),
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
        let old_holds_water_carrier =
            self.reg.is_water(old) || self.reg.block(old).name == "base:ice";
        let new_holds_water_carrier =
            self.reg.is_water(block) || self.reg.block(block).name == "base:ice";
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
        if old_holds_water_carrier
            && !new_holds_water_carrier
            && let Some(carriers) = self.water_carriers.as_mut()
        {
            // The regional arcane ledger remains authoritative for the dross;
            // this only retires a no-longer-physical per-voxel allocation.
            // Ice deliberately retains the allocation for exact thawing.
            carriers.cells.remove(&pos);
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

        // Changing material identity invalidates the machine living here.
        // Metadata is ordinary state on the same physical block (crop stage,
        // mechanism latch, water level) and must not silently delete its
        // embodied block entity.
        if old != block
            && let Some(entity) = self.block_entities.remove(&pos)
        {
            let spilled: Vec<ItemStack> = match entity {
                BlockEntity::Furnace(f) => {
                    [f.input, f.fuel, f.output].into_iter().flatten().collect()
                }
                BlockEntity::Chest(c) => c.slots.into_iter().flatten().collect(),
                BlockEntity::Offering(o) => o.slots.into_iter().flatten().collect(),
                BlockEntity::Multiblock(m) => {
                    let reg = &self.reg;
                    let mut stacks: Vec<ItemStack> =
                        m.charge.into_iter().chain(m.fuel).flatten().collect();
                    if let Some(r) = m.reagent {
                        stacks.push(r);
                    }
                    let mut push = |name: &str, count: u32| {
                        if count > 0
                            && let Some(item) = reg.item_id(name)
                        {
                            let mut stack = ItemStack::new(reg, item, 1);
                            stack.count = count;
                            stacks.push(stack);
                        }
                    };
                    if !m.reclaim.is_empty()
                        && let Some(ledger) = &mut self.material_ledger
                        && let Err(error) = ledger.bury_materials(
                            pos,
                            &m.reclaim,
                            "machine dismantled with fractional recovered stock",
                        )
                    {
                        eprintln!("materials: machine stock salvage failed: {error}");
                    }
                    push("base:rare_earth_powder", m.powder);
                    push("base:charcoal", m.separator_fuel);
                    push("base:neodymium", m.neodymium);
                    push("base:cerium", m.cerium);
                    stacks
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
                BlockEntity::SurveyFolio(folio) => self
                    .reg
                    .item_id("base:survey_folio")
                    .map(|item| {
                        let mut stack = ItemStack::new(&self.reg, item, 1);
                        stack.arcane_id = folio.object_id;
                        vec![stack]
                    })
                    .unwrap_or_default(),
                BlockEntity::DiscoveryApparatus(apparatus) => {
                    [apparatus.sample, apparatus.reference]
                        .into_iter()
                        .flatten()
                        .collect()
                }
                BlockEntity::BindingFrame(frame) => frame
                    .mounts()
                    .into_iter()
                    .chain([frame.output])
                    .flatten()
                    .collect(),
                BlockEntity::ChargeVessel(vessel) => vessel.vessel.into_iter().collect(),
                BlockEntity::Switch(_) => Vec::new(),
            };
            for stack in spilled {
                self.push_drop_at(pos, stack);
            }
        }

        // Event-driven multiblock revalidation (Phase 2, spec Part 1.2's
        // scaling note): a real block change anywhere within a registered
        // instance's shell region re-runs that instance's shape match and
        // re-folds its stats immediately — no dependence on the machine
        // being lit or ticked. Water and meta-only edits keep `old ==
        // block` and skip this; remote replicas let the host decide.
        if !self.remote && old != block {
            // A ghost overlay (spec Part 1.4) is satisfied cell-by-cell by
            // ordinary placement: this is the only hook, and it clears a
            // pending cell exactly when the voxel holds the required block.
            // Any other write is an ordinary edit and leaves the fill alone.
            self.clear_pending_fill_at(pos, block);
            self.revalidate_multiblocks_around(pos);
        }
    }

    /// Revalidate every registered multiblock instance whose shell region
    /// could contain the edited position. The per-instance test is O(1)
    /// arithmetic ([`crate::world::multiblock::pos_within_extent`]); only
    /// instances actually in range re-run their shape match. Delegates to
    /// the store-generic hook shared with structure-hosted machines.
    pub(super) fn revalidate_multiblocks_around(&mut self, pos: BlockPos) {
        let revalidated = machines::revalidate_machines_around(self, pos);
        #[cfg(not(test))]
        let _ = revalidated;
        #[cfg(test)]
        {
            self.multiblock_revalidations += revalidated;
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

    /// Apply one network poll's authoritative block states as a single light
    /// transaction. Hosts often deliver many weather, fluid, or ecology edits
    /// together; relighting the connected view after every individual message
    /// can monopolize a guest for minutes even though the final voxel state is
    /// identical. The ordinary typed mutation path still owns support checks,
    /// metadata, salinity, dirty meshes, and fluid wakeups.
    pub(crate) fn apply_remote_block_states(
        &mut self,
        updates: impl IntoIterator<Item = (BlockPos, BlockId, u8, u16, u8)>,
    ) {
        debug_assert!(self.remote, "only a guest may apply replicated states");
        self.edit_batch(|world| {
            for (pos, block, meta, salt_mass, soil_salinity) in updates {
                world.set_block_state_at(pos, block, meta, salt_mass, soil_salinity);
            }
        });
        self.clear_pending_drops();
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
