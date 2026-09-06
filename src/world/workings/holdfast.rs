//! Holdfast workings transaction coordination.

use crate::arcane::ArcaneOwner;
use crate::world::BlockEntity;
use crate::planet::BlockPos;
use crate::workings::PhysicalDebit;
use crate::workings::PhysicalDebitKind;
use crate::workings::PreservationKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingResult;
use crate::workings::WorkingTargetSnapshot;
use crate::world::World;
use super::inventory_target_id;
use super::mounted_target_id;
use super::working_distance;

impl World {
    /// Begin continuous preservation of one exact carried fragile stack.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn begin_holdfast_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        inventory: &crate::inventory::Inventory,
        target_slot: usize,
        duration_ticks: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.begin_holdfast_working_definition(
            actor,
            actor_label,
            source,
            wand_id,
            "base:holdfast",
            inventory,
            target_slot,
            duration_ticks,
            forced,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_holdfast_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        inventory: &crate::inventory::Inventory,
        target_slot: usize,
        duration_ticks: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        let stack = inventory
            .slots
            .get(target_slot)
            .copied()
            .flatten()
            .ok_or("Holdfast's target slot is empty.")?;
        let definition = self.reg.item(stack.item);
        let ages = (definition.food.is_some() || definition.name.ends_with("_seed"))
            && definition.durability != 0;
        let leaks_charge = definition.arcane.is_some()
            && definition.places.is_some()
            && stack.arcane_id != 0
            && self
                .arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.account(&ArcaneOwner::Item(stack.arcane_id)))
                .is_some_and(|account| !account.current.is_empty());
        if (!ages && !leaks_charge) || stack.count != 1 {
            return Err(
                "Holdfast needs one declared perishable, seed, or charged botanical specimen."
                    .into(),
            );
        }
        let preservation_kind = if leaks_charge {
            PreservationKind::ChargeLeakage
        } else {
            PreservationKind::ElapsedAge
        };
        let item_id = if leaks_charge {
            stack.arcane_id
        } else {
            inventory_target_id(actor, target_slot, stack.item.0)
        };
        let elapsed = if ages {
            u64::from(definition.durability.saturating_sub(stack.durability))
        } else {
            0
        };
        self.reserve_wand_effect(
            actor,
            actor_label,
            source,
            wand_id,
            working_id,
            vec![WorkingTargetSnapshot::Item {
                stable_id: item_id,
                item_name: definition.name.clone(),
                durability: stack.durability,
                age_ticks: elapsed,
                version: target_slot as u64,
            }],
            vec![PhysicalDebit {
                kind: PhysicalDebitKind::ElapsedAge,
                source: format!("inventory:{target_slot}"),
                content_id: definition.name.clone(),
                units: duration_ticks.max(1),
                expected_version: target_slot as u64,
            }],
            WorkingEffect::Preserve {
                item_id,
                preservation_kind,
                before_age_ticks: elapsed,
                elapsed_ticks: 0,
                age_advance_ticks: 0,
                charge_spent_units: 0,
            },
            1,
            0,
            duration_ticks,
            forced,
        )
    }

    /// Preserve one real sample held by the discovery apparatus. This is a
    /// physical mount, not a remote inventory: ordinary mounted aging owns the
    /// actual durability decrement and consults this exact reservation.
    #[allow(clippy::too_many_arguments)]
    pub fn begin_holdfast_mounted_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        mount: BlockPos,
        duration_ticks: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        let (bay, stack) = self
            .mounted_fragile_at(mount)
            .ok_or("Holdfast needs one live perishable or seed in that physical sample mount.")?;
        let definition = self.reg.item(stack.item);
        let item_id = mounted_target_id(mount, bay, stack.item.0);
        let elapsed = u64::from(definition.durability.saturating_sub(stack.durability));
        self.reserve_wand_effect(
            actor,
            actor_label,
            source,
            wand_id,
            working_id,
            vec![
                self.block_snapshot(mount),
                WorkingTargetSnapshot::Item {
                    stable_id: item_id,
                    item_name: definition.name.clone(),
                    durability: stack.durability,
                    age_ticks: elapsed,
                    version: u64::from(bay),
                },
            ],
            vec![PhysicalDebit {
                kind: PhysicalDebitKind::ElapsedAge,
                source: format!("sample_mount:{mount:?}:{bay}"),
                content_id: definition.name.clone(),
                units: duration_ticks.max(1),
                expected_version: u64::from(bay),
            }],
            WorkingEffect::Preserve {
                item_id,
                preservation_kind: PreservationKind::ElapsedAge,
                before_age_ticks: elapsed,
                elapsed_ticks: 0,
                age_advance_ticks: 0,
                charge_spent_units: 0,
            },
            1,
            working_distance(source, mount),
            duration_ticks,
            forced,
        )
    }

    /// Guest-safe context discovery may identify the physical apparatus, but
    /// only the authoritative host can see and accept its mounted specimen.
    pub fn holdfast_mounted_target_at(&self, mount: BlockPos) -> bool {
        self.mounted_fragile_at(mount).is_some()
    }

    pub(super) fn mounted_fragile_at(&self, mount: BlockPos) -> Option<(u8, crate::inventory::ItemStack)> {
        let BlockEntity::DiscoveryApparatus(apparatus) = self.block_entity_at(&mount)? else {
            return None;
        };
        [apparatus.sample, apparatus.reference]
            .into_iter()
            .enumerate()
            .find_map(|(bay, stack)| {
                let stack = stack?;
                let definition = self.reg.item(stack.item);
                (stack.count == 1
                    && stack.durability != 0
                    && definition.durability != 0
                    && (definition.food.is_some() || definition.name.ends_with("_seed")))
                .then_some((bay as u8, stack))
            })
    }
}
