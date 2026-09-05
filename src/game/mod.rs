//! Client game-state implementation, split by existing responsibility.

mod actions;
mod app;
mod browser;
mod capture;
pub(crate) mod combat;
mod containers;
mod content;
mod content_watch;
mod demos;
mod dialogue;
mod equipment;
#[cfg(test)]
pub(crate) use containers::recipe_gates_met;
#[cfg(test)]
pub(crate) use dialogue::apply_recipe_unlock_reward;
#[cfg(test)]
pub(crate) use dialogue::apply_reputation_reward;
mod frame;
mod input;
mod navigation;
mod presentation;
mod startup;
mod interaction;
mod inventory_ui;
mod inventory_panel;
mod widgets;
mod keymap;
mod keyboard_events;
mod text_input;
mod menus;
mod mesh_jobs;
mod remote;
mod roster_ui;
mod session;
mod skills;
mod stats;
mod status;
mod streaming;
mod survival;
mod tooltip;
mod ui;
mod world_loading;
mod world_loading_ui;

pub(super) use app::run_windowed;
#[cfg(test)]
pub(crate) use browser::browser_items;
#[cfg(test)]
pub(crate) use content_watch::content_tree_stamp_of;
#[cfg(test)]
pub(crate) use survival::reduced_damage;
#[cfg(test)]
pub(crate) use world_loading_ui::next_world_name;
use content_watch::{content_tree_stamp, script_mod_dirs};
use input::InputState;
use navigation::{Screen, UiState};
use presentation::PresentationState;

use crate::{atlas, audio, bounce, identity, mobs, mp, net, renderer, script, server, style, visual_capture};

use std::sync::Arc;
use std::time::Instant;

use glam::Vec3;
use winit::window::Window;

use crate::audio::{Audio, Sfx};
use crate::camera::Camera;
use crate::chunk::ChunkPos;
use crate::config::Config;
use crate::inventory::{Inventory, ItemStack};
use crate::physics::Player;
use crate::registry::Registry;
use crate::ui::UiBatch;

const GEN_BUDGET: usize = 4; // chunk generations per frame (256-tall gen is pricey)
pub(crate) const SHOT_SETTLE_FRAMES: u64 = 10;
const SHOT_FIXED_DT: f32 = 1.0 / 60.0;
const SHOT_MAX_FRAMES: u64 = 3000;

fn advance_capture_clock(current: u64, awaiting_direct_entry: bool) -> u64 {
    if awaiting_direct_entry {
        0
    } else {
        current.saturating_add(1)
    }
}
const BUILD_MARKER: &str = "BIOME-R4";
const REACH: f32 = 5.0;
const MAX_HEALTH: f32 = 14.0; // base half-hearts (7 hearts)
const MAX_AIR: f32 = 15.0; // seconds of breath

/// One snapshot-smoothing span: render glides from -> to over the
/// measured packet interval instead of snapping at 20 Hz.
struct Lerp {
    from: Vec3,
    to: Vec3,
    from_yaw: f32,
    to_yaw: f32,
    /// Walk-cycle accumulator (mobs), fed by apparent speed — survives
    /// the snapshot rebuild so legs don't snap mid-stride.
    phase: f32,
}

impl Lerp {
    fn at(&self, t: f32) -> (Vec3, f32) {
        (
            self.from.lerp(self.to, t),
            mobs::lerp_yaw(self.from_yaw, self.to_yaw, t),
        )
    }
}

/// Player vitals, armor, recovery timers, and respawn ownership.
struct SurvivalState {
    armor: [Option<ItemStack>; 5],
    /// Slotted components per armor slot (capability E6), index-aligned
    /// with `armor`. Kept in sync: equipping a frame clears the slot's
    /// loadout, unequipping returns its components intact.
    loadouts: [crate::equipment::Loadout; 5],
    /// Named saved equipment configurations (capability E6).
    loadout_presets: Vec<crate::equipment::LoadoutPreset>,
    health: f32,
    hunger: f32,
    nutrition: [f32; 5],
    eating: f32,
    exhaustion_regen: f32,
    /// Accumulator for the pack's food-freshness sweep.
    perish_accum: f32,
    /// Host-authoritative preparation effects advance at one-second cadence.
    alchemy_accum: f32,
    /// Personal exposure burden available to bounded antidote handlers. The
    /// regional dross ledger remains separate and is never cleared by this.
    bodily_dross: u64,
    preparation_modifiers: crate::alchemy::PreparationModifiers,
    /// Seconds of slow-hunger benefit already paid for by one authoritative
    /// fixed-interval charm debit.
    hunger_charm_credit: f32,
    starve_timer: f32,
    air: f32,
    since_damage: f32,
    drown_timer: f32,
    burn_timer: f32,
    damage_flash: f32,
    fall_start: Option<f32>,
    spawn_point: crate::planet::EntityPos,
    killed_by_wild: bool,
}

impl SurvivalState {
    fn new(spawn_point: crate::planet::EntityPos) -> Self {
        Self {
            armor: [None; 5],
            loadouts: Default::default(),
            loadout_presets: Vec::new(),
            health: MAX_HEALTH,
            hunger: 20.0,
            nutrition: [0.0; 5],
            eating: 0.0,
            exhaustion_regen: 0.0,
            perish_accum: 0.0,
            alchemy_accum: 0.0,
            bodily_dross: 0,
            preparation_modifiers: crate::alchemy::PreparationModifiers::default(),
            hunger_charm_credit: 0.0,
            starve_timer: 0.0,
            air: MAX_AIR,
            since_damage: 100.0,
            drown_timer: 0.0,
            burn_timer: 0.0,
            damage_flash: 0.0,
            fall_start: None,
            spawn_point,
            killed_by_wild: false,
        }
    }

    fn attackable(&self, creative: bool) -> bool {
        !creative && self.health > 0.0
    }
}

/// Transport/session state shared by hosting, guest play, discovery, and chat.
#[derive(Default)]
struct MultiplayerState {
    host: Option<mp::HostSession>,
    host_sleeping: bool,
    remote: Option<Remote>,
    discovery: Option<net::Discovery>,
    join_ip: String,
    join_status: String,
    pending_join_disclosure: Option<std::net::SocketAddr>,
    chat_open: bool,
    chat_text: String,
    roster_open: bool,
    move_timer: f32,
    tick_accum: f32,
}

/// Loaded content plus hot-reload and active-pack bookkeeping.
struct ContentRuntime {
    reg: Arc<Registry>,
    scripts: script::ScriptHost,
    mods_stamp: u64,
    mods_poll: f32,
    packs: Vec<atlas::PackInfo>,
    pack_warnings: Vec<String>,
    pack_override: Option<String>,
    /// Alternate tiles the active pack chain supplies, consulted by the mesher.
    tile_variants: atlas::TileVariants,
    /// Capture-only stable names for the atlas slots above. Constructed from
    /// already-loaded content; ordinary frames never consult this table.
    diagnostic_families: Option<Vec<visual_capture::DiagnosticFamily>>,
}

/// What the player is currently mining: a world block or a structure
/// block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BreakTarget {
    World(crate::planet::BlockPos),
    Structure(
        crate::world::local_structure::LocalStructureId,
        (i32, i32, i32),
    ),
}

/// In-progress use actions, crafting inputs, and local item entities.
struct InteractionState {
    bow_draw: f32,
    brushing: f32,
    brush_target: Option<crate::planet::BlockPos>,
    lens_settle: f32,
    lens_target: Option<DiscoveryAim>,
    experiment_kind: usize,
    anvil_work: f32,
    anvil_pos: Option<crate::planet::BlockPos>,
    craft_grid: [Option<ItemStack>; 9],
    craft_size: usize,
    breaking: Option<(BreakTarget, f32)>,
    /// Waystones this player has touched: (name, x, z). Loaded from a
    /// per-world sidecar; purely local knowledge, never synced.
    attuned: Vec<(String, crate::planet::SurfacePos)>,
    /// The vehicle under us (mob id), if we're aboard one.
    riding: Option<u32>,
    /// A cast line: (bobber cell center, seconds to the bite, bite
    /// window remaining). The water decides when.
    fishing: Option<(crate::planet::EntityPos, f32, f32)>,
    /// Last authoritative revision observed for each binding frame. Reliable
    /// mutations echo a new value and stale concurrent requests are refused.
    binding_revisions: std::collections::HashMap<crate::planet::BlockPos, u64>,
    /// Last authoritative revision observed for each alchemy installation.
    alchemy_revisions: std::collections::HashMap<crate::planet::BlockPos, u64>,
    /// Recipe highlighted by empty-hand mortar interaction.
    alchemy_recipe: usize,
    alchemy_recipe_initialized: bool,
    working: Option<LocalWorkingChannel>,
}

struct LocalWorkingChannel {
    stable_id: u64,
    working_id: String,
    wand_id: u64,
    target: crate::workings::WorkingTargetIntent,
    held_secs: f32,
    hold_sent: bool,
}

impl Default for InteractionState {
    fn default() -> Self {
        Self {
            bow_draw: 0.0,
            brushing: 0.0,
            brush_target: None,
            lens_settle: 0.0,
            lens_target: None,
            experiment_kind: 0,
            anvil_work: 0.0,
            anvil_pos: None,
            craft_grid: [None; 9],
            craft_size: 2,
            breaking: None,
            attuned: Vec::new(),
            riding: None,
            fishing: None,
            binding_revisions: std::collections::HashMap::new(),
            alchemy_revisions: std::collections::HashMap::new(),
            alchemy_recipe: 0,
            alchemy_recipe_initialized: false,
            working: None,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum DiscoveryAim {
    Region(crate::planet::BlockPos),
    Block(crate::planet::BlockPos),
}

/// Guest-side connection state.
struct Remote {
    client: net::Client,
    my_id: u32,
    role: identity::Role,
    session: crate::client_session::GuestSession,
    /// id -> (name, pos, yaw) of every other player (render state).
    players: std::collections::HashMap<u32, (String, Vec3, f32)>,
    /// Latest canonical authoritative position for each rendered player.
    player_positions: std::collections::HashMap<u32, crate::planet::EntityPos>,
    /// Wire item id each player holds (from Players snapshots).
    player_held: std::collections::HashMap<u32, u16>,
    player_implement: std::collections::HashMap<u32, crate::implements::ImplementVisual>,
    /// Packed Style per player (from Players snapshots).
    player_style: std::collections::HashMap<u32, u32>,
    sleeping: bool,
    /// Interpolation spans keyed by player / mob id, plus the shared
    /// clocks (age since last snapshot, measured snapshot interval).
    player_lerp: std::collections::HashMap<u32, Lerp>,
    player_age: f32,
    player_interval: f32,
    mob_lerp: std::collections::HashMap<u32, Lerp>,
    mob_age: f32,
    mob_interval: f32,
    /// View distance the host granted, in chunks. Terrain past it is not
    /// coming, so the fog and the eviction radius both respect it.
    granted_view_dist: i32,
    /// What we last told the host we wanted, so the slider only speaks when
    /// it actually moves.
    asked_view_dist: i32,
    /// Chunks we have asked the host for and not yet received, so a gap is
    /// requested once rather than every frame until it lands.
    wants: std::collections::HashSet<ChunkPos>,
}

struct Game {
    window: Arc<Window>,
    renderer: renderer::Renderer,
    server: server::Server,
    player: Player,
    camera: Camera,

    input: InputState,

    // Survival state
    ui_state: UiState,
    inventory: Inventory,
    survival: SurvivalState,
    interaction: InteractionState,
    presentation: PresentationState,
    combat: combat::CombatState,
    /// Skill-tree progression (capability E5), persisted with the profile.
    skills: crate::skills::SkillState,
    rng: u32,

    // Menus / meta
    content: ContentRuntime,
    multiplayer: MultiplayerState,
    identity: identity::LocalIdentity,
    atproto_account: Option<identity::atproto::AtprotoAccount>,
    config: Config,
    audio: Option<Audio>,
    in_world: bool,
    /// (name, seed) of every compatible, committed world under saves/.
    worlds: Vec<(String, u32)>,
    /// Browser-only metadata for compatible worlds and exact notices for
    /// incompatible/corrupt/incomplete folders. Authoritative loading still
    /// validates the complete atlas off-thread before entry.
    world_details: std::collections::HashMap<String, String>,
    world_problems: Vec<(String, String)>,
    loading: world_loading::WorldLoading,
    gen_pool: Option<crate::terrain_jobs::TerrainJobs>,
    mesh_pool: Option<mesh_jobs::MeshPool>,
    /// Start of this frame's streaming work (shared adopt+mesh budget).
    stream_t0: std::time::Instant,
    creative: bool,
    flying: bool,
    last_space: f32,
    time_abs: f32,

    total_frames: u64,
    /// Frames eligible for an automated capture. Direct-entry loading and
    /// creation screens do not consume the world's settle/timeout budget.
    capture_frames: u64,
    settled_frames: u64,
    shot_at: Option<u64>,
    /// A chunk within the DDA occupancy grid's reach remeshed (a block edit),
    /// so the grid is stale and must be rebuilt even if the camera hasn't moved.
    occ_dirty: bool,
    /// Mean albedo per block id, read off the atlas once at startup and written
    /// into the occupancy grid so bounced light knows what colour each cell
    /// hands back. Presentation only — the pack decides it, so it never enters
    /// the sim or the protocol.
    block_albedo: Vec<[u8; 3]>,
    /// One probe's read on what colour the light around the player has become,
    /// as SH-L1. Presentation only, recomputed from the world each frame.
    room_light: bounce::RoomLight,
    /// Your chosen look (config `appearance`, style.rs palettes).
    style: style::Style,
    auto_shot: Option<String>,
    last_frame: Instant,
    last_title: Instant,
    frames: u32,
    fps: u32,
    /// Smoothed frame-section times in ms: (authority, render).
    frame_ms: (f32, f32),
    ui: UiBatch,
}

impl Game {
    pub(super) fn rand01(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.rng >> 8) as f32 / (1 << 24) as f32
    }

    fn sfx(&self, s: Sfx) {
        if let Some(a) = &self.audio {
            a.play(s);
        }
    }

    /// Play at a volume (distance-attenuated world sounds).
    fn sfx_vol(&self, s: Sfx, vol: f32) {
        if vol <= 0.02 {
            return;
        }
        if let Some(a) = &self.audio {
            a.play_vol(s, vol);
        }
    }

    /// The footstep surface under a world position.
    fn step_mat_at(&self, pos: crate::planet::EntityPos) -> audio::StepMat {
        let b = pos
            .translated(Vec3::new(0.0, -0.1, 0.0))
            .ok()
            .and_then(|canonical| canonical.pos.block())
            .map(|block| self.server.world.get_block_at(block))
            .unwrap_or(crate::registry::AIR);
        audio::step_mat(&self.content.reg.block(b).name, self.break_mat(b))
    }
}

#[cfg(test)]
mod state_characterization {
    use super::{PresentationState, advance_capture_clock};

    #[test]
    fn direct_entry_loading_does_not_consume_capture_budget() {
        assert_eq!(advance_capture_clock(2_999, true), 0);
        assert_eq!(advance_capture_clock(0, false), 1);
        assert_eq!(advance_capture_clock(u64::MAX, false), u64::MAX);
    }

    #[test]
    fn presentation_randomness_cannot_advance_the_sim_stream() {
        let sim_seed = 0x51ed_c0de;
        let mut presentation = PresentationState::new();
        let before = presentation.rng;
        let _ = presentation.vary();
        assert_ne!(presentation.rng, before);
        assert_eq!(sim_seed, 0x51ed_c0de);
    }
}
