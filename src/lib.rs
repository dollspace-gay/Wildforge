//! Wildforge — a Minecraft-alpha-style voxel game.
//!
//! Controls: WASD move, mouse look, Space jump, Ctrl sprint,
//! hold left click to mine, right click place, middle click pick block,
//! 1-9 / scroll wheel select hotbar slot, E inventory, Esc pause,
//! F2 screenshot, F11 fullscreen.

mod agent;
mod app;
pub mod alchemy;
mod arcane;
mod arcane_ecology;
mod arcane_geography;
mod atlas;
mod audio;
mod background;
mod bounce;
mod camera;
mod chunk;
mod client_session;
mod config;
mod content_files;
mod crafting;
mod dedicated;
mod discovery;
mod dross;
mod edifice;
mod entity;
mod equipment;
mod game;
mod geode_capture;
mod identity;
mod implements;
mod inventory;
mod lights;
mod machines;
mod magic_qualification;
mod materials;
mod mesher;
mod mobs;
mod mod_lint;
mod mp;
mod net;
mod npc;
mod particles;
mod persist;
mod physics;
mod player_ops;
pub mod planet;
pub mod planet_atlas;
mod raycast;
mod registry;
mod renderer;
mod ruleset;
mod screens;
mod script;
mod server;
mod shader;
mod skills;
mod sky;
mod stats;
mod style;
mod terrain_jobs;
#[cfg(test)]
mod tests;
mod ui;
mod visual_capture;
mod workings;
mod world;
mod worldgen;

#[cfg(test)]
pub(crate) use game::{browser_items, content_tree_stamp_of, next_world_name, reduced_damage};

#[cfg(test)]
use world::World;

/// Run Wildforge using process arguments and the platform event loop.
pub fn run() {
    app::run();
}
