//! Experiments discovery transaction coordination.

use crate::discovery::ExperimentKind;
use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::world::BlockEntity;
use crate::world::World;

impl World {
    pub fn exchange_experiment_item_at(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
        slot: usize,
    ) -> Result<String, String> {
        let fixture = self
            .reg
            .block(self.get_block_at(pos))
            .discovery_fixture
            .as_ref()
            .filter(|fixture| fixture.kind == "experiment_apparatus")
            .ok_or("There is no comparative apparatus there.")?;
        if fixture.experiments.is_empty() {
            return Err("That apparatus has no configured trials.".into());
        }
        let selected = inventory
            .slots
            .get(slot)
            .copied()
            .ok_or("That pack slot does not exist.")?;
        if let Some(stack) = selected {
            let definition = self.reg.item(stack.item);
            let reference_kind = definition
                .discovery
                .as_ref()
                .filter(|discovery| discovery.kind == "reference_object")
                .and_then(|discovery| discovery.experiment);
            let is_sample = definition.observation.is_some()
                || definition.arcane.is_some()
                || matches!(
                    definition.name.as_str(),
                    "base:dirt"
                        | "base:grass"
                        | "base:bucket_water"
                        | "base:bucket_brackish"
                        | "base:bucket_salt"
                );
            if reference_kind.is_none() && !is_sample {
                return Err("That is neither a measurable sample nor a reference standard.".into());
            }
            let occupied = match self.installations.get(&pos) {
                Some(BlockEntity::DiscoveryApparatus(apparatus)) => {
                    if reference_kind.is_some() {
                        apparatus.reference.is_some()
                    } else {
                        apparatus.sample.is_some()
                    }
                }
                _ => false,
            };
            if occupied {
                return Err(if reference_kind.is_some() {
                    "Retrieve the installed reference before fitting another one.".into()
                } else {
                    "Retrieve the held sample before loading another one.".into()
                });
            }
            let mut physical = inventory
                .take_one_stack(slot)
                .ok_or("The selected item moved before it could be loaded.")?;
            if reference_kind.is_some()
                && let Err(error) = self.bind_discovery_stack_at(pos, &mut physical)
            {
                let _ = inventory.add_stack(&self.reg, physical);
                return Err(error.to_string());
            }
            let entity = self
                .installations
                .entry(pos)
                .or_insert_with(|| BlockEntity::DiscoveryApparatus(Default::default()));
            let BlockEntity::DiscoveryApparatus(apparatus) = entity else {
                let _ = inventory.add_stack(&self.reg, physical);
                return Err("Another block entity occupies the apparatus.".into());
            };
            if let Some(reference_kind) = reference_kind {
                apparatus.reference = Some(physical);
                Ok(format!(
                    "Installed the {} reference standard.",
                    reference_kind.label()
                ))
            } else {
                apparatus.sample = Some(physical);
                Ok("Loaded one physical sample into the apparatus holder.".into())
            }
        } else {
            let Some(BlockEntity::DiscoveryApparatus(apparatus)) = self.installations.get_mut(&pos)
            else {
                return Err("The apparatus bays are empty.".into());
            };
            let physical = apparatus
                .sample
                .take()
                .or_else(|| apparatus.reference.take())
                .ok_or("The apparatus bays are empty.")?;
            inventory.slots[slot] = Some(physical);
            Ok("Retrieved one physical apparatus item.".into())
        }
    }

    pub fn experiment_sample_at(
        &self,
        pos: BlockPos,
        kind: ExperimentKind,
    ) -> Result<ItemStack, String> {
        let Some(BlockEntity::DiscoveryApparatus(apparatus)) = self.installations.get(&pos) else {
            return Err("Load a sample and calibrated reference into the apparatus.".into());
        };
        let sample = apparatus
            .sample
            .ok_or("Load a physical sample into the apparatus holder.")?;
        let reference = apparatus
            .reference
            .ok_or("Install a calibrated reference in the apparatus.")?;
        let reference_kind = self
            .reg
            .item(reference.item)
            .discovery
            .as_ref()
            .and_then(|definition| definition.experiment);
        if reference_kind != Some(kind) || reference.arcane_id == 0 {
            return Err(format!(
                "The installed reference is not calibrated for the {}.",
                kind.label()
            ));
        }
        Ok(sample)
    }
}
