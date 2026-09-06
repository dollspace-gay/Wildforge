//! Runtime ticking for bloomeries, clamps, furnaces, and related machines.

mod shared_dispatch;
mod food_storage;
mod clamps;
mod stations;
mod steam;
mod entities;


mod bloomery;
pub(super) use bloomery::tick_bloomery_machines;
mod forge;
pub(super) use forge::tick_forge_machines;
mod separator;
pub(super) use separator::tick_separator_machines;
mod kiln;
pub(super) use kiln::tick_kiln_machines;
