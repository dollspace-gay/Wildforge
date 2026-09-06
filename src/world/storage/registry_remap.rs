//! Registry remap storage transaction coordination.

use crate::registry::Registry;
use crate::world::World;
use std::collections::HashMap;

impl World {
    /// Remap all in-memory chunks from an old registry to the current one
    /// (used by hot reload). Unknown blocks become the placeholder.
    pub fn remap_from(&mut self, old: &Registry) {
        self.chunks.remap_from(old, &self.reg);
        // Re-resolve gated positions (spec 2.5) against the new registry's
        // gate list by their sealed block; a gate whose def was removed (or
        // whose sealed block changed) stops gating rather than softlocking
        // the world with a permanent unbreakable wall.
        let mut gates: HashMap<_, _> = HashMap::with_capacity(self.gated.len());
        for pos in std::mem::take(&mut self.gated).into_keys() {
            if let Some(gate) = self.reg.gate_for_block(self.get_block_at(pos)) {
                gates.insert(pos, gate);
            }
        }
        self.gated = gates;
        // Re-resolve hidden cells (spec 3.4): a record whose settlement def
        // was removed, or whose tier is no longer declared, is dropped and
        // treated as revealed (the placed block simply becomes solid/visible).
        self.hidden.retain(|_, key| {
            self.reg
                .settlements
                .get(key.settlement)
                .is_some_and(|def| def.tiers.iter().any(|t| t.tier == key.tier))
        });
    }
}
