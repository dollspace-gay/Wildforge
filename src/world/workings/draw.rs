//! Draw workings transaction coordination.

use crate::world::BlockPos;
use crate::workings::PhysicalDebit;
use crate::workings::PhysicalDebitKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingResult;
use crate::world::World;
use super::working_distance;

impl World {
    /// Build one exact conservative water parcel. Both reservoir snapshots
    /// remain in the durable transaction so replay can distinguish before,
    /// after, and an unsafe competing third state.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn begin_draw_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        from: BlockPos,
        to: BlockPos,
        water_hu: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.begin_draw_working_definition(
            actor,
            actor_label,
            source,
            wand_id,
            "base:draw",
            from,
            to,
            water_hu,
            forced,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_draw_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        from: BlockPos,
        to: BlockPos,
        water_hu: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        if from == to
            || water_hu == 0
            || water_hu > 64
            || !water_hu.is_multiple_of(crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL)
        {
            return Err("Draw moves 32 or 64 hydro-units between distinct reservoirs.".into());
        }
        let mut source_mass = self
            .water_mass_at(from)
            .ok_or("The source is not a legitimate visible water reservoir.")?;
        if source_mass.water_hu < water_hu {
            return Err("The source does not contain that exact volume.".into());
        }
        let destination_mass = self.water_mass_at(to).unwrap_or_default();
        if !self.reg.is_air(self.get_block_at(to)) && !self.reg.is_water(self.get_block_at(to)) {
            return Err(
                "The destination is neither water nor a replaceable empty reservoir.".into(),
            );
        }
        let source_before = source_mass;
        let parcel = source_mass.take(water_hu);
        let destination_after = destination_mass
            .checked_add(parcel)
            .filter(|mass| mass.water_hu <= crate::planet_atlas::HYDRO_UNITS_PER_BLOCK)
            .ok_or("The destination would overflow; no water was moved.")?;
        let source_carrier = self.water_carrier_at(from, source_before);
        let destination_carrier = self.water_carrier_at(to, destination_mass);
        let mut source_after_carrier = source_carrier;
        let parcel_carrier = source_after_carrier
            .take(source_before.water_hu, water_hu)
            .ok_or("Draw could not split the source's exact carriers.")?;
        let _destination_after_carrier = destination_carrier
            .checked_add(parcel_carrier)
            .ok_or("Draw destination carrier accounting overflowed.")?;
        let targets = vec![
            self.reservoir_snapshot_with_carrier(from, source_before, source_carrier),
            self.reservoir_snapshot_with_carrier(to, destination_mass, destination_carrier),
        ];
        let effect = WorkingEffect::TransferWater {
            from,
            to,
            water_hu: parcel.water_hu,
            salt_mass: parcel.salt_mass,
            temperature_millic: parcel_carrier.temperature_millic(parcel.water_hu),
            thermal_millic_hu: parcel_carrier.thermal_millic_hu,
            dross_units: parcel_carrier.dross_subunits / 256,
            dross_subunits: parcel_carrier.dross_subunits,
            carrier_remainder_before: source_carrier.dross_subunits % 256,
            carrier_remainder_after: source_after_carrier.dross_subunits % 256,
        };
        let mut physical = vec![PhysicalDebit {
            kind: PhysicalDebitKind::Fluid,
            source: format!("reservoir:{from:?}"),
            content_id: "water".into(),
            units: parcel.water_hu,
            expected_version: 0,
        }];
        if parcel.salt_mass != 0 {
            physical.push(PhysicalDebit {
                kind: PhysicalDebitKind::Fluid,
                source: format!("reservoir:{from:?}"),
                content_id: "salt".into(),
                units: parcel.salt_mass,
                expected_version: 0,
            });
        }
        if parcel_carrier.dross_subunits != 0 {
            physical.push(PhysicalDebit {
                kind: PhysicalDebitKind::Fluid,
                source: format!("reservoir:{from:?}"),
                content_id: "waterborne_dross_subunit".into(),
                units: parcel_carrier.dross_subunits,
                expected_version: source_carrier.dross_subunits,
            });
        }
        let _ = destination_after;
        self.reserve_wand_effect(
            actor,
            actor_label,
            source,
            wand_id,
            working_id,
            targets,
            physical,
            effect,
            u32::try_from(water_hu / crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL)
                .map_err(|_| "Draw magnitude overflowed.")?,
            working_distance(from, to),
            0,
            forced,
        )
    }
}
