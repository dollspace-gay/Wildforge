//! Charm migration implements transaction coordination.

use crate::world::BlockEntity;
use crate::planet::BlockPos;
use crate::inventory::ItemStack;
use crate::world::World;

impl World {
    /// Explicit one-time migration for player-owned charm locations. The
    /// caller persists the owning profile after this returns; every changed
    /// stack already has a finite ledger account and an idempotence/audit
    /// record in the implements sidecar.
    pub fn migrate_legacy_player_charms(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
        armor: &mut [Option<ItemStack>; 5],
        cursor: &mut Option<ItemStack>,
        owner: &str,
    ) -> usize {
        let mut migrated = 0;
        for slot in inventory
            .slots
            .iter_mut()
            .chain(armor.iter_mut())
            .chain(std::iter::once(cursor))
        {
            let Some(mut stack) = *slot else { continue };
            if self.reg.item(stack.item).charm_def.is_none() {
                continue;
            }
            let before = stack.arcane_id;
            let had_state = self
                .implements_state
                .as_ref()
                .is_some_and(|state| state.instance(before).is_some());
            if self
                .ensure_charm_instance_at(
                    pos,
                    &mut stack,
                    &format!("explicit planetary save migration for {owner}"),
                )
                .is_ok()
            {
                *slot = Some(stack);
                if before != stack.arcane_id || !had_state {
                    migrated += 1;
                }
            }
        }
        migrated
    }

    /// Migrate every charm already resident in a persisted block entity.
    /// Entities are temporarily detached so ledger/state mutation never
    /// aliases their inventory slots.
    pub(in crate::world) fn migrate_loaded_entity_charms(&mut self) {
        let positions: Vec<BlockPos> = self.installations.keys().copied().collect();
        let mut changed = false;
        for pos in positions {
            let Some(mut entity) = self.installations.remove(&pos) else {
                continue;
            };
            let mut migrate = |slot: &mut Option<ItemStack>| {
                let Some(mut stack) = *slot else { return };
                if self.reg.item(stack.item).charm_def.is_none() {
                    return;
                }
                let before = stack.arcane_id;
                let had_state = self
                    .implements_state
                    .as_ref()
                    .is_some_and(|state| state.instance(before).is_some());
                if self
                    .ensure_charm_instance_at(
                        pos,
                        &mut stack,
                        "explicit planetary block-entity charm migration",
                    )
                    .is_ok()
                {
                    *slot = Some(stack);
                    changed |= before != stack.arcane_id || !had_state;
                }
            };
            match &mut entity {
                BlockEntity::Furnace(state) => {
                    for slot in [&mut state.input, &mut state.fuel, &mut state.output] {
                        migrate(slot);
                    }
                }
                BlockEntity::Chest(state) => state.slots.iter_mut().for_each(&mut migrate),
                BlockEntity::Offering(state) => state.slots.iter_mut().for_each(&mut migrate),
                BlockEntity::Multiblock(state) => {
                    state.charge.iter_mut().for_each(&mut migrate);
                    migrate(&mut state.reagent);
                    state.fuel.iter_mut().for_each(&mut migrate);
                }
                BlockEntity::Anvil(state) => migrate(&mut state.bloom),
                BlockEntity::Stall(state) => {
                    state.goods.iter_mut().for_each(&mut migrate);
                    migrate(&mut state.price);
                    state.till.iter_mut().for_each(&mut migrate);
                }
                BlockEntity::Smoker(state) => state.meat.iter_mut().for_each(&mut migrate),
                BlockEntity::DiscoveryApparatus(state) => {
                    migrate(&mut state.sample);
                    migrate(&mut state.reference);
                }
                BlockEntity::BindingFrame(state) => {
                    for slot in [
                        &mut state.body,
                        &mut state.reservoir,
                        &mut state.focus,
                        &mut state.binding,
                        &mut state.output,
                    ] {
                        migrate(slot);
                    }
                }
                BlockEntity::ChargeVessel(state) => migrate(&mut state.vessel),
                BlockEntity::Clamp(_)
                | BlockEntity::Sign(_)
                | BlockEntity::Steam(_)
                | BlockEntity::SurveyFolio(_)
                | BlockEntity::Switch(_)
                | BlockEntity::Depot(_) => {}
            }
            self.installations.insert(pos, entity);
        }
        if changed && let Err(error) = self.save_entities() {
            eprintln!("implements: migrated block-entity charms could not be saved: {error}");
        }
    }

    pub(in crate::world) fn migrate_loaded_mob_charms(&mut self) {
        let mut changed = false;
        for index in 0..self.population.mobs().len() {
            let pos = self.population.mobs()[index].pos.block();
            let Some(mut cargo) = self.population.mobs_mut()[index].cargo.take() else {
                continue;
            };
            if let Some(pos) = pos {
                for slot in cargo.iter_mut() {
                    let Some(mut stack) = *slot else { continue };
                    if self.reg.item(stack.item).charm_def.is_none() {
                        continue;
                    }
                    let before = stack.arcane_id;
                    let had_state = self
                        .implements_state
                        .as_ref()
                        .is_some_and(|state| state.instance(before).is_some());
                    if self
                        .ensure_charm_instance_at(
                            pos,
                            &mut stack,
                            "explicit planetary cargo charm migration",
                        )
                        .is_ok()
                    {
                        *slot = Some(stack);
                        changed |= before != stack.arcane_id || !had_state;
                    }
                }
            }
            self.population.mobs_mut()[index].cargo = Some(cargo);
        }
        if changed {
            for failure in self.save_mobs() {
                eprintln!("implements: migrated cargo charms could not be saved: {failure:?}");
            }
        }
    }
}
