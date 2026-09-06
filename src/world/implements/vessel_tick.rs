//! Vessel tick implements transaction coordination.

use crate::arcane::ArcaneOwner;
use std::collections::BTreeMap;
use crate::world::BlockEntity;
use crate::planet::BlockPos;
use crate::arcane::Current;
use crate::arcane::DrossMedium;
use crate::implements::ImplementAuditEvent;
use crate::implements::ImplementKind;
use crate::inventory::ItemStack;
use crate::arcane::LinkedFileReplacement;
use crate::world::World;
use super::add_current;
use super::all_neighbors;
use super::transaction_from_maps;

impl World {
    /// Bounded physical upkeep for placed charge vessels. The server calls
    /// this once per five seconds, so a no-magic world pays one cheap empty
    /// block-entity scan at that cadence rather than work on every 30 Hz tick.
    pub fn tick_implements(&mut self, cursor: &mut usize) {
        let mut vessel_positions: Vec<BlockPos> = self.installations
            .iter()
            .filter_map(|(&pos, entity)| {
                matches!(entity, BlockEntity::ChargeVessel(_)).then_some(pos)
            })
            .collect();
        vessel_positions.sort_unstable();
        if vessel_positions.is_empty() {
            *cursor = 0;
            return;
        }
        if *cursor >= vessel_positions.len() {
            *cursor = 0;
        }
        let end = (*cursor + crate::implements::MAX_CONDUCTOR_NETWORK).min(vessel_positions.len());
        let vessels: Vec<(BlockPos, ItemStack, u16)> = vessel_positions[*cursor..end]
            .iter()
            .filter_map(|pos| match self.installations.get(pos) {
                Some(BlockEntity::ChargeVessel(state)) => {
                    state.vessel.map(|stack| (*pos, stack, state.damage))
                }
                _ => None,
            })
            .collect();
        *cursor = if end == vessel_positions.len() {
            0
        } else {
            end
        };

        let mut entity_changed = false;
        for (pos, stack, old_damage) in vessels {
            if stack.arcane_id == 0 {
                continue;
            }
            let heat = all_neighbors(pos).into_iter().any(|at| {
                let block = self.get_block_at(at);
                self.reg.is_lava(block) || self.reg.block(block).name == "base:fire"
            });
            let mut damage = old_damage;
            if heat {
                let containment = self.vessel_containment_at(pos, stack.arcane_id);
                let heat_damage = 24u16.saturating_sub(containment.saturating_div(50)).max(2);
                damage = damage.saturating_add(heat_damage).min(1_000);
                if let Some(BlockEntity::ChargeVessel(state)) = self.installations.get_mut(&pos) {
                    state.damage = damage;
                    state.revision = state.revision.saturating_add(1);
                    entity_changed = true;
                }
            }

            let (capacity, strain) = self
                .implements_state
                .as_ref()
                .and_then(|state| state.instance(stack.arcane_id))
                .map_or((0, 0), |instance| {
                    (instance.usable_capacity(), instance.strain)
                });
            let usable = self
                .arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.item_clean_total(stack.arcane_id))
                .map(crate::implements::usable_charge)
                .unwrap_or(0);
            if damage >= 1_000
                || strain >= crate::implements::MAX_WAND_STRAIN
                || capacity != 0 && usable > capacity
            {
                if let Err(error) = self.fail_placed_vessel(
                    pos,
                    stack,
                    if usable > capacity {
                        "pressure exceeded finite vessel capacity"
                    } else if heat {
                        "heat fractured a strained vessel"
                    } else {
                        "vessel strain exceeded its physical limit"
                    },
                ) {
                    eprintln!("implements: vessel failure at {pos:?} could not settle: {error}");
                }
                entity_changed = true;
                continue;
            }
            if damage != 0
                && let Err(error) = self.leak_placed_vessel(pos, stack, damage)
            {
                eprintln!("implements: vessel leak at {pos:?} could not settle: {error}");
            }
        }
        if entity_changed && let Err(error) = self.save_entities() {
            eprintln!("implements: vessel state save failed: {error}");
        }
    }

    pub(super) fn vessel_containment_at(&self, pos: BlockPos, instance_id: u64) -> u16 {
        let inherent = self
            .implements_state
            .as_ref()
            .and_then(|state| state.instance(instance_id))
            .and_then(|instance| match &instance.kind {
                ImplementKind::Vessel { containment, .. } => Some(*containment),
                _ => None,
            })
            .unwrap_or(0);
        let arrangement = all_neighbors(pos).into_iter().fold(0u16, |score, at| {
            let name = self.reg.block(self.get_block_at(at)).name.as_str();
            score.saturating_add(match name {
                "base:containment_post" => 90,
                "base:still_salt" => 120,
                "base:hushwood" | "base:hushwood_log" => 45,
                _ => 0,
            })
        });
        inherent.saturating_add(arrangement).min(980)
    }

    pub(super) fn leak_placed_vessel(
        &mut self,
        pos: BlockPos,
        stack: ItemStack,
        damage: u16,
    ) -> Result<(), String> {
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let instance = next_state
            .instance(stack.arcane_id)
            .cloned()
            .ok_or("The vessel has no construction record.")?;
        let containment = self.vessel_containment_at(pos, stack.arcane_id);
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let clean_owner = ArcaneOwner::Item(stack.arcane_id);
        let dross_owner = ArcaneOwner::ItemDross(stack.arcane_id);
        let clean = ledger
            .account(&clean_owner)
            .map(|account| account.current.clone())
            .unwrap_or_default();
        let stored_dross = ledger
            .account(&dross_owner)
            .map(|account| account.current.clone())
            .unwrap_or_default();
        let risk = u64::from(damage)
            .saturating_add(u64::from(instance.strain) / 20)
            .min(1_000);
        let exposure = u64::from(1_000u16.saturating_sub(containment));
        let usable = crate::implements::usable_charge(clean.total());
        let leak_units = usable
            .saturating_mul(risk)
            .saturating_mul(exposure)
            .div_ceil(10_000_000)
            .min(usable);
        let dross_units = stored_dross
            .total()
            .saturating_mul(risk)
            .saturating_mul(exposure)
            .div_ceil(10_000_000)
            .min(stored_dross.total());
        if leak_units == 0 && dross_units == 0 {
            return Ok(());
        }

        let mut debits = BTreeMap::new();
        let mut credits = BTreeMap::new();
        let ambient = ArcaneOwner::Ambient(region);
        let air_dross = ArcaneOwner::Dross {
            region,
            medium: DrossMedium::Air,
        };
        let soil_dross = ArcaneOwner::Dross {
            region,
            medium: DrossMedium::Soil,
        };
        let mut released = clean;
        let mut released = released
            .take_units(leak_units, std::iter::empty())
            .map_err(|error| error.to_string())?;
        let fouled_units = leak_units.div_ceil(5).min(leak_units);
        let fouled = if fouled_units == 0 {
            Current::default()
        } else {
            released
                .take_units(fouled_units, std::iter::empty())
                .map_err(|error| error.to_string())?
        };
        let mut escaped_dross = stored_dross;
        let escaped_dross = escaped_dross
            .take_units(dross_units, std::iter::empty())
            .map_err(|error| error.to_string())?;
        let mut total_clean_debit = released.clone();
        total_clean_debit
            .checked_add(&fouled)
            .map_err(|error| error.to_string())?;
        if !total_clean_debit.is_empty() {
            add_current(&mut debits, clean_owner, &total_clean_debit)
                .map_err(|error| error.to_string())?;
        }
        if !escaped_dross.is_empty() {
            add_current(&mut debits, dross_owner, &escaped_dross)
                .map_err(|error| error.to_string())?;
        }
        if !released.is_empty() {
            add_current(&mut credits, ambient, &released).map_err(|error| error.to_string())?;
        }
        if !fouled.is_empty() {
            add_current(&mut credits, air_dross, &fouled).map_err(|error| error.to_string())?;
        }
        if !escaped_dross.is_empty() {
            add_current(&mut credits, soil_dross, &escaped_dross)
                .map_err(|error| error.to_string())?;
        }
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        if let Some(record) = next_state.instances.get_mut(&stack.arcane_id) {
            record.strain = record
                .strain
                .saturating_add(1)
                .min(crate::implements::MAX_WAND_STRAIN);
        }
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "vessel_leak".into(),
            instance_id: stack.arcane_id,
            units: released.total(),
            dross: fouled.total().saturating_add(escaped_dross.total()),
            actor: "world".into(),
            note: format!("damage {damage}; effective containment {containment}"),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &instance.content_id,
            "damaged charge vessel leakage",
        )?;
        let replacement = LinkedFileReplacement {
            subsystem: "implements".into(),
            operation_id,
            relative_path: crate::implements::IMPLEMENTS_FILE.into(),
            after: Some(next_state.encode().map_err(|error| error.to_string())?),
        };
        ledger
            .commit_linked_files(transaction, vec![replacement])
            .map_err(|error| error.to_string())?;
        self.implements_state = Some(next_state);
        if let Some(BlockEntity::ChargeVessel(state)) = self.installations.get_mut(&pos) {
            state.revision = state.revision.saturating_add(1);
        }
        Ok(())
    }
}
