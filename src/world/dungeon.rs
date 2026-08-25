//! Instanced dungeon zones (capability E10).
//!
//! A dungeon is an ordinary assembly whose def carries a [`DungeonDef`]:
//! it never generates at worldgen. Instead a *run* stamps it on demand
//! into a reserved slot on the Deep face — the seventh wing of the same
//! chunk store — and the run resets when it empties. Because Deep chunks
//! are never persisted (`World::save_chunk` skips them), every entry
//! regenerates fresh: the "world-timer reset" is structural, not bookkeeping.
//!
//! Being below is not a flag on the player: an entity stands in a dungeon
//! exactly when its position's face is the Deep. The overworld freeze,
//! checkpoint respawns, and exit routing all read that one fact.

use std::collections::HashMap;

use super::*;
use crate::planet::{ChunkPos, EntityPos, Face};

/// Chunks per side of one run's slot. 16 chunks = 256 blocks of floor
/// space per axis, room enough for any authored `max_pieces` walk.
pub const SLOT_CHUNKS: u16 = 16;

/// Slots per row across the Deep face (`FACE_CHUNKS / SLOT_CHUNKS`).
pub const SLOTS_PER_ROW: u16 = crate::planet::FACE_CHUNKS / SLOT_CHUNKS;

/// One active dungeon run: where it lives in the Deep, who came from
/// where, how far the party has gotten, and how long it has been empty.
#[derive(Clone, Debug)]
pub struct DungeonRun {
    /// Index into `Registry::assemblies`.
    pub assembly: usize,
    /// Reserved slot; `anchor` is its lowest chunk.
    pub slot: u32,
    pub anchor: ChunkPos,
    /// Where participants stand when they arrive (the entry piece origin).
    pub spawn: EntityPos,
    /// Per-participant overworld return positions (0 = host).
    pub returns: HashMap<u32, EntityPos>,
    /// The party-shared checkpoint (set at a checkpoint block).
    pub checkpoint: Option<EntityPos>,
    /// Seconds since the zone was last occupied; reset at the def's limit.
    pub empty_secs: f32,
}

impl World {
    /// Enter the named dungeon: reuse its live run or stamp a fresh one
    /// into a free Deep slot. Returns the in-dungeon spawn position; the
    /// caller teleports the participant — the return trip is recorded here
    /// against their player id.
    pub fn enter_dungeon(
        &mut self,
        player_id: u32,
        player_pos: EntityPos,
        assembly_name: &str,
    ) -> Option<EntityPos> {
        let idx = self
            .reg
            .assemblies
            .iter()
            .position(|a| a.name == assembly_name && a.dungeon.is_some())?;
        if let Some(run) = self
            .dungeon_runs
            .iter_mut()
            .find(|r| r.assembly == idx)
        {
            let spawn = run.spawn;
            run.returns.insert(player_id, player_pos);
            return Some(spawn);
        }
        let asm = self.reg.assemblies[idx].clone();
        let slot = Self::free_slot(&self.dungeon_runs)?;
        let base_u = (slot % u32::from(SLOTS_PER_ROW)) * u32::from(SLOT_CHUNKS);
        let base_v = (slot / u32::from(SLOTS_PER_ROW)) * u32::from(SLOT_CHUNKS);
        let anchor =
            ChunkPos::new(Face::Deep, base_u as u16, base_v as u16).ok()?;
        self.run_seed = self.run_seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let seed = self.seed ^ self.run_seed;
        // Void chunks generate instantly, so stamping is synchronous: the
        // rooms exist before this call returns and nobody falls through.
        let (markers, placed) = self.place_assembly(asm, anchor, seed);
        if placed == 0 {
            return None;
        }
        // Resolve the markers a run cares about: nest markers place their
        // den blocks through the ordinary block path, which registers the
        // garrison automatically (capability E9). Unknown kinds are
        // silently skipped, exactly like worldgen marker resolution.
        #[cfg(test)]
        eprintln!("diag-enter: markers={} first={:?}", markers.len(), markers.first().map(|m| m.kind.clone()));
        for marker in &markers {
            if let Some(nest_name) = marker.kind.strip_prefix("spawn:nest:") {
                let full = format!("belt_quest:{nest_name}");
                let _ = &full;
                if let Some(nest) =
                    self.reg.nests.iter().find(|n| n.id.ends_with(nest_name))
                {
                    self.set_block_at(marker.at, nest.block);
                }
            }
        }
        let origin = self.last_entry_anchor?;
        let spawn = EntityPos::new(
            Face::Deep,
            f32::from(origin.u()) + 0.5,
            f32::from(origin.y()) + 1.05,
            f32::from(origin.v()) + 0.5,
        )
        .ok()?;
        self.dungeon_runs.push(DungeonRun {
            assembly: idx,
            slot,
            anchor,
            spawn,
            returns: HashMap::new(),
            checkpoint: None,
            empty_secs: 0.0,
        });
        let run = self.dungeon_runs.last_mut().expect("just pushed");
        run.returns.insert(player_id, player_pos);
        Some(spawn)
    }

    /// Leave the dungeon whose slot contains `player_pos`, routed back to
    /// that participant's own recorded overworld position.
    pub fn exit_dungeon(&mut self, player_id: u32, player_pos: EntityPos) -> Option<EntityPos> {
        let run = self
            .dungeon_runs
            .iter()
            .find(|r| Self::run_contains(r, player_pos))?;
        run.returns.get(&player_id).copied()
    }

    /// Record the party-shared checkpoint for the run containing `at`.
    pub fn set_dungeon_checkpoint(&mut self, at: EntityPos) {
        if let Some(run) = self
            .dungeon_runs
            .iter_mut()
            .find(|r| Self::run_contains(r, at))
        {
            run.checkpoint = Some(at);
        }
    }

    /// Where a fallen participant respawns: the party checkpoint if the
    /// death happened inside its run, else nothing (ordinary respawn).
    pub fn dungeon_checkpoint_for(&self, at: EntityPos) -> Option<EntityPos> {
        let run = self
            .dungeon_runs
            .iter()
            .find(|r| Self::run_contains(r, at))?;
        run.checkpoint
    }

    /// Advance the run clocks. Empty zones reset after their def's delay;
    /// a resetting run's chunks are dropped unsaved so the next entry
    /// stamps fresh rooms.
    pub fn tick_dungeon_runs(&mut self, dt: f32, players_deep: usize) {
        if players_deep > 0 {
            for run in &mut self.dungeon_runs {
                run.empty_secs = 0.0;
            }
            return;
        }
        for run in &mut self.dungeon_runs {
            run.empty_secs += dt;
        }
        let resets: Vec<f32> = self
            .reg
            .assemblies
            .iter()
            .map(|a| a.dungeon.as_ref().map_or(f32::INFINITY, |d| d.reset))
            .collect();
        let mut expired: Vec<DungeonRun> = Vec::new();
        self.dungeon_runs.retain(|run| {
            if run.empty_secs >= resets[run.assembly] {
                expired.push(run.clone());
                return false;
            }
            true
        });
        for run in expired {
            for cu in 0..u32::from(SLOT_CHUNKS) {
                for cv in 0..u32::from(SLOT_CHUNKS) {
                    let Ok(pos) = ChunkPos::new(
                        Face::Deep,
                        run.anchor.u() + cu as u16,
                        run.anchor.v() + cv as u16,
                    ) else {
                        continue;
                    };
                    self.chunks.remove(&pos);
                }
            }
            // Nests stamped inside the zone die with it.
            let anchor = run.anchor;
            let max_u = anchor.u() + SLOT_CHUNKS;
            let max_v = anchor.v() + SLOT_CHUNKS;
            self.nests.retain(|nest_pos, _| {
                let c = nest_pos.chunk();
                !(c.u() >= anchor.u() && c.u() < max_u && c.v() >= anchor.v() && c.v() < max_v)
            });
        }
    }

    fn run_contains(run: &DungeonRun, pos: EntityPos) -> bool {
        let Some(chunk) = pos.chunk() else {
            return false;
        };
        chunk.face() == Face::Deep && Self::chunk_in_slot(run.anchor, chunk)
    }

    fn chunk_in_slot(anchor: ChunkPos, chunk: crate::planet::ChunkPos) -> bool {
        chunk.u() >= anchor.u()
            && chunk.u() < anchor.u() + SLOT_CHUNKS
            && chunk.v() >= anchor.v()
            && chunk.v() < anchor.v() + SLOT_CHUNKS
    }

    fn free_slot(runs: &[DungeonRun]) -> Option<u32> {
        (0..u32::from(SLOTS_PER_ROW) * u32::from(SLOTS_PER_ROW))
            .find(|slot| runs.iter().all(|r| r.slot != *slot))
    }
}
