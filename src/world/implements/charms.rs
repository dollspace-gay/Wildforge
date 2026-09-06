//! Charms implements transaction coordination.

use super::add_current;
use super::charm_resonance_preference;
use super::physical_component;
use super::transaction_from_maps;
use crate::arcane::ArcaneOwner;
use crate::arcane::Current;
use crate::arcane::DrossMedium;
use crate::arcane::LinkedFileReplacement;
use crate::implements::ImplementAuditEvent;
use crate::implements::ImplementInstance;
use crate::implements::ImplementKind;
use crate::implements::STRUCTURAL_SPARK_UNITS;
use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::world::World;
use std::collections::BTreeMap;

impl World {
    /// Adopt a generated/legacy charm into durable implement metadata. Old
    /// charged accounts were already debited from finite Heart/Deep custody;
    /// an unbound pre-goal charm receives a one-time partial regional charge.
    pub fn ensure_charm_instance_at(
        &mut self,
        pos: BlockPos,
        stack: &mut ItemStack,
        reason: &str,
    ) -> Result<(), String> {
        let item = self.reg.item(stack.item);
        let Some(definition) = item.charm_def.clone() else {
            return Err("That item is not a declared charm.".into());
        };
        if stack.count != 1 {
            return Err("A charm must remain one stable physical instance.".into());
        }
        if stack.arcane_id != 0
            && self
                .implements_state
                .as_ref()
                .is_some_and(|state| state.instance(stack.arcane_id).is_some())
        {
            return Ok(());
        }
        let content_id = item.name.clone();
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let new_identity = stack.arcane_id == 0;
        let instance_id = if new_identity {
            ledger
                .allocate_item_id()
                .map_err(|error| error.to_string())?
        } else {
            stack.arcane_id
        };
        let item_owner = ArcaneOwner::Item(instance_id);
        let mut debits = BTreeMap::new();
        let mut credits = BTreeMap::new();
        let migrated_units = if new_identity {
            let ambient = ArcaneOwner::Ambient(region);
            let source = ledger
                .account(&ambient)
                .filter(|account| !account.current.is_empty())
                .map(|_| ambient)
                .unwrap_or(ArcaneOwner::Deep);
            let source_account = ledger
                .account(&source)
                .cloned()
                .ok_or("The finite migration reserve is unavailable.")?;
            let requested = definition.capacity.saturating_add(STRUCTURAL_SPARK_UNITS);
            let amount = requested.min(source_account.current.total());
            if amount == 0 {
                return Err("The finite migration reserve is empty.".into());
            }
            let mut available = source_account.current;
            let selected = available
                .take_units(amount, charm_resonance_preference(definition.effect))
                .map_err(|error| error.to_string())?;
            add_current(&mut debits, source, &selected).map_err(|error| error.to_string())?;
            add_current(&mut credits, item_owner.clone(), &selected)
                .map_err(|error| error.to_string())?;
            crate::implements::usable_charge(selected.total())
        } else {
            let account = ledger
                .account(&item_owner)
                .cloned()
                .ok_or("The charm names no extant finite Current account.")?;
            // Link pre-existing custody to the new sidecar record with a real
            // versioned no-op write; no charge is created or silently moved.
            let mut available = account.current.clone();
            let pulse = available
                .take_units(
                    STRUCTURAL_SPARK_UNITS.min(account.current.total()),
                    std::iter::empty(),
                )
                .map_err(|error| error.to_string())?;
            add_current(&mut debits, item_owner.clone(), &pulse)
                .map_err(|error| error.to_string())?;
            add_current(&mut credits, item_owner.clone(), &pulse)
                .map_err(|error| error.to_string())?;
            crate::implements::usable_charge(account.current.total())
        };

        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        next_state
            .charm_manifests
            .insert(content_id.clone(), definition.clone());
        next_state
            .insert(ImplementInstance {
                instance_id,
                content_id: content_id.clone(),
                kind: ImplementKind::Charm {
                    effect: definition.effect,
                    capacity: definition.capacity,
                    charge_per_trigger: definition.charge_per_trigger,
                    stability: definition.stability,
                    dross_per_transfer: definition.dross_per_transfer,
                },
                construction: vec![physical_component(&self.reg, *stack)],
                wear: 0,
                strain: 0,
                provenance: format!("legacy_or_generated:{pos:?}"),
                created_day: self.calendar_state.day(),
                format_version: crate::implements::IMPLEMENT_RESOLVER_VERSION,
                creative: self.mode == "creative",
            })
            .map_err(|error| error.to_string())?;
        let migration_key = format!("{content_id}:{instance_id}");
        next_state.migrated_legacy.insert(migration_key);
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "charm_migration".into(),
            instance_id,
            units: migrated_units,
            dross: 0,
            actor: "world-migration".into(),
            note: reason.into(),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &content_id,
            "one-time ledger-balanced legacy charm migration",
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
        stack.arcane_id = instance_id;
        Ok(())
    }

    pub fn charm_can_pay(&self, stack: ItemStack, kind: &str) -> bool {
        crate::world::item_presentation::charm_can_pay(
            &self.reg,
            stack,
            kind,
            self.inspectable_item_current(stack.arcane_id),
        )
    }

    pub fn implement_tooltip(&self, stack: ItemStack, exact: bool) -> Vec<String> {
        if stack.arcane_id == 0 {
            return Vec::new();
        }
        let clean = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.item_clean_total(stack.arcane_id))
            .unwrap_or(0);
        if let Some(instance) = self
            .implements_state
            .as_ref()
            .and_then(|state| state.instance(stack.arcane_id))
        {
            let dross = self
                .arcane_ledger
                .as_ref()
                .map_or(0, |ledger| ledger.item_dross_total(stack.arcane_id));
            crate::implements::tooltip(instance, clean, dross, exact)
        } else {
            Vec::new()
        }
    }

    /// Spend one charm trigger into a declared regional dross reservoir. A
    /// failed debit returns false and callers must not grant the benefit.
    pub fn debit_charm_at(
        &mut self,
        pos: BlockPos,
        stack: &mut ItemStack,
        kind: &str,
        reason: &str,
    ) -> bool {
        if self.ensure_charm_instance_at(pos, stack, reason).is_err() {
            return false;
        }
        let Some(definition) = self.reg.item(stack.item).charm_def.clone() else {
            return false;
        };
        if definition.effect.id() != kind {
            return false;
        }
        let Some(region) = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
        else {
            return false;
        };
        let owner = ArcaneOwner::Item(stack.arcane_id);
        let dross_destination = ArcaneOwner::Dross {
            region,
            medium: match definition.effect {
                crate::implements::CharmEffect::Quiet => DrossMedium::Air,
                crate::implements::CharmEffect::Bark => DrossMedium::Soil,
                crate::implements::CharmEffect::Hunger => DrossMedium::Water,
            },
        };
        let Some(ledger) = self.arcane_ledger.as_mut() else {
            return false;
        };
        let Some(account) = ledger.account(&owner).cloned() else {
            return false;
        };
        if crate::implements::usable_charge(account.current.total()) < definition.charge_per_trigger
        {
            return false;
        }
        let mut selected_from = account.current.clone();
        let Ok(mut selected) = selected_from.take_units(
            definition.charge_per_trigger,
            charm_resonance_preference(definition.effect),
        ) else {
            return false;
        };
        // Never spend the structural spark, even if a mod's resonance
        // preference happens to name its band first.
        if selected_from.total() < STRUCTURAL_SPARK_UNITS {
            return false;
        }
        let dross_units = definition
            .charge_per_trigger
            .saturating_mul(u64::from(definition.dross_per_transfer))
            .div_ceil(1_000)
            .min(definition.charge_per_trigger);
        let dross = if dross_units == 0 {
            Current::default()
        } else {
            match selected.take_units(dross_units, std::iter::empty()) {
                Ok(dross) => dross,
                Err(_) => return false,
            }
        };
        let mut total_debit = selected.clone();
        if total_debit.checked_add(&dross).is_err() {
            return false;
        }
        let Some(mut next_state) = self.implements_state.clone() else {
            return false;
        };
        let Ok(operation_id) = next_state.operation_id() else {
            return false;
        };
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: format!("charm_{}", definition.effect.id()),
            instance_id: stack.arcane_id,
            units: definition.charge_per_trigger,
            dross: dross.total(),
            actor: "authoritative-survival".into(),
            note: reason.into(),
        });
        let mut debits = BTreeMap::new();
        let mut credits = BTreeMap::new();
        if add_current(&mut debits, owner, &total_debit).is_err()
            || !selected.is_empty()
                && add_current(&mut credits, ArcaneOwner::Ambient(region), &selected).is_err()
            || !dross.is_empty() && add_current(&mut credits, dross_destination, &dross).is_err()
        {
            return false;
        }
        let Ok(transaction) = transaction_from_maps(
            ledger,
            debits,
            credits,
            &self.reg.item(stack.item).name,
            reason,
        ) else {
            return false;
        };
        let replacement = LinkedFileReplacement {
            subsystem: "implements".into(),
            operation_id,
            relative_path: crate::implements::IMPLEMENTS_FILE.into(),
            after: match next_state.encode() {
                Ok(bytes) => Some(bytes),
                Err(_) => return false,
            },
        };
        if ledger
            .commit_linked_files(transaction, vec![replacement])
            .is_err()
        {
            return false;
        }
        self.implements_state = Some(next_state);
        true
    }
}
