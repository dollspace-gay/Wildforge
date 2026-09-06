//! Fragile custody workings transaction coordination.

use crate::arcane::ArcaneOwner;
use crate::planet::BlockPos;
use crate::arcane::Current;
use crate::arcane::LinkedFileReplacement;
use crate::workings::PreservationKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingPhase;
use crate::workings::WorkingTargetSnapshot;
use crate::world::World;

impl World {
    /// Leak a charged botanical specimen through the ordinary finite Current
    /// ledger. Holdfast quarters—not reverses—the real leak and consumes only
    /// the portion of its reservation corresponding to elapsed work.
    pub fn leak_fragile_item_charge(
        &mut self,
        actor: [u8; 16],
        slot: usize,
        stack: crate::inventory::ItemStack,
        at: BlockPos,
        elapsed_seconds: u32,
    ) -> Result<u64, String> {
        if elapsed_seconds == 0 || stack.arcane_id == 0 || stack.count != 1 {
            return Ok(0);
        }
        let (stability_permille, botanical) = {
            let definition = self.reg.item(stack.item);
            let Some(arcane) = definition.arcane.as_ref() else {
                return Ok(0);
            };
            (arcane.stability_permille, definition.places.is_some())
        };
        if !botanical {
            return Ok(0);
        }
        let instability = u64::from(1_000u16.saturating_sub(stability_permille));
        let ordinary = instability
            .saturating_mul(u64::from(elapsed_seconds))
            .div_ceil(2_000)
            .max(1);
        let candidate = self.workings_state.as_ref().and_then(|state| {
            state.active.values().find_map(|transaction| {
                let WorkingEffect::Preserve {
                    item_id,
                    preservation_kind: PreservationKind::ChargeLeakage,
                    ..
                } = transaction.effect
                else {
                    return None;
                };
                if transaction.actor != actor
                    || transaction.phase != WorkingPhase::Active
                    || item_id != stack.arcane_id
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
                    } if *stable_id == item_id && *version == slot as u64 => {
                        Some((item_name.as_str(), *durability))
                    }
                    _ => None,
                })?;
                Some((transaction.id, target.0.to_string(), target.1))
            })
        });
        let active = if let Some((id, item_name, durability)) = candidate {
            if self.reg.item(stack.item).name != item_name || stack.durability != durability {
                let _ = self.interrupt_working(id);
                None
            } else {
                Some(id)
            }
        } else {
            None
        };
        let requested = if active.is_some() {
            ordinary.div_ceil(4)
        } else {
            ordinary
        };
        let temperature_millic = (self.weather_at_surface(at.surface()).temperature_c * 1_000.0)
            .round()
            .clamp(i32::MIN as f32, i32::MAX as f32) as i32;
        let requested =
            self.coated_specimen_age_advance(stack.arcane_id, requested, temperature_millic);
        let owner = ArcaneOwner::Item(stack.arcane_id);
        let (source_version, mut current) = match self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&owner))
        {
            Some(account) => (account.version, account.current.clone()),
            None => return Ok(0),
        };
        let amount = requested.min(current.total());
        if amount == 0 {
            return Ok(0);
        }
        let moved = current
            .take_units(amount, std::iter::empty())
            .map_err(|error| error.to_string())?;
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(at.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let destination = ArcaneOwner::Ambient(region);
        let mut next_state = self.workings_state.clone();
        let replacement = if let Some(id) = active {
            let state = next_state
                .as_mut()
                .ok_or("The Holdfast transaction state is unavailable.")?;
            let transaction = state
                .active
                .get_mut(&id)
                .ok_or("The Holdfast transaction ended during leakage.")?;
            let return_total = transaction.return_current.total();
            if let WorkingEffect::Preserve {
                elapsed_ticks,
                age_advance_ticks,
                charge_spent_units,
                ..
            } = &mut transaction.effect
            {
                *elapsed_ticks = elapsed_ticks.saturating_add(ordinary);
                *age_advance_ticks = age_advance_ticks.saturating_add(amount);
                *charge_spent_units = charge_spent_units
                    .saturating_add(
                        transaction
                            .definition
                            .charge_per_second
                            .saturating_mul(u64::from(elapsed_seconds)),
                    )
                    .min(return_total);
            }
            transaction.validate().map_err(|error| error.to_string())?;
            Some(LinkedFileReplacement {
                subsystem: "workings".into(),
                operation_id: id,
                relative_path: crate::workings::WORKINGS_FILE.into(),
                after: Some(state.encode().map_err(|error| error.to_string())?),
            })
        } else {
            None
        };
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The finite Current ledger is unavailable.")?;
        let transaction = crate::arcane::ArcaneTransaction::transfer(
            ledger
                .system_transaction_id()
                .map_err(|error| error.to_string())?,
            owner,
            source_version,
            destination.clone(),
            ledger.version_of(&destination),
            moved,
            crate::arcane::ArcaneAuthority::System,
            "ordinary charged botanical leakage",
        );
        if let Some(replacement) = replacement {
            ledger
                .commit_linked_files(transaction, vec![replacement])
                .map_err(|error| error.to_string())?;
            self.workings_state = next_state;
        } else {
            ledger
                .commit(transaction)
                .map_err(|error| error.to_string())?;
        }
        Ok(amount)
    }
}
