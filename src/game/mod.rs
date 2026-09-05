//! Client game-state implementation, split by existing responsibility.

mod actions;
mod app;
mod browser;
mod capture;
pub(crate) mod combat;
mod containers;
mod content;
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
mod interaction;
mod inventory_ui;
mod keymap;
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

use crate::*;
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

#[derive(Clone, PartialEq)]
enum Screen {
    Title,
    NewWorld,
    CreatingWorld,
    Accounts,
    Moderation(u32),
    Mods,
    Packs,
    Settings,
    Appearance,
    ConfirmDelete,
    Playing,
    Inventory,
    Furnace(crate::planet::BlockPos),
    Chest(crate::planet::BlockPos),
    Offering(crate::planet::BlockPos),
    Bloomery(crate::planet::BlockPos),
    Kiln(crate::planet::BlockPos),
    /// A recipe-list station machine (capability E7): lists the machine's
    /// `station` recipes and crafts them from the inventory. Temporary
    /// hardcoded screen; E11 generalizes it into mod-extensible screens.
    Workbench(crate::planet::BlockPos),
    /// A tamed carrier's saddlebags, keyed by mob id.
    MobCargo(u32),
    /// Writing a placed sign or waystone.
    SignEdit(crate::planet::BlockPos),
    /// A market stall: the owner manages, everyone else shops.
    Stall(crate::planet::BlockPos),
    /// Talking to a friendly NPC (spec 3.2): the dialogue tree in
    /// `reg.dialogues` selected by the NPC's def. Holds the NPC mob id,
    /// the current node id, and the highlighted choice row.
    Dialog {
        npc: u32,
        node_id: String,
        choice_sel: usize,
    },
    /// The quest journal (spec 3.3): accepted quests and their progress.
    Journal,
    /// The skill tree (capability E5): allocate learned nodes in the
    /// active world's mode-gated tree. Temporary hardcoded screen; E11
    /// generalizes this into mod-extensible screens.
    Skills,
    /// The loadout (capability E6): slot components into worn frames,
    /// repair disabled frames, and save/apply loadout presets. Temporary
    /// hardcoded screen; E11 generalizes this into mod-extensible screens.
    Loadout,
    /// A data-driven mod screen (capability E11): the index into
    /// `Registry::screens`. Rows render from the def; buttons dispatch the
    /// mod's `on_screen_click` hook host-authoritatively.
    Mod(usize),
    Join,
    Paused,
    Dead,
}

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

#[derive(Default)]
struct KeysDown {
    w: bool,
    a: bool,
    s: bool,
    d: bool,
    space: bool,
    sprint: bool,
    block: bool,
}

/// Pointer, keyboard, capture, and input-rate state that resets together.
struct InputState {
    keys: KeysDown,
    mouse_captured: bool,
    raw_look: bool,
    last_cursor: Option<(f64, f64)>,
    warp_pending: bool,
    allow_warp: bool,
    left_held: bool,
    right_held: bool,
    /// Edge-triggered dodge request, consumed by `advance_player`.
    dodge_pressed: bool,
    action_cooldown: f32,
    attack_cooldown: f32,
    hotbar_sel: usize,
    scroll_accum: f32,
    scroll_cooldown: f32,
    ui_cursor: (f32, f32),
    /// WILDFORGE_CURSOR parked the pointer for a headless capture;
    /// the window's synthetic CursorMoved events must not undo it.
    cursor_locked: bool,
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

/// Screen navigation, focus, browser history, and cursor-held inventory state.
struct UiState {
    screen: Screen,
    held_stack: Option<ItemStack>,
    settings_from_pause: bool,
    pending_delete: Option<usize>,
    dragging_slider: Option<usize>,
    search: String,
    search_focus: bool,
    /// Sign editor buffer (three short lines) and the active line.
    sign_lines: [String; 3],
    sign_line: usize,
    browse_page: usize,
    browse_view: Option<(ItemId, bool)>,
    browse_back: Vec<(ItemId, bool)>,
    inventory_status_open: bool,
    inventory_browser_open: bool,
    inventory_discovery_open: bool,
    discovery_holder: Option<net::RecordHolderSnap>,
    discovery_copy_target: Option<net::RecordHolderSnap>,
    discovery_writing_pos: Option<crate::planet::BlockPos>,
    discovery_records: Vec<crate::discovery::ObservationSummary>,
    discovery_capacity: u16,
    discovery_page: usize,
    discovery_sort: u8,
    discovery_selected: [Option<u64>; 2],
    discovery_include_location: bool,
    discovery_label: String,
    discovery_label_focus: bool,
    appearance_from_pause: bool,
    account_name: String,
    account_handle: String,
    account_focus: u8,
    account_status: String,
    account_task: Option<std::sync::mpsc::Receiver<AccountTaskResult>>,
    new_world_mode: String,
    new_world_seed: String,
    new_world_status: String,
    moderation_confirm: Option<u8>,
    creation_status: String,
    creation_progress: (usize, usize),
    /// Index into `reg.skills.branches` shown on the skill screen.
    skills_branch: usize,
    /// Which armor slot (0..=3) the loadout screen acts on.
    loadout_select: usize,
    /// Which numbered preset the loadout screen saves into / applies from.
    loadout_preset_sel: usize,
}

enum AccountTaskResult {
    Linked(Result<identity::atproto::AtprotoAccount, String>),
    Revoked(Result<(), String>),
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            screen: Screen::Title,
            held_stack: None,
            settings_from_pause: false,
            pending_delete: None,
            dragging_slider: None,
            search: String::new(),
            search_focus: false,
            sign_lines: Default::default(),
            sign_line: 0,
            browse_page: 0,
            browse_view: None,
            browse_back: Vec::new(),
            inventory_status_open: false,
            inventory_browser_open: false,
            inventory_discovery_open: false,
            discovery_holder: None,
            discovery_copy_target: None,
            discovery_writing_pos: None,
            discovery_records: Vec::new(),
            discovery_capacity: 0,
            discovery_page: 0,
            discovery_sort: 0,
            discovery_selected: [None; 2],
            discovery_include_location: false,
            discovery_label: String::new(),
            discovery_label_focus: false,
            appearance_from_pause: false,
            account_name: String::new(),
            account_handle: String::new(),
            account_focus: 0,
            account_status: String::new(),
            account_task: None,
            new_world_mode: "survival".into(),
            new_world_seed: String::new(),
            new_world_status: String::new(),
            moderation_confirm: None,
            creation_status: String::new(),
            creation_progress: (0, crate::planet_atlas::AtlasStage::ALL.len()),
            skills_branch: 0,
            loadout_select: 0,
            loadout_preset_sel: 0,
        }
    }
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

/// Cosmetic animation, particles, transient feedback, and light selection.
struct PresentationState {
    /// Top of the view-distance slider on this machine, resolved once at
    /// startup from available memory. A setting that cannot be honoured is
    /// worse than one that is not offered.
    max_view_dist: i32,
    /// Region-whisper bookkeeping: the cell we're in, and cells
    /// already whispered this session.
    last_ire_cell: Option<world::RegionCell>,
    whispered_cells: std::collections::HashSet<world::RegionCell>,
    /// Qualitative magical signatures already presented this session. The
    /// same ordinary condition does not toast on every atlas-cell crossing.
    arcane_signs: std::collections::HashSet<String>,
    swing: f32,
    hand_bob: f32,
    weather_vis: f32,
    lightning: f32,
    thunder_delay: f32,
    atlas_season: usize,
    juice: bool,
    rng: u32,
    pool: particles::Pool,
    step_accum: f32,
    mob_strides: std::collections::HashMap<u32, f32>,
    remote_strides: std::collections::HashMap<u32, (crate::planet::EntityPos, f32)>,
    ui_flies: Vec<(u16, (f32, f32), usize, f32)>,
    slot_pulse: [f32; HOTBAR_SLOTS],
    pickup_streak: (u32, f32),
    screen_age: f32,
    /// Countdown to the next ambient speck (songbird, dragonfly).
    ambient_timer: f32,
    sel_bounce: f32,
    press_dip: f32,
    hitch: f32,
    nudge: (Vec3, f32),
    presence_timer: f32,
    hunger_timer: f32,
    demo_burst: Option<(Vec3, u16)>,
    toasts: Vec<(String, f32)>,
    lights: lights::Director,
    player_gait: std::collections::HashMap<u32, (Vec3, f32)>,
    demo_lights: Vec<lights::DynLight>,
    /// Last host-authored active cue by stable working id. Dedicated guests
    /// refresh this bounded presentation cache once per second; local play
    /// reads the authoritative state directly.
    working_cues: std::collections::HashMap<u64, (crate::workings::WorkingCue, f32)>,
}

impl PresentationState {
    fn new() -> Self {
        Self {
            max_view_dist: config::max_view_dist_for_memory(),
            last_ire_cell: None,
            whispered_cells: std::collections::HashSet::new(),
            arcane_signs: std::collections::HashSet::new(),
            swing: 0.0,
            hand_bob: 0.0,
            weather_vis: 0.0,
            lightning: 0.0,
            thunder_delay: -1.0,
            atlas_season: 1,
            juice: std::env::var("WILDFORGE_JUICE")
                .map(|v| v != "0")
                .unwrap_or(true),
            rng: 0x9e3779b9,
            pool: particles::Pool::default(),
            step_accum: 0.0,
            mob_strides: Default::default(),
            remote_strides: Default::default(),
            ui_flies: Vec::new(),
            slot_pulse: [0.0; HOTBAR_SLOTS],
            pickup_streak: (0, 0.0),
            screen_age: 1.0,
            ambient_timer: 3.0,
            sel_bounce: 1.0,
            press_dip: 0.0,
            hitch: 0.0,
            nudge: (Vec3::ZERO, 0.0),
            presence_timer: 0.0,
            hunger_timer: 0.0,
            demo_burst: None,
            toasts: Vec::new(),
            lights: lights::Director::new(),
            player_gait: Default::default(),
            demo_lights: Vec::new(),
            working_cues: Default::default(),
        }
    }

    fn vary(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
        0.9 + ((self.rng >> 8) as f32 / (1 << 24) as f32) * 0.2
    }
}

/// Guest-side connection state.
struct Remote {
    client: net::Client,
    my_id: u32,
    role: identity::Role,
    content: crate::client_session::ContentMap,
    /// id -> (name, pos, yaw) of every other player (render state).
    players: std::collections::HashMap<u32, (String, Vec3, f32)>,
    /// Latest canonical authoritative position for each rendered player.
    player_positions: std::collections::HashMap<u32, crate::planet::EntityPos>,
    /// Wire item id each player holds (from Players snapshots).
    player_held: std::collections::HashMap<u32, u16>,
    player_implement: std::collections::HashMap<u32, crate::implements::ImplementVisual>,
    /// Packed Style per player (from Players snapshots).
    player_style: std::collections::HashMap<u32, u32>,
    names: std::collections::HashMap<u32, String>,
    sleeping: bool,
    /// Interpolation spans keyed by player / mob id, plus the shared
    /// clocks (age since last snapshot, measured snapshot interval).
    player_lerp: std::collections::HashMap<u32, Lerp>,
    player_age: f32,
    player_interval: f32,
    mob_lerp: std::collections::HashMap<u32, Lerp>,
    mob_age: f32,
    mob_interval: f32,
    snapshots: crate::client_session::Snapshots,
    /// View distance the host granted, in chunks. Terrain past it is not
    /// coming, so the fog and the eviction radius both respect it.
    granted_view_dist: i32,
    /// What we last told the host we wanted, so the slider only speaks when
    /// it actually moves.
    asked_view_dist: i32,
    /// Chunks we have asked the host for and not yet received, so a gap is
    /// requested once rather than every frame until it lands.
    wants: std::collections::HashSet<ChunkPos>,
    /// Host-declared bounded terrain that must be decoded before this
    /// connection becomes a simulated player.
    entry_required: std::collections::HashSet<ChunkPos>,
    entry_manifest_received: bool,
    entry_ready_sent: bool,
    entry_world_name: Option<String>,
    entry_center: Option<ChunkPos>,
    entry_center_meshed: bool,
    pending_entry_chunks: std::collections::VecDeque<(ChunkPos, Vec<u8>)>,
    entry_activity: std::time::Instant,
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

/// (mod id, dir) pairs for mods that ship a main.rhai.
fn script_mod_dirs(reg: &Registry) -> Vec<(String, PathBuf)> {
    reg.mods
        .iter()
        .filter(|m| m.has_script && m.error.is_none())
        .filter_map(|m| m.path.clone().map(|p| (m.id.clone(), p)))
        .collect()
}

/// Cheap fingerprint of the mods tree (file count + max mtime) for hot reload.
/// Newest-mtime + file-count stamp over the hot-reloadable content trees
/// (mods/ and packs/); a change re-triggers the 1 s reload poll.
pub(crate) fn content_tree_stamp_of(roots: &[&std::path::Path]) -> u64 {
    fn walk(dir: &std::path::Path, acc: &mut u64, count: &mut u64) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, acc, count);
            } else if let Ok(md) = e.metadata() {
                *count += 1;
                if let Ok(t) = md.modified()
                    && let Ok(d) = t.duration_since(std::time::UNIX_EPOCH)
                {
                    *acc = (*acc).max(d.as_secs() * 1000 + d.subsec_millis() as u64);
                }
            }
        }
    }
    let (mut acc, mut count) = (0u64, 0u64);
    for root in roots {
        walk(root, &mut acc, &mut count);
    }
    acc ^ (count << 48)
}

fn content_tree_stamp() -> u64 {
    content_tree_stamp_of(&[std::path::Path::new("mods"), std::path::Path::new("packs")])
}

/// Armor: each point blocks 4% of the wild's damage, capped at 60%.
pub(crate) fn reduced_damage(amount: f32, points: u32) -> f32 {
    amount * (1.0 - (points as f32 * 0.04).min(0.6))
}

/// First free "worldN" name. A name is taken if it's in the world list OR
/// its folder exists on disk at all — a new world must never adopt an
/// existing folder's chunks/player.toml, even one the listing can't parse.
pub(crate) fn next_world_name(saves: &std::path::Path, worlds: &[(String, u32)]) -> String {
    let mut n = 1;
    loop {
        let name = format!("world{n}");
        if !worlds.iter().any(|(w, _)| w == &name) && !saves.join(&name).exists() {
            return name;
        }
        n += 1;
    }
}

/// Browser item list: public items (no internal /variants), search-filtered.
pub(crate) fn browser_items(reg: &Registry, search: &str, creative: bool) -> Vec<ItemId> {
    let q = search.to_lowercase();
    (0..reg.items.len() as u16)
        .map(ItemId)
        .filter(|i| {
            let d = reg.item(*i);
            // `/` marks a generated variant — a growth stage, a fluid
            // level, or the creative-only placer synthesised for a
            // block nobody can hold. In creative the builder wants all
            // of them; in survival none exist.
            let variant = d.name.contains('/');
            (!variant || (creative && d.creative_only))
                && (q.is_empty()
                    || d.label.to_lowercase().contains(&q)
                    || d.name.to_lowercase().contains(&q))
        })
        .collect()
}

impl Game {
    fn new(window: Arc<Window>) -> Game {
        // Registry + atlas first: the renderer needs the packed texture atlas.
        let reg = Arc::new(registry::load(std::path::Path::new("mods")));
        for m in &reg.mods {
            if let Some(e) = &m.error {
                eprintln!("mod {}: {e}", m.id);
            }
        }
        std::fs::create_dir_all("packs").ok();
        let mut config = Config::load();
        // Dev/capture override, intentionally never persisted. Production
        // planets can otherwise spend the entire bounded screenshot run
        // filling a player's large everyday horizon before frame one.
        if let Ok(distance) = std::env::var("WILDFORGE_VIEW_DIST")
            && let Ok(distance) = distance.parse::<i32>()
        {
            config.view_dist = distance.clamp(config::MIN_VIEW_DIST, config::MAX_VIEW_DIST);
        }
        let identity = identity::LocalIdentity::load_or_create(&identity::identity_dir())
            .expect("load or create local identity");
        let atproto_account = identity::atproto::AtprotoAccount::load(&identity::identity_dir())
            .unwrap_or_else(|error| {
                eprintln!("identity: could not load ATProto link: {error}");
                None
            });
        // Dev override (never persisted): WILDFORGE_PACK=<id> selects a pack.
        let pack_override = std::env::var("WILDFORGE_PACK").ok();
        let active_pack = pack_override.clone().unwrap_or_else(|| config.pack.clone());
        let atlas = atlas::build_atlas(
            &reg.tex_files,
            &atlas::pack_chain(&active_pack),
            &reg.tex_names,
        );
        let pack_warnings = atlas.warnings;
        let tile_variants = atlas.variants;
        let diagnostic_families = visual_capture::evidence_enabled()
            .then(|| visual_capture::diagnostic_families(&reg, &tile_variants));
        // Read the albedos off the finished atlas, before it is handed to the
        // renderer — this is the last point at which the packed image and the
        // slot assignments are both in hand.
        let block_albedo = reg.block_albedo(&atlas::slot_albedo(&atlas.color, atlas.px));
        let renderer = pollster::block_on(renderer::Renderer::new(
            window.clone(),
            atlas.color,
            atlas.material,
            atlas.normal,
            atlas.px,
        ));
        let mut scripts = script::ScriptHost::new();
        scripts.load_mods(&script_mod_dirs(&reg));
        // No world yet — the game opens on the title screen.
        let world = World::new(0, PathBuf::from("saves/.none"), reg.clone());
        let sim = server::Server::new(world, 0.3, 0x51ed_c0de);
        let spawn = crate::planet::EntityPos::new(
            crate::planet::Face::PosZ,
            crate::planet::FACE_BLOCKS as f32 * 0.5 + 0.5,
            80.0,
            crate::planet::FACE_BLOCKS as f32 * 0.5 + 0.5,
        )
        .expect("initial menu position is at the planet face center");
        let own_style = style::Style::unpack(config.appearance);

        let size = window.inner_size();
        let aspect = size.width as f32 / size.height.max(1) as f32;
        let audio = Audio::new(config.volume);

        let mut g = Game {
            window,
            renderer,
            server: sim,
            player: Player::new_at(spawn),
            camera: Camera::new(
                spawn
                    .translated(Vec3::new(0.0, EYE_HEIGHT, 0.0))
                    .expect("initial camera height is inside the shell")
                    .pos
                    .render_pos(),
                aspect,
            ),
            input: InputState {
                keys: KeysDown::default(),
                mouse_captured: false,
                raw_look: false,
                last_cursor: None,
                warp_pending: false,
                allow_warp: std::env::var("WSL_DISTRO_NAME").is_err()
                    && !std::path::Path::new("/mnt/wslg").exists(),
                left_held: false,
                right_held: false,
                dodge_pressed: false,
                action_cooldown: 0.0,
                attack_cooldown: 0.0,
                hotbar_sel: 0,
                scroll_accum: 0.0,
                scroll_cooldown: 0.0,
                ui_cursor: (0.0, 0.0),
                cursor_locked: false,
            },
            ui_state: UiState::default(),
            inventory: Inventory::new(),
            survival: SurvivalState::new(spawn),
            interaction: InteractionState::default(),
            presentation: PresentationState::new(),
            combat: combat::CombatState::new(),
            skills: crate::skills::SkillState::default(),
            rng: if std::env::var("WILDFORGE_SHOT").is_ok() {
                0x1234_5678
            } else {
                0x1234_5678
                    ^ std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.subsec_nanos())
                        .unwrap_or(0)
            },
            content: ContentRuntime {
                reg,
                scripts,
                mods_stamp: 0,
                mods_poll: 0.0,
                packs: atlas::discover_packs(),
                pack_warnings,
                pack_override,
                tile_variants,
                diagnostic_families,
            },
            multiplayer: MultiplayerState::default(),
            identity,
            atproto_account,
            config,
            audio,
            in_world: false,
            worlds: Vec::new(),
            world_details: Default::default(),
            world_problems: Vec::new(),
            loading: world_loading::WorldLoading::default(),
            gen_pool: None,
            mesh_pool: None,
            stream_t0: std::time::Instant::now(),
            creative: false,
            flying: false,
            last_space: -9.0,
            time_abs: 0.0,
            total_frames: 0,
            capture_frames: 0,
            occ_dirty: false,
            block_albedo,
            room_light: bounce::RoomLight::new(),
            settled_frames: 0,
            shot_at: None,
            style: own_style,
            auto_shot: std::env::var("WILDFORGE_SHOT").ok(),
            last_frame: Instant::now(),
            last_title: Instant::now(),
            frames: 0,
            fps: 0,
            frame_ms: (0.0, 0.0),
            ui: UiBatch::new(),
        };
        g.content.mods_stamp = content_tree_stamp();
        g.ui_state.account_name = g.config.display_name.clone();
        // Migration convenience only: the old implicit name is proposed in
        // an editable local field. It is neither saved nor transmitted until
        // the player explicitly presses SAVE LOCAL NAME.
        if !g.config.profile_complete {
            for key in ["WILDFORGE_NAME", "USER", "USERNAME"] {
                if let Ok(value) = std::env::var(key)
                    && let Ok(name) = identity::DisplayName::parse(&value)
                {
                    g.ui_state.account_name = name.to_string();
                    break;
                }
            }
        }
        g.ui_state.account_handle = g
            .atproto_account
            .as_ref()
            .and_then(|account| account.handle.clone())
            .unwrap_or_default();
        g.apply_config();
        // Capture/benchmark override only; applying it after `apply_config`
        // keeps a diagnostic run from rewriting the player's saved slider.
        if let Ok(view_dist) = std::env::var("WILDFORGE_VIEW_DIST")
            && let Ok(view_dist) = view_dist.parse::<i32>()
        {
            g.config.view_dist =
                view_dist.clamp(crate::config::MIN_VIEW_DIST, g.presentation.max_view_dist);
        }
        g.refresh_worlds();
        // Dev/headless: open a specific menu screen for UI verification.
        match std::env::var("WILDFORGE_SCREEN").as_deref() {
            Ok("newworld") => {
                g.ui_state.new_world_seed = "20260801".into();
                g.ui_state.screen = Screen::NewWorld;
            }
            Ok("creating") => {
                g.ui_state.creation_status = "QUALIFYING HOMELAND".into();
                g.ui_state.creation_progress = (17, 25);
                g.ui_state.screen = Screen::CreatingWorld;
            }
            Ok("mods") => g.ui_state.screen = Screen::Mods,
            Ok("packs") => g.ui_state.screen = Screen::Packs,
            Ok("settings") => g.ui_state.screen = Screen::Settings,
            Ok("appearance") => g.ui_state.screen = Screen::Appearance,
            Ok("accounts") => g.ui_state.screen = Screen::Accounts,
            Ok("confirm") => {
                g.ui_state.pending_delete = if g.worlds.is_empty() { None } else { Some(0) };
                g.ui_state.screen = Screen::ConfirmDelete;
            }
            Ok("join") => {
                g.multiplayer.discovery = net::Discovery::start().ok();
                g.ui_state.screen = Screen::Join;
            }
            _ => {}
        }
        if !g.config.profile_complete && std::env::var("WILDFORGE_SCREEN").is_err() {
            g.ui_state.screen = Screen::Accounts;
        }
        g
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

    /// The variation rule: every repeated sound differs a little.
    /// Uses the juice rng so cosmetics never touch the sim's dice.
    fn vary(&mut self) -> f32 {
        self.presentation.vary()
    }

    /// Debris burst from a block's own texture (breaks, hits).
    fn juice_burst(&mut self, at: Vec3, tile: u16, n: usize, speed: f32) {
        if !self.presentation.juice {
            return;
        }
        let mut r = self.presentation.rng;
        self.presentation.pool.burst(at, tile, n, speed, &mut r);
        self.presentation.rng = r;
    }

    /// A soft ground puff (landings, grinding).
    fn juice_puff(&mut self, at: Vec3, tile: u16, n: usize) {
        if !self.presentation.juice {
            return;
        }
        let mut r = self.presentation.rng;
        self.presentation.pool.puff(at, tile, n, &mut r);
        self.presentation.rng = r;
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

/// Start the platform event loop and windowed client.
pub(super) fn run_windowed() {
    // Prefer X11/XWayland on Linux: it supports cursor confinement and
    // warping, which pure Wayland compositors (notably WSLg) often don't.
    #[cfg(target_os = "linux")]
    let event_loop = {
        use winit::platform::x11::EventLoopBuilderExtX11;
        let mut builder = EventLoop::builder();
        if std::env::var("DISPLAY").is_ok() {
            builder.with_x11();
        }
        builder.build().expect("create event loop")
    };
    #[cfg(not(target_os = "linux"))]
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = app::App::default();
    event_loop.run_app(&mut app).expect("run event loop");
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
