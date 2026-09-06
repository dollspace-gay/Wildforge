//! Trace gleam workings transaction coordination.

use super::working_distance;
use crate::planet::BlockPos;
use crate::workings::WorkingEffect;
use crate::workings::WorkingResult;
use crate::world::World;

impl World {
    /// Begin the harmless sensing working. The active transaction itself is
    /// the bounded observation capability read by lens/renderer code.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn begin_trace_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        target: BlockPos,
        duration_ticks: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.begin_trace_working_definition(
            actor,
            actor_label,
            source,
            wand_id,
            "base:trace",
            target,
            duration_ticks,
            forced,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_trace_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        target: BlockPos,
        duration_ticks: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        let effect = WorkingEffect::Observe {
            origin: target,
            expires_tick: self.working_tick().saturating_add(duration_ticks),
        };
        self.reserve_wand_effect(
            actor,
            actor_label,
            source,
            wand_id,
            working_id,
            vec![self.block_snapshot(target)],
            Vec::new(),
            effect,
            1,
            working_distance(source, target),
            duration_ticks,
            forced,
        )
    }

    /// Begin a temporary point light. No voxel or inventory item is created.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn begin_gleam_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        target: BlockPos,
        intensity: u8,
        duration_ticks: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.begin_gleam_working_definition(
            actor,
            actor_label,
            source,
            wand_id,
            "base:gleam",
            target,
            intensity,
            duration_ticks,
            forced,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_gleam_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        target: BlockPos,
        intensity: u8,
        duration_ticks: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        if intensity == 0 || intensity > 8 {
            return Err("Gleam is bounded to intensity 1..=8.".into());
        }
        let effect = WorkingEffect::PointLight {
            source,
            target,
            intensity,
            expires_tick: self.working_tick().saturating_add(duration_ticks),
        };
        self.reserve_wand_effect(
            actor,
            actor_label,
            source,
            wand_id,
            working_id,
            vec![self.block_snapshot(target)],
            Vec::new(),
            effect,
            u32::from(intensity),
            working_distance(source, target),
            duration_ticks,
            forced,
        )
    }
}
