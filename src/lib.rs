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
mod dedicated;
mod entity;
mod game;
mod identity;
mod inventory;
mod lights;
mod mesher;
mod mobs;
mod mp;
mod net;
mod particles;
mod physics;
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
    if let Some(i) = args.iter().position(|a| a == "--server") {
        let world = args
            .get(i + 1)
            .cloned()
            .unwrap_or_else(|| "world1".to_string());
        dedicated::run_headless_server(&world);
        return;
    }
    game::run_windowed();
}
