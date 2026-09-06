//! Kindle workings transaction coordination.

use crate::planet::BlockPos;
use crate::workings::PhysicalDebit;
use crate::workings::PhysicalDebitKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingResult;
use crate::world::World;
use super::working_distance;

impl World {
    /// Kindle supplies initial heat only. The named effect records both the
    /// ordinary fuel and the replaceable cell where normal fire will live.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn begin_kindle_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        fuel: BlockPos,
        fire_cell: BlockPos,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.begin_kindle_working_definition(
            actor,
            actor_label,
            source,
            wand_id,
            "base:kindle",
            fuel,
            fire_cell,
            forced,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_kindle_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        fuel: BlockPos,
        fire_cell: BlockPos,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.validate_kindle_target(fuel, fire_cell)?;
        let effect = WorkingEffect::Ignite {
            fuel,
            fire_cell,
            expected_fuel: self.get_block_at(fuel).0,
            expected_air: self.get_block_at(fire_cell).0,
            player_caused: true,
        };
        let targets = vec![self.block_snapshot(fuel), self.block_snapshot(fire_cell)];
        let physical = vec![PhysicalDebit {
            kind: PhysicalDebitKind::Heat,
            source: format!("fuel:{fuel:?}"),
            content_id: self.reg.block(self.get_block_at(fuel)).name.clone(),
            units: 1,
            expected_version: 0,
        }];
        self.reserve_wand_effect(
            actor,
            actor_label,
            source,
            wand_id,
            working_id,
            targets,
            physical,
            effect,
            1,
            working_distance(source, fuel),
            0,
            forced,
        )
    }
}
