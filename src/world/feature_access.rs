//! Feature access coordinator for the authoritative world.

use super::{BlockPos, ChunkPos, RevealKey, World};

impl World {
    /// Index of the gate sealing `pos`, if any (spec 2.5).
    pub fn gate_at(&self, pos: BlockPos) -> Option<usize> {
        self.gated.get(&pos).copied()
    }

    /// Place a gate's sealed block at `pos` and record it as gated. Used by
    /// the `feature:<id>` marker consumer; unknown gate ids never reach here.
    pub(crate) fn place_gate_at(&mut self, gate: usize, pos: BlockPos) {
        let reg = self.reg.clone();
        let Some(definition) = reg.gates.get(gate) else {
            return;
        };
        self.ensure_chunk(pos.chunk());
        self.set_block_at(pos, definition.block);
        self.gated.insert(pos, gate);
        if let Some(chunk) = self.chunks.get_mut(&pos.chunk()) {
            chunk.modified = true;
        }
    }
    /// Remove a position from the gated registry after it has been unlocked
    /// and replaced (so it never counts again).
    pub(crate) fn ungate_at(&mut self, pos: BlockPos) {
        self.gated.remove(&pos);
    }

    #[cfg(test)]
    /// Whether a position is sealed by a gate (for gate tests).
    pub fn is_gated_for_test(&self, pos: BlockPos) -> bool {
        self.gated.contains_key(&pos)
    }

    // ---- Settlement hidden cells (spec 3.4) ----

    /// Whether `pos` is a settlement hidden cell (placed at worldgen but
    /// behaving as air until its tier is revealed).
    pub fn is_hidden(&self, pos: BlockPos) -> bool {
        self.hidden.contains_key(&pos)
    }

    /// The hidden positions inside one chunk (for mesher capture; the mesh
    /// runs off a snapshot, so the world's hidden registry must be consulted
    /// on the main thread at capture time).
    pub(crate) fn hidden_in_chunk(&self, chunk: crate::planet::ChunkPos) -> Vec<BlockPos> {
        self.hidden
            .keys()
            .filter(|pos| pos.chunk() == chunk)
            .copied()
            .collect()
    }

    /// Record a settlement hidden cell. The block is expected to already be
    /// stamped at `pos`; this marks it inert until the tier is revealed.
    pub(crate) fn hide_at(&mut self, pos: BlockPos, key: RevealKey) {
        self.hidden.insert(pos, key);
    }

    /// Reveal every hidden cell of `settlement` whose tier threshold
    /// `reputation` meets, dropping them from the registry so the already-
    /// placed blocks become solid/visible. Marks affected chunks modified for
    /// re-mesh. Returns the number of tiers revealed.
    pub fn reveal_settlement(&mut self, settlement: usize, reputation: u32) -> u32 {
        let mut revealed_tiers: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        self.hidden.retain(|pos, key| {
            if key.settlement != settlement {
                return true;
            }
            let revealed = self
                .reg
                .settlements
                .get(settlement)
                .and_then(|def| {
                    def.tiers
                        .iter()
                        .find(|t| t.tier == key.tier)
                        .map(|t| t.threshold <= reputation)
                })
                .unwrap_or(true);
            if revealed {
                revealed_tiers.insert(key.tier);
                if let Some(chunk) = self.chunks.get_mut(&pos.chunk()) {
                    chunk.modified = true;
                }
            }
            !revealed
        });
        revealed_tiers.len() as u32
    }

    #[cfg(test)]
    /// Number of hidden cells of one settlement (for settlement tests).
    pub fn hidden_count_for(&self, settlement: usize) -> usize {
        self.hidden
            .values()
            .filter(|key| key.settlement == settlement)
            .count()
    }

    #[cfg(test)]
    /// Whether `pos` is a hidden cell of the given settlement tier.
    pub fn is_hidden_tier_for_test(&self, pos: BlockPos, settlement: usize, tier: u32) -> bool {
        self.hidden
            .get(&pos)
            .is_some_and(|key| key.settlement == settlement && key.tier == tier)
    }

    #[cfg(test)]
    /// The hidden positions of one settlement (for settlement tests).
    pub fn hidden_positions_for_test(&self, settlement: usize) -> Vec<BlockPos> {
        self.hidden
            .iter()
            .filter(|(_, key)| key.settlement == settlement)
            .map(|(pos, _)| *pos)
            .collect()
    }

    #[cfg(test)]
    /// Whether this chunk is claimed by a structure or piece assembly.
    pub fn is_structure_chunk_for_test(&self, pos: ChunkPos) -> bool {
        self.structure_chunks.contains(&pos)
    }

    #[cfg(test)]
    /// Entries held across the land's decaying ledgers.
    ///
    /// These are keyed per 256-block cell, persisted, and rewritten whole on
    /// every save, so they are only bounded because each decays to nothing
    /// and drops its entry when it gets there.
    pub fn ledger_len(&self) -> usize {
        self.regional_ire.len() + self.bloom.len() + self.blessed_streak.len()
    }
}
