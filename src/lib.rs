//! Wildforge — a Minecraft-alpha-style voxel game.
//!
//! Controls: WASD move, mouse look, Space jump, Ctrl sprint,
//! hold left click to mine, right click place, middle click pick block,
//! 1-9 / scroll wheel select hotbar slot, E inventory, Esc pause,
//! F2 screenshot, F11 fullscreen.

mod agent;
mod atlas;
mod audio;
mod camera;
mod chunk;
mod config;
mod crafting;
mod dedicated;
mod edifice;
mod entity;
mod game;
mod identity;
mod inventory;
mod lights;
mod materials;
mod mesher;
mod mobs;
mod mp;
mod net;
mod particles;
mod persist;
mod physics;
pub mod planet;
pub mod planet_atlas;
mod raycast;
mod registry;
mod renderer;
mod script;
mod server;
mod sky;
mod style;
#[cfg(test)]
mod tests;
mod ui;
mod world;
mod worldgen;

#[cfg(test)]
pub(crate) use game::{browser_items, content_tree_stamp_of, next_world_name, reduced_damage};

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

/// Run Wildforge using process arguments and the platform event loop.
pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|arg| arg == "--material-audit") {
        let Some(world) = args.get(i + 1).map(PathBuf::from) else {
            eprintln!("usage: wildforge --material-audit <world>");
            std::process::exit(2);
        };
        match materials::audit_world(&world) {
            Ok(audit) => {
                print!("{}", audit.render());
                if !audit.is_balanced() || !audit.is_qualified() {
                    std::process::exit(1);
                }
            }
            Err(error) => {
                eprintln!("material audit failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(i) = args.iter().position(|arg| arg == "--water-audit") {
        let Some(world) = args.get(i + 1).map(PathBuf::from) else {
            eprintln!("usage: wildforge --water-audit <world>");
            std::process::exit(2);
        };
        match planet_atlas::PlanetAtlas::load(&world) {
            Ok(atlas) => {
                print!("{}", atlas.water_audit_text());
                let audit = atlas.water_audit();
                if audit.unexplained_water_delta_hu != 0 || audit.unexplained_salt_delta != 0 {
                    std::process::exit(1);
                }
            }
            Err(error) => {
                eprintln!("water audit failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--generate-atlas") {
        let Some(seed) = args.get(i + 1).and_then(|value| value.parse::<u32>().ok()) else {
            eprintln!("usage: wildforge --generate-atlas <seed> --output <directory>");
            std::process::exit(2);
        };
        let Some(output) = args
            .iter()
            .position(|arg| arg == "--output")
            .and_then(|index| args.get(index + 1))
            .map(PathBuf::from)
        else {
            eprintln!("usage: wildforge --generate-atlas <seed> --output <directory>");
            std::process::exit(2);
        };
        let content_hash = planet_atlas::genesis_content_hash(std::path::Path::new("mods"));
        let cancel = planet_atlas::CancellationToken::default();
        eprintln!("atlas: seed {seed}, output {}", output.display());
        let atlas = match planet_atlas::PlanetAtlas::generate(
            seed,
            content_hash,
            planet_atlas::AtlasConfig::production(),
            &cancel,
            |progress| {
                eprintln!(
                    "atlas: [{}/{}] {}",
                    progress.completed_stages + 1,
                    progress.total_stages,
                    progress.stage.label()
                );
            },
        ) {
            Ok(atlas) => atlas,
            Err(error) => {
                eprintln!("atlas: generation failed: {error}");
                std::process::exit(1);
            }
        };
        if let Err(error) = atlas
            .write_new(&output)
            .and_then(|_| planet_atlas::export_diagnostics(&atlas, &output).map(|_| ()))
        {
            eprintln!("atlas: export failed: {error}");
            std::process::exit(1);
        }
        eprintln!("atlas: complete");
        return;
    }
    if let Some(i) = args.iter().position(|arg| arg == "--validate-entry") {
        let Some(world_arg) = args.get(i + 1).map(PathBuf::from) else {
            eprintln!("usage: wildforge --validate-entry <world>");
            std::process::exit(2);
        };
        let world_path = if world_arg.components().count() == 1 {
            PathBuf::from("saves").join(world_arg)
        } else {
            world_arg
        };
        let reg = Arc::new(registry::load(std::path::Path::new("mods")));
        let mut world = match World::load_or_create(world_path.clone(), reg) {
            Ok(world) => world,
            Err(error) => {
                eprintln!("entry validation failed: {error}");
                std::process::exit(1);
            }
        };
        let spawn = match world.prepare_common_spawn(|stage, completed, total| {
            eprintln!("entry: {stage} {completed}/{total}");
        }) {
            Ok(spawn) => spawn,
            Err(error) => {
                eprintln!("entry validation failed: {error}");
                std::process::exit(1);
            }
        };
        let save = world.save_modified();
        if !save.is_ok() {
            eprintln!("entry validation save failed: {}", save.summary());
            std::process::exit(1);
        }
        let atlas = match planet_atlas::PlanetAtlas::load(&world_path) {
            Ok(atlas) => atlas,
            Err(error) => {
                eprintln!("entry validation atlas reload failed: {error}");
                std::process::exit(1);
            }
        };
        let water = atlas.water_audit();
        let material = match materials::audit_world(&world_path) {
            Ok(audit) => audit,
            Err(error) => {
                eprintln!("entry validation material audit failed: {error}");
                std::process::exit(1);
            }
        };
        println!(
            "entry qualified at {} {:.1},{:.1},{:.1}; {}",
            spawn.face().name(),
            spawn.u(),
            spawn.y,
            spawn.v(),
            save.summary()
        );
        println!(
            "water delta: {} HU; salt delta: {}; material: {}",
            water.unexplained_water_delta_hu,
            water.unexplained_salt_delta,
            if material.is_balanced() && material.is_qualified() {
                "balanced and qualified"
            } else {
                "FAILED"
            }
        );
        if water.unexplained_water_delta_hu != 0
            || water.unexplained_salt_delta != 0
            || !material.is_balanced()
            || !material.is_qualified()
        {
            std::process::exit(1);
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--server") {
        let world = args
            .get(i + 1)
            .cloned()
            .unwrap_or_else(|| "world1".to_string());
        dedicated::run_headless_server(&world);
        return;
    }
    // An agent is a guest, not a god: same protocol, same admission,
    // driven over stdio by MCP (docs/agent-mcp-plan.md).
    if let Some(i) = args.iter().position(|a| a == "--agent") {
        let addr = args.get(i + 1).cloned().unwrap_or_else(|| {
            eprintln!("usage: wildforge --agent <host[:port]> [--name NAME]");
            std::process::exit(2);
        });
        let name = args
            .iter()
            .position(|a| a == "--name")
            .and_then(|n| args.get(n + 1).cloned())
            .unwrap_or_else(|| "AGENT".to_string());
        agent::run_agent(&addr, &name);
        return;
    }
    game::run_windowed();
}
