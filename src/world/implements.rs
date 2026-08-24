//! Authoritative world integration for physical magical implements.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::*;
use crate::arcane::{
    AccountRead, ArcaneAuthority, ArcaneMove, ArcaneOwner, ArcaneTransaction, Current, DrossMedium,
    LinkedFileReplacement,
};
use crate::implements::{
    ComponentRole, FrameAction, FrameLayout, FrameResult, ImplementAuditEvent, ImplementComponent,
    ImplementCue, ImplementInstance, ImplementKind, STRUCTURAL_SPARK_UNITS, VESSEL_CAPACITY,
    VESSEL_SAFE_TRANSFER,
};

const ASSEMBLY_CALIBRATION_UNITS: u64 = 16;
const VESSEL_INITIAL_CHARGE: u64 = 64;

impl World {
    /// Make a bounded amount of this cell's dense planetary Ambient Current
    /// available to sparse apparatus transactions. This is a custody handoff,
    /// not recharge: Geography loses exactly what the regional Ambient owner
    /// gains, with the geography files committed through the same journal.
    pub(super) fn ensure_regional_ambient_units(
        &mut self,
        region: crate::planet_atlas::AtlasPos,
        requested: u64,
        reason: &str,
    ) -> Result<(), String> {
        let owner = ArcaneOwner::Ambient(region);
        let present = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&owner))
            .map_or(0, |account| account.current.total());
        if present >= requested {
            return Ok(());
        }
        let missing = requested.saturating_sub(present);
        let (index, old_cell, old_sequence, old_exported, old_external_imported) = {
            let geography = self
                .arcane_geography
                .as_ref()
                .ok_or("The finite magical geography is unavailable.")?;
            let index = region.index(geography.manifest.side);
            (
                index,
                geography.dynamic.cells[index],
                geography.dynamic.ecology.event_sequence,
                geography.dynamic.ecology.exported,
                geography.dynamic.dross_state.external_imported,
            )
        };
        let moved = self
            .arcane_geography
            .as_mut()
            .ok_or("The finite magical geography is unavailable.")?
            .export_ambient_for_apparatus(region, missing)
            .map_err(|error| error.to_string())?;
        let operation_id = self
            .arcane_geography
            .as_ref()
            .expect("geography was checked")
            .dynamic
            .ecology
            .event_sequence
            .max(1);
        let rollback = |world: &mut World| {
            if let Some(geography) = world.arcane_geography.as_mut() {
                geography.dynamic.cells[index] = old_cell;
                geography.dynamic.ecology.event_sequence = old_sequence;
                geography.dynamic.ecology.exported = old_exported;
                geography.dynamic.dross_state.external_imported = old_external_imported;
            }
        };
        let (manifest, files) = match self
            .arcane_geography
            .as_ref()
            .expect("geography was checked")
            .linked_dynamic_replacements(&self.save_dir, operation_id)
        {
            Ok(prepared) => prepared,
            Err(error) => {
                rollback(self);
                return Err(error.to_string());
            }
        };
        if self.arcane_ledger.is_none() {
            rollback(self);
            return Err("The world has no Current ledger.".into());
        }
        let geography_owner = ArcaneOwner::Geography;
        let transaction_id = match self
            .arcane_ledger
            .as_mut()
            .expect("ledger was checked")
            .system_transaction_id()
        {
            Ok(id) => id,
            Err(error) => {
                rollback(self);
                return Err(error.to_string());
            }
        };
        let ledger = self.arcane_ledger.as_mut().expect("ledger was checked");
        let mut reads = vec![
            AccountRead {
                owner: geography_owner.clone(),
                expected_version: ledger.version_of(&geography_owner),
            },
            AccountRead {
                owner: owner.clone(),
                expected_version: ledger.version_of(&owner),
            },
        ];
        reads.sort_by(|a, b| a.owner.cmp(&b.owner));
        let transaction = ArcaneTransaction {
            id: transaction_id,
            reads,
            debits: vec![ArcaneMove {
                owner: geography_owner,
                current: moved.clone(),
                content_id: None,
            }],
            credits: vec![ArcaneMove {
                owner: owner.clone(),
                current: moved,
                content_id: Some("base:regional_ambient".into()),
            }],
            transforms: Vec::new(),
            authority: ArcaneAuthority::System,
            reason: reason.into(),
            content_id: "base:regional_ambient".into(),
            linked: Vec::new(),
        };
        if let Err(error) = ledger.commit_linked_files(transaction, files) {
            rollback(self);
            return Err(error.to_string());
        }
        self.arcane_geography
            .as_mut()
            .expect("geography survived linked commit")
            .accept_linked_manifest(manifest);
        let now = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&owner))
            .map_or(0, |account| account.current.total());
        if now < requested {
            return Err(format!(
                "The local Ambient Current supplied {now} of {requested} required units."
            ));
        }
        Ok(())
    }

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
                created_day: self.day,
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
    pub(super) fn migrate_loaded_entity_charms(&mut self) {
        let positions: Vec<BlockPos> = self.block_entities.keys().copied().collect();
        let mut changed = false;
        for pos in positions {
            let Some(mut entity) = self.block_entities.remove(&pos) else {
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
            self.block_entities.insert(pos, entity);
        }
        if changed && let Err(error) = self.save_entities() {
            eprintln!("implements: migrated block-entity charms could not be saved: {error}");
        }
    }

    pub(super) fn migrate_loaded_mob_charms(&mut self) {
        let mut changed = false;
        for index in 0..self.mobs.len() {
            let pos = self.mobs[index].pos.block();
            let Some(mut cargo) = self.mobs[index].cargo.take() else {
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
            self.mobs[index].cargo = Some(cargo);
        }
        if changed {
            for failure in self.save_mobs() {
                eprintln!("implements: migrated cargo charms could not be saved: {failure:?}");
            }
        }
    }

    pub fn charm_can_pay(&self, stack: ItemStack, kind: &str) -> bool {
        let definition = self.reg.item(stack.item);
        let Some(charm) = &definition.charm_def else {
            return false;
        };
        charm.effect.id() == kind
            && stack.arcane_id != 0
            && self
                .inspectable_item_current(stack.arcane_id)
                .is_some_and(|units| {
                    crate::implements::usable_charge(units) >= charm.charge_per_trigger
                })
    }

    pub fn implement_tooltip(&self, stack: ItemStack, exact: bool) -> Vec<String> {
        if stack.arcane_id == 0 {
            return Vec::new();
        }
        let clean = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.item_clean_total(stack.arcane_id))
            .or_else(|| self.remote_arcane_items.get(&stack.arcane_id).copied())
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
            self.remote_implements
                .get(&stack.arcane_id)
                .map_or_else(Vec::new, |state| state.tooltip(clean, exact))
        }
    }

    #[cfg(test)]
    pub fn set_remote_implements(&mut self, states: Vec<crate::implements::ImplementPublicState>) {
        self.remote_implements.clear();
        self.remote_implements.extend(
            states
                .into_iter()
                .filter(|state| state.instance_id != 0)
                .map(|state| (state.instance_id, state)),
        );
    }

    pub fn clear_remote_implement_snapshot(&mut self) {
        self.remote_arcane_items.clear();
        self.remote_implements.clear();
        self.remote_apparatus.clear();
    }

    pub fn extend_remote_arcane_items(&mut self, charges: Vec<(u64, u64)>) {
        self.remote_arcane_items
            .extend(charges.into_iter().filter(|(id, _)| *id != 0));
    }

    pub fn extend_remote_implements(
        &mut self,
        states: Vec<crate::implements::ImplementPublicState>,
    ) {
        self.remote_implements.extend(
            states
                .into_iter()
                .filter(|state| state.instance_id != 0)
                .map(|state| (state.instance_id, state)),
        );
    }

    pub fn extend_remote_apparatus(&mut self, cues: Vec<crate::implements::ApparatusCue>) {
        self.remote_apparatus
            .extend(cues.into_iter().take(128).map(|cue| (cue.pos, cue)));
    }

    #[cfg(test)]
    pub fn set_remote_apparatus(&mut self, cues: Vec<crate::implements::ApparatusCue>) {
        self.remote_apparatus.clear();
        self.remote_apparatus
            .extend(cues.into_iter().take(128).map(|cue| (cue.pos, cue)));
    }

    /// Bounded qualitative apparatus state for local presentation or nearby
    /// guest interest management. Exact amounts remain in item custody and
    /// tuning-lens responses.
    pub fn apparatus_cues_near(
        &self,
        observer: crate::planet::EntityPos,
        radius: f32,
    ) -> Vec<crate::implements::ApparatusCue> {
        let radius = radius.clamp(1.0, 96.0);
        if self.remote {
            return self
                .remote_apparatus
                .values()
                .copied()
                .filter(|cue| observer.distance_to(cue.pos.entity_center()) <= radius)
                .take(128)
                .collect();
        }
        let Some(state) = self.implements_state.as_ref() else {
            return Vec::new();
        };
        let Some(ledger) = self.arcane_ledger.as_ref() else {
            return Vec::new();
        };
        self.block_entities
            .iter()
            .filter_map(|(&pos, entity)| {
                if observer.distance_to(pos.entity_center()) > radius {
                    return None;
                }
                let BlockEntity::ChargeVessel(vessel) = entity else {
                    return None;
                };
                let stack = vessel.vessel?;
                let instance = state.instance(stack.arcane_id)?;
                let total = ledger.item_clean_total(stack.arcane_id).unwrap_or(0);
                let strain_band = match vessel.damage {
                    0..=199 => 0,
                    200..=499 => 1,
                    500..=799 => 2,
                    _ => 3,
                };
                Some(crate::implements::ApparatusCue {
                    pos,
                    charge_band: crate::implements::charge_band(total, instance.usable_capacity()),
                    strain_band,
                })
            })
            .take(128)
            .collect()
    }

    pub fn implement_visual(&self, stack: ItemStack) -> Option<crate::implements::ImplementVisual> {
        let kind = self
            .implements_state
            .as_ref()
            .and_then(|state| state.instance(stack.arcane_id))
            .map(|instance| &instance.kind)
            .or_else(|| {
                self.remote_implements
                    .get(&stack.arcane_id)
                    .map(|state| &state.kind)
            })?;
        let ImplementKind::Wand { parts, resolved } = kind else {
            return None;
        };
        // Saved component ids outlive content packs. A removed mod part keeps
        // its manifest/stat identity, while rendering falls back by physical
        // role instead of making the entire held model disappear.
        let item = |name: &str, fallback: &str| {
            self.reg
                .item_id(name)
                .or_else(|| self.reg.item_id(fallback))
                .map(|item| item.0)
        };
        let usable = self
            .inspectable_item_current(stack.arcane_id)
            .map(crate::implements::usable_charge)
            .unwrap_or(0);
        let charge_band = crate::implements::charge_band(
            usable.saturating_add(crate::implements::STRUCTURAL_SPARK_UNITS),
            resolved.capacity,
        );
        let focus_shape = match parts.focus.as_str() {
            "base:echo_slate" => 1,
            "base:choirstone" => 2,
            "base:wake_iron" => 3,
            "base:pilgrim_root_cutting" => 4,
            _ => {
                1 + (parts.focus.bytes().fold(0u32, |hash, byte| {
                    hash.wrapping_mul(16777619) ^ u32::from(byte)
                }) % 4) as u8
            }
        };
        Some(crate::implements::ImplementVisual {
            body: item(&parts.body, "base:seasoned_wand_body")?,
            reservoir: item(&parts.reservoir, "base:ritual_rod_socket")?,
            focus: item(&parts.focus, "base:echo_slate")?,
            binding: item(&parts.binding, "base:bronze_wand_binding")?,
            focus_shape,
            charge_band,
        })
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

    pub fn binding_frame_layout(&self, pos: BlockPos) -> FrameLayout {
        let mut problems = Vec::new();
        if self
            .reg
            .block(self.get_block_at(pos))
            .interaction
            .as_deref()
            != Some("binding_frame")
        {
            problems.push("The work surface is not a binding frame.".into());
        }
        let neighbors = horizontal_neighbors(pos);
        let mut focus_mounts = 0u8;
        let mut vessels = 0u8;
        let mut conductor_endpoints = 0u8;
        for neighbor in &neighbors {
            match self
                .reg
                .block(self.get_block_at(*neighbor))
                .interaction
                .as_deref()
            {
                Some("focus_mount") => focus_mounts = focus_mounts.saturating_add(1),
                Some("charge_vessel") => vessels = vessels.saturating_add(1),
                Some("arcane_conductor") => {
                    conductor_endpoints = conductor_endpoints.saturating_add(1)
                }
                _ => {}
            }
        }
        if focus_mounts == 0 {
            problems.push("Place a focus mount beside the frame.".into());
        }
        if vessels == 0 {
            problems.push("Place a charge vessel beside the frame.".into());
        }
        if conductor_endpoints == 0 {
            problems.push("Join a conductor endpoint directly to the frame.".into());
        }

        let (network_size, touches_unloaded, network_overflow) = self.conductor_network(pos);
        if touches_unloaded {
            problems.push("The conductor reaches an unloaded boundary; transfer is paused.".into());
        }
        if network_overflow {
            problems.push(format!(
                "The conductor network exceeds its {}-segment local budget.",
                crate::implements::MAX_CONDUCTOR_NETWORK
            ));
        }

        let mut containment = 0u16;
        for du in -2..=2 {
            for dv in -2..=2 {
                if du == 0 && dv == 0 {
                    continue;
                }
                let Some(at) = pos.offset(du, 0, dv) else {
                    continue;
                };
                let definition = self.reg.block(self.get_block_at(at));
                containment = containment.saturating_add(match definition.name.as_str() {
                    "base:still_salt" => 180,
                    "base:hushwood" => 90,
                    _ if definition.interaction.as_deref() == Some("containment") => 150,
                    // Deliberate physical spacing is useful but cannot replace
                    // material containment on its own.
                    _ if self.reg.is_air(self.get_block_at(at)) && du.abs().max(dv.abs()) == 2 => 4,
                    _ => 0,
                });
            }
        }
        containment = containment.min(1_000);
        if containment < 120 {
            problems.push("The frame needs still salt, Hushwood, posts, or more spacing.".into());
        }

        FrameLayout {
            valid: problems.is_empty(),
            focus_mounts,
            vessels,
            conductor_endpoints,
            containment,
            network_size: network_size.min(u8::MAX as usize) as u8,
            touches_unloaded,
            problems,
        }
    }

    fn conductor_network(&self, frame: BlockPos) -> (usize, bool, bool) {
        let (visited, touches_unloaded, overflow) = self.conductor_network_positions(frame);
        (visited.len(), touches_unloaded, overflow)
    }

    fn conductor_network_positions(&self, frame: BlockPos) -> (BTreeSet<BlockPos>, bool, bool) {
        let mut queue = VecDeque::new();
        for pos in horizontal_neighbors(frame) {
            if self
                .reg
                .block(self.get_block_at(pos))
                .interaction
                .as_deref()
                == Some("arcane_conductor")
            {
                queue.push_back(pos);
            }
        }
        let mut visited = BTreeSet::new();
        let mut touches_unloaded = false;
        let mut overflow = false;
        while let Some(pos) = queue.pop_front() {
            if !visited.insert(pos) {
                continue;
            }
            if visited.len() > crate::implements::MAX_CONDUCTOR_NETWORK {
                overflow = true;
                break;
            }
            for next in apparatus_neighbors(pos) {
                if !self.has_chunk(next.chunk()) {
                    touches_unloaded = true;
                    continue;
                }
                if self
                    .reg
                    .block(self.get_block_at(next))
                    .interaction
                    .as_deref()
                    == Some("arcane_conductor")
                    && !visited.contains(&next)
                {
                    queue.push_back(next);
                }
            }
        }
        (visited, touches_unloaded, overflow)
    }

    /// The first deterministic loaded conductor cell that actually touches a
    /// confluence or well. A conductor merely passing through strong Current
    /// is not an extractor, and unloaded network tails never qualify.
    fn conductor_place_source(
        &self,
        frame: BlockPos,
    ) -> Option<(
        crate::planet_atlas::AtlasPos,
        crate::arcane_geography::ArcanePlaceType,
    )> {
        let (positions, touches_unloaded, overflow) = self.conductor_network_positions(frame);
        if touches_unloaded || overflow {
            return None;
        }
        let atlas = self.planet_atlas.as_ref()?;
        let geography = self.arcane_geography.as_ref()?;
        positions.into_iter().find_map(|position| {
            let region = atlas.atlas_pos(position.surface());
            let (_, kind, _) = geography.survey(region, true).nearby_place?;
            matches!(
                kind,
                crate::arcane_geography::ArcanePlaceType::Confluence
                    | crate::arcane_geography::ArcanePlaceType::Well
            )
            .then_some((region, kind))
        })
    }

    pub fn operate_binding_frame(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
        selected_slot: usize,
        action: FrameAction,
        expected_revision: Option<u64>,
        actor: &str,
    ) -> Result<FrameResult, String> {
        if selected_slot >= inventory.slots.len() {
            return Err("That inventory slot does not exist.".into());
        }
        if self
            .reg
            .block(self.get_block_at(pos))
            .interaction
            .as_deref()
            != Some("binding_frame")
        {
            return Err("There is no binding frame there.".into());
        }
        let current_revision = match self.block_entities.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => frame.revision,
            Some(_) => return Err("Another block entity occupies the frame.".into()),
            None => 0,
        };
        if action != FrameAction::Inspect && expected_revision.is_none() && current_revision != 0 {
            let mut result = self.inspect_binding_frame(pos)?;
            result.success = false;
            result.cue = ImplementCue::Strain;
            result.message =
                "The frame has prior changes; its authoritative state was inspected without mutating it. Repeat the operation.".into();
            return Ok(result);
        }
        if action != FrameAction::Inspect
            && expected_revision.is_some_and(|revision| revision != current_revision)
        {
            let mut result = self.inspect_binding_frame(pos)?;
            result.success = false;
            result.cue = ImplementCue::Strain;
            result.message =
                "The frame changed before that operation arrived; its current state was returned without mutating it. Repeat the operation.".into();
            return Ok(result);
        }
        let action = if action == FrameAction::Contextual {
            self.contextual_frame_action(pos, inventory.slots[selected_slot])
        } else {
            action
        };
        match action {
            FrameAction::Contextual => unreachable!("contextual action resolves above"),
            FrameAction::ExchangeSelected => {
                self.exchange_frame_item(pos, inventory, selected_slot)
            }
            FrameAction::Assemble => self.assemble_wand(pos, actor),
            FrameAction::Calibrate => self.calibrate_frame_or_vessel(pos, actor),
            FrameAction::Inspect => self.inspect_binding_frame(pos),
            FrameAction::BindCharm => self.bind_charm_at_frame(pos, actor),
            FrameAction::Transfer => {
                self.transfer_at_frame(pos, inventory.slots[selected_slot], actor)
            }
            FrameAction::SafeDischarge => self.discharge_at_frame(pos, actor),
            FrameAction::Disassemble => {
                self.disassemble_at_frame(pos, inventory, inventory.slots[selected_slot], actor)
            }
            FrameAction::SwapFocus => self.swap_focus_at_frame(pos, inventory, actor),
            FrameAction::Repair => self.repair_at_frame(pos, inventory, selected_slot, actor),
        }
    }

    fn contextual_frame_action(&self, pos: BlockPos, held: Option<ItemStack>) -> FrameAction {
        if let Some(stack) = held {
            let definition = self.reg.item(stack.item);
            if definition
                .discovery
                .as_ref()
                .is_some_and(|definition| definition.kind == "tuning_lens")
            {
                return FrameAction::Inspect;
            }
            if definition.shears {
                return FrameAction::Disassemble;
            }
            if definition.hammer {
                return FrameAction::Repair;
            }
            if stack.arcane_id != 0 && definition.arcane.is_some() {
                return FrameAction::Transfer;
            }
            if definition.name == "base:still_salt" {
                return FrameAction::SafeDischarge;
            }
            return FrameAction::ExchangeSelected;
        }
        let Some(BlockEntity::BindingFrame(frame)) = self.block_entities.get(&pos) else {
            return FrameAction::Calibrate;
        };
        if let Some(output) = frame.output {
            let name = self.reg.item(output.item).name.as_str();
            if matches!(
                name,
                "base:quiet_charm_blank" | "base:bark_charm_blank" | "base:hunger_charm_blank"
            ) && frame.mounts().into_iter().flatten().next().is_some()
            {
                return FrameAction::BindCharm;
            }
            if self
                .implements_state
                .as_ref()
                .and_then(|state| state.instance(output.arcane_id))
                .is_some_and(|instance| matches!(instance.kind, ImplementKind::Wand { .. }))
                && frame.focus.is_some()
            {
                return FrameAction::SwapFocus;
            }
            return FrameAction::ExchangeSelected;
        }
        if frame.mounts().into_iter().all(|mount| mount.is_some()) {
            FrameAction::Assemble
        } else if frame.is_empty() {
            FrameAction::Calibrate
        } else {
            FrameAction::ExchangeSelected
        }
    }

    fn exchange_frame_item(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
        selected_slot: usize,
    ) -> Result<FrameResult, String> {
        let selected = inventory.slots[selected_slot];
        self.block_entities
            .entry(pos)
            .or_insert_with(|| BlockEntity::BindingFrame(Default::default()));
        let Some(BlockEntity::BindingFrame(frame)) = self.block_entities.get_mut(&pos) else {
            return Err("Another block entity occupies the frame.".into());
        };
        let message = if let Some(stack) = selected {
            let definition = self.reg.item(stack.item);
            if let Some(component) = &definition.wand_component {
                let mount = match component.role {
                    ComponentRole::Body => &mut frame.body,
                    ComponentRole::Reservoir => &mut frame.reservoir,
                    ComponentRole::Focus => &mut frame.focus,
                    ComponentRole::Binding => &mut frame.binding,
                };
                if mount.is_some() {
                    return Err(format!(
                        "The {} mount is already occupied.",
                        component.role.label()
                    ));
                }
                *mount = inventory.take_one_stack(selected_slot);
                format!(
                    "Mounted {} as the {}.",
                    definition.label,
                    component.role.label()
                )
            } else if definition.implement.is_some()
                || matches!(
                    definition.name.as_str(),
                    "base:quiet_charm_blank" | "base:bark_charm_blank" | "base:hunger_charm_blank"
                )
            {
                if frame.output.is_some() {
                    return Err("The finished-object cradle is occupied.".into());
                }
                frame.output = inventory.take_one_stack(selected_slot);
                format!("Placed {} in the finished-object cradle.", definition.label)
            } else {
                return Err("That item fits none of the frame's physical mounts.".into());
            }
        } else {
            let retrieved = frame
                .output
                .take()
                .or_else(|| frame.binding.take())
                .or_else(|| frame.focus.take())
                .or_else(|| frame.reservoir.take())
                .or_else(|| frame.body.take())
                .ok_or("Every frame mount is empty.")?;
            inventory.slots[selected_slot] = Some(retrieved);
            format!(
                "Retrieved {} from the frame.",
                self.reg.item(retrieved.item).label
            )
        };
        frame.revision = frame.revision.saturating_add(1);
        let revision = frame.revision;
        self.save_entities().map_err(|error| error.to_string())?;
        let preview = self.frame_preview(pos).ok();
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Use,
            message,
            preview,
            lines: Vec::new(),
        })
    }

    fn frame_preview(&self, pos: BlockPos) -> Result<crate::implements::ResolvedWand, String> {
        let Some(BlockEntity::BindingFrame(frame)) = self.block_entities.get(&pos) else {
            return Err("The frame has no mounts.".into());
        };
        let stacks = frame.mounts();
        let mut parts: Vec<(String, crate::implements::WandComponentDef)> = Vec::new();
        for stack in stacks {
            let stack = stack.ok_or("Install one component in every role mount.")?;
            let definition = self.reg.item(stack.item);
            let component = definition
                .wand_component
                .clone()
                .ok_or("A mounted item is not a wand component.")?;
            parts.push((definition.name.clone(), component));
        }
        let parts: [(String, crate::implements::WandComponentDef); 4] = parts
            .try_into()
            .map_err(|_| "The frame must hold exactly four parts.".to_string())?;
        crate::implements::resolve_wand(&parts)
            .map(|(_, resolved)| resolved)
            .map_err(|error| error.to_string())
    }

    fn inspect_binding_frame(&self, pos: BlockPos) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        let revision = match self.block_entities.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => frame.revision,
            _ => 0,
        };
        let preview = self.frame_preview(pos).ok();
        let mut lines = if layout.valid {
            vec![format!(
                "Frame embodied: {} conductors, containment {} permille.",
                layout.network_size, layout.containment
            )]
        } else {
            layout.problems.clone()
        };
        if let Some(BlockEntity::BindingFrame(frame)) = self.block_entities.get(&pos)
            && let Some(output) = frame.output
            && output.arcane_id != 0
            && let (Some(state), Some(ledger)) = (&self.implements_state, &self.arcane_ledger)
            && let Some(instance) = state.instance(output.arcane_id)
        {
            let clean = ledger.item_clean_total(output.arcane_id).unwrap_or(0);
            let dross = ledger.item_dross_total(output.arcane_id);
            lines.extend(crate::implements::tooltip(instance, clean, dross, false));
        }
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Use,
            message: if layout.valid {
                "The binding frame is physically complete.".into()
            } else {
                "The binding frame is incomplete.".into()
            },
            preview,
            lines,
        })
    }

    fn assemble_wand(&mut self, pos: BlockPos, actor: &str) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        let (mounts, revision, output_occupied) = match self.block_entities.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => {
                (frame.mounts(), frame.revision, frame.output.is_some())
            }
            _ => return Err("Install the four components in the frame.".into()),
        };
        if output_occupied {
            return Err("Retrieve the object in the finished cradle first.".into());
        }
        let mut declared = Vec::new();
        for stack in mounts {
            let stack = stack.ok_or("Install one body, reservoir, focus, and binding.")?;
            let definition = self.reg.item(stack.item);
            let component = definition
                .wand_component
                .clone()
                .ok_or("A mounted item is not a declared wand component.")?;
            declared.push((definition.name.clone(), component));
        }
        let declared: [(String, crate::implements::WandComponentDef); 4] = declared
            .try_into()
            .map_err(|_| "The frame needs exactly four component roles.".to_string())?;
        let (parts, resolved) =
            crate::implements::resolve_wand(&declared).map_err(|error| error.to_string())?;

        let output_item = self
            .reg
            .item_id("base:bound_wand")
            .ok_or("The bound-wand content definition is missing.")?;
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        self.ensure_regional_ambient_units(
            region,
            ASSEMBLY_CALIBRATION_UNITS.saturating_add(STRUCTURAL_SPARK_UNITS),
            "binding-frame wand calibration reserve",
        )?;
        let ambient_owner = ArcaneOwner::Ambient(region);
        let content_id = self.reg.item(output_item).name.clone();

        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let instance_id = ledger
            .allocate_item_id()
            .map_err(|error| error.to_string())?;
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;

        let mut debits = BTreeMap::<ArcaneOwner, Current>::new();
        let mut clean = Current::default();
        let mut carried_dross = Current::default();
        for stack in mounts.into_iter().flatten() {
            if stack.arcane_id == 0 {
                continue;
            }
            for (owner, dross) in [
                (ArcaneOwner::Item(stack.arcane_id), false),
                (ArcaneOwner::ItemDross(stack.arcane_id), true),
            ] {
                if let Some(account) = ledger.account(&owner) {
                    add_current(&mut debits, owner, &account.current)
                        .map_err(|error| error.to_string())?;
                    if dross {
                        carried_dross
                            .checked_add(&account.current)
                            .map_err(|error| error.to_string())?;
                    } else {
                        clean
                            .checked_add(&account.current)
                            .map_err(|error| error.to_string())?;
                    }
                }
            }
        }
        let calibration_needed = ASSEMBLY_CALIBRATION_UNITS
            .saturating_add(STRUCTURAL_SPARK_UNITS)
            .saturating_sub(clean.total());
        if calibration_needed != 0 {
            let account = ledger
                .account(&ambient_owner)
                .ok_or("This region has no measurable Ambient Current.")?;
            let mut available = account.current.clone();
            let selected = available
                .take_units(calibration_needed, resolved.resonance.keys().cloned())
                .map_err(|_| "The regional Ambient reserve cannot calibrate this wand.")?;
            add_current(&mut debits, ambient_owner.clone(), &selected)
                .map_err(|error| error.to_string())?;
            clean
                .checked_add(&selected)
                .map_err(|error| error.to_string())?;
        }
        if clean.total() < STRUCTURAL_SPARK_UNITS {
            return Err("The frame cannot establish the wand's structural spark.".into());
        }

        let keep = clean
            .total()
            .min(resolved.capacity.saturating_add(STRUCTURAL_SPARK_UNITS));
        let mut source_for_keep = clean.clone();
        let mut item_clean = source_for_keep
            .take_units(keep, resolved.resonance.keys().cloned())
            .map_err(|error| error.to_string())?;
        let overflow = source_for_keep;

        let usable_before_dross = item_clean.total().saturating_sub(STRUCTURAL_SPARK_UNITS);
        let generated_units = usable_before_dross
            .saturating_mul(u64::from(resolved.dross_per_thousand))
            .div_ceil(1_000)
            .min(usable_before_dross);
        let generated_dross = if generated_units == 0 {
            Current::default()
        } else {
            item_clean
                .take_units(generated_units, std::iter::empty())
                .map_err(|error| error.to_string())?
        };
        carried_dross
            .checked_add(&generated_dross)
            .map_err(|error| error.to_string())?;
        if item_clean.total() < STRUCTURAL_SPARK_UNITS {
            return Err("Assembly dross consumed the structural spark.".into());
        }

        let target = ArcaneOwner::Item(instance_id);
        let dross_target = ArcaneOwner::ItemDross(instance_id);
        let mut credits = BTreeMap::<ArcaneOwner, Current>::new();
        add_current(&mut credits, target.clone(), &item_clean)
            .map_err(|error| error.to_string())?;
        if !carried_dross.is_empty() {
            add_current(&mut credits, dross_target.clone(), &carried_dross)
                .map_err(|error| error.to_string())?;
        }
        if !overflow.is_empty() {
            add_current(&mut credits, ambient_owner.clone(), &overflow)
                .map_err(|error| error.to_string())?;
        }

        for (id, component) in &declared {
            next_state
                .component_manifests
                .insert(id.clone(), component.clone());
        }
        next_state
            .insert(ImplementInstance {
                instance_id,
                content_id: content_id.clone(),
                kind: ImplementKind::Wand {
                    parts,
                    resolved: resolved.clone(),
                },
                construction: mounts
                    .into_iter()
                    .flatten()
                    .map(|stack| physical_component(&self.reg, stack))
                    .collect(),
                wear: 0,
                strain: 0,
                provenance: format!("binding_frame:{pos:?}"),
                created_day: self.day,
                format_version: crate::implements::IMPLEMENT_RESOLVER_VERSION,
                creative: self.mode == "creative",
            })
            .map_err(|error| error.to_string())?;
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "assemble_wand".into(),
            instance_id,
            units: item_clean.total().saturating_sub(STRUCTURAL_SPARK_UNITS),
            dross: carried_dross.total(),
            actor: actor.into(),
            note: format!(
                "frame revision {revision}; containment {}",
                layout.containment
            ),
        });

        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &content_id,
            "binding-frame wand assembly",
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
        let Some(BlockEntity::BindingFrame(frame)) = self.block_entities.get_mut(&pos) else {
            return Err("The frame vanished after assembly committed.".into());
        };
        frame.body = None;
        frame.reservoir = None;
        frame.focus = None;
        frame.binding = None;
        frame.output = Some(ItemStack {
            item: output_item,
            count: 1,
            durability: self.reg.item(output_item).durability,
            arcane_id: instance_id,
        });
        frame.revision = frame.revision.saturating_add(1);
        let revision = frame.revision;
        self.save_entities().map_err(|error| error.to_string())?;
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Use,
            message: format!(
                "The four parts settle into a bound wand: capacity {}, safe transfer {}, stability {}.",
                resolved.capacity, resolved.safe_transfer, resolved.stability
            ),
            preview: Some(resolved),
            lines: vec![format!(
                "Assembly retained {} dross units; no Current was created.",
                carried_dross.total()
            )],
        })
    }

    fn calibrate_frame_or_vessel(
        &mut self,
        pos: BlockPos,
        actor: &str,
    ) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        let vessel_pos = horizontal_neighbors(pos)
            .into_iter()
            .find(|at| {
                matches!(
                    self.block_entities.get(at),
                    Some(BlockEntity::ChargeVessel(_))
                )
            })
            .ok_or("The adjacent vessel has no physical state.")?;
        let (stack, damage, vessel_revision) = match self.block_entities.get(&vessel_pos) {
            Some(BlockEntity::ChargeVessel(vessel)) => (
                vessel.vessel.ok_or("The vessel shell is missing.")?,
                vessel.damage,
                vessel.revision,
            ),
            _ => return Err("The adjacent vessel has no physical state.".into()),
        };
        if damage >= 900 {
            return Err("The vessel is too damaged to accept a calibration pulse.".into());
        }
        if stack.arcane_id != 0 {
            return self.calibration_pulse(pos, actor);
        }
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let source = ArcaneOwner::Ambient(region);
        self.ensure_regional_ambient_units(
            region,
            VESSEL_INITIAL_CHARGE + STRUCTURAL_SPARK_UNITS + 1,
            "binding-frame vessel calibration reserve",
        )?;
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let source_account = ledger
            .account(&source)
            .ok_or("This region has no measurable Ambient Current.")?;
        let requested = VESSEL_INITIAL_CHARGE + STRUCTURAL_SPARK_UNITS + 1;
        let mut available = source_account.current.clone();
        let selected = available
            .take_units(
                requested,
                crate::arcane::BASE_RESONANCES
                    .into_iter()
                    .map(str::to_string),
            )
            .map_err(|_| "The regional Ambient reserve cannot calibrate the vessel.")?;
        let mut clean = selected.clone();
        let dross = clean
            .take_units(1, std::iter::empty())
            .map_err(|error| error.to_string())?;
        let instance_id = ledger
            .allocate_item_id()
            .map_err(|error| error.to_string())?;
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        next_state
            .insert(ImplementInstance {
                instance_id,
                content_id: self.reg.item(stack.item).name.clone(),
                kind: ImplementKind::Vessel {
                    capacity: VESSEL_CAPACITY,
                    safe_transfer: VESSEL_SAFE_TRANSFER,
                    containment: layout.containment,
                },
                construction: vec![physical_component(&self.reg, stack)],
                wear: 0,
                strain: u32::from(damage) * 5,
                provenance: format!("calibrated_vessel:{vessel_pos:?}"),
                created_day: self.day,
                format_version: crate::implements::IMPLEMENT_RESOLVER_VERSION,
                creative: self.mode == "creative",
            })
            .map_err(|error| error.to_string())?;
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "calibrate_vessel".into(),
            instance_id,
            units: clean.total().saturating_sub(STRUCTURAL_SPARK_UNITS),
            dross: dross.total(),
            actor: actor.into(),
            note: format!("vessel revision {vessel_revision}"),
        });
        let target = ArcaneOwner::Item(instance_id);
        let dross_target = ArcaneOwner::ItemDross(instance_id);
        let mut transaction = ArcaneTransaction {
            id: ledger
                .system_transaction_id()
                .map_err(|error| error.to_string())?,
            reads: vec![
                AccountRead {
                    owner: source.clone(),
                    expected_version: ledger.version_of(&source),
                },
                AccountRead {
                    owner: target.clone(),
                    expected_version: 0,
                },
                AccountRead {
                    owner: dross_target.clone(),
                    expected_version: 0,
                },
            ],
            debits: vec![ArcaneMove {
                owner: source,
                current: selected,
                content_id: None,
            }],
            credits: vec![
                ArcaneMove {
                    owner: target,
                    current: clean,
                    content_id: Some(self.reg.item(stack.item).name.clone()),
                },
                ArcaneMove {
                    owner: dross_target,
                    current: dross,
                    content_id: Some(self.reg.item(stack.item).name.clone()),
                },
            ],
            transforms: Vec::new(),
            authority: ArcaneAuthority::System,
            reason: "binding-frame vessel calibration".into(),
            content_id: self.reg.item(stack.item).name.clone(),
            linked: Vec::new(),
        };
        transaction.reads.sort_by(|a, b| a.owner.cmp(&b.owner));
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
        let Some(BlockEntity::ChargeVessel(vessel)) = self.block_entities.get_mut(&vessel_pos)
        else {
            return Err("The vessel vanished after calibration committed.".into());
        };
        if vessel.revision != vessel_revision {
            return Err("The vessel changed during calibration.".into());
        }
        let Some(physical) = vessel.vessel.as_mut() else {
            return Err("The vessel shell vanished during calibration.".into());
        };
        physical.arcane_id = instance_id;
        vessel.revision = vessel.revision.saturating_add(1);
        self.save_entities().map_err(|error| error.to_string())?;
        let frame_revision = match self.block_entities.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => frame.revision,
            _ => 0,
        };
        Ok(FrameResult {
            success: true,
            revision: frame_revision,
            cue: ImplementCue::Transfer,
            message: "The vessel accepts a measured calibration pulse and begins to glow.".into(),
            preview: self.frame_preview(pos).ok(),
            lines: vec![
                "One unit settled as contained dross; the rest remains finite Current.".into(),
            ],
        })
    }

    fn calibration_pulse(&mut self, pos: BlockPos, actor: &str) -> Result<FrameResult, String> {
        self.transfer_at_frame_with_limit(pos, actor, Some(2), true)
    }

    // Every frame verb below shares this host-authoritative surface and the
    // same finite-ledger transaction rules.
    fn bind_charm_at_frame(&mut self, pos: BlockPos, actor: &str) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        let (blank, mounts, frame_revision) = match self.block_entities.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => (
                frame
                    .output
                    .ok_or("Place an unbound charm in the finished cradle.")?,
                frame.mounts(),
                frame.revision,
            ),
            _ => return Err("The frame has no physical mounts.".into()),
        };
        if blank.arcane_id != 0 {
            return Err("That finished cradle already holds a bound object.".into());
        }
        let blank_name = self.reg.item(blank.item).name.as_str();
        let (target_name, effect, reagent_names): (&str, crate::implements::CharmEffect, &[&str]) =
            match blank_name {
                "base:quiet_charm_blank" => (
                    "base:charm_quiet",
                    crate::implements::CharmEffect::Quiet,
                    &["base:nightglass_pod", "base:echo_slate"],
                ),
                "base:bark_charm_blank" => (
                    "base:charm_bark",
                    crate::implements::CharmEffect::Bark,
                    &["base:hushwood_switch", "base:pilgrim_root_cutting"],
                ),
                "base:hunger_charm_blank" => (
                    "base:charm_hunger",
                    crate::implements::CharmEffect::Hunger,
                    &["base:ashlace_tissue", "base:wellglass_shard"],
                ),
                _ => {
                    return Err("Place one of the three unbound charm bodies in the cradle.".into());
                }
            };
        let reagent = mounts
            .into_iter()
            .flatten()
            .find(|stack| reagent_names.contains(&self.reg.item(stack.item).name.as_str()))
            .ok_or_else(|| {
                format!(
                    "The {} binding needs a matching living or mineral reagent in a mount.",
                    effect.id()
                )
            })?;
        let target_item = self
            .reg
            .item_id(target_name)
            .ok_or("The bound charm content definition is missing.")?;
        let definition = self
            .reg
            .item(target_item)
            .charm_def
            .clone()
            .ok_or("The bound charm has no authoritative effect definition.")?;
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let ambient = ArcaneOwner::Ambient(region);
        self.ensure_regional_ambient_units(
            region,
            definition.capacity.min(64) + STRUCTURAL_SPARK_UNITS,
            "binding-frame charm binding reserve",
        )?;
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let mut clean = Current::default();
        let mut carried_dross = Current::default();
        let mut debits = BTreeMap::new();
        if reagent.arcane_id != 0 {
            for (owner, is_dross) in [
                (ArcaneOwner::Item(reagent.arcane_id), false),
                (ArcaneOwner::ItemDross(reagent.arcane_id), true),
            ] {
                if let Some(account) = ledger.account(&owner) {
                    add_current(&mut debits, owner, &account.current)
                        .map_err(|error| error.to_string())?;
                    if is_dross {
                        carried_dross
                            .checked_add(&account.current)
                            .map_err(|error| error.to_string())?;
                    } else {
                        clean
                            .checked_add(&account.current)
                            .map_err(|error| error.to_string())?;
                    }
                }
            }
        }
        let desired_initial = definition.capacity.min(64) + STRUCTURAL_SPARK_UNITS;
        if clean.total() < desired_initial {
            let missing = desired_initial - clean.total();
            let ambient_account = ledger
                .account(&ambient)
                .ok_or("This region has no measurable Ambient Current.")?;
            let mut available = ambient_account.current.clone();
            let selected = available
                .take_units(missing, charm_resonance_preference(effect))
                .map_err(|_| "The regional Ambient reserve cannot settle this charm.")?;
            add_current(&mut debits, ambient.clone(), &selected)
                .map_err(|error| error.to_string())?;
            clean
                .checked_add(&selected)
                .map_err(|error| error.to_string())?;
        }
        let keep = clean
            .total()
            .min(definition.capacity.saturating_add(STRUCTURAL_SPARK_UNITS));
        let mut source = clean;
        let mut item_clean = source
            .take_units(keep, charm_resonance_preference(effect))
            .map_err(|error| error.to_string())?;
        let overflow = source;
        let usable = item_clean.total().saturating_sub(STRUCTURAL_SPARK_UNITS);
        let generated_units = usable
            .saturating_mul(u64::from(definition.dross_per_transfer))
            .div_ceil(1_000)
            .min(usable);
        if generated_units != 0 {
            let generated = item_clean
                .take_units(generated_units, std::iter::empty())
                .map_err(|error| error.to_string())?;
            carried_dross
                .checked_add(&generated)
                .map_err(|error| error.to_string())?;
        }
        if item_clean.total() < STRUCTURAL_SPARK_UNITS {
            return Err("The binding would consume the charm's structural spark.".into());
        }
        let instance_id = ledger
            .allocate_item_id()
            .map_err(|error| error.to_string())?;
        let target = ArcaneOwner::Item(instance_id);
        let dross_target = ArcaneOwner::ItemDross(instance_id);
        let mut credits = BTreeMap::new();
        add_current(&mut credits, target, &item_clean).map_err(|error| error.to_string())?;
        if !carried_dross.is_empty() {
            add_current(&mut credits, dross_target, &carried_dross)
                .map_err(|error| error.to_string())?;
        }
        if !overflow.is_empty() {
            add_current(&mut credits, ambient, &overflow).map_err(|error| error.to_string())?;
        }
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        next_state
            .charm_manifests
            .insert(target_name.into(), definition.clone());
        next_state
            .insert(ImplementInstance {
                instance_id,
                content_id: target_name.into(),
                kind: ImplementKind::Charm {
                    effect,
                    capacity: definition.capacity,
                    charge_per_trigger: definition.charge_per_trigger,
                    stability: definition.stability,
                    dross_per_transfer: definition.dross_per_transfer,
                },
                construction: vec![
                    physical_component(&self.reg, blank),
                    physical_component(&self.reg, reagent),
                ],
                wear: 0,
                strain: 0,
                provenance: format!("charm_binding:{pos:?}"),
                created_day: self.day,
                format_version: crate::implements::IMPLEMENT_RESOLVER_VERSION,
                creative: self.mode == "creative",
            })
            .map_err(|error| error.to_string())?;
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "bind_charm".into(),
            instance_id,
            units: crate::implements::usable_charge(item_clean.total()),
            dross: carried_dross.total(),
            actor: actor.into(),
            note: format!("{} binding at frame revision {frame_revision}", effect.id()),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            target_name,
            "binding-frame charm binding",
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
        let Some(BlockEntity::BindingFrame(frame)) = self.block_entities.get_mut(&pos) else {
            return Err("The frame vanished after binding committed.".into());
        };
        for mount in [
            &mut frame.body,
            &mut frame.reservoir,
            &mut frame.focus,
            &mut frame.binding,
        ] {
            if mount.is_some_and(|stack| stack == reagent) {
                *mount = None;
                break;
            }
        }
        frame.output = Some(ItemStack {
            item: target_item,
            count: 1,
            durability: self.reg.item(target_item).durability,
            arcane_id: instance_id,
        });
        frame.revision = frame.revision.saturating_add(1);
        let revision = frame.revision;
        self.save_entities().map_err(|error| error.to_string())?;
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Use,
            message: format!(
                "The {} charm settles into a reproducible charged implement.",
                effect.id()
            ),
            preview: None,
            lines: vec![format!(
                "Usable charge {}; retained dross {}.",
                crate::implements::usable_charge(item_clean.total()),
                carried_dross.total()
            )],
        })
    }

    fn transfer_at_frame(
        &mut self,
        pos: BlockPos,
        selected: Option<ItemStack>,
        actor: &str,
    ) -> Result<FrameResult, String> {
        if let Some(source) = selected
            && source.arcane_id != 0
            && self.reg.item(source.item).arcane.is_some()
        {
            return self.transfer_selected_source_at_frame(pos, source, actor);
        }
        // When both local reservoirs are dormant, an attached loaded
        // conductor at a real confluence/well is the only eligible source.
        // Once either reservoir holds charge, the ordinary vessel direction
        // remains deterministic and players can stage cargo deliberately.
        if self.conductor_place_source(pos).is_some()
            && let Some(BlockEntity::BindingFrame(frame)) = self.block_entities.get(&pos)
            && let Some(output) = frame.output
            && output.arcane_id != 0
            && let Ok((_, vessel, _)) = self.adjacent_vessel(pos)
            && vessel.arcane_id != 0
            && self.arcane_ledger.as_ref().is_some_and(|ledger| {
                ledger
                    .item_clean_total(output.arcane_id)
                    .map(crate::implements::usable_charge)
                    .unwrap_or(0)
                    == 0
                    && ledger
                        .item_clean_total(vessel.arcane_id)
                        .map(crate::implements::usable_charge)
                        .unwrap_or(0)
                        == 0
            })
        {
            return self.transfer_conductor_place_at_frame(pos, actor);
        }
        self.transfer_at_frame_with_limit(pos, actor, None, false)
    }

    fn transfer_conductor_place_at_frame(
        &mut self,
        pos: BlockPos,
        actor: &str,
    ) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        let (source_region, source_kind) = self
            .conductor_place_source(pos)
            .ok_or("The loaded conductor no longer touches a confluence or well.")?;
        let (output, frame_revision) = match self.block_entities.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => (
                frame
                    .output
                    .ok_or("Fit an implement in the finished-object cradle.")?,
                frame.revision,
            ),
            _ => return Err("The frame has no physical mounts.".into()),
        };
        if output.arcane_id == 0 {
            return Err("The fitted object has not been bound to finite Current.".into());
        }
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let target_instance = next_state
            .instance(output.arcane_id)
            .cloned()
            .ok_or("The fitted implement has no stable construction record.")?;
        let target_usable = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.item_clean_total(output.arcane_id))
            .map(crate::implements::usable_charge)
            .unwrap_or(0);
        let target_free = target_instance
            .usable_capacity()
            .saturating_sub(target_usable);
        if target_free == 0 {
            return Err("The receiving implement is visibly full.".into());
        }
        let (_, target_rate, target_dross_rate) =
            implement_transfer_properties(&target_instance.kind);
        let environmental_instability = self.arcane_geography.as_ref().map_or(0, |geography| {
            geography
                .dross_band_at(source_region)
                .stability_penalty_permille()
        });
        let requested = target_free
            .min(target_rate)
            .min(u64::from(layout.network_size.max(1)) * 32);
        if requested == 0 {
            return Err("The local conductor has no safe transfer budget.".into());
        }

        let (atlas_index, old_cell, old_sequence, old_exported, old_external_imported) = {
            let geography = self
                .arcane_geography
                .as_ref()
                .ok_or("The finite magical geography is unavailable.")?;
            (
                source_region.index(geography.manifest.side),
                geography.dynamic.cells[source_region.index(geography.manifest.side)],
                geography.dynamic.ecology.event_sequence,
                geography.dynamic.ecology.exported,
                geography.dynamic.dross_state.external_imported,
            )
        };
        let selected = self
            .arcane_geography
            .as_mut()
            .ok_or("The finite magical geography is unavailable.")?
            .export_ambient_for_apparatus(source_region, requested)
            .map_err(|error| error.to_string())?;
        let rollback_geography = |world: &mut World| {
            if let Some(geography) = world.arcane_geography.as_mut() {
                geography.dynamic.cells[atlas_index] = old_cell;
                geography.dynamic.ecology.event_sequence = old_sequence;
                geography.dynamic.ecology.exported = old_exported;
                geography.dynamic.dross_state.external_imported = old_external_imported;
            }
        };

        let amount = selected.total();
        let network_loss = u16::from(layout.network_size.saturating_sub(1)).saturating_mul(2);
        let dross_permille = target_dross_rate
            .saturating_add(network_loss)
            .saturating_add(environmental_instability)
            .clamp(1, 900);
        let dross_units = amount
            .saturating_mul(u64::from(dross_permille))
            .div_ceil(1_000)
            .min(amount.saturating_sub(1));
        let mut clean = selected.clone();
        let dross = if dross_units == 0 {
            Current::default()
        } else {
            match clean.take_units(dross_units, std::iter::empty()) {
                Ok(current) => current,
                Err(error) => {
                    rollback_geography(self);
                    return Err(error.to_string());
                }
            }
        };
        let operation_id = match next_state.operation_id() {
            Ok(id) => id,
            Err(error) => {
                rollback_geography(self);
                return Err(error.to_string());
            }
        };
        if let Some(record) = next_state.instances.get_mut(&output.arcane_id) {
            let instability = match &record.kind {
                ImplementKind::Wand { resolved, .. } => u64::from(
                    1_000u16
                        .saturating_sub(resolved.stability)
                        .saturating_add(resolved.saturation_instability),
                ),
                ImplementKind::Charm { stability, .. } => {
                    u64::from(1_000u16.saturating_sub(*stability))
                }
                ImplementKind::Vessel { containment, .. } => {
                    u64::from(1_000u16.saturating_sub(*containment))
                }
                ImplementKind::Fragments { .. } => 1_000,
            }
            .max(25);
            record.strain = record
                .strain
                .saturating_add(amount.saturating_mul(instability).div_ceil(300) as u32)
                .min(crate::implements::MAX_WAND_STRAIN);
            if matches!(
                &record.kind,
                ImplementKind::Wand { .. } | ImplementKind::Charm { .. }
            ) {
                record.wear = record
                    .wear
                    .saturating_add(
                        amount.saturating_mul(instability).div_ceil(60_000).max(1) as u32
                    )
                    .min(crate::implements::MAX_WAND_WEAR);
            }
        }
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "place_conductor_transfer".into(),
            instance_id: output.arcane_id,
            units: clean.total(),
            dross: dross.total(),
            actor: actor.into(),
            note: format!(
                "{} at {:?}; frame revision {frame_revision}; {} loaded segments",
                source_kind.label(),
                source_region,
                layout.network_size
            ),
        });
        let (manifest, mut files) = match self
            .arcane_geography
            .as_ref()
            .expect("geography was checked above")
            .linked_dynamic_replacements(&self.save_dir, operation_id)
        {
            Ok(prepared) => prepared,
            Err(error) => {
                rollback_geography(self);
                return Err(error.to_string());
            }
        };
        let implement_after = match next_state.encode() {
            Ok(bytes) => bytes,
            Err(error) => {
                rollback_geography(self);
                return Err(error.to_string());
            }
        };
        files.push(LinkedFileReplacement {
            subsystem: "implements".into(),
            operation_id,
            relative_path: crate::implements::IMPLEMENTS_FILE.into(),
            after: Some(implement_after),
        });
        let geography_owner = ArcaneOwner::Geography;
        let target_owner = ArcaneOwner::Item(output.arcane_id);
        let dross_owner = ArcaneOwner::ItemDross(output.arcane_id);
        if self.arcane_ledger.is_none() {
            rollback_geography(self);
            return Err("The world has no Current ledger.".into());
        }
        let transaction_id = match self
            .arcane_ledger
            .as_mut()
            .expect("ledger presence was checked")
            .system_transaction_id()
        {
            Ok(id) => id,
            Err(error) => {
                rollback_geography(self);
                return Err(error.to_string());
            }
        };
        let ledger = self
            .arcane_ledger
            .as_mut()
            .expect("ledger presence was checked");
        let mut reads = vec![
            AccountRead {
                owner: geography_owner.clone(),
                expected_version: ledger.version_of(&geography_owner),
            },
            AccountRead {
                owner: target_owner.clone(),
                expected_version: ledger.version_of(&target_owner),
            },
        ];
        let mut credits = vec![ArcaneMove {
            owner: target_owner,
            current: clean.clone(),
            content_id: Some(target_instance.content_id.clone()),
        }];
        if !dross.is_empty() {
            reads.push(AccountRead {
                owner: dross_owner.clone(),
                expected_version: ledger.version_of(&dross_owner),
            });
            credits.push(ArcaneMove {
                owner: dross_owner,
                current: dross.clone(),
                content_id: Some(target_instance.content_id.clone()),
            });
        }
        reads.sort_by(|a, b| a.owner.cmp(&b.owner));
        let transaction = ArcaneTransaction {
            id: transaction_id,
            reads,
            debits: vec![ArcaneMove {
                owner: geography_owner,
                current: selected,
                content_id: None,
            }],
            credits,
            transforms: Vec::new(),
            authority: ArcaneAuthority::System,
            reason: "loaded local conductor extraction".into(),
            content_id: target_instance.content_id.clone(),
            linked: Vec::new(),
        };
        if let Err(error) = ledger.commit_linked_files(transaction, files) {
            rollback_geography(self);
            return Err(error.to_string());
        }
        self.arcane_geography
            .as_mut()
            .expect("geography survived linked commit")
            .accept_linked_manifest(manifest);
        self.implements_state = Some(next_state);
        let revision = match self.block_entities.get_mut(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => {
                frame.revision = frame.revision.saturating_add(1);
                frame.revision
            }
            _ => frame_revision,
        };
        self.save_entities().map_err(|error| error.to_string())?;
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Transfer,
            message: format!(
                "The loaded conductor draws {} units from the local {} into the fitted implement.",
                amount,
                source_kind.label()
            ),
            preview: self.frame_preview(pos).ok(),
            lines: vec![format!(
                "{} units settled as retained dross.",
                dross.total()
            )],
        })
    }

    /// Drain a physically selected charged shard, biological harvest, warden
    /// material, or other declaratively arcane reservoir into the implement
    /// fitted to the frame. The registry definition supplies the same bounded
    /// conductivity and stability contract to base and mod content; no item
    /// name or client-authored transfer statistic is trusted here.
    fn transfer_selected_source_at_frame(
        &mut self,
        pos: BlockPos,
        source_stack: ItemStack,
        actor: &str,
    ) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        let (output, frame_revision) = match self.block_entities.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => (
                frame
                    .output
                    .ok_or("Fit an implement in the finished-object cradle.")?,
                frame.revision,
            ),
            _ => return Err("The frame has no physical mounts.".into()),
        };
        if output.arcane_id == 0 {
            return Err("The fitted object has not been bound to finite Current.".into());
        }
        if output.arcane_id == source_stack.arcane_id {
            return Err("The fitted implement cannot be its own charge source.".into());
        }
        let source_definition = self
            .reg
            .item(source_stack.item)
            .arcane
            .clone()
            .ok_or("The selected item declares no charge-source behavior.")?;
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let target_instance = next_state
            .instance(output.arcane_id)
            .cloned()
            .ok_or("The fitted implement has no stable construction record.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let source_owner = ArcaneOwner::Item(source_stack.arcane_id);
        let target_owner = ArcaneOwner::Item(output.arcane_id);
        let dross_owner = ArcaneOwner::ItemDross(output.arcane_id);
        let source_account = ledger
            .account(&source_owner)
            .cloned()
            .ok_or("The selected source has no finite Current custody.")?;
        let source_usable = crate::implements::usable_charge(source_account.current.total());
        if source_usable == 0 {
            return Err("The selected source is dormant.".into());
        }
        let target_usable = ledger
            .item_clean_total(output.arcane_id)
            .map(crate::implements::usable_charge)
            .unwrap_or(0);
        let target_free = target_instance
            .usable_capacity()
            .saturating_sub(target_usable);
        if target_free == 0 {
            return Err("The receiving implement is visibly full.".into());
        }
        let (_, target_rate, target_dross_rate) =
            implement_transfer_properties(&target_instance.kind);
        let source_rate = u64::from(source_definition.conductivity_permille)
            .saturating_mul(VESSEL_SAFE_TRANSFER)
            .div_ceil(1_000)
            .max(1);
        let conductor_rate = u64::from(layout.network_size.max(1)) * 32;
        let amount = source_usable
            .min(target_free)
            .min(source_rate)
            .min(target_rate)
            .min(conductor_rate);
        if amount == 0 {
            return Err("There is no usable Current within the local transfer bounds.".into());
        }

        let mut remaining = source_account.current.clone();
        let mut selected = remaining
            .take_units(amount, std::iter::empty())
            .map_err(|error| error.to_string())?;
        if remaining.total() < STRUCTURAL_SPARK_UNITS {
            return Err("The transfer would tear out the source's structural spark.".into());
        }
        let source_loss = 1_000u16.saturating_sub(source_definition.stability_permille) / 5;
        let network_loss = u16::from(layout.network_size.saturating_sub(1)).saturating_mul(2);
        let dross_permille = target_dross_rate
            .saturating_add(source_loss)
            .saturating_add(network_loss)
            .clamp(1, 500);
        let dross_units = amount
            .saturating_mul(u64::from(dross_permille))
            .div_ceil(1_000)
            .min(amount.saturating_sub(1));
        let dross = if dross_units == 0 {
            Current::default()
        } else {
            selected
                .take_units(dross_units, std::iter::empty())
                .map_err(|error| error.to_string())?
        };
        let mut debit_current = selected.clone();
        debit_current
            .checked_add(&dross)
            .map_err(|error| error.to_string())?;
        let mut debits = BTreeMap::new();
        add_current(&mut debits, source_owner, &debit_current)
            .map_err(|error| error.to_string())?;
        let mut credits = BTreeMap::new();
        add_current(&mut credits, target_owner, &selected).map_err(|error| error.to_string())?;
        if !dross.is_empty() {
            add_current(&mut credits, dross_owner, &dross).map_err(|error| error.to_string())?;
        }

        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        if let Some(record) = next_state.instances.get_mut(&output.arcane_id) {
            let instability = match &record.kind {
                ImplementKind::Wand { resolved, .. } => u64::from(
                    1_000u16
                        .saturating_sub(resolved.stability)
                        .saturating_add(resolved.saturation_instability),
                ),
                ImplementKind::Charm { stability, .. } => {
                    u64::from(1_000u16.saturating_sub(*stability))
                }
                ImplementKind::Vessel { containment, .. } => {
                    u64::from(1_000u16.saturating_sub(*containment))
                }
                ImplementKind::Fragments { .. } => 1_000,
            }
            .max(25);
            record.strain = record
                .strain
                .saturating_add(amount.saturating_mul(instability).div_ceil(350) as u32)
                .min(crate::implements::MAX_WAND_STRAIN);
            if matches!(
                record.kind,
                ImplementKind::Wand { .. } | ImplementKind::Charm { .. }
            ) {
                record.wear = record
                    .wear
                    .saturating_add(
                        amount.saturating_mul(instability).div_ceil(60_000).max(1) as u32
                    )
                    .min(crate::implements::MAX_WAND_WEAR);
            }
        }
        // A charged implement used as the selected reservoir also experiences
        // outgoing strain. Ordinary shard/biology records remain in the
        // arcane ledger and do not acquire fabricated implement metadata.
        if let Some(record) = next_state.instances.get_mut(&source_stack.arcane_id) {
            record.strain = record
                .strain
                .saturating_add(amount.div_ceil(4).max(1) as u32)
                .min(crate::implements::MAX_WAND_STRAIN);
        }
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "selected_source_transfer".into(),
            instance_id: output.arcane_id,
            units: selected.total(),
            dross: dross.total(),
            actor: actor.into(),
            note: format!(
                "{} ({}) -> {}; frame revision {frame_revision}",
                source_stack.arcane_id,
                self.reg.item(source_stack.item).name,
                output.arcane_id
            ),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &target_instance.content_id,
            "binding-frame selected reservoir transfer",
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
        let revision = match self.block_entities.get_mut(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => {
                frame.revision = frame.revision.saturating_add(1);
                frame.revision
            }
            _ => frame_revision,
        };
        self.save_entities().map_err(|error| error.to_string())?;
        let failed = self
            .implements_state
            .as_ref()
            .and_then(|state| state.instance(output.arcane_id))
            .is_some_and(|instance| {
                instance.wear >= crate::implements::MAX_WAND_WEAR
                    || instance.strain >= crate::implements::MAX_WAND_STRAIN
            });
        if failed {
            return self.fail_frame_output(
                pos,
                "selected reservoir transfer exceeded the implement's physical limit",
            );
        }
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Transfer,
            message: format!(
                "The selected {} routes {} units into the fitted implement.",
                self.reg.item(source_stack.item).label,
                amount
            ),
            preview: self.frame_preview(pos).ok(),
            lines: vec![format!(
                "{} units settled as retained dross.",
                dross.total()
            )],
        })
    }

    fn transfer_at_frame_with_limit(
        &mut self,
        pos: BlockPos,
        actor: &str,
        forced_limit: Option<u64>,
        calibration: bool,
    ) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        let (output, frame_revision) = match self.block_entities.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => (
                frame
                    .output
                    .ok_or("Fit an implement in the finished-object cradle.")?,
                frame.revision,
            ),
            _ => return Err("The frame has no physical mounts.".into()),
        };
        if output.arcane_id == 0 {
            return Err("The fitted object has not been bound to finite Current.".into());
        }
        let (vessel_pos, vessel_stack, vessel_revision) = self.adjacent_vessel(pos)?;
        if vessel_stack.arcane_id == 0 {
            return Err("Calibrate the adjacent vessel before transferring.".into());
        }
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let output_instance = next_state
            .instance(output.arcane_id)
            .cloned()
            .ok_or("The fitted implement has no stable construction record.")?;
        let vessel_instance = next_state
            .instance(vessel_stack.arcane_id)
            .cloned()
            .ok_or("The vessel has no stable construction record.")?;
        let environmental_instability = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .and_then(|region| {
                self.arcane_geography
                    .as_ref()
                    .map(|geography| geography.dross_band_at(region))
            })
            .map_or(0, crate::dross::DrossBand::stability_penalty_permille);
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let output_total = ledger.item_clean_total(output.arcane_id).unwrap_or(0);
        let vessel_total = ledger.item_clean_total(vessel_stack.arcane_id).unwrap_or(0);
        let output_usable = crate::implements::usable_charge(output_total);
        let vessel_usable = crate::implements::usable_charge(vessel_total);
        let output_free = output_instance
            .usable_capacity()
            .saturating_sub(output_usable);
        let vessel_free = vessel_instance
            .usable_capacity()
            .saturating_sub(vessel_usable);
        let to_output = calibration || (output_free != 0 && vessel_usable != 0);
        let (source_id, target_id, source_instance, target_instance, source_usable, target_free) =
            if to_output {
                (
                    vessel_stack.arcane_id,
                    output.arcane_id,
                    &vessel_instance,
                    &output_instance,
                    vessel_usable,
                    output_free,
                )
            } else {
                (
                    output.arcane_id,
                    vessel_stack.arcane_id,
                    &output_instance,
                    &vessel_instance,
                    output_usable,
                    vessel_free,
                )
            };
        if source_usable == 0 {
            return Err("The source implement is dormant.".into());
        }
        if target_free == 0 {
            return Err("The receiving implement is visibly full.".into());
        }
        let (_, source_rate, _) = implement_transfer_properties(&source_instance.kind);
        let (_, target_rate, target_dross_rate) =
            implement_transfer_properties(&target_instance.kind);
        let conductor_rate = u64::from(layout.network_size.max(1)) * 32;
        let amount = source_usable
            .min(target_free)
            .min(source_rate)
            .min(target_rate)
            .min(conductor_rate)
            .min(forced_limit.unwrap_or(u64::MAX));
        if amount == 0 || calibration && amount < 2 {
            return Err("There is not enough free, usable Current for that pulse.".into());
        }
        let source_owner = ArcaneOwner::Item(source_id);
        let target_owner = ArcaneOwner::Item(target_id);
        let dross_owner = ArcaneOwner::ItemDross(target_id);
        let source_account = ledger
            .account(&source_owner)
            .cloned()
            .ok_or("The source Current account is missing.")?;
        let mut available = source_account.current.clone();
        let mut selected = available
            .take_units(amount, std::iter::empty())
            .map_err(|error| error.to_string())?;
        if available.total() < STRUCTURAL_SPARK_UNITS {
            return Err("The transfer would tear out the source's structural spark.".into());
        }
        let network_dross = u64::from(layout.network_size.saturating_sub(1)) * 2;
        let dross_permille = u64::from(target_dross_rate)
            .saturating_add(network_dross)
            .saturating_add(u64::from(environmental_instability))
            .clamp(1, 900);
        let dross_units = amount
            .saturating_mul(dross_permille)
            .div_ceil(1_000)
            .min(amount.saturating_sub(1));
        let dross = if dross_units == 0 {
            Current::default()
        } else {
            selected
                .take_units(dross_units, std::iter::empty())
                .map_err(|error| error.to_string())?
        };
        let mut debits = BTreeMap::new();
        add_current(&mut debits, source_owner, &{
            let mut total = selected.clone();
            total
                .checked_add(&dross)
                .map_err(|error| error.to_string())?;
            total
        })
        .map_err(|error| error.to_string())?;
        let mut credits = BTreeMap::new();
        add_current(&mut credits, target_owner, &selected).map_err(|error| error.to_string())?;
        if !dross.is_empty() {
            add_current(&mut credits, dross_owner, &dross).map_err(|error| error.to_string())?;
        }
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        for (id, receiving) in [(source_id, false), (target_id, true)] {
            let Some(record) = next_state.instances.get_mut(&id) else {
                continue;
            };
            let load = if receiving {
                amount
            } else {
                amount.div_ceil(2)
            };
            let (strain_delta, wear_delta) = match &record.kind {
                ImplementKind::Wand { resolved, .. } => {
                    let instability = u64::from(1_000u16.saturating_sub(resolved.stability))
                        .saturating_add(u64::from(resolved.saturation_instability))
                        .clamp(25, 1_500);
                    (
                        load.saturating_mul(instability).div_ceil(250).max(1) as u32,
                        load.saturating_mul(instability).div_ceil(50_000).max(1) as u32,
                    )
                }
                ImplementKind::Charm { stability, .. } => {
                    let instability = u64::from(1_000u16.saturating_sub(*stability)).max(25);
                    (
                        load.saturating_mul(instability).div_ceil(500).max(1) as u32,
                        load.saturating_mul(instability).div_ceil(75_000).max(1) as u32,
                    )
                }
                ImplementKind::Vessel { containment, .. } => {
                    let exposure = u64::from(1_000u16.saturating_sub(*containment)).max(20);
                    (load.saturating_mul(exposure).div_ceil(400).max(1) as u32, 0)
                }
                ImplementKind::Fragments { .. } => (crate::implements::MAX_WAND_STRAIN, 0),
            };
            record.strain = record
                .strain
                .saturating_add(strain_delta)
                .min(crate::implements::MAX_WAND_STRAIN);
            record.wear = record
                .wear
                .saturating_add(wear_delta)
                .min(crate::implements::MAX_WAND_WEAR);
        }
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: if calibration {
                "calibration_pulse"
            } else {
                "transfer"
            }
            .into(),
            instance_id: target_id,
            units: selected.total(),
            dross: dross.total(),
            actor: actor.into(),
            note: format!(
                "{} -> {}; frame revision {frame_revision}; vessel revision {vessel_revision}",
                source_id, target_id
            ),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &target_instance.content_id,
            if calibration {
                "binding-frame calibration pulse"
            } else {
                "local conductor transfer"
            },
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
        if let Some(BlockEntity::BindingFrame(frame)) = self.block_entities.get_mut(&pos) {
            frame.revision = frame.revision.saturating_add(1);
        }
        if let Some(BlockEntity::ChargeVessel(vessel)) = self.block_entities.get_mut(&vessel_pos) {
            let after_usable = if target_id == vessel_stack.arcane_id {
                vessel_usable.saturating_add(selected.total())
            } else {
                vessel_usable.saturating_sub(amount)
            };
            let pressure = after_usable
                .saturating_mul(1_000)
                .checked_div(vessel_instance.usable_capacity().max(1))
                .unwrap_or(1_000)
                .min(2_000);
            let pressure_damage = pressure
                .saturating_sub(700)
                .saturating_mul(amount)
                .div_ceil(200_000) as u16;
            vessel.damage = vessel
                .damage
                .saturating_add(pressure_damage)
                .saturating_add(dross.total().min(4) as u16)
                .min(1_000);
            vessel.revision = vessel.revision.saturating_add(1);
        }
        self.save_entities().map_err(|error| error.to_string())?;
        let output_failed = self
            .implements_state
            .as_ref()
            .and_then(|state| state.instance(output.arcane_id))
            .is_some_and(|instance| {
                instance.wear >= crate::implements::MAX_WAND_WEAR
                    || instance.strain >= crate::implements::MAX_WAND_STRAIN
            });
        if output_failed {
            return self.fail_frame_output(
                pos,
                "transfer strain exceeded the implement's physical limit",
            );
        }
        let vessel_failed = self.block_entities.get(&vessel_pos).is_some_and(
            |entity| matches!(entity, BlockEntity::ChargeVessel(vessel) if vessel.damage >= 1_000),
        );
        if vessel_failed {
            self.fail_placed_vessel(
                vessel_pos,
                vessel_stack,
                "transfer pressure fractured the charge vessel",
            )?;
            self.save_entities().map_err(|error| error.to_string())?;
            return Ok(FrameResult {
                success: false,
                revision: self
                    .block_entities
                    .get(&pos)
                    .and_then(|entity| match entity {
                        BlockEntity::BindingFrame(frame) => Some(frame.revision),
                        _ => None,
                    })
                    .unwrap_or(frame_revision),
                cue: ImplementCue::Failure,
                message: "The overstrained vessel fractures visibly; recoverable fragments remain."
                    .into(),
                preview: self.frame_preview(pos).ok(),
                lines: vec![
                    "Released Current and retained dross were settled in the local ledger.".into(),
                ],
            });
        }
        let revision = match self.block_entities.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => frame.revision,
            _ => frame_revision,
        };
        Ok(FrameResult {
            success: true,
            revision,
            cue: if calibration {
                ImplementCue::Strain
            } else {
                ImplementCue::Transfer
            },
            message: if calibration {
                format!("A {amount}-unit pulse settles through the fitted implement.")
            } else if to_output {
                format!("The vessel routes {amount} units into the fitted implement.")
            } else {
                format!("The fitted implement routes {amount} units back into the vessel.")
            },
            preview: self.frame_preview(pos).ok(),
            lines: vec![format!(
                "{} units settled as retained dross.",
                dross.total()
            )],
        })
    }

    fn fail_frame_output(&mut self, pos: BlockPos, reason: &str) -> Result<FrameResult, String> {
        let output = match self.block_entities.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => frame
                .output
                .ok_or("The critically strained implement is no longer in the frame.")?,
            _ => return Err("The binding frame vanished before failure settled.".into()),
        };
        let fragments = self.fracture_implement_at(pos, output, reason)?;
        let revision = match self.block_entities.get_mut(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => {
                frame.output = None;
                frame.revision = frame.revision.saturating_add(1);
                frame.revision
            }
            _ => 0,
        };
        self.push_drop_at(pos, fragments);
        self.save_entities().map_err(|error| error.to_string())?;
        Ok(FrameResult {
            success: false,
            revision,
            cue: ImplementCue::Failure,
            message: "The implement fractures under visible strain; recoverable fragments fall clear.".into(),
            preview: self.frame_preview(pos).ok(),
            lines: vec!["Its remaining Current and dross were released through explicit ledger dispositions.".into()],
        })
    }

    fn discharge_at_frame(&mut self, pos: BlockPos, actor: &str) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        let (output, frame_revision) = match self.block_entities.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => (
                frame
                    .output
                    .ok_or("Fit an implement in the finished cradle.")?,
                frame.revision,
            ),
            _ => return Err("The frame has no mounts.".into()),
        };
        if output.arcane_id == 0 {
            return Err("The fitted object carries no bound Current.".into());
        }
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let instance = next_state
            .instance(output.arcane_id)
            .cloned()
            .ok_or("The fitted implement has no construction record.")?;
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let source_owner = ArcaneOwner::Item(output.arcane_id);
        let source = ledger
            .account(&source_owner)
            .cloned()
            .ok_or("The fitted implement is dormant.")?;
        let usable = crate::implements::usable_charge(source.current.total());
        if usable == 0 {
            return Err("The fitted implement is already dormant.".into());
        }
        let mut remaining = source.current.clone();
        let mut discharged = remaining
            .take_units(usable, std::iter::empty())
            .map_err(|error| error.to_string())?;
        let dross_units = usable.div_ceil(100).min(usable.saturating_sub(1));
        let dross = if dross_units == 0 {
            Current::default()
        } else {
            discharged
                .take_units(dross_units, std::iter::empty())
                .map_err(|error| error.to_string())?
        };
        let ambient = ArcaneOwner::Ambient(region);
        let environmental_dross = ArcaneOwner::Dross {
            region,
            medium: DrossMedium::Air,
        };
        let mut debits = BTreeMap::new();
        let mut total = discharged.clone();
        total
            .checked_add(&dross)
            .map_err(|error| error.to_string())?;
        add_current(&mut debits, source_owner, &total).map_err(|error| error.to_string())?;
        let mut credits = BTreeMap::new();
        add_current(&mut credits, ambient, &discharged).map_err(|error| error.to_string())?;
        if !dross.is_empty() {
            add_current(&mut credits, environmental_dross, &dross)
                .map_err(|error| error.to_string())?;
        }
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "safe_discharge".into(),
            instance_id: output.arcane_id,
            units: discharged.total(),
            dross: dross.total(),
            actor: actor.into(),
            note: format!(
                "frame revision {frame_revision}; containment {}",
                layout.containment
            ),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &instance.content_id,
            "binding-frame safe environmental discharge",
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
        let revision =
            if let Some(BlockEntity::BindingFrame(frame)) = self.block_entities.get_mut(&pos) {
                frame.revision = frame.revision.saturating_add(1);
                frame.revision
            } else {
                frame_revision
            };
        self.save_entities().map_err(|error| error.to_string())?;
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Transfer,
            message: format!(
                "Safely discharged {} usable Current into the local environment.",
                usable
            ),
            preview: self.frame_preview(pos).ok(),
            lines: vec![format!(
                "{} units became measured airborne dross.",
                dross.total()
            )],
        })
    }

    /// Settle every non-structural unit held by the fitted output before the
    /// work surface disappears. Ordinary mounted items are still spilled by
    /// the block-entity removal path, but a broken frame is not a portable
    /// charger: usable Current first fills its adjacent vessel, then returns
    /// to local Ambient custody, while retained and break-induced dross enters
    /// the environment. The fitted implement keeps one structural spark and
    /// therefore the same stable identity when it falls clear.
    pub(crate) fn settle_binding_frame_break_at(
        &mut self,
        pos: BlockPos,
        controlled: bool,
    ) -> Result<(), String> {
        let output = match self.block_entities.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => frame.output,
            _ => None,
        };
        let Some(output) = output else {
            return Ok(());
        };
        if output.arcane_id == 0 {
            return Ok(());
        }

        let layout = self.binding_frame_layout(pos);
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let adjacent_vessel = self.adjacent_vessel(pos).ok();
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let instance = next_state
            .instance(output.arcane_id)
            .cloned()
            .ok_or("The fitted implement has no construction record.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;

        let clean_owner = ArcaneOwner::Item(output.arcane_id);
        let retained_owner = ArcaneOwner::ItemDross(output.arcane_id);
        let clean = ledger
            .account(&clean_owner)
            .map(|account| account.current.clone())
            .ok_or("The fitted implement has no finite Current custody.")?;
        let retained = ledger
            .account(&retained_owner)
            .map(|account| account.current.clone())
            .unwrap_or_default();
        let usable = crate::implements::usable_charge(clean.total());
        if usable == 0 && retained.is_empty() {
            return Ok(());
        }

        let mut clean_left = clean.clone();
        let clean_debit = clean_left
            .take_units(usable, std::iter::empty())
            .map_err(|error| error.to_string())?;
        let mut routed_clean = clean_debit.clone();
        let exposure = if controlled {
            1_000u64.saturating_sub(u64::from(layout.containment))
        } else {
            1_000
        };
        let dross_divisor = if controlled { 20_000 } else { 4_000 };
        let induced_units = routed_clean
            .total()
            .saturating_mul(exposure)
            .div_ceil(dross_divisor)
            .min(routed_clean.total());
        let induced_dross = routed_clean
            .take_units(induced_units, std::iter::empty())
            .map_err(|error| error.to_string())?;

        let mut debits = BTreeMap::new();
        if !clean_debit.is_empty() {
            add_current(&mut debits, clean_owner, &clean_debit)
                .map_err(|error| error.to_string())?;
        }
        if !retained.is_empty() {
            add_current(&mut debits, retained_owner, &retained)
                .map_err(|error| error.to_string())?;
        }

        let mut credits = BTreeMap::new();
        if let Some((_, vessel, _)) = adjacent_vessel
            && vessel.arcane_id != 0
            && vessel.arcane_id != output.arcane_id
            && let Some(vessel_instance) = next_state.instance(vessel.arcane_id)
        {
            let vessel_usable = ledger
                .item_clean_total(vessel.arcane_id)
                .map(crate::implements::usable_charge)
                .unwrap_or_default();
            let free = vessel_instance
                .usable_capacity()
                .saturating_sub(vessel_usable);
            let into_vessel_units = free.min(routed_clean.total());
            if into_vessel_units != 0 {
                let into_vessel = routed_clean
                    .take_units(into_vessel_units, std::iter::empty())
                    .map_err(|error| error.to_string())?;
                add_current(
                    &mut credits,
                    ArcaneOwner::Item(vessel.arcane_id),
                    &into_vessel,
                )
                .map_err(|error| error.to_string())?;
            }
        }
        if !routed_clean.is_empty() {
            add_current(&mut credits, ArcaneOwner::Ambient(region), &routed_clean)
                .map_err(|error| error.to_string())?;
        }
        let mut environmental_dross = retained.clone();
        environmental_dross
            .checked_add(&induced_dross)
            .map_err(|error| error.to_string())?;
        if !environmental_dross.is_empty() {
            add_current(
                &mut credits,
                ArcaneOwner::Dross {
                    region,
                    medium: if controlled {
                        DrossMedium::Soil
                    } else {
                        DrossMedium::Air
                    },
                },
                &environmental_dross,
            )
            .map_err(|error| error.to_string())?;
        }

        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "frame_break".into(),
            instance_id: output.arcane_id,
            units: clean_debit.total(),
            dross: environmental_dross.total(),
            actor: if controlled {
                "controlled dismantle".into()
            } else {
                "destructive break".into()
            },
            note: format!(
                "containment {}; stable output identity spilled at {pos:?}",
                layout.containment
            ),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &instance.content_id,
            "binding-frame break disposition",
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
        Ok(())
    }

    fn disassemble_at_frame(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
        selected: Option<ItemStack>,
        actor: &str,
    ) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        if !selected.is_some_and(|stack| self.reg.item(stack.item).shears) {
            return Err("Safe disassembly requires shears in the selected hand.".into());
        }
        let (output, frame_revision) = match self.block_entities.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => (
                frame
                    .output
                    .ok_or("Fit a bound implement in the finished cradle.")?,
                frame.revision,
            ),
            _ => return Err("The frame has no mounts.".into()),
        };
        if output.arcane_id == 0 {
            return Err("That object has no stable implement identity to disassemble.".into());
        }
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let instance = next_state
            .instances
            .get(&output.arcane_id)
            .cloned()
            .ok_or("The fitted implement has no construction record.")?;
        let source_kind = match &instance.kind {
            ImplementKind::Fragments { source, .. } => source.as_ref().clone(),
            kind => kind.clone(),
        };
        let mut recovered_names = Vec::new();
        let mut unavailable = Vec::new();
        for component in &instance.construction {
            let compatible = self
                .reg
                .item_id(&component.content_id)
                .is_some_and(|item| self.reg.item(item).materials == component.materials);
            if compatible {
                recovered_names.push(component.content_id.clone());
            } else {
                unavailable.push(component.clone());
            }
        }
        if recovered_names.is_empty()
            && !unavailable.is_empty()
            && matches!(instance.kind, ImplementKind::Fragments { .. })
        {
            return Err(
                "No saved fragment component has a compatible registered item; restore its content pack or keep the conserved bundle."
                    .into(),
            );
        }
        let leaves_fragment = !unavailable.is_empty();
        let fragment_content = self
            .reg
            .item_id("base:implement_fragment")
            .map(|item| self.reg.item(item).name.clone())
            .ok_or("The conserved fragment-bundle content definition is missing.")?;
        if leaves_fragment {
            let record = next_state
                .instances
                .get_mut(&output.arcane_id)
                .ok_or("The fitted implement vanished during disassembly.")?;
            let pieces = unavailable.len().clamp(1, u8::MAX as usize) as u8;
            record.kind = ImplementKind::Fragments {
                source: Box::new(source_kind),
                pieces,
            };
            record.construction = unavailable;
            record.content_id = fragment_content.clone();
            record.wear = crate::implements::MAX_WAND_WEAR;
            record.strain = crate::implements::MAX_WAND_STRAIN;
        } else {
            next_state.instances.remove(&output.arcane_id);
        }
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let adjacent_vessel = self.adjacent_vessel(pos).ok();
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let clean_owner = ArcaneOwner::Item(output.arcane_id);
        let dross_owner = ArcaneOwner::ItemDross(output.arcane_id);
        let clean = ledger
            .account(&clean_owner)
            .map(|account| account.current.clone())
            .unwrap_or_default();
        let dross = ledger
            .account(&dross_owner)
            .map(|account| account.current.clone())
            .unwrap_or_default();
        if clean.is_empty() && dross.is_empty() {
            return Err("The fitted identity names no finite Current custody.".into());
        }
        let mut debits = BTreeMap::new();
        if !clean.is_empty() {
            add_current(&mut debits, clean_owner, &clean).map_err(|error| error.to_string())?;
        }
        if !dross.is_empty() {
            add_current(&mut debits, dross_owner, &dross).map_err(|error| error.to_string())?;
        }
        let mut credits = BTreeMap::new();
        let mut clean_remaining = clean.clone();
        if leaves_fragment {
            let spark = clean_remaining
                .take_units(STRUCTURAL_SPARK_UNITS, std::iter::empty())
                .map_err(|error| error.to_string())?;
            add_current(&mut credits, ArcaneOwner::Item(output.arcane_id), &spark)
                .map_err(|error| error.to_string())?;
        }
        if let Some((_, vessel, _)) = adjacent_vessel
            && vessel.arcane_id != 0
            && let Some(vessel_instance) = next_state.instance(vessel.arcane_id)
        {
            let vessel_usable = ledger
                .item_clean_total(vessel.arcane_id)
                .map(crate::implements::usable_charge)
                .unwrap_or(0);
            let free = vessel_instance
                .usable_capacity()
                .saturating_sub(vessel_usable);
            let take = free.min(clean_remaining.total());
            if take != 0 {
                let into_vessel = clean_remaining
                    .take_units(take, std::iter::empty())
                    .map_err(|error| error.to_string())?;
                add_current(
                    &mut credits,
                    ArcaneOwner::Item(vessel.arcane_id),
                    &into_vessel,
                )
                .map_err(|error| error.to_string())?;
            }
        }
        if !clean_remaining.is_empty() {
            add_current(&mut credits, ArcaneOwner::Ambient(region), &clean_remaining)
                .map_err(|error| error.to_string())?;
        }
        if !dross.is_empty() {
            add_current(
                &mut credits,
                ArcaneOwner::Dross {
                    region,
                    medium: DrossMedium::Soil,
                },
                &dross,
            )
            .map_err(|error| error.to_string())?;
        }
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "safe_disassembly".into(),
            instance_id: output.arcane_id,
            units: clean.total(),
            dross: dross.total(),
            actor: actor.into(),
            note: format!(
                "frame revision {frame_revision}; {} compatible component(s) recovered, {} retained in conserved bundle",
                recovered_names.len(),
                usize::from(leaves_fragment)
            ),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            if leaves_fragment {
                &fragment_content
            } else {
                &instance.content_id
            },
            "binding-frame safe implement disassembly",
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
        let Some(BlockEntity::BindingFrame(frame)) = self.block_entities.get_mut(&pos) else {
            return Err("The frame vanished after disassembly committed.".into());
        };
        frame.output = None;
        frame.revision = frame.revision.saturating_add(1);
        let revision = frame.revision;
        for name in &recovered_names {
            let Some(item) = self.reg.item_id(name) else {
                let Some(fragment) = self.reg.item_id("base:implement_fragment") else {
                    continue;
                };
                self.push_drop_at(pos, ItemStack::new(&self.reg, fragment, 1));
                continue;
            };
            let stack = ItemStack::new(&self.reg, item, 1);
            let left = inventory.add_stack(&self.reg, stack);
            if left != 0 {
                self.push_drop_at(
                    pos,
                    ItemStack {
                        count: left,
                        ..stack
                    },
                );
            }
        }
        if leaves_fragment && let Some(fragment) = self.reg.item_id("base:implement_fragment") {
            self.push_drop_at(
                pos,
                ItemStack {
                    item: fragment,
                    count: 1,
                    durability: self.reg.item(fragment).durability,
                    arcane_id: output.arcane_id,
                },
            );
        }
        self.save_entities().map_err(|error| error.to_string())?;
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Use,
            message: format!(
                "Safely disassembled the implement into {} recoverable part(s){}.",
                recovered_names.len(),
                if leaves_fragment {
                    " and one conserved unavailable-component bundle"
                } else {
                    ""
                }
            ),
            preview: self.frame_preview(pos).ok(),
            lines: vec![format!(
                "Returned {} Current units and {} dross units without duplication.",
                clean.total(),
                dross.total()
            )],
        })
    }

    fn swap_focus_at_frame(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
        actor: &str,
    ) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        let (output, replacement_focus, frame_revision) = match self.block_entities.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => (
                frame
                    .output
                    .ok_or("Fit a bound wand in the finished cradle.")?,
                frame.focus.ok_or("Mount the replacement focus first.")?,
                frame.revision,
            ),
            _ => return Err("The frame has no mounts.".into()),
        };
        let replacement_definition = self
            .reg
            .item(replacement_focus.item)
            .wand_component
            .clone()
            .filter(|definition| definition.role == ComponentRole::Focus)
            .ok_or("The focus mount does not contain a declared focus.")?;
        let replacement_id = self.reg.item(replacement_focus.item).name.clone();
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let instance = next_state
            .instance(output.arcane_id)
            .cloned()
            .ok_or("The fitted wand has no construction record.")?;
        let ImplementKind::Wand { parts, .. } = instance.kind else {
            return Err("Only a bound wand has a replaceable focus.".into());
        };
        let old_focus = parts.focus.clone();
        let old_focus_item = self.reg.item_id(&old_focus).ok_or(
            "The installed focus's content pack is unavailable; safely disassemble the wand so its exact matter remains in a conserved fragment bundle.",
        )?;
        let component = |id: &str| {
            self.reg
                .item_id(id)
                .and_then(|item| self.reg.item(item).wand_component.clone())
                .or_else(|| next_state.component_manifests.get(id).cloned())
                .ok_or_else(|| format!("The saved component definition for {id} is unavailable."))
        };
        let declared = [
            (parts.body.clone(), component(&parts.body)?),
            (parts.reservoir.clone(), component(&parts.reservoir)?),
            (replacement_id.clone(), replacement_definition.clone()),
            (parts.binding.clone(), component(&parts.binding)?),
        ];
        let (new_parts, resolved) =
            crate::implements::resolve_wand(&declared).map_err(|error| error.to_string())?;
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let wand_owner = ArcaneOwner::Item(output.arcane_id);
        let wand_total = ledger
            .item_clean_total(output.arcane_id)
            .ok_or("The wand's Current account is missing.")?;
        let wand_usable = crate::implements::usable_charge(wand_total);
        let mut debits = BTreeMap::new();
        let mut credits = BTreeMap::new();
        let mut incoming_clean = Current::default();
        let mut incoming_dross = Current::default();
        if replacement_focus.arcane_id != 0 {
            for (owner, dross) in [
                (ArcaneOwner::Item(replacement_focus.arcane_id), false),
                (ArcaneOwner::ItemDross(replacement_focus.arcane_id), true),
            ] {
                if let Some(account) = ledger.account(&owner) {
                    add_current(&mut debits, owner, &account.current)
                        .map_err(|error| error.to_string())?;
                    if dross {
                        incoming_dross
                            .checked_add(&account.current)
                            .map_err(|error| error.to_string())?;
                    } else {
                        incoming_clean
                            .checked_add(&account.current)
                            .map_err(|error| error.to_string())?;
                    }
                }
            }
        }
        let free = resolved.capacity.saturating_sub(wand_usable);
        let into_wand_units = free.min(incoming_clean.total());
        let into_wand = if into_wand_units == 0 {
            Current::default()
        } else {
            incoming_clean
                .take_units(into_wand_units, resolved.resonance.keys().cloned())
                .map_err(|error| error.to_string())?
        };
        if !into_wand.is_empty() {
            add_current(&mut credits, wand_owner.clone(), &into_wand)
                .map_err(|error| error.to_string())?;
        }
        if !incoming_dross.is_empty() {
            add_current(
                &mut credits,
                ArcaneOwner::ItemDross(output.arcane_id),
                &incoming_dross,
            )
            .map_err(|error| error.to_string())?;
        }
        if !incoming_clean.is_empty() {
            add_current(&mut credits, ArcaneOwner::Ambient(region), &incoming_clean)
                .map_err(|error| error.to_string())?;
        }
        if debits.is_empty() {
            let account = ledger
                .account(&wand_owner)
                .ok_or("The wand has no structural spark.")?;
            let mut one = account.current.clone();
            let one = one
                .take_units(STRUCTURAL_SPARK_UNITS, std::iter::empty())
                .map_err(|error| error.to_string())?;
            add_current(&mut debits, wand_owner.clone(), &one)
                .map_err(|error| error.to_string())?;
            add_current(&mut credits, wand_owner.clone(), &one)
                .map_err(|error| error.to_string())?;
        }
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        next_state
            .component_manifests
            .insert(replacement_id.clone(), replacement_definition);
        let Some(record) = next_state.instances.get_mut(&output.arcane_id) else {
            return Err("The fitted wand vanished from implement state.".into());
        };
        record.kind = ImplementKind::Wand {
            parts: new_parts,
            resolved: resolved.clone(),
        };
        let saved_focus = record
            .construction
            .iter_mut()
            .find(|component| component.content_id == old_focus)
            .ok_or("The wand's saved physical focus bill is missing.")?;
        if self.reg.item(old_focus_item).materials != saved_focus.materials {
            return Err("The installed focus's material identity changed; an explicit content migration is required.".into());
        }
        *saved_focus = physical_component(&self.reg, replacement_focus);
        record.strain = record
            .strain
            .saturating_add(20)
            .min(crate::implements::MAX_WAND_STRAIN);
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "swap_focus".into(),
            instance_id: output.arcane_id,
            units: into_wand.total(),
            dross: incoming_dross.total(),
            actor: actor.into(),
            note: format!("{old_focus} -> {replacement_id}; frame revision {frame_revision}"),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &instance.content_id,
            "binding-frame focus swap",
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
        let Some(BlockEntity::BindingFrame(frame)) = self.block_entities.get_mut(&pos) else {
            return Err("The frame vanished after focus swap committed.".into());
        };
        frame.focus = None;
        frame.revision = frame.revision.saturating_add(1);
        let revision = frame.revision;
        let old_stack = ItemStack::new(&self.reg, old_focus_item, 1);
        let left = inventory.add_stack(&self.reg, old_stack);
        if left != 0 {
            self.push_drop_at(
                pos,
                ItemStack {
                    count: left,
                    ..old_stack
                },
            );
        }
        self.save_entities().map_err(|error| error.to_string())?;
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Use,
            message: format!(
                "Rebound the wand around {}.",
                self.reg.item(replacement_focus.item).label
            ),
            preview: Some(resolved),
            lines: vec![format!(
                "Recovered {}; routed {} Current and {} retained dross from the replacement.",
                old_focus,
                into_wand.total(),
                incoming_dross.total()
            )],
        })
    }

    fn repair_at_frame(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
        selected_slot: usize,
        actor: &str,
    ) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        let (output, frame_revision) = match self.block_entities.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => (
                frame
                    .output
                    .ok_or("Fit a worn implement in the finished cradle.")?,
                frame.revision,
            ),
            _ => return Err("The frame has no mounts.".into()),
        };
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let instance = next_state
            .instance(output.arcane_id)
            .cloned()
            .ok_or("The fitted implement has no construction record.")?;
        if instance.wear == 0 && instance.strain == 0 {
            return Err("The fitted implement is already sound.".into());
        }
        let mut repair_materials = Vec::new();
        match &instance.kind {
            ImplementKind::Wand { parts, .. } => {
                for id in [&parts.body, &parts.binding] {
                    if let Some(component) = self
                        .reg
                        .item_id(id)
                        .and_then(|item| self.reg.item(item).wand_component.as_ref())
                        .or_else(|| next_state.component_manifests.get(id))
                    {
                        repair_materials.push(component.repair_material.clone());
                    }
                }
            }
            ImplementKind::Charm { .. } => repair_materials.push("base:leather_strip".into()),
            ImplementKind::Vessel { .. } => repair_materials.push("base:glass".into()),
            ImplementKind::Fragments { .. } => {
                return Err("Sort the conserved fragment bundle with safe disassembly before attempting repair.".into());
            }
        }
        let hammer_slot = (0..inventory.slots.len())
            .find(|&slot| {
                inventory.slots[slot].is_some_and(|stack| self.reg.item(stack.item).hammer)
            })
            .ok_or("Repair requires a hammer in the pack.")?;
        let material_slot = if inventory.slots[selected_slot]
            .is_some_and(|stack| repair_materials.contains(&self.reg.item(stack.item).name))
        {
            Some(selected_slot)
        } else {
            (0..inventory.slots.len()).find(|&slot| {
                inventory.slots[slot]
                    .is_some_and(|stack| repair_materials.contains(&self.reg.item(stack.item).name))
            })
        }
        .ok_or_else(|| format!("Repair needs one of: {}.", repair_materials.join(", ")))?;
        let repair_stack = inventory.slots[material_slot]
            .ok_or("The matching repair material moved before use.")?;
        let repair_matter = crate::materials::stack_materials(
            &self.reg,
            ItemStack {
                count: 1,
                ..repair_stack
            },
        );
        let staged_material = if repair_matter.is_empty() {
            None
        } else {
            self.material_ledger
                .as_ref()
                .ok_or("The world has no finite material ledger.")?
                .stage_linked_recipe_loss(&repair_matter)
                .map_err(|error| error.to_string())?
        };
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let target_owner = ArcaneOwner::Item(output.arcane_id);
        let mut debits = BTreeMap::new();
        let mut credits = BTreeMap::new();
        let mut routed = 0u64;
        let mut retained_dross = 0u64;
        if repair_stack.arcane_id != 0 {
            let target_free = instance.usable_capacity().saturating_sub(
                ledger
                    .item_clean_total(output.arcane_id)
                    .map(crate::implements::usable_charge)
                    .unwrap_or(0),
            );
            if let Some(account) = ledger.account(&ArcaneOwner::Item(repair_stack.arcane_id)) {
                let mut incoming = account.current.clone();
                add_current(
                    &mut debits,
                    ArcaneOwner::Item(repair_stack.arcane_id),
                    &incoming,
                )
                .map_err(|error| error.to_string())?;
                let into_target = incoming
                    .take_units(target_free.min(incoming.total()), std::iter::empty())
                    .map_err(|error| error.to_string())?;
                routed = into_target.total();
                if !into_target.is_empty() {
                    add_current(&mut credits, target_owner.clone(), &into_target)
                        .map_err(|error| error.to_string())?;
                }
                if !incoming.is_empty() {
                    add_current(&mut credits, ArcaneOwner::Ambient(region), &incoming)
                        .map_err(|error| error.to_string())?;
                }
            }
            if let Some(account) = ledger.account(&ArcaneOwner::ItemDross(repair_stack.arcane_id)) {
                retained_dross = account.current.total();
                add_current(
                    &mut debits,
                    ArcaneOwner::ItemDross(repair_stack.arcane_id),
                    &account.current,
                )
                .map_err(|error| error.to_string())?;
                add_current(
                    &mut credits,
                    ArcaneOwner::ItemDross(output.arcane_id),
                    &account.current,
                )
                .map_err(|error| error.to_string())?;
            }
        }
        if debits.is_empty() {
            let account = ledger
                .account(&target_owner)
                .ok_or("The fitted implement has no structural spark.")?;
            let mut pulse = account.current.clone();
            let pulse = pulse
                .take_units(STRUCTURAL_SPARK_UNITS, std::iter::empty())
                .map_err(|error| error.to_string())?;
            add_current(&mut debits, target_owner.clone(), &pulse)
                .map_err(|error| error.to_string())?;
            add_current(&mut credits, target_owner.clone(), &pulse)
                .map_err(|error| error.to_string())?;
        }
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        let record = next_state
            .instances
            .get_mut(&output.arcane_id)
            .ok_or("The fitted implement vanished from state.")?;
        let wear_before = record.wear;
        let strain_before = record.strain;
        record.wear = record.wear.saturating_sub(250);
        record.strain = record.strain.saturating_sub(1_000);
        let wear_after = record.wear;
        let strain_after = record.strain;
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "repair".into(),
            instance_id: output.arcane_id,
            units: routed,
            dross: retained_dross,
            actor: actor.into(),
            note: format!(
                "{}; wear {wear_before}->{}, strain {strain_before}->{}; frame revision {frame_revision}",
                self.reg.item(repair_stack.item).name,
                wear_after,
                strain_after
            ),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &instance.content_id,
            "binding-frame matching-material repair",
        )?;
        let replacement = LinkedFileReplacement {
            subsystem: "implements".into(),
            operation_id,
            relative_path: crate::implements::IMPLEMENTS_FILE.into(),
            after: Some(next_state.encode().map_err(|error| error.to_string())?),
        };
        let mut replacements = vec![replacement];
        if let Some((_, bytes)) = &staged_material {
            replacements.push(LinkedFileReplacement {
                subsystem: "materials".into(),
                operation_id,
                relative_path: crate::materials::MaterialLedger::linked_delta_path().into(),
                after: Some(bytes.clone()),
            });
        }
        ledger
            .commit_linked_files(transaction, replacements)
            .map_err(|error| error.to_string())?;
        self.implements_state = Some(next_state);
        if let Some((next_material, _)) = staged_material {
            self.material_ledger = Some(next_material);
        }
        inventory.take_one_stack(material_slot);
        inventory.wear_tool(&self.reg, hammer_slot);
        let revision =
            if let Some(BlockEntity::BindingFrame(frame)) = self.block_entities.get_mut(&pos) {
                frame.revision = frame.revision.saturating_add(1);
                frame.revision
            } else {
                frame_revision
            };
        self.save_entities().map_err(|error| error.to_string())?;
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Strain,
            message: format!(
                "Repaired the implement with {}.",
                self.reg.item(repair_stack.item).label
            ),
            preview: self.frame_preview(pos).ok(),
            lines: vec![format!(
                "Routed {routed} carried Current; retained {retained_dross} dross."
            )],
        })
    }

    /// Bounded physical upkeep for placed charge vessels. The server calls
    /// this once per five seconds, so a no-magic world pays one cheap empty
    /// block-entity scan at that cadence rather than work on every 30 Hz tick.
    pub fn tick_implements(&mut self, cursor: &mut usize) {
        let mut vessel_positions: Vec<BlockPos> = self
            .block_entities
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
            .filter_map(|pos| match self.block_entities.get(pos) {
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
                if let Some(BlockEntity::ChargeVessel(state)) = self.block_entities.get_mut(&pos) {
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

    fn vessel_containment_at(&self, pos: BlockPos, instance_id: u64) -> u16 {
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

    fn leak_placed_vessel(
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
        if let Some(BlockEntity::ChargeVessel(state)) = self.block_entities.get_mut(&pos) {
            state.revision = state.revision.saturating_add(1);
        }
        Ok(())
    }

    fn fail_placed_vessel(
        &mut self,
        pos: BlockPos,
        stack: ItemStack,
        reason: &str,
    ) -> Result<(), String> {
        let fragments = self.fracture_implement_at(pos, stack, reason)?;
        self.block_entities.remove(&pos);
        self.set_block_at(pos, AIR);
        self.push_drop_at(pos, fragments);
        Ok(())
    }

    /// Turn a failed implement into one stable, non-stackable bundle whose
    /// sidecar still contains the exact original construction bill. Only the
    /// structural spark remains bound; useful charge and retained dross are
    /// released through the same durable transaction.
    fn fracture_implement_at(
        &mut self,
        pos: BlockPos,
        stack: ItemStack,
        reason: &str,
    ) -> Result<ItemStack, String> {
        let fragment_item = self
            .reg
            .item_id("base:implement_fragment")
            .ok_or("The conserved fragment-bundle content definition is missing.")?;
        let fragment_content = self.reg.item(fragment_item).name.clone();
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let original = next_state
            .instance(stack.arcane_id)
            .cloned()
            .ok_or("The failing implement has no construction record.")?;
        if matches!(original.kind, ImplementKind::Fragments { .. }) {
            return Err("A fragment bundle cannot recursively fracture.".into());
        }
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
            .ok_or("The failing implement has no structural Current custody.")?;
        if clean.total() < STRUCTURAL_SPARK_UNITS {
            return Err("The failing implement has lost its structural spark.".into());
        }
        let retained = ledger
            .account(&dross_owner)
            .map(|account| account.current.clone())
            .unwrap_or_default();
        let mut released = clean.clone();
        let spark = released
            .take_units(STRUCTURAL_SPARK_UNITS, std::iter::empty())
            .map_err(|error| error.to_string())?;
        let destabilized = released
            .take_units(released.total().div_ceil(2), std::iter::empty())
            .map_err(|error| error.to_string())?;
        let mut all_dross = destabilized;
        all_dross
            .checked_add(&retained)
            .map_err(|error| error.to_string())?;

        let mut debits = BTreeMap::new();
        add_current(&mut debits, clean_owner.clone(), &clean).map_err(|error| error.to_string())?;
        if !retained.is_empty() {
            add_current(&mut debits, dross_owner, &retained).map_err(|error| error.to_string())?;
        }
        let mut credits = BTreeMap::new();
        add_current(&mut credits, clean_owner, &spark).map_err(|error| error.to_string())?;
        if !released.is_empty() {
            add_current(&mut credits, ArcaneOwner::Ambient(region), &released)
                .map_err(|error| error.to_string())?;
        }
        if !all_dross.is_empty() {
            add_current(
                &mut credits,
                ArcaneOwner::Dross {
                    region,
                    medium: DrossMedium::Air,
                },
                &all_dross,
            )
            .map_err(|error| error.to_string())?;
        }

        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        let record = next_state
            .instances
            .get_mut(&stack.arcane_id)
            .ok_or("The failing implement vanished from state.")?;
        let pieces = record.construction.len().clamp(1, u8::MAX as usize) as u8;
        record.kind = ImplementKind::Fragments {
            source: Box::new(original.kind),
            pieces,
        };
        record.content_id = fragment_content.clone();
        record.wear = crate::implements::MAX_WAND_WEAR;
        record.strain = crate::implements::MAX_WAND_STRAIN;
        record.provenance = format!("fractured:{}", original.instance_id);
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "catastrophic_fragmentation".into(),
            instance_id: stack.arcane_id,
            units: released.total(),
            dross: all_dross.total(),
            actor: "world".into(),
            note: reason.into(),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &fragment_content,
            "visible conserved implement fragmentation",
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
        Ok(ItemStack {
            item: fragment_item,
            count: 1,
            durability: self.reg.item(fragment_item).durability,
            arcane_id: stack.arcane_id,
        })
    }

    /// Remove a destroyed/despawned implement and its sidecar identity in the
    /// same durable commit that settles every clean and retained-dross unit.
    /// This is deliberately separate from safe frame disassembly: an unsafe
    /// path yields fragments only where a physical catastrophe leaves them.
    pub(super) fn retire_implement_at(
        &mut self,
        pos: BlockPos,
        stack: ItemStack,
        reason: &str,
    ) -> Result<(), String> {
        let heat_fracture = reason.contains("lava") || reason.contains("burned in fire");
        if heat_fracture
            && self
                .implements_state
                .as_ref()
                .and_then(|state| state.instance(stack.arcane_id))
                .is_some_and(|instance| {
                    matches!(
                        instance.kind,
                        ImplementKind::Wand { .. } | ImplementKind::Vessel { .. }
                    )
                })
        {
            let fragments = self.fracture_implement_at(pos, stack, reason)?;
            self.push_drop_at(pos, fragments);
            return Ok(());
        }
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let instance = next_state
            .instances
            .remove(&stack.arcane_id)
            .ok_or("The destroyed implement has no construction record.")?;
        let tracked_materials = instance
            .tracked_materials()
            .map_err(|error| error.to_string())?;
        let material_consumed = [
            "consumed",
            "transformed",
            "fitted",
            "composted",
            "ignition",
            "placed into the environment",
        ]
        .iter()
        .any(|needle| reason.contains(needle));
        let staged_material = if tracked_materials.is_empty() {
            None
        } else {
            let material_ledger = self
                .material_ledger
                .as_ref()
                .ok_or("The world has no finite material ledger.")?;
            if material_consumed {
                material_ledger.stage_linked_recipe_loss(&tracked_materials)
            } else {
                material_ledger.stage_linked_bury_materials(pos, &tracked_materials, reason)
            }
            .map_err(|error| error.to_string())?
        };
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let disposition = self
            .reg
            .items
            .get(stack.item.0 as usize)
            .and_then(|definition| definition.arcane.as_ref())
            .map_or(crate::registry::ArcaneDisposition::Dross, |arcane| {
                arcane.on_destroy
            });
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
        let retained = ledger
            .account(&dross_owner)
            .map(|account| account.current.clone())
            .unwrap_or_default();
        if clean.is_empty() && retained.is_empty() {
            return Err("The destroyed implement names no finite Current custody.".into());
        }
        let clean_destination = match disposition {
            crate::registry::ArcaneDisposition::Ambient => ArcaneOwner::Ambient(region),
            crate::registry::ArcaneDisposition::Dross
            | crate::registry::ArcaneDisposition::Scar => ArcaneOwner::Dross {
                region,
                medium: if heat_fracture {
                    DrossMedium::Air
                } else {
                    DrossMedium::Soil
                },
            },
        };
        let dross_destination = ArcaneOwner::Dross {
            region,
            medium: if heat_fracture {
                DrossMedium::Air
            } else {
                DrossMedium::Soil
            },
        };
        let mut debits = BTreeMap::new();
        let mut credits = BTreeMap::new();
        if !clean.is_empty() {
            add_current(&mut debits, clean_owner, &clean).map_err(|error| error.to_string())?;
            add_current(&mut credits, clean_destination, &clean)
                .map_err(|error| error.to_string())?;
        }
        if !retained.is_empty() {
            add_current(&mut debits, dross_owner, &retained).map_err(|error| error.to_string())?;
            add_current(&mut credits, dross_destination, &retained)
                .map_err(|error| error.to_string())?;
        }
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "destructive_retirement".into(),
            instance_id: stack.arcane_id,
            units: clean.total(),
            dross: retained.total(),
            actor: "world".into(),
            note: reason.into(),
        });
        let transaction =
            transaction_from_maps(ledger, debits, credits, &instance.content_id, reason)?;
        let replacement = LinkedFileReplacement {
            subsystem: "implements".into(),
            operation_id,
            relative_path: crate::implements::IMPLEMENTS_FILE.into(),
            after: Some(next_state.encode().map_err(|error| error.to_string())?),
        };
        let mut replacements = vec![replacement];
        if let Some((_, bytes)) = &staged_material {
            replacements.push(LinkedFileReplacement {
                subsystem: "materials".into(),
                operation_id,
                relative_path: crate::materials::MaterialLedger::linked_delta_path().into(),
                after: Some(bytes.clone()),
            });
        }
        ledger
            .commit_linked_files(transaction, replacements)
            .map_err(|error| error.to_string())?;
        self.implements_state = Some(next_state);
        if let Some((next_material, _)) = staged_material {
            self.material_ledger = Some(next_material);
        }
        Ok(())
    }

    fn adjacent_vessel(&self, pos: BlockPos) -> Result<(BlockPos, ItemStack, u64), String> {
        horizontal_neighbors(pos)
            .into_iter()
            .find_map(|at| match self.block_entities.get(&at) {
                Some(BlockEntity::ChargeVessel(vessel)) => {
                    vessel.vessel.map(|stack| (at, stack, vessel.revision))
                }
                _ => None,
            })
            .ok_or_else(|| "Place a physical charge vessel directly beside the frame.".into())
    }
}

fn horizontal_neighbors(pos: BlockPos) -> Vec<BlockPos> {
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .filter_map(|(du, dv)| pos.offset(du, 0, dv))
        .collect()
}

fn all_neighbors(pos: BlockPos) -> Vec<BlockPos> {
    let mut out = horizontal_neighbors(pos);
    if let Some(up) = pos.offset(0, 1, 0) {
        out.push(up);
    }
    if let Some(down) = pos.offset(0, -1, 0) {
        out.push(down);
    }
    out
}

fn charm_resonance_preference(effect: crate::implements::CharmEffect) -> Vec<String> {
    match effect {
        crate::implements::CharmEffect::Quiet => [crate::arcane::ECHO, crate::arcane::GALE],
        crate::implements::CharmEffect::Bark => [crate::arcane::ROOT, crate::arcane::STONE],
        crate::implements::CharmEffect::Hunger => [crate::arcane::ROOT, crate::arcane::TIDE],
    }
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn implement_transfer_properties(kind: &ImplementKind) -> (u64, u64, u16) {
    match kind {
        ImplementKind::Wand { resolved, .. } => (
            resolved.capacity,
            resolved.safe_transfer,
            resolved.dross_per_thousand,
        ),
        ImplementKind::Charm {
            capacity,
            stability,
            dross_per_transfer,
            ..
        } => (
            *capacity,
            32,
            (*dross_per_transfer)
                .saturating_add((1_000u16.saturating_sub(*stability)) / 10)
                .clamp(1, 500),
        ),
        ImplementKind::Vessel {
            capacity,
            safe_transfer,
            containment,
        } => (
            *capacity,
            *safe_transfer,
            (1_000u16.saturating_sub(*containment) / 5).clamp(1, 500),
        ),
        ImplementKind::Fragments { .. } => (0, 0, 500),
    }
}

fn apparatus_neighbors(pos: BlockPos) -> Vec<BlockPos> {
    let mut out = horizontal_neighbors(pos);
    out.extend([1, -1].into_iter().filter_map(|dy| pos.offset(0, dy, 0)));
    out
}

pub(super) fn add_current(
    map: &mut BTreeMap<ArcaneOwner, Current>,
    owner: ArcaneOwner,
    current: &Current,
) -> Result<(), crate::arcane::ArcaneError> {
    map.entry(owner).or_default().checked_add(current)
}

fn physical_component(reg: &crate::registry::Registry, stack: ItemStack) -> ImplementComponent {
    ImplementComponent {
        content_id: reg.item(stack.item).name.clone(),
        materials: crate::materials::stack_materials(reg, ItemStack { count: 1, ..stack }),
    }
}

pub(super) fn transaction_from_maps(
    ledger: &mut crate::arcane::ArcaneLedger,
    debits: BTreeMap<ArcaneOwner, Current>,
    credits: BTreeMap<ArcaneOwner, Current>,
    content_id: &str,
    reason: &str,
) -> Result<ArcaneTransaction, String> {
    if debits.len().saturating_add(credits.len()) > crate::implements::MAX_IMPLEMENT_TRANSFER_OWNERS
    {
        return Err(format!(
            "implement transfer touches {} owner entries; local budget is {}",
            debits.len().saturating_add(credits.len()),
            crate::implements::MAX_IMPLEMENT_TRANSFER_OWNERS
        ));
    }
    let touched = debits
        .keys()
        .chain(credits.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let credit_content = credits
        .keys()
        .filter(|owner| matches!(owner, ArcaneOwner::Item(_) | ArcaneOwner::ItemDross(_)))
        .map(|owner| {
            let existing = ledger.account(owner);
            let fully_replaced = existing.is_some_and(|account| {
                debits
                    .get(owner)
                    .is_some_and(|removed| removed == &account.current)
            });
            let identity = if fully_replaced {
                content_id.to_string()
            } else {
                existing
                    .and_then(|account| account.content_id.clone())
                    .unwrap_or_else(|| content_id.to_string())
            };
            (owner.clone(), identity)
        })
        .collect::<BTreeMap<_, _>>();
    let reads = touched
        .into_iter()
        .map(|owner| AccountRead {
            expected_version: ledger.version_of(&owner),
            owner,
        })
        .collect();
    let debits = debits
        .into_iter()
        .filter(|(_, current)| !current.is_empty())
        .map(|(owner, current)| ArcaneMove {
            owner,
            current,
            content_id: None,
        })
        .collect();
    let credits = credits
        .into_iter()
        .filter(|(_, current)| !current.is_empty())
        .map(|(owner, current)| {
            let identity = credit_content.get(&owner).cloned();
            ArcaneMove {
                owner,
                current,
                content_id: identity,
            }
        })
        .collect();
    Ok(ArcaneTransaction {
        id: ledger
            .system_transaction_id()
            .map_err(|error| error.to_string())?,
        reads,
        debits,
        credits,
        transforms: Vec::new(),
        authority: ArcaneAuthority::System,
        reason: reason.into(),
        content_id: content_id.into(),
        linked: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apparatus_neighbor_walk_is_bounded_and_unique() {
        let pos = BlockPos::of_world(2, 80, 2).unwrap();
        let neighbors = apparatus_neighbors(pos);
        assert_eq!(neighbors.len(), 6);
        assert_eq!(neighbors.iter().copied().collect::<BTreeSet<_>>().len(), 6);
    }
}
