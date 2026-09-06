//! Shared dispatch machine_tick transaction coordination.

use crate::world::World;
use super::tick_bloomery_machines;
use super::tick_forge_machines;
use super::tick_kiln_machines;
use super::tick_separator_machines;

impl World {
    /// Advance machines. Returns true if any visible state changed.
    /// Fire every lit bloomery: the weather can slow or douse an
    /// unroofed stack, and the batch is cashed when the fire's done.
    /// The shell itself is revalidated by the edit hook, not here.
    pub(in crate::world) fn tick_bloomeries(&mut self, dt: f32) {
        tick_bloomery_machines(self, dt);
    }

    /// Fire every lit forge: chimney and all, so rain never touches
    /// it — the workshop's edge over the open stack. A firing smelts
    /// any furnace recipe in batch at FORGE_ITEMS_PER_FUEL per fuel,
    /// spitting outputs (and cupellation byproducts) at the mouth.
    /// The shell is revalidated by the edit hook, not here.
    pub(in crate::world) fn tick_forges(&mut self, dt: f32) {
        tick_forge_machines(self, dt);
    }

    /// Fire every charged separator on a valid firebrick stack: one
    /// powder and one fuel a batch, neodymium and cerium out — the
    /// rare-earth thread, finally honest (mechanization stage 6). The
    /// shell is revalidated by the edit hook, not here.
    pub(in crate::world) fn tick_separators(&mut self, dt: f32) {
        tick_separator_machines(self, dt);
    }

    /// Fire every lit kiln: shared shell/weather rules, glass out. The
    /// shell is revalidated by the edit hook, not here; a chimneyed
    /// kiln reads its glassworks bonus from the folded stats.
    pub(in crate::world) fn tick_kilns(&mut self, dt: f32) {
        tick_kiln_machines(self, dt);
    }

    /// Tick machines hosted inside spawned structures, with the same
    /// per-kind logic and progress as world-hosted machines, but
    /// structure-local state only. Structure machines are exempt from
    /// the main world's ire and material ledger by design; their
    /// outputs collect in `LocalStructure.outbox`.
    pub(in crate::world) fn tick_local_structure_machines(&mut self, dt: f32) {
        for structure in self.construction.structures_mut() {
            tick_bloomery_machines(structure, dt);
            tick_kiln_machines(structure, dt);
            tick_forge_machines(structure, dt);
            tick_separator_machines(structure, dt);
        }
    }
}
