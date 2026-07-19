//! Wildforge — a Minecraft-alpha-style voxel game.
//!
//! Controls: WASD move, mouse look, Space jump, Ctrl sprint,
//! hold left click to mine, right click place, middle click pick block,
//! 1-9 / scroll wheel select hotbar slot, E inventory, Esc pause,
//! F2 screenshot, F11 fullscreen.

mod atlas;
mod audio;
mod camera;
mod chunk;
mod config;
mod crafting;
mod entity;
mod inventory;
mod mesher;
mod mobs;
mod mp;
mod net;
mod physics;
mod raycast;
mod registry;
mod renderer;
mod script;
mod server;
#[cfg(test)]
mod tests;
mod ui;
mod world;
mod worldgen;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use glam::Vec3;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{
    DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

use audio::{Audio, BreakMat, Sfx};
use camera::Camera;
use chunk::{CHUNK_X, ChunkPos, SEA_LEVEL};
use config::Config;
use entity::ItemEntity;
use inventory::{HOTBAR_SLOTS, Inventory, ItemStack, TOTAL_SLOTS};
use physics::{EYE_HEIGHT, Player};
use registry::{AIR, ItemId, Registry, ToolKind};
use renderer::FrameInput;
use ui::UiBatch;
use world::World;

const GEN_BUDGET: usize = 4; // chunk generations per frame (256-tall gen is pricey)
const MESH_BUDGET: usize = 6; // chunk remeshes per frame
const REACH: f32 = 5.0;
const MAX_HEALTH: f32 = 14.0; // base half-hearts (7 hearts)
const MAX_AIR: f32 = 15.0; // seconds of breath

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Title,
    Mods,
    Packs,
    Settings,
    ConfirmDelete,
    Playing,
    Inventory,
    Furnace((i32, i32, i32)),
    Chest((i32, i32, i32)),
    Offering((i32, i32, i32)),
    Bloomery((i32, i32, i32)),
    Join,
    Paused,
    Dead,
}

/// Player name for multiplayer: config-free, from the OS user.
fn whoami() -> String {
    std::env::var("WILDFORGE_NAME")
        .or_else(|_| std::env::var("USER"))
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "player".into())
        .chars()
        .take(16)
        .collect()
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

/// Guest-side connection state.
struct Remote {
    client: net::Client,
    my_id: u32,
    /// Host block id -> local block id.
    block_map: Vec<crate::registry::BlockId>,
    /// Host item id -> local item id.
    item_map: Vec<Option<ItemId>>,
    /// Local block id -> host id (for Place).
    host_block: std::collections::HashMap<u16, u16>,
    /// id -> (name, pos, yaw) of every other player (render state).
    players: std::collections::HashMap<u32, (String, Vec3, f32)>,
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

    keys: KeysDown,
    mouse_captured: bool,
    /// True when the OS pinned the cursor in place (Locked grab): look uses
    /// raw motion. Otherwise look uses cursor-position deltas + recentering,
    /// which behaves correctly where raw deltas are broken (e.g. WSLg).
    raw_look: bool,
    last_cursor: Option<(f64, f64)>,
    warp_pending: bool,
    /// Under WSLg the compositor cannot move the host Windows cursor, so any
    /// warp desyncs guest/host pointers and produces huge bogus deltas
    /// (microsoft/wslg#1361). There: never warp, use pure position deltas.
    allow_warp: bool,
    left_held: bool,
    right_held: bool,
    action_cooldown: f32,
    attack_cooldown: f32,
    hotbar_sel: usize,
    scroll_accum: f32,
    scroll_cooldown: f32,
    /// Cursor position in window pixels (for menus).
    ui_cursor: (f32, f32),

    // Survival state
    screen: Screen,
    inventory: Inventory,
    /// Worn armor: head, chest, legs, feet — plus the charm slot (4).
    armor: [Option<ItemStack>; 5],
    /// Seconds the bow has been drawn (0 = not drawing).
    bow_draw: f32,
    /// Archaeology channel: seconds brushing + the block under the brush.
    brushing: f32,
    brush_target: Option<(i32, i32, i32)>,
    /// First-person viewmodel: swing progress (1 at trigger, decays to
    /// 0) and the walk-bob phase accumulator.
    swing: f32,
    hand_bob: f32,
    /// Weather presentation: lerped gloom 0..1, lightning flash timer,
    /// pending thunder delay, and the season the atlas was tinted for.
    weather_vis: f32,
    lightning: f32,
    thunder_delay: f32,
    atlas_season: usize,
    /// Smithing channel: seconds into the current hammer strike.
    anvil_work: f32,
    anvil_pos: Option<(i32, i32, i32)>,
    /// Stack picked up by the cursor inside the inventory screen.
    held_stack: Option<ItemStack>,
    /// Crafting grid contents (row-major; 2x2 uses the first 4 cells).
    craft_grid: [Option<ItemStack>; 9],
    /// 2 in the inventory screen, 3 at a crafting table.
    craft_size: usize,
    items: Vec<ItemEntity>,
    /// Block being mined and progress 0..1.
    breaking: Option<((i32, i32, i32), f32)>,
    health: f32,
    hunger: f32,
    nutrition: [f32; 5],
    eating: f32,
    exhaustion_regen: f32,
    starve_timer: f32,
    air: f32,
    since_damage: f32,
    drown_timer: f32,
    damage_flash: f32,
    fall_start: Option<f32>,
    spawn_point: Vec3,
    rng: u32,

    // Menus / meta
    reg: Arc<Registry>,
    scripts: script::ScriptHost,
    toasts: Vec<(String, f32)>,
    mods_stamp: u64,
    mods_poll: f32,
    /// Discovered texture packs (refreshed when opening the packs screen).
    packs: Vec<atlas::PackInfo>,
    /// Warnings from the active texture pack's last build.
    pack_warnings: Vec<String>,
    /// WILDFORGE_PACK dev override; shadows config.pack, never persisted.
    pack_override: Option<String>,
    /// Hosting: the multiplayer session (pause menu -> OPEN TO FRIENDS).
    host: Option<mp::HostSession>,
    /// Waiting for everyone else to sleep (multiplayer bedroll).
    host_sleeping: bool,
    /// Joined someone else's world.
    remote: Option<Remote>,
    /// LAN server discovery for the JOIN screen.
    discovery: Option<net::Discovery>,
    join_ip: String,
    join_status: String,
    chat_open: bool,
    chat_text: String,
    move_timer: f32,
    tick_accum: f32,
    config: Config,
    audio: Option<Audio>,
    in_world: bool,
    /// (name, seed) of every world under saves/.
    worlds: Vec<(String, u32)>,
    settings_from_pause: bool,
    pending_delete: Option<usize>,
    /// Slider being dragged in the settings screen.
    dragging_slider: Option<usize>,
    creative: bool,
    flying: bool,
    /// Death screen subtitle: slain by a warden, not a fall.
    killed_by_wild: bool,
    last_space: f32,
    time_abs: f32,
    search: String,
    search_focus: bool,
    browse_page: usize,
    /// (item, uses-tab, back stack)
    browse_view: Option<(ItemId, bool)>,
    browse_back: Vec<(ItemId, bool)>,

    total_frames: u64,
    auto_shot: Option<String>,
    last_frame: Instant,
    last_title: Instant,
    frames: u32,
    fps: u32,
    ui: UiBatch,
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
fn content_tree_stamp_of(roots: &[&std::path::Path]) -> u64 {
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
fn reduced_damage(amount: f32, points: u32) -> f32 {
    amount * (1.0 - (points as f32 * 0.04).min(0.6))
}

/// First free "worldN" name. A name is taken if it's in the world list OR
/// its folder exists on disk at all — a new world must never adopt an
/// existing folder's chunks/player.toml, even one the listing can't parse.
fn next_world_name(saves: &std::path::Path, worlds: &[(String, u32)]) -> String {
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
pub fn browser_items(reg: &Registry, search: &str) -> Vec<ItemId> {
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
        // Dev override (never persisted): WILDFORGE_PACK=<id> selects a pack.
        let pack_override = std::env::var("WILDFORGE_PACK").ok();
        let active_pack = pack_override.clone().unwrap_or_else(|| config.pack.clone());
        let (atlas_data, atlas_px, pack_warnings) =
            atlas::build_atlas(&reg.tex_files, pack_source_of(&active_pack), &reg.tex_names);
        let renderer = pollster::block_on(renderer::Renderer::new(
            window.clone(),
            atlas_data,
            atlas_px,
        ));
        let mut scripts = script::ScriptHost::new();
        scripts.load_mods(&script_mod_dirs(&reg));
        // No world yet — the game opens on the title screen.
        let world = World::new(0, PathBuf::from("saves/.none"), reg.clone());
        let sim = server::Server::new(world, 0.3, 0x51ed_c0de);
        let spawn = Vec3::new(0.5, 80.0, 0.5);

        let size = window.inner_size();
        let aspect = size.width as f32 / size.height.max(1) as f32;
        let audio = Audio::new(config.volume);

        let mut g = Game {
            window,
            renderer,
            server: sim,
            player: Player::new(spawn),
            camera: Camera::new(spawn + Vec3::new(0.0, EYE_HEIGHT, 0.0), aspect),
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
            screen: Screen::Title,
            inventory: Inventory::new(),
            armor: [None; 5],
            bow_draw: 0.0,
            brushing: 0.0,
            brush_target: None,
            swing: 0.0,
            hand_bob: 0.0,
            weather_vis: 0.0,
            lightning: 0.0,
            thunder_delay: -1.0,
            atlas_season: 1,
            anvil_work: 0.0,
            anvil_pos: None,
            held_stack: None,
            craft_grid: [None; 9],
            craft_size: 2,
            items: Vec::new(),
            breaking: None,
            health: MAX_HEALTH,
            hunger: 20.0,
            nutrition: [0.0; 5],
            eating: 0.0,
            exhaustion_regen: 0.0,
            starve_timer: 0.0,
            air: MAX_AIR,
            since_damage: 100.0,
            drown_timer: 0.0,
            damage_flash: 0.0,
            fall_start: None,
            spawn_point: spawn,
            rng: 0x1234_5678
                ^ std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos())
                    .unwrap_or(0),
            reg,
            scripts,
            toasts: Vec::new(),
            mods_stamp: 0,
            mods_poll: 0.0,
            packs: atlas::discover_packs(),
            pack_warnings,
            pack_override,
            host: None,
            host_sleeping: false,
            remote: None,
            discovery: None,
            join_ip: String::new(),
            join_status: String::new(),
            chat_open: false,
            chat_text: String::new(),
            move_timer: 0.0,
            tick_accum: 0.0,
            config,
            audio,
            in_world: false,
            worlds: Vec::new(),
            settings_from_pause: false,
            pending_delete: None,
            dragging_slider: None,
            creative: false,
            flying: false,
            killed_by_wild: false,
            last_space: -9.0,
            time_abs: 0.0,
            search: String::new(),
            search_focus: false,
            browse_page: 0,
            browse_view: None,
            browse_back: Vec::new(),
            total_frames: 0,
            auto_shot: std::env::var("WILDFORGE_SHOT").ok(),
            last_frame: Instant::now(),
            last_title: Instant::now(),
            frames: 0,
            fps: 0,
            ui: UiBatch::new(),
        };
        g.mods_stamp = content_tree_stamp();
        g.apply_config();
        g.refresh_worlds();
        // Dev/headless: open a specific menu screen for UI verification.
        match std::env::var("WILDFORGE_SCREEN").as_deref() {
            Ok("mods") => g.screen = Screen::Mods,
            Ok("packs") => g.screen = Screen::Packs,
            Ok("settings") => g.screen = Screen::Settings,
            Ok("confirm") => {
                g.pending_delete = if g.worlds.is_empty() { None } else { Some(0) };
                g.screen = Screen::ConfirmDelete;
            }
            Ok("join") => {
                g.discovery = net::Discovery::start().ok();
                g.screen = Screen::Join;
            }
            _ => {}
        }
        g
    }

    fn sfx(&self, s: Sfx) {
        if let Some(a) = &self.audio {
            a.play(s);
        }
    }

    fn apply_config(&mut self) {
        self.camera.sens = self.config.sensitivity;
        self.camera.fovy = self.config.fov.to_radians();
        if let Some(a) = &mut self.audio {
            a.volume = self.config.volume;
        }
        self.config.save();
    }

    fn refresh_worlds(&mut self) {
        self.worlds = world::list_worlds(std::path::Path::new("saves"));
    }

    /// Load (or create) a world and enter it.
    fn start_world(&mut self, name: &str) {
        let mut world = World::load_or_create(PathBuf::from("saves").join(name), self.reg.clone());
        // Dev: WILDFORGE_SPAWN="x,z" overrides the spawn search.
        let (sx, sz) = std::env::var("WILDFORGE_SPAWN")
            .ok()
            .and_then(|s| {
                let (a, b) = s.split_once(',')?;
                Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
            })
            .unwrap_or_else(|| find_spawn(&world));
        let spawn_chunk = ChunkPos::of_world(sx, sz);
        for dx in -1..=1 {
            for dz in -1..=1 {
                world.ensure_chunk(ChunkPos {
                    x: spawn_chunk.x + dx,
                    z: spawn_chunk.z + dz,
                });
            }
        }
        // 3D terrain can put the "highest solid" on an overhang lip or a
        // spike; refine to a locally flat, dry column so spawning is safe.
        let (sx, sz) = {
            let mut best = (sx, sz);
            let mut best_score = i32::MAX;
            for dx in -12..=12 {
                for dz in -12..=12 {
                    let (x, z) = (sx + dx, sz + dz);
                    let h = world.surface_height(x, z);
                    if h <= SEA_LEVEL + 1 {
                        continue;
                    }
                    let mut slope = 0;
                    for (nx, nz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        slope = slope.max((h - world.surface_height(x + nx, z + nz)).abs());
                    }
                    let score = slope * 100 + dx.abs() + dz.abs();
                    if slope <= 1 {
                        best = (x, z);
                        best_score = 0;
                        break;
                    }
                    if score < best_score {
                        best_score = score;
                        best = (x, z);
                    }
                }
                if best_score == 0 {
                    break;
                }
            }
            best
        };
        let sy = world.surface_height(sx, sz) + 1;
        let spawn = Vec3::new(sx as f32 + 0.5, sy as f32 + 0.2, sz as f32 + 0.5);

        self.renderer.chunks.clear();
        self.server = server::Server::new(world, 0.3, self.rng ^ 0x5ee1);
        self.player = Player::new(spawn);
        self.spawn_point = spawn;
        self.camera.pos = spawn + Vec3::new(0.0, EYE_HEIGHT, 0.0);
        self.camera.yaw = -std::f32::consts::FRAC_PI_2;
        self.camera.pitch = 0.0;
        self.inventory = Inventory::new();
        self.armor = [None; 5];
        self.bow_draw = 0.0;
        if std::env::var("WILDFORGE_GIVE").is_ok() {
            let reg = self.reg.clone();
            for (name, n) in [
                ("base:dirt", 64),
                ("base:cobblestone", 32),
                ("base:log", 8),
                ("base:planks", 12),
                ("base:stick", 8),
                ("base:wood_pickaxe", 1),
                ("base:potato", 5),
                ("base:bread", 3),
                ("base:bronze_sword", 1),
                ("base:hunting_bow", 1),
                ("base:arrow", 16),
                ("base:leather_chestplate", 1),
            ] {
                if let Some(item) = reg.item_id(name) {
                    self.inventory.add(&reg, item, n);
                }
            }
            // Auto-equip a starter set so armor pips show in shots.
            for name in ["base:leather_helmet", "base:bronze_chestplate"] {
                if let Some(item) = reg.item_id(name)
                    && let Some((slot, _)) = reg.item(item).armor
                {
                    self.armor[slot as usize] = Some(ItemStack::new(&reg, item, 1));
                }
            }
        }
        self.held_stack = None;
        self.craft_grid = [None; 9];
        self.items.clear();
        self.breaking = None;
        self.health = MAX_HEALTH;
        self.killed_by_wild = false;
        self.hunger = 20.0;
        self.nutrition = [0.0; 5];
        self.eating = 0.0;
        self.exhaustion_regen = 0.0;
        self.starve_timer = 0.0;
        self.drown_timer = 0.0;
        self.air = MAX_AIR;
        self.since_damage = 100.0;
        self.damage_flash = 0.0;
        self.fall_start = None;
        self.server.time_of_day = 0.3;
        self.hotbar_sel = 0;
        let (_, mode, _) = world::read_world_meta(&PathBuf::from("saves").join(name));
        self.creative = mode == "creative";
        self.flying = false;
        self.in_world = true;
        if self.load_player(&PathBuf::from("saves").join(name)) {
            // Ensure the chunk under the restored position exists.
            let cp = ChunkPos::of_world(self.player.pos.x as i32, self.player.pos.z as i32);
            for dx in -1..=1 {
                for dz in -1..=1 {
                    self.server.world.ensure_chunk(ChunkPos {
                        x: cp.x + dx,
                        z: cp.z + dz,
                    });
                }
            }
            // A save from below the world floor (a void casualty) comes
            // back standing on whatever ground its column still has.
            if self.player.pos.y < 1.0 {
                let (px, pz) = (
                    self.player.pos.x.floor() as i32,
                    self.player.pos.z.floor() as i32,
                );
                let h = self.server.world.surface_height(px, pz);
                self.player.pos.y = h as f32 + 1.05;
                self.player.vel = Vec3::ZERO;
            }
        }
        // Dev: pick the hotbar slot screenshots hold up (after the
        // profile load so it isn't overwritten).
        if let Ok(s) = std::env::var("WILDFORGE_SEL")
            && let Ok(i) = s.parse::<usize>()
        {
            self.hotbar_sel = i.min(HOTBAR_SLOTS - 1);
        }
        self.server.sync_tier();
        self.scripts.load_kv(&PathBuf::from("saves").join(name));
        if self.scripts.wants("on_world_start") {
            self.scripts
                .dispatch(&self.server.world, "on_world_start", (name.to_string(),));
            self.apply_script_cmds();
        }
        // Dev: drop a water source on a pillar ahead of spawn to watch it flow.
        if std::env::var("WILDFORGE_DEMO_WATER").is_ok() {
            let (bx, bz) = (spawn.x as i32 - 6, spawn.z as i32 - 14);
            let by = self.server.world.surface_height(bx, bz);
            let stone = self.reg.block_id("base:stone").unwrap_or(AIR);
            let water = self.reg.block_id("base:water").unwrap_or(AIR);
            for y in by + 1..=by + 4 {
                self.server.world.set_block(bx, y, bz, stone);
            }
            self.server.world.set_block(bx, by + 5, bz, water);
            eprintln!(
                "demo water source at ({bx},{},{bz}), spawn {:?}",
                by + 5,
                spawn
            );
        }
        self.set_screen(Screen::Playing);
        // Dev: force time of day (0..1; 0.75 = midnight).
        if let Ok(t) = std::env::var("WILDFORGE_TIME")
            && let Ok(t) = t.parse::<f32>()
        {
            self.server.time_of_day = t.fract();
        }
        // Dev: force camera look ("yaw,pitch" in radians) for framed captures.
        if let Ok(l) = std::env::var("WILDFORGE_LOOK")
            && let Some((y, p)) = l.split_once(',')
            && let (Ok(y), Ok(p)) = (y.trim().parse::<f32>(), p.trim().parse::<f32>())
        {
            self.camera.yaw = y;
            self.camera.pitch = p;
        }
        // Dev: a ring of torches near spawn (lighting verification).
        if std::env::var("WILDFORGE_DEMO_TORCH").is_ok()
            && let Some(torch) = self.reg.block_id("base:torch")
        {
            for (dx, dz) in [(3, 0), (-3, 2), (0, 4), (2, -4)] {
                let (x, z) = (spawn.x as i32 + dx, spawn.z as i32 + dz);
                let y = self.server.world.surface_height(x, z);
                self.server.world.set_block(x, y + 1, z, torch);
            }
        }
        // Dev: two pillars flanked by a blue and a red lamp — colored-shadow
        // test (each pillar should cast a blue shadow away from the blue lamp
        // and a red one away from the red lamp, purple where both reach).
        if std::env::var("WILDFORGE_DEMO_COLORSHADOW").is_ok() {
            let blue = self.reg.block_id("base:blue_lamp");
            let red = self.reg.block_id("base:red_lamp");
            let stone = self.reg.block_id("base:cobblestone");
            let bx = spawn.x as i32;
            let bz = spawn.z as i32 + 4;
            let y = self.server.world.surface_height(bx, bz);
            if let Some(stone) = stone {
                // A neutral grey floor reads colored light far better than grass.
                for dx in -8..=8 {
                    for dz in -6..=8 {
                        self.server.world.set_block(bx + dx, y, bz + dz, stone);
                    }
                }
                // Two pillars as occluders.
                for px in [-2i32, 2] {
                    for h in 1..=3 {
                        self.server.world.set_block(bx + px, y + h, bz, stone);
                    }
                }
            }
            // Low colored lamps to either side so shadows rake across the floor.
            if let Some(b) = blue {
                self.server.world.set_block(bx - 5, y + 2, bz, b);
            }
            if let Some(r) = red {
                self.server.world.set_block(bx + 5, y + 2, bz, r);
            }
        }
        // Dev: a flat water pool ahead of spawn (specular-glint verification).
        if std::env::var("WILDFORGE_DEMO_POOL").is_ok()
            && let Some(water) = self.reg.block_id("base:water")
        {
            let cx = spawn.x as i32;
            let cz = spawn.z as i32 + 10;
            let y = self.server.world.surface_height(cx, cz);
            for dx in -8..=8 {
                for dz in -8..=8 {
                    self.server.world.set_block(cx + dx, y, cz + dz, water);
                }
            }
        }
        // Dev: a warm torch and a red ruby block side by side (colored-light
        // verification — pools of warm and red that blend where they meet).
        if std::env::var("WILDFORGE_DEMO_COLORLIGHT").is_ok() {
            let place = |w: &mut World, name: &str, dx: i32, dz: i32| {
                if let Some(b) = w.reg.block_id(name) {
                    let (x, z) = (spawn.x as i32 + dx, spawn.z as i32 + dz);
                    let y = w.surface_height(x, z);
                    w.set_block(x, y + 1, z, b);
                }
            };
            place(&mut self.server.world, "base:torch", -2, 5);
            place(&mut self.server.world, "gems:ruby_block", 2, 5);
        }
        // Dev: a few tall pillars near spawn (shadow-casting verification).
        if std::env::var("WILDFORGE_DEMO_PILLARS").is_ok()
            && let Some(stone) = self.reg.block_id("base:cobblestone")
        {
            for (dx, dz, h) in [(4, 2, 6), (7, -3, 8), (-2, 6, 5), (10, 4, 7)] {
                let (x, z) = (spawn.x as i32 + dx, spawn.z as i32 + dz);
                let base = self.server.world.surface_height(x, z);
                for i in 1..=h {
                    self.server.world.set_block(x, base + i, z, stone);
                }
            }
        }
        // Dev: a ready steelworks near spawn (bloomery shell + anvil +
        // materials) for screenshots and hands-on QA.
        if std::env::var("WILDFORGE_DEMO_STEELWORKS").is_ok() {
            let b = |n: &str| self.reg.block_id(n);
            if let (Some(fb), Some(mouth), Some(anvil)) = (
                b("base:firebrick"),
                b("base:bloomery"),
                b("base:stone_anvil"),
            ) {
                let (sx, sz) = (spawn.x as i32 + 6, spawn.z as i32 + 4);
                let sy = self.server.world.surface_height(sx, sz) + 1;
                // Core at (sx, sy, sz); mouth on its -X side.
                for ly in 0..3 {
                    for rx in -1..=1i32 {
                        for rz in -1..=1i32 {
                            if rx == 0 && rz == 0 {
                                continue;
                            }
                            self.server.world.set_block(sx + rx, sy + ly, sz + rz, fb);
                        }
                    }
                    self.server
                        .world
                        .set_block(sx, sy + ly, sz, crate::registry::AIR);
                }
                self.server.world.set_block(sx - 1, sy, sz, mouth);
                self.server.world.set_block(sx - 3, sy, sz + 2, anvil);
                // A second stack, already charged and burning.
                let (lx, lz) = (sx, sz + 8);
                let ly = self.server.world.surface_height(lx, lz) + 1;
                for dy in 0..3 {
                    for rx in -1..=1i32 {
                        for rz in -1..=1i32 {
                            if rx == 0 && rz == 0 {
                                continue;
                            }
                            self.server.world.set_block(lx + rx, ly + dy, lz + rz, fb);
                        }
                    }
                    self.server
                        .world
                        .set_block(lx, ly + dy, lz, crate::registry::AIR);
                }
                self.server.world.set_block(lx - 1, ly, lz, mouth);
                let reg2 = self.reg.clone();
                if let (Some(iron), Some(coal)) = (
                    reg2.item_id("base:iron_ingot"),
                    reg2.item_id("base:charcoal"),
                ) {
                    let mut st = world::BloomeryState::default();
                    for i in 0..4 {
                        st.charge[i] = Some(ItemStack::new(&reg2, iron, 2));
                        st.fuel[i] = Some(ItemStack::new(&reg2, coal, 2));
                    }
                    self.server
                        .world
                        .block_entities
                        .insert((lx - 1, ly, lz), world::BlockEntity::Bloomery(st));
                    let _ = self.server.world.light_bloomery(lx - 1, ly, lz);
                }
                // A bloom resting on the anvil, ready for the hammer.
                if let Some(bl) = reg2.item_id("base:steel_bloom") {
                    self.server
                        .world
                        .anvil_put((sx - 3, sy, sz + 2), ItemStack::new(&reg2, bl, 1));
                }
                let reg = self.reg.clone();
                for (name, n) in [
                    ("base:iron_ingot", 8),
                    ("base:charcoal", 8),
                    ("base:ember", 2),
                    ("base:smith_hammer", 1),
                    ("base:steel_bloom", 2),
                    ("base:log", 8),
                    ("base:dirt", 32),
                ] {
                    if let Some(item) = reg.item_id(name) {
                        self.inventory.add(&reg, item, n);
                    }
                }
            }
        }
        // Dev: WILDFORGE_IRE=N forces the wild's ire (spawn testing).
        if let Ok(v) = std::env::var("WILDFORGE_IRE")
            && let Ok(v) = v.parse::<f32>()
        {
            self.server.world.ire = v.clamp(0.0, 100.0);
            self.server.sync_tier();
        }
        // Dev: force the calendar and the sky.
        if let Ok(v) = std::env::var("WILDFORGE_DAY")
            && let Ok(v) = v.parse::<u32>()
        {
            self.server.world.day = v;
        }
        if let Ok(v) = std::env::var("WILDFORGE_SEASON")
            && let Ok(v) = v.parse::<u32>()
        {
            self.server.world.day = (v % 4) * world::SEASON_DAYS;
        }
        if let Ok(v) = std::env::var("WILDFORGE_WEATHER") {
            self.server.world.weather = world::Weather::from_name(&v);
            self.server.world.weather_timer = 1.0e9; // pinned for the session
            self.weather_vis = match self.server.world.weather {
                world::Weather::Clear => 0.0,
                world::Weather::Overcast => 0.4,
                world::Weather::Precip => 0.55,
                world::Weather::Storm => 0.7,
            };
        }
        // Dev: a row of wardens near spawn (rendering/combat verification).
        if std::env::var("WILDFORGE_DEMO_WARDENS").is_ok() {
            for (i, name) in [
                "base:thornling",
                "base:dryad",
                "base:emberkin",
                "base:gravelurk",
                "base:wrathwood",
            ]
            .iter()
            .enumerate()
            {
                if let Some(si) = self.reg.animal_id(name) {
                    let x = spawn.x as i32 - 4 + i as i32 * 3;
                    let z = spawn.z as i32 - 7;
                    let y = self.server.world.surface_height(x, z) + 1;
                    let mut m = mobs::Mob::new(
                        si,
                        Vec3::new(x as f32 + 0.5, y as f32 + 0.05, z as f32 + 0.5),
                        0.0,
                    );
                    m.health = self.reg.animals[si].health;
                    self.server.world.mobs.push(m);
                }
            }
        }
        // Dev: stewardship showcase — offering stone with gifts, a planted
        // sapling, and a grown oak (verification).
        if std::env::var("WILDFORGE_DEMO_STEWARD").is_ok() {
            let reg = self.reg.clone();
            let (sx, sz) = (spawn.x as i32, spawn.z as i32);
            if let Some(os) = reg.block_id("base:offering_stone") {
                let y = self.server.world.surface_height(sx - 3, sz - 5) + 1;
                self.server.world.set_block(sx - 3, y, sz - 5, os);
                let mut st = world::OfferingState::default();
                if let Some(hw) = reg.item_id("base:heartwood") {
                    st.slots[0] = Some(ItemStack::new(&reg, hw, 2));
                }
                self.server
                    .world
                    .block_entities
                    .insert((sx - 3, y, sz - 5), world::BlockEntity::Offering(st));
            }
            if let Some(sap) = reg.block_id("base:oak_sapling") {
                let y = self.server.world.surface_height(sx + 2, sz - 6) + 1;
                self.server.world.set_block(sx + 2, y, sz - 6, sap);
            }
            let ty = self.server.world.surface_height(sx + 6, sz - 8) + 1;
            self.server.world.grow_tree(sx + 6, ty, sz - 8, "oak", 3);
            for name in ["base:bedroll", "base:oak_sapling"] {
                if let Some(item) = reg.item_id(name) {
                    self.inventory.add(&reg, item, 1);
                }
            }
        }
        // Dev: a stocked chest next to spawn, screen open (UI verification).
        if std::env::var("WILDFORGE_DEMO_CHEST").is_ok() {
            let p = (spawn.x as i32 - 2, spawn.y as i32, spawn.z as i32);
            let reg = self.reg.clone();
            if let Some(cb) = reg.block_id("base:chest") {
                self.server.world.set_block(p.0, p.1, p.2, cb);
                let mut st = world::ChestState::default();
                for (i, (name, n)) in [
                    ("base:bread", 5),
                    ("base:torch", 12),
                    ("base:bronze_sword", 1),
                ]
                .iter()
                .enumerate()
                {
                    if let Some(item) = reg.item_id(name) {
                        st.slots[i * 4] = Some(ItemStack::new(&reg, item, *n));
                    }
                }
                self.server
                    .world
                    .block_entities
                    .insert(p, world::BlockEntity::Chest(st));
                self.set_screen(Screen::Chest(p));
            }
        }
        // Dev/headless: open the inventory for UI verification.
        if std::env::var("WILDFORGE_SCREEN").as_deref() == Ok("inventory") {
            self.craft_size = 2;
            self.set_screen(Screen::Inventory);
        }
        // Dev: a small menagerie near spawn (rendering/combat verification).
        if std::env::var("WILDFORGE_DEMO_MOBS").is_ok() {
            for (i, name) in [
                "base:deer",
                "base:boar",
                "base:goat",
                "base:grouse",
                "base:rabbit",
            ]
            .iter()
            .enumerate()
            {
                if let Some(si) = self.reg.animal_id(name) {
                    let x = spawn.x as i32 - 3 + i as i32 * 2;
                    let z = spawn.z as i32 - 6;
                    let y = self.server.world.surface_height(x, z) + 1;
                    let mut m = mobs::Mob::new(
                        si,
                        Vec3::new(x as f32 + 0.5, y as f32 + 0.05, z as f32 + 0.5),
                        i as f32 * 1.3,
                    );
                    m.health = self.reg.animals[si].health;
                    self.server.world.mobs.push(m);
                }
            }
        }
        // Dev: a stocked furnace next to spawn, screen open (UI verification).
        if std::env::var("WILDFORGE_DEMO_FURNACE").is_ok() {
            let p = (spawn.x as i32 + 2, spawn.y as i32, spawn.z as i32);
            let reg = self.reg.clone();
            if let (Some(fb), Some(raw), Some(log)) = (
                reg.block_id("base:furnace"),
                reg.item_id("base:raw_copper"),
                reg.item_id("base:log"),
            ) {
                self.server.world.set_block(p.0, p.1, p.2, fb);
                self.server.world.block_entities.insert(
                    p,
                    world::BlockEntity::Furnace(world::FurnaceState {
                        input: Some(ItemStack::new(&reg, raw, 5)),
                        fuel: Some(ItemStack::new(&reg, log, 3)),
                        ..Default::default()
                    }),
                );
                self.inventory
                    .add(&reg, reg.item_id("base:copper_ingot").unwrap(), 7);
                self.set_screen(Screen::Furnace(p));
            }
        }
    }

    /// Create a fresh world folder with a random seed and enter it.
    fn new_world_mode(&mut self, mode: &str) {
        let name = next_world_name(std::path::Path::new("saves"), &self.worlds);
        let seed = (self.rand01() * u32::MAX as f32) as u32;
        world::write_world_meta(&PathBuf::from("saves").join(&name), seed, mode, 0.0);
        self.refresh_worlds();
        self.start_world(&name);
    }

    fn save_player(&self) {
        if !self.in_world {
            return;
        }
        use std::fmt::Write as _;
        let mut out = String::new();
        let p = self.player.pos;
        let _ = writeln!(out, "pos = [{}, {}, {}]", p.x, p.y, p.z);
        let _ = writeln!(
            out,
            "yaw = {}\npitch = {}",
            self.camera.yaw, self.camera.pitch
        );
        let _ = writeln!(out, "health = {}\nhunger = {}", self.health, self.hunger);
        let _ = writeln!(out, "nutrition = {:?}", self.nutrition);
        let _ = writeln!(out, "hotbar = {}", self.hotbar_sel);
        let sp = self.spawn_point;
        let _ = writeln!(out, "spawn = [{}, {}, {}]", sp.x, sp.y, sp.z);
        for (i, s) in self.inventory.slots.iter().enumerate() {
            if let Some(s) = s {
                let _ = writeln!(
                    out,
                    "[[slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}",
                    self.reg.item(s.item).name,
                    s.count,
                    s.durability
                );
            }
        }
        for (i, s) in self.armor.iter().enumerate() {
            if let Some(s) = s {
                let _ = writeln!(
                    out,
                    "[[armor]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}",
                    self.reg.item(s.item).name,
                    s.count,
                    s.durability
                );
            }
        }
        let _ = std::fs::write(
            self.server.world.save_dir_for_saving().join("player.toml"),
            out,
        );
    }

    fn load_player(&mut self, dir: &std::path::Path) -> bool {
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct SlotT {
            index: usize,
            item: String,
            count: u32,
            durability: u32,
        }
        #[derive(Deserialize)]
        struct P {
            pos: [f32; 3],
            yaw: f32,
            pitch: f32,
            health: f32,
            hunger: f32,
            nutrition: [f32; 5],
            hotbar: usize,
            #[serde(default)]
            spawn: Option<[f32; 3]>,
            #[serde(default)]
            slot: Vec<SlotT>,
            #[serde(default)]
            armor: Vec<SlotT>,
        }
        let Ok(text) = std::fs::read_to_string(dir.join("player.toml")) else {
            return false;
        };
        let Ok(p) = toml::from_str::<P>(&text) else {
            return false;
        };
        self.player.pos = Vec3::new(p.pos[0], p.pos[1], p.pos[2]);
        self.camera.yaw = p.yaw;
        self.camera.pitch = p.pitch;
        self.health = p.health;
        self.hunger = p.hunger;
        self.nutrition = p.nutrition;
        self.hotbar_sel = p.hotbar.min(HOTBAR_SLOTS - 1);
        if let Some(sp) = p.spawn {
            self.spawn_point = Vec3::new(sp[0], sp[1], sp[2]);
        }
        for s in p.slot {
            if s.index < TOTAL_SLOTS
                && let Some(item) = self.reg.item_id(&s.item)
            {
                self.inventory.slots[s.index] = Some(ItemStack {
                    item,
                    count: s.count,
                    durability: s.durability,
                });
            }
        }
        for s in p.armor {
            if s.index < 5
                && let Some(item) = self.reg.item_id(&s.item)
            {
                self.armor[s.index] = Some(ItemStack {
                    item,
                    count: s.count,
                    durability: s.durability,
                });
            }
        }
        true
    }

    fn quit_to_title(&mut self) {
        self.host = None; // closes connections
        self.host_sleeping = false;
        self.server.world.log_edits = false;
        if self.remote.is_some() {
            self.save_player(); // guest profile under saves/.remote/profile
            self.remote = None;
            self.renderer.chunks.clear();
            self.server = server::Server::new(
                World::new(0, PathBuf::from("saves/.none"), self.reg.clone()),
                0.3,
                1,
            );
            self.items.clear();
            self.in_world = false;
            self.refresh_worlds();
            self.set_screen(Screen::Title);
            return;
        }
        if self.in_world {
            self.save_player();
            self.server.world.settle_falling();
            self.server.world.save_modified();
            self.scripts
                .save_kv(&self.server.world.save_dir_for_saving());
        }
        self.renderer.chunks.clear();
        self.server = server::Server::new(
            World::new(0, PathBuf::from("saves/.none"), self.reg.clone()),
            0.3,
            1,
        );
        self.items.clear();
        self.in_world = false;
        self.refresh_worlds();
        self.set_screen(Screen::Title);
    }

    fn rand01(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.rng >> 8) as f32 / (1 << 24) as f32
    }

    fn capture_mouse(&mut self, capture: bool) {
        if capture {
            // A Locked grab pins the cursor: raw deltas are the only signal.
            // Anything less (Confined, or no grab at all): use cursor-position
            // deltas + recentering instead — raw deltas are unreliable on some
            // stacks (notably WSLg's XWayland).
            self.raw_look = self.window.set_cursor_grab(CursorGrabMode::Locked).is_ok();
            if !self.raw_look {
                let _ = self.window.set_cursor_grab(CursorGrabMode::Confined);
                self.last_cursor = None;
                self.warp_pending =
                    self.allow_warp && self.window.set_cursor_position(self.center()).is_ok();
            }
        } else {
            let _ = self.window.set_cursor_grab(CursorGrabMode::None);
        }
        self.window.set_cursor_visible(!capture);
        self.mouse_captured = capture;
    }

    fn center(&self) -> winit::dpi::PhysicalPosition<f64> {
        let size = self.window.inner_size();
        winit::dpi::PhysicalPosition::new(size.width as f64 / 2.0, size.height as f64 / 2.0)
    }

    /// Cursor-position-based look using successive position differences —
    /// exact 1:1 deltas even when events are coalesced into big jumps.
    /// The cursor is kept pinned in a small bubble around the window center;
    /// the warp's own event is recognized by landing exactly on center, so
    /// real motion events are never swallowed.
    fn cursor_look(&mut self, pos: winit::dpi::PhysicalPosition<f64>) {
        let c = self.center();
        if self.warp_pending && (pos.x - c.x).abs() < 1.5 && (pos.y - c.y).abs() < 1.5 {
            self.warp_pending = false;
            self.last_cursor = Some((c.x, c.y));
            return;
        }
        if let Some((lx, ly)) = self.last_cursor {
            self.camera.turn((pos.x - lx) as f32, (pos.y - ly) as f32);
        }
        self.last_cursor = Some((pos.x, pos.y));

        if self.allow_warp
            && ((pos.x - c.x).abs() > 40.0 || (pos.y - c.y).abs() > 40.0)
            && self.window.set_cursor_position(c).is_ok()
        {
            self.warp_pending = true;
        }
    }

    fn set_screen(&mut self, screen: Screen) {
        if self.screen == screen {
            return;
        }
        self.bow_draw = 0.0; // opening any screen relaxes the draw

        // Leaving a container tells the host to stop streaming it.
        if matches!(
            self.screen,
            Screen::Furnace(_) | Screen::Chest(_) | Screen::Offering(_) | Screen::Bloomery(_)
        ) && let Some(r) = &self.remote
        {
            r.client.send(&net::C2S::CloseContainer);
        }
        // Leaving the inventory returns the cursor-held stack and craft grid.
        if self.screen == Screen::Inventory
            || matches!(
                self.screen,
                Screen::Furnace(_) | Screen::Chest(_) | Screen::Offering(_) | Screen::Bloomery(_)
            )
        {
            let mut back: Vec<ItemStack> = self.held_stack.take().into_iter().collect();
            for slot in self.craft_grid.iter_mut() {
                if let Some(s) = slot.take() {
                    back.push(s);
                }
            }
            let reg = self.reg.clone();
            for s in back {
                let left = self.inventory.add_stack(&reg, s);
                if left > 0 {
                    self.drop_stack(ItemStack { count: left, ..s });
                }
            }
        }
        self.screen = screen;
        let playing = screen == Screen::Playing;
        if playing {
            self.capture_mouse(true);
        } else {
            self.capture_mouse(false);
            self.keys = KeysDown::default();
            self.left_held = false;
            self.right_held = false;
            self.breaking = None;
        }
    }

    /// Join a host: blocks briefly for the QUIC handshake, then the
    /// Welcome/ModFiles flow finishes in remote_pump.
    fn join_server(&mut self, addr: std::net::SocketAddr) {
        let name = whoami();
        let hash = net::content_hash(std::path::Path::new("mods"));
        match net::Client::connect(addr, name, hash) {
            Ok(client) => {
                self.remote = Some(Remote {
                    client,
                    my_id: 0,
                    block_map: Vec::new(),
                    item_map: Vec::new(),
                    host_block: Default::default(),
                    players: Default::default(),
                    names: Default::default(),
                    sleeping: false,
                    player_lerp: Default::default(),
                    player_age: 0.0,
                    player_interval: 0.05,
                    mob_lerp: Default::default(),
                    mob_age: 0.0,
                    mob_interval: 0.05,
                });
                self.join_status = "CONNECTED - SYNCING...".to_string();
            }
            Err(e) => {
                self.join_status = format!("FAILED: {e}").to_uppercase();
            }
        }
    }

    /// Everything a guest does per frame: apply the host's stream, send
    /// our movement. The local Server never advances in remote mode.
    fn remote_pump(&mut self, dt: f32) {
        let Some(mut r) = self.remote.take() else {
            return;
        };
        if !r.client.is_connected() && self.in_world {
            self.toast("Disconnected from host.".to_string());
            self.quit_to_title();
            return; // remote dropped
        }
        let msgs = r.client.poll();
        for msg in msgs {
            match msg {
                net::S2C::ModFiles(files) => {
                    // The host's content, cached and loaded as ours.
                    let cache = PathBuf::from("saves/.remote/mods");
                    let _ = std::fs::remove_dir_all(&cache);
                    for (rel, bytes) in files {
                        if rel.contains("..") {
                            continue; // no path escapes
                        }
                        let p = cache.join(rel);
                        if let Some(parent) = p.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        let _ = std::fs::write(p, bytes);
                    }
                    self.reg = Arc::new(registry::load(&cache));
                    let (mut data, px, warns) = atlas::build_atlas(
                        &self.reg.tex_files,
                        pack_source_of(&self.active_pack_id()),
                        &self.reg.tex_names,
                    );
                    atlas::season_tint(&mut data, px, self.server.world.season());
                    self.atlas_season = self.server.world.season();
                    self.pack_warnings = warns;
                    self.renderer.set_atlas(&data, px);
                    self.toast("Synced the host's mods.".to_string());
                }
                net::S2C::Welcome {
                    seed,
                    mode,
                    time,
                    ire,
                    palette,
                    items,
                    your_id,
                    spawn,
                    world_name,
                } => {
                    let mut world = World::new(
                        seed,
                        PathBuf::from("saves/.remote/profile"),
                        self.reg.clone(),
                    );
                    world.remote = true;
                    world.mode = mode.clone();
                    world.ire = ire;
                    r.my_id = your_id;
                    r.block_map = mp::block_remap(&world, &palette);
                    r.item_map = mp::item_remap(&world, &items);
                    r.host_block = r
                        .block_map
                        .iter()
                        .enumerate()
                        .map(|(host, local)| (local.0, host as u16))
                        .collect();
                    self.server = server::Server::new(world, time, 7);
                    self.renderer.chunks.clear();
                    self.player = Player::new(spawn);
                    self.spawn_point = spawn;
                    self.camera.pos = spawn + Vec3::new(0.0, EYE_HEIGHT, 0.0);
                    self.inventory = Inventory::new();
                    self.armor = [None; 5];
                    self.health = MAX_HEALTH;
                    self.hunger = 20.0;
                    self.creative = mode == "creative";
                    self.in_world = true;
                    self.load_player(&PathBuf::from("saves/.remote/profile"));
                    self.set_screen(Screen::Playing);
                    self.toast(format!("Joined {}.", world_name.to_uppercase()));
                }
                net::S2C::Refused(why) => {
                    if self.in_world {
                        // Kicked mid-game: a clean exit, not a broken
                        // half-local world.
                        self.toast(format!("Removed by host: {why}"));
                        self.quit_to_title();
                    } else {
                        self.join_status = format!("REFUSED: {why}").to_uppercase();
                        self.remote = None;
                    }
                    return;
                }
                net::S2C::Chunk { x, z, rle } => {
                    self.server
                        .world
                        .insert_remote_chunk(ChunkPos { x, z }, &rle, &r.block_map);
                }
                net::S2C::BlockSet { x, y, z, id } => {
                    let local = r
                        .block_map
                        .get(id as usize)
                        .copied()
                        .unwrap_or(self.reg.unknown_block);
                    self.server.world.set_block(x, y, z, local);
                    self.server.world.pending_drops.clear();
                }
                net::S2C::Players(list) => {
                    // New span: from wherever each player currently
                    // renders, toward the fresh snapshot.
                    let t = (r.player_age / r.player_interval.max(0.001)).clamp(0.0, 1.0);
                    for (id, pos, yaw) in list {
                        if id == r.my_id {
                            continue;
                        }
                        let cur = match r.player_lerp.get(&id) {
                            Some(l) => l.at(t),
                            None => (pos, yaw),
                        };
                        r.player_lerp.insert(
                            id,
                            Lerp {
                                from: cur.0,
                                to: pos,
                                from_yaw: cur.1,
                                to_yaw: yaw,
                                phase: 0.0,
                            },
                        );
                        let name = r
                            .names
                            .get(&id)
                            .cloned()
                            .unwrap_or_else(|| format!("P{id}"));
                        r.players.insert(id, (name, cur.0, cur.1));
                    }
                    r.player_interval = r.player_age.clamp(0.03, 0.3);
                    r.player_age = 0.0;
                }
                net::S2C::Mobs(snaps) => {
                    let t = (r.mob_age / r.mob_interval.max(0.001)).clamp(0.0, 1.0);
                    let mut lerps = std::collections::HashMap::new();
                    self.server.world.mobs = snaps
                        .into_iter()
                        .filter(|s| (s.species as usize) < self.reg.animals.len())
                        .map(|s| {
                            let (cur, phase) = match r.mob_lerp.get(&s.id) {
                                Some(l) if s.id != 0 => (l.at(t), l.phase),
                                _ => ((s.pos, s.yaw), 0.0),
                            };
                            lerps.insert(
                                s.id,
                                Lerp {
                                    from: cur.0,
                                    to: s.pos,
                                    from_yaw: cur.1,
                                    to_yaw: s.yaw,
                                    phase,
                                },
                            );
                            let mut m = mobs::Mob::new(s.species as usize, cur.0, cur.1);
                            m.id = s.id;
                            m.growth = s.growth;
                            m.hurt_flash = s.hurt;
                            m.fed = s.fed; // "won't take food" — gates guest feeding
                            m.health = 1.0;
                            m.anim_phase = phase;
                            m
                        })
                        .collect();
                    r.mob_lerp = lerps; // dead mobs' spans fall away
                    r.mob_interval = r.mob_age.clamp(0.03, 0.3);
                    r.mob_age = 0.0;
                }
                net::S2C::Falling(snaps) => {
                    self.server.world.falling = snaps
                        .into_iter()
                        .map(|f| world::FallingBlock {
                            pos: f.pos,
                            vel: 0.0,
                            block: r
                                .block_map
                                .get(f.block as usize)
                                .copied()
                                .unwrap_or(self.reg.unknown_block),
                        })
                        .collect();
                }
                net::S2C::Bolts(snaps) => {
                    self.server.world.projectiles = snaps
                        .into_iter()
                        .map(|s| mobs::Projectile {
                            pos: s.pos,
                            // Dead-reckoned between snapshots below.
                            vel: s.vel,
                            tile: s.tile,
                            damage: 0.0,
                            age: s.age,
                            from_player: false,
                            drop_item: None,
                            owner: 0,
                        })
                        .collect();
                }
                net::S2C::TimeIre {
                    time,
                    ire,
                    day,
                    weather,
                } => {
                    self.server.time_of_day = time;
                    self.server.world.ire = ire;
                    self.server.world.day = day;
                    self.server.world.weather = world::Weather::from_u8(weather);
                }
                net::S2C::Hit { dmg, from } => self.hurt_player_from_wild(dmg, from),
                net::S2C::Give {
                    item,
                    count,
                    durability,
                } => {
                    if let Some(Some(local)) = r.item_map.get(item as usize) {
                        let reg = self.reg.clone();
                        let mut stack = ItemStack::new(&reg, *local, count.max(1));
                        if durability > 0 {
                            stack.durability = durability;
                        }
                        let left = self.inventory.add_stack(&reg, stack);
                        if left == 0 {
                            self.sfx(Sfx::Pickup);
                        }
                    }
                }
                net::S2C::Container {
                    x,
                    y,
                    z,
                    kind,
                    slots,
                    aux,
                } => {
                    let reg = self.reg.clone();
                    let conv = |s: &Option<net::StackSnap>| -> Option<ItemStack> {
                        let s = s.as_ref()?;
                        let local = (*r.item_map.get(s.item as usize)?)?;
                        Some(ItemStack {
                            item: local,
                            count: s.count,
                            durability: s.durability,
                        })
                    };
                    let pos = (x, y, z);
                    let entity = match kind {
                        0 => {
                            let mut c = world::ChestState::default();
                            for (i, s) in slots.iter().enumerate().take(world::CHEST_SLOTS) {
                                c.slots[i] = conv(s);
                            }
                            world::BlockEntity::Chest(c)
                        }
                        1 => {
                            let f = world::FurnaceState {
                                input: slots.first().and_then(&conv),
                                fuel: slots.get(1).and_then(&conv),
                                output: slots.get(2).and_then(&conv),
                                progress: aux.first().copied().unwrap_or(0.0),
                                burn_left: aux.get(1).copied().unwrap_or(0.0),
                                burn_total: aux.get(2).copied().unwrap_or(0.0),
                                ..Default::default()
                            };
                            world::BlockEntity::Furnace(f)
                        }
                        3 => {
                            let mut b = world::BloomeryState {
                                lit: aux.first().copied().unwrap_or(0.0) > 0.5,
                                progress: aux.get(1).copied().unwrap_or(0.0)
                                    * world::BLOOMERY_FIRE_SECS,
                                ..Default::default()
                            };
                            for (i, s) in slots.iter().enumerate().take(8) {
                                if i < 4 {
                                    b.charge[i] = conv(s);
                                } else {
                                    b.fuel[i - 4] = conv(s);
                                }
                            }
                            world::BlockEntity::Bloomery(b)
                        }
                        _ => {
                            let mut o = world::OfferingState::default();
                            for (i, s) in slots.iter().enumerate().take(3) {
                                o.slots[i] = conv(s);
                            }
                            world::BlockEntity::Offering(o)
                        }
                    };
                    self.server.world.block_entities.insert(pos, entity);
                    if matches!(self.screen, Screen::Playing) {
                        self.set_screen(match kind {
                            0 => Screen::Chest(pos),
                            1 => Screen::Furnace(pos),
                            3 => Screen::Bloomery(pos),
                            _ => Screen::Offering(pos),
                        });
                    }
                    let _ = reg;
                }
                net::S2C::HeldResult(held) => {
                    // The authoritative cursor after our click replaces
                    // the local prediction (identical on agreement).
                    self.held_stack = held.and_then(|s| {
                        let local = (*r.item_map.get(s.item as usize)?)?;
                        Some(ItemStack {
                            item: local,
                            count: s.count,
                            durability: s.durability,
                        })
                    });
                }
                net::S2C::Sleep { sleeping, present } => {
                    self.toast(format!("{sleeping}/{present} sleeping..."));
                }
                net::S2C::Toast(msg) => self.toast(msg),
                net::S2C::Chat { from, msg } => self.toast(format!("{from}: {msg}")),
                net::S2C::Joined { id, name } => {
                    r.names.insert(id, name.clone());
                    if id != r.my_id {
                        self.toast(format!("{name} joined."));
                    }
                }
                net::S2C::Left { id } => {
                    r.players.remove(&id);
                    r.player_lerp.remove(&id);
                    if let Some(n) = r.names.remove(&id) {
                        self.toast(format!("{n} left."));
                    }
                }
            }
        }
        // Snapshot smoothing: glide players and mobs along their spans,
        // dead-reckon bolts, advance walk cycles from apparent speed.
        r.player_age += dt;
        r.mob_age += dt;
        let t = (r.player_age / r.player_interval.max(0.001)).clamp(0.0, 1.0);
        for (id, entry) in r.players.iter_mut() {
            if let Some(l) = r.player_lerp.get(id) {
                let (p, y) = l.at(t);
                entry.1 = p;
                entry.2 = y;
            }
        }
        let t = (r.mob_age / r.mob_interval.max(0.001)).clamp(0.0, 1.0);
        for m in &mut self.server.world.mobs {
            if let Some(l) = r.mob_lerp.get_mut(&m.id) {
                let (p, y) = l.at(t);
                m.pos = p;
                m.yaw = y;
                let d = l.to - l.from;
                let hspeed = Vec3::new(d.x, 0.0, d.z).length() / r.mob_interval.max(0.03);
                l.phase += hspeed * dt * 3.2; // same feel as the local tick
                m.anim_phase = l.phase;
                m.hurt_flash = (m.hurt_flash - dt).max(0.0);
            }
        }
        for p in &mut self.server.world.projectiles {
            p.pos += p.vel * dt;
            p.age += dt;
        }
        // Our movement upstream at 20 Hz.
        if self.in_world {
            self.move_timer += dt;
            if self.move_timer >= 0.05 {
                self.move_timer = 0.0;
                r.client.send_datagram(&net::C2S::Move {
                    pos: self.player.pos,
                    yaw: self.camera.yaw,
                });
            }
        }
        self.remote = Some(r);
    }

    /// A line from the lost takers, chosen at random.
    fn read_tablet(&mut self) {
        const LINES: [&str; 10] = [
            "We burned the south wood. The nights grew teeth.",
            "The forge ran hot for a year. Then the trees walked.",
            "Plant after you cut. My father knew. I forgot.",
            "It does not hate. It answers.",
            "We left the stone offerings too late.",
            "The deep ones never slept. We dug anyway.",
            "Third winter: the wisps crossed the river.",
            "Feed the land and it feeds you. Starve it and it comes.",
            "My daughter planted a row of oaks. They spared her field.",
            "If you read this: the wild forgives. Slowly.",
        ];
        let i = (self.rand01() * LINES.len() as f32) as usize % LINES.len();
        self.toast(LINES[i].to_string());
        self.sfx(Sfx::Click);
    }

    /// Bedroll: sleep to dawn if it's night and the wild is far enough.
    /// In multiplayer, dawn waits for everyone (the sleep vote).
    fn try_sleep(&mut self) {
        let sun = (self.server.time_of_day * std::f32::consts::TAU).sin();
        if sun > -0.05 {
            self.toast("You can only sleep at night.".to_string());
            return;
        }
        if let Some(r) = &mut self.remote {
            r.client.send(&net::C2S::SleepRequest);
            r.sleeping = true;
            self.toast("You settle in, waiting for the others... (move to get up)".to_string());
            return;
        }
        if self.host.as_ref().is_some_and(|h| !h.guests.is_empty()) {
            self.host_sleeping = true;
            self.spawn_point = self.player.pos;
            self.toast("You settle in, waiting for the others... (move to get up)".to_string());
            return;
        }
        let reg = self.reg.clone();
        let near_warden = self.server.world.mobs.iter().any(|m| {
            reg.animals.get(m.species).is_some_and(|d| d.hostile)
                && (m.pos - self.player.pos).length_squared() < 24.0 * 24.0
        });
        if near_warden {
            self.toast("The wild is too close.".to_string());
            return;
        }
        // Time passes fairly: the skipped night still decays ire.
        let skipped = (1.0 + 0.3 - self.server.time_of_day) % 1.0;
        if self.server.world.tick_ire(skipped) {
            let r = self.server.world.accept_offerings();
            if r > 0.0 {
                self.toast("The wild has accepted your offering.".to_string());
            }
        }
        self.server.sleep_to_dawn();
        self.spawn_point = self.player.pos;
        if !self.creative {
            self.inventory.wear_tool(&reg, self.hotbar_sel);
        }
        self.save_player();
        self.server.world.settle_falling();
        self.server.world.save_modified();
        self.toast("You camp until dawn. This is home now.".to_string());
        self.sfx(Sfx::Craft);
    }

    fn armor_points(&self) -> u32 {
        let base: u32 = self
            .armor
            .iter()
            .flatten()
            .filter_map(|s| self.reg.item(s.item).armor.map(|(_, p)| p))
            .sum();
        base + if self.charm("bark") { 1 } else { 0 }
    }

    /// Is a charm of this kind worn?
    fn charm(&self, kind: &str) -> bool {
        self.armor[4]
            .as_ref()
            .is_some_and(|s| self.reg.item(s.item).charm.as_deref() == Some(kind))
    }

    /// Damage from a warden: knockback away from the attacker, and the
    /// death screen knows who to blame. Armor blocks 4% per point (cap
    /// 60%) and wears; it does nothing against falls or hunger.
    fn hurt_player_from_wild(&mut self, amount: f32, from: Vec3) {
        if self.creative || self.screen == Screen::Dead {
            return;
        }
        let pts = self.armor_points();
        let amount = reduced_damage(amount, pts);
        if pts > 0 {
            let reg = self.reg.clone();
            for a in self.armor.iter_mut() {
                if let Some(st) = a {
                    if reg.item(st.item).durability == 0 {
                        continue; // charms don't wear
                    }
                    st.durability = st.durability.saturating_sub(1);
                    if st.durability == 0 {
                        *a = None; // worn through
                    }
                }
            }
        }
        let mut away = self.player.pos - from;
        away.y = 0.0;
        if away.length_squared() > 0.001 {
            let dir = away.normalize();
            self.player.vel += dir * 6.0 + Vec3::new(0.0, 3.5, 0.0);
        }
        self.killed_by_wild = true;
        self.damage(amount);
        self.killed_by_wild = self.health <= 0.0;
    }

    fn damage(&mut self, amount: f32) {
        if amount <= 0.0 || self.screen == Screen::Dead || self.creative {
            return;
        }
        if std::env::var("WILDFORGE_DEBUG").is_ok() {
            eprintln!(
                "damage {amount} at pos {:?} vel {:?} fall_start {:?} frame {}",
                self.player.pos, self.player.vel, self.fall_start, self.total_frames
            );
        }
        self.health -= amount;
        self.damage_flash = 0.45;
        self.since_damage = 0.0;
        self.sfx(Sfx::Hurt);
        if self.health <= 0.0 {
            self.health = 0.0;
            // Death: scatter the inventory and worn armor as item drops.
            let stacks = self.inventory.drain();
            for s in stacks {
                self.drop_stack(s);
            }
            let worn: Vec<ItemStack> = self.armor.iter_mut().filter_map(|a| a.take()).collect();
            for s in worn {
                self.drop_stack(s);
            }
            self.held_stack = None;
            self.set_screen(Screen::Dead);
        }
    }

    fn drop_stack(&mut self, stack: ItemStack) {
        let a = self.rand01() * std::f32::consts::TAU;
        let v = Vec3::new(a.cos() * 2.0, 3.0 + self.rand01() * 1.5, a.sin() * 2.0);
        let pos = self.player.pos + Vec3::new(0.0, 1.0, 0.0);
        self.items
            .push(ItemEntity::new(pos, v, stack.item, stack.count));
    }

    fn respawn(&mut self) {
        self.player = Player::new(self.spawn_point);
        // A dug-out or collapsed spawn column: come to on the first
        // ground below instead of free-falling to a second death.
        let (sx, sz) = (
            self.spawn_point.x.floor() as i32,
            self.spawn_point.z.floor() as i32,
        );
        self.server.world.ensure_chunk(ChunkPos::of_world(sx, sz));
        let sy = (self.spawn_point.y.floor() as i32).clamp(0, chunk::CHUNK_Y as i32 - 1);
        let ground = (0..=sy)
            .rev()
            .find(|&y| self.reg.is_solid(self.server.world.get_block(sx, y, sz)));
        if let Some(g) = ground
            && self.spawn_point.y - g as f32 > 4.0
        {
            self.player.pos.y = g as f32 + 1.05;
        }
        self.health = self.max_health();
        self.hunger = 20.0;
        self.air = MAX_AIR;
        self.fall_start = None;
        self.drown_timer = 0.0;
        self.since_damage = 100.0;
        self.set_screen(Screen::Playing);
        if self.scripts.wants("on_player_respawn") {
            self.scripts
                .dispatch(&self.server.world, "on_player_respawn", ());
            self.apply_script_cmds();
        }
    }

    fn stream_chunks(&mut self) {
        let pcx = (self.player.pos.x.floor() as i32).div_euclid(CHUNK_X as i32);
        let pcz = (self.player.pos.z.floor() as i32).div_euclid(CHUNK_X as i32);

        // Generate missing chunks, nearest first.
        let mut wanted: Vec<(i32, ChunkPos)> = Vec::new();
        let vd = self.config.view_dist;
        for dx in -vd..=vd {
            for dz in -vd..=vd {
                let pos = ChunkPos {
                    x: pcx + dx,
                    z: pcz + dz,
                };
                if !self.server.world.chunks.contains_key(&pos) {
                    wanted.push((dx * dx + dz * dz, pos));
                }
            }
        }
        wanted.sort_by_key(|(d, _)| *d);
        for (_, pos) in wanted.into_iter().take(GEN_BUDGET) {
            self.server.world.ensure_chunk(pos);
            // New terrain changes neighbors' visible faces at the border.
            for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                if let Some(c) = self.server.world.chunks.get_mut(&ChunkPos {
                    x: pos.x + dx,
                    z: pos.z + dz,
                }) {
                    c.dirty = true;
                }
            }
        }

        // Unload chunks far outside the view radius.
        let limit = vd + 2;
        let far: Vec<ChunkPos> = self
            .server
            .world
            .chunks
            .keys()
            .filter(|p| (p.x - pcx).abs() > limit || (p.z - pcz).abs() > limit)
            .copied()
            .collect();
        if !far.is_empty() {
            self.server.world.settle_falling();
            self.server.world.save_modified();
            for pos in far {
                self.server.world.chunks.remove(&pos);
                self.renderer.drop_chunk(pos);
            }
        }

        // Remesh dirty chunks (only those whose 4 neighbors exist), nearest first.
        let mut dirty: Vec<(i32, ChunkPos)> = self
            .server
            .world
            .chunks
            .iter()
            .filter(|(_, c)| c.dirty)
            .map(|(p, _)| ((p.x - pcx).pow(2) + (p.z - pcz).pow(2), *p))
            .collect();
        dirty.retain(|(_, p)| {
            [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().all(|(dx, dz)| {
                self.server.world.chunks.contains_key(&ChunkPos {
                    x: p.x + dx,
                    z: p.z + dz,
                })
            })
        });
        dirty.sort_by_key(|(d, _)| *d);
        for (_, pos) in dirty.into_iter().take(MESH_BUDGET) {
            let mesh = mesher::mesh_chunk(&self.server.world, pos);
            self.renderer.upload_chunk(pos, &mesh);
            if let Some(c) = self.server.world.chunks.get_mut(&pos) {
                c.dirty = false;
            }
        }
    }

    /// Break-sound family for a block, from its tool class.
    fn break_mat(&self, b: registry::BlockId) -> BreakMat {
        match self.reg.block(b).tool {
            Some(ToolKind::Pickaxe) => BreakMat::Stone,
            Some(ToolKind::Axe) => BreakMat::Wood,
            Some(ToolKind::Shovel) => BreakMat::Soft,
            Some(ToolKind::Hoe) | None => BreakMat::Leafy,
        }
    }

    fn has_ammo(&self, class: &str) -> bool {
        self.inventory
            .slots
            .iter()
            .flatten()
            .any(|s| self.reg.item(s.item).ammo.as_deref() == Some(class))
    }

    /// Remove one item of the ammo class; returns its id.
    fn take_ammo(&mut self, class: &str) -> Option<ItemId> {
        let reg = self.reg.clone();
        for slot in self.inventory.slots.iter_mut() {
            if let Some(s) = slot
                && reg.item(s.item).ammo.as_deref() == Some(class)
            {
                let id = s.item;
                if s.count > 1 {
                    s.count -= 1;
                } else {
                    *slot = None;
                }
                return Some(id);
            }
        }
        None
    }

    /// Loose an arrow: charge in 0..1 scales damage and speed.
    fn fire_bow(&mut self, bow: &registry::BowDef, charge: f32) {
        let reg = self.reg.clone();
        let arrow_id = if self.creative {
            reg.item_id("base:arrow")
        } else {
            self.take_ammo("arrow")
        };
        let Some(arrow_id) = arrow_id else { return };
        let dir = self.camera.forward();
        if let Some(r) = &self.remote {
            r.client.send(&net::C2S::FireArrow {
                pos: self.camera.pos + dir * 0.4,
                vel: dir * bow.speed * (0.6 + 0.4 * charge),
                dmg: bow.damage * (0.45 + 0.55 * charge),
                tile: reg.item(arrow_id).icon,
                recover: !self.creative,
            });
            if !self.creative {
                self.inventory.wear_tool(&reg, self.hotbar_sel);
            }
            self.sfx(Sfx::Bolt(0.8 + charge * 0.8));
            return;
        }
        self.server.world.projectiles.push(mobs::Projectile {
            pos: self.camera.pos + dir * 0.4,
            vel: dir * bow.speed * (0.6 + 0.4 * charge),
            tile: reg.item(arrow_id).icon,
            damage: bow.damage * (0.45 + 0.55 * charge),
            age: 0.0,
            from_player: true,
            // Arrows that stick into terrain are recoverable.
            drop_item: (!self.creative).then_some(arrow_id),
            owner: 0,
        });
        if !self.creative {
            self.inventory.wear_tool(&reg, self.hotbar_sel);
        }
        self.sfx(Sfx::Bolt(0.8 + charge * 0.8));
    }

    /// Nearest mob under the crosshair within reach, unless a solid block
    /// sits in front of it.
    fn mob_in_crosshair(&self, hit: &Option<raycast::Hit>) -> Option<usize> {
        let origin = self.camera.pos;
        let dir = self.camera.forward();
        // A wall in the way shields the mob behind it (approximate the
        // wall distance by its block center).
        let wall_t = hit
            .as_ref()
            .map(|h| {
                let c = Vec3::new(
                    h.block.0 as f32 + 0.5,
                    h.block.1 as f32 + 0.5,
                    h.block.2 as f32 + 0.5,
                );
                (c - origin).length() + 0.5
            })
            .unwrap_or(REACH);
        let mut best: Option<(usize, f32)> = None;
        for (i, m) in self.server.world.mobs.iter().enumerate() {
            let def = &self.reg.animals[m.species];
            if let Some(t) = m.ray_hit(def, origin, dir, REACH.min(wall_t))
                && best.is_none_or(|(_, bt)| t < bt)
            {
                best = Some((i, t));
            }
        }
        best.map(|(i, _)| i)
    }

    /// Remove dead mobs: roll their drop table, spill items, notify mods.
    fn sweep_dead_mobs(&mut self) {
        let reg = self.reg.clone();
        let mut i = 0;
        while i < self.server.world.mobs.len() {
            if self.server.world.mobs[i].health > 0.0 {
                i += 1;
                continue;
            }
            let m = self.server.world.mobs.swap_remove(i);
            let def = &reg.animals[m.species];
            if !def.hostile {
                // The wild counts its dead — wardens are not individuals.
                self.server.world.add_ire(2.0);
            }
            self.sfx(Sfx::MobDeath(def.sound_pitch));
            if m.growth < 1.0 {
                continue; // the young return nothing (you monster)
            }
            for (item, min, max) in &def.drops {
                let n = min + (self.rand01() * (*max - *min + 1) as f32) as u32;
                let n = n.min(*max);
                if n == 0 {
                    continue;
                }
                if m.last_hit_by != 0 {
                    // A guest's kill: their loot crosses the wire.
                    let stack = ItemStack::new(&reg, *item, n);
                    self.server.world.pending_gives.push((m.last_hit_by, stack));
                    continue;
                }
                let a = self.rand01() * std::f32::consts::TAU;
                let v = Vec3::new(a.cos() * 1.2, 2.5, a.sin() * 1.2);
                self.items.push(ItemEntity::new(
                    m.pos + Vec3::new(0.0, def.height * 0.5, 0.0),
                    v,
                    *item,
                    n,
                ));
            }
            if self.scripts.wants("on_animal_killed") {
                self.scripts.dispatch(
                    &self.server.world,
                    "on_animal_killed",
                    (
                        def.name.clone(),
                        m.pos.x as i64,
                        m.pos.y as i64,
                        m.pos.z as i64,
                    ),
                );
                self.apply_script_cmds();
            }
        }
    }

    /// Mining and placing while playing.
    fn interact(&mut self, dt: f32) {
        let reg = self.reg.clone();
        let hit = raycast::raycast(
            &self.server.world,
            self.camera.pos,
            self.camera.forward(),
            REACH,
        );
        let held = self.inventory.slots[self.hotbar_sel].map(|s| s.item);

        // Bow: hold right to draw, release to loose (0.25 s minimum).
        let bow_def = held.and_then(|i| reg.item(i).bow.clone());
        if let Some(bow) = bow_def {
            if self.right_held && (self.creative || self.has_ammo("arrow")) {
                self.bow_draw += dt;
            } else {
                if self.bow_draw >= 0.25 {
                    let charge = ((self.bow_draw - 0.25) / 0.75).clamp(0.0, 1.0);
                    self.fire_bow(&bow, charge);
                }
                self.bow_draw = 0.0;
            }
        } else if self.bow_draw > 0.0 {
            self.bow_draw = 0.0; // switched away mid-draw
        }

        // Archaeology: sweeping a remnant is a slow, careful channel.
        let brush_held = held.is_some_and(|i| reg.item(i).brush_tool);
        let brush_target = hit.as_ref().map(|h| h.block).filter(|t| {
            brush_held
                && reg
                    .block(self.server.world.get_block(t.0, t.1, t.2))
                    .brush
                    .is_some()
        });
        if let (true, Some(target)) = (self.right_held, brush_target) {
            if self.brush_target != Some(target) {
                self.brush_target = Some(target);
                self.brushing = 0.0;
            }
            self.brushing += dt;
            if self.brushing >= 1.5 {
                self.brushing = 0.0;
                self.brush_target = None;
                if let Some(rc) = &self.remote {
                    // The host rolls the find and Gives it straight to
                    // us; the BlockSet echo swaps the remnant out.
                    rc.client.send(&net::C2S::BrushBlock {
                        x: target.0,
                        y: target.1,
                        z: target.2,
                    });
                    if !self.creative {
                        self.inventory.wear_tool(&reg, self.hotbar_sel);
                    }
                    return;
                }
                let mut r = self.rng;
                let found = self
                    .server
                    .world
                    .brush_block(target.0, target.1, target.2, &mut r);
                self.rng = r;
                if let Some(stack) = found {
                    let center = Vec3::new(
                        target.0 as f32 + 0.5,
                        target.1 as f32 + 0.6,
                        target.2 as f32 + 0.5,
                    );
                    let mut ent =
                        ItemEntity::new(center, Vec3::new(0.0, 2.0, 0.0), stack.item, stack.count);
                    // Old tools surface as worn as they were buried.
                    if stack.durability < reg.item(stack.item).durability {
                        ent.durability = stack.durability;
                    }
                    self.items.push(ent);
                    self.sfx(Sfx::Pickup);
                }
                if !self.creative {
                    self.inventory.wear_tool(&reg, self.hotbar_sel);
                }
            }
            return;
        } else {
            self.brushing = 0.0;
            self.brush_target = None;
        }

        // Smithing: hammering a rested bloom is a held channel, three
        // strikes to a bar. The archaeology brush taught us this shape.
        let anvil_target = hit.as_ref().map(|h| h.block).filter(|t| {
            held.is_some_and(|i| reg.item(i).hammer)
                && reg
                    .block(self.server.world.get_block(t.0, t.1, t.2))
                    .interaction
                    .as_deref()
                    == Some("anvil")
                && matches!(
                    self.server.world.block_entities.get(t),
                    Some(world::BlockEntity::Anvil(a)) if a.bloom.is_some()
                )
        });
        if let (true, Some(target)) = (self.right_held, anvil_target) {
            if self.anvil_pos != Some(target) {
                self.anvil_pos = Some(target);
                self.anvil_work = 0.0;
            }
            self.anvil_work += dt;
            if self.anvil_work >= 2.0 {
                self.anvil_work = 0.0;
                self.sfx(Sfx::Break(BreakMat::Stone));
                if !self.creative {
                    self.inventory.wear_tool(&reg, self.hotbar_sel);
                }
                if let Some(rc) = &self.remote {
                    // The host counts strikes and Gives the bar.
                    rc.client.send(&net::C2S::AnvilStrike {
                        x: target.0,
                        y: target.1,
                        z: target.2,
                    });
                } else if let Some(out) = self.server.world.anvil_strike(target) {
                    let center = Vec3::new(
                        target.0 as f32 + 0.5,
                        target.1 as f32 + 1.0,
                        target.2 as f32 + 0.5,
                    );
                    self.items.push(ItemEntity::new(
                        center,
                        Vec3::new(0.0, 2.0, 0.0),
                        out.item,
                        out.count,
                    ));
                    self.sfx(Sfx::Craft);
                }
            }
            return;
        } else {
            self.anvil_work = 0.0;
            self.anvil_pos = None;
        }

        // Attacking: a mob in the crosshair takes the swing before the
        // block behind it. Held tools/swords set the damage.
        if self.left_held
            && let Some(mi) = self.mob_in_crosshair(&hit)
        {
            self.breaking = None;
            if self.attack_cooldown <= 0.0 {
                self.attack_cooldown = 0.35;
                self.swing = 1.0;
                let dmg = held.map(|i| reg.item(i).damage).unwrap_or(1.0);
                let sp = self.server.world.mobs[mi].species;
                let pitch = reg.animals[sp].sound_pitch;
                let from = self.camera.pos;
                if let Some(r) = &self.remote {
                    let id = self.server.world.mobs[mi].id;
                    r.client.send(&net::C2S::AttackMob { id, dmg, from });
                    self.server.world.mobs[mi].hurt_flash = 0.35; // feedback
                    self.sfx(Sfx::MobHurt(pitch));
                    self.hunger = (self.hunger - 0.01).max(0.0);
                    if !self.creative {
                        self.inventory.wear_tool(&reg, self.hotbar_sel);
                    }
                    self.attack_cooldown = 0.35;
                    return;
                }
                let def = reg.animals[sp].clone();
                self.server.world.mobs[mi].hurt(&def, dmg, from);
                self.sfx(Sfx::MobHurt(pitch));
                self.hunger = (self.hunger - 0.01).max(0.0);
                if !self.creative {
                    self.inventory.wear_tool(&reg, self.hotbar_sel);
                }
            }
            return;
        }

        // Hold-to-mine; tools speed up matching blocks and wear down.
        if self.left_held {
            if let Some(h) = &hit {
                let target = h.block;
                let b = self.server.world.get_block(target.0, target.1, target.2);
                let hardness = if self.creative {
                    // Creative breaks anything instantly — except the
                    // unbreakable (the world's floor stays a floor).
                    reg.block(b).hardness.map(|_| 0.0001)
                } else {
                    reg.effective_hardness(b, held)
                };
                if let Some(hardness) = hardness {
                    let progress = match self.breaking {
                        Some((t, p)) if t == target => p + dt / hardness.max(0.0001),
                        _ => dt / hardness.max(0.0001),
                    };
                    if progress >= 1.0 {
                        // Cancellable mod event.
                        let allow = if self.scripts.wants("on_block_break") {
                            let name = reg.block(b).name.clone();
                            let ok = self.scripts.dispatch(
                                &self.server.world,
                                "on_block_break",
                                (target.0 as i64, target.1 as i64, target.2 as i64, name),
                            );
                            self.apply_script_cmds();
                            ok
                        } else {
                            true
                        };
                        self.breaking = None;
                        if allow && self.remote.is_some() {
                            // Guests request; the echo applies the change.
                            if let Some(r) = &self.remote {
                                r.client.send(&net::C2S::Break {
                                    x: target.0,
                                    y: target.1,
                                    z: target.2,
                                });
                            }
                            self.hunger = (self.hunger - 0.008).max(0.0);
                            self.sfx(Sfx::Break(self.break_mat(b)));
                            if !self.creative {
                                self.inventory.wear_tool(&reg, self.hotbar_sel);
                            }
                            return;
                        }
                        if allow {
                            self.hunger = (self.hunger - 0.008).max(0.0);
                            let cost = self.server.world.ire_for_block(b);
                            self.server.world.add_ire(cost);
                            self.server
                                .world
                                .set_block(target.0, target.1, target.2, AIR);
                            self.sfx(Sfx::Break(self.break_mat(b)));
                            if !self.creative {
                                self.inventory.wear_tool(&reg, self.hotbar_sel);
                            }
                            // Shears: leaves come off whole.
                            let sheared = held.is_some_and(|i| reg.item(i).shears)
                                && reg.block(b).name.contains("leaves");
                            if sheared
                                && !self.creative
                                && let Some(item) = reg.item_id(&reg.block(b).name)
                            {
                                let center = Vec3::new(
                                    target.0 as f32 + 0.5,
                                    target.1 as f32 + 0.3,
                                    target.2 as f32 + 0.5,
                                );
                                self.items.push(ItemEntity::new(
                                    center,
                                    Vec3::new(0.0, 2.2, 0.0),
                                    item,
                                    1,
                                ));
                            }
                            if let Some((drop, n)) = reg
                                .drops_for(b, held)
                                .filter(|_| !self.creative && !sheared)
                            {
                                let center = Vec3::new(
                                    target.0 as f32 + 0.5,
                                    target.1 as f32 + 0.3,
                                    target.2 as f32 + 0.5,
                                );
                                let a = self.rand01() * std::f32::consts::TAU;
                                let v = Vec3::new(a.cos() * 1.2, 2.2, a.sin() * 1.2);
                                self.items.push(ItemEntity::new(center, v, drop, n));
                            }
                            // Chance extras (leaves drop saplings).
                            if let Some((item, ch)) = reg.block(b).bonus_drop
                                && !self.creative
                                && self.rand01() < ch
                            {
                                let center = Vec3::new(
                                    target.0 as f32 + 0.5,
                                    target.1 as f32 + 0.3,
                                    target.2 as f32 + 0.5,
                                );
                                let a = self.rand01() * std::f32::consts::TAU;
                                let v = Vec3::new(a.cos() * 1.2, 2.2, a.sin() * 1.2);
                                self.items.push(ItemEntity::new(center, v, item, 1));
                            }
                        }
                    } else {
                        self.breaking = Some((target, progress));
                        // Keep the arm swinging while we chip away.
                        if self.swing <= 0.0 {
                            self.swing = 1.0;
                        }
                    }
                } else {
                    self.breaking = None;
                }
            } else {
                self.breaking = None;
            }
        } else {
            self.breaking = None;
        }

        // Right click: interact with the targeted block (crafting table),
        // otherwise place the selected block.
        // Feeding wildlife: right-click an adult with its favorite food.
        if self.right_held && self.action_cooldown <= 0.0 {
            if let Some(mi) = self.mob_in_crosshair(&hit) {
                let sp = self.server.world.mobs[mi].species;
                let def = &reg.animals[sp];
                if let (Some(bf), Some(h)) = (def.breed_food, held)
                    && bf == h
                    && !def.hostile
                    && self.server.world.mobs[mi].growth >= 1.0
                    && self.server.world.mobs[mi].breed_cd <= 0.0
                    && !self.server.world.mobs[mi].fed
                    && (self.creative || self.inventory.take_one(self.hotbar_sel).is_some())
                {
                    // Guests request; setting fed locally is the
                    // prediction until the snapshot echoes it.
                    if let Some(rc) = &self.remote {
                        let id = self.server.world.mobs[mi].id;
                        rc.client.send(&net::C2S::FeedMob { id });
                    }
                    let m = &mut self.server.world.mobs[mi];
                    m.fed = true;
                    m.calm = 30.0;
                    self.action_cooldown = 0.4;
                    self.sfx(Sfx::Pickup);
                    return;
                }
            }
            // A covered log pile takes a warden's ember: the clamp.
            let ember = reg.item_id("base:ember");
            if held == ember
                && let Some(hb) = &hit
            {
                let (bx, by, bz) = hb.block;
                let tb = self.server.world.get_block(bx, by, bz);
                let is_log = reg.tags.get("base:logs").is_some_and(|l| {
                    reg.item_id(&reg.block(tb).name)
                        .is_some_and(|i| l.contains(&i))
                });
                if is_log {
                    if let Some(rc) = &self.remote {
                        self.inventory.take_one(self.hotbar_sel);
                        rc.client.send(&net::C2S::LightClamp {
                            x: bx,
                            y: by,
                            z: bz,
                        });
                    } else {
                        match self.server.world.try_light_clamp(bx, by, bz) {
                            Ok(n) => {
                                self.inventory.take_one(self.hotbar_sel);
                                self.sfx(Sfx::Bolt(0.8));
                                self.toast(format!(
                                    "The clamp smolders - {n} logs, {:.0} minutes.",
                                    n as f32 * world::CLAMP_SECS_PER_LOG / 60.0
                                ));
                            }
                            Err(e) => self.toast(e.to_string()),
                        }
                    }
                    self.action_cooldown = 0.5;
                    return;
                }
            }
            // Throwables (snowballs): loosed from the hand.
            if let Some(speed) = held.and_then(|i| reg.item(i).throw_speed)
                && (self.creative || self.inventory.take_one(self.hotbar_sel).is_some())
            {
                let item = held.unwrap();
                let dir = self.camera.forward();
                let pos = self.camera.pos + dir * 0.4;
                let vel = dir * speed;
                let tile = reg.item(item).icon;
                if let Some(rc) = &self.remote {
                    rc.client.send(&net::C2S::FireArrow {
                        pos,
                        vel,
                        dmg: 0.0,
                        tile,
                        recover: false,
                    });
                } else {
                    self.server.world.projectiles.push(mobs::Projectile {
                        pos,
                        vel,
                        tile,
                        damage: 0.0,
                        age: 0.0,
                        from_player: true,
                        drop_item: None,
                        owner: 0,
                    });
                }
                self.sfx(Sfx::Bolt(1.6));
                self.action_cooldown = 0.35;
                return;
            }
            // Etched tablets: the lost takers speak.
            if held.is_some_and(|i| reg.item(i).tablet) {
                self.read_tablet();
                self.action_cooldown = 0.6;
                return;
            }
            // Bedroll: camp until dawn.
            if held.is_some_and(|i| reg.item(i).bedroll) {
                self.try_sleep();
                self.action_cooldown = 0.5;
                return;
            }
        }
        let held_is_food = held.is_some_and(|i| reg.item(i).food.is_some());
        if self.right_held
            && self.action_cooldown <= 0.0
            && !held_is_food
            && let Some(h) = &hit
        {
            let tb = self.server.world.get_block(h.block.0, h.block.1, h.block.2);
            // Harvestable blocks (berry bushes).
            if let Some((item, n, becomes)) = reg.block(tb).harvest {
                self.server
                    .world
                    .set_block(h.block.0, h.block.1, h.block.2, becomes);
                let left = self.inventory.add(&reg, item, n);
                if left > 0 {
                    self.drop_stack(ItemStack::new(&reg, item, left));
                }
                self.sfx(Sfx::Pickup);
                self.action_cooldown = 0.3;
                return;
            }
            // Hoe tills grass/dirt into farmland.
            if let (Some((ToolKind::Hoe, _, _)), Some(farm)) = (
                held.and_then(|i| reg.item(i).tool),
                reg.block_id("base:farmland"),
            ) {
                let name = reg.block(tb).name.as_str();
                if name == "base:grass" || name == "base:dirt" {
                    self.server
                        .world
                        .set_block(h.block.0, h.block.1, h.block.2, farm);
                    self.inventory.wear_tool(&reg, self.hotbar_sel);
                    self.sfx(Sfx::Place);
                    self.action_cooldown = 0.3;
                    return;
                }
            }
            if self.scripts.wants("on_interact") {
                let name = reg.block(tb).name.clone();
                let allow = self.scripts.dispatch(
                    &self.server.world,
                    "on_interact",
                    (h.block.0 as i64, h.block.1 as i64, h.block.2 as i64, name),
                );
                self.apply_script_cmds();
                if !allow {
                    self.right_held = false;
                    self.action_cooldown = 0.22;
                    return;
                }
            }
            match reg.block(tb).interaction.as_deref() {
                Some("crafting") => {
                    self.right_held = false;
                    self.craft_size = 3;
                    self.set_screen(Screen::Inventory);
                    return;
                }
                Some("furnace") => {
                    self.right_held = false;
                    self.server
                        .world
                        .block_entities
                        .entry(h.block)
                        .or_insert_with(|| world::BlockEntity::Furnace(Default::default()));
                    self.set_screen(Screen::Furnace(h.block));
                    return;
                }
                Some("chest") if self.action_cooldown <= 0.0 => {
                    self.action_cooldown = 0.3;
                    if let Some(rc) = &self.remote {
                        rc.client.send(&net::C2S::OpenContainer {
                            x: h.block.0,
                            y: h.block.1,
                            z: h.block.2,
                        });
                        return;
                    }
                    let e = self
                        .server
                        .world
                        .block_entities
                        .entry(h.block)
                        .or_insert_with(|| world::BlockEntity::Chest(Default::default()));
                    if let world::BlockEntity::Chest(c) = e
                        && c.wild_owned
                    {
                        c.wild_owned = false;
                        self.server.world.add_ire(1.0);
                        self.toast("The wild keeps its trophies.".to_string());
                    }
                    self.set_screen(Screen::Chest(h.block));
                    return;
                }
                Some("offering") if self.action_cooldown <= 0.0 => {
                    self.action_cooldown = 0.3;
                    if let Some(rc) = &self.remote {
                        rc.client.send(&net::C2S::OpenContainer {
                            x: h.block.0,
                            y: h.block.1,
                            z: h.block.2,
                        });
                        return;
                    }
                    self.server
                        .world
                        .block_entities
                        .entry(h.block)
                        .or_insert_with(|| world::BlockEntity::Offering(Default::default()));
                    self.set_screen(Screen::Offering(h.block));
                    return;
                }
                _ => {}
            }
            let (x, y, z) = h.adjacent;
            let place = self.inventory.slots[self.hotbar_sel].and_then(|s| reg.item(s.item).places);
            if let Some(block) = place {
                let bd = reg.block(block);
                let needs_farmland = bd.crop_next.is_some() && !bd.crop_any_soil;
                let soil = self.server.world.get_block(x, y - 1, z);
                if needs_farmland && Some(soil) != reg.block_id("base:farmland") {
                    return;
                }
                // Cross blocks (torches, plants) need solid ground.
                if bd.cross && !reg.is_solid(soil) {
                    return;
                }
                if !reg.is_solid(self.server.world.get_block(x, y, z))
                    && !self.player.overlaps_block(x, y, z)
                {
                    let allow = if self.scripts.wants("on_block_place") {
                        let name = reg.block(block).name.clone();
                        let ok = self.scripts.dispatch(
                            &self.server.world,
                            "on_block_place",
                            (x as i64, y as i64, z as i64, name),
                        );
                        self.apply_script_cmds();
                        ok
                    } else {
                        true
                    };
                    let consumed =
                        self.creative || self.inventory.take_one(self.hotbar_sel).is_some();
                    if allow && consumed && self.remote.is_some() {
                        if let Some(r) = &self.remote
                            && let Some(host_id) = r.host_block.get(&block.0)
                        {
                            r.client.send(&net::C2S::Place {
                                x,
                                y,
                                z,
                                block: *host_id,
                            });
                        }
                        self.action_cooldown = 0.22;
                        self.sfx(Sfx::Place);
                        return;
                    }
                    if allow && consumed {
                        self.server.world.set_block(x, y, z, block);
                        if bd.crop_next.is_some() {
                            // The wild notices things growing where you walk.
                            self.server.world.plant_ire(0.2);
                        }
                        self.action_cooldown = 0.22;
                        self.sfx(Sfx::Place);
                    }
                }
            }
        }
    }

    /// Apply world mutations queued by scripts during the last dispatch.
    fn apply_script_cmds(&mut self) {
        let reg = self.reg.clone();
        for cmd in self.scripts.take_cmds() {
            match cmd {
                script::Cmd::SetBlock(x, y, z, name) => {
                    if let Some(b) = reg.block_id(&name) {
                        self.server.world.set_block(x, y, z, b);
                    }
                }
                script::Cmd::Give(name, n) => {
                    if let Some(item) = reg.item_id(&name) {
                        let left = self.inventory.add(&reg, item, n);
                        if left > 0 {
                            self.drop_stack(ItemStack::new(&reg, item, left));
                        }
                    }
                }
                script::Cmd::Hud(msg) => self.toast(msg),
                script::Cmd::SpawnAnimal(name, x, y, z) => {
                    if let Some(si) = reg.animal_id(&name)
                        && self.server.world.mobs.len() < world::MOB_CAP
                    {
                        let mut m = mobs::Mob::new(si, Vec3::new(x, y, z), 0.0);
                        m.health = reg.animals[si].health;
                        self.server.world.mobs.push(m);
                    }
                }
                script::Cmd::Sound(name) => {
                    let sfx = match name.as_str() {
                        "click" => Some(Sfx::Click),
                        "place" => Some(Sfx::Place),
                        "pickup" => Some(Sfx::Pickup),
                        "hurt" => Some(Sfx::Hurt),
                        "craft" => Some(Sfx::Craft),
                        "splash" => Some(Sfx::Splash),
                        _ => None,
                    };
                    if let Some(s) = sfx {
                        self.sfx(s);
                    }
                }
            }
        }
    }

    /// Hot reload: rebuild the registry + atlas from disk, remap the live
    /// world and inventories by string id, recompile scripts.
    /// The pack id in effect: the dev env override, else the config choice.
    fn active_pack_id(&self) -> String {
        self.pack_override
            .clone()
            .unwrap_or_else(|| self.config.pack.clone())
    }

    /// Rebuild + swap the atlas for the currently selected texture pack and
    /// persist the choice. Registry/scripts are untouched — packs are art only.
    fn apply_pack(&mut self) {
        let (mut data, px, warns) = atlas::build_atlas(
            &self.reg.tex_files,
            pack_source_of(&self.active_pack_id()),
            &self.reg.tex_names,
        );
        let season = if self.in_world {
            self.server.world.season()
        } else {
            1
        };
        atlas::season_tint(&mut data, px, season);
        self.atlas_season = season;
        self.renderer.set_atlas(&data, px);
        self.pack_warnings = warns;
        self.config.save();
    }

    fn reload_mods(&mut self, forced: bool) {
        let old = self.reg.clone();
        let new_reg = Arc::new(registry::load(std::path::Path::new("mods")));
        let (mut atlas_data, atlas_px, warns) = atlas::build_atlas(
            &new_reg.tex_files,
            pack_source_of(&self.active_pack_id()),
            &new_reg.tex_names,
        );
        let season = if self.in_world {
            self.server.world.season()
        } else {
            1
        };
        atlas::season_tint(&mut atlas_data, atlas_px, season);
        self.atlas_season = season;
        self.pack_warnings = warns;
        self.renderer.set_atlas(&atlas_data, atlas_px);

        // Remap items by name (old registry -> new); unknown items vanish.
        let remap_item =
            |reg: &Registry, it: ItemId| -> Option<ItemId> { reg.item_id(&old.item(it).name) };
        let fix_stack = |reg: &Registry, s: Option<ItemStack>| -> Option<ItemStack> {
            s.and_then(|s| remap_item(reg, s.item).map(|item| ItemStack { item, ..s }))
        };
        for slot in self.inventory.slots.iter_mut() {
            *slot = fix_stack(&new_reg, *slot);
        }
        for slot in self.craft_grid.iter_mut() {
            *slot = fix_stack(&new_reg, *slot);
        }
        self.held_stack = fix_stack(&new_reg, self.held_stack);
        self.items
            .retain_mut(|e| match remap_item(&new_reg, e.item) {
                Some(item) => {
                    e.item = item;
                    true
                }
                None => false,
            });
        self.breaking = None;

        self.reg = new_reg.clone();
        self.server.world.reg = new_reg.clone();
        self.server.world.remap_from(&old);
        self.server.world.generator = worldgen::Generator::new(self.server.world.seed, &new_reg);
        self.scripts.load_mods(&script_mod_dirs(&new_reg));

        let errors: Vec<String> = new_reg
            .mods
            .iter()
            .filter_map(|m| m.error.clone())
            .chain(self.scripts.mods.iter().filter_map(|m| m.error.clone()))
            .collect();
        if errors.is_empty() {
            eprintln!(
                "mods: reloaded ({} blocks, {} items, {} recipes)",
                new_reg.blocks.len(),
                new_reg.items.len(),
                new_reg.recipes.len()
            );
            self.toast(format!(
                "mods reloaded ({} blocks, {} items, {} recipes)",
                new_reg.blocks.len(),
                new_reg.items.len(),
                new_reg.recipes.len()
            ));
        } else {
            for e in errors.iter().take(3) {
                self.toast(format!("mod error: {e}"));
            }
        }
        if forced {
            self.sfx(Sfx::Click);
        }
    }

    fn toast(&mut self, msg: String) {
        self.toasts.push((msg, 4.0));
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }
    }

    fn max_health(&self) -> f32 {
        MAX_HEALTH + self.nutrition.iter().filter(|&&n| n >= 40.0).count() as f32 * 2.0
    }

    fn update_food(&mut self, dt: f32, input: &physics::Input) {
        if self.creative {
            return;
        }
        // Activity-based hunger drain (the hunger charm slows it).
        let charm_mult = if self.charm("hunger") { 0.85 } else { 1.0 };
        let mut drain = 0.01 * charm_mult;
        if input.sprint && (input.forward != 0.0 || input.strafe != 0.0) {
            drain += 0.02;
        }
        self.hunger = (self.hunger - drain * dt).max(0.0);
        // Nutrition decays slowly (~full to empty over long play).
        for n in self.nutrition.iter_mut() {
            *n = (*n - dt * 0.01).max(0.0);
        }
        let maxh = self.max_health();
        self.health = self.health.min(maxh);
        // Food-gated regen (replaces free idle regen).
        if self.hunger >= 17.0 && self.health < maxh && self.since_damage > 4.0 {
            self.exhaustion_regen += dt;
            if self.exhaustion_regen >= 3.0 {
                self.exhaustion_regen = 0.0;
                self.health = (self.health + 1.0).min(maxh);
                self.hunger = (self.hunger - 0.5).max(0.0);
            }
        }
        // Starvation weakens to 1 heart, never kills.
        if self.hunger <= 0.0 {
            self.starve_timer += dt;
            if self.starve_timer >= 4.0 {
                self.starve_timer = 0.0;
                if self.health > 2.0 {
                    self.health -= 1.0;
                    self.damage_flash = 0.3;
                    self.sfx(Sfx::Hurt);
                }
            }
        }
        // Eating: hold right-click with food selected.
        let food =
            self.inventory.slots[self.hotbar_sel].and_then(|s| self.reg.item(s.item).food.clone());
        if self.right_held
            && self.screen == Screen::Playing
            && let Some(f) = food
        {
            let want = self.hunger < 19.5
                || f.nutrition
                    .iter()
                    .zip(&self.nutrition)
                    .any(|(a, b)| *a > 0.0 && *b < 99.0);
            if want {
                self.eating += dt;
                if self.eating >= f.eat_time {
                    self.eating = 0.0;
                    self.hunger = (self.hunger + f.hunger).min(20.0);
                    for (n, add) in self.nutrition.iter_mut().zip(&f.nutrition) {
                        *n = (*n + add).min(100.0);
                    }
                    self.inventory.take_one(self.hotbar_sel);
                    self.sfx(Sfx::Pickup);
                }
                return;
            }
        }
        self.eating = 0.0;
    }

    fn update_survival(&mut self, dt: f32) {
        // Fall damage: measure from the apex of the fall.
        if self.player.in_water || self.player.on_ground {
            if let (Some(start), true) = (self.fall_start, self.player.on_ground) {
                let fall = start - self.player.pos.y;
                self.damage((fall - 3.0).floor());
            }
            self.fall_start = None;
        } else if self.player.vel.y < 0.0 {
            self.fall_start = Some(
                self.fall_start
                    .unwrap_or(self.player.pos.y)
                    .max(self.player.pos.y),
            );
        } else {
            self.fall_start = None;
        }

        // The void below the world's floor: nothing survives long out
        // there (a backstop — worldroot should make this unreachable).
        if self.player.pos.y < -8.0 && self.since_damage >= 0.4 {
            self.killed_by_wild = false;
            self.damage(4.0);
        }

        // Drowning.
        if self.player.head_underwater(&self.server.world) {
            self.air -= dt;
            if self.air <= 0.0 {
                self.air = 0.0;
                self.drown_timer += dt;
                if self.drown_timer >= 1.0 {
                    self.drown_timer = 0.0;
                    self.damage(2.0);
                }
            }
        } else {
            self.air = (self.air + dt * 4.0).min(MAX_AIR);
            self.drown_timer = 0.0;
        }

        self.since_damage += dt;
        self.damage_flash = (self.damage_flash - dt).max(0.0);
    }

    fn update_items(&mut self, dt: f32) {
        let world = &self.server.world;
        self.items.retain_mut(|it| it.update(world, dt));
        if self.screen == Screen::Dead {
            return;
        }
        // Pickup: magnetize into the inventory.
        let target = self.player.pos + Vec3::new(0.0, 0.9, 0.0);
        let mut i = 0;
        while i < self.items.len() {
            let it = &self.items[i];
            let d = it.pos.distance(target);
            if it.age > entity::PICKUP_DELAY && d < 1.4 {
                let (item, count, dur) = (
                    self.items[i].item,
                    self.items[i].count,
                    self.items[i].durability,
                );
                let reg = self.reg.clone();
                let left = if dur > 0 {
                    let mut stack = ItemStack::new(&reg, item, count);
                    stack.durability = dur;
                    self.inventory.add_stack(&reg, stack)
                } else {
                    self.inventory.add(&reg, item, count)
                };
                if left < count {
                    self.sfx(Sfx::Pickup);
                }
                if left == 0 {
                    self.items.swap_remove(i);
                    continue;
                } else {
                    self.items[i].count = left;
                }
            }
            i += 1;
        }
    }

    /// First-person viewmodel: your arm, or the block/item it holds,
    /// anchored low-right of the camera, walk-bobbed, and swung on use.
    /// Emitted in world space; the renderer draws it depth-cleared so
    /// it never sinks into a wall you're standing against.
    fn emit_hand(&self, verts: &mut Vec<mesher::Vertex>, idx: &mut Vec<u32>) {
        if !self.in_world || self.health <= 0.0 {
            return;
        }
        let reg = self.reg.clone();
        let held = self.inventory.slots[self.hotbar_sel];

        // Camera basis: f forward, r screen-right, u screen-up.
        let f = self.camera.forward();
        let mut r = f.cross(Vec3::Y);
        if r.length_squared() < 1e-6 {
            r = Vec3::new(-self.camera.yaw.sin(), 0.0, self.camera.yaw.cos());
        }
        let r = r.normalize();
        let u = r.cross(f).normalize();

        // Swing arc peaks mid-animation (progress runs 1 -> 0).
        // Dev: WILDFORGE_POSE freezes the swing for screenshots.
        let swing = std::env::var("WILDFORGE_POSE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(self.swing);
        let arc = ((1.0 - swing) * std::f32::consts::PI).sin();
        let bow_charge = if held.is_some_and(|s| reg.item(s.item).bow.is_some()) {
            self.bow_draw.min(1.0)
        } else {
            0.0
        };

        // Anchor low-right, breathing with the walk.
        let moving = (Vec3::new(self.player.vel.x, 0.0, self.player.vel.z)
            .length()
            .min(4.0))
            / 4.0;
        let bob_x = self.hand_bob.sin() * 0.02 * moving;
        let bob_y = -(self.hand_bob * 2.0).sin().abs() * 0.025 * moving;
        let mut anchor = self.camera.pos + f * 0.60 + r * (0.38 + bob_x) + u * (-0.38 + bob_y);
        // Swing sweeps toward where you're aiming; a drawn bow comes
        // toward center.
        anchor += (f * 0.08 - u * 0.05 - r * 0.08) * arc;
        anchor += (-r * 0.16 + f * 0.04) * bow_charge;
        if self.eating > 0.0 {
            // Nibbling: toward the face, jittering.
            anchor += -r * 0.14 + u * (0.04 + (self.time_abs * 16.0).sin() * 0.02);
        }
        if self.brushing > 0.0 {
            // Scrubbing side to side.
            anchor += r * (self.time_abs * 22.0).sin() * 0.03;
        }

        // Local space: x right, y up, z forward. The swing dips the tip
        // forward-down and sweeps it inward, hinged at the wrist.
        let a = arc * 0.85;
        let b = arc * 0.8;
        let xf = |q: Vec3| -> Vec3 {
            let (sa, ca) = a.sin_cos();
            let q = Vec3::new(q.x, q.y * ca - q.z * sa, q.y * sa + q.z * ca);
            let (sb, cb) = b.sin_cos();
            let q = Vec3::new(q.x * cb - q.z * sb, q.y, q.x * sb + q.z * cb);
            anchor + r * q.x + u * q.y + f * q.z
        };

        // Lit like anything standing where the camera stands.
        let (bl, sl) = self.server.world.light_at(
            self.camera.pos.x.floor() as i32,
            self.camera.pos.y.floor() as i32,
            self.camera.pos.z.floor() as i32,
        );
        let lum = (bl as f32 / 15.0, sl as f32 / 15.0);

        // A local-space box textured one tile per face (arm, held block).
        fn cube(
            verts: &mut Vec<mesher::Vertex>,
            idx: &mut Vec<u32>,
            xf: &dyn Fn(Vec3) -> Vec3,
            min: Vec3,
            max: Vec3,
            tiles: [u16; 6],
            lum: (f32, f32),
        ) {
            let ts = 1.0 / atlas::ATLAS_TILES as f32;
            let inset = ts / 32.0;
            for (face, (&slot, corners)) in tiles.iter().zip(mesher::CORNERS.iter()).enumerate() {
                let (tx, ty) = (
                    slot as u32 % atlas::ATLAS_TILES,
                    slot as u32 / atlas::ATLAS_TILES,
                );
                let base = verts.len() as u32;
                for c in corners.iter() {
                    let lp = Vec3::new(
                        min.x + c[0] * (max.x - min.x),
                        min.y + c[1] * (max.y - min.y),
                        min.z + c[2] * (max.z - min.z),
                    );
                    let wp = xf(lp);
                    let (uu, vv) = match face {
                        0 | 1 => (c[2], 1.0 - c[1]),
                        4 | 5 => (c[0], 1.0 - c[1]),
                        _ => (c[0], c[2]),
                    };
                    let shade = mesher::FACE_SHADE[face].max(0.7);
                    verts.push(mesher::Vertex {
                        pos: [wp.x, wp.y, wp.z],
                        uv: [
                            tx as f32 * ts + inset + uu * (ts - 2.0 * inset),
                            ty as f32 * ts + inset + vv * (ts - 2.0 * inset),
                        ],
                        normal: [0.0, 0.0, 0.0],
                        light: [shade * lum.0, shade * lum.0, shade * lum.0],
                        sky: shade * lum.1,
                    });
                }
                idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
        // Pre-rotations in local space (3/4 view for blocks, arm angle).
        let pre_y = |ang: f32| {
            move |q: Vec3| {
                let (s, c) = ang.sin_cos();
                Vec3::new(q.x * c - q.z * s, q.y, q.x * s + q.z * c)
            }
        };
        let pre_x = |ang: f32| {
            move |q: Vec3| {
                let (s, c) = ang.sin_cos();
                Vec3::new(q.x, q.y * c - q.z * s, q.y * s + q.z * c)
            }
        };

        match held {
            // A block rides as a mini-cube, turned for a 3/4 view.
            Some(st)
                if reg
                    .item(st.item)
                    .places
                    .is_some_and(|pb| !reg.block(pb).cross) =>
            {
                let pb = reg.item(st.item).places.unwrap();
                let tilt = pre_y(0.65);
                let x2 = |q: Vec3| xf(tilt(q));
                cube(
                    verts,
                    idx,
                    &x2,
                    Vec3::new(-0.10, -0.12, -0.01),
                    Vec3::new(0.10, 0.08, 0.19),
                    reg.block(pb).tiles,
                    lum,
                );
            }
            // Anything else shows as its icon: a flat angled card,
            // drawn double-sided like dropped item sprites.
            Some(st) => {
                let slot = reg.item(st.item).icon;
                let ts = 1.0 / atlas::ATLAS_TILES as f32;
                let inset = ts / 32.0;
                let (tx, ty) = (
                    slot as u32 % atlas::ATLAS_TILES,
                    slot as u32 / atlas::ATLAS_TILES,
                );
                let ax = Vec3::new(0.44, 0.07, -0.21);
                let ay = Vec3::new(-0.10, 0.44, 0.17);
                let origin = Vec3::new(0.02, -0.16, 0.06);
                for flip in [false, true] {
                    let base = verts.len() as u32;
                    let (u0, u1) = if flip {
                        ((tx + 1) as f32 * ts - inset, tx as f32 * ts + inset)
                    } else {
                        (tx as f32 * ts + inset, (tx + 1) as f32 * ts - inset)
                    };
                    let s = if flip { -1.0 } else { 1.0 };
                    for (i, j, uu) in [
                        (-0.5, 0.0, u0),
                        (0.5, 0.0, u1),
                        (0.5, 1.0, u1),
                        (-0.5, 1.0, u0),
                    ] {
                        let lp = origin + ax * (i * s) + ay * j;
                        let wp = xf(lp);
                        let vv = if j < 0.5 {
                            (ty + 1) as f32 * ts - inset
                        } else {
                            ty as f32 * ts + inset
                        };
                        verts.push(mesher::Vertex {
                            pos: [wp.x, wp.y, wp.z],
                            uv: [uu, vv],
                            normal: [0.0, 0.0, 0.0],
                            light: [0.95 * lum.0, 0.95 * lum.0, 0.95 * lum.0],
                            sky: 0.95 * lum.1,
                        });
                    }
                    idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                }
            }
            // Bare hand: your forearm (blue sleeve and all — the same
            // body other players see) reaching in from the low right.
            None => {
                let skin = *atlas::builtin_slots().get("player_skin").unwrap_or(&0);
                let ty = pre_y(-0.30);
                let tx = pre_x(-0.55);
                let x2 = |q: Vec3| xf(ty(tx(q)));
                cube(
                    verts,
                    idx,
                    &x2,
                    Vec3::new(-0.055, -0.09, -0.16),
                    Vec3::new(0.055, 0.02, 0.26),
                    [skin; 6],
                    lum,
                );
            }
        }
    }

    fn update(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;
        self.time_abs += dt;

        let paused = self.screen == Screen::Paused || !self.in_world;
        if !paused {
            self.action_cooldown = (self.action_cooldown - dt).max(0.0);
            self.attack_cooldown = (self.attack_cooldown - dt).max(0.0);
            self.scroll_cooldown = (self.scroll_cooldown - dt).max(0.0);
            self.swing = (self.swing - dt / 0.3).max(0.0);
            let hv = Vec3::new(self.player.vel.x, 0.0, self.player.vel.z).length();
            if self.player.on_ground {
                self.hand_bob += hv.min(6.0) * dt * 1.6;
            }
        }

        if !self.in_world && self.remote.is_some() {
            self.remote_pump(dt);
        }
        if self.in_world {
            self.stream_chunks();
            // The authoritative simulation steps at its fixed tick; the
            // client applies the results as presentation.
            if self.remote.is_some() {
                self.remote_pump(dt);
            } else if !paused || self.host.is_some() {
                let ctx = server::PlayerCtx {
                    pos: self.player.pos,
                    spawn: self.spawn_point,
                    attackable: !self.creative && self.health > 0.0,
                    aggro_mod: if self.charm("quiet") { -2.0 } else { 0.0 },
                };
                // Hosting: guests are simulated players too, and their
                // requests apply before the tick.
                let players = if let Some(mut sess) = self.host.take() {
                    self.server.world.log_edits = true;
                    let fx = sess.pump(
                        &mut self.server,
                        Some((self.player.pos, self.camera.yaw, self.host_sleeping)),
                        dt,
                    );
                    for f in fx {
                        match f {
                            mp::HostFx::Chat { from, msg } => {
                                self.toast(format!("{from}: {msg}"));
                            }
                            mp::HostFx::Joined(n) => self.toast(format!("{n} joined.")),
                            mp::HostFx::Left(n) => self.toast(format!("{n} left.")),
                            mp::HostFx::AllSlept => {
                                self.host_sleeping = false;
                                self.spawn_point = self.player.pos;
                                self.toast("Dawn. The camp wakes.".to_string());
                            }
                        }
                    }
                    let players = sess.player_ctxs(Some(ctx));
                    self.host = Some(sess);
                    players
                } else {
                    vec![ctx]
                };
                let mut evs = Vec::new();
                self.server.advance(dt, &players, &mut evs);
                for ev in evs {
                    match ev {
                        server::SimEvent::PlayerHit { who, dmg, from } => {
                            if who == 0 && self.remote.is_none() {
                                self.hurt_player_from_wild(dmg, from);
                            } else if let Some(sess) = &self.host {
                                // Guests are listed after the host.
                                let ids: Vec<u32> = sess.guests.keys().copied().collect();
                                if let Some(gid) = ids.get(who.saturating_sub(1)) {
                                    sess.net.send(*gid, &net::S2C::Hit { dmg, from });
                                }
                            }
                        }
                        server::SimEvent::BoltCast => self.sfx(Sfx::Bolt(1.2)),
                        server::SimEvent::Bred => {
                            self.sfx(Sfx::Pickup);
                            self.toast("New life stirs in the wild.".to_string());
                        }
                        server::SimEvent::Dawn { offering_refund } => {
                            if offering_refund > 0.0 {
                                self.sfx(Sfx::Pickup);
                                self.toast("The wild has accepted your offering.".to_string());
                            }
                        }
                        server::SimEvent::WeatherChanged(w) => {
                            // Presentation lerps from world.weather every
                            // frame; only a breaking storm needs a latch:
                            // no flash or rumble after the sky clears.
                            if w != world::Weather::Storm {
                                self.lightning = 0.0;
                                self.thunder_delay = -1.0;
                            }
                        }
                        server::SimEvent::IreTier { rose, tier } => {
                            let name = world::IRE_TIERS[tier.min(world::IRE_TIERS.len() - 1)];
                            self.toast(if rose {
                                format!("The wild stirs against you - {name}.")
                            } else {
                                format!("The wild settles - {name}.")
                            });
                        }
                    }
                }
                if self.remote.is_none() {
                    self.sweep_dead_mobs();
                }
                for (pos, s) in std::mem::take(&mut self.server.world.pending_drops) {
                    let center =
                        Vec3::new(pos.0 as f32 + 0.5, pos.1 as f32 + 0.5, pos.2 as f32 + 0.5);
                    let a = self.rand01() * std::f32::consts::TAU;
                    let v = Vec3::new(a.cos() * 1.5, 2.5, a.sin() * 1.5);
                    self.items.push(ItemEntity::new(center, v, s.item, s.count));
                }
                // Close container screens if their block vanished.
                if let Screen::Furnace(pos)
                | Screen::Chest(pos)
                | Screen::Offering(pos)
                | Screen::Bloomery(pos) = self.screen
                    && !self.server.world.block_entities.contains_key(&pos)
                {
                    self.set_screen(Screen::Playing);
                }
                // Mod tick at 10 Hz.
                if self.scripts.wants("on_tick") {
                    self.tick_accum += dt;
                    if self.tick_accum >= 0.1 {
                        let t = self.tick_accum;
                        self.tick_accum = 0.0;
                        self.scripts
                            .dispatch(&self.server.world, "on_tick", (t as f64,));
                        self.apply_script_cmds();
                    }
                }
            }
        }
        // The turning of the season repaints the leaves.
        if self.in_world && self.server.world.season() != self.atlas_season {
            let (mut data, px, warns) = atlas::build_atlas(
                &self.reg.tex_files,
                pack_source_of(&self.active_pack_id()),
                &self.reg.tex_names,
            );
            atlas::season_tint(&mut data, px, self.server.world.season());
            self.atlas_season = self.server.world.season();
            self.pack_warnings = warns;
            self.renderer.set_atlas(&data, px);
        }

        // Hot reload: poll the mods + packs trees once a second.
        self.mods_poll += dt;
        if self.mods_poll >= 1.0 {
            self.mods_poll = 0.0;
            let stamp = content_tree_stamp();
            if stamp != self.mods_stamp {
                self.mods_stamp = stamp;
                self.reload_mods(false);
            }
        }
        for t in self.toasts.iter_mut() {
            t.1 -= dt;
        }
        self.toasts.retain(|t| t.1 > 0.0);

        // Physics — only once the chunk under the player exists.
        let pchunk = ChunkPos::of_world(
            self.player.pos.x.floor() as i32,
            self.player.pos.z.floor() as i32,
        );
        let can_sim = self.server.world.chunks.contains_key(&pchunk) && !paused;
        if can_sim && self.screen != Screen::Dead {
            let input = physics::Input {
                forward: (self.keys.w as i32 - self.keys.s as i32) as f32,
                strafe: (self.keys.d as i32 - self.keys.a as i32) as f32,
                jump: self.keys.space,
                sprint: self.keys.sprint && self.hunger >= 6.0,
            };
            if self.keys.space && self.player.on_ground {
                self.hunger = (self.hunger - 0.005).max(0.0);
            }
            // Getting up: any movement withdraws a pending sleep vote.
            if input.forward != 0.0 || input.strafe != 0.0 || input.jump {
                if self.host_sleeping {
                    self.host_sleeping = false;
                    self.toast("You get up.".to_string());
                }
                if self.remote.as_ref().is_some_and(|r| r.sleeping) {
                    let r = self.remote.as_mut().unwrap();
                    r.sleeping = false;
                    r.client.send(&net::C2S::SleepCancel);
                    self.toast("You get up.".to_string());
                }
            }
            self.update_food(dt, &input);
            if self.flying {
                let mut wish =
                    self.camera.flat_forward() * input.forward + self.camera.right() * input.strafe;
                if wish.length_squared() > 1.0 {
                    wish = wish.normalize();
                }
                let mut v = wish * 9.0;
                if self.keys.space {
                    v.y += 8.0;
                }
                if self.keys.sprint {
                    v.y -= 8.0;
                }
                self.player.fly(&self.server.world, v, dt);
            }
            let was_in_water = self.player.in_water;
            let fall_speed = self.player.vel.y;
            if !self.flying {
                self.player.update(
                    &self.server.world,
                    &input,
                    self.camera.flat_forward(),
                    self.camera.right(),
                    dt,
                );
            }
            if !was_in_water && self.player.in_water && fall_speed < -4.0 {
                self.sfx(Sfx::Splash);
            }
            self.update_survival(dt);
        }
        if can_sim {
            self.update_items(dt);
        }
        self.camera.pos = self.player.eye();

        if self.screen == Screen::Playing && self.mouse_captured {
            self.interact(dt);
        }

        // Day/night: daylight factor from a sun curve (full day on menus).
        let sun = (self.server.time_of_day * std::f32::consts::TAU).sin();
        // 0.12 floor: moonlit surfaces stay navigable; torch light is
        // unaffected (its own vertex channel).
        let daylight = if self.in_world {
            (sun * 2.5 + 0.5).clamp(0.12, 1.0)
        } else {
            1.0
        };
        let day_sky = [0.55, 0.75, 0.95];
        let night_sky = [0.02, 0.03, 0.08];
        let f = daylight;
        self.renderer.sky_color = [
            night_sky[0] + (day_sky[0] - night_sky[0]) * f,
            night_sky[1] + (day_sky[1] - night_sky[1]) * f,
            night_sky[2] + (day_sky[2] - night_sky[2]) * f,
        ];

        // Sun for directional lighting. The sun arcs east->west over the day
        // (noon at time 0.25); we keep it a touch above the horizon while up so
        // shadows never degenerate. Warm direct light, cool sky-ambient fill,
        // both faded by `daylight` so night is lit only by the moonlit floor
        // and torches.
        let ang = self.server.time_of_day * std::f32::consts::TAU;
        let elev = ang.sin(); // 1 at noon, -1 at midnight
        let horiz = ang.cos(); // +1 dawn -> 0 noon -> -1 dusk
        let sun_dir = Vec3::new(horiz * 0.8, elev.max(0.05) + 0.15, 0.45).normalize();
        let sun_vis = elev.clamp(0.0, 1.0).sqrt(); // 0 below horizon
        // Golden hour: the sun's hue warms from near-white at noon to deep
        // orange as it nears the horizon.
        let noon = Vec3::new(1.0, 0.96, 0.86);
        let horizon = Vec3::new(1.0, 0.54, 0.26);
        let mut sun_col = horizon.lerp(noon, elev.clamp(0.0, 1.0).sqrt()) * (0.64 * sun_vis);
        let mut amb_col = Vec3::new(0.60, 0.68, 0.82) * (0.42 * daylight);

        // Weather gloom: fronts dim the direct sun hard and the ambient
        // gently, gray the sky, and pull the fog in. Lerped over ~10 s
        // so transitions read as skies changing, not a light switch.
        let gloom_target = if self.in_world {
            match self.server.world.weather {
                world::Weather::Clear => 0.0,
                world::Weather::Overcast => 0.4,
                world::Weather::Precip => 0.55,
                world::Weather::Storm => 0.7,
            }
        } else {
            0.0
        };
        self.weather_vis += (gloom_target - self.weather_vis) * (dt / 10.0).min(1.0);
        let gloom = self.weather_vis;
        sun_col *= 1.0 - gloom;
        amb_col *= 1.0 - gloom * 0.45;
        let gray = [0.36 * f, 0.39 * f, 0.44 * f];
        let mix = (gloom * 1.4).min(1.0);
        for (c, g) in self.renderer.sky_color.iter_mut().zip(gray) {
            *c += (g - *c) * mix;
        }

        // Storms flash: two frames of borrowed noon, thunder later.
        let mut daylight = daylight;
        if self.in_world && self.server.world.weather == world::Weather::Storm {
            self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
            if ((self.rng >> 8) as f32 / (1 << 24) as f32) < dt / 25.0 {
                self.lightning = 0.12;
                self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
                self.thunder_delay = 0.5 + ((self.rng >> 8) as f32 / (1 << 24) as f32) * 2.5;
            }
        }
        if self.lightning > 0.0 {
            self.lightning -= dt;
            daylight = 1.0;
            self.renderer.sky_color = [0.85, 0.88, 0.95];
            sun_col = Vec3::new(0.9, 0.92, 1.0);
        }
        if self.thunder_delay >= 0.0 {
            self.thunder_delay -= dt;
            if self.thunder_delay < 0.0 {
                self.sfx(Sfx::Thunder);
            }
        }

        // The weather bed follows what's actually falling where you stand.
        if let Some(a) = &self.audio {
            let (px, pz) = (
                self.player.pos.x.floor() as i32,
                self.player.pos.z.floor() as i32,
            );
            let want = if self.in_world
                && self.server.world.weather.precipitating()
                && self.server.world.rains_at(px, pz)
            {
                Some(if self.server.world.weather == world::Weather::Storm {
                    audio::Ambience::Storm
                } else {
                    audio::Ambience::Rain
                })
            } else {
                None
            };
            a.set_ambience(want);
        }

        let playing = self.screen == Screen::Playing;
        let outline = if playing {
            raycast::raycast(
                &self.server.world,
                self.camera.pos,
                self.camera.forward(),
                REACH,
            )
            .map(|h| h.block)
        } else {
            None
        };
        let underwater = self.player.head_underwater(&self.server.world);
        let fog = (self.config.view_dist as f32 - 0.5) * CHUNK_X as f32 * (1.0 - 0.35 * gloom);

        // World-space extras: item entities + mining crack overlay.
        let mut entity_verts = Vec::new();
        let mut entity_idx = Vec::new();
        let sample = |w: &World, p: Vec3| -> ([f32; 3], f32) {
            let (b, s) = w.light_rgb_at(
                p.x.floor() as i32,
                (p.y + 0.4).floor() as i32,
                p.z.floor() as i32,
            );
            (
                [b[0] as f32 / 15.0, b[1] as f32 / 15.0, b[2] as f32 / 15.0],
                s as f32 / 15.0,
            )
        };
        for it in &self.items {
            let lum = sample(&self.server.world, it.pos);
            it.emit(&self.reg, lum, &mut entity_verts, &mut entity_idx);
        }
        for m in &self.server.world.mobs {
            let lum = sample(&self.server.world, m.pos);
            m.emit(&self.reg, lum, &mut entity_verts, &mut entity_idx);
        }
        for p in &self.server.world.projectiles {
            p.emit(&mut entity_verts, &mut entity_idx);
        }
        // Fellow players, boxy and proud.
        let skin = *atlas::builtin_slots().get("player_skin").unwrap_or(&0);
        let face = *atlas::builtin_slots().get("player_face").unwrap_or(&0);
        if let Some(r) = &self.remote {
            for (_, pos, yaw) in r.players.values() {
                let lum = sample(&self.server.world, *pos);
                mobs::emit_humanoid(
                    *pos,
                    *yaw,
                    skin,
                    face,
                    lum,
                    &mut entity_verts,
                    &mut entity_idx,
                );
            }
        }
        if let Some(hst) = &self.host {
            for g in hst.guests.values() {
                let (pos, yaw) = g.render_pos();
                let lum = sample(&self.server.world, pos);
                mobs::emit_humanoid(
                    pos,
                    yaw,
                    skin,
                    face,
                    lum,
                    &mut entity_verts,
                    &mut entity_idx,
                );
            }
        }
        // Airborne sand tumbles as full-size cubes.
        for f in &self.server.world.falling.clone() {
            let lum = sample(&self.server.world, f.pos + Vec3::new(0.5, 0.5, 0.5));
            let d = self.reg.block(f.block);
            let ts = 1.0 / atlas::ATLAS_TILES as f32;
            let inset = ts / 32.0;
            for (face, corners) in mesher::CORNERS.iter().enumerate() {
                let slot = d.tiles[face];
                let (tx, ty) = (
                    slot as u32 % atlas::ATLAS_TILES,
                    slot as u32 / atlas::ATLAS_TILES,
                );
                let base = entity_verts.len() as u32;
                for c in corners.iter() {
                    let (uu, vv) = match face {
                        0 | 1 => (c[2], 1.0 - c[1]),
                        4 | 5 => (c[0], 1.0 - c[1]),
                        _ => (c[0], c[2]),
                    };
                    let shade = mesher::FACE_SHADE[face].max(0.6);
                    entity_verts.push(mesher::Vertex {
                        pos: [f.pos.x + c[0], f.pos.y + c[1], f.pos.z + c[2]],
                        uv: [
                            tx as f32 * ts + inset + uu * (ts - 2.0 * inset),
                            ty as f32 * ts + inset + vv * (ts - 2.0 * inset),
                        ],
                        normal: [0.0, 0.0, 0.0],
                        light: [shade * lum.0[0], shade * lum.0[1], shade * lum.0[2]],
                        sky: shade * lum.1,
                    });
                }
                entity_idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }

        // The steelworks show their work: a rested bloom on the anvil,
        // smoke over lit bloomeries and smoldering clamps.
        if self.in_world {
            let ts = 1.0 / atlas::ATLAS_TILES as f32;
            let inset = ts / 32.0;
            let smoke_slot = *atlas::builtin_slots().get("snow_flake").unwrap_or(&0);
            let t = self.time_abs;
            let sprite = |slot: u16,
                          cx: f32,
                          cy: f32,
                          cz: f32,
                          size: f32,
                          lum: f32,
                          verts: &mut Vec<mesher::Vertex>,
                          idx: &mut Vec<u32>| {
                let (tx, ty) = (
                    slot as u32 % atlas::ATLAS_TILES,
                    slot as u32 / atlas::ATLAS_TILES,
                );
                for (dx, dz) in [(1.0f32, 0.0f32), (0.0, 1.0)] {
                    for flip in [false, true] {
                        let base = verts.len() as u32;
                        let (u0, u1) = if flip {
                            ((tx + 1) as f32 * ts - inset, tx as f32 * ts + inset)
                        } else {
                            (tx as f32 * ts + inset, (tx + 1) as f32 * ts - inset)
                        };
                        let sgn = if flip { -1.0 } else { 1.0 };
                        for (o, dy, uu) in [
                            (-0.5 * size * sgn, 0.0, u0),
                            (0.5 * size * sgn, 0.0, u1),
                            (0.5 * size * sgn, size, u1),
                            (-0.5 * size * sgn, size, u0),
                        ] {
                            let vv = if dy == 0.0 {
                                (ty + 1) as f32 * ts - inset
                            } else {
                                ty as f32 * ts + inset
                            };
                            verts.push(mesher::Vertex {
                                pos: [cx + dx * o, cy + dy, cz + dz * o],
                                uv: [uu, vv],
                                normal: [0.0, 0.0, 0.0],
                                light: [lum; 3],
                                sky: lum,
                            });
                        }
                        idx.extend_from_slice(&[
                            base,
                            base + 1,
                            base + 2,
                            base,
                            base + 2,
                            base + 3,
                        ]);
                    }
                }
            };
            let mut work: Vec<(u16, f32, f32, f32, f32, f32)> = Vec::new();
            for (&(x, y, z), e) in &self.server.world.block_entities {
                match e {
                    world::BlockEntity::Anvil(a) => {
                        if let Some(b) = a.bloom {
                            let icon = self.reg.item(b.item).icon;
                            work.push((
                                icon,
                                x as f32 + 0.5,
                                y as f32 + 0.78,
                                z as f32 + 0.5,
                                0.32,
                                1.0,
                            ));
                        }
                    }
                    world::BlockEntity::Bloomery(b) if b.lit => {
                        for k in 0..3 {
                            let rise = (t * 0.7 + k as f32 * 0.65) % 2.0;
                            let drift = (t * 0.9 + k as f32 * 2.1).sin() * 0.2;
                            work.push((
                                smoke_slot,
                                x as f32 + 0.5 + drift,
                                y as f32 + 3.2 + rise,
                                z as f32 + 0.5,
                                0.5 + rise * 0.3,
                                0.12,
                            ));
                        }
                    }
                    world::BlockEntity::Clamp(_) => {
                        for k in 0..2 {
                            let rise = (t * 0.5 + k as f32 * 0.9) % 1.8;
                            let drift = (t * 0.8 + k as f32 * 1.7).sin() * 0.15;
                            work.push((
                                smoke_slot,
                                x as f32 + 0.5 + drift,
                                y as f32 + 1.2 + rise,
                                z as f32 + 0.5,
                                0.4 + rise * 0.25,
                                0.12,
                            ));
                        }
                    }
                    _ => {}
                }
            }
            for (slot, cx, cy, cz, size, lum) in work {
                sprite(
                    slot,
                    cx,
                    cy,
                    cz,
                    size,
                    lum,
                    &mut entity_verts,
                    &mut entity_idx,
                );
            }
        }
        // Precipitation: a cylinder of falling quads around the camera.
        // Each streak owns a column; roofed columns stay dry.
        if self.in_world && self.server.world.weather.precipitating() {
            let cam = self.camera.pos;
            let t = self.time_abs;
            let ts = 1.0 / atlas::ATLAS_TILES as f32;
            let inset = ts / 32.0;
            let rain_slot = *atlas::builtin_slots().get("rain_streak").unwrap_or(&0);
            let snow_slot = *atlas::builtin_slots().get("snow_flake").unwrap_or(&0);
            for i in 0..150u32 {
                let h = i.wrapping_mul(2654435761);
                let a = (h >> 8 & 0xffff) as f32 / 65536.0 * std::f32::consts::TAU;
                let r = 2.0 + (h >> 16 & 0xff) as f32 / 255.0 * 13.0;
                let phase = (h & 0xff) as f32 / 255.0;
                let wx = cam.x + a.cos() * r;
                let wz = cam.z + a.sin() * r;
                let (cx, cz) = (wx.floor() as i32, wz.floor() as i32);
                if !self.server.world.rains_at(cx, cz) {
                    continue;
                }
                let snow = self.server.world.snows_at(cx, cz);
                let speed = if snow { 3.0 } else { 13.0 };
                let span = 14.0;
                let y = cam.y + 7.0 - (t * speed + phase * span) % span;
                if self.server.world.light_at(cx, y.floor() as i32, cz).1 < 15 {
                    continue; // a roof owns this column
                }
                let slot = if snow { snow_slot } else { rain_slot };
                let (tx, ty) = (
                    slot as u32 % atlas::ATLAS_TILES,
                    slot as u32 / atlas::ATLAS_TILES,
                );
                let (w2, h2) = if snow { (0.09, 0.09) } else { (0.035, 0.55) };
                let drift = if snow {
                    (t * 1.3 + phase * 9.0).sin() * 0.25
                } else {
                    0.0
                };
                for (dx, dz) in [(1.0f32, 0.0f32), (0.0, 1.0)] {
                    for flip in [false, true] {
                        let base = entity_verts.len() as u32;
                        let (u0, u1) = if flip {
                            ((tx + 1) as f32 * ts - inset, tx as f32 * ts + inset)
                        } else {
                            (tx as f32 * ts + inset, (tx + 1) as f32 * ts - inset)
                        };
                        let sgn = if flip { -1.0 } else { 1.0 };
                        for (o, dy, uu) in [
                            (-0.5 * w2 * sgn, 0.0, u0),
                            (0.5 * w2 * sgn, 0.0, u1),
                            (0.5 * w2 * sgn, h2, u1),
                            (-0.5 * w2 * sgn, h2, u0),
                        ] {
                            let vv = if dy == 0.0 {
                                (ty + 1) as f32 * ts - inset
                            } else {
                                ty as f32 * ts + inset
                            };
                            entity_verts.push(mesher::Vertex {
                                pos: [wx + drift + dx * o, y + dy, wz + dz * o],
                                uv: [uu, vv],
                                normal: [0.0, 0.0, 0.0],
                                light: [0.5; 3],
                                sky: 0.9,
                            });
                        }
                        entity_idx.extend_from_slice(&[
                            base,
                            base + 1,
                            base + 2,
                            base,
                            base + 2,
                            base + 3,
                        ]);
                    }
                }
            }
        }
        let mut overlay_verts = Vec::new();
        let mut overlay_idx = Vec::new();
        if let Some((target, progress)) = self.breaking {
            entity::emit_crack(target, progress, &mut overlay_verts, &mut overlay_idx);
        }
        let mut hand_verts = Vec::new();
        let mut hand_idx = Vec::new();
        self.emit_hand(&mut hand_verts, &mut hand_idx);

        self.build_ui();

        match self.renderer.render(FrameInput {
            view_proj: self.camera.view_proj(),
            cam_pos: self.camera.pos,
            fog_dist: fog,
            underwater,
            daylight,
            sun_dir,
            sun_col,
            amb_col,
            outline,
            entity_verts: &entity_verts,
            entity_idx: &entity_idx,
            overlay_verts: &overlay_verts,
            overlay_idx: &overlay_idx,
            hand_verts: &hand_verts,
            hand_idx: &hand_idx,
            ui_verts: &self.ui.verts,
            crosshair: playing,
        }) {
            Ok(()) => {}
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                let size = self.window.inner_size();
                self.renderer.resize(size.width, size.height);
            }
            Err(e) => eprintln!("render error: {e:?}"),
        }

        // Headless verification: WILDFORGE_SHOT=path.ppm captures a frame once
        // the world is meshed, then exits.
        self.total_frames += 1;
        if let Some(path) = self.auto_shot.clone() {
            let shot_frame: u64 = std::env::var("WILDFORGE_SHOT_FRAME")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(240);
            if self.total_frames == shot_frame {
                eprintln!("fps at capture: {}", self.fps);
                self.renderer.pending_screenshot = Some(path);
            } else if self.total_frames > shot_frame + 1 {
                std::process::exit(0);
            }
        }

        // Window-title HUD.
        self.frames += 1;
        if (now - self.last_title).as_secs_f32() > 0.5 {
            self.fps = (self.frames as f32 / (now - self.last_title).as_secs_f32()) as u32;
            self.frames = 0;
            self.last_title = now;
            let p = self.player.pos;
            let biome = if self.in_world {
                format!(
                    " | {}",
                    self.server
                        .world
                        .generator
                        .biome(p.x as i32, p.z as i32)
                        .name()
                )
            } else {
                String::new()
            };
            self.window.set_title(&format!(
                "Wildforge — {} fps | XYZ {:.1} / {:.1} / {:.1}{biome}{}",
                self.fps,
                p.x,
                p.y,
                p.z,
                if self.mouse_captured || self.screen != Screen::Playing {
                    ""
                } else {
                    "  [click to capture mouse]"
                },
            ));
        }
    }

    // ---------- UI layout ----------

    const SLOT: f32 = 46.0;

    fn hotbar_origin(&self) -> (f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        ((w - 9.0 * Self::SLOT) / 2.0, h - Self::SLOT - 8.0)
    }

    fn hotbar_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let (x0, y0) = self.hotbar_origin();
        (x0 + i as f32 * Self::SLOT, y0, Self::SLOT, Self::SLOT)
    }

    /// Slot rects for the inventory screen: 0..9 hotbar row, 9..36 storage grid.
    fn inv_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let panel_w = 9.0 * Self::SLOT;
        let x0 = (w - panel_w) / 2.0;
        let grid_y = h / 2.0 - 2.0 * Self::SLOT;
        if i < HOTBAR_SLOTS {
            (
                x0 + i as f32 * Self::SLOT,
                grid_y + 3.0 * Self::SLOT + 14.0,
                Self::SLOT,
                Self::SLOT,
            )
        } else {
            let j = i - HOTBAR_SLOTS;
            (
                x0 + (j % 9) as f32 * Self::SLOT,
                grid_y + (j / 9) as f32 * Self::SLOT,
                Self::SLOT,
                Self::SLOT,
            )
        }
    }

    fn menu_button_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 - 150.0,
            h / 2.0 - 40.0 + i as f32 * 56.0,
            300.0,
            40.0,
        )
    }

    /// Kick button beside the pause menu, one row per connected guest.
    fn kick_rect(&self, row: usize) -> (f32, f32, f32, f32) {
        let (bx, by, bw, _) = self.menu_button_rect(2);
        (bx + bw + 16.0, by + row as f32 * 36.0, 70.0, 28.0)
    }

    /// Guest (id, name) rows in a stable order for the pause menu.
    fn guest_rows(&self) -> Vec<(u32, String)> {
        let Some(h) = &self.host else {
            return Vec::new();
        };
        let mut rows: Vec<(u32, String)> = h
            .guests
            .iter()
            .map(|(id, g)| (*id, g.name.clone()))
            .collect();
        rows.sort_by_key(|(id, _)| *id);
        rows
    }

    // ---- title screen layout ----

    fn title_row_y(&self, i: usize) -> f32 {
        self.renderer.config.height as f32 * 0.28 + i as f32 * 54.0
    }

    fn title_play_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        (w / 2.0 + 60.0, self.title_row_y(i), 100.0, 42.0)
    }

    fn title_delete_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        (w / 2.0 + 172.0, self.title_row_y(i), 46.0, 42.0)
    }

    /// 0 = new world, 1 = settings, 2 = quit.
    fn title_action_rect(&self, j: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let base = self.title_row_y(self.worlds.len().min(6)) + 26.0;
        (w / 2.0 - 150.0, base + j as f32 * 56.0, 300.0, 42.0)
    }

    // ---- texture packs screen layout ----

    /// Row 0 is "NONE", rows 1.. are discovered packs.
    fn pack_row_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 - 220.0, h * 0.20 + i as f32 * 68.0, 440.0, 42.0)
    }

    fn pack_back_rect(&self) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 - 150.0, h - 80.0, 300.0, 42.0)
    }

    // ---- settings screen layout ----

    const SLIDERS: [&'static str; 4] = ["VOLUME", "SENSITIVITY", "RENDER DIST", "FOV"];

    fn slider_bar_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 - 20.0, h * 0.30 + i as f32 * 64.0, 300.0, 30.0)
    }

    fn settings_back_rect(&self) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 - 150.0, h * 0.30 + 4.0 * 64.0 + 24.0, 300.0, 42.0)
    }

    fn slider_frac(&self, i: usize) -> f32 {
        match i {
            0 => self.config.volume,
            1 => (self.config.sensitivity - 0.1) / 2.9,
            2 => (self.config.view_dist - 4) as f32 / 8.0,
            _ => (self.config.fov - 50.0) / 60.0,
        }
    }

    fn slider_label(&self, i: usize) -> String {
        match i {
            0 => format!("{:.0}", self.config.volume * 100.0),
            1 => format!("{:.2}", self.config.sensitivity),
            2 => format!("{}", self.config.view_dist),
            _ => format!("{:.0}", self.config.fov),
        }
    }

    fn set_slider(&mut self, i: usize, frac: f32) {
        let f = frac.clamp(0.0, 1.0);
        match i {
            0 => self.config.volume = (f * 20.0).round() / 20.0,
            1 => self.config.sensitivity = ((0.1 + f * 2.9) * 20.0).round() / 20.0,
            2 => self.config.view_dist = 4 + (f * 8.0).round() as i32,
            _ => self.config.fov = 50.0 + (f * 60.0).round(),
        }
        self.apply_config();
    }

    fn draw_button(ui: &mut UiBatch, r: (f32, f32, f32, f32), label: &str, hover: bool) {
        let bg = if hover {
            [0.5, 0.5, 0.5, 0.95]
        } else {
            [0.25, 0.25, 0.25, 0.95]
        };
        ui.rect(r.0, r.1, r.2, r.3, [0.1, 0.1, 0.1, 0.95]);
        ui.rect(r.0 + 2.0, r.1 + 2.0, r.2 - 4.0, r.3 - 4.0, bg);
        let lw = UiBatch::text_width(2.0, label);
        ui.text_shadow(
            r.0 + (r.2 - lw) / 2.0,
            r.1 + (r.3 - 14.0) / 2.0,
            2.0,
            label,
            [1.0; 4],
        );
    }

    fn hit(&self, r: (f32, f32, f32, f32)) -> bool {
        let (x, y) = self.ui_cursor;
        x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3
    }

    fn draw_slot(
        reg: &Registry,
        ui: &mut UiBatch,
        r: (f32, f32, f32, f32),
        stack: Option<ItemStack>,
        selected: bool,
        hover: bool,
    ) {
        let (x, y, w, h) = r;
        let border = if selected {
            [1.0, 1.0, 1.0, 0.9]
        } else {
            [0.35, 0.35, 0.35, 0.9]
        };
        ui.rect(x + 1.0, y + 1.0, w - 2.0, h - 2.0, border);
        let bg = if hover {
            [0.45, 0.45, 0.45, 0.92]
        } else {
            [0.18, 0.18, 0.18, 0.92]
        };
        ui.rect(x + 3.0, y + 3.0, w - 6.0, h - 6.0, bg);
        if let Some(s) = stack {
            let pad = 8.0;
            let icon = reg.item(s.item).icon;
            let tile = icon;
            ui.tile(
                x + pad,
                y + pad,
                w - 2.0 * pad,
                h - 2.0 * pad,
                tile,
                [1.0; 4],
            );
            if s.count > 1 {
                let txt = format!("{}", s.count);
                let tw = UiBatch::text_width(2.0, &txt);
                ui.text_shadow(x + w - tw - 4.0, y + h - 18.0, 2.0, &txt, [1.0; 4]);
            }
            // Durability bar for worn tools.
            let max = reg.item(s.item).durability;
            if max > 0 && s.durability < max {
                let frac = s.durability as f32 / max as f32;
                ui.rect(x + 6.0, y + h - 9.0, w - 12.0, 4.0, [0.05, 0.05, 0.05, 0.9]);
                ui.rect(
                    x + 6.0,
                    y + h - 9.0,
                    (w - 12.0) * frac,
                    4.0,
                    [1.0 - frac, frac, 0.1, 1.0],
                );
            }
        }
    }

    /// Craft grid layout: grid slots then the result slot to their right.
    fn craft_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let n = self.craft_size;
        let (sx, sy, _, _) = self.inv_slot_rect(HOTBAR_SLOTS); // storage top-left
        let y0 = sy - (n as f32) * Self::SLOT - 16.0;
        let x0 = sx + 1.5 * Self::SLOT;
        (
            x0 + (i % n) as f32 * Self::SLOT,
            y0 + (i / n) as f32 * Self::SLOT,
            Self::SLOT,
            Self::SLOT,
        )
    }

    fn result_slot_rect(&self) -> (f32, f32, f32, f32) {
        let n = self.craft_size;
        let (gx, gy, _, _) = self.craft_slot_rect(0);
        (
            gx + n as f32 * Self::SLOT + Self::SLOT,
            gy + ((n as f32) - 1.0) * Self::SLOT / 2.0,
            Self::SLOT,
            Self::SLOT,
        )
    }

    fn build_ui(&mut self) {
        let mut ui = std::mem::replace(&mut self.ui, UiBatch::new());
        ui.clear();
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;

        // Menu-only screens draw over the sky and skip the HUD entirely.
        match self.screen {
            Screen::Title => {
                ui.rect(0.0, 0.0, w, h, [0.05, 0.08, 0.15, 0.55]);
                let tw = UiBatch::text_width(8.0, "WILDFORGE");
                ui.text_shadow(
                    (w - tw) / 2.0,
                    h * 0.10,
                    8.0,
                    "WILDFORGE",
                    [1.0, 0.95, 0.7, 1.0],
                );
                if self.worlds.is_empty() {
                    let msg = "NO WORLDS YET - CREATE ONE";
                    let mw = UiBatch::text_width(2.0, msg);
                    ui.text_shadow(
                        (w - mw) / 2.0,
                        self.title_row_y(0) + 12.0,
                        2.0,
                        msg,
                        [0.8, 0.8, 0.8, 1.0],
                    );
                }
                for (i, (name, seed)) in self.worlds.iter().take(6).enumerate() {
                    let y = self.title_row_y(i);
                    let label = format!("{}  SEED {}", name.to_uppercase(), seed);
                    ui.text_shadow(w / 2.0 - 310.0, y + 13.0, 2.0, &label, [1.0; 4]);
                    let pr = self.title_play_rect(i);
                    Self::draw_button(&mut ui, pr, "PLAY", self.hit(pr));
                    let dr = self.title_delete_rect(i);
                    Self::draw_button(&mut ui, dr, "X", self.hit(dr));
                }
                for (j, label) in [
                    "NEW SURVIVAL WORLD",
                    "NEW CREATIVE WORLD",
                    "JOIN GAME",
                    "MODS",
                    "TEXTURE PACKS",
                    "SETTINGS",
                    "QUIT",
                ]
                .iter()
                .enumerate()
                {
                    let r = self.title_action_rect(j);
                    Self::draw_button(&mut ui, r, label, self.hit(r));
                }
                self.ui = ui;
                return;
            }
            Screen::Mods => {
                ui.rect(0.0, 0.0, w, h, [0.02, 0.05, 0.1, 0.75]);
                let tw = UiBatch::text_width(4.0, "MODS");
                ui.text_shadow((w - tw) / 2.0, h * 0.10, 4.0, "MODS", [1.0; 4]);
                let mut y = h * 0.24;
                for m in self.reg.mods.iter().take(10) {
                    let script = if m.has_script { " +SCRIPT" } else { "" };
                    let line = format!("{} {}{}", m.name.to_uppercase(), m.version, script);
                    ui.text_shadow(w / 2.0 - 300.0, y, 2.0, &line, [1.0; 4]);
                    match &m.error {
                        Some(e) => {
                            let msg: String = e.chars().take(60).collect();
                            ui.text_shadow(
                                w / 2.0 - 300.0,
                                y + 20.0,
                                1.5,
                                &msg.to_uppercase(),
                                [1.0, 0.5, 0.5, 1.0],
                            );
                            y += 44.0;
                        }
                        None => {
                            ui.text_shadow(w / 2.0 + 200.0, y, 2.0, "OK", [0.5, 1.0, 0.5, 1.0]);
                            y += 30.0;
                        }
                    }
                }
                let hint = "EDIT MODS/ WHILE PLAYING - CHANGES HOT RELOAD. F5 FORCES.";
                ui.text_shadow(w / 2.0 - 300.0, y + 16.0, 1.5, hint, [0.7, 0.7, 0.7, 1.0]);
                let br = self.menu_button_rect(4);
                Self::draw_button(&mut ui, br, "BACK", self.hit(br));
                self.ui = ui;
                return;
            }
            Screen::Packs => {
                ui.rect(0.0, 0.0, w, h, [0.02, 0.05, 0.1, 0.75]);
                let tw = UiBatch::text_width(4.0, "TEXTURE PACKS");
                ui.text_shadow((w - tw) / 2.0, h * 0.08, 4.0, "TEXTURE PACKS", [1.0; 4]);
                for i in 0..=self.packs.len().min(7) {
                    let r = self.pack_row_rect(i);
                    let label = if i == 0 {
                        "NONE - PROCEDURAL".to_string()
                    } else {
                        self.packs[i - 1].name.to_uppercase()
                    };
                    Self::draw_button(&mut ui, r, &label, self.hit(r));
                    let cur = self.active_pack_id();
                    let active = if i == 0 {
                        cur.is_empty() || pack_source_of(&cur).is_none()
                    } else {
                        self.packs[i - 1].id == cur
                    };
                    if active {
                        ui.text_shadow(
                            r.0 + r.2 + 18.0,
                            r.1 + 12.0,
                            2.0,
                            "ACTIVE",
                            [0.5, 1.0, 0.5, 1.0],
                        );
                    }
                    if i > 0 && !self.packs[i - 1].description.is_empty() {
                        let d: String = self.packs[i - 1].description.chars().take(64).collect();
                        ui.text_shadow(
                            r.0 + 8.0,
                            r.1 + r.3 + 4.0,
                            1.5,
                            &d.to_uppercase(),
                            [0.7, 0.7, 0.7, 1.0],
                        );
                    }
                }
                let mut y = self.pack_row_rect(self.packs.len().min(7)).1 + 60.0;
                for warn in self.pack_warnings.iter().take(3) {
                    let msg: String = warn.chars().take(70).collect();
                    ui.text_shadow(
                        w / 2.0 - 300.0,
                        y,
                        1.5,
                        &msg.to_uppercase(),
                        [1.0, 0.5, 0.5, 1.0],
                    );
                    y += 20.0;
                }
                let hint = "DROP PACKS IN PACKS/ - PNG EDITS HOT RELOAD LIVE.";
                ui.text_shadow(w / 2.0 - 300.0, y + 4.0, 1.5, hint, [0.7, 0.7, 0.7, 1.0]);
                let br = self.pack_back_rect();
                Self::draw_button(&mut ui, br, "BACK", self.hit(br));
                self.ui = ui;
                return;
            }
            Screen::Join => {
                ui.rect(0.0, 0.0, w, h, [0.02, 0.05, 0.1, 0.75]);
                let tw = UiBatch::text_width(4.0, "JOIN GAME");
                ui.text_shadow((w - tw) / 2.0, h * 0.08, 4.0, "JOIN GAME", [1.0; 4]);
                if let Some(d) = &mut self.discovery {
                    d.poll();
                }
                let found: Vec<(std::net::SocketAddr, String)> = self
                    .discovery
                    .as_ref()
                    .map(|d| d.found.clone())
                    .unwrap_or_default();
                if found.is_empty() {
                    ui.text_shadow(
                        w / 2.0 - 220.0,
                        h * 0.20 + 10.0,
                        2.0,
                        "SEARCHING THE LAN...",
                        [0.7, 0.7, 0.7, 1.0],
                    );
                }
                for (i, (addr, name)) in found.iter().take(5).enumerate() {
                    let r = (w / 2.0 - 220.0, h * 0.20 + i as f32 * 56.0, 440.0, 42.0);
                    Self::draw_button(
                        &mut ui,
                        r,
                        &format!("{} - {}", name.to_uppercase(), addr),
                        self.hit(r),
                    );
                }
                // The searching line occupies one row when the list is
                // empty; the click handler mirrors this formula.
                let y = h * 0.20 + found.len().clamp(1, 5) as f32 * 56.0 + 26.0;
                ui.text_shadow(w / 2.0 - 220.0, y, 2.0, "DIRECT IP:", [1.0; 4]);
                ui.rect(w / 2.0 - 80.0, y - 6.0, 300.0, 34.0, [0.1, 0.1, 0.1, 0.95]);
                let shown = if self.join_ip.is_empty() {
                    "TYPE ADDRESS"
                } else {
                    &self.join_ip
                };
                let col = if self.join_ip.is_empty() {
                    [0.5, 0.5, 0.5, 1.0]
                } else {
                    [1.0; 4]
                };
                ui.text_shadow(w / 2.0 - 72.0, y, 2.0, &shown.to_uppercase(), col);
                let cr = (w / 2.0 + 240.0, y - 6.0, 160.0, 34.0);
                Self::draw_button(&mut ui, cr, "CONNECT", self.hit(cr));
                if !self.join_status.is_empty() {
                    ui.text_shadow(
                        w / 2.0 - 220.0,
                        y + 46.0,
                        2.0,
                        &self.join_status,
                        [1.0, 0.6, 0.5, 1.0],
                    );
                }
                let br = self.pack_back_rect();
                Self::draw_button(&mut ui, br, "BACK", self.hit(br));
                self.ui = ui;
                return;
            }
            Screen::Settings => {
                ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.6]);
                let tw = UiBatch::text_width(4.0, "SETTINGS");
                ui.text_shadow((w - tw) / 2.0, h * 0.12, 4.0, "SETTINGS", [1.0; 4]);
                for i in 0..Self::SLIDERS.len() {
                    let (bx, by, bw, bh) = self.slider_bar_rect(i);
                    ui.text_shadow(w / 2.0 - 300.0, by + 8.0, 2.0, Self::SLIDERS[i], [1.0; 4]);
                    ui.rect(bx, by, bw, bh, [0.1, 0.1, 0.1, 0.95]);
                    let frac = self.slider_frac(i);
                    ui.rect(
                        bx + 2.0,
                        by + 2.0,
                        (bw - 4.0) * frac,
                        bh - 4.0,
                        [0.35, 0.65, 0.35, 0.95],
                    );
                    // Handle notch.
                    let hx = bx + 2.0 + (bw - 8.0) * frac;
                    ui.rect(hx, by, 4.0, bh, [0.9, 0.9, 0.9, 1.0]);
                    ui.text_shadow(
                        bx + bw + 14.0,
                        by + 8.0,
                        2.0,
                        &self.slider_label(i),
                        [1.0; 4],
                    );
                }
                let br = self.settings_back_rect();
                Self::draw_button(&mut ui, br, "BACK", self.hit(br));
                self.ui = ui;
                return;
            }
            Screen::ConfirmDelete => {
                ui.rect(0.0, 0.0, w, h, [0.1, 0.02, 0.02, 0.7]);
                let name = self
                    .pending_delete
                    .and_then(|i| self.worlds.get(i))
                    .map(|(n, _)| n.to_uppercase())
                    .unwrap_or_default();
                let msg = format!("DELETE {name}?");
                let tw = UiBatch::text_width(4.0, &msg);
                ui.text_shadow((w - tw) / 2.0, h * 0.25, 4.0, &msg, [1.0, 0.8, 0.8, 1.0]);
                let sub = "THIS CANNOT BE UNDONE";
                let sw = UiBatch::text_width(2.0, sub);
                ui.text_shadow(
                    (w - sw) / 2.0,
                    h * 0.25 + 50.0,
                    2.0,
                    sub,
                    [0.9, 0.7, 0.7, 1.0],
                );
                for (j, label) in ["DELETE", "CANCEL"].iter().enumerate() {
                    let r = self.menu_button_rect(j);
                    Self::draw_button(&mut ui, r, label, self.hit(r));
                }
                self.ui = ui;
                return;
            }
            _ => {}
        }

        // Damage flash vignette.
        if self.damage_flash > 0.0 {
            ui.rect(0.0, 0.0, w, h, [0.8, 0.1, 0.1, self.damage_flash * 0.55]);
        }

        // Mod/system toasts, top center.
        for (i, (msg, ttl)) in self.toasts.iter().enumerate() {
            let a = ttl.min(1.0);
            let m = msg.to_uppercase();
            let tw = UiBatch::text_width(2.0, &m);
            ui.text_shadow(
                (w - tw) / 2.0,
                16.0 + i as f32 * 22.0,
                2.0,
                &m,
                [1.0, 1.0, 0.6, a],
            );
        }

        // Hotbar.
        for i in 0..HOTBAR_SLOTS {
            let r = self.hotbar_rect(i);
            Self::draw_slot(
                &self.reg,
                &mut ui,
                r,
                self.inventory.slots[i],
                i == self.hotbar_sel,
                false,
            );
        }
        // Selected item name above the hotbar.
        if let Some(s) = self.inventory.slots[self.hotbar_sel] {
            let name = &self.reg.item(s.item).label.to_uppercase();
            let tw = UiBatch::text_width(2.0, name);
            let (hx0, hy0) = self.hotbar_origin();
            ui.text_shadow(
                hx0 + (9.0 * Self::SLOT - tw) / 2.0,
                hy0 - 52.0,
                2.0,
                name,
                [1.0; 4],
            );
        }

        // Hearts above the hotbar (count follows max health).
        let (hx, hy) = self.hotbar_origin();
        let hs = 2.6;
        let hearts = if self.creative {
            0
        } else {
            (self.max_health() / 2.0).ceil() as i32
        };
        for i in 0..hearts {
            let kind = if self.health >= (i * 2 + 2) as f32 {
                2
            } else if self.health >= (i * 2 + 1) as f32 {
                1
            } else {
                0
            };
            ui.heart(hx + i as f32 * 8.0 * hs, hy - 8.0 * hs - 4.0, hs, kind);
        }
        // Armor pips above the hearts, only while wearing any.
        let ap = if self.creative {
            0
        } else {
            self.armor_points()
        };
        for i in 0..ap.min(15) {
            let x = hx + i as f32 * 6.0 * hs * 0.8;
            ui.rect(
                x,
                hy - 14.0 * hs - 6.0,
                4.0 * hs,
                4.0 * hs,
                [0.75, 0.72, 0.6, 0.95],
            );
        }
        // Hunger pips, right-aligned above the hotbar.
        let pips = (self.hunger / 2.0).ceil() as i32;
        for i in 0..if self.creative { 0 } else { 10 } {
            let x = hx + 9.0 * Self::SLOT - (i + 1) as f32 * 8.0 * hs;
            let a = if i < pips { 1.0 } else { 0.25 };
            ui.rect(
                x,
                hy - 8.0 * hs - 4.0 + 4.0,
                6.0 * hs * 0.7,
                5.0 * hs * 0.7,
                [0.85, 0.55, 0.2, a],
            );
        }
        // Bow draw near the crosshair (red until min draw, then filling).
        if self.bow_draw > 0.0 {
            let t = ((self.bow_draw - 0.25) / 0.75).clamp(0.0, 1.0);
            ui.rect(
                w / 2.0 - 30.0,
                h / 2.0 + 24.0,
                60.0,
                6.0,
                [0.1, 0.1, 0.1, 0.8],
            );
            let col = if self.bow_draw < 0.25 {
                [0.7, 0.3, 0.2, 0.95]
            } else {
                [0.75, 0.9, 0.5, 0.95]
            };
            ui.rect(w / 2.0 - 30.0, h / 2.0 + 24.0, 60.0 * t.max(0.06), 6.0, col);
        }
        // Chat entry line.
        if self.chat_open {
            ui.rect(12.0, h - 46.0, w * 0.5, 30.0, [0.0, 0.0, 0.0, 0.7]);
            let line = format!("SAY: {}_", self.chat_text.to_uppercase());
            ui.text_shadow(18.0, h - 40.0, 2.0, &line, [1.0; 4]);
        }
        // Remote players: name tags projected into the world.
        if let Some(r) = &self.remote {
            let vp = self.camera.view_proj();
            for (name, pos, _) in r.players.values() {
                let head = *pos + Vec3::new(0.0, 2.1, 0.0);
                let clip = vp * head.extend(1.0);
                if clip.w > 0.5 {
                    let sx = (clip.x / clip.w * 0.5 + 0.5) * w;
                    let sy = (0.5 - clip.y / clip.w * 0.5) * h;
                    let tw2 = UiBatch::text_width(1.5, &name.to_uppercase());
                    ui.text_shadow(sx - tw2 / 2.0, sy, 1.5, &name.to_uppercase(), [1.0; 4]);
                }
            }
        }
        if let Some(hst) = &self.host {
            let vp = self.camera.view_proj();
            for g in hst.guests.values() {
                let head = g.render_pos().0 + Vec3::new(0.0, 2.1, 0.0);
                let clip = vp * head.extend(1.0);
                if clip.w > 0.5 {
                    let sx = (clip.x / clip.w * 0.5 + 0.5) * w;
                    let sy = (0.5 - clip.y / clip.w * 0.5) * h;
                    let tw2 = UiBatch::text_width(1.5, &g.name.to_uppercase());
                    ui.text_shadow(sx - tw2 / 2.0, sy, 1.5, &g.name.to_uppercase(), [1.0; 4]);
                }
            }
        }
        // Brushing progress near the crosshair.
        if self.anvil_work > 0.0 {
            let t = (self.anvil_work / 2.0).min(1.0);
            ui.rect(
                w / 2.0 - 30.0,
                h / 2.0 + 24.0,
                60.0,
                6.0,
                [0.1, 0.1, 0.1, 0.8],
            );
            ui.rect(
                w / 2.0 - 30.0,
                h / 2.0 + 24.0,
                60.0 * t,
                6.0,
                [0.85, 0.85, 0.9, 0.95],
            );
        }
        if self.brushing > 0.0 {
            let t = (self.brushing / 1.5).min(1.0);
            ui.rect(
                w / 2.0 - 30.0,
                h / 2.0 + 24.0,
                60.0,
                6.0,
                [0.1, 0.1, 0.1, 0.8],
            );
            ui.rect(
                w / 2.0 - 30.0,
                h / 2.0 + 24.0,
                60.0 * t,
                6.0,
                [0.75, 0.7, 0.5, 0.95],
            );
        }
        // Eat progress near the crosshair.
        if self.eating > 0.0
            && let Some(f) = self.inventory.slots[self.hotbar_sel]
                .and_then(|s| self.reg.item(s.item).food.clone())
        {
            let t = (self.eating / f.eat_time).min(1.0);
            ui.rect(
                w / 2.0 - 30.0,
                h / 2.0 + 24.0,
                60.0,
                6.0,
                [0.1, 0.1, 0.1, 0.8],
            );
            ui.rect(
                w / 2.0 - 30.0,
                h / 2.0 + 24.0,
                60.0 * t,
                6.0,
                [0.9, 0.8, 0.3, 0.95],
            );
        }

        // Air bubbles (right-aligned above hotbar) when submerged.
        if self.air < MAX_AIR && !self.creative {
            let n = (self.air / MAX_AIR * 10.0).ceil() as usize;
            for i in 0..n {
                let x = hx + 9.0 * Self::SLOT - (i + 1) as f32 * 8.0 * hs;
                ui.bubble(x, hy - 16.0 * hs - 8.0, hs);
            }
        }

        match self.screen {
            Screen::Playing
            | Screen::Title
            | Screen::Mods
            | Screen::Packs
            | Screen::Join
            | Screen::Settings
            | Screen::ConfirmDelete => {}
            Screen::Furnace(pos) => {
                ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
                let title = "FURNACE";
                let tw = UiBatch::text_width(3.0, title);
                ui.text_shadow((w - tw) / 2.0, h / 2.0 - 285.0, 3.0, title, [1.0; 4]);
                let (inp, fuel, out, prog, burn) = self.furnace_view(pos);
                let ir = self.furnace_slot_rect(0);
                let fr = self.furnace_slot_rect(1);
                let orr = self.furnace_slot_rect(2);
                Self::draw_slot(&self.reg, &mut ui, ir, inp, false, self.hit(ir));
                Self::draw_slot(&self.reg, &mut ui, fr, fuel, false, self.hit(fr));
                Self::draw_slot(&self.reg, &mut ui, orr, out, false, self.hit(orr));
                // Flame between input and fuel, arrow toward the output.
                let flame_h = 24.0 * burn;
                ui.rect(
                    ir.0 + 12.0,
                    fr.1 - 4.0 - flame_h,
                    22.0,
                    flame_h,
                    [1.0, 0.55, 0.1, 0.95],
                );
                let ay = ir.1 + Self::SLOT + 14.0;
                ui.rect(ir.0 + 64.0, ay, 100.0, 8.0, [0.15, 0.15, 0.15, 0.9]);
                ui.rect(ir.0 + 64.0, ay, 100.0 * prog, 8.0, [1.0, 1.0, 1.0, 0.95]);
                // Player inventory below for restocking.
                for i in 0..TOTAL_SLOTS {
                    let r = self.inv_slot_rect(i);
                    Self::draw_slot(
                        &self.reg,
                        &mut ui,
                        r,
                        self.inventory.slots[i],
                        i == self.hotbar_sel,
                        self.hit(r),
                    );
                }
                self.draw_browser(&mut ui);
                if let Some(s) = self.held_stack {
                    let (cx, cy) = self.ui_cursor;
                    let icon = self.reg.item(s.item).icon;
                    ui.tile(cx - 16.0, cy - 16.0, 32.0, 32.0, icon, [1.0; 4]);
                    if s.count > 1 {
                        ui.text_shadow(cx + 6.0, cy + 4.0, 2.0, &format!("{}", s.count), [1.0; 4]);
                    }
                }
                self.ui = ui;
                return;
            }
            Screen::Bloomery(pos) => {
                ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
                let title = "BLOOMERY";
                let tw = UiBatch::text_width(3.0, title);
                ui.text_shadow((w - tw) / 2.0, h / 2.0 - 300.0, 3.0, title, [1.0; 4]);
                let (slots, lit, progress, breached) = {
                    let breached = self
                        .server
                        .world
                        .check_bloomery(pos.0, pos.1, pos.2)
                        .is_none();
                    match self.server.world.block_entities.get(&pos) {
                        Some(world::BlockEntity::Bloomery(b)) => {
                            let mut v = [None; 8];
                            v[..4].copy_from_slice(&b.charge);
                            v[4..].copy_from_slice(&b.fuel);
                            (v, b.lit, b.progress / world::BLOOMERY_FIRE_SECS, breached)
                        }
                        _ => ([None; 8], false, 0.0, breached),
                    }
                };
                ui.text_shadow(w / 2.0 - 150.0, h / 2.0 - 268.0, 1.5, "CHARGE", [1.0; 4]);
                ui.text_shadow(w / 2.0 - 150.0, h / 2.0 - 186.0, 1.5, "CHARCOAL", [1.0; 4]);
                for (i, s) in slots.iter().enumerate() {
                    let r = self.bloomery_slot_rect(i);
                    Self::draw_slot(&self.reg, &mut ui, r, *s, false, self.hit(r));
                }
                let lr = self.bloomery_light_rect();
                if lit {
                    let br = (
                        w / 2.0 - 2.0 * (Self::SLOT + 10.0) + 5.0,
                        h / 2.0 - 120.0,
                        4.0 * (Self::SLOT + 10.0) - 10.0,
                        10.0,
                    );
                    ui.rect(br.0, br.1, br.2, br.3, [0.15, 0.15, 0.15, 0.9]);
                    ui.rect(br.0, br.1, br.2 * progress, br.3, [1.0, 0.55, 0.1, 0.95]);
                    ui.text_shadow(
                        br.0,
                        br.1 + 16.0,
                        1.5,
                        "FIRING - SEALED",
                        [1.0, 0.8, 0.5, 1.0],
                    );
                } else if breached {
                    ui.text_shadow(
                        lr.0,
                        lr.1 + 44.0,
                        1.5,
                        "THE STACK IS BREACHED",
                        [1.0, 0.5, 0.4, 1.0],
                    );
                    Self::draw_button(&mut ui, lr, "LIGHT", false);
                } else {
                    Self::draw_button(&mut ui, lr, "LIGHT", self.hit(lr));
                }
                for i in 0..TOTAL_SLOTS {
                    let r = self.inv_slot_rect(i);
                    Self::draw_slot(
                        &self.reg,
                        &mut ui,
                        r,
                        self.inventory.slots[i],
                        i == self.hotbar_sel,
                        self.hit(r),
                    );
                }
                self.draw_browser(&mut ui);
                if let Some(s) = self.held_stack {
                    let (cx, cy) = self.ui_cursor;
                    let icon = self.reg.item(s.item).icon;
                    ui.tile(cx - 16.0, cy - 16.0, 32.0, 32.0, icon, [1.0; 4]);
                    if s.count > 1 {
                        ui.text_shadow(cx + 6.0, cy + 4.0, 2.0, &format!("{}", s.count), [1.0; 4]);
                    }
                }
                self.ui = ui;
                return;
            }
            Screen::Chest(pos) => {
                ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
                let title = "CHEST";
                let tw = UiBatch::text_width(3.0, title);
                ui.text_shadow((w - tw) / 2.0, h / 2.0 - 340.0, 3.0, title, [1.0; 4]);
                let slots = match self.server.world.block_entities.get(&pos) {
                    Some(world::BlockEntity::Chest(c)) => c.slots,
                    _ => [None; world::CHEST_SLOTS],
                };
                for (i, st) in slots.iter().enumerate() {
                    let r = self.chest_slot_rect(i);
                    Self::draw_slot(&self.reg, &mut ui, r, *st, false, self.hit(r));
                }
                for i in 0..TOTAL_SLOTS {
                    let r = self.inv_slot_rect(i);
                    Self::draw_slot(
                        &self.reg,
                        &mut ui,
                        r,
                        self.inventory.slots[i],
                        i == self.hotbar_sel,
                        self.hit(r),
                    );
                }
                self.draw_browser(&mut ui);
                if let Some(s) = self.held_stack {
                    let (cx, cy) = self.ui_cursor;
                    let icon = self.reg.item(s.item).icon;
                    ui.tile(cx - 16.0, cy - 16.0, 32.0, 32.0, icon, [1.0; 4]);
                    if s.count > 1 {
                        ui.text_shadow(cx + 6.0, cy + 4.0, 2.0, &format!("{}", s.count), [1.0; 4]);
                    }
                }
                self.ui = ui;
                return;
            }
            Screen::Offering(pos) => {
                ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
                let title = "OFFERING STONE";
                let tw = UiBatch::text_width(3.0, title);
                ui.text_shadow((w - tw) / 2.0, h / 2.0 - 260.0, 3.0, title, [1.0; 4]);
                let hint = "LEFT AT DUSK, TAKEN BY DAWN";
                let hw2 = UiBatch::text_width(1.5, hint);
                ui.text_shadow(
                    (w - hw2) / 2.0,
                    h / 2.0 - 232.0,
                    1.5,
                    hint,
                    [0.7, 0.85, 0.65, 1.0],
                );
                let slots = match self.server.world.block_entities.get(&pos) {
                    Some(world::BlockEntity::Offering(o)) => o.slots,
                    _ => [None; 3],
                };
                for (i, st) in slots.iter().enumerate() {
                    let r = self.offering_slot_rect(i);
                    Self::draw_slot(&self.reg, &mut ui, r, *st, false, self.hit(r));
                }
                for i in 0..TOTAL_SLOTS {
                    let r = self.inv_slot_rect(i);
                    Self::draw_slot(
                        &self.reg,
                        &mut ui,
                        r,
                        self.inventory.slots[i],
                        i == self.hotbar_sel,
                        self.hit(r),
                    );
                }
                self.draw_browser(&mut ui);
                if let Some(s) = self.held_stack {
                    let (cx, cy) = self.ui_cursor;
                    let icon = self.reg.item(s.item).icon;
                    ui.tile(cx - 16.0, cy - 16.0, 32.0, 32.0, icon, [1.0; 4]);
                    if s.count > 1 {
                        ui.text_shadow(cx + 6.0, cy + 4.0, 2.0, &format!("{}", s.count), [1.0; 4]);
                    }
                }
                self.ui = ui;
                return;
            }
            Screen::Inventory => {
                ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
                let title = if self.craft_size == 3 {
                    "CRAFTING"
                } else {
                    "INVENTORY"
                };
                let tw = UiBatch::text_width(3.0, title);
                let (c0x, c0y, _, _) = self.craft_slot_rect(0);
                ui.text_shadow((w - tw) / 2.0, c0y - 40.0, 3.0, title, [1.0; 4]);
                for i in 0..TOTAL_SLOTS {
                    let r = self.inv_slot_rect(i);
                    let hover = self.hit(r);
                    Self::draw_slot(
                        &self.reg,
                        &mut ui,
                        r,
                        self.inventory.slots[i],
                        i == self.hotbar_sel,
                        hover,
                    );
                }
                // Nutrition panel.
                let (p0x, p0y, _, _) = self.inv_slot_rect(HOTBAR_SLOTS);
                let names = ["GRAIN", "VEG", "FRUIT", "FUNGI", "PROT"];
                let cols = [
                    [0.85, 0.7, 0.25, 1.0],
                    [0.35, 0.75, 0.3, 1.0],
                    [0.85, 0.3, 0.3, 1.0],
                    [0.6, 0.45, 0.3, 1.0],
                    [0.8, 0.4, 0.35, 1.0],
                ];
                for i in 0..5 {
                    let y = p0y + i as f32 * 26.0;
                    ui.text_shadow(p0x - 190.0, y, 1.5, names[i], [1.0; 4]);
                    ui.rect(p0x - 130.0, y, 100.0, 10.0, [0.12, 0.12, 0.12, 0.9]);
                    let v = self.nutrition[i] / 100.0;
                    ui.rect(p0x - 130.0, y, 100.0 * v, 10.0, cols[i]);
                    if self.nutrition[i] >= 40.0 {
                        ui.text_shadow(p0x - 24.0, y, 1.5, "+", [0.6, 1.0, 0.6, 1.0]);
                    }
                }
                let bonus = (self.max_health() - MAX_HEALTH) as i32 / 2;
                ui.text_shadow(
                    p0x - 190.0,
                    p0y + 134.0,
                    1.5,
                    &format!("MAX HEALTH +{bonus}"),
                    [1.0; 4],
                );
                // The calendar: day count and where the season stands.
                let w = &self.server.world;
                let third = ["EARLY", "MID", "LATE"][((w.season_progress() * 3.0) as usize).min(2)];
                ui.text_shadow(
                    p0x - 190.0,
                    p0y + 158.0,
                    1.5,
                    &format!("DAY {} - {third} {}", w.day + 1, world::SEASONS[w.season()]),
                    [0.85, 0.9, 1.0, 1.0],
                );
                // The wild's ire: tier word + vine meter.
                let tier = self.server.world.ire_tier();
                let tier_col = [
                    [0.45, 0.75, 0.4, 1.0],
                    [0.8, 0.75, 0.35, 1.0],
                    [0.9, 0.55, 0.25, 1.0],
                    [0.9, 0.3, 0.25, 1.0],
                ][tier];
                ui.text_shadow(p0x - 190.0, p0y + 162.0, 1.5, "THE WILD", [1.0; 4]);
                ui.rect(
                    p0x - 130.0,
                    p0y + 162.0,
                    100.0,
                    10.0,
                    [0.12, 0.12, 0.12, 0.9],
                );
                ui.rect(
                    p0x - 130.0,
                    p0y + 162.0,
                    self.server.world.ire,
                    10.0,
                    tier_col,
                );
                ui.text_shadow(
                    p0x - 24.0,
                    p0y + 162.0,
                    1.5,
                    world::IRE_TIERS[tier],
                    tier_col,
                );
                // Armor column: head/chest/legs/feet.
                for (i, label) in ["H", "C", "L", "B", "*"].iter().enumerate() {
                    let r = self.armor_slot_rect(i);
                    Self::draw_slot(&self.reg, &mut ui, r, self.armor[i], false, self.hit(r));
                    if self.armor[i].is_none() {
                        ui.text_shadow(
                            r.0 + r.2 / 2.0 - 5.0,
                            r.1 + r.3 / 2.0 - 7.0,
                            2.0,
                            label,
                            [0.55, 0.55, 0.55, 0.8],
                        );
                    }
                }
                // Craft grid, arrow, result.
                let n2 = self.craft_size * self.craft_size;
                for i in 0..n2 {
                    let r = self.craft_slot_rect(i);
                    Self::draw_slot(
                        &self.reg,
                        &mut ui,
                        r,
                        self.craft_grid[i],
                        false,
                        self.hit(r),
                    );
                }
                let rr = self.result_slot_rect();
                ui.text_shadow(rr.0 - 34.0, rr.1 + 16.0, 2.5, "-", [1.0; 4]);
                ui.text_shadow(rr.0 - 24.0, rr.1 + 14.0, 2.5, ">", [1.0; 4]);
                let result =
                    crafting::match_recipe(&self.reg, &self.craft_grid[..n2], self.craft_size)
                        .map(|r| ItemStack::new(&self.reg, r.output, r.count));
                Self::draw_slot(&self.reg, &mut ui, rr, result, false, self.hit(rr));
                let _ = c0x;
                self.draw_browser(&mut ui);
                // Stack on the cursor.
                if let Some(s) = self.held_stack {
                    let (cx, cy) = self.ui_cursor;
                    let icon = self.reg.item(s.item).icon;
                    ui.tile(cx - 16.0, cy - 16.0, 32.0, 32.0, icon, [1.0; 4]);
                    if s.count > 1 {
                        let txt = format!("{}", s.count);
                        ui.text_shadow(cx + 6.0, cy + 4.0, 2.0, &txt, [1.0; 4]);
                    }
                }
            }
            Screen::Paused => {
                ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.6]);
                let title = "GAME PAUSED";
                let tw = UiBatch::text_width(4.0, title);
                ui.text_shadow((w - tw) / 2.0, h / 2.0 - 130.0, 4.0, title, [1.0; 4]);
                let mode = if self.creative {
                    "MODE: CREATIVE"
                } else {
                    "MODE: SURVIVAL"
                };
                let friends = match &self.host {
                    Some(h) => format!("FRIENDS: {} CONNECTED", h.guests.len()),
                    None if self.remote.is_some() => "CONNECTED AS GUEST".to_string(),
                    None => "OPEN TO FRIENDS".to_string(),
                };
                for (i, label) in [
                    "RESUME",
                    mode,
                    &friends,
                    "SETTINGS",
                    "SAVE AND QUIT TO TITLE",
                ]
                .iter()
                .enumerate()
                {
                    let r = self.menu_button_rect(i);
                    Self::draw_button(&mut ui, r, label, self.hit(r));
                }
                // Hosting: each guest gets a name row and a KICK button.
                for (row, (_, name)) in self.guest_rows().iter().enumerate() {
                    let r = self.kick_rect(row);
                    ui.text_shadow(
                        r.0,
                        r.1 - 16.0,
                        1.5,
                        &name.to_uppercase(),
                        [0.9, 0.9, 0.9, 1.0],
                    );
                    Self::draw_button(&mut ui, r, "KICK", self.hit(r));
                }
            }
            Screen::Dead => {
                ui.rect(0.0, 0.0, w, h, [0.5, 0.0, 0.0, 0.5]);
                let title = "YOU DIED";
                let tw = UiBatch::text_width(5.0, title);
                ui.text_shadow(
                    (w - tw) / 2.0,
                    h / 2.0 - 120.0,
                    5.0,
                    title,
                    [1.0, 0.85, 0.85, 1.0],
                );
                if self.killed_by_wild {
                    let sub = "RECLAIMED BY THE WILD";
                    let sw = UiBatch::text_width(2.0, sub);
                    ui.text_shadow(
                        (w - sw) / 2.0,
                        h / 2.0 - 60.0,
                        2.0,
                        sub,
                        [0.8, 0.95, 0.75, 1.0],
                    );
                }
                let r = self.menu_button_rect(0);
                let hover = self.hit(r);
                let bg = if hover {
                    [0.5, 0.5, 0.5, 0.95]
                } else {
                    [0.25, 0.25, 0.25, 0.95]
                };
                ui.rect(r.0, r.1, r.2, r.3, [0.1, 0.1, 0.1, 0.95]);
                ui.rect(r.0 + 2.0, r.1 + 2.0, r.2 - 4.0, r.3 - 4.0, bg);
                let lw = UiBatch::text_width(2.0, "RESPAWN");
                ui.text_shadow(
                    r.0 + (r.2 - lw) / 2.0,
                    r.1 + (r.3 - 14.0) / 2.0,
                    2.0,
                    "RESPAWN",
                    [1.0; 4],
                );
            }
        }
        self.ui = ui;
    }

    // ---------- Menu / inventory clicks ----------

    fn slot_get(&self, craft: bool, i: usize) -> Option<ItemStack> {
        if craft {
            self.craft_grid[i]
        } else {
            self.inventory.slots[i]
        }
    }

    fn slot_set(&mut self, craft: bool, i: usize, v: Option<ItemStack>) {
        if craft {
            self.craft_grid[i] = v;
        } else {
            self.inventory.slots[i] = v;
        }
    }

    fn inventory_click(&mut self, craft: bool, slot: usize, right: bool) {
        let cur = self.slot_get(craft, slot);
        let (new_slot, new_held) = inventory::click_stack(&self.reg, cur, self.held_stack, right);
        self.slot_set(craft, slot, new_slot);
        self.held_stack = new_held;
    }

    /// Click the craft result slot: take the output, consume ingredients.
    fn result_click(&mut self) {
        let reg = self.reg.clone();
        let n2 = self.craft_size * self.craft_size;
        let Some(recipe) = crafting::match_recipe(&reg, &self.craft_grid[..n2], self.craft_size)
        else {
            return;
        };
        let out = ItemStack::new(&reg, recipe.output, recipe.count);
        match self.held_stack {
            None => {
                self.held_stack = Some(out);
            }
            Some(h)
                if h.can_merge(&reg, &out) && h.count + out.count <= reg.item(h.item).max_stack =>
            {
                self.held_stack = Some(ItemStack {
                    count: h.count + out.count,
                    ..h
                });
            }
            _ => return, // held stack can't take the output
        }
        crafting::consume(&mut self.craft_grid[..n2]);
        self.sfx(Sfx::Craft);
        if self.scripts.wants("on_craft") {
            let name = reg.item(recipe.output).name.clone();
            self.scripts
                .dispatch(&self.server.world, "on_craft", (name,));
            self.apply_script_cmds();
        }
    }

    /// Furnace slot rects: 0 input, 1 fuel, 2 output (centered panel).
    /// Bloomery slots: 0-3 charge (top row), 4-7 fuel (bottom row).
    fn bloomery_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let (col, row) = (i % 4, i / 4);
        (
            w / 2.0 - 2.0 * (Self::SLOT + 10.0) + col as f32 * (Self::SLOT + 10.0) + 5.0,
            h / 2.0 - 250.0 + row as f32 * (Self::SLOT + 34.0),
            Self::SLOT,
            Self::SLOT,
        )
    }

    fn bloomery_light_rect(&self) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 + 2.0 * (Self::SLOT + 10.0) + 20.0,
            h / 2.0 - 230.0,
            110.0,
            36.0,
        )
    }

    fn bloomery_click(&mut self, pos: (i32, i32, i32), slot: usize, right: bool) {
        self.remote_container_notify(pos, slot, right);
        let reg = self.reg.clone();
        let Some(world::BlockEntity::Bloomery(b)) = self.server.world.block_entities.get_mut(&pos)
        else {
            return;
        };
        // Mirror of the host rule: sealed while firing, charge takes
        // the chain's ore, the bank takes its fuel; taking is free.
        if b.lit || slot >= 8 {
            return;
        }
        let chain = reg.bloomery.first().cloned();
        let want = chain.map(|c| if slot < 4 { c.charge } else { c.fuel });
        let s = if slot < 4 {
            &mut b.charge[slot]
        } else {
            &mut b.fuel[slot - 4]
        };
        if self.held_stack.is_none() || self.held_stack.map(|h| Some(h.item)) == Some(want) {
            let (ns, nh) = inventory::click_stack(&reg, *s, self.held_stack, right);
            *s = ns;
            self.held_stack = nh;
        }
    }

    /// The LIGHT action: needs an ember in hand or inventory, a valid
    /// shell, and a charge. Guests request; the host answers.
    fn light_bloomery_action(&mut self, pos: (i32, i32, i32)) {
        let reg = self.reg.clone();
        let Some(ember) = reg.item_id("base:ember") else {
            return;
        };
        let slot =
            (0..TOTAL_SLOTS).find(|&i| self.inventory.slots[i].is_some_and(|s| s.item == ember));
        let Some(slot) = slot else {
            self.toast("Lighting the stack takes a warden's ember.".to_string());
            return;
        };
        if let Some(rc) = &self.remote {
            self.inventory.take_one(slot);
            rc.client.send(&net::C2S::LightBloomery {
                x: pos.0,
                y: pos.1,
                z: pos.2,
            });
            return;
        }
        match self.server.world.light_bloomery(pos.0, pos.1, pos.2) {
            Ok(()) => {
                self.inventory.take_one(slot);
                self.sfx(Sfx::Bolt(0.8));
                self.toast("The stack takes the ember. Half a day of fire.".to_string());
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    fn furnace_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let (cx, cy) = (w / 2.0, h / 2.0 - 190.0);
        match i {
            0 => (cx - 120.0, cy - 46.0, Self::SLOT, Self::SLOT),
            1 => (cx - 120.0, cy + 34.0, Self::SLOT, Self::SLOT),
            _ => (cx + 50.0, cy - 6.0, Self::SLOT, Self::SLOT),
        }
    }

    #[allow(clippy::type_complexity)]
    fn furnace_view(
        &self,
        pos: (i32, i32, i32),
    ) -> (
        Option<ItemStack>,
        Option<ItemStack>,
        Option<ItemStack>,
        f32,
        f32,
    ) {
        match self.server.world.block_entities.get(&pos) {
            Some(world::BlockEntity::Furnace(f)) => {
                let time = f
                    .input
                    .and_then(|s| self.reg.smelt_for(s.item))
                    .map(|s| s.time)
                    .unwrap_or(8.0);
                let burn = if f.burn_total > 0.0 {
                    f.burn_left / f.burn_total
                } else {
                    0.0
                };
                (
                    f.input,
                    f.fuel,
                    f.output,
                    (f.progress / time).min(1.0),
                    burn,
                )
            }
            _ => (None, None, None, 0.0, 0.0),
        }
    }

    /// Chest slot rects: 9x3 grid centered above the inventory panel.
    fn chest_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let x0 = w / 2.0 - 4.5 * Self::SLOT;
        let y0 = h / 2.0 - 300.0;
        (
            x0 + (i % 9) as f32 * Self::SLOT,
            y0 + (i / 9) as f32 * Self::SLOT,
            Self::SLOT,
            Self::SLOT,
        )
    }

    /// Armor column: right of the storage grid — head, chest, legs, feet.
    fn armor_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let (x0, y0, _, _) = self.inv_slot_rect(HOTBAR_SLOTS);
        (
            x0 + 9.0 * Self::SLOT + 14.0,
            y0 + i as f32 * Self::SLOT,
            Self::SLOT,
            Self::SLOT,
        )
    }

    fn armor_click(&mut self, i: usize) {
        let reg = self.reg.clone();
        match (self.held_stack, self.armor[i]) {
            (Some(h), cur) => {
                // Matching piece in its slot; charms in the charm slot.
                let fits = if i == 4 {
                    reg.item(h.item).charm.is_some()
                } else {
                    reg.item(h.item).armor.map(|(s, _)| s as usize) == Some(i)
                };
                if fits {
                    self.armor[i] = Some(h);
                    self.held_stack = cur;
                }
            }
            (None, Some(_)) => {
                self.held_stack = self.armor[i].take();
            }
            (None, None) => {}
        }
    }

    /// Offering stone: three slots, centered.
    fn offering_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 + (i as f32 - 1.5) * (Self::SLOT + 10.0) + 5.0,
            h / 2.0 - 200.0,
            Self::SLOT,
            Self::SLOT,
        )
    }

    fn offering_click(&mut self, pos: (i32, i32, i32), slot: usize, right: bool) {
        self.remote_container_notify(pos, slot, right);
        let reg = self.reg.clone();
        let Some(world::BlockEntity::Offering(o)) = self.server.world.block_entities.get_mut(&pos)
        else {
            return;
        };
        let (new_slot, new_held) =
            inventory::click_stack(&reg, o.slots[slot], self.held_stack, right);
        o.slots[slot] = new_slot;
        self.held_stack = new_held;
    }

    /// Guests mirror container clicks to the host with the cursor stack
    /// riding along; the local mutation that follows is a prediction
    /// (same click_stack, same synced content) and the Container +
    /// HeldResult echo is the truth that reconciles it.
    fn remote_container_notify(&mut self, pos: (i32, i32, i32), slot: usize, right: bool) {
        let Some(r) = &self.remote else { return };
        let held = self.held_stack.map(|h| net::StackSnap {
            item: h.item.0, // same content: local ids match host ids
            count: h.count,
            durability: h.durability,
        });
        r.client.send(&net::C2S::ContainerClick {
            x: pos.0,
            y: pos.1,
            z: pos.2,
            slot: slot as u8,
            right,
            held,
        });
    }

    fn chest_click(&mut self, pos: (i32, i32, i32), slot: usize, right: bool) {
        self.remote_container_notify(pos, slot, right);
        let reg = self.reg.clone();
        let Some(world::BlockEntity::Chest(c)) = self.server.world.block_entities.get_mut(&pos)
        else {
            return;
        };
        let (new_slot, new_held) =
            inventory::click_stack(&reg, c.slots[slot], self.held_stack, right);
        c.slots[slot] = new_slot;
        self.held_stack = new_held;
    }

    fn furnace_click(&mut self, pos: (i32, i32, i32), slot: usize, right: bool) {
        self.remote_container_notify(pos, slot, right);
        let reg = self.reg.clone();
        let Some(world::BlockEntity::Furnace(f)) = self.server.world.block_entities.get_mut(&pos)
        else {
            return;
        };
        match slot {
            0 | 1 => {
                let cur = if slot == 0 { f.input } else { f.fuel };
                let (new_slot, new_held) =
                    inventory::click_stack(&reg, cur, self.held_stack, right);
                if slot == 0 {
                    if f.input.map(|s| s.item) != new_slot.map(|s| s.item) {
                        f.progress = 0.0;
                    }
                    f.input = new_slot;
                } else {
                    f.fuel = new_slot;
                }
                self.held_stack = new_held;
            }
            _ => {
                // Output: take-only, merging into the held stack.
                let Some(out) = f.output else { return };
                match self.held_stack {
                    None => {
                        self.held_stack = Some(out);
                        f.output = None;
                    }
                    Some(h)
                        if h.can_merge(&reg, &out)
                            && h.count + out.count <= reg.item(h.item).max_stack =>
                    {
                        self.held_stack = Some(ItemStack {
                            count: h.count + out.count,
                            ..h
                        });
                        f.output = None;
                    }
                    _ => {}
                }
            }
        }
    }

    const BCOLS: usize = 6;
    const BROWS: usize = 8;
    const BSLOT: f32 = 40.0;

    fn browser_origin(&self) -> (f32, f32) {
        (
            self.renderer.config.width as f32 - Self::BCOLS as f32 * Self::BSLOT - 20.0,
            96.0,
        )
    }

    fn browser_cell(&self, i: usize) -> (f32, f32, f32, f32) {
        let (x0, y0) = self.browser_origin();
        (
            x0 + (i % Self::BCOLS) as f32 * Self::BSLOT,
            y0 + (i / Self::BCOLS) as f32 * Self::BSLOT,
            Self::BSLOT,
            Self::BSLOT,
        )
    }

    fn browser_search_rect(&self) -> (f32, f32, f32, f32) {
        let (x0, y0) = self.browser_origin();
        (x0, y0 - 34.0, Self::BCOLS as f32 * Self::BSLOT, 26.0)
    }

    fn browser_nav_rect(&self, next: bool) -> (f32, f32, f32, f32) {
        let (x0, y0) = self.browser_origin();
        let y = y0 + Self::BROWS as f32 * Self::BSLOT + 8.0;
        if next {
            (x0 + Self::BCOLS as f32 * Self::BSLOT - 40.0, y, 40.0, 26.0)
        } else {
            (x0, y, 40.0, 26.0)
        }
    }

    fn draw_browser(&self, ui: &mut UiBatch) {
        let reg = &self.reg;
        let sr = self.browser_search_rect();
        ui.rect(sr.0, sr.1, sr.2, sr.3, [0.08, 0.08, 0.08, 0.95]);
        let caret = if self.search_focus && (self.time_abs * 2.0) as i32 % 2 == 0 {
            "_"
        } else {
            ""
        };
        ui.text_shadow(
            sr.0 + 6.0,
            sr.1 + 6.0,
            2.0,
            &format!("{}{caret}", self.search.to_uppercase()),
            [1.0; 4],
        );
        if self.search.is_empty() && !self.search_focus {
            ui.text_shadow(sr.0 + 6.0, sr.1 + 6.0, 2.0, "SEARCH", [0.5, 0.5, 0.5, 1.0]);
        }
        let items = browser_items(reg, &self.search);
        let per = Self::BCOLS * Self::BROWS;
        let pages = items.len().div_ceil(per).max(1);
        let page = self.browse_page.min(pages - 1);
        for (ci, item) in items.iter().skip(page * per).take(per).enumerate() {
            let r = self.browser_cell(ci);
            let hov = self.hit(r);
            ui.rect(
                r.0 + 1.0,
                r.1 + 1.0,
                r.2 - 2.0,
                r.3 - 2.0,
                if hov {
                    [0.4, 0.4, 0.4, 0.85]
                } else {
                    [0.16, 0.16, 0.16, 0.85]
                },
            );
            let icon = reg.item(*item).icon;
            ui.tile(r.0 + 5.0, r.1 + 5.0, 30.0, 30.0, icon, [1.0; 4]);
            if hov {
                ui.text_shadow(
                    r.0 - 120.0,
                    r.1 + 12.0,
                    1.5,
                    &reg.item(*item).label.to_uppercase(),
                    [1.0, 1.0, 0.7, 1.0],
                );
            }
        }
        for (next, lbl) in [(false, "<"), (true, ">")] {
            let r = self.browser_nav_rect(next);
            Self::draw_button(ui, r, lbl, self.hit(r));
        }
        let (x0, _) = self.browser_origin();
        let y = self.browser_nav_rect(false).1 + 5.0;
        ui.text_shadow(
            x0 + 90.0,
            y,
            2.0,
            &format!("{}/{pages}", page + 1),
            [1.0; 4],
        );

        // Recipe overlay.
        if let Some((item, uses)) = self.browse_view {
            let (px, py) = (40.0, 96.0);
            let pw = 380.0;
            ui.rect(px - 10.0, py - 40.0, pw, 460.0, [0.05, 0.05, 0.08, 0.96]);
            ui.text_shadow(
                px,
                py - 30.0,
                2.0,
                &reg.item(item).label.to_uppercase(),
                [1.0, 1.0, 0.6, 1.0],
            );
            for (ti, lbl) in ["RECIPES", "USES"].iter().enumerate() {
                let r = (px + 150.0 + ti as f32 * 90.0, py - 34.0, 84.0, 24.0);
                Self::draw_button(ui, r, lbl, (ti == 1) == uses);
            }
            let cycle = (self.time_abs / 0.8) as usize;
            let mut y = py + 8.0;
            let (recipes, smelts) = if uses {
                let (r, s, fuel) = reg.uses_of(item);
                if fuel {
                    ui.text_shadow(px, y, 2.0, "USABLE AS FURNACE FUEL", [1.0, 0.8, 0.4, 1.0]);
                    y += 26.0;
                }
                (r, s)
            } else {
                (reg.recipes_for(item), reg.smelts_for(item))
            };
            for r in recipes.iter().take(3) {
                for cy in 0..r.h {
                    for cx in 0..r.w {
                        let cell = (px + cx as f32 * 38.0, y + cy as f32 * 38.0, 36.0, 36.0);
                        ui.rect(cell.0, cell.1, cell.2, cell.3, [0.18, 0.18, 0.18, 0.9]);
                        if let Some(ing) = &r.pattern[cy * r.w + cx] {
                            let show = match ing {
                                crate::registry::Ingredient::One(i) => *i,
                                crate::registry::Ingredient::Any(l) => l[cycle % l.len()],
                            };
                            let ic = reg.item(show).icon;
                            ui.tile(cell.0 + 3.0, cell.1 + 3.0, 30.0, 30.0, ic, [1.0; 4]);
                        }
                    }
                }
                let oy = y + (r.h as f32 * 38.0 - 36.0) / 2.0;
                ui.text_shadow(px + 126.0, oy + 12.0, 2.5, ">", [1.0; 4]);
                ui.rect(px + 150.0, oy, 36.0, 36.0, [0.22, 0.22, 0.22, 0.9]);
                let oc = reg.item(r.output).icon;
                ui.tile(px + 153.0, oy + 3.0, 30.0, 30.0, oc, [1.0; 4]);
                if r.count > 1 {
                    ui.text_shadow(
                        px + 172.0,
                        oy + 22.0,
                        2.0,
                        &format!("{}", r.count),
                        [1.0; 4],
                    );
                }
                y += r.h as f32 * 38.0 + 14.0;
            }
            for s in smelts.iter().take(2) {
                ui.text_shadow(px, y + 10.0, 2.0, "SMELT", [1.0, 0.6, 0.2, 1.0]);
                let show = match &s.input {
                    crate::registry::Ingredient::One(i) => *i,
                    crate::registry::Ingredient::Any(l) => l[cycle % l.len()],
                };
                for (sx, it2) in [(90.0, show), (170.0, s.output)] {
                    ui.rect(px + sx, y, 36.0, 36.0, [0.2, 0.2, 0.2, 0.9]);
                    let ic = reg.item(it2).icon;
                    ui.tile(px + sx + 3.0, y + 3.0, 30.0, 30.0, ic, [1.0; 4]);
                }
                ui.text_shadow(px + 140.0, y + 12.0, 2.5, ">", [1.0; 4]);
                y += 50.0;
            }
            if recipes.is_empty() && smelts.is_empty() {
                ui.text_shadow(px, y, 2.0, "NOTHING HERE", [0.6, 0.6, 0.6, 1.0]);
            }
            if !self.browse_back.is_empty() {
                let r = (px, py + 370.0, 84.0, 24.0);
                Self::draw_button(ui, r, "BACK", self.hit(r));
            }
        }
    }

    /// Returns true if the click was handled by the browser.
    fn browser_click(&mut self, right: bool) -> bool {
        if self.hit(self.browser_search_rect()) {
            self.search_focus = true;
            return true;
        }
        self.search_focus = false;
        for (next, _) in [(false, ()), (true, ())] {
            if self.hit(self.browser_nav_rect(next)) {
                let items = browser_items(&self.reg, &self.search);
                let pages = items.len().div_ceil(Self::BCOLS * Self::BROWS).max(1);
                self.browse_page = if next {
                    (self.browse_page + 1).min(pages - 1)
                } else {
                    self.browse_page.saturating_sub(1)
                };
                return true;
            }
        }
        if let Some((item, uses)) = self.browse_view {
            let (px, py) = (40.0, 96.0);
            for ti in 0..2 {
                let r = (px + 150.0 + ti as f32 * 90.0, py - 34.0, 84.0, 24.0);
                if self.hit(r) {
                    self.browse_view = Some((item, ti == 1));
                    return true;
                }
            }
            if !self.browse_back.is_empty() {
                let r = (px, py + 370.0, 84.0, 24.0);
                if self.hit(r) {
                    self.browse_view = self.browse_back.pop();
                    return true;
                }
            }
            let _ = uses;
            if self.hit((px - 10.0, py - 40.0, 380.0, 460.0)) {
                return true; // swallow clicks inside the panel
            }
            self.browse_view = None;
            return true;
        }
        let items = browser_items(&self.reg, &self.search);
        let per = Self::BCOLS * Self::BROWS;
        let page = self.browse_page.min(items.len().div_ceil(per).max(1) - 1);
        for (ci, item) in items.iter().skip(page * per).take(per).enumerate() {
            if self.hit(self.browser_cell(ci)) {
                if self.creative {
                    let reg = self.reg.clone();
                    let n = if right { 1 } else { reg.item(*item).max_stack };
                    self.held_stack = Some(ItemStack::new(&reg, *item, n));
                } else {
                    self.browse_back.clear();
                    self.browse_view = Some((*item, false));
                }
                return true;
            }
        }
        false
    }

    fn menu_click(&mut self, event_loop: &ActiveEventLoop, right: bool) {
        if std::env::var("WILDFORGE_DEBUG").is_ok() {
            eprintln!("menu_click at {:?} right={right}", self.ui_cursor);
        }
        match self.screen {
            Screen::Inventory => {
                if self.browser_click(right) {
                    return;
                }
                for i in 0..5 {
                    if self.hit(self.armor_slot_rect(i)) {
                        self.armor_click(i);
                        return;
                    }
                }
                for i in 0..TOTAL_SLOTS {
                    if self.hit(self.inv_slot_rect(i)) {
                        self.inventory_click(false, i, right);
                        return;
                    }
                }
                for i in 0..self.craft_size * self.craft_size {
                    if self.hit(self.craft_slot_rect(i)) {
                        self.inventory_click(true, i, right);
                        return;
                    }
                }
                if self.hit(self.result_slot_rect()) {
                    self.result_click();
                }
            }
            Screen::Paused => {
                if self.hit(self.menu_button_rect(0)) {
                    self.sfx(Sfx::Click);
                    self.set_screen(Screen::Playing);
                } else if self.hit(self.menu_button_rect(1)) {
                    self.sfx(Sfx::Click);
                    self.creative = !self.creative;
                    self.flying = false;
                    let mode = if self.creative {
                        "creative"
                    } else {
                        "survival"
                    };
                    world::write_world_meta_full(
                        &self.server.world.save_dir_for_saving(),
                        self.server.world.seed,
                        mode,
                        self.server.world.ire,
                        self.server.world.day,
                        self.server.world.weather,
                    );
                    if self.scripts.wants("on_mode_change") {
                        self.scripts.dispatch(
                            &self.server.world,
                            "on_mode_change",
                            (mode.to_string(),),
                        );
                        self.apply_script_cmds();
                    }
                } else if self.hit(self.menu_button_rect(2)) {
                    self.sfx(Sfx::Click);
                    if self.host.is_none() && self.remote.is_none() {
                        let wname = self
                            .server
                            .world
                            .save_dir_for_saving()
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "world".into());
                        match mp::HostSession::start(wname) {
                            Ok(sess) => {
                                self.server.world.log_edits = true;
                                self.toast(format!(
                                    "Open to friends on port {} (LAN + direct IP).",
                                    sess.net.port
                                ));
                                self.host = Some(sess);
                            }
                            Err(e) => self.toast(format!("Could not host: {e}")),
                        }
                    }
                } else if self.hit(self.menu_button_rect(3)) {
                    self.sfx(Sfx::Click);
                    self.settings_from_pause = true;
                    self.set_screen(Screen::Settings);
                } else if self.hit(self.menu_button_rect(4)) {
                    self.sfx(Sfx::Click);
                    self.quit_to_title();
                } else {
                    for (row, (id, _)) in self.guest_rows().iter().enumerate() {
                        if self.hit(self.kick_rect(row)) {
                            self.sfx(Sfx::Click);
                            if let Some(h) = &mut self.host
                                && let Some(name) = h.kick_guest(*id)
                            {
                                self.toast(format!("{name} was kicked."));
                            }
                            break;
                        }
                    }
                }
            }
            Screen::Dead => {
                if self.hit(self.menu_button_rect(0)) {
                    self.sfx(Sfx::Click);
                    self.respawn();
                }
            }
            Screen::Title => {
                for i in 0..self.worlds.len().min(6) {
                    if self.hit(self.title_play_rect(i)) {
                        self.sfx(Sfx::Click);
                        let name = self.worlds[i].0.clone();
                        self.start_world(&name);
                        return;
                    }
                    if self.hit(self.title_delete_rect(i)) {
                        self.sfx(Sfx::Click);
                        self.pending_delete = Some(i);
                        self.set_screen(Screen::ConfirmDelete);
                        return;
                    }
                }
                if self.hit(self.title_action_rect(0)) {
                    self.sfx(Sfx::Click);
                    self.new_world_mode("survival");
                } else if self.hit(self.title_action_rect(1)) {
                    self.sfx(Sfx::Click);
                    self.new_world_mode("creative");
                } else if self.hit(self.title_action_rect(2)) {
                    self.sfx(Sfx::Click);
                    self.discovery = net::Discovery::start().ok();
                    self.join_status.clear();
                    self.set_screen(Screen::Join);
                } else if self.hit(self.title_action_rect(3)) {
                    self.sfx(Sfx::Click);
                    self.set_screen(Screen::Mods);
                } else if self.hit(self.title_action_rect(4)) {
                    self.sfx(Sfx::Click);
                    self.packs = atlas::discover_packs();
                    self.set_screen(Screen::Packs);
                } else if self.hit(self.title_action_rect(5)) {
                    self.sfx(Sfx::Click);
                    self.settings_from_pause = false;
                    self.set_screen(Screen::Settings);
                } else if self.hit(self.title_action_rect(6)) {
                    event_loop.exit();
                }
            }
            Screen::Mods => {
                if self.hit(self.menu_button_rect(4)) {
                    self.sfx(Sfx::Click);
                    self.set_screen(Screen::Title);
                }
            }
            Screen::Packs => {
                if self.hit(self.pack_back_rect()) {
                    self.sfx(Sfx::Click);
                    self.set_screen(Screen::Title);
                    return;
                }
                for i in 0..=self.packs.len().min(7) {
                    if self.hit(self.pack_row_rect(i)) {
                        let sel = if i == 0 {
                            String::new()
                        } else {
                            self.packs[i - 1].id.clone()
                        };
                        self.sfx(Sfx::Click);
                        if sel != self.active_pack_id() {
                            self.pack_override = None;
                            self.config.pack = sel;
                            self.apply_pack();
                        }
                        return;
                    }
                }
            }
            Screen::Furnace(pos) => {
                if self.browser_click(right) {
                    return;
                }
                for i in 0..3 {
                    if self.hit(self.furnace_slot_rect(i)) {
                        self.furnace_click(pos, i, right);
                        return;
                    }
                }
                for i in 0..TOTAL_SLOTS {
                    if self.hit(self.inv_slot_rect(i)) {
                        self.inventory_click(false, i, right);
                        return;
                    }
                }
            }
            Screen::Bloomery(pos) => {
                if self.browser_click(right) {
                    return;
                }
                if self.hit(self.bloomery_light_rect()) {
                    self.sfx(Sfx::Click);
                    self.light_bloomery_action(pos);
                    return;
                }
                for i in 0..8 {
                    if self.hit(self.bloomery_slot_rect(i)) {
                        self.bloomery_click(pos, i, right);
                        return;
                    }
                }
                for i in 0..TOTAL_SLOTS {
                    if self.hit(self.inv_slot_rect(i)) {
                        self.inventory_click(false, i, right);
                        return;
                    }
                }
            }
            Screen::Chest(pos) => {
                if self.browser_click(right) {
                    return;
                }
                for i in 0..world::CHEST_SLOTS {
                    if self.hit(self.chest_slot_rect(i)) {
                        self.chest_click(pos, i, right);
                        return;
                    }
                }
                for i in 0..TOTAL_SLOTS {
                    if self.hit(self.inv_slot_rect(i)) {
                        self.inventory_click(false, i, right);
                        return;
                    }
                }
            }
            Screen::Offering(pos) => {
                if self.browser_click(right) {
                    return;
                }
                for i in 0..3 {
                    if self.hit(self.offering_slot_rect(i)) {
                        self.offering_click(pos, i, right);
                        return;
                    }
                }
                for i in 0..TOTAL_SLOTS {
                    if self.hit(self.inv_slot_rect(i)) {
                        self.inventory_click(false, i, right);
                        return;
                    }
                }
            }
            Screen::Join => {
                if self.hit(self.pack_back_rect()) {
                    self.sfx(Sfx::Click);
                    self.discovery = None;
                    self.set_screen(Screen::Title);
                    return;
                }
                let w = self.renderer.config.width as f32;
                let h = self.renderer.config.height as f32;
                let found: Vec<(std::net::SocketAddr, String)> = self
                    .discovery
                    .as_ref()
                    .map(|d| d.found.clone())
                    .unwrap_or_default();
                for (i, (addr, _)) in found.iter().take(5).enumerate() {
                    let r = (w / 2.0 - 220.0, h * 0.20 + i as f32 * 56.0, 440.0, 42.0);
                    if self.hit(r) {
                        self.sfx(Sfx::Click);
                        self.join_status = "CONNECTING...".to_string();
                        self.join_server(*addr);
                        return;
                    }
                }
                let y = h * 0.20 + found.len().clamp(1, 5) as f32 * 56.0 + 26.0;
                let cr = (w / 2.0 + 240.0, y - 6.0, 160.0, 34.0);
                if self.hit(cr) {
                    self.sfx(Sfx::Click);
                    let text = self.join_ip.trim().to_string();
                    let addr = if text.contains(':') {
                        text.parse().ok()
                    } else {
                        format!("{text}:{}", net::GAME_PORT).parse().ok()
                    };
                    match addr {
                        Some(a) => {
                            self.join_status = "CONNECTING...".to_string();
                            self.join_server(a);
                        }
                        None => self.join_status = "BAD ADDRESS".to_string(),
                    }
                }
            }
            Screen::Settings => {
                for i in 0..Self::SLIDERS.len() {
                    let (bx, by, bw, bh) = self.slider_bar_rect(i);
                    let (cx, cy) = self.ui_cursor;
                    if cx >= bx && cx < bx + bw && cy >= by && cy < by + bh {
                        self.dragging_slider = Some(i);
                        self.set_slider(i, (cx - bx - 2.0) / (bw - 4.0));
                        return;
                    }
                }
                if self.hit(self.settings_back_rect()) {
                    self.sfx(Sfx::Click);
                    self.config.save();
                    self.set_screen(if self.settings_from_pause {
                        Screen::Paused
                    } else {
                        Screen::Title
                    });
                }
            }
            Screen::ConfirmDelete => {
                if self.hit(self.menu_button_rect(0)) {
                    self.sfx(Sfx::Click);
                    if let Some(i) = self.pending_delete.take() {
                        if let Some((name, _)) = self.worlds.get(i) {
                            let _ = std::fs::remove_dir_all(PathBuf::from("saves").join(name));
                        }
                        self.refresh_worlds();
                    }
                    self.set_screen(Screen::Title);
                } else if self.hit(self.menu_button_rect(1)) {
                    self.sfx(Sfx::Click);
                    self.pending_delete = None;
                    self.set_screen(Screen::Title);
                }
            }
            Screen::Playing => {}
        }
    }

    fn key(&mut self, code: KeyCode, pressed: bool, _event_loop: &ActiveEventLoop) {
        match code {
            KeyCode::KeyW | KeyCode::ArrowUp => self.keys.w = pressed,
            KeyCode::KeyA | KeyCode::ArrowLeft => self.keys.a = pressed,
            KeyCode::KeyS | KeyCode::ArrowDown => self.keys.s = pressed,
            KeyCode::KeyD | KeyCode::ArrowRight => self.keys.d = pressed,
            KeyCode::Space => {
                if pressed && self.creative && self.screen == Screen::Playing {
                    if self.time_abs - self.last_space < 0.3 {
                        self.flying = !self.flying;
                        self.player.vel = Vec3::ZERO;
                    }
                    self.last_space = self.time_abs;
                }
                self.keys.space = pressed;
            }
            KeyCode::ControlLeft | KeyCode::ControlRight => self.keys.sprint = pressed,
            KeyCode::Escape if pressed => match self.screen {
                Screen::Playing => self.set_screen(Screen::Paused),
                Screen::Inventory
                | Screen::Furnace(_)
                | Screen::Chest(_)
                | Screen::Offering(_)
                | Screen::Bloomery(_) => self.set_screen(Screen::Playing),
                Screen::Paused => self.set_screen(Screen::Playing),
                Screen::Settings => {
                    self.config.save();
                    self.set_screen(if self.settings_from_pause {
                        Screen::Paused
                    } else {
                        Screen::Title
                    });
                }
                Screen::ConfirmDelete => {
                    self.pending_delete = None;
                    self.set_screen(Screen::Title);
                }
                Screen::Mods | Screen::Packs => self.set_screen(Screen::Title),
                Screen::Join => {
                    self.discovery = None;
                    self.set_screen(Screen::Title);
                }
                Screen::Title | Screen::Dead => {}
            },
            KeyCode::KeyE if pressed && self.in_world => match self.screen {
                Screen::Playing => {
                    self.craft_size = 2;
                    self.set_screen(Screen::Inventory);
                }
                Screen::Inventory
                | Screen::Furnace(_)
                | Screen::Chest(_)
                | Screen::Offering(_)
                | Screen::Bloomery(_) => self.set_screen(Screen::Playing),
                _ => {}
            },
            KeyCode::KeyT
                if pressed
                    && self.screen == Screen::Playing
                    && (self.host.is_some() || self.remote.is_some()) =>
            {
                self.chat_open = true;
                self.chat_text.clear();
            }
            KeyCode::F5 if pressed => {
                self.reload_mods(true);
            }
            KeyCode::F2 if pressed => {
                let name = format!(
                    "screenshot-{}.ppm",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0)
                );
                self.renderer.pending_screenshot = Some(name);
            }
            KeyCode::F11 if pressed => {
                let fs = self.window.fullscreen();
                self.window.set_fullscreen(if fs.is_some() {
                    None
                } else {
                    Some(Fullscreen::Borderless(None))
                });
            }
            _ => {
                if pressed && self.screen == Screen::Playing {
                    let digit = match code {
                        KeyCode::Digit1 => Some(0),
                        KeyCode::Digit2 => Some(1),
                        KeyCode::Digit3 => Some(2),
                        KeyCode::Digit4 => Some(3),
                        KeyCode::Digit5 => Some(4),
                        KeyCode::Digit6 => Some(5),
                        KeyCode::Digit7 => Some(6),
                        KeyCode::Digit8 => Some(7),
                        KeyCode::Digit9 => Some(8),
                        _ => None,
                    };
                    if let Some(d) = digit {
                        self.hotbar_sel = d;
                    }
                }
            }
        }
    }
}

#[derive(Default)]
struct App {
    game: Option<Game>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.game.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Wildforge — loading world…")
                        .with_inner_size(LogicalSize::new(1280, 720)),
                )
                .expect("create window"),
        );
        let mut game = Game::new(window);
        // Headless/dev: WILDFORGE_WORLD=name skips the title screen.
        if let Ok(name) = std::env::var("WILDFORGE_WORLD") {
            game.start_world(&name);
        }
        self.game = Some(game);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => {
                if game.in_world {
                    game.save_player();
                    game.server.world.settle_falling();
                    game.server.world.save_modified();
                }
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                game.renderer.resize(size.width, size.height);
                game.camera.aspect = size.width as f32 / size.height.max(1) as f32;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // Join-screen IP entry.
                if game.screen == Screen::Join && event.state.is_pressed() {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Backspace) => {
                            game.join_ip.pop();
                        }
                        _ => {
                            if let Some(t) = &event.text {
                                for ch in t.chars() {
                                    if (ch.is_ascii_alphanumeric() || ".:".contains(ch))
                                        && game.join_ip.len() < 40
                                    {
                                        game.join_ip.push(ch);
                                    }
                                }
                            }
                        }
                    }
                    // Esc still handled below for leaving the screen.
                    if !matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape)) {
                        return;
                    }
                }
                // Chat entry (multiplayer).
                if game.chat_open && event.state.is_pressed() {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Escape) => {
                            game.chat_open = false;
                            game.chat_text.clear();
                        }
                        PhysicalKey::Code(KeyCode::Enter) => {
                            let msg: String = game.chat_text.trim().chars().take(200).collect();
                            game.chat_open = false;
                            game.chat_text.clear();
                            if !msg.is_empty() {
                                let me = whoami();
                                if let Some(r) = &game.remote {
                                    r.client.send(&net::C2S::Chat(msg.clone()));
                                } else if let Some(h) = &game.host {
                                    h.net.broadcast(&net::S2C::Chat {
                                        from: me.clone(),
                                        msg: msg.clone(),
                                    });
                                }
                                game.toast(format!("{me}: {msg}"));
                            }
                        }
                        PhysicalKey::Code(KeyCode::Backspace) => {
                            game.chat_text.pop();
                        }
                        _ => {
                            if let Some(t) = &event.text {
                                for ch in t.chars() {
                                    if !ch.is_control() && game.chat_text.len() < 200 {
                                        game.chat_text.push(ch);
                                    }
                                }
                            }
                        }
                    }
                    return;
                }
                let searchable = matches!(
                    game.screen,
                    Screen::Inventory
                        | Screen::Furnace(_)
                        | Screen::Chest(_)
                        | Screen::Offering(_)
                        | Screen::Bloomery(_)
                );
                if game.search_focus && searchable && event.state.is_pressed() {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Backspace) => {
                            game.search.pop();
                            game.browse_page = 0;
                        }
                        PhysicalKey::Code(KeyCode::Escape) | PhysicalKey::Code(KeyCode::Enter) => {
                            game.search_focus = false;
                        }
                        _ => {
                            if let Some(t) = &event.text {
                                for ch in t.chars() {
                                    if (ch.is_ascii_alphanumeric()
                                        || ch == ' '
                                        || ch == ':'
                                        || ch == '_')
                                        && game.search.len() < 24
                                    {
                                        game.search.push(ch);
                                        game.browse_page = 0;
                                    }
                                }
                            }
                        }
                    }
                    return;
                }
                if let PhysicalKey::Code(code) = event.physical_key {
                    game.key(code, event.state.is_pressed(), event_loop);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                if !pressed {
                    game.dragging_slider = None;
                }
                // Menu screens take clicks directly.
                if game.screen != Screen::Playing {
                    if pressed {
                        match button {
                            MouseButton::Left => game.menu_click(event_loop, false),
                            MouseButton::Right => game.menu_click(event_loop, true),
                            _ => {}
                        }
                    }
                    return;
                }
                if !game.mouse_captured {
                    if pressed {
                        game.capture_mouse(true);
                    }
                    return;
                }
                match button {
                    MouseButton::Left => {
                        game.left_held = pressed;
                        if pressed {
                            game.swing = 1.0;
                        } else {
                            game.breaking = None;
                        }
                    }
                    MouseButton::Right => {
                        game.right_held = pressed;
                        if pressed {
                            game.action_cooldown = 0.0;
                            game.swing = 1.0;
                        }
                    }
                    MouseButton::Middle if pressed => {
                        if let Some(h) = raycast::raycast(
                            &game.server.world,
                            game.camera.pos,
                            game.camera.forward(),
                            REACH,
                        ) {
                            let b = game.server.world.get_block(h.block.0, h.block.1, h.block.2);
                            let reg = game.reg.clone();
                            let found = game.inventory.slots[..HOTBAR_SLOTS]
                                .iter()
                                .position(|s| s.map(|s| reg.item(s.item).places) == Some(Some(b)));
                            if let Some(i) = found {
                                game.hotbar_sel = i;
                            }
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // WSLg can synthesize a scroll event as the window opens.
                if game.total_frames < 30 || game.screen != Screen::Playing {
                    return;
                }
                // Some stacks fire many small wheel events per physical notch;
                // accumulate and step one hotbar slot per whole notch.
                game.scroll_accum += match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 120.0,
                };
                // One slot per notch, rate-limited: platforms like WSLg fire
                // multiple events per physical notch.
                let steps = game.scroll_accum.trunc() as i32;
                if steps != 0 {
                    if game.scroll_cooldown <= 0.0 {
                        let n = HOTBAR_SLOTS as i32;
                        let sel = (game.hotbar_sel as i32 - steps.signum()).rem_euclid(n);
                        game.hotbar_sel = sel as usize;
                        game.scroll_cooldown = 0.15;
                    }
                    game.scroll_accum = 0.0;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                game.ui_cursor = (position.x as f32, position.y as f32);
                if let Some(i) = game.dragging_slider {
                    let (bx, _, bw, _) = game.slider_bar_rect(i);
                    game.set_slider(i, (position.x as f32 - bx - 2.0) / (bw - 4.0));
                }
                if game.mouse_captured && !game.raw_look && game.screen == Screen::Playing {
                    game.cursor_look(position);
                }
            }
            // Crossing the window boundary teleports the cursor; never treat
            // that jump as look motion.
            WindowEvent::CursorEntered { .. } | WindowEvent::CursorLeft { .. } => {
                game.last_cursor = None;
            }
            WindowEvent::Focused(false) => {
                if game.screen == Screen::Playing {
                    game.capture_mouse(false);
                }
                game.keys = KeysDown::default();
                game.left_held = false;
                game.right_held = false;
                game.breaking = None;
            }
            WindowEvent::RedrawRequested => {
                game.update();
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event
            && let Some(game) = self.game.as_mut()
            && game.mouse_captured
            && game.raw_look
            && game.screen == Screen::Playing
        {
            game.camera.turn(dx as f32, dy as f32);
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(game) = self.game.as_ref() {
            game.window.request_redraw();
        }
    }
}

/// Headless dedicated host: same binary, no window. `--server <world>`.
fn run_headless_server(world_name: &str) {
    let reg = Arc::new(registry::load(std::path::Path::new("mods")));
    let world = World::load_or_create(PathBuf::from("saves").join(world_name), reg.clone());
    let mut sim = server::Server::new(world, 0.3, 0xd5ed);
    sim.world.log_edits = true;
    let mut sess = match mp::HostSession::start(world_name.to_string()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("server: could not bind: {e}");
            std::process::exit(1);
        }
    };
    eprintln!(
        "wildforge --server \"{world_name}\": listening on port {} (LAN beacon on)",
        sess.net.port
    );
    let mut last = Instant::now();
    let mut save_timer = 0.0f32;
    loop {
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.25);
        last = now;
        for f in sess.pump(&mut sim, None, dt) {
            match f {
                mp::HostFx::Joined(n) => eprintln!("server: {n} joined"),
                mp::HostFx::Left(n) => eprintln!("server: {n} left"),
                mp::HostFx::Chat { from, msg } => eprintln!("<{from}> {msg}"),
                mp::HostFx::AllSlept => eprintln!("server: the camp sleeps to dawn"),
            }
        }
        let players = sess.player_ctxs(None);
        let mut evs = Vec::new();
        sim.advance(dt, &players, &mut evs);
        for ev in evs {
            if let server::SimEvent::PlayerHit { who, dmg, from } = ev {
                let ids: Vec<u32> = sess.guests.keys().copied().collect();
                if let Some(gid) = ids.get(who) {
                    sess.net.send(*gid, &net::S2C::Hit { dmg, from });
                }
            }
        }
        save_timer += dt;
        if save_timer >= 300.0 {
            save_timer = 0.0;
            sim.world.settle_falling();
            sim.world.save_modified();
            eprintln!("server: world saved");
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--server") {
        let world = args
            .get(i + 1)
            .cloned()
            .unwrap_or_else(|| "world1".to_string());
        run_headless_server(&world);
        return;
    }
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
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("run event loop");
}
