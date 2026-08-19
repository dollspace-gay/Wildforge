//! Wildforge — a Minecraft-alpha-style voxel game.
//!
//! Controls: WASD move, mouse look, Space jump, Ctrl sprint,
//! hold left click to mine, right click place, middle click pick block,
//! 1-9 / scroll wheel select hotbar slot, E inventory, Esc pause,
//! F2 screenshot, F11 fullscreen.

mod agent;
pub mod alchemy;
mod arcane;
mod arcane_ecology;
mod arcane_geography;
mod atlas;
mod audio;
mod bounce;
mod camera;
mod chunk;
mod config;
mod crafting;
mod dedicated;
mod discovery;
mod dross;
mod edifice;
mod entity;
mod game;
mod geode_capture;
mod identity;
mod implements;
mod inventory;
mod lights;
mod magic_qualification;
mod materials;
mod mesher;
mod mobs;
mod mod_lint;
mod mp;
mod npc;
mod net;
mod particles;
mod persist;
mod physics;
pub mod planet;
pub mod planet_atlas;
mod raycast;
mod registry;
mod renderer;
mod ruleset;
mod script;
mod server;
mod skills;
mod sky;
mod stats;
mod style;
#[cfg(test)]
mod tests;
mod ui;
mod visual_capture;
mod workings;
mod world;
mod worldgen;

#[cfg(test)]
pub(crate) use game::{browser_items, content_tree_stamp_of, next_world_name, reduced_damage};

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use glam::Vec3;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
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
    if let Some(result) = geode_capture::run_cli(&args) {
        if let Err(error) = result {
            eprintln!("cracked-geode qualification failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    if let Some(i) = args.iter().position(|arg| arg == "--magic-qualification") {
        let Some(world) = args.get(i + 1).map(PathBuf::from) else {
            eprintln!(
                "usage: wildforge --magic-qualification <world> [--mods <directory>] [--output <report.txt>]"
            );
            std::process::exit(2);
        };
        let mods = args
            .iter()
            .position(|arg| arg == "--mods")
            .and_then(|index| args.get(index + 1))
            .map_or_else(|| PathBuf::from("mods"), PathBuf::from);
        match magic_qualification::audit_world(&world, &mods) {
            Ok(report) => {
                print!("{}", report.render());
                if let Some(output) = args
                    .iter()
                    .position(|arg| arg == "--output")
                    .and_then(|index| args.get(index + 1))
                    .map(PathBuf::from)
                    && let Err(error) =
                        identity::atomic_write(&output, report.render().as_bytes(), false)
                {
                    eprintln!("magic qualification report write failed: {error}");
                    std::process::exit(1);
                }
                if !report.qualified {
                    std::process::exit(1);
                }
            }
            Err(error) => {
                eprintln!("magic qualification failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(i) = args.iter().position(|arg| arg == "--alchemy-audit") {
        let Some(world) = args.get(i + 1).map(PathBuf::from) else {
            eprintln!("usage: wildforge --alchemy-audit <world>");
            std::process::exit(2);
        };
        match alchemy::audit_world(&world) {
            Ok(audit) => {
                print!("{}", audit.render());
                if !audit.is_qualified() {
                    std::process::exit(1);
                }
            }
            Err(error) => {
                eprintln!("alchemy audit failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(i) = args.iter().position(|arg| arg == "--workings-audit") {
        let Some(world) = args.get(i + 1).map(PathBuf::from) else {
            eprintln!("usage: wildforge --workings-audit <world>");
            std::process::exit(2);
        };
        match workings::audit_world(&world) {
            Ok(audit) => {
                print!("{}", audit.render());
                if !audit.is_qualified() {
                    std::process::exit(1);
                }
            }
            Err(error) => {
                eprintln!("workings audit failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(i) = args.iter().position(|arg| arg == "--implements-audit") {
        let Some(world) = args.get(i + 1).map(PathBuf::from) else {
            eprintln!("usage: wildforge --implements-audit <world>");
            std::process::exit(2);
        };
        match implements::audit_world(&world) {
            Ok(audit) => {
                print!("{}", audit.render());
                if !audit.is_qualified() {
                    std::process::exit(1);
                }
            }
            Err(error) => {
                eprintln!("implements audit failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(i) = args.iter().position(|arg| arg == "--discovery-audit") {
        let Some(world) = args.get(i + 1).map(PathBuf::from) else {
            eprintln!("usage: wildforge --discovery-audit <world>");
            std::process::exit(2);
        };
        match discovery::audit_world(&world) {
            Ok(audit) => {
                print!("{}", audit.render());
                if !audit.is_qualified() {
                    std::process::exit(1);
                }
            }
            Err(error) => {
                eprintln!("discovery audit failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
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
    if let Some(i) = args.iter().position(|arg| arg == "--arcane-audit") {
        let Some(world) = args.get(i + 1).map(PathBuf::from) else {
            eprintln!("usage: wildforge --arcane-audit <world>");
            std::process::exit(2);
        };
        match arcane::audit_world(&world) {
            Ok(audit) => {
                print!("{}", audit.render());
                if !audit.is_balanced() {
                    std::process::exit(1);
                }
                match dross::audit_world(&world) {
                    Ok(dross) => {
                        print!("{}", dross.render());
                        if !dross.is_balanced() {
                            std::process::exit(1);
                        }
                    }
                    Err(error) => {
                        eprintln!("dross audit failed: {error}");
                        std::process::exit(1);
                    }
                }
            }
            Err(error) => {
                eprintln!("arcane audit failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(i) = args.iter().position(|arg| arg == "--arcane-atlas") {
        let Some(world) = args.get(i + 1).map(PathBuf::from) else {
            eprintln!(
                "usage: wildforge --arcane-atlas <world> --layer dross [--output <directory>]"
            );
            std::process::exit(2);
        };
        let layer = args
            .iter()
            .position(|arg| arg == "--layer")
            .and_then(|index| args.get(index + 1));
        if layer.is_none_or(|layer| layer != "dross") {
            eprintln!("arcane atlas currently requires --layer dross");
            std::process::exit(2);
        }
        let output = args
            .iter()
            .position(|arg| arg == "--output")
            .and_then(|index| args.get(index + 1))
            .map_or_else(|| world.join("diagnostics/arcane-atlas"), PathBuf::from);
        let result = (|| -> Result<Vec<String>, String> {
            let atlas =
                planet_atlas::PlanetAtlas::load(&world).map_err(|error| error.to_string())?;
            let geography = arcane_geography::ArcaneGeography::load(&world, &atlas)
                .map_err(|error| error.to_string())?;
            geography
                .export_dross_diagnostics(&atlas, &output)
                .map_err(|error| error.to_string())
        })();
        match result {
            Ok(files) => println!(
                "exported {} dross atlas views to {}",
                files.len(),
                output.display()
            ),
            Err(error) => {
                eprintln!("dross atlas export failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(i) = args
        .iter()
        .position(|arg| arg == "--arcane-geography-audit")
    {
        let Some(world) = args.get(i + 1).map(PathBuf::from) else {
            eprintln!("usage: wildforge --arcane-geography-audit <world>");
            std::process::exit(2);
        };
        match arcane_geography::audit_world(&world) {
            Ok((audit, custody_matches)) => {
                print!("{}", audit.render());
                println!("Ledger custody matches: {custody_matches}");
                if !audit.is_balanced() || !custody_matches {
                    std::process::exit(1);
                }
            }
            Err(error) => {
                eprintln!("arcane geography audit failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(i) = args.iter().position(|arg| arg == "--arcane-ecology-audit") {
        let Some(world) = args.get(i + 1).map(PathBuf::from) else {
            eprintln!("usage: wildforge --arcane-ecology-audit <world>");
            std::process::exit(2);
        };
        let result = (|| -> Result<(arcane_ecology::EcologyAudit, bool), String> {
            let atlas =
                planet_atlas::PlanetAtlas::load(&world).map_err(|error| error.to_string())?;
            let registry = registry::load(std::path::Path::new("mods"));
            if !registry.arcane_errors.is_empty() {
                return Err(registry.arcane_errors.join("\n"));
            }
            let geography = arcane_geography::ArcaneGeography::load(&world, &atlas)
                .map_err(|error| error.to_string())?;
            let ecology = arcane_ecology::audit(&registry, &geography.dynamic.ecology)?;
            let (geography_audit, custody_matches) =
                arcane_geography::audit_world(&world).map_err(|error| error.to_string())?;
            Ok((ecology, geography_audit.is_balanced() && custody_matches))
        })();
        match result {
            Ok((audit, balanced)) => {
                print!("{}", audit.render());
                if !balanced {
                    std::process::exit(1);
                }
            }
            Err(error) => {
                eprintln!("arcane ecology audit failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(i) = args
        .iter()
        .position(|arg| arg == "--arcane-geography-export")
    {
        let Some(world) = args.get(i + 1).map(PathBuf::from) else {
            eprintln!("usage: wildforge --arcane-geography-export <world> --output <directory>");
            std::process::exit(2);
        };
        let Some(output) = args
            .iter()
            .position(|arg| arg == "--output")
            .and_then(|index| args.get(index + 1))
            .map(PathBuf::from)
        else {
            eprintln!("usage: wildforge --arcane-geography-export <world> --output <directory>");
            std::process::exit(2);
        };
        let result = (|| {
            let atlas = planet_atlas::PlanetAtlas::load(&world)?;
            let geography = arcane_geography::ArcaneGeography::load(&world, &atlas)
                .map_err(|error| planet_atlas::AtlasError::Corrupt(error.to_string()))?;
            geography
                .export_diagnostics(&atlas, &output)
                .map_err(|error| planet_atlas::AtlasError::Corrupt(error.to_string()))
        })();
        match result {
            Ok(report) => println!(
                "exported {} arcane maps for {} cells to {}",
                report.maps.len(),
                report.cell_count,
                output.display()
            ),
            Err(error) => {
                eprintln!("arcane geography export failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(i) = args
        .iter()
        .position(|arg| arg == "--arcane-geography-retrogen")
    {
        let Some(world) = args.get(i + 1).map(PathBuf::from) else {
            eprintln!("usage: wildforge --arcane-geography-retrogen <world>");
            std::process::exit(2);
        };
        let result = (|| -> Result<usize, String> {
            let atlas =
                planet_atlas::PlanetAtlas::load(&world).map_err(|error| error.to_string())?;
            let registry = registry::load(std::path::Path::new("mods"));
            if !registry.arcane_errors.is_empty() {
                return Err(registry.arcane_errors.join("\n"));
            }
            let mut geography = arcane_geography::ArcaneGeography::load(&world, &atlas)
                .map_err(|error| error.to_string())?;
            let added = geography
                .apply_retrogen(&atlas, &registry)
                .map_err(|error| error.to_string())?;
            geography
                .save_retrogen(&world)
                .map_err(|error| error.to_string())?;
            Ok(added)
        })();
        match result {
            Ok(added) => println!("arcane geography retrogen added {added} finite sites"),
            Err(error) => {
                eprintln!("arcane geography retrogen failed: {error}");
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
    if let Some(i) = args.iter().position(|arg| arg == "--create-world") {
        let Some(seed) = args.get(i + 1).and_then(|value| value.parse::<u32>().ok()) else {
            eprintln!("usage: wildforge --create-world <seed> --output <directory>");
            std::process::exit(2);
        };
        let Some(output) = args
            .iter()
            .position(|arg| arg == "--output")
            .and_then(|index| args.get(index + 1))
            .map(PathBuf::from)
        else {
            eprintln!("usage: wildforge --create-world <seed> --output <directory>");
            std::process::exit(2);
        };
        let registry = Arc::new(registry::load(std::path::Path::new("mods")));
        if !registry.arcane_errors.is_empty() {
            eprintln!(
                "world creation rejected invalid arcane content:\n{}",
                registry.arcane_errors.join("\n")
            );
            std::process::exit(1);
        }
        let content_hash = planet_atlas::genesis_content_hash(std::path::Path::new("mods"));
        let cancel = planet_atlas::CancellationToken::default();
        eprintln!("world: seed {seed}, output {}", output.display());
        if let Err(error) = world::create_world_atomic(
            &output,
            seed,
            "survival",
            content_hash,
            registry,
            &cancel,
            |progress| match progress {
                world::WorldCreationProgress::Atlas(progress) => eprintln!(
                    "world: atlas [{}/{}] {}",
                    progress.completed_stages + 1,
                    progress.total_stages,
                    progress.stage.label()
                ),
                world::WorldCreationProgress::Arcane(progress) => eprintln!(
                    "world: arcane [{}/{}] {}",
                    progress.completed_stages + 1,
                    progress.total_stages,
                    progress.stage.label()
                ),
                world::WorldCreationProgress::Homeland {
                    stage,
                    completed,
                    total,
                } => eprintln!("world: homeland [{completed}/{total}] {stage}"),
            },
        ) {
            eprintln!("world creation failed: {error}");
            std::process::exit(1);
        }
        eprintln!("world: complete");
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
    if let Some(i) = args.iter().position(|arg| arg == "--mod-qualification") {
        let Some(mods_dir) = args.get(i + 1).map(PathBuf::from) else {
            eprintln!("usage: wildforge --mod-qualification <mods_dir>");
            std::process::exit(2);
        };
        let report = mod_lint::qualify_mods(&mods_dir);
        print!("{}", report.render());
        if !report.is_qualified() {
            std::process::exit(1);
        }
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
