//! Windowed client construction. Content and renderer creation stay ordered.

use std::{path::PathBuf, sync::Arc, time::Instant};
use glam::Vec3;
use winit::window::Window;
use super::{Game, Screen, InputState, UiState, SurvivalState, InteractionState,
    PresentationState, ContentRuntime, MultiplayerState, combat, world_loading,
    content_tree_stamp, script_mod_dirs};
use crate::{atlas, bounce, config, identity, net, renderer, script, server, style, visual_capture};
use crate::audio::Audio;
use crate::camera::Camera;
use crate::config::Config;
use crate::inventory::Inventory;
use crate::physics::{EYE_HEIGHT, Player};
use crate::registry;
use crate::ui::UiBatch;
use crate::world::World;

impl Game {
    pub(super) fn new(window: Arc<Window>) -> Game {
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
            input: InputState::new(),
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

}
