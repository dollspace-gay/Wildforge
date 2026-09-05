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
mod block_store;
mod calendar;
mod calendar_view;
mod chunks;
mod discovery;
pub(crate) use discovery::ObservationTarget;
mod dross;
mod ecology;
pub use ecology::SettledMobDeath;
pub(crate) mod belt;
pub(crate) mod dungeon;
mod entities;
mod fire;
mod fluids;
mod hearts;
mod implements;
mod item_presentation;
mod lighting;
mod machine_tick;
pub(crate) mod machines;
pub(crate) mod multiblock;
mod persistence;
pub(crate) mod pieces;
mod power;
mod query;
mod standing;
mod country_view;
mod calendar_state;
mod weather_state;
mod installations;
mod construction;
mod population;
mod view;
pub(crate) use view::WorldView;
mod replica;
mod replication;
pub(crate) use replica::ReplicaWorld;
pub(crate) use replication::ReplicationTarget;
mod observations;
pub(crate) use observations::ReplicaObservations;
pub use query::{SceneRead, TerrainRead};
pub(crate) mod power_draw;
mod preparation;
#[cfg_attr(test, allow(unused))]
#[path = "storage/region.rs"]
pub(crate) mod region;
mod storage;

pub use hearts::ROOT_READY_FRAC;
#[cfg(test)]
pub use hearts::{HEART_CUTTING_DAYS, HEART_DEATH_STRAIN, HEART_SICKEN_STRAIN, ROOT_DAYS};
pub use hearts::{Heart, heart_block_name, seed_nature, seed_of_form};
pub use crate::worldgen::{heart_form, heart_height};
pub use machines::{station_powered, worked_table_for};
pub mod soil;
mod spawn;
pub(crate) use spawn::player_entry_chunks;
pub(crate) use storage::{ChunkLoader, ChunkRead, ChunkRevision, encode_stream_chunk};
pub(crate) mod local_structure;
pub(crate) mod rail;
pub(crate) mod template;
mod ticks;
mod terrain;
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

mod save_reports;
pub use save_reports::{SaveFailure, SaveReport, ResidencyReport};

mod block_entities;
pub use block_entities::{BlockEntity, SwitchState, SurveyFolioState, DiscoveryApparatusState, BindingFrameState, ChargeVesselState, MachineInstance, SEPARATE_SECS, KILN_FIRE_SECS, ELEC_RADIUS, SteamState, STEAM_SECS_PER_WATER, STEAM_FUEL_CAP, STEAM_RATE, SmokerState, SMOKE_SECS, StallState, DepotState, SignState, ClampState, AnvilState, FallingBlock, BLOOMERY_FIRE_SECS, FORGE_FIRE_SECS, FORGE_ITEMS_PER_FUEL, CLAMP_SECS_PER_LOG, STATION_STRIKE_SECS, HELVE_STRIKE_SECS, PUMP_STROKE_SECS, PUMP_REACH, OfferingState, CHEST_SLOTS, ChestState, FurnaceState};

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

mod world_metadata;
use world_metadata::MIN_SUPPORTED_WORLD_GENERATOR_VERSION;
pub use world_metadata::{WORLD_GENERATOR_VERSION, WorldMeta, load_world_meta, read_world_meta, read_world_meta_full, write_world_meta, write_world_meta_full, list_worlds, WorldBrowserEntry, inspect_worlds};
pub use crate::planet::WORLD_TOPOLOGY;

mod creation;
pub use creation::{WorldCreationProgress, create_world_atomic};
pub(crate) use creation::{create_qualification_world_from_atlas};
#[cfg(test)]
pub use creation::create_world_fixture_atomic;

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

    /// A representative surface position inside this cell — its center —
    /// for charging ire when only the cell is known (capability E12).
    pub fn any_surface(self) -> Option<crate::planet::SurfacePos> {
        crate::planet::SurfacePos::new(
            self.face,
            u16::from(self.u) * Self::BLOCKS + Self::BLOCKS / 2,
            u16::from(self.v) * Self::BLOCKS + Self::BLOCKS / 2,
        )
        .ok()
    }
}

/// A settlement hidden cell's reveal key (spec 3.4): which settlement and
/// which tier of that settlement reveals this position. The world stores
/// positions -> key so `reveal_settlement` can find every cell of a tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RevealKey {
    /// Index into `Registry::settlements`.
    pub settlement: usize,
    /// The settlement tier (>= 2) that reveals the cell.
    pub tier: u32,
}

pub struct World {
    construction: construction::Construction,
    calendar_state: calendar_state::CalendarState,
    population: population::Population,
    chunks: terrain::TerrainStore,
    pub generator: Generator,
    planet_atlas: Option<Arc<crate::planet_atlas::PlanetAtlas>>,
    weather_state: weather_state::WeatherState,
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
    pub reg: Arc<Registry>,
    #[allow(dead_code)]
    pub seed: u32,
    save_dir: PathBuf,
    region_store: storage::RegionStore,
    palette: storage::PaletteStore,
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
    /// Multiblock revalidations triggered by block edits since construction.
    /// Test-only: proves the 2c edit hook is scoped, not global.
    #[cfg(test)]
    multiblock_revalidations: usize,
    /// Belt cells carrying cargo (spec §2.2). Persisted as `[[belt]]`
    /// records in `entities.toml` (capability E8), so a reloaded line
    /// resumes with its cargo, progress, entry direction, and splitter
    /// phase instead of starting empty.
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
    /// Nest/dens spawn-gates (capability E9): world position -> nest index
    /// into the registry. The record lives exactly while its marker block
    /// does: breaking the nest removes it, and the species stops respawning
    /// nearby. Persisted like `gated`.
    nests: HashMap<BlockPos, usize>,
    /// Per-nest spawn cadence (capability E9): last-spawn clock per nest
    /// position. Transient — the 4-second spawn cycle already paces spawns,
    /// so a lost cadence across a restart costs at most one interval.
    nest_spawn_cd: HashMap<BlockPos, f32>,
    /// Settlement hidden cells (spec 3.4): world position -> the settlement
    /// tier that reveals it. Tier-2+ pieces are placed at worldgen but behave
    /// as air until the player's reputation crosses the tier threshold.
    /// Persisted so a save never depends on regeneration to know what is
    /// hidden.
    hidden: HashMap<BlockPos, RevealKey>,
    /// Bloom ledger: days of post-wrath eruption left per 256-cell.
    pub(crate) bloom: HashMap<RegionCell, f32>,
    /// The spirits of the land, keyed by province.
    pub(crate) hearts: HashMap<crate::worldgen::ProvinceKey, Heart>,
    /// How much bloom a cell has already been given without being
    /// tended back — the ground's willingness, spent.
    pub(crate) bloom_spent: HashMap<RegionCell, f32>,
    /// When each chunk last took its random ticks (persisted, so the
    /// world can live on while a chunk is away).
    last_random: HashMap<ChunkPos, f64>,
    installations: installations::Installations,
    /// Items spilled by removed block entities, for the game loop to spawn.
    pending_drops: Vec<(crate::planet::BlockPos, ItemStack)>,
    /// The entry-piece anchor of the most recent `place_assembly` that
    /// placed anything (capability E10): a dungeon run reads it to know
    /// where to stand its participants. Transient, single-threaded.
    pub(crate) last_entry_anchor: Option<BlockPos>,
    /// Live instanced dungeon runs (capability E10), keyed by their Deep
    /// slot. Runs are session state: their chunks never persist.
    pub dungeon_runs: Vec<crate::world::dungeon::DungeonRun>,
    /// Per-run generation seed drift so two visits to one dungeon differ.
    run_seed: u32,
    /// Game mode string, persisted in world.toml alongside seed/ire.
    pub mode: String,
    /// The client camera mode chosen for this world (`first`/`third`/`orbit`),
    /// persisted in world.toml. Host-authoritative so a world carries its view.
    pub camera: String,
    /// The wild's ire 0..100 — reciprocity meter driving hostile spawns.
    pub ire: f32,
    /// How much ire planting has already refunded today (daily cap).
    plant_ire_today: f32,
    /// Host mode: record block edits for broadcasting.
    log_edits: bool,
    edit_log: Vec<(crate::planet::BlockPos, BlockId, u8, u16, u8)>,
    /// Gravity blocks currently airborne.
    falling: Vec<FallingBlock>,
    /// (guest id, stack) owed over the wire: mining drops, kill loot,
    /// recovered arrows, brush finds — full stacks so durability rides.
    pending_gives: Vec<(u32, ItemStack)>,
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


mod initialization;
mod arcane_environment;
mod arcane_context;
mod ecology_custody;
mod item_custody;
mod world_events;
mod installation_access;
mod residency;
mod feature_access;
mod voxel_access;
mod mining;
mod placement;
mod material_transactions;
mod block_edits;
mod spawn_rescue;
mod lunar_observation;
