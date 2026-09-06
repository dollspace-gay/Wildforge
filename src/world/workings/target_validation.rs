//! Target validation workings transaction coordination.

use crate::world::BlockEntity;
use crate::planet::BlockPos;
use crate::arcane::Current;
use crate::workings::NudgeEntityKind;
use crate::workings::WorkingApparatus;
use crate::workings::WorkingEffect;
use crate::workings::WorkingHandler;
use crate::workings::WorkingTargetSnapshot;
use crate::workings::WorkingTransaction;
use crate::world::World;
use super::carrier_from_snapshots;
use super::reservoir_from_snapshots;
use super::vec3_milli;

impl World {
    pub(super) fn validate_saved_world_targets(&self, transaction: &WorkingTransaction) -> Result<(), String> {
        for target in &transaction.targets {
            match target {
                WorkingTargetSnapshot::Block {
                    pos,
                    block_name,
                    metadata,
                    ..
                } => {
                    if self.reg.block(self.get_block_at(*pos)).name != *block_name
                        || self.get_meta_at(*pos) != *metadata
                    {
                        return Err(format!(
                            "A reserved ritual block at {pos:?} changed before completion."
                        ));
                    }
                }
                WorkingTargetSnapshot::Reservoir {
                    pos,
                    water_hu,
                    salt_mass,
                    thermal_millic_hu,
                    dross_units,
                    carrier_remainder,
                    ..
                } => {
                    let actual = self.water_mass_at(*pos).unwrap_or_default();
                    let expected_carrier = crate::workings::WaterCarrier {
                        thermal_millic_hu: *thermal_millic_hu,
                        dross_subunits: dross_units
                            .checked_mul(256)
                            .and_then(|units| units.checked_add(*carrier_remainder))
                            .ok_or("A ritual reservoir carrier snapshot overflowed.")?,
                    };
                    if actual.water_hu != *water_hu
                        || actual.salt_mass != *salt_mass
                        || (transaction.definition.handler == WorkingHandler::Draw
                            && self.water_carrier_at(*pos, actual) != expected_carrier)
                    {
                        return Err(format!(
                            "A reserved ritual reservoir at {pos:?} changed before completion."
                        ));
                    }
                }
                WorkingTargetSnapshot::Area { .. }
                | WorkingTargetSnapshot::Item { .. }
                | WorkingTargetSnapshot::Entity { .. } => {}
            }
        }
        Ok(())
    }

    pub(super) fn validate_ritual_apparatus(&self, transaction: &WorkingTransaction) -> Result<(), String> {
        let WorkingApparatus::Ritual {
            controller,
            expected_revision,
        } = transaction.apparatus
        else {
            return Ok(());
        };
        if self.binding_frame_revision(controller)? != expected_revision {
            return Err("The ritual controller changed while Current was reserved.".into());
        }
        let layout = self.binding_frame_layout(controller);
        if !layout.valid {
            return Err(format!(
                "The ritual's visible containment path is broken: {}",
                layout.problems.join(" ")
            ));
        }
        let area_cells = transaction.targets.iter().find_map(|target| match target {
            WorkingTargetSnapshot::Area {
                controller: at,
                cells,
                ..
            } if *at == controller => Some(cells.as_slice()),
            _ => None,
        });
        for target in &transaction.targets {
            let WorkingTargetSnapshot::Item {
                stable_id,
                durability,
                version,
                ..
            } = target
            else {
                continue;
            };
            let present = area_cells.into_iter().flatten().any(|pos| {
                matches!(
                    self.block_entity_at(pos),
                    Some(BlockEntity::ChargeVessel(vessel))
                        if vessel.revision == *version
                            && vessel.vessel.is_some_and(|stack| {
                                stack.arcane_id == *stable_id && stack.durability == *durability
                            })
                )
            });
            if !present {
                return Err(
                    "A ritual vessel was moved, damaged, or replaced before completion.".into(),
                );
            }
        }
        Ok(())
    }

    pub(super) fn validate_kindle_target(&self, fuel: BlockPos, fire_cell: BlockPos) -> Result<(), String> {
        if !crate::planet::neighbors6(fuel).any(|neighbor| neighbor == fire_cell)
            || self.reg.block(self.get_block_at(fuel)).burns == 0
            || !self.reg.is_replaceable(self.get_block_at(fire_cell))
        {
            return Err(
                "Kindle needs one adjacent dry combustible and a replaceable fire cell.".into(),
            );
        }
        let weather = self.weather_at_surface(fuel.surface());
        if weather.kind.precipitating() && self.light_at_pos(fuel).1 == 15 {
            return Err("The exposed fuel is too wet to take ordinary fire.".into());
        }
        if crate::planet::neighbors6(fuel)
            .any(|neighbor| self.reg.is_water(self.get_block_at(neighbor)))
        {
            return Err("Waterlogged fuel refuses Kindle.".into());
        }
        Ok(())
    }

    pub(super) fn validate_wand_line_of_sight(
        &self,
        source: BlockPos,
        effect: &WorkingEffect,
        range: u16,
    ) -> Result<(), String> {
        let target = match effect {
            WorkingEffect::Observe { origin, .. } => Some(*origin),
            WorkingEffect::PointLight { target, .. } => Some(*target),
            WorkingEffect::Ignite { fuel, .. } => Some(*fuel),
            WorkingEffect::Impulse { target, .. }
            | WorkingEffect::OperateMechanism { target, .. } => Some(*target),
            WorkingEffect::AdvancePlant(advance) => Some(advance.pos),
            WorkingEffect::TransferWater { from, .. } => Some(*from),
            WorkingEffect::RepairItem { .. } | WorkingEffect::Preserve { .. } => None,
            WorkingEffect::Settle { .. }
            | WorkingEffect::AdvanceBed { .. }
            | WorkingEffect::Ward { .. }
            | WorkingEffect::TransferCurrent { .. } => {
                return Err("A ritual effect cannot be channeled through a held wand.".into());
            }
        };
        let Some(target) = target else {
            return Ok(());
        };
        if target == source {
            return Ok(());
        }
        let origin = source.entity_center();
        let delta = origin.local_delta_to(target.entity_center());
        let distance = delta.length();
        if !distance.is_finite() || distance > f32::from(range) + 0.75 {
            return Err("The target is outside this working's bounded reach.".into());
        }
        if crate::raycast::raycast_at(self, origin, delta, (distance - 0.65).max(0.0)).is_some() {
            return Err("Solid terrain breaks the wand's line of sight.".into());
        }
        Ok(())
    }

    /// Revalidate a live wand channel from the actor's current physical cell.
    /// Inventory workings move with their carried targets; spatial workings
    /// end when the actor leaves bounded reach or a wall breaks the path.
    pub fn wand_working_reachable_from(&self, id: u64, source: BlockPos) -> bool {
        let Some(transaction) = self
            .workings_state
            .as_ref()
            .and_then(|state| state.active.get(&id))
        else {
            return false;
        };
        if !matches!(transaction.apparatus, WorkingApparatus::Wand { .. }) {
            return true;
        }
        self.validate_wand_line_of_sight(source, &transaction.effect, transaction.definition.range)
            .is_ok()
    }

    pub(super) fn validate_working_effect_before(&self, effect: &WorkingEffect) -> Result<(), String> {
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
                ..
            } => {
                if self.get_block_at(*fuel).0 != *expected_fuel
                    || self.get_block_at(*fire_cell).0 != *expected_air
                {
                    return Err(
                        "Kindle target changed before release; the working remains reserved."
                            .into(),
                    );
                }
                self.validate_kindle_target(*fuel, *fire_cell)
            }
            WorkingEffect::AdvancePlant(advance) => {
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
                        "Rootwake target or reserved biological inputs changed before release."
                            .into(),
                    );
                }
                Ok(())
            }
            WorkingEffect::TransferWater { from, to, .. } => {
                let snapshots = self
                    .workings_state
                    .as_ref()
                    .into_iter()
                    .flat_map(|state| state.active.values())
                    .find(|transaction| &transaction.effect == effect)
                    .map(|transaction| &transaction.targets)
                    .ok_or("Draw lost its exact target snapshots.")?;
                let source = reservoir_from_snapshots(snapshots, *from)
                    .ok_or("Draw source snapshot is missing.")?;
                let destination = reservoir_from_snapshots(snapshots, *to)
                    .ok_or("Draw destination snapshot is missing.")?;
                let source_carrier = carrier_from_snapshots(snapshots, *from)
                    .ok_or("Draw source carrier snapshot is missing.")?;
                let destination_carrier = carrier_from_snapshots(snapshots, *to)
                    .ok_or("Draw destination carrier snapshot is missing.")?;
                if self.water_mass_at(*from) != Some(source)
                    || self.water_mass_at(*to).unwrap_or_default() != destination
                    || self.water_carrier_at(*from, source) != source_carrier
                    || self.water_carrier_at(*to, destination) != destination_carrier
                {
                    return Err("Draw source or destination changed before release.".into());
                }
                Ok(())
            }
            WorkingEffect::Impulse {
                entity_id,
                entity_kind,
                before_velocity_milli,
                expected_version,
                ..
            } => {
                let velocity = match entity_kind {
                    NudgeEntityKind::Projectile => self
                        .projectiles()
                        .iter()
                        .find(|projectile| projectile.stable_id == *entity_id)
                        .map(|projectile| vec3_milli(projectile.vel)),
                    NudgeEntityKind::DroppedItem => self
                        .loose_items()
                        .iter()
                        .find(|item| item.stable_id == *entity_id)
                        .map(|item| vec3_milli(item.vel)),
                }
                .ok_or("The Nudge target left the host simulation before release.")?;
                if *entity_id != *expected_version || velocity != *before_velocity_milli {
                    return Err("The Nudge target changed before release.".into());
                }
                Ok(())
            }
            WorkingEffect::OperateMechanism {
                target,
                block_name,
                before_state,
                ..
            } => {
                if self.reg.block(self.get_block_at(*target)).name != *block_name
                    || !self.is_nudge_mechanism_at(*target)
                    || !matches!(
                        self.block_entity_at(target),
                        Some(BlockEntity::Steam(steam))
                            if u8::from(steam.draft_closed) == *before_state
                    )
                {
                    return Err("The Nudge mechanism changed before release.".into());
                }
                Ok(())
            }
            WorkingEffect::AdvanceBed { .. } | WorkingEffect::Ward { .. } => {
                let transaction = self
                    .workings_state
                    .as_ref()
                    .into_iter()
                    .flat_map(|state| state.active.values())
                    .find(|transaction| &transaction.effect == effect)
                    .ok_or("The ritual lost its exact target snapshots.")?;
                self.validate_saved_world_targets(transaction)
            }
            WorkingEffect::Settle { .. } => Ok(()),
            _ => Err("That native effect needs its domain-specific completion adapter.".into()),
        }
    }
}
