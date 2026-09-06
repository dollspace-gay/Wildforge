//! Material transactions coordinator for the authoritative world.

use super::{BlockEntity, ItemStack, World};

impl World {
    /// Move material carried by physically consumed stacks into the explicit
    /// sink. Callers remove the gameplay objects; this keeps eating,
    /// offerings, composting, and spoilage from becoming hidden deletion
    /// paths for ingredients such as finite salt.
    pub fn record_consumed_stacks(
        &mut self,
        stacks: impl IntoIterator<Item = ItemStack>,
    ) -> std::io::Result<()> {
        let mut total = crate::registry::MaterialVector::new();
        for stack in stacks {
            for (material, units) in crate::materials::stack_materials(&self.reg, stack) {
                let stored = total.entry(material).or_default();
                *stored = stored.saturating_add(units);
            }
        }
        let Some(ledger) = &mut self.material_ledger else {
            return Ok(());
        };
        ledger.record_consumption(&total)
    }

    pub(super) fn complete_material_operation(
        &mut self,
        operation: &crate::materials::MaterialOperation,
    ) {
        // The voxel lands first. If the process stops after this write, the
        // pending operation replays exactly once on load. If the chunk write
        // fails, leave the journal unapplied: disk still owns the old voxel.
        if let Err(error) = self.save_chunk(operation.pos.chunk()) {
            eprintln!(
                "materials: tracked chunk write failed at {:?}; journal retained: {error}",
                operation.pos
            );
            return;
        }
        let Some(ledger) = &mut self.material_ledger else {
            return;
        };
        if let Err(error) = ledger.apply_operation(operation) {
            eprintln!(
                "materials: ledger operation {} failed: {error}",
                operation.id
            );
            return;
        }
        if let Err(error) = ledger.finish_operation() {
            eprintln!(
                "materials: operation {} committed but journal cleanup failed: {error}",
                operation.id
            );
        }
    }

    /// Account for a stack introduced by an explicit development/admin path.
    pub fn record_external_stack(&mut self, stack: ItemStack, source: &str) -> std::io::Result<()> {
        let reg = self.reg.clone();
        let Some(ledger) = &mut self.material_ledger else {
            return Ok(());
        };
        ledger.record_external_stack(&reg, stack, source)
    }

    /// Account for a stack overwritten by an explicit development/admin path.
    pub fn record_admin_stack_deletion(&mut self, stack: ItemStack) -> std::io::Result<()> {
        let reg = self.reg.clone();
        let Some(ledger) = &mut self.material_ledger else {
            return Ok(());
        };
        ledger.record_admin_stack_deletion(&reg, stack)
    }

    pub(crate) fn block_entity_stacks(entity: &BlockEntity) -> Vec<ItemStack> {
        let mut stacks = Vec::new();
        let mut add = |slots: &[Option<ItemStack>]| {
            stacks.extend(slots.iter().flatten().copied());
        };
        match entity {
            BlockEntity::Furnace(state) => {
                add(&[state.input, state.fuel, state.output]);
            }
            BlockEntity::Depot(d) => add(d.storage.as_ref()),
            BlockEntity::Chest(state) => add(&state.slots),
            BlockEntity::Offering(state) => add(&state.slots),
            BlockEntity::Multiblock(state) => {
                add(&state.charge);
                add(&[state.reagent]);
                add(&state.fuel);
            }
            BlockEntity::Anvil(state) => add(&[state.bloom]),
            BlockEntity::Stall(state) => {
                add(&state.goods);
                add(&[state.price]);
                add(&state.till);
            }
            BlockEntity::Smoker(state) => add(&state.meat),
            BlockEntity::DiscoveryApparatus(state) => add(&[state.sample, state.reference]),
            BlockEntity::BindingFrame(state) => {
                add(&state.mounts());
                add(&[state.output]);
            }
            BlockEntity::ChargeVessel(state) => add(&[state.vessel]),
            BlockEntity::Clamp(_)
            | BlockEntity::Sign(_)
            | BlockEntity::Steam(_)
            | BlockEntity::SurveyFolio(_)
            | BlockEntity::Switch(_) => {}
        }
        stacks
    }

    /// Account for all physical contents represented by a block entity,
    /// including separator counters and a forge's fractional secondary stock.
    pub fn record_external_block_entity_contents(
        &mut self,
        entity: &BlockEntity,
        source: &str,
    ) -> std::io::Result<()> {
        for stack in Self::block_entity_stacks(entity) {
            self.record_external_stack(stack, source)?;
        }
        if let BlockEntity::Multiblock(state) = entity {
            let Some(ledger) = &mut self.material_ledger else {
                return Ok(());
            };
            ledger.record_external_materials(&state.reclaim, true, source)?;
            let counts = [
                ("base:rare_earth_powder", state.powder),
                ("base:charcoal", state.separator_fuel),
                ("base:neodymium", state.neodymium),
                ("base:cerium", state.cerium),
            ];
            for (name, count) in counts {
                if count != 0
                    && let Some(item) = self.reg.item_id(name)
                {
                    self.record_external_stack(ItemStack::new(&self.reg, item, count), source)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn record_admin_block_entity_deletion(
        &mut self,
        entity: &BlockEntity,
    ) -> std::io::Result<()> {
        for stack in Self::block_entity_stacks(entity) {
            self.record_admin_stack_deletion(stack)?;
        }
        if let BlockEntity::Multiblock(state) = entity {
            let Some(ledger) = &mut self.material_ledger else {
                return Ok(());
            };
            ledger.record_admin_secondary_deletion(&state.reclaim)?;
            let counts = [
                ("base:rare_earth_powder", state.powder),
                ("base:charcoal", state.separator_fuel),
                ("base:neodymium", state.neodymium),
                ("base:cerium", state.cerium),
            ];
            for (name, count) in counts {
                if count != 0
                    && let Some(item) = self.reg.item_id(name)
                {
                    self.record_admin_stack_deletion(ItemStack::new(&self.reg, item, count))?;
                }
            }
        }
        Ok(())
    }
}
