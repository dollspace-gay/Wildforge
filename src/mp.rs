//! Host-side multiplayer facade.
//!
//! Session orchestration lives under `multiplayer/`; this compatibility module
//! keeps the rest of the game independent of that internal file layout.

#[path = "multiplayer/host.rs"]
mod host;

pub use host::{HostFx, HostSession, Role, block_remap, item_remap};
