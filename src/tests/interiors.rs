//! Block-built interiors (task brief Phase 7b): a functioning machine
//! inside a `LocalStructure`'s own block store.
//!
//! Proof that the `BlockStore` generalization gave structures real block
//! behavior: an in-structure forge validates through the same matcher that
//! reads the main chunk grid, lights through the same logic, fires through
//! the same per-kind tick, stays scoped to its own edits, and survives a
//! save/reload round-trip — all without a single main-world block edit.
//!
//! Phase 8 extends this with structure-aware targeting (`raycast_target_at`),
//! structure-scoped break/place, and the Phase‑8 player-facing workflow.

use super::*;
use crate::world::local_structure::{LocalStructureId, from_template};
use crate::world::machines::light_machine_at;
use crate::world::multiblock::BlockRead;
use crate::world::multiblock::{BlockStore, Rotation};
use crate::world::template::capture_region;
use crate::world::{BlockEntity, FORGE_FIRE_SECS, MachineInstance};

const MY: i32 = 120;

/// The forge helpers below place their mouth at the local offset `(1, 0, 1)`
/// of the captured region, so every shape (which anchors at the mouth) can
/// be validated from this constant.
const MOUTH: (i32, i32, i32) = (1, 0, 1);

/// A forge machine holding 8 raw copper + 8 charcoal — the same batch the
/// world-hosted forge test fires, so outcomes are directly comparable.
fn charged_forge(rc: &Registry) -> MachineInstance {
    let raw = it(rc, "base:raw_copper");
    let coal = it(rc, "base:charcoal");
    let mut st = MachineInstance {
        kind: rc.machine_kind("base:forge").unwrap_or_default(),
        ..Default::default()
    };
    for i in 0..4 {
        st.charge[i] = Some(ItemStack::new(rc, raw, 2));
    }
    st.fuel[0] = Some(ItemStack::new(rc, coal, 4));
    st.fuel[1] = Some(ItemStack::new(rc, coal, 4));
    st
}

/// Captured copper ingots sitting in a structure's outbox.
fn outbox_ingots(structure: &crate::world::local_structure::LocalStructure) -> u32 {
    let ingot = structure.reg.item_id("base:copper_ingot");
    structure
        .outbox
        .iter()
        .filter(|s| Some(s.item) == ingot)
        .map(|s| s.count)
        .sum()
}

/// Absolute‑frame BlockPos — the same coordinate frame the world DDA
/// (`cast_at`) walks in (absolute `u`, `y`, `v` on the face).
fn bp_abs(u: i32, y: i32, v: i32) -> crate::planet::BlockPos {
    crate::planet::BlockPos::new(crate::planet::Face::PosZ, u as u16, y as u8, v as u16)
        .expect("valid absolute coordinate")
}

/// Absolute‑frame EntityPos — same frame as the world DDA.
fn origin_abs(u: f32, y: f32, v: f32) -> crate::planet::EntityPos {
    crate::planet::EntityPos::new(crate::planet::Face::PosZ, u, y, v)
        .expect("valid absolute entity position")
}

/// A single‑cell template captured from a loaded absolute position, well
/// above the ray path. Only the cell offsets matter, not the capture site.
fn make_single_abs(w: &mut World, rc: &Registry) -> crate::world::template::Template {
    let site = bp_abs(4080, MY + 50, 4080);
    w.set_block_at(site, b(rc, "base:firebrick"));
    capture_region(w, site, site, "s").expect("capture")
}

/// Face‑centre absolute u/v coordinates.
const TU: i32 = 4096;
const TV: i32 = 4096;

/// Spawn a single‑cell structure at absolute `(TU + cu, MY, TV)`.
fn spawn_single_abs(w: &mut World, rc: &Registry, cu: i32) -> LocalStructureId {
    let tpl = make_single_abs(w, rc);
    w.spawn_structure(&tpl, bp_abs(TU + cu, MY, TV), Rotation::R0)
        .expect("spawn")
}

mod edits;
mod machines;
mod selection;
