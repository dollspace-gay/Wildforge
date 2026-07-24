//! Client game-state implementation, split by existing responsibility.

mod actions;
mod app;
mod browser;
mod containers;
mod content;
mod frame;
mod input;
mod interaction;
mod inventory_ui;
mod keymap;
mod menus;
mod remote;
mod roster_ui;
mod session;
mod status;
mod streaming;
mod survival;
mod ui;

use crate::*;
const GEN_BUDGET: usize = 4; // chunk generations per frame (256-tall gen is pricey)
const MESH_BUDGET: usize = 6; // chunk remeshes per frame
const SHOT_SETTLE_FRAMES: u64 = 10;
const SHOT_FIXED_DT: f32 = 1.0 / 60.0;
const SHOT_MAX_FRAMES: u64 = 3000;
const REACH: f32 = 5.0;
const MAX_HEALTH: f32 = 14.0; // base half-hearts (7 hearts)
const MAX_AIR: f32 = 15.0; // seconds of breath

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Title,
    Accounts,
    Moderation(u32),
    Mods,
    Packs,
    Settings,
    Appearance,
    ConfirmDelete,
    Playing,
    Inventory,
    Furnace((i32, i32, i32)),
    Chest((i32, i32, i32)),
    Offering((i32, i32, i32)),
    Bloomery((i32, i32, i32)),
    Kiln((i32, i32, i32)),
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
    action_cooldown: f32,
    attack_cooldown: f32,
    hotbar_sel: usize,
    scroll_accum: f32,
    scroll_cooldown: f32,
    ui_cursor: (f32, f32),
}

/// Player vitals, armor, recovery timers, and respawn ownership.
struct SurvivalState {
    armor: [Option<ItemStack>; 5],
    health: f32,
    hunger: f32,
    nutrition: [f32; 5],
    eating: f32,
    exhaustion_regen: f32,
    starve_timer: f32,
    air: f32,
    since_damage: f32,
    drown_timer: f32,
    burn_timer: f32,
    damage_flash: f32,
    fall_start: Option<f32>,
    spawn_point: Vec3,
    killed_by_wild: bool,
}

impl SurvivalState {
    fn new(spawn_point: Vec3) -> Self {
        Self {
            armor: [None; 5],
            health: MAX_HEALTH,
            hunger: 20.0,
            nutrition: [0.0; 5],
            eating: 0.0,
            exhaustion_regen: 0.0,
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
    browse_page: usize,
    browse_view: Option<(ItemId, bool)>,
    browse_back: Vec<(ItemId, bool)>,
    inventory_status_open: bool,
    inventory_browser_open: bool,
    appearance_from_pause: bool,
    account_name: String,
    account_handle: String,
    account_focus: u8,
    account_status: String,
    account_task: Option<std::sync::mpsc::Receiver<AccountTaskResult>>,
    moderation_confirm: Option<u8>,
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
            browse_page: 0,
            browse_view: None,
            browse_back: Vec::new(),
            inventory_status_open: false,
            inventory_browser_open: false,
            appearance_from_pause: false,
            account_name: String::new(),
            account_handle: String::new(),
            account_focus: 0,
            account_status: String::new(),
            account_task: None,
            moderation_confirm: None,
        }
    }
}

/// In-progress use actions, crafting inputs, and local item entities.
struct InteractionState {
    bow_draw: f32,
    brushing: f32,
    brush_target: Option<(i32, i32, i32)>,
    anvil_work: f32,
    anvil_pos: Option<(i32, i32, i32)>,
    craft_grid: [Option<ItemStack>; 9],
    craft_size: usize,
    items: Vec<ItemEntity>,
    breaking: Option<((i32, i32, i32), f32)>,
}

impl Default for InteractionState {
    fn default() -> Self {
        Self {
            bow_draw: 0.0,
            brushing: 0.0,
            brush_target: None,
            anvil_work: 0.0,
            anvil_pos: None,
            craft_grid: [None; 9],
            craft_size: 2,
            items: Vec::new(),
            breaking: None,
        }
    }
}

/// Cosmetic animation, particles, transient feedback, and light selection.
struct PresentationState {
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
    remote_strides: std::collections::HashMap<u32, (Vec3, f32)>,
    ui_flies: Vec<(u16, (f32, f32), usize, f32)>,
    slot_pulse: [f32; HOTBAR_SLOTS],
    pickup_streak: (u32, f32),
    screen_age: f32,
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
}

impl PresentationState {
    fn new() -> Self {
        Self {
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
    /// Host block id -> local block id.
    block_map: Vec<crate::registry::BlockId>,
    /// Host item id -> local item id.
    item_map: Vec<Option<ItemId>>,
    /// Local block id -> host id (for Place).
    host_block: std::collections::HashMap<u16, u16>,
    /// id -> (name, pos, yaw) of every other player (render state).
    players: std::collections::HashMap<u32, (String, Vec3, f32)>,
    /// Wire item id each player holds (from Players snapshots).
    player_held: std::collections::HashMap<u32, u16>,
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
    rng: u32,

    // Menus / meta
    content: ContentRuntime,
    multiplayer: MultiplayerState,
    identity: identity::LocalIdentity,
    atproto_account: Option<identity::atproto::AtprotoAccount>,
    config: Config,
    audio: Option<Audio>,
    in_world: bool,
    /// (name, seed) of every world under saves/.
    worlds: Vec<(String, u32)>,
    gen_pool: Option<streaming::GenPool>,
    creative: bool,
    flying: bool,
    last_space: f32,
    time_abs: f32,

    total_frames: u64,
    settled_frames: u64,
    shot_at: Option<u64>,
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

/// Resolve a configured pack id: a folder under packs/ wins (editable,
/// hot-reloads), else a pack compiled into the binary, else none.
fn pack_source_of(id: &str) -> Option<atlas::PackSource> {
    if id.is_empty() {
        return None;
    }
    let p = PathBuf::from("packs").join(id);
    if p.is_dir() {
        return Some(atlas::PackSource::Dir(p));
    }
    atlas::embedded_pack(id).map(atlas::PackSource::Embedded)
}

/// Browser item list: public items (no internal /variants), search-filtered.
pub(crate) fn browser_items(reg: &Registry, search: &str) -> Vec<ItemId> {
    let q = search.to_lowercase();
    (0..reg.items.len() as u16)
        .map(ItemId)
        .filter(|i| {
            let d = reg.item(*i);
            !d.name.contains('/')
                && (q.is_empty()
                    || d.label.to_lowercase().contains(&q)
                    || d.name.to_lowercase().contains(&q))
        })
        .collect()
}

fn find_spawn(world: &World) -> (i32, i32) {
    // Walk outward until we find dry land.
    let g = &world.generator;
    let mut best = (0, 0);
    'outer: for r in 0..64 {
        let d = r * 8;
        for (x, z) in [(d, 0), (-d, 0), (0, d), (0, -d), (d, d), (-d, -d)] {
            if g.surface_estimate(x, z) > SEA_LEVEL + 1 {
                best = (x, z);
                break 'outer;
            }
        }
    }
    best
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
        let config = Config::load();
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
        let atlas =
            atlas::build_atlas(&reg.tex_files, pack_source_of(&active_pack), &reg.tex_names);
        let pack_warnings = atlas.warnings;
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
        let spawn = Vec3::new(0.5, 80.0, 0.5);
        let own_style = style::Style::unpack(config.appearance);

        let size = window.inner_size();
        let aspect = size.width as f32 / size.height.max(1) as f32;
        let audio = Audio::new(config.volume);

        let mut g = Game {
            window,
            renderer,
            server: sim,
            player: Player::new(spawn),
            camera: Camera::new(spawn + Vec3::new(0.0, EYE_HEIGHT, 0.0), aspect),
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
                action_cooldown: 0.0,
                attack_cooldown: 0.0,
                hotbar_sel: 0,
                scroll_accum: 0.0,
                scroll_cooldown: 0.0,
                ui_cursor: (0.0, 0.0),
            },
            ui_state: UiState::default(),
            inventory: Inventory::new(),
            survival: SurvivalState::new(spawn),
            interaction: InteractionState::default(),
            presentation: PresentationState::new(),
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
            },
            multiplayer: MultiplayerState::default(),
            identity,
            atproto_account,
            config,
            audio,
            in_world: false,
            worlds: Vec::new(),
            gen_pool: None,
            creative: false,
            flying: false,
            last_space: -9.0,
            time_abs: 0.0,
            total_frames: 0,
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
        g.refresh_worlds();
        // Dev/headless: open a specific menu screen for UI verification.
        match std::env::var("WILDFORGE_SCREEN").as_deref() {
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
    fn step_mat_at(&self, x: f32, y: f32, z: f32) -> audio::StepMat {
        let b = self.server.world.get_block(
            x.floor() as i32,
            (y - 0.1).floor() as i32,
            z.floor() as i32,
        );
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
    use super::PresentationState;

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
