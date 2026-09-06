//! Physical effects workings transaction coordination.

use crate::world::BlockEntity;
use crate::world::BlockId;
use crate::world::BlockPos;
use crate::workings::NudgeEntityKind;
use crate::workings::PlantAdvance;
use crate::workings::WorkingEffect;
use crate::workings::WorkingTargetSnapshot;
use crate::world::World;
use super::carrier_from_snapshots;
use super::effect_path;
use super::milli_vec3;
use super::reservoir_from_snapshots;
use super::vec3_milli;

impl World {
    pub(super) fn apply_working_effect(
        &mut self,
        effect: &WorkingEffect,
        targets: &[WorkingTargetSnapshot],
    ) -> Result<(), String> {
        match effect {
            WorkingEffect::Observe { .. }
            | WorkingEffect::PointLight { .. }
            | WorkingEffect::Preserve { .. }
            | WorkingEffect::TransferCurrent { .. } => Ok(()),
            WorkingEffect::Ignite {
                fuel,
                fire_cell,
                expected_fuel,
                expected_air,
                player_caused,
            } => {
                if self
                    .reg
                    .block_id("base:fire")
                    .is_some_and(|fire| self.get_block_at(*fire_cell) == fire)
                {
                    return Ok(());
                }
                if self.get_block_at(*fuel).0 != *expected_fuel
                    || self.get_block_at(*fire_cell).0 != *expected_air
                {
                    return Err("Kindle replay found neither its before nor after state.".into());
                }
                self.validate_kindle_target(*fuel, *fire_cell)?;
                if self.light_fire_at(*fire_cell, *player_caused) {
                    Ok(())
                } else {
                    Err("Ordinary fire refused the Kindle effect.".into())
                }
            }
            WorkingEffect::AdvancePlant(advance) => self.apply_plant_advance(*advance),
            WorkingEffect::TransferWater {
                from,
                to,
                water_hu,
                salt_mass,
                temperature_millic,
                thermal_millic_hu,
                dross_units,
                dross_subunits,
                carrier_remainder_before,
                carrier_remainder_after,
            } => self.apply_draw(
                targets,
                *from,
                *to,
                *water_hu,
                *salt_mass,
                *temperature_millic,
                *thermal_millic_hu,
                *dross_units,
                *dross_subunits,
                *carrier_remainder_before,
                *carrier_remainder_after,
            ),
            WorkingEffect::Impulse {
                entity_id,
                entity_kind,
                before_velocity_milli,
                impulse_milli,
                expected_version,
                ..
            } => {
                let before = milli_vec3(*before_velocity_milli);
                let after = (before + milli_vec3(*impulse_milli)).clamp_length_max(40.0);
                let mut outcome = Err("Nudge replay could not find its transient target.".into());
                let mut apply = |stable_id: u64, velocity: &mut glam::Vec3| {
                    if stable_id != *entity_id || stable_id != *expected_version {
                        return;
                    }
                    let actual = vec3_milli(*velocity);
                    if actual == vec3_milli(after) {
                        outcome = Ok(());
                    } else if actual == *before_velocity_milli {
                        *velocity = after;
                        outcome = Ok(());
                    } else {
                        outcome = Err(
                            "Nudge replay found neither its exact before nor after velocity."
                                .into(),
                        );
                    }
                };
                match entity_kind {
                    NudgeEntityKind::Projectile => self.for_each_projectile_mut(|projectile| {
                        apply(projectile.stable_id, &mut projectile.vel)
                    }),
                    NudgeEntityKind::DroppedItem => {
                        self.for_each_loose_item_mut(|item| apply(item.stable_id, &mut item.vel))
                    }
                }
                outcome
            }
            WorkingEffect::OperateMechanism {
                target,
                block_name,
                before_state,
                after_state,
                ..
            } => {
                if self.reg.block(self.get_block_at(*target)).name != *block_name {
                    return Err("Nudge replay found a different physical mechanism.".into());
                }
                let Some(BlockEntity::Steam(steam)) = self.installations.get_mut(target) else {
                    return Err("Nudge replay found no embodied draft latch.".into());
                };
                let actual = u8::from(steam.draft_closed);
                if actual == *after_state {
                    return Ok(());
                }
                if actual != *before_state {
                    return Err(
                        "Nudge replay found neither the exact before nor after latch state.".into(),
                    );
                }
                steam.draft_closed = *after_state != 0;
                Ok(())
            }
            WorkingEffect::AdvanceBed { plants, .. } => {
                for advance in plants {
                    self.apply_plant_advance(*advance)?;
                }
                Ok(())
            }
            WorkingEffect::Settle { .. } | WorkingEffect::Ward { .. } => Ok(()),
            _ => Err(
                "That working must complete through its named entity/item/ritual adapter.".into(),
            ),
        }
    }

    pub(super) fn apply_plant_advance(&mut self, advance: PlantAdvance) -> Result<(), String> {
        let after_block = BlockId(advance.after_block);
        let plant_after = self.get_block_at(advance.pos) == after_block
            && self.get_meta_at(advance.pos) == advance.after_meta;
        let soil_after = advance
            .soil_pos
            .is_none_or(|soil| self.get_meta_at(soil) == advance.after_soil_meta);
        let water_after = advance.water_source.is_none_or(|water| {
            self.water_mass_at(water).is_some_and(|mass| {
                mass.water_hu == advance.water_after_hu && mass.salt_mass == advance.salt_after
            })
        });
        if plant_after && soil_after && water_after {
            return Ok(());
        }
        if self.get_block_at(advance.pos).0 != advance.before_block
            || self.get_meta_at(advance.pos) != advance.before_meta
            || advance
                .soil_pos
                .is_some_and(|soil| self.get_meta_at(soil) != advance.before_soil_meta)
            || advance.water_source.is_some_and(|water| {
                self.water_mass_at(water).is_none_or(|mass| {
                    mass.water_hu != advance.water_before_hu
                        || mass.salt_mass != advance.salt_before
                })
            })
        {
            return Err(
                "Rootwake replay found neither its complete before nor after state.".into(),
            );
        }
        self.set_block_meta_at(advance.pos, after_block, advance.after_meta);
        if let Some(soil) = advance.soil_pos {
            let block = self.get_block_at(soil);
            self.set_block_meta_at(soil, block, advance.after_soil_meta);
        }
        if let Some(water) = advance.water_source {
            self.write_water_mass_at(
                water,
                crate::planet_atlas::ReservoirMass {
                    water_hu: advance.water_after_hu,
                    salt_mass: advance.salt_after,
                },
            );
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_draw(
        &mut self,
        targets: &[WorkingTargetSnapshot],
        from: BlockPos,
        to: BlockPos,
        water_hu: u64,
        salt_mass: u64,
        temperature_millic: i32,
        thermal_millic_hu: i64,
        dross_units: u64,
        dross_subunits: u64,
        carrier_remainder_before: u64,
        carrier_remainder_after: u64,
    ) -> Result<(), String> {
        let source_before =
            reservoir_from_snapshots(targets, from).ok_or("Draw source snapshot is missing.")?;
        let destination_before =
            reservoir_from_snapshots(targets, to).ok_or("Draw destination snapshot is missing.")?;
        let source_carrier_before = carrier_from_snapshots(targets, from)
            .ok_or("Draw source carrier snapshot is missing.")?;
        let destination_carrier_before = carrier_from_snapshots(targets, to)
            .ok_or("Draw destination carrier snapshot is missing.")?;
        let mut source_after = source_before;
        let parcel = source_after.take(water_hu);
        if parcel.water_hu != water_hu || parcel.salt_mass != salt_mass {
            return Err("Draw parcel disagrees with its conserved source snapshot.".into());
        }
        let destination_after = destination_before
            .checked_add(parcel)
            .filter(|mass| mass.water_hu <= crate::planet_atlas::HYDRO_UNITS_PER_BLOCK)
            .ok_or("Draw destination overflows during replay.")?;
        let mut source_carrier_after = source_carrier_before;
        let parcel_carrier = source_carrier_after
            .take(source_before.water_hu, water_hu)
            .ok_or("Draw carrier parcel disagrees with its source snapshot.")?;
        if parcel_carrier.thermal_millic_hu != thermal_millic_hu
            || parcel_carrier.temperature_millic(water_hu) != temperature_millic
            || parcel_carrier.dross_subunits != dross_subunits
            || parcel_carrier.dross_subunits / 256 != dross_units
            || source_carrier_before.dross_subunits % 256 != carrier_remainder_before
            || source_carrier_after.dross_subunits % 256 != carrier_remainder_after
        {
            return Err("Draw's typed effect disagrees with its exact carrier snapshots.".into());
        }
        let destination_carrier_after = destination_carrier_before
            .checked_add(parcel_carrier)
            .ok_or("Draw destination carrier overflows during replay.")?;
        let actual_source = self.water_mass_at(from).unwrap_or_default();
        let actual_destination = self.water_mass_at(to).unwrap_or_default();
        let actual_source_carrier = self.water_carrier_at(from, actual_source);
        let actual_destination_carrier = self.water_carrier_at(to, actual_destination);
        if actual_source == source_after
            && actual_destination == destination_after
            && actual_source_carrier == source_carrier_after
            && actual_destination_carrier == destination_carrier_after
        {
            return Ok(());
        }
        if ![source_before, source_after].contains(&actual_source)
            || ![destination_before, destination_after].contains(&actual_destination)
            || ![source_carrier_before, source_carrier_after].contains(&actual_source_carrier)
            || ![destination_carrier_before, destination_carrier_after]
                .contains(&actual_destination_carrier)
        {
            return Err("Draw replay found neither its complete before nor after state.".into());
        }
        if actual_destination != destination_after {
            self.write_water_mass_at(to, destination_after);
        }
        if actual_source != source_after {
            self.write_water_mass_at(from, source_after);
        }
        self.set_water_carrier_at(to, destination_after, destination_carrier_after)?;
        self.set_water_carrier_at(from, source_after, source_carrier_after)?;
        Ok(())
    }

    pub(super) fn save_effect_chunks(&self, effect: &WorkingEffect) -> Result<(), String> {
        let mut chunks = effect_path(effect)
            .into_iter()
            .map(BlockPos::chunk)
            .collect::<Vec<_>>();
        chunks.sort();
        chunks.dedup();
        for chunk in chunks {
            self.save_chunk(chunk).map_err(|error| {
                format!("working effect chunk {chunk:?} could not be persisted: {error}")
            })?;
        }
        Ok(())
    }
}
