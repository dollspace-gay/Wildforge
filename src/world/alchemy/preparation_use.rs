//! Preparation use alchemy transaction coordination.

use super::incompatible_status_groups;
use super::preparation_color;
use super::produced;
use super::settle_immediate_current;
use crate::alchemy::AlchemyAuditEvent;
use crate::alchemy::AlchemyCue;
use crate::alchemy::AlchemyCueKind;
use crate::alchemy::AlchemyTarget;
use crate::alchemy::BatchOutcome;
use crate::alchemy::PreparationHandler;
use crate::alchemy::PreparationUseResult;
use crate::arcane::ArcaneAuthority;
use crate::arcane::ArcaneOwner;
use crate::arcane::Current;
use crate::inventory::Inventory;
use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::world::World;

impl World {
    /// Consume or apply one exact stable preparation container. The caller
    /// supplies only target intent; the host resolves the saved batch,
    /// handler, dose, water, Current, and exclusion rules.
    pub fn use_preparation(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        actor_pos: BlockPos,
        inventory: &mut Inventory,
        slot: usize,
        target: AlchemyTarget,
    ) -> Result<PreparationUseResult, String> {
        if actor == [0; 16] || actor_label.trim().is_empty() {
            return Err("Preparation use needs a stable actor identity.".into());
        }
        let mut state = self
            .alchemy_state
            .clone()
            .ok_or("The authoritative alchemy state is unavailable.")?;
        let mut next_inventory = inventory.clone();
        let stack = next_inventory
            .slots
            .get(slot)
            .copied()
            .flatten()
            .ok_or("That authoritative inventory slot is empty.")?;
        if stack.arcane_id == 0 || stack.count != 1 {
            return Err("That item is not one stable preparation container.".into());
        }
        let dose = state
            .containers
            .get(&stack.arcane_id)
            .cloned()
            .ok_or("That stable container has no authoritative preparation state.")?;
        if self.reg.item(stack.item).name != dose.item_name {
            return Err("The physical bottle identity disagrees with its saved dose.".into());
        }
        let definition = self
            .reg
            .preparations
            .get(&dose.preparation_id)
            .ok_or("The saved preparation definition is unavailable.")?
            .clone();
        if definition.version != dose.definition_version {
            return Err(
                "That dose belongs to an incompatible preparation definition version.".into(),
            );
        }
        let now = self.alchemy_tick();
        if dose.outcome == BatchOutcome::Spoiled || now >= dose.expires_tick {
            let spent_item = self
                .reg
                .item_id("base:spent_liquor")
                .ok_or("The named spent-liquor outcome is unavailable.")?;
            next_inventory.slots[slot] = Some(ItemStack {
                item: spent_item,
                ..stack
            });
            let operation_id = state
                .allocate_operation_id()
                .map_err(|error| error.to_string())?;
            let saved = state
                .containers
                .get_mut(&dose.container_id)
                .ok_or("The spoiled container disappeared during inspection.")?;
            saved.item_name = "base:spent_liquor".into();
            saved.outcome = BatchOutcome::Spoiled;
            state.record(AlchemyAuditEvent {
                operation_id,
                installation_id: dose.source_installation,
                batch_id: dose.source_batch,
                actor,
                action: "identify_spoilage".into(),
                preparation_id: definition.id.clone(),
                volume_units: dose.liquid.volume_units,
                current_units: dose.current_units,
                dross_units: dose.dross_units,
                tick: now,
                note: format!(
                    "{} identified as named spent liquor by {}",
                    definition.label,
                    actor_label.trim()
                ),
            });
            self.persist_alchemy_state(state)?;
            *inventory = next_inventory;
            return Ok(PreparationUseResult {
                preparation_id: definition.id,
                source_batch: dose.source_batch,
                status_id: None,
                returned_vessel: None,
                byproduct: None,
                cue: AlchemyCue {
                    pos: actor_pos,
                    installation_id: dose.source_installation,
                    batch_id: dose.source_batch,
                    revision: operation_id,
                    kind: AlchemyCueKind::Spoil,
                    intensity: 180,
                    color: [150, 80, 170],
                    message: "The aged preparation is now visibly named spent liquor; its liquid and charge remain in the bottle for recovery or disposal.".into(),
                },
                message: "Spoiled dose identified as spent liquor; no effect was applied and nothing was discarded.".into(),
            });
        }
        if dose.outcome != BatchOutcome::Ready {
            return Err(
                "That named failed preparation cannot masquerade as a ready effect.".into(),
            );
        }
        let empty_vessel_item = self
            .reg
            .item_id(&definition.empty_vessel)
            .ok_or("The saved preparation's reusable vessel is unavailable.")?;
        let expected_vessel_materials = crate::materials::stack_materials(
            &self.reg,
            ItemStack::new(&self.reg, empty_vessel_item, 1),
        );
        if expected_vessel_materials != dose.vessel_materials {
            return Err(
                "The preparation's saved reusable-vessel matter disagrees with current content."
                    .into(),
            );
        }
        match (definition.application, target) {
            (crate::alchemy::ApplicationKind::Drink, AlchemyTarget::SelfActor)
            | (crate::alchemy::ApplicationKind::Plot, AlchemyTarget::Plot(_))
            | (crate::alchemy::ApplicationKind::Wash, AlchemyTarget::Surface(_))
            | (crate::alchemy::ApplicationKind::Wash, AlchemyTarget::Item(_))
            | (crate::alchemy::ApplicationKind::Coat, AlchemyTarget::Item(_)) => {}
            _ => return Err("That preparation cannot be applied to this target kind.".into()),
        }
        let _physical = next_inventory
            .take_one_stack(slot)
            .ok_or("The preparation moved before it could be reserved.")?;
        let empty = produced(&self.reg, &definition.empty_vessel, 1, 0)?;
        if next_inventory.add_stack(
            &self.reg,
            empty
                .clone()
                .into_stack(&self.reg)
                .map_err(|error| error.to_string())?,
        ) != 0
        {
            return Err("Make room for the reusable empty vessel before applying the dose.".into());
        }
        let clean_owner = ArcaneOwner::Item(dose.container_id);
        let dose_dross_owner = ArcaneOwner::ItemDross(dose.container_id);
        let clean = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&clean_owner))
            .map_or_else(Current::default, |account| account.current.clone());
        let dose_dross = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&dose_dross_owner))
            .map_or_else(Current::default, |account| account.current.clone());
        if clean.total() != dose.current_units || dose_dross.total() != dose.dross_units {
            return Err("The preparation sidecar and finite Current ledger disagree.".into());
        }
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(actor_pos.surface()));
        let mut status_id = None;
        let mut byproduct = None;
        let water_return: (BlockPos, bool);
        let mut root_update = None::<(BlockPos, u8, bool)>;
        let mut debits = Vec::new();
        let mut credits = Vec::new();
        let mut dense_dross_rollback = None::<(
            crate::planet_atlas::AtlasPos,
            u64,
            crate::dross::DenseDrossCaptureBefore,
        )>;
        let mut dense_dross_manifest = None;
        let mut dense_dross_replacements = Vec::new();
        if !clean.is_empty() {
            debits.push((clean_owner, clean.clone()));
        }
        if !dose_dross.is_empty() {
            debits.push((dose_dross_owner, dose_dross.clone()));
        }

        match definition.handler {
            PreparationHandler::TraceSight
            | PreparationHandler::NaturalRecovery
            | PreparationHandler::StrainRelief
            | PreparationHandler::ThroughputSurge
            | PreparationHandler::DrossAntidote => {
                if target != AlchemyTarget::SelfActor {
                    return Err(
                        "Drinkable preparations only target their authoritative user.".into(),
                    );
                }
                let incompatible = state
                    .statuses
                    .get(&actor)
                    .into_iter()
                    .flatten()
                    .any(|status| {
                        status.due_tick > now
                            && incompatible_status_groups(
                                &status.stack_group,
                                &definition.stack_group,
                            )
                    });
                if incompatible {
                    return Err("Those active preparations have a documented incompatible bodily interaction.".into());
                }
                let existing_index = state.statuses.get(&actor).and_then(|statuses| {
                    statuses
                        .iter()
                        .position(|status| status.stack_group == definition.stack_group)
                });
                let (id, refreshed) = if let Some(index) = existing_index {
                    let statuses = state
                        .statuses
                        .get_mut(&actor)
                        .expect("index came from status list");
                    let status = &mut statuses[index];
                    if status.due_tick <= now && now < status.recovery_until_tick {
                        return Err(
                            "That preparation is still inside its authoritative recovery interval."
                                .into(),
                        );
                    }
                    if status.preparation_id != definition.id || status.refresh_count >= 3 {
                        return Err(
                            "This effect cannot stack or refresh beyond its declared bounded rule."
                                .into(),
                        );
                    }
                    let cap = status
                        .started_tick
                        .saturating_add(definition.effect.duration_ticks.saturating_mul(2));
                    status.due_tick = status
                        .due_tick
                        .max(now.saturating_add(definition.effect.duration_ticks / 2))
                        .min(cap);
                    status.recovery_until_tick = status
                        .due_tick
                        .saturating_add(definition.effect.recovery_ticks);
                    status.refresh_count = status.refresh_count.saturating_add(1);
                    if definition.handler == PreparationHandler::NaturalRecovery {
                        status.overdose_until_tick =
                            now.saturating_add(600).min(status.recovery_until_tick);
                    }
                    status
                        .active_current
                        .checked_add(&clean)
                        .map_err(|error| error.to_string())?;
                    status
                        .dross_current
                        .checked_add(&dose_dross)
                        .map_err(|error| error.to_string())?;
                    (status.status_id, true)
                } else {
                    let id = state
                        .allocate_status_id()
                        .map_err(|error| error.to_string())?;
                    state.statuses.entry(actor).or_default().push(
                        crate::alchemy::ActivePreparationStatus {
                            status_id: id,
                            preparation_id: definition.id.clone(),
                            definition_version: definition.version,
                            source_batch: dose.source_batch,
                            actor,
                            dose_volume_units: dose.liquid.volume_units,
                            active_current: clean.clone(),
                            dross_current: dose_dross.clone(),
                            started_tick: now,
                            last_tick: now,
                            due_tick: now.saturating_add(definition.effect.duration_ticks),
                            recovery_until_tick: now
                                .saturating_add(definition.effect.duration_ticks)
                                .saturating_add(definition.effect.recovery_ticks),
                            stack_group: definition.stack_group.clone(),
                            completed_units: 0,
                            refresh_count: 0,
                            overdose_until_tick: 0,
                        },
                    );
                    (id, false)
                };
                status_id = Some(id);
                let owner_id = crate::alchemy::status_owner_id(id);
                if !clean.is_empty() {
                    credits.push((
                        ArcaneOwner::Alchemy(owner_id),
                        clean.clone(),
                        Some(definition.id.clone()),
                    ));
                }
                if !dose_dross.is_empty() {
                    credits.push((
                        ArcaneOwner::AlchemyDross(owner_id),
                        dose_dross.clone(),
                        Some(definition.id.clone()),
                    ));
                }
                if refreshed {
                    // The same stable active owner receives the second dose;
                    // no parallel stack is created.
                }
                self.preflight_preparation_water_return(actor_pos, &dose, false)?;
                water_return = (actor_pos, false);
            }
            PreparationHandler::RootUptake => {
                let AlchemyTarget::Plot(plot) = target else {
                    return Err("Root wash needs one exact plot or rooting bed.".into());
                };
                if self.reg.block(self.get_block_at(plot)).fert_tiles.is_none() {
                    return Err("Root wash cannot be applied to a non-soil block.".into());
                }
                self.preflight_preparation_water_return(plot, &dose, false)?;
                water_return = (plot, false);
                let prior_concentration = state
                    .root_treatments
                    .get(&plot)
                    .filter(|treatment| treatment.expires_tick > now)
                    .map_or(0, |treatment| treatment.concentration_permille);
                let concentration = prior_concentration.saturating_add(1_000);
                state.root_treatments.insert(
                    plot,
                    crate::alchemy::RootTreatment {
                        source_batch: dose.source_batch,
                        actor,
                        applied_tick: now,
                        expires_tick: now.saturating_add(definition.effect.duration_ticks),
                        water_hu: u64::from(definition.effect.water_hu),
                        nutrient_units: u64::from(definition.effect.nutrient_cost),
                        uptake_permille: definition.effect.strength.min(1_000) as u16,
                        concentration_permille: concentration.min(2_000),
                    },
                );
                root_update = Some((
                    plot,
                    u8::try_from(definition.effect.nutrient_cost.min(8)).unwrap_or(8),
                    concentration > 1_000,
                ));
                settle_immediate_current(
                    &mut credits,
                    region,
                    clean.clone(),
                    dose_dross.clone(),
                    crate::arcane::DrossMedium::Soil,
                )?;
            }
            PreparationHandler::PreserveSpecimen => {
                let AlchemyTarget::Item(item_id) = target else {
                    return Err("Frostlace suspension coats one stable botanical specimen.".into());
                };
                let specimen = next_inventory
                    .slots
                    .iter()
                    .flatten()
                    .find(|stack| stack.arcane_id == item_id && stack.count == 1)
                    .copied();
                if item_id == 0 || specimen.is_none() {
                    return Err(
                        "The target specimen is not in authoritative carried custody.".into(),
                    );
                }
                if self
                    .reg
                    .item(specimen.expect("checked specimen").item)
                    .arcane_ecology
                    .is_none()
                {
                    return Err(
                        "Frostlace coats one charged botanical specimen, not equipment or bulk food."
                            .into(),
                    );
                }
                if state.coatings.contains_key(&item_id) {
                    return Err(
                        "That specimen already carries a finite Frostlace coating; let it settle before recoating."
                            .into(),
                    );
                }
                let id = state
                    .allocate_status_id()
                    .map_err(|error| error.to_string())?;
                status_id = Some(id);
                state.coatings.insert(
                    item_id,
                    crate::alchemy::SpecimenCoating {
                        status_id: id,
                        item_id,
                        source_batch: dose.source_batch,
                        actor,
                        applied_pos: actor_pos,
                        applied_tick: now,
                        expires_tick: now.saturating_add(definition.effect.duration_ticks),
                        preservation_permille: definition.effect.preservation_permille,
                        maximum_temperature_millic: definition.storage_temperature_millic[1],
                        age_paid: 0,
                    },
                );
                let owner_id = crate::alchemy::status_owner_id(id);
                if !clean.is_empty() {
                    credits.push((
                        ArcaneOwner::Alchemy(owner_id),
                        clean.clone(),
                        Some(definition.id.clone()),
                    ));
                }
                if !dose_dross.is_empty() {
                    credits.push((
                        ArcaneOwner::AlchemyDross(owner_id),
                        dose_dross.clone(),
                        Some(definition.id.clone()),
                    ));
                }
                self.preflight_preparation_water_return(actor_pos, &dose, false)?;
                water_return = (actor_pos, false);
                let spent = produced(&self.reg, "base:spent_carrier", 1, 0)?;
                if next_inventory.add_stack(
                    &self.reg,
                    spent
                        .clone()
                        .into_stack(&self.reg)
                        .map_err(|error| error.to_string())?,
                ) != 0
                {
                    return Err(
                        "Make room for the Frostlace suspension's spent carrier before coating."
                            .into(),
                    );
                }
                byproduct = Some(spent);
            }
            PreparationHandler::DrossWash => {
                let (target_owner, target_pos, dense_region) = match target {
                    AlchemyTarget::Surface(surface) => {
                        let atlas = self
                            .planet_atlas
                            .as_ref()
                            .ok_or("Surface washing needs the authoritative atlas.")?;
                        let region = atlas.atlas_pos(surface.surface());
                        (
                            ArcaneOwner::Dross {
                                region,
                                medium: crate::arcane::DrossMedium::Soil,
                            },
                            surface,
                            Some(region),
                        )
                    }
                    AlchemyTarget::Item(item_id)
                        if item_id != 0
                            && next_inventory
                                .slots
                                .iter()
                                .flatten()
                                .any(|stack| stack.arcane_id == item_id && stack.count == 1) =>
                    {
                        (ArcaneOwner::ItemDross(item_id), actor_pos, None)
                    }
                    AlchemyTarget::Item(_) => {
                        return Err(
                            "The tool or vessel to wash is not in authoritative carried custody."
                                .into(),
                        );
                    }
                    _ => {
                        return Err("Ashlace wash needs one small surface, tool, or vessel.".into());
                    }
                };
                let available = self
                    .arcane_ledger
                    .as_ref()
                    .and_then(|ledger| ledger.account(&target_owner))
                    .map_or_else(Current::default, |account| account.current.clone());
                let mut available_work = available;
                let capacity = u64::from(definition.effect.dross_capacity);
                let sparse_captured = available_work
                    .take_units(
                        capacity.min(available_work.total()),
                        [definition.resonance.clone()],
                    )
                    .map_err(|error| error.to_string())?;
                let dense_requested = capacity.saturating_sub(sparse_captured.total());
                let dense_available = dense_region
                    .and_then(|region| {
                        self.arcane_geography.as_ref().map(|geography| {
                            geography
                                .dense_dross_current_at(region, crate::dross::DrossCarrier::Soil)
                        })
                    })
                    .unwrap_or_default()
                    .total()
                    .min(dense_requested);
                if sparse_captured.is_empty() && dense_available == 0 && dose_dross.is_empty() {
                    return Err(
                        "The wash finds no mobile dross to bind; no empty sludge identity is created."
                            .into(),
                    );
                }
                let sludge_id = self
                    .arcane_ledger
                    .as_mut()
                    .ok_or("The finite Current ledger is unavailable.")?
                    .allocate_item_id()
                    .map_err(|error| error.to_string())?;
                self.preflight_preparation_water_return(target_pos, &dose, true)?;
                water_return = (target_pos, true);
                let sludge = produced(&self.reg, "base:dross_sludge", 1, sludge_id)?;
                if next_inventory.add_stack(
                    &self.reg,
                    sludge
                        .clone()
                        .into_stack(&self.reg)
                        .map_err(|error| error.to_string())?,
                ) != 0
                {
                    return Err("Make room for the recoverable dross sludge before washing.".into());
                }
                let dense_captured =
                    if let Some(region) = dense_region.filter(|_| dense_available != 0) {
                        let before = self
                            .arcane_geography
                            .as_ref()
                            .ok_or("The finite magical geography is unavailable.")?
                            .snapshot_dense_dross_capture(region, sludge_id);
                        let (captured, provenance) = self
                            .arcane_geography
                            .as_mut()
                            .expect("dense dross geography was checked")
                            .export_environmental_dross(
                                region,
                                crate::dross::DrossCarrier::Soil,
                                dense_available,
                            )?;
                        if let Err(error) = self
                            .arcane_geography
                            .as_mut()
                            .expect("dense dross geography was checked")
                            .record_contained_dross_provenance(sludge_id, provenance)
                        {
                            self.arcane_geography
                                .as_mut()
                                .expect("dense dross geography was checked")
                                .restore_dense_dross_capture(region, sludge_id, before);
                            return Err(error);
                        }
                        dense_dross_rollback = Some((region, sludge_id, before));
                        captured
                    } else {
                        Current::default()
                    };
                let mut captured = sparse_captured.clone();
                captured
                    .checked_add(&dense_captured)
                    .map_err(|error| error.to_string())?;
                let mut sludge_current = dose_dross.clone();
                sludge_current
                    .checked_add(&captured)
                    .map_err(|error| error.to_string())?;
                if !sparse_captured.is_empty() {
                    debits.push((target_owner, sparse_captured));
                }
                if !dense_captured.is_empty() {
                    debits.push((ArcaneOwner::Geography, dense_captured));
                }
                if !sludge_current.is_empty() {
                    credits.push((
                        ArcaneOwner::ItemDross(sludge_id),
                        sludge_current,
                        Some("base:dross_sludge".into()),
                    ));
                }
                if !clean.is_empty() {
                    let region = region.ok_or("Wash settlement needs the planet atlas.")?;
                    credits.push((ArcaneOwner::Ambient(region), clean.clone(), None));
                }
                if dense_dross_rollback.is_some() {
                    match self
                        .arcane_geography
                        .as_ref()
                        .expect("dense dross geography was changed")
                        .linked_dross_replacements(&self.save_dir, operation_id)
                    {
                        Ok((manifest, replacements)) => {
                            dense_dross_manifest = Some(manifest);
                            dense_dross_replacements = replacements;
                        }
                        Err(error) => {
                            let (region, item_id, before) =
                                dense_dross_rollback.take().expect("rollback was present");
                            self.arcane_geography
                                .as_mut()
                                .expect("dense dross geography was changed")
                                .restore_dense_dross_capture(region, item_id, before);
                            return Err(error.to_string());
                        }
                    }
                }
                byproduct = Some(sludge);
            }
        }
        // Retained ingredient matter leaves the circulating preparation at
        // application time: metabolism, soil uptake, a coating film, or
        // captured wash waste is an explicit material sink rather than an
        // untracked disappearance. Stage that debit beside the state/Current
        // replacement so an I/O refusal cannot consume only half the dose.
        // Reusable-vessel matter is not debited; it returns in `empty` below.
        let staged_material_result = if dose.materials.is_empty() {
            Ok(None)
        } else {
            match self.material_ledger.as_ref() {
                Some(ledger) => ledger
                    .stage_linked_consumption(&dose.materials)
                    .map_err(|error| error.to_string()),
                None => Err("The finite material ledger is unavailable.".into()),
            }
        };
        let staged_material = match staged_material_result {
            Ok(staged) => staged,
            Err(error) => {
                if let Some((region, item_id, before)) = dense_dross_rollback.take() {
                    self.arcane_geography
                        .as_mut()
                        .expect("dense dross geography was changed")
                        .restore_dense_dross_capture(region, item_id, before);
                }
                return Err(error);
            }
        };
        state.containers.remove(&dose.container_id);
        let event_installation = dose.source_installation;
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id: event_installation,
            batch_id: dose.source_batch,
            actor,
            action: format!("apply_{:?}", definition.application).to_lowercase(),
            preparation_id: definition.id.clone(),
            volume_units: dose.liquid.volume_units,
            current_units: clean.total(),
            dross_units: dose_dross.total(),
            tick: now,
            note: format!("{} by {}", definition.label, actor_label.trim()),
        });
        let committed = self.commit_alchemy_current_with_material_and_links(
            state.clone(),
            operation_id,
            &definition.id,
            "authoritative preparation application transferred exact dose Current and matter",
            debits,
            credits,
            staged_material,
            ArcaneAuthority::SystemForPlayer(actor),
            dense_dross_replacements,
        );
        if let Err(error) = committed {
            if let Some((region, item_id, before)) = dense_dross_rollback.take() {
                self.arcane_geography
                    .as_mut()
                    .expect("dense dross geography was changed")
                    .restore_dense_dross_capture(region, item_id, before);
            }
            return Err(error);
        }
        if let Some(manifest) = dense_dross_manifest {
            self.arcane_geography
                .as_mut()
                .expect("dense dross geography survived wash")
                .accept_linked_manifest(manifest);
        }
        let (return_pos, runoff) = water_return;
        self.return_preparation_water(return_pos, &dose, runoff)?;
        if let Some((plot, nutrient, salted)) = root_update {
            let _ = self.feed_soil_at(plot, nutrient);
            if salted {
                self.set_soil_salinity_at(plot, self.get_soil_salinity_at(plot).saturating_add(24));
            }
        }
        *inventory = next_inventory;
        Ok(PreparationUseResult {
            preparation_id: definition.id,
            source_batch: dose.source_batch,
            status_id,
            returned_vessel: Some(empty),
            byproduct,
            cue: AlchemyCue {
                pos: actor_pos,
                installation_id: event_installation,
                batch_id: dose.source_batch,
                revision: operation_id,
                kind: if target == AlchemyTarget::SelfActor {
                    AlchemyCueKind::Drink
                } else {
                    AlchemyCueKind::Apply
                },
                intensity: 128,
                color: preparation_color(definition.handler),
                message: format!("{} takes effect through its accounted dose.", definition.label),
            },
            message: "The reusable vessel is empty; the dose now exists in its target/status and declared waste path.".into(),
        })
    }
}
