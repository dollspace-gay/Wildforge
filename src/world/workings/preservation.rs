//! Preservation workings transaction coordination.

use super::inventory_target_id;
use super::mounted_target_id;
use crate::planet::BlockPos;
use crate::workings::PreservationKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingPhase;
use crate::workings::WorkingTargetSnapshot;
use crate::world::World;

impl World {
    /// Apply one ordinary aging sweep to a carried target. The caller still
    /// performs the real freshness/age decrement; this method merely returns
    /// the accounted slowed amount and advances the durable monotonic age
    /// evidence. A moved or replaced target receives no benefit.
    pub fn holdfast_age_step(
        &mut self,
        actor: [u8; 16],
        slot: usize,
        stack: crate::inventory::ItemStack,
        ordinary_step: u32,
        elapsed_seconds: u32,
    ) -> u32 {
        self.holdfast_age_step_for(
            Some(actor),
            inventory_target_id(actor, slot, stack.item.0),
            slot as u64,
            stack,
            ordinary_step,
            elapsed_seconds,
        )
    }

    /// Mounted samples age on the world's ordinary container clock. The mount
    /// and bay form a stable physical identity, so moving or replacing the
    /// sample invalidates the benefit exactly like moving an inventory target.
    pub(crate) fn holdfast_mounted_age_step(
        &mut self,
        mount: BlockPos,
        bay: u8,
        stack: crate::inventory::ItemStack,
        ordinary_step: u32,
        elapsed_seconds: u32,
    ) -> u32 {
        self.holdfast_age_step_for(
            None,
            mounted_target_id(mount, bay, stack.item.0),
            u64::from(bay),
            stack,
            ordinary_step,
            elapsed_seconds,
        )
    }

    pub(super) fn holdfast_age_step_for(
        &mut self,
        actor: Option<[u8; 16]>,
        expected_id: u64,
        expected_version: u64,
        stack: crate::inventory::ItemStack,
        ordinary_step: u32,
        elapsed_seconds: u32,
    ) -> u32 {
        if ordinary_step == 0 {
            return 0;
        }
        let candidate = self.workings_state.as_ref().and_then(|state| {
            state.active.values().find_map(|transaction| {
                let WorkingEffect::Preserve {
                    item_id,
                    preservation_kind: PreservationKind::ElapsedAge,
                    age_advance_ticks,
                    ..
                } = transaction.effect
                else {
                    return None;
                };
                if actor.is_some_and(|actor| transaction.actor != actor)
                    || transaction.phase != WorkingPhase::Active
                    || item_id != expected_id
                {
                    return None;
                }
                let target = transaction.targets.iter().find_map(|target| match target {
                    WorkingTargetSnapshot::Item {
                        stable_id,
                        item_name,
                        durability,
                        version,
                        ..
                    } if *stable_id == item_id && *version == expected_version => {
                        Some((item_name.as_str(), *durability))
                    }
                    _ => None,
                })?;
                Some((
                    transaction.id,
                    target.0.to_string(),
                    target.1,
                    age_advance_ticks,
                ))
            })
        });
        let Some((id, item_name, initial_durability, prior_advance)) = candidate else {
            return ordinary_step;
        };
        if self.reg.item(stack.item).name != item_name
            || stack.durability
                != initial_durability.saturating_sub(prior_advance.min(u64::from(u32::MAX)) as u32)
        {
            let _ = self.interrupt_working(id);
            return ordinary_step;
        }
        let slowed = ordinary_step.div_ceil(4).max(1);
        if let Some(state) = self.workings_state.as_mut()
            && let Some(transaction) = state.active.get_mut(&id)
            && let WorkingEffect::Preserve {
                elapsed_ticks,
                age_advance_ticks,
                charge_spent_units,
                ..
            } = &mut transaction.effect
        {
            *elapsed_ticks = elapsed_ticks.saturating_add(u64::from(ordinary_step));
            *age_advance_ticks = age_advance_ticks.saturating_add(u64::from(slowed));
            *charge_spent_units = charge_spent_units
                .saturating_add(
                    transaction
                        .definition
                        .charge_per_second
                        .saturating_mul(u64::from(elapsed_seconds)),
                )
                .min(transaction.return_current.total());
            if state.save().is_err() {
                return ordinary_step;
            }
        }
        slowed
    }
}
