//! Host-authoritative embodied preparation operations.

use std::collections::BTreeMap;

use super::*;
use crate::alchemy::{
    AgitationKind, AlchemyApparatusState, AlchemyAuditEvent, AlchemyBatch, AlchemyCue,
    AlchemyCueKind, AlchemyError, AlchemyRequest, AlchemyResult, AlchemyTarget, ApparatusAction,
    ApparatusKind, BatchFailure, BatchIngredientState, BatchOutcome, CarrierKind, DisposalRoute,
    ExactLiquid, PreparationHandler, PreparationModifiers, PreparationPhysiology,
    PreparationTickResult, PreparationUseResult, ProcessObservation, ProcessStep, ProducedStack,
};
use crate::arcane::{
    AccountRead, ArcaneAuthority, ArcaneMove, ArcaneOwner, ArcaneTransaction, Current,
    LinkedFileReplacement,
};
use crate::inventory::{Inventory, ItemStack};
use crate::planet_atlas::{HYDRO_UNITS_PER_BLOCK, ReservoirMass, WaterClass};
use crate::registry::MaterialVector;
use crate::workings::WaterCarrier;

const CARRIER_ITEM_UNITS: u64 = 64;
const APPARATUS_REACH: i32 = 4;

impl World {
    pub fn alchemy_state(&self) -> Option<&crate::alchemy::AlchemyState> {
        self.alchemy_state.as_ref()
    }

    /// Owner-visible state for one filled preparation card. Without a tuning
    /// lens the Current/dross reading remains qualitative, matching apparatus
    /// sampling rather than leaking exact hidden ledger values.
    pub fn preparation_tooltip(&self, stack: ItemStack, has_lens: bool) -> Vec<String> {
        let Some(dose) = self
            .alchemy_state
            .as_ref()
            .and_then(|state| state.containers.get(&stack.arcane_id))
            .filter(|dose| dose.item_name == self.reg.item(stack.item).name)
        else {
            return Vec::new();
        };
        let now = self.alchemy_tick();
        let condition = if now >= dose.expires_tick || dose.outcome == BatchOutcome::Spoiled {
            "SPOILED — SPENT LIQUOR / DISPOSAL ONLY".to_string()
        } else {
            match dose.outcome {
                BatchOutcome::Ready => "BATCH CONDITION: READY".into(),
                BatchOutcome::Failed(failure) => format!("FAILED BATCH: {failure:?}"),
                BatchOutcome::Processing => "INVALID UNFINISHED CONTAINER".into(),
                BatchOutcome::Spoiled => "SPOILED — SPENT LIQUOR / DISPOSAL ONLY".into(),
            }
        };
        let remaining_ticks = dose.expires_tick.saturating_sub(now);
        let remaining_days = remaining_ticks as f32 / 20.0 / crate::server::DAY_LENGTH.max(1.0);
        let mut lines = vec![
            condition,
            format!("ONE EXACT {}-UNIT DOSE", dose.liquid.volume_units),
            if remaining_days < 1.0 {
                "LIFE: UNDER ONE DAY".into()
            } else {
                format!("LIFE: ABOUT {} DAYS", remaining_days.floor() as u64)
            },
        ];
        let total = dose.current_units.saturating_add(dose.dross_units);
        if has_lens {
            lines.push(format!(
                "LENS: {} CURRENT / {} DROSS",
                crate::arcane::qualitative_current(total, crate::alchemy::MAX_PREPARATION_CHARGE)
                    .to_uppercase(),
                if dose.dross_units == 0 {
                    "CLEAR"
                } else if dose.dross_units.saturating_mul(4) <= total.max(1) {
                    "TRACE"
                } else {
                    "TURBID"
                }
            ));
        } else {
            lines.push("A TUNING LENS READS CHARGE + DROSS CONDITION".into());
        }
        lines
    }

    /// Apply one non-duplicable carried-storage assessment. Absolute world
    /// time supplies ordinary aging; Holdfast may reduce `ordinary_age_ticks`
    /// and therefore extends only the interval it actually protected, while
    /// temperatures outside the declared band accelerate the remainder.
    pub fn age_preparation_storage(
        &mut self,
        stack: ItemStack,
        temperature_millic: i32,
        ordinary_age_ticks: u64,
    ) -> Result<Option<bool>, String> {
        if stack.arcane_id == 0 {
            return Ok(None);
        }
        let now = self.alchemy_tick();
        let (preparation_id, item_name) = match self
            .alchemy_state
            .as_ref()
            .and_then(|state| state.containers.get(&stack.arcane_id))
        {
            Some(dose) => (dose.preparation_id.clone(), dose.item_name.clone()),
            None => return Ok(None),
        };
        if stack.count != 1 || self.reg.item(stack.item).name != item_name {
            return Err("A carried preparation disagrees with its stable storage state.".into());
        }
        let storage_band = self
            .reg
            .preparations
            .get(&preparation_id)
            .ok_or("The stored preparation definition is unavailable.")?
            .storage_temperature_millic;
        let dose = self
            .alchemy_state
            .as_mut()
            .and_then(|state| state.containers.get_mut(&stack.arcane_id))
            .ok_or("The preparation moved during its storage assessment.")?;
        let last = dose.last_storage_tick.max(dose.born_tick).min(now);
        let actual_elapsed = now.saturating_sub(last);
        dose.last_storage_tick = now;
        if actual_elapsed == 0 {
            return Ok(Some(false));
        }
        let effective_elapsed = ordinary_age_ticks.min(actual_elapsed);
        let protected = actual_elapsed - effective_elapsed;
        dose.expires_tick = dose.expires_tick.saturating_add(protected);
        let outside_by = if temperature_millic < storage_band[0] {
            storage_band[0].saturating_sub(temperature_millic)
        } else if temperature_millic > storage_band[1] {
            temperature_millic.saturating_sub(storage_band[1])
        } else {
            0
        };
        let extra_multiplier = u64::try_from(outside_by)
            .unwrap_or(u64::MAX)
            .div_ceil(10_000)
            .min(3);
        let extra_age = effective_elapsed.saturating_mul(extra_multiplier);
        dose.expires_tick = dose
            .expires_tick
            .saturating_sub(extra_age)
            .max(dose.born_tick.saturating_add(1));
        let newly_spoiled = now >= dose.expires_tick && dose.outcome != BatchOutcome::Spoiled;
        if newly_spoiled {
            dose.outcome = BatchOutcome::Spoiled;
        }
        Ok(Some(newly_spoiled))
    }

    /// Settle a filled preparation that is physically broken, burned, or
    /// otherwise lost. The liquid enters local runoff, ingredient matter and
    /// glass follow explicit material paths, and both clean charge and dross
    /// become attributed environmental dross. Returning `false` means the
    /// stable item was not an alchemy container and the generic destruction
    /// path should continue.
    pub(crate) fn destroy_preparation_container_at(
        &mut self,
        at: BlockPos,
        stack: ItemStack,
        reason: &str,
    ) -> Result<bool, String> {
        if stack.arcane_id == 0 {
            return Ok(false);
        }
        let mut state = self
            .alchemy_state
            .clone()
            .ok_or("The authoritative alchemy state is unavailable.")?;
        let Some(dose) = state.containers.get(&stack.arcane_id).cloned() else {
            return Ok(false);
        };
        if stack.count != 1 || self.reg.item(stack.item).name != dose.item_name {
            return Err(
                "A destroyed preparation's physical item disagrees with its stable sidecar.".into(),
            );
        }
        let atlas = self
            .planet_atlas
            .as_ref()
            .ok_or("Preparation breakage needs the authoritative planet atlas.")?;
        let region = atlas.atlas_pos(at.surface());
        let clean_owner = ArcaneOwner::Item(dose.container_id);
        let dross_owner = ArcaneOwner::ItemDross(dose.container_id);
        let clean = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&clean_owner))
            .map_or_else(Current::default, |account| account.current.clone());
        let dross = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&dross_owner))
            .map_or_else(Current::default, |account| account.current.clone());
        if clean.total() != dose.current_units || dross.total() != dose.dross_units {
            return Err("Destroyed preparation state and Current custody disagree.".into());
        }
        let mut environmental = clean.clone();
        environmental
            .checked_add(&dross)
            .map_err(|error| error.to_string())?;

        self.preflight_industrial_water_return(at, dose.liquid.water, true)?;

        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        state.containers.remove(&dose.container_id);
        let pollution = state.pollution.entry(at).or_default();
        pollution.water = pollution
            .water
            .checked_add(dose.liquid.water)
            .ok_or("Broken preparation water custody overflowed.")?;
        if let Some(carrier) = dose.liquid.carrier {
            let units = pollution.carrier_units.entry(carrier).or_default();
            *units = units
                .checked_add(dose.liquid.volume_units)
                .ok_or("Broken preparation carrier custody overflowed.")?;
        }
        add_materials(&mut pollution.materials, &dose.materials)?;
        add_materials(&mut pollution.solutes, &dose.liquid.solutes)?;
        pollution
            .dross
            .checked_add(&environmental)
            .map_err(|error| error.to_string())?;
        pollution.last_actor = [255; 16];
        pollution.last_operation_id = operation_id;
        pollution.updated_tick = self.alchemy_tick();
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id: dose.source_installation,
            batch_id: dose.source_batch,
            actor: [255; 16],
            action: "break_container".into(),
            preparation_id: dose.preparation_id.clone(),
            volume_units: dose.liquid.volume_units,
            current_units: clean.total(),
            dross_units: dross.total(),
            tick: self.alchemy_tick(),
            note: format!("stable container {} destroyed: {reason}", dose.container_id),
        });
        let mut debits = Vec::new();
        if !clean.is_empty() {
            debits.push((clean_owner, clean));
        }
        if !dross.is_empty() {
            debits.push((dross_owner, dross));
        }
        self.commit_alchemy_current(
            state,
            operation_id,
            &dose.preparation_id,
            "broken preparation transferred all contents into local pollution",
            debits,
            if environmental.is_empty() {
                Vec::new()
            } else {
                vec![(
                    ArcaneOwner::Dross {
                        region,
                        medium: crate::arcane::DrossMedium::Water,
                    },
                    environmental,
                    None,
                )]
            },
        )?;
        self.apply_industrial_water_return(at, dose.liquid.water, true)?;
        if let Some(ledger) = &mut self.material_ledger {
            ledger
                .record_consumption(&dose.materials)
                .map_err(|error| error.to_string())?;
            ledger
                .bury_materials(at, &dose.vessel_materials, "broken reusable alchemy vessel")
                .map_err(|error| error.to_string())?;
        }
        Ok(true)
    }

    /// The one native dispatch used by local play, hosted guests, automation,
    /// and agents. Clients supply intent, a revision precondition, and
    /// inventory slot choices; the host reconstructs every item, volume,
    /// process, time, and Current consequence.
    pub fn operate_alchemy(
        &mut self,
        pos: BlockPos,
        inventory: &mut Inventory,
        request: AlchemyRequest,
    ) -> Result<AlchemyResult, String> {
        if request.actor == [0; 16]
            || request.actor_label.trim().is_empty()
            || request.actor_label.len() > crate::alchemy::MAX_PREPARATION_ID_BYTES
        {
            return Err("Alchemy needs a stable, bounded operator identity.".into());
        }
        let kind = self.alchemy_kind_at(pos)?;
        // Inspection is the hot UI polling path. Once an apparatus has a
        // stable installation record it is read-only, so answer directly from
        // authoritative state instead of cloning a settlement-wide census.
        if matches!(&request.action, ApparatusAction::Inspect) {
            let state = self
                .alchemy_state
                .as_ref()
                .ok_or("The authoritative alchemy state is unavailable.")?;
            if let Some(apparatus) = state.apparatus.get(&pos) {
                if apparatus.kind != kind {
                    return Err("Saved apparatus kind disagrees with the world block.".into());
                }
                if request
                    .expected_revision
                    .is_some_and(|revision| revision != apparatus.revision)
                {
                    return Err(format!(
                        "The apparatus changed (expected revision {}, found {}); inspect it before retrying.",
                        request.expected_revision.unwrap_or_default(),
                        apparatus.revision
                    ));
                }
                return result_for(
                    &self.reg,
                    state,
                    pos,
                    AlchemyCueKind::Bubble,
                    "The apparatus is inspected.",
                    None,
                );
            }
        }
        let mut next_state = self
            .alchemy_state
            .clone()
            .ok_or("The authoritative alchemy state is unavailable.")?;
        let mut next_inventory = inventory.clone();
        ensure_apparatus(&mut next_state, pos, kind, request.actor)
            .map_err(|error| error.to_string())?;
        let actual_revision = next_state
            .apparatus
            .get(&pos)
            .map(|apparatus| apparatus.revision)
            .unwrap_or_default();
        if request
            .expected_revision
            .is_some_and(|revision| revision != actual_revision)
        {
            return Err(format!(
                "The apparatus changed (expected revision {}, found {actual_revision}); inspect it before retrying.",
                request.expected_revision.unwrap_or_default()
            ));
        }

        let result = match request.action.clone() {
            ApparatusAction::Inspect => result_for(
                &self.reg,
                &next_state,
                pos,
                AlchemyCueKind::Bubble,
                "The apparatus is inspected.",
                None,
            ),
            ApparatusAction::Begin { preparation_id } => {
                self.alchemy_begin(pos, &request, preparation_id, &mut next_state)
            }
            ApparatusAction::Grind { inventory_slot } => self.alchemy_grind(
                pos,
                &request,
                usize::from(inventory_slot),
                &mut next_state,
                &mut next_inventory,
            ),
            ApparatusAction::TransferMash { destination } => {
                self.alchemy_transfer_mash(pos, destination, &request, &mut next_state)
            }
            ApparatusAction::LoadCarrier { inventory_slot } => self.alchemy_load_carrier(
                pos,
                &request,
                usize::from(inventory_slot),
                &mut next_state,
                &mut next_inventory,
            ),
            ApparatusAction::LoadFilter { inventory_slot } => self.alchemy_load_filter(
                pos,
                &request,
                usize::from(inventory_slot),
                &mut next_state,
                &mut next_inventory,
            ),
            ApparatusAction::SetHeat { temperature_millic } => {
                self.alchemy_set_heat(pos, &request, temperature_millic, &mut next_state)
            }
            ApparatusAction::SetAgitation { agitation } => {
                self.alchemy_set_agitation(pos, &request, agitation, &mut next_state)
            }
            ApparatusAction::Advance { step } => {
                self.alchemy_advance(pos, &request, step, &mut next_state)
            }
            ApparatusAction::Charge {
                inventory_slot,
                units,
            } => self.alchemy_charge(
                pos,
                &request,
                inventory_slot.map(usize::from),
                units,
                &mut next_state,
                &next_inventory,
            ),
            ApparatusAction::Sample => self.alchemy_sample(pos, &request, &mut next_state),
            ApparatusAction::Decant { vessel_slot } => self.alchemy_decant(
                pos,
                &request,
                usize::from(vessel_slot),
                &mut next_state,
                &mut next_inventory,
            ),
            ApparatusAction::Clean {
                water_slot,
                filter_slot,
            } => self.alchemy_clean(
                pos,
                &request,
                usize::from(water_slot),
                filter_slot.map(usize::from),
                &mut next_state,
                &mut next_inventory,
            ),
            ApparatusAction::Repair { material_slot } => self.alchemy_repair(
                pos,
                &request,
                usize::from(material_slot),
                &mut next_state,
                &mut next_inventory,
            ),
            ApparatusAction::Drain { route } => {
                self.alchemy_drain(pos, &request, route, &mut next_state)
            }
            ApparatusAction::Dismantle { route } => {
                self.alchemy_dismantle(pos, &request, route, &mut next_state)
            }
            ApparatusAction::FermentAlcohol {
                water_slot,
                wheat_slot,
                berry_slot,
            } => self.alchemy_ferment(
                pos,
                &request,
                [
                    usize::from(water_slot),
                    usize::from(wheat_slot),
                    usize::from(berry_slot),
                ],
                &mut next_state,
                &mut next_inventory,
            ),
            ApparatusAction::PressOil { seed_slot } => self.alchemy_press_oil(
                pos,
                &request,
                usize::from(seed_slot),
                &mut next_state,
                &mut next_inventory,
            ),
        }?;
        self.persist_alchemy_state(next_state)?;
        *inventory = next_inventory;
        Ok(result)
    }

    fn alchemy_tick(&self) -> u64 {
        (self.clock.max(0.0) * 20.0).round() as u64
    }

    fn alchemy_kind_at(&self, pos: BlockPos) -> Result<ApparatusKind, String> {
        match self
            .reg
            .block(self.get_block_at(pos))
            .interaction
            .as_deref()
        {
            Some("alchemy_mortar") => Ok(ApparatusKind::Mortar),
            Some("alchemy_basin") => Ok(ApparatusKind::InfusionBasin),
            Some("alchemy_alembic") => Ok(ApparatusKind::Alembic),
            Some("alchemy_filter") => Ok(ApparatusKind::FilterStand),
            _ => Err("That block is not an alchemy apparatus.".into()),
        }
    }

    fn persist_alchemy_state(
        &mut self,
        next_state: crate::alchemy::AlchemyState,
    ) -> Result<(), String> {
        // Ordinary world mutations use the normal autosave/shutdown barrier,
        // just like inventories, chunks, and block entities. Synchronously
        // rewriting the entire sidecar after every stir or inspection made a
        // large settlement freeze on input and still was not atomic with the
        // player's separately saved inventory. Current-moving operations use
        // `commit_alchemy_current*` below and retain their linked durable
        // replacement; this path only installs a validated in-memory result.
        next_state.validate().map_err(|error| error.to_string())?;
        self.alchemy_state = Some(next_state);
        Ok(())
    }

    fn commit_alchemy_current(
        &mut self,
        next_state: crate::alchemy::AlchemyState,
        operation_id: u64,
        content_id: &str,
        reason: &str,
        debits: Vec<(ArcaneOwner, Current)>,
        credits: Vec<(ArcaneOwner, Current, Option<String>)>,
    ) -> Result<(), String> {
        self.commit_alchemy_current_with_material(
            next_state,
            operation_id,
            content_id,
            reason,
            debits,
            credits,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_alchemy_current_with_material(
        &mut self,
        next_state: crate::alchemy::AlchemyState,
        operation_id: u64,
        content_id: &str,
        reason: &str,
        debits: Vec<(ArcaneOwner, Current)>,
        credits: Vec<(ArcaneOwner, Current, Option<String>)>,
        staged_material: Option<(crate::materials::MaterialLedger, Vec<u8>)>,
    ) -> Result<(), String> {
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The finite Current ledger is unavailable.")?;
        let mut reads = BTreeMap::<ArcaneOwner, u64>::new();
        for (owner, _) in &debits {
            reads.insert(owner.clone(), ledger.version_of(owner));
        }
        for (owner, _, _) in &credits {
            reads.insert(owner.clone(), ledger.version_of(owner));
        }
        let transaction = ArcaneTransaction {
            id: ledger
                .system_transaction_id()
                .map_err(|error| error.to_string())?,
            reads: reads
                .into_iter()
                .map(|(owner, expected_version)| AccountRead {
                    owner,
                    expected_version,
                })
                .collect(),
            debits: debits
                .into_iter()
                .filter(|(_, current)| !current.is_empty())
                .map(|(owner, current)| ArcaneMove {
                    owner,
                    current,
                    content_id: None,
                })
                .collect(),
            credits: credits
                .into_iter()
                .filter(|(_, current, _)| !current.is_empty())
                .map(|(owner, current, content_id)| ArcaneMove {
                    owner,
                    current,
                    content_id,
                })
                .collect(),
            transforms: Vec::new(),
            authority: ArcaneAuthority::System,
            reason: reason.into(),
            content_id: content_id.into(),
            linked: Vec::new(),
        };
        let replacement = LinkedFileReplacement {
            subsystem: "alchemy".into(),
            operation_id,
            relative_path: crate::alchemy::ALCHEMY_FILE.into(),
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
        self.alchemy_state = Some(next_state);
        if let Some((next_material, _)) = staged_material {
            self.material_ledger = Some(next_material);
        }
        Ok(())
    }
}

impl World {
    fn alchemy_load_carrier(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        slot: usize,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        let (preparation_id, batch_id, installation_id, current_volume) = {
            let apparatus = state
                .apparatus
                .get(&pos)
                .ok_or("The vessel is not installed.")?;
            let batch = apparatus
                .batch
                .as_ref()
                .ok_or("The vessel holds no measured batch.")?;
            (
                batch.preparation_id.clone(),
                batch.id,
                apparatus.installation_id,
                batch.liquid.volume_units,
            )
        };
        let definition = self
            .reg
            .preparations
            .get(&preparation_id)
            .ok_or("The saved preparation definition is unavailable.")?
            .clone();
        if state
            .apparatus
            .get(&pos)
            .and_then(|apparatus| apparatus.batch.as_ref())
            .and_then(|batch| definition.steps.get(usize::from(batch.step_index)))
            != Some(&ProcessStep::Load)
        {
            return Err("Loading carrier is not the current declared process step.".into());
        }
        let expected_item = self
            .reg
            .item_id(&definition.solvent_item)
            .ok_or("The declared carrier item is unavailable.")?;
        let stack = take_exact_slot(inventory, slot, expected_item)?;
        let amount = if definition.carrier.is_water() {
            HYDRO_UNITS_PER_BLOCK
        } else {
            CARRIER_ITEM_UNITS
        };
        if current_volume.saturating_add(amount) > definition.solvent_units {
            inventory.slots[slot] = Some(stack);
            return Err("That carrier would overfill the exact recipe volume.".into());
        }
        let temperature = state
            .apparatus
            .get(&pos)
            .map_or(20_000, |apparatus| apparatus.temperature_millic);
        if definition.carrier.is_water() {
            let empty_bucket = self
                .reg
                .item_id("base:bucket")
                .ok_or("The reusable empty bucket is unavailable.")?;
            if inventory.add(&self.reg, empty_bucket, 1) != 0 {
                return Err("Make room for the reusable empty bucket before pouring.".into());
            }
        }
        let water_move = if definition.carrier.is_water() {
            let class = match definition.carrier {
                CarrierKind::FreshWater => WaterClass::Fresh,
                CarrierKind::Brine => WaterClass::Salt,
                _ => unreachable!(),
            };
            let mass = self
                .planetary_weather
                .as_ref()
                .ok_or("Alchemy water needs the authoritative planetary water cycle.")?
                .preview_move_portable_to_industrial(class, amount)
                .ok_or("The portable-water ledger does not contain that full vessel.")?;
            Some((class, mass))
        } else {
            None
        };
        let water = water_move.map_or_else(ReservoirMass::default, |(_, mass)| mass);
        let parcel = ExactLiquid {
            carrier: Some(definition.carrier),
            volume_units: amount,
            water,
            carrier_state: WaterCarrier {
                thermal_millic_hu: i64::from(temperature)
                    .checked_mul(i64::try_from(amount).map_err(|_| "Carrier volume overflowed.")?)
                    .ok_or("Carrier heat custody overflowed.")?,
                dross_subunits: 0,
            },
            solutes: BTreeMap::from([(definition.solvent_item.clone(), amount)]),
        };
        parcel.validate().map_err(|error| error.to_string())?;
        let now = self.alchemy_tick();
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let complete = {
            let apparatus = state
                .apparatus
                .get_mut(&pos)
                .ok_or("The vessel disappeared.")?;
            let batch = apparatus.batch.as_mut().ok_or("The batch disappeared.")?;
            batch
                .liquid
                .checked_add(parcel)
                .map_err(|error| error.to_string())?;
            let complete = batch.liquid.volume_units == definition.solvent_units;
            if complete {
                add_dissolved_displacement(
                    &mut batch.liquid,
                    &definition,
                    apparatus.temperature_millic,
                )?;
                batch.observations.push(ProcessObservation {
                    step: ProcessStep::Load,
                    tick: now,
                    temperature_millic: apparatus.temperature_millic,
                    agitation: apparatus.agitation,
                    cleanliness_permille: apparatus.cleanliness_permille,
                    charge_delta: 0,
                });
                batch.step_index = batch.step_index.saturating_add(1);
                batch.started_tick = now;
                batch.due_tick = now.saturating_add(definition.process_ticks);
            }
            batch.revision = batch.revision.saturating_add(1);
            apparatus.cleanliness_permille = apparatus.cleanliness_permille.saturating_sub(3);
            apparatus.revision = apparatus.revision.saturating_add(1);
            apparatus.last_operator = request.actor;
            complete
        };
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id,
            actor: request.actor,
            action: "load_carrier".into(),
            preparation_id: preparation_id.clone(),
            volume_units: amount,
            current_units: 0,
            dross_units: 0,
            tick: now,
            note: if complete {
                "exact carrier volume complete".into()
            } else {
                "partial exact carrier volume loaded".into()
            },
        });
        let result = result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Pour,
            if complete {
                "The exact carrier volume is loaded."
            } else {
                "The vessel records the partial fill; more declared carrier is needed."
            },
            None,
        )?;
        state.validate().map_err(|error| error.to_string())?;
        if let Some((class, expected)) = water_move {
            let moved = self
                .planetary_weather
                .as_mut()
                .and_then(|weather| weather.move_portable_to_industrial(class, amount))
                .ok_or("Preflighted carrier water unexpectedly failed to move.")?;
            debug_assert_eq!(moved, expected);
        }
        Ok(result)
    }

    fn alchemy_load_filter(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        slot: usize,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        let stack = inventory
            .slots
            .get(slot)
            .copied()
            .flatten()
            .ok_or("That authoritative filter-media slot is empty.")?;
        let item_name = self.reg.item(stack.item).name.clone();
        if !matches!(
            item_name.as_str(),
            "base:ashlace_tissue" | "base:filter_cloth" | "base:charcoal" | "base:still_salt"
        ) {
            return Err(
                "The filter stand accepts Ashlace, cloth, charcoal, or still salt as physical media."
                    .into(),
            );
        }
        if stack.arcane_id != 0 && item_name != "base:ashlace_tissue" {
            return Err(
                "Only Ashlace may carry a stable magical identity into disposable filter media."
                    .into(),
            );
        }
        let (installation_id, batch_id, preparation_id) = {
            let apparatus = state
                .apparatus
                .get(&pos)
                .ok_or("The filter stand is not installed.")?;
            if apparatus.kind != ApparatusKind::FilterStand {
                return Err("Disposable filter media mounts only in a filter stand.".into());
            }
            if apparatus.filter_medium.is_some() || apparatus.filter_burden != 0 {
                return Err(
                    "Clean or recover the previous filter burden before mounting fresh media."
                        .into(),
                );
            }
            let batch = apparatus
                .batch
                .as_ref()
                .ok_or("There is no batch to filter.")?;
            let definition = self
                .reg
                .preparations
                .get(&batch.preparation_id)
                .ok_or("The saved preparation definition is unavailable.")?;
            if definition.steps.get(usize::from(batch.step_index)) != Some(&ProcessStep::Filter) {
                return Err("Filtering is not the current visible recipe step.".into());
            }
            (
                apparatus.installation_id,
                batch.id,
                batch.preparation_id.clone(),
            )
        };
        let taken = take_exact_slot(inventory, slot, stack.item)?;
        let materials = crate::materials::stack_materials(&self.reg, taken);
        let clean = if taken.arcane_id == 0 {
            Current::default()
        } else {
            self.arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.account(&ArcaneOwner::Item(taken.arcane_id)))
                .map_or_else(Current::default, |account| account.current.clone())
        };
        let dross = if taken.arcane_id == 0 {
            Current::default()
        } else {
            self.arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.account(&ArcaneOwner::ItemDross(taken.arcane_id)))
                .map_or_else(Current::default, |account| account.current.clone())
        };
        if taken.arcane_id != 0 && clean.is_empty() && dross.is_empty() {
            return Err("That stable Ashlace identity has no finite Current custody.".into());
        }
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let filter_owner_id = crate::alchemy::status_owner_id(
            state
                .allocate_status_id()
                .map_err(|error| error.to_string())?,
        );
        let now = self.alchemy_tick();
        let apparatus = state
            .apparatus
            .get_mut(&pos)
            .ok_or("The filter stand disappeared.")?;
        apparatus.filter_medium = Some(item_name.clone());
        apparatus.filter_owner_id = filter_owner_id;
        apparatus.filter_medium_materials = materials;
        let batch = apparatus
            .batch
            .as_mut()
            .ok_or("The filter batch disappeared.")?;
        batch.current_units = batch
            .current_units
            .checked_add(clean.total())
            .ok_or("Filter Current custody overflowed.")?;
        batch.charge_input_units = batch
            .charge_input_units
            .checked_add(clean.total())
            .ok_or("Filter admitted-charge counter overflowed.")?;
        batch.dross_units = batch
            .dross_units
            .checked_add(dross.total())
            .ok_or("Filter dross custody overflowed.")?;
        batch.revision = batch.revision.saturating_add(1);
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id,
            actor: request.actor,
            action: "load_filter".into(),
            preparation_id: preparation_id.clone(),
            volume_units: 0,
            current_units: clean.total(),
            dross_units: dross.total(),
            tick: now,
            note: format!("mounted one physical {item_name} filter medium"),
        });
        if !clean.is_empty() || !dross.is_empty() {
            let mut debits = Vec::new();
            let mut credits = Vec::new();
            if !clean.is_empty() {
                debits.push((ArcaneOwner::Item(taken.arcane_id), clean.clone()));
                credits.push((
                    ArcaneOwner::Alchemy(crate::alchemy::batch_owner_id(batch_id)),
                    clean,
                    Some(preparation_id.clone()),
                ));
            }
            if !dross.is_empty() {
                debits.push((ArcaneOwner::ItemDross(taken.arcane_id), dross.clone()));
                credits.push((
                    ArcaneOwner::AlchemyDross(crate::alchemy::batch_owner_id(batch_id)),
                    dross,
                    Some(preparation_id.clone()),
                ));
            }
            self.commit_alchemy_current(
                state.clone(),
                operation_id,
                &preparation_id,
                "Ashlace filter media transferred its finite charge and burden into the batch",
                debits,
                credits,
            )?;
        }
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Filter,
            "Fresh physical media is mounted; the next filter step will leave a hazardous burden.",
            None,
        )
    }

    fn alchemy_set_heat(
        &self,
        pos: BlockPos,
        request: &AlchemyRequest,
        temperature_millic: i32,
        state: &mut crate::alchemy::AlchemyState,
    ) -> Result<AlchemyResult, String> {
        if !(-50_000..=250_000).contains(&temperature_millic) {
            return Err(
                "That requested temperature is outside the physical apparatus range.".into(),
            );
        }
        let ambient_millic = (self.weather_at_surface(pos.surface()).temperature_c * 1_000.0)
            .round()
            .clamp(-50_000.0, 60_000.0) as i32;
        let mut minimum_millic = ambient_millic;
        let mut maximum_millic = ambient_millic;
        for at in (-1..=1)
            .flat_map(|du| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dv| (du, dy, dv))))
            .filter(|offset| *offset != (0, 0, 0))
            .filter_map(|(du, dy, dv)| pos.offset(du, dy, dv))
        {
            let block = self.get_block_at(at);
            let name = self.reg.block(block).name.as_str();
            if self.reg.is_lava(block) {
                maximum_millic = maximum_millic.max(180_000);
            } else if name == "base:firebox_lit" {
                maximum_millic = maximum_millic.max(150_000);
            } else if name == "base:fire" {
                maximum_millic = maximum_millic.max(105_000);
            } else if name == "base:furnace"
                && self
                    .block_entity_at(&at)
                    .is_some_and(|entity| matches!(entity, BlockEntity::Furnace(furnace) if furnace.burn_left > 0.0))
            {
                maximum_millic = maximum_millic.max(125_000);
            }
            if name == "base:ice" || name.starts_with("base:snow") {
                minimum_millic = minimum_millic.min(-8_000);
            }
        }
        if !(minimum_millic..=maximum_millic).contains(&temperature_millic) {
            return Err(format!(
                "The nearby ordinary heat/cooling arrangement can hold only {minimum_millic}..={maximum_millic} m°C; it cannot assert the requested temperature."
            ));
        }
        let apparatus = state
            .apparatus
            .get_mut(&pos)
            .ok_or("The apparatus is not installed.")?;
        if apparatus.batch.is_none() {
            return Err("There is no batch here to heat or cool.".into());
        }
        let delta = temperature_millic.saturating_sub(apparatus.temperature_millic);
        apparatus.temperature_millic = apparatus
            .temperature_millic
            .saturating_add(delta.clamp(-20_000, 20_000));
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Bubble,
            "The vessel temperature moves one bounded interval toward the ordinary heat control.",
            None,
        )
    }

    fn alchemy_set_agitation(
        &self,
        pos: BlockPos,
        request: &AlchemyRequest,
        agitation: AgitationKind,
        state: &mut crate::alchemy::AlchemyState,
    ) -> Result<AlchemyResult, String> {
        let apparatus = state
            .apparatus
            .get_mut(&pos)
            .ok_or("The apparatus is not installed.")?;
        if apparatus.batch.is_none() {
            return Err("There is no batch here to stir or settle.".into());
        }
        apparatus.agitation = agitation;
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Bubble,
            "The mechanical agitation setting is changed.",
            None,
        )
    }

    fn alchemy_advance(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        step: ProcessStep,
        state: &mut crate::alchemy::AlchemyState,
    ) -> Result<AlchemyResult, String> {
        let (batch_id, preparation_id, installation_id, step_index) = {
            let apparatus = state
                .apparatus
                .get(&pos)
                .ok_or("The apparatus is not installed.")?;
            let batch = apparatus.batch.as_ref().ok_or("There is no batch here.")?;
            (
                batch.id,
                batch.preparation_id.clone(),
                apparatus.installation_id,
                batch.step_index,
            )
        };
        let definition = self
            .reg
            .preparations
            .get(&preparation_id)
            .ok_or("The saved preparation definition is unavailable.")?
            .clone();
        if definition.steps.get(usize::from(step_index)) != Some(&step) {
            return Err("The requested operation is out of the visible recipe order.".into());
        }
        if matches!(
            step,
            ProcessStep::Grind | ProcessStep::Load | ProcessStep::Charge
        ) {
            return Err("That process step has its own embodied station action.".into());
        }
        let apparatus_kind = state
            .apparatus
            .get(&pos)
            .map(|apparatus| apparatus.kind)
            .ok_or("The apparatus is not installed.")?;
        let required_kind = match step {
            ProcessStep::Filter => ApparatusKind::FilterStand,
            ProcessStep::Distill => ApparatusKind::Alembic,
            _ => definition.process.apparatus(),
        };
        if apparatus_kind != required_kind {
            return Err(format!(
                "The visible {step:?} step needs its {required_kind:?}; transfer the batch physically."
            ));
        }
        if step == ProcessStep::Filter
            && state
                .apparatus
                .get(&pos)
                .is_none_or(|apparatus| apparatus.filter_medium.is_none())
        {
            return Err(
                "Mount one physical cloth, charcoal, or still-salt medium before filtering.".into(),
            );
        }
        let (filter_owner_id, filter_capacity) = if step == ProcessStep::Filter {
            let apparatus = state
                .apparatus
                .get(&pos)
                .ok_or("The filter stand disappeared.")?;
            let medium = apparatus
                .filter_medium
                .as_deref()
                .ok_or("The filter medium disappeared.")?;
            let capacity = match medium {
                "base:filter_cloth" => 4,
                "base:charcoal" => 8,
                "base:still_salt" => 16,
                "base:ashlace_tissue" => 32,
                _ => return Err("The mounted filter medium is no longer approved.".into()),
            };
            (apparatus.filter_owner_id, capacity)
        } else {
            (0, 0)
        };
        let now = self.alchemy_tick();
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let (failure, final_step, captured_dross) = {
            let apparatus = state
                .apparatus
                .get_mut(&pos)
                .ok_or("The apparatus disappeared.")?;
            let temperature_millic = apparatus.temperature_millic;
            let agitation = apparatus.agitation;
            let cleanliness_permille = apparatus.cleanliness_permille;
            let batch = apparatus.batch.as_mut().ok_or("The batch disappeared.")?;
            let failure = process_failure(
                &definition,
                temperature_millic,
                agitation,
                cleanliness_permille,
                batch,
                step,
                now,
            );
            batch.observations.push(ProcessObservation {
                step,
                tick: now,
                temperature_millic: apparatus.temperature_millic,
                agitation: apparatus.agitation,
                cleanliness_permille: apparatus.cleanliness_permille,
                charge_delta: 0,
            });
            batch.step_index = batch.step_index.saturating_add(1);
            if let Some(failure) = failure {
                batch.outcome = BatchOutcome::Failed(failure);
            } else if step == ProcessStep::Distill {
                separate_brine_distillate(batch)?;
            }
            let final_step = usize::from(batch.step_index) == definition.steps.len();
            if final_step && failure.is_none() {
                batch.outcome = BatchOutcome::Ready;
            }
            batch.revision = batch.revision.saturating_add(1);
            apparatus.cleanliness_permille = apparatus.cleanliness_permille.saturating_sub(
                if matches!(step, ProcessStep::Distill | ProcessStep::Filter) {
                    18
                } else {
                    4
                },
            );
            let captured_dross = if step == ProcessStep::Filter {
                let captured = batch.dross_units.min(filter_capacity);
                batch.dross_units -= captured;
                apparatus.filter_burden = apparatus
                    .filter_burden
                    .checked_add(captured)
                    .ok_or("Filter burden overflowed.")?;
                captured
            } else {
                0
            };
            if matches!(step, ProcessStep::Heat | ProcessStep::Distill) {
                apparatus.integrity_permille = apparatus.integrity_permille.saturating_sub(1);
            }
            apparatus.revision = apparatus.revision.saturating_add(1);
            apparatus.last_operator = request.actor;
            (failure, final_step, captured_dross)
        };
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id,
            actor: request.actor,
            action: format!("advance_{step:?}").to_lowercase(),
            preparation_id: preparation_id.clone(),
            volume_units: 0,
            current_units: 0,
            dross_units: captured_dross,
            tick: now,
            note: failure.map_or_else(
                || {
                    if final_step {
                        "declared batch ready".into()
                    } else {
                        "declared control step accepted".into()
                    }
                },
                |failure| format!("deterministic named failure: {failure:?}"),
            ),
        });
        if captured_dross != 0 {
            let source_owner = ArcaneOwner::AlchemyDross(crate::alchemy::batch_owner_id(batch_id));
            let mut available = self
                .arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.account(&source_owner))
                .map(|account| account.current.clone())
                .ok_or("The batch's filterable dross account is unavailable.")?;
            let captured = available
                .take_units(captured_dross, [definition.resonance.clone()])
                .map_err(|error| error.to_string())?;
            self.commit_alchemy_current(
                state.clone(),
                operation_id,
                &preparation_id,
                "physical filter media captured exact batch dross",
                vec![(source_owner, captured.clone())],
                vec![(
                    ArcaneOwner::AlchemyDross(filter_owner_id),
                    captured,
                    Some("base:spent_filter".into()),
                )],
            )?;
        }
        result_for(
            &self.reg,
            state,
            pos,
            failure.map_or(AlchemyCueKind::Bubble, |failure| {
                if failure == BatchFailure::OverchargedBatch {
                    AlchemyCueKind::Overcharge
                } else {
                    AlchemyCueKind::Leak
                }
            }),
            failure.map_or(
                if final_step {
                    "The preparation reaches its declared stable batch state."
                } else {
                    "The measured process step completes."
                },
                |_| "The controls produce a deterministic named failure; every input remains in custody.",
            ),
            None,
        )
    }

    fn alchemy_sample(
        &self,
        pos: BlockPos,
        request: &AlchemyRequest,
        state: &mut crate::alchemy::AlchemyState,
    ) -> Result<AlchemyResult, String> {
        let apparatus = state
            .apparatus
            .get_mut(&pos)
            .ok_or("The apparatus is not installed.")?;
        let batch = apparatus
            .batch
            .as_mut()
            .ok_or("There is no batch to sample.")?;
        batch.revision = batch.revision.saturating_add(1);
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Drip,
            "A non-consuming tuning sample reports only this batch's temperature, step, fill, condition, and coarse charge cue.",
            None,
        )
    }

    fn alchemy_charge(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        source_slot: Option<usize>,
        units: u64,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &Inventory,
    ) -> Result<AlchemyResult, String> {
        let (batch_id, preparation_id, installation_id, prior_input, prior_clean) = {
            let apparatus = state
                .apparatus
                .get(&pos)
                .ok_or("The apparatus is not installed.")?;
            let batch = apparatus.batch.as_ref().ok_or("There is no batch here.")?;
            (
                batch.id,
                batch.preparation_id.clone(),
                apparatus.installation_id,
                batch.charge_input_units,
                batch.current_units,
            )
        };
        let definition = self
            .reg
            .preparations
            .get(&preparation_id)
            .ok_or("The saved preparation definition is unavailable.")?
            .clone();
        let step = state
            .apparatus
            .get(&pos)
            .and_then(|apparatus| apparatus.batch.as_ref())
            .and_then(|batch| definition.steps.get(usize::from(batch.step_index)))
            .copied();
        if step != Some(ProcessStep::Charge) {
            return Err("Charging is not the current visible recipe step.".into());
        }
        if units > crate::alchemy::MAX_PREPARATION_CHARGE {
            return Err("That charge request exceeds the bounded apparatus capacity.".into());
        }
        if units != 0 && !self.has_adjacent_alchemy_conductor(pos) {
            return Err(
                "The vessel needs an adjacent ordinary arcane conductor for charge transfer."
                    .into(),
            );
        }
        let target_owner = ArcaneOwner::Alchemy(crate::alchemy::batch_owner_id(batch_id));
        let dross_owner = ArcaneOwner::AlchemyDross(crate::alchemy::batch_owner_id(batch_id));
        let (source_owner, moved) = if units == 0 {
            (None, Current::default())
        } else if let Some(slot) = source_slot {
            let stack = inventory
                .slots
                .get(slot)
                .copied()
                .flatten()
                .ok_or("That authoritative charge-source slot is empty.")?;
            if stack.arcane_id == 0 {
                return Err("That physical item has no bound Current account.".into());
            }
            if !self
                .reg
                .item(stack.item)
                .implement
                .as_ref()
                .is_some_and(|implement| {
                    implement.kind == crate::implements::ImplementItemKind::ChargeVessel
                })
                || !self
                    .implements_state
                    .as_ref()
                    .and_then(|state| state.instance(stack.arcane_id))
                    .is_some_and(|instance| {
                        matches!(
                            instance.kind,
                            crate::implements::ImplementKind::Vessel { .. }
                        )
                    })
            {
                return Err(
                    "Only a live, embodied charge vessel can fund an alchemy transfer.".into(),
                );
            }
            let owner = ArcaneOwner::Item(stack.arcane_id);
            let account = self
                .arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.account(&owner))
                .ok_or("That charge source has no live clean Current custody.")?;
            if account.current.units_of(&definition.resonance) < units {
                return Err("That vessel cannot fund the requested resonance and amount.".into());
            }
            (
                Some(owner),
                Current::single(definition.resonance.clone(), units),
            )
        } else {
            let atlas = self
                .planet_atlas
                .as_ref()
                .ok_or("Ambient charging needs the authoritative planet atlas.")?;
            let owner = ArcaneOwner::Ambient(atlas.atlas_pos(pos.surface()));
            let account = self
                .arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.account(&owner))
                .ok_or("No local Ambient Current account can fund this transfer.")?;
            if account.current.units_of(&definition.resonance) < units {
                return Err("The local ambient resonance cannot fund that measured charge.".into());
            }
            (
                Some(owner),
                Current::single(definition.resonance.clone(), units),
            )
        };
        let next_input = prior_input
            .checked_add(moved.total())
            .ok_or("Batch charge input overflowed.")?;
        let rate_failure = if units != 0 && units < u64::from(definition.charge_rate[0]) {
            Some(BatchFailure::SpentLiquor)
        } else if units > u64::from(definition.charge_rate[1]) {
            Some(BatchFailure::OverchargedBatch)
        } else {
            None
        };
        let finish = units == 0 || next_input >= definition.charge_units || rate_failure.is_some();
        let mut failure = rate_failure;
        if finish && failure.is_none() {
            failure = if next_input < definition.charge_units {
                Some(BatchFailure::SpentLiquor)
            } else if next_input > definition.charge_units {
                Some(BatchFailure::OverchargedBatch)
            } else {
                None
            };
        }
        let declared_dross = if finish {
            definition
                .dross_units
                .min(prior_clean.saturating_add(moved.total()))
        } else {
            0
        };
        let from_new = declared_dross.min(moved.units_of(&definition.resonance));
        let from_existing = declared_dross.saturating_sub(from_new);
        let new_clean_credit = moved
            .units_of(&definition.resonance)
            .saturating_sub(from_new);
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let now = self.alchemy_tick();
        {
            let apparatus = state
                .apparatus
                .get_mut(&pos)
                .ok_or("The apparatus disappeared.")?;
            let batch = apparatus.batch.as_mut().ok_or("The batch disappeared.")?;
            batch.charge_input_units = next_input;
            batch.current_units = prior_clean
                .checked_add(moved.total())
                .and_then(|value| value.checked_sub(declared_dross))
                .ok_or("Batch clean Current settlement underflowed.")?;
            batch.dross_units = batch
                .dross_units
                .checked_add(declared_dross)
                .ok_or("Batch dross settlement overflowed.")?;
            batch.observations.push(ProcessObservation {
                step: ProcessStep::Charge,
                tick: now,
                temperature_millic: apparatus.temperature_millic,
                agitation: apparatus.agitation,
                cleanliness_permille: apparatus.cleanliness_permille,
                charge_delta: moved.total(),
            });
            if finish {
                batch.step_index = batch.step_index.saturating_add(1);
                if let Some(failure) = failure {
                    batch.outcome = BatchOutcome::Failed(failure);
                }
            }
            batch.revision = batch.revision.saturating_add(1);
            apparatus.revision = apparatus.revision.saturating_add(1);
            apparatus.last_operator = request.actor;
        }
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id,
            actor: request.actor,
            action: "charge".into(),
            preparation_id: preparation_id.clone(),
            volume_units: 0,
            current_units: moved.total().saturating_sub(declared_dross),
            dross_units: declared_dross,
            tick: now,
            note: failure.map_or_else(
                || {
                    if finish {
                        "measured charge step complete".into()
                    } else {
                        "bounded charge increment admitted".into()
                    }
                },
                |failure| format!("deterministic named charge failure: {failure:?}"),
            ),
        });
        let mut debits = Vec::new();
        let mut credits = Vec::new();
        if let Some(source_owner) = source_owner
            && !moved.is_empty()
        {
            debits.push((source_owner, moved.clone()));
        }
        if new_clean_credit != 0 {
            credits.push((
                target_owner.clone(),
                Current::single(definition.resonance.clone(), new_clean_credit),
                Some(preparation_id.clone()),
            ));
        }
        if from_existing != 0 {
            debits.push((
                target_owner,
                Current::single(definition.resonance.clone(), from_existing),
            ));
        }
        if declared_dross != 0 {
            credits.push((
                dross_owner,
                Current::single(definition.resonance.clone(), declared_dross),
                Some(preparation_id.clone()),
            ));
        }
        if !debits.is_empty() || !credits.is_empty() {
            self.commit_alchemy_current(
                state.clone(),
                operation_id,
                &preparation_id,
                "measured preparation charge and dross settlement",
                debits,
                credits,
            )?;
        }
        result_for(
            &self.reg,
            state,
            pos,
            failure.map_or(AlchemyCueKind::Pulse, |_| AlchemyCueKind::Overcharge),
            failure.map_or(
                if finish {
                    "The charge step settles its declared clean and dross custody."
                } else {
                    "A bounded charge increment enters the batch."
                },
                |_| "The measured rate or amount produces its deterministic named failure.",
            ),
            None,
        )
    }

    fn has_adjacent_alchemy_conductor(&self, pos: BlockPos) -> bool {
        [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0)]
            .into_iter()
            .filter_map(|(du, dy, dv)| pos.offset(du, dy, dv))
            .any(|at| self.reg.block(self.get_block_at(at)).name == "base:arcane_conductor")
    }
}

fn process_failure(
    definition: &crate::alchemy::PreparationDef,
    temperature_millic: i32,
    agitation: AgitationKind,
    cleanliness_permille: u16,
    batch: &AlchemyBatch,
    step: ProcessStep,
    now: u64,
) -> Option<BatchFailure> {
    if batch
        .ingredients
        .iter()
        .any(|ingredient| ingredient.condition_permille < 250)
    {
        return Some(BatchFailure::WeakExtraction);
    }
    if cleanliness_permille < definition.cleanliness_min {
        return Some(BatchFailure::FouledBatch);
    }
    if batch.charge_input_units > definition.charge_units {
        return Some(BatchFailure::OverchargedBatch);
    }
    if step == ProcessStep::Charge && batch.charge_input_units < definition.charge_units {
        return Some(BatchFailure::SpentLiquor);
    }
    if matches!(
        step,
        ProcessStep::Heat
            | ProcessStep::Agitate
            | ProcessStep::Settle
            | ProcessStep::Distill
            | ProcessStep::Filter
    ) {
        if temperature_millic > definition.temperature_millic[1] {
            return Some(BatchFailure::ScorchedMash);
        }
        if temperature_millic < definition.temperature_millic[0] {
            return Some(BatchFailure::WeakExtraction);
        }
    }
    if step == ProcessStep::Cool
        && !(definition.storage_temperature_millic[0]..=definition.storage_temperature_millic[1])
            .contains(&temperature_millic)
    {
        return Some(
            if temperature_millic > definition.storage_temperature_millic[1] {
                BatchFailure::ScorchedMash
            } else {
                BatchFailure::BrokenEmulsion
            },
        );
    }
    if step == ProcessStep::Agitate && agitation != definition.agitation {
        return Some(BatchFailure::BrokenEmulsion);
    }
    if matches!(
        step,
        ProcessStep::Settle | ProcessStep::Distill | ProcessStep::Filter
    ) && now < batch.due_tick
    {
        return Some(BatchFailure::WeakExtraction);
    }
    None
}

/// A successful still run moves the exact salt mass out of brine without
/// creating or deleting a single hydro unit. The salt remains physically in
/// the apparatus residue until cleaning or disposal returns it through the
/// ordinary water/material paths; the bottled condensate is fresh water.
fn separate_brine_distillate(batch: &mut AlchemyBatch) -> Result<u64, String> {
    if batch.liquid.carrier != Some(CarrierKind::Brine) {
        return Ok(0);
    }
    let salt_mass = batch.liquid.water.salt_mass;
    batch.residue_water.salt_mass = batch
        .residue_water
        .salt_mass
        .checked_add(salt_mass)
        .ok_or("Distillation salt residue overflowed its exact custody.")?;
    batch.liquid.water.salt_mass = 0;
    batch.liquid.carrier = Some(CarrierKind::FreshWater);
    batch.liquid.validate().map_err(|error| error.to_string())?;
    Ok(salt_mass)
}

impl World {
    fn alchemy_decant(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        vessel_slot: usize,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        let (batch_id, preparation_id, installation_id, outcome, before_volume) = {
            let apparatus = state
                .apparatus
                .get(&pos)
                .ok_or("The apparatus is not installed.")?;
            let batch = apparatus
                .batch
                .as_ref()
                .ok_or("There is no batch to decant.")?;
            (
                batch.id,
                batch.preparation_id.clone(),
                apparatus.installation_id,
                batch.outcome.clone(),
                batch.liquid.volume_units,
            )
        };
        if !matches!(
            outcome,
            BatchOutcome::Ready | BatchOutcome::Failed(_) | BatchOutcome::Spoiled
        ) {
            return Err(
                "The batch is still processing and cannot be bottled as a finished dose.".into(),
            );
        }
        let definition = self
            .reg
            .preparations
            .get(&preparation_id)
            .ok_or("The saved preparation definition is unavailable.")?
            .clone();
        if before_volume < definition.dose_units {
            return Err(
                "Less than one declared minimum dose remains; drain it through a disposal path."
                    .into(),
            );
        }
        let vessel = self
            .reg
            .item_id(&definition.empty_vessel)
            .ok_or("The declared reusable vessel is unavailable.")?;
        let empty_vessel = take_exact_slot(inventory, vessel_slot, vessel)?;
        let vessel_materials = crate::materials::stack_materials(&self.reg, empty_vessel);
        let item_name = match outcome {
            BatchOutcome::Ready => definition.output_item.clone(),
            BatchOutcome::Failed(failure) => failure.item_id().into(),
            BatchOutcome::Spoiled => BatchFailure::SpentLiquor.item_id().into(),
            BatchOutcome::Processing => unreachable!(),
        };
        let clean_owner = ArcaneOwner::Alchemy(crate::alchemy::batch_owner_id(batch_id));
        let dross_owner = ArcaneOwner::AlchemyDross(crate::alchemy::batch_owner_id(batch_id));
        let clean_account = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&clean_owner))
            .map(|account| account.current.clone())
            .unwrap_or_default();
        let dross_account = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&dross_owner))
            .map(|account| account.current.clone())
            .unwrap_or_default();
        let clean_units =
            proportional_units(clean_account.total(), definition.dose_units, before_volume)?;
        let dross_units =
            proportional_units(dross_account.total(), definition.dose_units, before_volume)?;
        let mut clean_work = clean_account;
        let clean = clean_work
            .take_units(clean_units, [definition.resonance.clone()])
            .map_err(|error| error.to_string())?;
        let mut dross_work = dross_account;
        let dross = dross_work
            .take_units(dross_units, [definition.resonance.clone()])
            .map_err(|error| error.to_string())?;
        if clean.is_empty() && dross.is_empty() {
            return Err(
                "A state-bearing alchemy dose has no finite Current identity to carry.".into(),
            );
        }
        let container_id = self
            .arcane_ledger
            .as_mut()
            .ok_or("The finite Current ledger is unavailable.")?
            .allocate_item_id()
            .map_err(|error| error.to_string())?;
        let now = self.alchemy_tick();
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let (liquid, materials, born_tick, expires_tick, source_batch, dose_outcome) = {
            let apparatus = state
                .apparatus
                .get_mut(&pos)
                .ok_or("The apparatus disappeared.")?;
            let batch = apparatus.batch.as_mut().ok_or("The batch disappeared.")?;
            let liquid = batch
                .liquid
                .take(definition.dose_units)
                .map_err(|error| error.to_string())?;
            let mut materials = MaterialVector::new();
            for ingredient in &mut batch.ingredients {
                let parcel = take_material_fraction(
                    &mut ingredient.retained_materials,
                    definition.dose_units,
                    before_volume,
                )?;
                add_materials(&mut materials, &parcel)?;
            }
            batch.current_units = batch
                .current_units
                .checked_sub(clean.total())
                .ok_or("Batch clean Current no longer matches its ledger.")?;
            batch.dross_units = batch
                .dross_units
                .checked_sub(dross.total())
                .ok_or("Batch dross no longer matches its ledger.")?;
            batch.revision = batch.revision.saturating_add(1);
            apparatus.revision = apparatus.revision.saturating_add(1);
            apparatus.last_operator = request.actor;
            (
                liquid,
                materials,
                batch.born_tick,
                batch.expires_tick,
                batch.id,
                batch.outcome.clone(),
            )
        };
        state.containers.insert(
            container_id,
            crate::alchemy::PreparationDose {
                container_id,
                preparation_id: preparation_id.clone(),
                definition_version: definition.version,
                item_name: item_name.clone(),
                liquid,
                vessel_materials,
                materials,
                current_units: clean.total(),
                dross_units: dross.total(),
                born_tick,
                expires_tick,
                last_storage_tick: now,
                outcome: dose_outcome,
                source_installation: installation_id,
                source_batch,
            },
        );
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id,
            actor: request.actor,
            action: "decant".into(),
            preparation_id: preparation_id.clone(),
            volume_units: definition.dose_units,
            current_units: clean.total(),
            dross_units: dross.total(),
            tick: now,
            note: format!("exact dose decanted into stable container {container_id}"),
        });
        let filled = produced(&self.reg, &item_name, 1, container_id)?;
        let filled_stack = filled
            .clone()
            .into_stack(&self.reg)
            .map_err(|error| error.to_string())?;
        if inventory.add_stack(&self.reg, filled_stack) != 0 {
            return Err("The filled vessel could not enter the authoritative inventory.".into());
        }
        let mut debits = Vec::new();
        let mut credits = Vec::new();
        if !clean.is_empty() {
            debits.push((clean_owner, clean.clone()));
            credits.push((
                ArcaneOwner::Item(container_id),
                clean,
                Some(item_name.clone()),
            ));
        }
        if !dross.is_empty() {
            debits.push((dross_owner, dross.clone()));
            credits.push((
                ArcaneOwner::ItemDross(container_id),
                dross,
                Some(item_name.clone()),
            ));
        }
        self.commit_alchemy_current(
            state.clone(),
            operation_id,
            &preparation_id,
            "exact preparation dose decanted into one stable reusable vessel",
            debits,
            credits,
        )?;
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Pour,
            "One exact dose leaves the batch; volume and every fixed-point remainder stay conserved.",
            Some(filled),
        )
    }
}

fn proportional_units(
    total: u64,
    requested_volume: u64,
    before_volume: u64,
) -> Result<u64, String> {
    if requested_volume > before_volume || before_volume == 0 {
        return Err("Invalid exact-volume proportion.".into());
    }
    if requested_volume == before_volume {
        return Ok(total);
    }
    u64::try_from(u128::from(total) * u128::from(requested_volume) / u128::from(before_volume))
        .map_err(|_| "Exact-volume proportion overflowed.".into())
}

/// Dissolved physical ingredients may displace a declared amount of liquid
/// volume without creating water or carrier matter. Keep that displacement
/// explicit in the solution's volume, heat, and named-solute state so a mod
/// recipe admitted by validation can actually yield every declared dose.
fn add_dissolved_displacement(
    liquid: &mut ExactLiquid,
    definition: &crate::alchemy::PreparationDef,
    temperature_millic: i32,
) -> Result<(), String> {
    let displaced = definition.dissolved_units;
    if displaced == 0 {
        return Ok(());
    }
    let before = liquid.volume_units;
    if before != definition.solvent_units || liquid.carrier != Some(definition.carrier) {
        return Err("Dissolved displacement needs the complete declared carrier first.".into());
    }
    liquid.volume_units = before
        .checked_add(displaced)
        .filter(|volume| *volume <= crate::alchemy::MAX_BATCH_VOLUME_UNITS)
        .ok_or("Dissolved displacement overflowed the batch vessel.")?;
    liquid.carrier_state.thermal_millic_hu = liquid
        .carrier_state
        .thermal_millic_hu
        .checked_add(
            i64::from(temperature_millic)
                .checked_mul(
                    i64::try_from(displaced)
                        .map_err(|_| "Dissolved displacement heat overflowed.")?,
                )
                .ok_or("Dissolved displacement heat overflowed.")?,
        )
        .ok_or("Dissolved displacement heat overflowed.")?;
    let total_parts = definition
        .ingredients
        .iter()
        .map(|ingredient| u64::from(ingredient.count))
        .sum::<u64>();
    if total_parts == 0 {
        return Err("Dissolved displacement has no physical ingredient source.".into());
    }
    let mut assigned = 0u64;
    for (index, ingredient) in definition.ingredients.iter().enumerate() {
        let units = if index + 1 == definition.ingredients.len() {
            displaced.saturating_sub(assigned)
        } else {
            u64::try_from(
                u128::from(displaced) * u128::from(ingredient.count) / u128::from(total_parts),
            )
            .map_err(|_| "Dissolved displacement split overflowed.")?
        };
        assigned = assigned
            .checked_add(units)
            .ok_or("Dissolved displacement split overflowed.")?;
        if units != 0 {
            let entry = liquid.solutes.entry(ingredient.item.clone()).or_default();
            *entry = entry
                .checked_add(units)
                .ok_or("Dissolved solute custody overflowed.")?;
        }
    }
    liquid.validate().map_err(|error| error.to_string())
}

fn take_material_fraction(
    source: &mut MaterialVector,
    requested_volume: u64,
    before_volume: u64,
) -> Result<MaterialVector, String> {
    let mut parcel = MaterialVector::new();
    for (name, remaining) in source.iter_mut() {
        let moved = proportional_units(*remaining, requested_volume, before_volume)?;
        *remaining -= moved;
        if moved != 0 {
            parcel.insert(name.clone(), moved);
        }
    }
    source.retain(|_, units| *units != 0);
    Ok(parcel)
}

impl World {
    fn alchemy_repair(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        material_slot: usize,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        let snapshot = state
            .apparatus
            .get(&pos)
            .cloned()
            .ok_or("The apparatus is not installed.")?;
        if snapshot.integrity_permille >= 1_000 {
            return Err("The apparatus is already structurally sound.".into());
        }
        if snapshot.batch.is_some()
            || !snapshot.residue_materials.is_empty()
            || snapshot.filter_burden != 0
            || snapshot.filter_medium.is_some()
        {
            return Err(
                "Drain and clean the apparatus before repairing its embodied structure.".into(),
            );
        }
        let material = inventory
            .slots
            .get(material_slot)
            .copied()
            .flatten()
            .ok_or("Hold one matching repair material.")?;
        let material_name = self.reg.item(material.item).name.as_str();
        let matching = match snapshot.kind {
            ApparatusKind::Mortar => material_name == "base:cobblestone",
            ApparatusKind::InfusionBasin => {
                matches!(material_name, "base:glass" | "base:bronze_ingot")
            }
            ApparatusKind::Alembic => matches!(
                material_name,
                "base:glass" | "base:copper_ingot" | "base:bronze_ingot"
            ),
            ApparatusKind::FilterStand => {
                matches!(material_name, "base:glass" | "base:filter_cloth")
                    || self
                        .reg
                        .tags
                        .get("base:planks")
                        .is_some_and(|items| items.contains(&material.item))
            }
        };
        if !matching {
            return Err(match snapshot.kind {
                ApparatusKind::Mortar => "Repair this mortar with cobblestone.",
                ApparatusKind::InfusionBasin => "Repair this basin with glass or bronze.",
                ApparatusKind::Alembic => "Repair this alembic with glass, copper, or bronze.",
                ApparatusKind::FilterStand => {
                    "Repair this stand with glass, filter cloth, or matching planks."
                }
            }
            .into());
        }
        let hammer_slot = inventory
            .slots
            .iter()
            .position(|stack| stack.is_some_and(|stack| self.reg.item(stack.item).hammer))
            .ok_or("Structural apparatus repair requires a carried hammer.")?;
        let consumed = inventory
            .take_one_stack(material_slot)
            .ok_or("The matching repair material moved before use.")?;
        let repair_materials = crate::materials::stack_materials(&self.reg, consumed);
        inventory.wear_tool(&self.reg, hammer_slot);

        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let now = self.alchemy_tick();
        let apparatus = state
            .apparatus
            .get_mut(&pos)
            .ok_or("The apparatus disappeared during repair.")?;
        let before = apparatus.integrity_permille;
        apparatus.integrity_permille = apparatus.integrity_permille.saturating_add(250).min(1_000);
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        let after = apparatus.integrity_permille;
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id: snapshot.installation_id,
            batch_id: 0,
            actor: request.actor,
            action: "repair".into(),
            preparation_id: "base:apparatus_repair".into(),
            volume_units: 0,
            current_units: 0,
            dross_units: 0,
            tick: now,
            note: format!("consumed one {material_name}; structural integrity {before}->{after}"),
        });
        let staged_material = self
            .material_ledger
            .as_ref()
            .map(|ledger| ledger.stage_linked_consumption(&repair_materials))
            .transpose()
            .map_err(|error| error.to_string())?
            .flatten();
        if staged_material.is_some() {
            self.commit_alchemy_current_with_material(
                state.clone(),
                operation_id,
                "base:apparatus_repair",
                "matching repair material entered the apparatus in the same linked commit",
                Vec::new(),
                Vec::new(),
                staged_material,
            )?;
        }
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Clean,
            "Matching material and hammer work restore a bounded part of the apparatus; no upgrade multiplies yield.",
            None,
        )
    }

    fn alchemy_clean(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        water_slot: usize,
        filter_slot: Option<usize>,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        let (
            installation_id,
            batch_snapshot,
            apparatus_residue,
            burden,
            mounted_filter,
            filter_owner_id,
            mounted_filter_materials,
        ) = {
            let apparatus = state
                .apparatus
                .get(&pos)
                .ok_or("The apparatus is not installed.")?;
            (
                apparatus.installation_id,
                apparatus.batch.clone(),
                apparatus.residue_materials.clone(),
                apparatus.filter_burden,
                apparatus.filter_medium.is_some(),
                apparatus.filter_owner_id,
                apparatus.filter_medium_materials.clone(),
            )
        };
        if batch_snapshot.as_ref().is_some_and(|batch| {
            batch.liquid.volume_units != 0 || batch.current_units != 0 || batch.dross_units != 0
        }) {
            return Err(
                "Decant or drain every liquid and Current remainder before cleaning.".into(),
            );
        }
        if batch_snapshot.is_none()
            && apparatus_residue.is_empty()
            && burden == 0
            && !mounted_filter
        {
            return Err("The apparatus is already clean.".into());
        }
        let water_item = self
            .reg
            .item_id("base:bucket_water")
            .ok_or("Fresh cleaning water is unavailable.")?;
        let _water_stack = take_exact_slot(inventory, water_slot, water_item)?;
        let filter_stack = if let Some(slot) = filter_slot {
            let filter_item = self
                .reg
                .item_id("base:filter_cloth")
                .ok_or("Filter cloth is unavailable.")?;
            Some(take_exact_slot(inventory, slot, filter_item)?)
        } else {
            None
        };
        if burden != 0 && filter_stack.is_none() && !mounted_filter {
            return Err(
                "This contaminated apparatus needs a physical filter cloth during cleaning.".into(),
            );
        }
        let empty_bucket = self
            .reg
            .item_id("base:bucket")
            .ok_or("The reusable empty bucket is unavailable.")?;
        if inventory.add(&self.reg, empty_bucket, 1) != 0 {
            return Err("Make room for the reusable empty bucket before cleaning.".into());
        }
        let definition = batch_snapshot
            .as_ref()
            .and_then(|batch| self.reg.preparations.get(&batch.preparation_id));
        let mut residue = apparatus_residue;
        if let Some(batch) = &batch_snapshot {
            for ingredient in &batch.ingredients {
                add_materials(&mut residue, &ingredient.retained_materials)?;
                add_materials(&mut residue, &ingredient.residue_materials)?;
            }
        }
        let mut consumed_materials = residue.clone();
        add_materials(&mut consumed_materials, &mounted_filter_materials)?;
        if let Some(stack) = filter_stack {
            add_materials(
                &mut consumed_materials,
                &crate::materials::stack_materials(&self.reg, stack),
            )?;
        }
        let residue_name = definition
            .map(|definition| definition.residue_item.clone())
            .unwrap_or_else(|| "base:spent_mash".into());
        let residue_count = if batch_snapshot.is_some() || !residue.is_empty() {
            definition.map_or(1, |definition| definition.residue_count)
        } else {
            0
        };
        if residue_count != 0 {
            let residue_item = self
                .reg
                .item_id(&residue_name)
                .ok_or("The declared residue item is unavailable.")?;
            if inventory.add(&self.reg, residue_item, u32::from(residue_count)) != 0 {
                return Err("Make room for the captured physical residue before cleaning.".into());
            }
        }
        let spent_filter = filter_stack.is_some() || mounted_filter;
        let captured_dross = if burden == 0 {
            Current::default()
        } else {
            let current = self
                .arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.account(&ArcaneOwner::AlchemyDross(filter_owner_id)))
                .map(|account| account.current.clone())
                .ok_or("The mounted filter burden has no finite dross custody.")?;
            if current.total() != burden {
                return Err("The mounted filter's burden and dross ledger disagree.".into());
            }
            current
        };
        let spent_filter_output = if spent_filter {
            let spent_id = if captured_dross.is_empty() {
                0
            } else {
                self.arcane_ledger
                    .as_mut()
                    .ok_or("The finite Current ledger is unavailable.")?
                    .allocate_item_id()
                    .map_err(|error| error.to_string())?
            };
            let output = produced(&self.reg, "base:spent_filter", 1, spent_id)?;
            if inventory.add_stack(
                &self.reg,
                output
                    .clone()
                    .into_stack(&self.reg)
                    .map_err(|error| error.to_string())?,
            ) != 0
            {
                return Err("Make room for the hazardous spent filter before cleaning.".into());
            }
            Some(output)
        } else {
            None
        };
        // Prove the complete portable -> industrial -> runoff exchange using
        // only exact parcel arithmetic. Cloning PlanetaryWeather here copied
        // every climate and hydrology cell for one bucket and was a major
        // laboratory hitch on live planets.
        let atlas_pos = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("Cleaning needs the authoritative planet atlas.")?;
        let residue_water = batch_snapshot
            .as_ref()
            .map_or_else(ReservoirMass::default, |batch| batch.residue_water);
        let cleaning_water = self
            .planetary_weather
            .as_ref()
            .ok_or("Cleaning needs the authoritative planetary water cycle.")?
            .preview_portable_exchange_to_runoff(
                atlas_pos,
                WaterClass::Fresh,
                HYDRO_UNITS_PER_BLOCK,
                residue_water,
            )
            .ok_or("The exact cleaning water and residue cannot enter local runoff.")?;
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let now = self.alchemy_tick();
        let apparatus = state
            .apparatus
            .get_mut(&pos)
            .ok_or("The apparatus disappeared.")?;
        apparatus.batch = None;
        apparatus.residue_materials.clear();
        apparatus.filter_burden = 0;
        apparatus.filter_medium = None;
        apparatus.filter_owner_id = 0;
        apparatus.filter_medium_materials.clear();
        apparatus.cleanliness_permille = 1_000;
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id: batch_snapshot.as_ref().map_or(0, |batch| batch.id),
            actor: request.actor,
            action: "clean".into(),
            preparation_id: batch_snapshot.as_ref().map_or_else(
                || "base:apparatus_cleaning".into(),
                |batch| batch.preparation_id.clone(),
            ),
            volume_units: cleaning_water.water_hu.saturating_add(
                batch_snapshot
                    .as_ref()
                    .map_or(0, |batch| batch.residue_water.water_hu),
            ),
            current_units: 0,
            dross_units: burden,
            tick: now,
            note: "captured residue and spent media; wastewater entered runoff".into(),
        });
        state.validate().map_err(|error| error.to_string())?;
        if !captured_dross.is_empty() {
            let output_id = spent_filter_output
                .as_ref()
                .map(|output| output.arcane_id)
                .filter(|id| *id != 0)
                .ok_or("The captured filter burden has no physical spent-filter identity.")?;
            let staged_material = if consumed_materials.is_empty() {
                None
            } else {
                self.material_ledger
                    .as_ref()
                    .ok_or("The finite material ledger is unavailable.")?
                    .stage_linked_consumption(&consumed_materials)
                    .map_err(|error| error.to_string())?
            };
            self.commit_alchemy_current_with_material(
                state.clone(),
                operation_id,
                "base:spent_filter",
                "cleaning transferred captured dross into one physical spent filter",
                vec![(
                    ArcaneOwner::AlchemyDross(filter_owner_id),
                    captured_dross.clone(),
                )],
                vec![(
                    ArcaneOwner::ItemDross(output_id),
                    captured_dross,
                    Some("base:spent_filter".into()),
                )],
                staged_material,
            )?;
        } else if !consumed_materials.is_empty() {
            self.material_ledger
                .as_mut()
                .ok_or("The finite material ledger is unavailable.")?
                .record_consumption(&consumed_materials)
                .map_err(|error| error.to_string())?;
        }
        let moved = self
            .planetary_weather
            .as_mut()
            .and_then(|weather| {
                weather.portable_exchange_to_runoff(
                    atlas_pos,
                    WaterClass::Fresh,
                    HYDRO_UNITS_PER_BLOCK,
                    residue_water,
                )
            })
            .ok_or("Preflighted cleaning-water exchange unexpectedly failed.")?;
        debug_assert_eq!(moved, cleaning_water);
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Clean,
            "The apparatus is clean; residue, spent media, glass, and wastewater all remain physical.",
            if residue_count != 0 {
                Some(produced(
                    &self.reg,
                    &residue_name,
                    u32::from(residue_count),
                    0,
                )?)
            } else {
                spent_filter_output
            },
        )
    }

    fn alchemy_drain(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        route: DisposalRoute,
        state: &mut crate::alchemy::AlchemyState,
    ) -> Result<AlchemyResult, String> {
        if route == DisposalRoute::SealedWaste {
            return Err("Sealed disposal needs a physical filter and cleaning action; it is not a delete button.".into());
        }
        let apparatus = state
            .apparatus
            .get(&pos)
            .ok_or("The apparatus is not installed.")?;
        let batch = apparatus
            .batch
            .as_ref()
            .ok_or("There is no batch to drain.")?
            .clone();
        let installation_id = apparatus.installation_id;
        let clean_owner = ArcaneOwner::Alchemy(crate::alchemy::batch_owner_id(batch.id));
        let dross_owner = ArcaneOwner::AlchemyDross(crate::alchemy::batch_owner_id(batch.id));
        let clean = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&clean_owner))
            .map_or_else(Current::default, |account| account.current.clone());
        let dross = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&dross_owner))
            .map_or_else(Current::default, |account| account.current.clone());
        let mut environmental = clean.clone();
        environmental
            .checked_add(&dross)
            .map_err(|error| error.to_string())?;
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let now = self.alchemy_tick();
        let wastewater = batch
            .liquid
            .water
            .checked_add(batch.residue_water)
            .ok_or("Batch wastewater custody overflowed.")?;
        let atlas_pos = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()));
        self.preflight_industrial_water_return(
            pos,
            wastewater,
            !matches!(route, DisposalRoute::Soil),
        )?;
        let mut materials = MaterialVector::new();
        for ingredient in &batch.ingredients {
            add_materials(&mut materials, &ingredient.retained_materials)?;
            add_materials(&mut materials, &ingredient.residue_materials)?;
        }
        let pollution = state.pollution.entry(pos).or_default();
        pollution.water = pollution
            .water
            .checked_add(wastewater)
            .ok_or("Local wastewater custody overflowed.")?;
        if let Some(carrier) = batch.liquid.carrier {
            let stored = pollution.carrier_units.entry(carrier).or_default();
            *stored = stored
                .checked_add(batch.liquid.volume_units)
                .ok_or("Local carrier pollution overflowed.")?;
        }
        add_materials(&mut pollution.materials, &materials)?;
        add_materials(&mut pollution.solutes, &batch.liquid.solutes)?;
        pollution
            .dross
            .checked_add(&environmental)
            .map_err(|error| error.to_string())?;
        pollution.last_actor = request.actor;
        pollution.last_operation_id = operation_id;
        pollution.updated_tick = now;
        let apparatus = state
            .apparatus
            .get_mut(&pos)
            .ok_or("The apparatus disappeared.")?;
        apparatus.batch = None;
        // Installed/spent filter media stays in the stand. Dumping the liquid
        // is not a way to delete its hazardous burden or physical material.
        apparatus.cleanliness_permille = apparatus.cleanliness_permille.saturating_sub(250);
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id: batch.id,
            actor: request.actor,
            action: format!("drain_{route:?}").to_lowercase(),
            preparation_id: batch.preparation_id.clone(),
            volume_units: batch.liquid.volume_units,
            current_units: clean.total(),
            dross_units: dross.total(),
            tick: now,
            note: "harmful dumping retained exact local custody and attribution".into(),
        });
        if !environmental.is_empty() {
            let region =
                atlas_pos.ok_or("Environmental dross settlement needs the planet atlas.")?;
            let medium = match route {
                DisposalRoute::Soil => crate::arcane::DrossMedium::Soil,
                DisposalRoute::Runoff => crate::arcane::DrossMedium::Water,
                DisposalRoute::Air => crate::arcane::DrossMedium::Air,
                DisposalRoute::SealedWaste => unreachable!(),
            };
            let mut debits = Vec::new();
            if !clean.is_empty() {
                debits.push((clean_owner, clean));
            }
            if !dross.is_empty() {
                debits.push((dross_owner, dross));
            }
            self.commit_alchemy_current(
                state.clone(),
                operation_id,
                &batch.preparation_id,
                "dumped alchemy became attributed local dross instead of disappearing",
                debits,
                vec![(ArcaneOwner::Dross { region, medium }, environmental, None)],
            )?;
        } else {
            self.persist_alchemy_state(state.clone())?;
        }
        self.apply_industrial_water_return(pos, wastewater, !matches!(route, DisposalRoute::Soil))?;
        if let Some(ledger) = &mut self.material_ledger {
            ledger
                .bury_materials(pos, &materials, "alchemy batch dumped into local pollution")
                .map_err(|error| error.to_string())?;
        }
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Leak,
            "The batch is gone from the vessel because its water, chemicals, material, and dross now pollute the selected local medium.",
            None,
        )
    }

    fn alchemy_dismantle(
        &mut self,
        pos: BlockPos,
        _request: &AlchemyRequest,
        _route: DisposalRoute,
        state: &mut crate::alchemy::AlchemyState,
    ) -> Result<AlchemyResult, String> {
        let apparatus = state
            .apparatus
            .get(&pos)
            .ok_or("The apparatus is not installed.")?;
        if apparatus.batch.is_some()
            || state.ordinary_jobs.contains_key(&pos)
            || !apparatus.residue_materials.is_empty()
            || apparatus.filter_burden != 0
            || apparatus.filter_medium.is_some()
        {
            return Err("Drain and clean every batch, ordinary process, residue, and spent filter before dismantling.".into());
        }
        let apparatus = state
            .apparatus
            .remove(&pos)
            .expect("validated apparatus remained installed");
        Ok(AlchemyResult {
            installation_id: apparatus.installation_id,
            batch_id: 0,
            revision: apparatus.revision.saturating_add(1),
            preparation_id: None,
            outcome: None,
            volume_units: 0,
            doses_remaining: 0,
            temperature_millic: apparatus.temperature_millic,
            cleanliness_permille: apparatus.cleanliness_permille,
            next_step: None,
            cue: AlchemyCue {
                pos,
                installation_id: apparatus.installation_id,
                batch_id: 0,
                revision: apparatus.revision.saturating_add(1),
                kind: AlchemyCueKind::Clean,
                intensity: 48,
                color: [170, 170, 160],
                message: "The empty apparatus is released to ordinary block dismantling.".into(),
            },
            produced: None,
        })
    }

    fn alchemy_ferment(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        slots: [usize; 3],
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        self.ordinary_carrier_job(
            pos,
            request,
            slots,
            crate::alchemy::OrdinaryProcessKind::FermentAlcohol,
            state,
            inventory,
        )
    }

    fn alchemy_press_oil(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        seed_slot: usize,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        self.ordinary_carrier_job(
            pos,
            request,
            [seed_slot, seed_slot, seed_slot],
            crate::alchemy::OrdinaryProcessKind::PressOil,
            state,
            inventory,
        )
    }

    fn ordinary_carrier_job(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        slots: [usize; 3],
        kind: crate::alchemy::OrdinaryProcessKind,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        let now = self.alchemy_tick();
        let apparatus = state
            .apparatus
            .get(&pos)
            .ok_or("The apparatus is not installed.")?;
        let required = match kind {
            crate::alchemy::OrdinaryProcessKind::FermentAlcohol => ApparatusKind::InfusionBasin,
            crate::alchemy::OrdinaryProcessKind::PressOil => ApparatusKind::Mortar,
        };
        if apparatus.kind != required || apparatus.batch.is_some() {
            return Err("That ordinary carrier process needs its empty declared apparatus.".into());
        }
        if let Some(job) = state.ordinary_jobs.get(&pos).cloned() {
            if job.kind != kind {
                return Err(
                    "A different ordinary carrier process already occupies this apparatus.".into(),
                );
            }
            if now < job.due_tick {
                return Err(format!(
                    "The ordinary process still needs {} ticks.",
                    job.due_tick - now
                ));
            }
            self.preflight_industrial_water_return(pos, job.process_water, false)?;
            let item = self
                .reg
                .item_id(&job.output_item)
                .ok_or("The ordinary carrier output is unavailable.")?;
            if inventory.add(&self.reg, item, u32::from(job.output_count)) != 0 {
                return Err("Make inventory room before collecting the carrier output.".into());
            }
            state.ordinary_jobs.remove(&pos);
            let apparatus = state.apparatus.get_mut(&pos).expect("apparatus existed");
            apparatus.cleanliness_permille = apparatus.cleanliness_permille.saturating_sub(40);
            apparatus.integrity_permille = apparatus.integrity_permille.saturating_sub(2);
            apparatus.revision = apparatus.revision.saturating_add(1);
            apparatus.last_operator = request.actor;
            let result = result_for(
                &self.reg,
                state,
                pos,
                AlchemyCueKind::Pour,
                "The timed ordinary process returns a measured finite carrier; no magical yield bonus applies.",
                Some(produced(
                    &self.reg,
                    &job.output_item,
                    u32::from(job.output_count),
                    0,
                )?),
            )?;
            state.validate().map_err(|error| error.to_string())?;
            if !job.input_materials.is_empty() {
                self.material_ledger
                    .as_mut()
                    .ok_or("The finite material ledger is unavailable.")?
                    .record_consumption(&job.input_materials)
                    .map_err(|error| error.to_string())?;
            }
            self.apply_industrial_water_return(pos, job.process_water, false)?;
            return Ok(result);
        }
        let mut input_materials = MaterialVector::new();
        let (process_water, portable_start) = match kind {
            crate::alchemy::OrdinaryProcessKind::FermentAlcohol => {
                let water_item = self
                    .reg
                    .item_id("base:bucket_water")
                    .ok_or("Fresh water is unavailable.")?;
                let wheat_item = self
                    .reg
                    .item_id("base:wheat")
                    .ok_or("Wheat is unavailable.")?;
                let berry_item = self
                    .reg
                    .item_id("base:berry")
                    .ok_or("Berries are unavailable.")?;
                let water = take_exact_slot(inventory, slots[0], water_item)?;
                let wheat = take_count(inventory, slots[1], wheat_item, 2)?;
                let berries = take_count(inventory, slots[2], berry_item, 2)?;
                // The water stack's ordinary matter is its reusable bucket;
                // that vessel returns below and therefore is not part of the
                // fermented feedstock material sink.
                let _ = water;
                add_materials(
                    &mut input_materials,
                    &crate::materials::stack_materials(&self.reg, wheat),
                )?;
                add_materials(
                    &mut input_materials,
                    &crate::materials::stack_materials(&self.reg, berries),
                )?;
                let bucket = self
                    .reg
                    .item_id("base:bucket")
                    .ok_or("The empty bucket is unavailable.")?;
                if inventory.add(&self.reg, bucket, 1) != 0 {
                    return Err("Make room for the reusable bucket before fermenting.".into());
                }
                let mass = self
                    .planetary_weather
                    .as_ref()
                    .ok_or("Fermentation needs the authoritative planetary water cycle.")?
                    .preview_move_portable_to_industrial(WaterClass::Fresh, HYDRO_UNITS_PER_BLOCK)
                    .ok_or("The portable-water ledger cannot fund fermentation.")?;
                (mass, Some((WaterClass::Fresh, mass)))
            }
            crate::alchemy::OrdinaryProcessKind::PressOil => {
                let seeds = self
                    .reg
                    .item_id("base:wheat_seeds")
                    .ok_or("Wheat seed is unavailable.")?;
                let stack = take_count(inventory, slots[0], seeds, 4)?;
                add_materials(
                    &mut input_materials,
                    &crate::materials::stack_materials(&self.reg, stack),
                )?;
                (ReservoirMass::default(), None)
            }
        };
        let (output_item, due_tick) = match kind {
            crate::alchemy::OrdinaryProcessKind::FermentAlcohol => {
                ("base:fermented_alcohol", now.saturating_add(1_200))
            }
            crate::alchemy::OrdinaryProcessKind::PressOil => {
                ("base:plant_oil", now.saturating_add(400))
            }
        };
        state.ordinary_jobs.insert(
            pos,
            crate::alchemy::OrdinaryProcessJob {
                kind,
                installation_id: apparatus.installation_id,
                actor: request.actor,
                started_tick: now,
                due_tick,
                output_item: output_item.into(),
                output_count: 4,
                process_water,
                input_materials,
            },
        );
        let apparatus = state.apparatus.get_mut(&pos).expect("apparatus existed");
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        let result = result_for(
            &self.reg,
            state,
            pos,
            if kind == crate::alchemy::OrdinaryProcessKind::FermentAlcohol {
                AlchemyCueKind::Bubble
            } else {
                AlchemyCueKind::Grind
            },
            "The ordinary carrier process has begun and will not finish before its measured time.",
            None,
        )?;
        state.validate().map_err(|error| error.to_string())?;
        if let Some((class, expected)) = portable_start {
            let moved = self
                .planetary_weather
                .as_mut()
                .and_then(|weather| {
                    weather.move_portable_to_industrial(class, HYDRO_UNITS_PER_BLOCK)
                })
                .ok_or("Preflighted fermentation water unexpectedly failed to move.")?;
            debug_assert_eq!(moved, expected);
        }
        Ok(result)
    }
}

fn take_count(
    inventory: &mut Inventory,
    slot: usize,
    expected_item: crate::registry::ItemId,
    count: u32,
) -> Result<ItemStack, String> {
    let stack = inventory
        .slots
        .get(slot)
        .copied()
        .flatten()
        .ok_or("That authoritative inventory slot is empty.")?;
    if stack.item != expected_item || stack.count < count || stack.arcane_id != 0 {
        return Err("That slot cannot fund the measured ordinary input count.".into());
    }
    let taken = ItemStack { count, ..stack };
    let remaining = stack.count - count;
    inventory.slots[slot] = (remaining != 0).then_some(ItemStack {
        count: remaining,
        ..stack
    });
    Ok(taken)
}

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
                let (target_owner, target_pos) = match target {
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
                        (ArcaneOwner::ItemDross(item_id), actor_pos)
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
                let captured = available_work
                    .take_units(
                        u64::from(definition.effect.dross_capacity).min(available_work.total()),
                        [definition.resonance.clone()],
                    )
                    .map_err(|error| error.to_string())?;
                if captured.is_empty() && dose_dross.is_empty() {
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
                let mut sludge_current = dose_dross.clone();
                sludge_current
                    .checked_add(&captured)
                    .map_err(|error| error.to_string())?;
                if !captured.is_empty() {
                    debits.push((target_owner, captured));
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
                byproduct = Some(sludge);
            }
        }
        // Retained ingredient matter leaves the circulating preparation at
        // application time: metabolism, soil uptake, a coating film, or
        // captured wash waste is an explicit material sink rather than an
        // untracked disappearance. Stage that debit beside the state/Current
        // replacement so an I/O refusal cannot consume only half the dose.
        // Reusable-vessel matter is not debited; it returns in `empty` below.
        let staged_material = if dose.materials.is_empty() {
            None
        } else {
            self.material_ledger
                .as_ref()
                .ok_or("The finite material ledger is unavailable.")?
                .stage_linked_consumption(&dose.materials)
                .map_err(|error| error.to_string())?
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
        self.commit_alchemy_current_with_material(
            state.clone(),
            operation_id,
            &definition.id,
            "authoritative preparation application transferred exact dose Current and matter",
            debits,
            credits,
            staged_material,
        )?;
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

    fn return_preparation_water(
        &mut self,
        pos: BlockPos,
        dose: &crate::alchemy::PreparationDose,
        runoff: bool,
    ) -> Result<(), String> {
        self.apply_industrial_water_return(pos, dose.liquid.water, runoff)
    }

    fn preflight_preparation_water_return(
        &self,
        pos: BlockPos,
        dose: &crate::alchemy::PreparationDose,
        runoff: bool,
    ) -> Result<(), String> {
        self.preflight_industrial_water_return(pos, dose.liquid.water, runoff)
    }

    fn preflight_industrial_water_return(
        &self,
        pos: BlockPos,
        mass: ReservoirMass,
        runoff: bool,
    ) -> Result<(), String> {
        if mass.water_hu == 0 && mass.salt_mass == 0 {
            return Ok(());
        }
        let (Some(atlas), Some(weather)) = (&self.planet_atlas, &self.planetary_weather) else {
            return Err("Alchemy water needs the authoritative atlas and water cycle.".into());
        };
        let region = atlas.atlas_pos(pos.surface());
        let possible = if runoff {
            weather.can_return_industrial_exact_to_runoff(region, mass)
        } else {
            weather.can_return_industrial_exact_to_soil(region, mass)
        };
        possible.then_some(()).ok_or_else(|| {
            "The exact alchemy water parcel cannot enter its local reservoir.".into()
        })
    }

    fn apply_industrial_water_return(
        &mut self,
        pos: BlockPos,
        mass: ReservoirMass,
        runoff: bool,
    ) -> Result<(), String> {
        if mass.water_hu == 0 && mass.salt_mass == 0 {
            return Ok(());
        }
        let (Some(atlas), Some(weather)) = (&self.planet_atlas, &mut self.planetary_weather) else {
            return Err("Alchemy water needs the authoritative atlas and water cycle.".into());
        };
        let region = atlas.atlas_pos(pos.surface());
        let returned = if runoff {
            weather.return_industrial_exact_to_runoff(region, mass)
        } else {
            weather.return_industrial_exact_to_soil(region, mass)
        };
        returned
            .then_some(())
            .ok_or_else(|| "Preflighted alchemy water settlement unexpectedly failed.".into())
    }
}

fn incompatible_status_groups(left: &str, right: &str) -> bool {
    matches!(
        (left, right),
        ("base:strain_control", "base:wand_throughput")
            | ("base:wand_throughput", "base:strain_control")
    )
}

fn settle_immediate_current(
    credits: &mut Vec<(ArcaneOwner, Current, Option<String>)>,
    region: Option<crate::planet_atlas::AtlasPos>,
    clean: Current,
    dross: Current,
    medium: crate::arcane::DrossMedium,
) -> Result<(), String> {
    let region = region.ok_or("Preparation Current settlement needs the authoritative atlas.")?;
    if !clean.is_empty() {
        credits.push((ArcaneOwner::Ambient(region), clean, None));
    }
    if !dross.is_empty() {
        credits.push((ArcaneOwner::Dross { region, medium }, dross, None));
    }
    Ok(())
}

fn preparation_color(handler: PreparationHandler) -> [u8; 3] {
    match handler {
        PreparationHandler::TraceSight => [130, 190, 255],
        PreparationHandler::NaturalRecovery | PreparationHandler::RootUptake => [110, 210, 100],
        PreparationHandler::StrainRelief | PreparationHandler::PreserveSpecimen => [170, 210, 255],
        PreparationHandler::DrossWash | PreparationHandler::DrossAntidote => [170, 120, 210],
        PreparationHandler::ThroughputSurge => [120, 180, 255],
    }
}

impl World {
    /// Read the bounded modifiers currently attached to one authoritative
    /// actor. This exposes only approved handler outputs; callers never see
    /// or reinterpret a preparation's private mixture.
    pub fn preparation_modifiers(&self, actor: [u8; 16]) -> PreparationModifiers {
        let now = self.alchemy_tick();
        let mut modifiers = PreparationModifiers::default();
        let Some(state) = &self.alchemy_state else {
            return modifiers;
        };
        for status in state.statuses.get(&actor).into_iter().flatten() {
            if now >= status.due_tick {
                continue;
            }
            let Some(definition) = self.reg.preparations.get(&status.preparation_id) else {
                continue;
            };
            match definition.handler {
                PreparationHandler::TraceSight => {
                    modifiers.trace_sight = modifiers
                        .trace_sight
                        .max(definition.effect.strength.min(u32::from(u16::MAX)) as u16);
                }
                PreparationHandler::StrainRelief => {
                    modifiers.strain_permille = modifiers
                        .strain_permille
                        .min(1_000u16.saturating_sub(definition.effect.strength.min(750) as u16));
                    modifiers.throughput_permille = modifiers
                        .throughput_permille
                        .min(definition.effect.throughput_permille);
                }
                PreparationHandler::ThroughputSurge => {
                    modifiers.throughput_permille = modifiers
                        .throughput_permille
                        .max(definition.effect.throughput_permille);
                    modifiers.drain_permille = modifiers
                        .drain_permille
                        .max(definition.effect.drain_permille);
                    modifiers.overdraw_permille = modifiers
                        .overdraw_permille
                        .max(definition.effect.overdraw_permille);
                    modifiers.storm_warning = true;
                }
                PreparationHandler::NaturalRecovery
                | PreparationHandler::RootUptake
                | PreparationHandler::DrossWash
                | PreparationHandler::PreserveSpecimen
                | PreparationHandler::DrossAntidote => {}
            }
        }
        modifiers
    }

    /// Bounded Root Wash uptake factor for an already viable plot. All
    /// ordinary light, season, temperature, moisture, soil, seed, and nutrient
    /// gates still run; the wash cannot create growth inputs or set age.
    pub fn root_uptake_multiplier_at(&self, plant: BlockPos) -> f32 {
        let now = self.alchemy_tick();
        let Some(state) = &self.alchemy_state else {
            return 1.0;
        };
        let treatment = state.root_treatments.get(&plant).or_else(|| {
            plant
                .offset(0, -1, 0)
                .and_then(|soil| state.root_treatments.get(&soil))
        });
        let Some(treatment) = treatment.filter(|treatment| now < treatment.expires_tick) else {
            return 1.0;
        };
        if treatment.concentration_permille > 1_000 {
            return (2_000u16.saturating_sub(treatment.concentration_permille) as f32 / 1_000.0)
                .clamp(0.1, 1.0);
        }
        (1.0 + f32::from(treatment.uptake_permille) / 1_000.0).clamp(1.0, 2.0)
    }

    /// Advance one actor's authoritative timed effects. Local and remote
    /// callers pass the same physiological snapshot and receive the same
    /// bounded result; no client computes healing, vision, or wand bonuses.
    pub fn tick_preparation_statuses(
        &mut self,
        actor: [u8; 16],
        actor_pos: BlockPos,
        mut physiology: PreparationPhysiology,
    ) -> Result<PreparationTickResult, String> {
        // Atlas-free fixture/remote worlds do not own planetary water,
        // Current, or an alchemy sidecar. Their ordinary survival tick is a
        // legitimate no-op here, not a once-per-second error. A qualified
        // atlas-backed world missing its sidecar still fails closed below.
        if self.alchemy_state.is_none() && self.planet_atlas.is_none() {
            return Ok(PreparationTickResult {
                physiology,
                modifiers: PreparationModifiers::default(),
                cues: Vec::new(),
            });
        }
        let now = self.alchemy_tick();
        // Validate every definition reference before touching the first
        // status. A corrupt later entry must not leave earlier physiology or
        // progress partially advanced on the direct (non-settlement) path.
        let authoritative_state = self
            .alchemy_state
            .as_ref()
            .ok_or("The authoritative alchemy state is unavailable.")?;
        if let Some(statuses) = authoritative_state.statuses.get(&actor) {
            for status in statuses {
                let definition = self
                    .reg
                    .preparations
                    .get(&status.preparation_id)
                    .ok_or("An active preparation definition is unavailable.")?;
                if definition.version != status.definition_version {
                    return Err(
                        "An active preparation definition version changed underneath its status."
                            .into(),
                    );
                }
            }
        }
        // Most actor ticks only advance bounded physiology and modifiers. Do
        // that in place so one active draught does not clone the entire world
        // census every second. Expiry is rarer and moves Current through the
        // linked ledger, so retain the old copy-on-commit transaction for that
        // path: a failed ledger write then leaves both custody records intact.
        let needs_staged_settlement = self
            .alchemy_state
            .as_ref()
            .and_then(|state| state.statuses.get(&actor))
            .is_some_and(|statuses| {
                statuses.iter().any(|status| {
                    now >= status.due_tick
                        && (!status.active_current.is_empty() || !status.dross_current.is_empty())
                })
            });
        let mut staged_state = needs_staged_settlement.then(|| {
            self.alchemy_state
                .clone()
                .expect("settlement preflight observed authoritative alchemy state")
        });
        let state = if let Some(state) = staged_state.as_mut() {
            state
        } else {
            self.alchemy_state
                .as_mut()
                .ok_or("The authoritative alchemy state is unavailable.")?
        };
        let mut modifiers = PreparationModifiers::default();
        let mut cues = Vec::new();
        let mut settlements = Vec::<(ArcaneOwner, Current, ArcaneOwner, Current, String)>::new();
        if let Some(statuses) = state.statuses.get_mut(&actor) {
            for status in statuses.iter_mut() {
                let definition = self
                    .reg
                    .preparations
                    .get(&status.preparation_id)
                    .expect("status definitions were preflight before mutation");
                debug_assert_eq!(definition.version, status.definition_version);
                let from_tick = status.last_tick.max(status.started_tick).min(now);
                let active_until = now.min(status.due_tick);
                if now < status.due_tick {
                    match definition.handler {
                        PreparationHandler::TraceSight => {
                            modifiers.trace_sight = modifiers
                                .trace_sight
                                .max(definition.effect.strength.min(u32::from(u16::MAX)) as u16);
                        }
                        PreparationHandler::NaturalRecovery => {
                            let cap_milli = u64::from(definition.effect.strength) * 1_000;
                            let span = status.due_tick.saturating_sub(status.started_tick).max(1);
                            let elapsed = active_until.saturating_sub(status.started_tick);
                            let desired = u64::try_from(
                                u128::from(cap_milli) * u128::from(elapsed) / u128::from(span),
                            )
                            .unwrap_or(cap_milli)
                            .min(cap_milli);
                            let wanted = desired.saturating_sub(status.completed_units);
                            let health_room_milli =
                                ((physiology.max_health - physiology.health).max(0.0) * 1_000.0)
                                    .floor() as u64;
                            let starving_room = if physiology.hunger <= 0.01 {
                                ((2.0 - physiology.health).max(0.0) * 1_000.0).floor() as u64
                            } else {
                                u64::MAX
                            };
                            let hunger_per_heal = if cap_milli == 0 {
                                0.0
                            } else {
                                f64::from(definition.effect.hunger_cost_milli)
                                    / cap_milli as f64
                                    / 1_000.0
                            };
                            let hunger_room = if hunger_per_heal == 0.0 {
                                u64::MAX
                            } else {
                                (f64::from(physiology.hunger.max(0.0)) / hunger_per_heal).floor()
                                    as u64
                            };
                            let nutrition_total = physiology
                                .nutrition
                                .iter()
                                .copied()
                                .map(|value| value.max(0.0))
                                .sum::<f32>();
                            let nutrient_per_heal = if cap_milli == 0 {
                                0.0
                            } else {
                                f64::from(definition.effect.nutrient_cost) / cap_milli as f64
                            };
                            let nutrient_room = if nutrient_per_heal == 0.0 {
                                u64::MAX
                            } else {
                                (f64::from(nutrition_total) / nutrient_per_heal).floor() as u64
                            };
                            let healed_milli = wanted
                                .min(health_room_milli)
                                .min(starving_room)
                                .min(hunger_room)
                                .min(nutrient_room);
                            if healed_milli != 0 {
                                physiology.health = (physiology.health
                                    + healed_milli as f32 / 1_000.0)
                                    .min(physiology.max_health);
                                physiology.hunger = (physiology.hunger
                                    - (healed_milli as f64 * hunger_per_heal) as f32)
                                    .max(0.0);
                                debit_nutrition(
                                    &mut physiology.nutrition,
                                    (healed_milli as f64 * nutrient_per_heal) as f32,
                                );
                                status.completed_units = status
                                    .completed_units
                                    .saturating_add(healed_milli)
                                    .min(cap_milli);
                            }
                            let sickness_until = active_until.min(status.overdose_until_tick);
                            let sickness_from = from_tick.min(sickness_until);
                            let sickness_elapsed = sickness_until.saturating_sub(sickness_from);
                            if sickness_elapsed != 0 {
                                physiology.hunger = (physiology.hunger
                                    - sickness_elapsed.min(600) as f32 / 600.0 * 0.5)
                                    .max(0.0);
                            }
                        }
                        PreparationHandler::StrainRelief => {
                            modifiers.strain_permille = modifiers.strain_permille.min(
                                1_000u16.saturating_sub(definition.effect.strength.min(750) as u16),
                            );
                            modifiers.throughput_permille = modifiers
                                .throughput_permille
                                .min(definition.effect.throughput_permille);
                        }
                        PreparationHandler::ThroughputSurge => {
                            modifiers.throughput_permille = modifiers
                                .throughput_permille
                                .max(definition.effect.throughput_permille);
                            modifiers.drain_permille = modifiers
                                .drain_permille
                                .max(definition.effect.drain_permille);
                            modifiers.overdraw_permille = modifiers
                                .overdraw_permille
                                .max(definition.effect.overdraw_permille);
                            modifiers.storm_warning = true;
                            if from_tick / 40 != active_until / 40 {
                                cues.push(AlchemyCue {
                                    pos: actor_pos,
                                    installation_id: 0,
                                    batch_id: status.source_batch,
                                    revision: status.status_id,
                                    kind: AlchemyCueKind::Pulse,
                                    intensity: 210,
                                    color: [110, 180, 255],
                                    message: "Storm cordial pulses: throughput, drain, and overdraw are all elevated.".into(),
                                });
                            }
                        }
                        PreparationHandler::DrossAntidote => {
                            let capacity = u64::from(definition.effect.dross_capacity);
                            let span = status.due_tick.saturating_sub(status.started_tick).max(1);
                            let elapsed = active_until.saturating_sub(status.started_tick);
                            let desired = u64::try_from(
                                u128::from(capacity) * u128::from(elapsed) / u128::from(span),
                            )
                            .unwrap_or(capacity)
                            .min(capacity);
                            let moved = desired
                                .saturating_sub(status.completed_units)
                                .min(physiology.bodily_dross);
                            physiology.bodily_dross -= moved;
                            status.completed_units = status.completed_units.saturating_add(moved);
                        }
                        PreparationHandler::RootUptake
                        | PreparationHandler::DrossWash
                        | PreparationHandler::PreserveSpecimen => {}
                    }
                }
                status.last_tick = now.min(status.recovery_until_tick);
                if now >= status.due_tick
                    && (!status.active_current.is_empty() || !status.dross_current.is_empty())
                {
                    let Some(atlas) = self.planet_atlas.as_ref() else {
                        return Err(
                            "Status settlement needs the authoritative planet atlas.".into()
                        );
                    };
                    let region = atlas.atlas_pos(actor_pos.surface());
                    let owner_id = crate::alchemy::status_owner_id(status.status_id);
                    settlements.push((
                        ArcaneOwner::Alchemy(owner_id),
                        std::mem::take(&mut status.active_current),
                        ArcaneOwner::AlchemyDross(owner_id),
                        std::mem::take(&mut status.dross_current),
                        status.preparation_id.clone(),
                    ));
                    let _ = region;
                }
            }
            statuses.retain(|status| {
                now < status.recovery_until_tick
                    || !status.active_current.is_empty()
                    || !status.dross_current.is_empty()
            });
        }
        let operation_id = if settlements.is_empty() {
            None
        } else {
            Some(
                state
                    .allocate_operation_id()
                    .map_err(|error| error.to_string())?,
            )
        };
        let result = PreparationTickResult {
            physiology,
            modifiers,
            cues,
        };
        if let Some(operation_id) = operation_id {
            let atlas = self
                .planet_atlas
                .as_ref()
                .ok_or("Status settlement needs the authoritative planet atlas.")?;
            let region = atlas.atlas_pos(actor_pos.surface());
            let mut debits = Vec::new();
            let mut credit_map = BTreeMap::<ArcaneOwner, Current>::new();
            for (clean_owner, clean, dross_owner, dross, _) in settlements {
                if !clean.is_empty() {
                    debits.push((clean_owner, clean.clone()));
                    add_current_map(&mut credit_map, ArcaneOwner::Ambient(region), &clean)?;
                }
                if !dross.is_empty() {
                    debits.push((dross_owner, dross.clone()));
                    add_current_map(
                        &mut credit_map,
                        ArcaneOwner::Dross {
                            region,
                            medium: crate::arcane::DrossMedium::Water,
                        },
                        &dross,
                    )?;
                }
            }
            self.commit_alchemy_current(
                staged_state
                    .take()
                    .expect("Current settlement was staged on a private state copy"),
                operation_id,
                "base:preparation_status_settlement",
                "completed preparation statuses returned clean Current and retained dross",
                debits,
                credit_map
                    .into_iter()
                    .map(|(owner, current)| (owner, current, None))
                    .collect(),
            )?;
        }
        Ok(result)
    }

    /// Apply Frostlace's bounded rate to one elapsed-age decrement. Returning
    /// a smaller positive decrement can slow age; it can never return a
    /// negative value or add freshness.
    pub fn coated_specimen_age_advance(
        &mut self,
        item_id: u64,
        ordinary_ticks: u64,
        temperature_millic: i32,
    ) -> u64 {
        if ordinary_ticks == 0 {
            return 0;
        }
        let now = self.alchemy_tick();
        let Some(state) = &mut self.alchemy_state else {
            return ordinary_ticks;
        };
        let Some(coating) = state.coatings.get_mut(&item_id) else {
            return ordinary_ticks;
        };
        if now >= coating.expires_tick || temperature_millic > coating.maximum_temperature_millic {
            return ordinary_ticks;
        }
        let advanced = u64::try_from(
            u128::from(ordinary_ticks) * u128::from(coating.preservation_permille) / 1_000,
        )
        .unwrap_or(ordinary_ticks)
        .max(1)
        .min(ordinary_ticks);
        coating.age_paid = coating.age_paid.saturating_add(ordinary_ticks - advanced);
        advanced
    }

    /// Bounded round-robin background maintenance: spoilage, deterministic
    /// cracked/overheated apparatus leaks, root-treatment expiry, and coating
    /// Current settlement. Persisted cursors ensure a large population cannot
    /// starve entries that sort after the first budget-sized prefix.
    pub fn tick_alchemy(&mut self, budget: usize) -> Result<Vec<AlchemyCue>, String> {
        if self.alchemy_state.is_none() && self.planet_atlas.is_none() {
            return Ok(Vec::new());
        }
        let now = self.alchemy_tick();
        let state = self
            .alchemy_state
            .as_mut()
            .ok_or("The authoritative alchemy state is unavailable.")?;
        let mut cues = Vec::new();
        let budget = budget.max(1);
        let phase = state.maintenance_phase;
        state.maintenance_phase = (phase + 1) % 4;
        let mut leaks = Vec::<(BlockPos, DisposalRoute)>::new();
        let mut expired = Vec::new();

        match phase {
            0 => {
                let visits = budget.min(state.apparatus.len());
                for _ in 0..visits {
                    let Some(pos) =
                        next_block_key(&state.apparatus, state.maintenance_apparatus_cursor)
                    else {
                        break;
                    };
                    state.maintenance_apparatus_cursor = Some(pos);
                    let apparatus = state
                        .apparatus
                        .get_mut(&pos)
                        .expect("round-robin apparatus key remained present");
                    let mut leak_route = None;
                    if let Some(batch) = apparatus.batch.as_mut() {
                        if now >= batch.expires_tick
                            && matches!(
                                batch.outcome,
                                BatchOutcome::Processing | BatchOutcome::Ready
                            )
                        {
                            batch.outcome = BatchOutcome::Spoiled;
                            batch.revision = batch.revision.saturating_add(1);
                            apparatus.revision = apparatus.revision.saturating_add(1);
                            cues.push(AlchemyCue {
                                pos: apparatus.pos,
                                installation_id: apparatus.installation_id,
                                batch_id: batch.id,
                                revision: apparatus.revision,
                                kind: AlchemyCueKind::Spoil,
                                intensity: 120,
                                color: [120, 90, 130],
                                message: "A preparation ages into its named spent-liquor state; nothing vanishes.".into(),
                            });
                        }
                        let excessive_heat = self
                            .reg
                            .preparations
                            .get(&batch.preparation_id)
                            .is_some_and(|definition| {
                                apparatus.temperature_millic
                                    > definition.temperature_millic[1].saturating_add(20_000)
                            });
                        if apparatus.integrity_permille <= 250 || excessive_heat {
                            leak_route = Some(
                                if apparatus.kind == ApparatusKind::Alembic || excessive_heat {
                                    DisposalRoute::Air
                                } else {
                                    DisposalRoute::Runoff
                                },
                            );
                        }
                    }
                    if let Some(route) = leak_route {
                        leaks.push((pos, route));
                    }
                }
            }
            1 => {
                let visits = budget.min(state.containers.len());
                for _ in 0..visits {
                    let Some(id) =
                        next_u64_key(&state.containers, state.maintenance_container_cursor)
                    else {
                        break;
                    };
                    state.maintenance_container_cursor = id;
                    let dose = state
                        .containers
                        .get_mut(&id)
                        .expect("round-robin container key remained present");
                    if now >= dose.expires_tick && dose.outcome == BatchOutcome::Ready {
                        dose.outcome = BatchOutcome::Spoiled;
                    }
                }
            }
            2 => {
                let visits = budget.min(state.root_treatments.len());
                for _ in 0..visits {
                    let Some(pos) =
                        next_block_key(&state.root_treatments, state.maintenance_root_cursor)
                    else {
                        break;
                    };
                    state.maintenance_root_cursor = Some(pos);
                    if state
                        .root_treatments
                        .get(&pos)
                        .is_some_and(|treatment| now >= treatment.expires_tick)
                    {
                        state.root_treatments.remove(&pos);
                    }
                }
            }
            3 => {
                let visits = budget.min(state.coatings.len());
                for _ in 0..visits {
                    let Some(item_id) =
                        next_u64_key(&state.coatings, state.maintenance_coating_cursor)
                    else {
                        break;
                    };
                    state.maintenance_coating_cursor = item_id;
                    if let Some(coating) = state
                        .coatings
                        .get(&item_id)
                        .filter(|coating| now >= coating.expires_tick)
                        .cloned()
                    {
                        expired.push((item_id, coating));
                    }
                }
            }
            _ => unreachable!("validated alchemy maintenance phase"),
        }

        for (pos, route) in leaks {
            let Some(apparatus) = self
                .alchemy_state
                .as_ref()
                .and_then(|state| state.apparatus.get(&pos))
                .cloned()
            else {
                continue;
            };
            let mut empty_inventory = Inventory::new();
            let result = self.operate_alchemy(
                pos,
                &mut empty_inventory,
                AlchemyRequest {
                    actor: apparatus.last_operator,
                    actor_label: "apparatus leak".into(),
                    expected_revision: Some(apparatus.revision),
                    action: ApparatusAction::Drain { route },
                },
            )?;
            let mut cue = result.cue;
            cue.message = "A cracked or dangerously overheated apparatus leaks through an ordinary environmental disposal path; its contents remain accounted.".into();
            cues.push(cue);
        }

        if expired.is_empty() {
            return Ok(cues);
        }
        // Coating expiry is its own phase, so no leak operation can have
        // replaced this snapshot while its linked Current settlement is built.
        let mut state = self
            .alchemy_state
            .clone()
            .ok_or("The authoritative alchemy state disappeared during maintenance.")?;
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let mut debits = Vec::new();
        let mut credit_map = BTreeMap::<ArcaneOwner, Current>::new();
        for (item_id, coating) in expired {
            state.coatings.remove(&item_id);
            let Some(atlas) = self.planet_atlas.as_ref() else {
                return Err("Coating settlement needs the authoritative atlas.".into());
            };
            let region = atlas.atlas_pos(coating.applied_pos.surface());
            let owner_id = crate::alchemy::status_owner_id(coating.status_id);
            for (owner, destination) in [
                (ArcaneOwner::Alchemy(owner_id), ArcaneOwner::Ambient(region)),
                (
                    ArcaneOwner::AlchemyDross(owner_id),
                    ArcaneOwner::Dross {
                        region,
                        medium: crate::arcane::DrossMedium::Soil,
                    },
                ),
            ] {
                if let Some(current) = self
                    .arcane_ledger
                    .as_ref()
                    .and_then(|ledger| ledger.account(&owner))
                    .map(|account| account.current.clone())
                {
                    debits.push((owner, current.clone()));
                    add_current_map(&mut credit_map, destination, &current)?;
                }
            }
        }
        self.commit_alchemy_current(
            state,
            operation_id,
            "base:frostlace_suspension",
            "expired specimen coatings settled their finite Current",
            debits,
            credit_map
                .into_iter()
                .map(|(owner, current)| (owner, current, None))
                .collect(),
        )?;
        Ok(cues)
    }

    pub fn settle_preparations_on_death(
        &mut self,
        actor: [u8; 16],
        actor_pos: BlockPos,
    ) -> Result<(), String> {
        let now = self.alchemy_tick();
        if let Some(state) = &mut self.alchemy_state
            && let Some(statuses) = state.statuses.get_mut(&actor)
        {
            for status in statuses {
                status.due_tick = status.due_tick.min(now);
                status.recovery_until_tick = status.recovery_until_tick.min(now);
            }
        }
        let _ =
            self.tick_preparation_statuses(actor, actor_pos, PreparationPhysiology::default())?;
        Ok(())
    }
}

fn debit_nutrition(nutrition: &mut [f32; 5], mut amount: f32) {
    for value in nutrition.iter_mut() {
        if amount <= 0.0 {
            break;
        }
        let debit = value.max(0.0).min(amount);
        *value -= debit;
        amount -= debit;
    }
}

fn add_current_map(
    map: &mut BTreeMap<ArcaneOwner, Current>,
    owner: ArcaneOwner,
    current: &Current,
) -> Result<(), String> {
    map.entry(owner)
        .or_default()
        .checked_add(current)
        .map_err(|error| error.to_string())
}

fn next_block_key<T>(
    map: &std::collections::BTreeMap<BlockPos, T>,
    cursor: Option<BlockPos>,
) -> Option<BlockPos> {
    use std::ops::Bound::{Excluded, Unbounded};

    cursor
        .and_then(|cursor| {
            map.range((Excluded(cursor), Unbounded))
                .next()
                .map(|(key, _)| *key)
        })
        .or_else(|| map.first_key_value().map(|(key, _)| *key))
}

fn next_u64_key<T>(map: &std::collections::BTreeMap<u64, T>, cursor: u64) -> Option<u64> {
    use std::ops::Bound::{Excluded, Unbounded};

    map.range((Excluded(cursor), Unbounded))
        .next()
        .or_else(|| map.first_key_value())
        .map(|(key, _)| *key)
}

fn ensure_apparatus(
    state: &mut crate::alchemy::AlchemyState,
    pos: BlockPos,
    kind: ApparatusKind,
    actor: [u8; 16],
) -> Result<(), AlchemyError> {
    if let Some(apparatus) = state.apparatus.get(&pos) {
        if apparatus.kind != kind {
            return Err(AlchemyError::Corrupt(
                "saved apparatus kind disagrees with the world block".into(),
            ));
        }
        return Ok(());
    }
    if state.apparatus.len() >= crate::alchemy::MAX_ALCHEMY_APPARATUS {
        return Err(AlchemyError::InvalidOperation(
            "the bounded planetary apparatus census is full".into(),
        ));
    }
    let installation_id = state.allocate_installation_id()?;
    state.apparatus.insert(
        pos,
        AlchemyApparatusState {
            installation_id,
            kind,
            pos,
            revision: 1,
            integrity_permille: 1_000,
            cleanliness_permille: 1_000,
            temperature_millic: 20_000,
            agitation: AgitationKind::Still,
            batch: None,
            residue_materials: MaterialVector::new(),
            filter_burden: 0,
            filter_medium: None,
            filter_owner_id: 0,
            filter_medium_materials: MaterialVector::new(),
            last_operator: actor,
        },
    );
    Ok(())
}

fn produced(
    registry: &crate::registry::Registry,
    item_name: &str,
    count: u32,
    arcane_id: u64,
) -> Result<ProducedStack, String> {
    let item = registry
        .item_id(item_name)
        .ok_or_else(|| format!("Alchemy output {item_name} is missing from the registry."))?;
    Ok(ProducedStack {
        item_name: item_name.into(),
        count,
        durability: registry.item(item).durability,
        arcane_id,
    })
}

fn take_exact_slot(
    inventory: &mut Inventory,
    slot: usize,
    expected_item: crate::registry::ItemId,
) -> Result<ItemStack, String> {
    let stack = inventory
        .slots
        .get(slot)
        .copied()
        .flatten()
        .ok_or("That authoritative inventory slot is empty.")?;
    if stack.item != expected_item {
        return Err("That slot does not contain the required physical input.".into());
    }
    inventory
        .take_one_stack(slot)
        .ok_or("The input changed before it could be reserved.".into())
}

fn split_materials(
    materials: &MaterialVector,
    retention_permille: u16,
) -> (MaterialVector, MaterialVector) {
    let mut retained = MaterialVector::new();
    let mut residue = MaterialVector::new();
    for (name, units) in materials {
        let kept = u64::try_from(u128::from(*units) * u128::from(retention_permille) / 1_000)
            .unwrap_or(*units);
        if kept != 0 {
            retained.insert(name.clone(), kept);
        }
        if *units != kept {
            residue.insert(name.clone(), *units - kept);
        }
    }
    (retained, residue)
}

fn add_materials(into: &mut MaterialVector, from: &MaterialVector) -> Result<(), String> {
    for (name, units) in from {
        let value = into
            .get(name)
            .copied()
            .unwrap_or_default()
            .checked_add(*units)
            .ok_or("Alchemy material custody overflowed.")?;
        into.insert(name.clone(), value);
    }
    Ok(())
}

fn near(a: BlockPos, b: BlockPos) -> bool {
    if a.face() != b.face() {
        return false;
    }
    i32::from(a.u()).abs_diff(i32::from(b.u()))
        + i32::from(a.y()).abs_diff(i32::from(b.y()))
        + i32::from(a.v()).abs_diff(i32::from(b.v()))
        <= APPARATUS_REACH as u32
}

impl World {
    fn alchemy_begin(
        &self,
        pos: BlockPos,
        request: &AlchemyRequest,
        preparation_id: String,
        state: &mut crate::alchemy::AlchemyState,
    ) -> Result<AlchemyResult, String> {
        let definition = self
            .reg
            .preparations
            .get(&preparation_id)
            .ok_or("That preparation is not registered in this world.")?
            .clone();
        let required_kind = if definition.steps.first() == Some(&ProcessStep::Grind) {
            ApparatusKind::Mortar
        } else {
            definition.process.apparatus()
        };
        let now = self.alchemy_tick();
        let batch_id = state
            .allocate_batch_id()
            .map_err(|error| error.to_string())?;
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let apparatus = state
            .apparatus
            .get_mut(&pos)
            .ok_or("The apparatus disappeared before the operation.")?;
        if apparatus.kind != required_kind {
            return Err(format!(
                "{} begins at its {:?}, not this {:?}.",
                definition.label, required_kind, apparatus.kind
            ));
        }
        if apparatus.batch.is_some() || !apparatus.residue_materials.is_empty() {
            return Err("Drain and clean this apparatus before beginning another batch.".into());
        }
        if apparatus.integrity_permille < 100 {
            return Err("This apparatus is too damaged to hold a batch safely.".into());
        }
        apparatus.batch = Some(AlchemyBatch {
            id: batch_id,
            preparation_id: definition.id.clone(),
            definition_version: definition.version,
            actor: request.actor,
            actor_label: request.actor_label.clone(),
            installation_id: apparatus.installation_id,
            liquid: ExactLiquid::default(),
            residue_water: ReservoirMass::default(),
            ingredients: Vec::new(),
            charge_input_units: 0,
            current_units: 0,
            dross_units: 0,
            step_index: 0,
            observations: Vec::new(),
            started_tick: now,
            due_tick: now.saturating_add(definition.process_ticks),
            born_tick: now,
            expires_tick: now.saturating_add(definition.shelf_life_ticks),
            outcome: BatchOutcome::Processing,
            revision: 1,
        });
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        let installation_id = apparatus.installation_id;
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id,
            actor: request.actor,
            action: "begin".into(),
            preparation_id: definition.id,
            volume_units: 0,
            current_units: 0,
            dross_units: 0,
            tick: now,
            note: "physical batch identity reserved at its first apparatus".into(),
        });
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Bubble,
            "The measured batch begins; its ingredients remain physical.",
            None,
        )
    }

    fn alchemy_grind(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        slot: usize,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        let (batch_id, preparation_id, installation_id) = {
            let apparatus = state
                .apparatus
                .get(&pos)
                .ok_or("The mortar is not installed.")?;
            if apparatus.kind != ApparatusKind::Mortar {
                return Err("Grinding needs the mortar and slab.".into());
            }
            let batch = apparatus
                .batch
                .as_ref()
                .ok_or("The mortar has no measured batch.")?;
            (
                batch.id,
                batch.preparation_id.clone(),
                apparatus.installation_id,
            )
        };
        let definition = self
            .reg
            .preparations
            .get(&preparation_id)
            .ok_or("The saved preparation definition is unavailable.")?
            .clone();
        let stack = inventory
            .slots
            .get(slot)
            .copied()
            .flatten()
            .ok_or("That authoritative inventory slot is empty.")?;
        let item_name = self.reg.item(stack.item).name.clone();
        let ingredient_definition = definition
            .ingredients
            .iter()
            .find(|ingredient| ingredient.item == item_name)
            .ok_or("That item is not a declared ingredient in this batch.")?
            .clone();
        let loaded = state
            .apparatus
            .get(&pos)
            .and_then(|apparatus| apparatus.batch.as_ref())
            .and_then(|batch| {
                batch
                    .ingredients
                    .iter()
                    .find(|ingredient| ingredient.item == item_name)
            })
            .map_or(0, |ingredient| ingredient.count);
        if loaded >= ingredient_definition.count {
            return Err("That ingredient is already present in its measured quantity.".into());
        }
        let taken = take_exact_slot(inventory, slot, stack.item)?;
        let item_definition = self.reg.item(taken.item);
        let condition_permille = if item_definition.food.is_some()
            && item_definition.durability != 0
            && taken.durability != 0
        {
            u16::try_from(
                u64::from(taken.durability)
                    .saturating_mul(1_000)
                    .checked_div(u64::from(item_definition.durability))
                    .unwrap_or(1_000)
                    .min(1_000),
            )
            .unwrap_or(1_000)
            .max(1)
        } else {
            1_000
        };
        let materials = crate::materials::stack_materials(&self.reg, taken);
        let (retained_materials, residue_materials) =
            split_materials(&materials, ingredient_definition.retention_permille);
        let ingredient_water = if item_name == "base:rainbell_dew" {
            let weather = self
                .planetary_weather
                .as_ref()
                .ok_or("Rainbell Dew needs the authoritative water ledger.")?;
            let mut available = weather.water.ledger.industrial;
            let parcel = available.take(crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL);
            if parcel.water_hu != crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL {
                return Err(
                    "That Rainbell Dew has no complete harvested-water custody behind it.".into(),
                );
            }
            parcel
        } else {
            ReservoirMass::default()
        };

        let clean = if taken.arcane_id == 0 {
            Current::default()
        } else {
            self.arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.account(&ArcaneOwner::Item(taken.arcane_id)))
                .map_or_else(Current::default, |account| account.current.clone())
        };
        let dross = if taken.arcane_id == 0 {
            Current::default()
        } else {
            self.arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.account(&ArcaneOwner::ItemDross(taken.arcane_id)))
                .map_or_else(Current::default, |account| account.current.clone())
        };
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let now = self.alchemy_tick();
        let (revision, all_loaded) = {
            let apparatus = state
                .apparatus
                .get_mut(&pos)
                .ok_or("The mortar disappeared.")?;
            let batch = apparatus.batch.as_mut().ok_or("The batch disappeared.")?;
            if batch.outcome != BatchOutcome::Processing
                || definition.steps.get(usize::from(batch.step_index)) != Some(&ProcessStep::Grind)
            {
                return Err("Grinding is not the current declared process step.".into());
            }
            if let Some(ingredient) = batch
                .ingredients
                .iter_mut()
                .find(|ingredient| ingredient.item == item_name)
            {
                ingredient.count = ingredient.count.saturating_add(1);
                ingredient.condition_permille =
                    ingredient.condition_permille.min(condition_permille);
                add_materials(&mut ingredient.retained_materials, &retained_materials)?;
                add_materials(&mut ingredient.residue_materials, &residue_materials)?;
                if taken.arcane_id != 0 {
                    ingredient.source_arcane_ids.push(taken.arcane_id);
                }
            } else {
                batch.ingredients.push(BatchIngredientState {
                    item: item_name.clone(),
                    count: 1,
                    condition_permille,
                    retained_materials,
                    residue_materials,
                    source_arcane_ids: (taken.arcane_id != 0)
                        .then_some(taken.arcane_id)
                        .into_iter()
                        .collect(),
                });
            }
            batch.current_units = batch
                .current_units
                .checked_add(clean.total())
                .ok_or("Batch Current custody overflowed.")?;
            batch.charge_input_units = batch
                .charge_input_units
                .checked_add(clean.total())
                .ok_or("Batch admitted-charge counter overflowed.")?;
            batch.dross_units = batch
                .dross_units
                .checked_add(dross.total())
                .ok_or("Batch dross custody overflowed.")?;
            batch.residue_water = batch
                .residue_water
                .checked_add(ingredient_water)
                .ok_or("Batch ingredient-water custody overflowed.")?;
            let all_loaded = definition.ingredients.iter().all(|required| {
                batch
                    .ingredients
                    .iter()
                    .find(|ingredient| ingredient.item == required.item)
                    .is_some_and(|ingredient| ingredient.count == required.count)
            });
            if all_loaded {
                batch.observations.push(ProcessObservation {
                    step: ProcessStep::Grind,
                    tick: now,
                    temperature_millic: apparatus.temperature_millic,
                    agitation: apparatus.agitation,
                    cleanliness_permille: apparatus.cleanliness_permille,
                    charge_delta: clean.total(),
                });
                batch.step_index = batch.step_index.saturating_add(1);
            }
            batch.revision = batch.revision.saturating_add(1);
            apparatus.integrity_permille = apparatus.integrity_permille.saturating_sub(2);
            apparatus.cleanliness_permille = apparatus.cleanliness_permille.saturating_sub(8);
            apparatus.revision = apparatus.revision.saturating_add(1);
            apparatus.last_operator = request.actor;
            (apparatus.revision, all_loaded)
        };
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id,
            actor: request.actor,
            action: "grind".into(),
            preparation_id: preparation_id.clone(),
            volume_units: 0,
            current_units: clean.total(),
            dross_units: dross.total(),
            tick: now,
            note: if all_loaded {
                "measured mash complete".into()
            } else {
                "one declared ingredient ground".into()
            },
        });
        if !clean.is_empty() || !dross.is_empty() {
            let mut debits = Vec::new();
            if !clean.is_empty() {
                debits.push((ArcaneOwner::Item(taken.arcane_id), clean.clone()));
            }
            if !dross.is_empty() {
                debits.push((ArcaneOwner::ItemDross(taken.arcane_id), dross.clone()));
            }
            let mut credits = Vec::new();
            if !clean.is_empty() {
                credits.push((
                    ArcaneOwner::Alchemy(crate::alchemy::batch_owner_id(batch_id)),
                    clean,
                    Some(preparation_id.clone()),
                ));
            }
            if !dross.is_empty() {
                credits.push((
                    ArcaneOwner::AlchemyDross(crate::alchemy::batch_owner_id(batch_id)),
                    dross,
                    Some(preparation_id.clone()),
                ));
            }
            self.commit_alchemy_current(
                state.clone(),
                operation_id,
                &preparation_id,
                "ingredient ground into exact alchemy custody",
                debits,
                credits,
            )?;
        }
        let message = if all_loaded {
            "The measured mash is ready to transfer."
        } else {
            "The ingredient is reduced; the recipe still shows missing material."
        };
        let mut result = result_for(&self.reg, state, pos, AlchemyCueKind::Grind, message, None)?;
        result.revision = revision;
        Ok(result)
    }

    fn alchemy_transfer_mash(
        &self,
        source: BlockPos,
        destination: BlockPos,
        request: &AlchemyRequest,
        state: &mut crate::alchemy::AlchemyState,
    ) -> Result<AlchemyResult, String> {
        if !near(source, destination) {
            return Err(
                "The receiving apparatus must be within the laboratory transfer reach.".into(),
            );
        }
        let destination_kind = self.alchemy_kind_at(destination)?;
        ensure_apparatus(state, destination, destination_kind, request.actor)
            .map_err(|error| error.to_string())?;
        let mut batch = state
            .apparatus
            .get_mut(&source)
            .and_then(|apparatus| apparatus.batch.take())
            .ok_or("There is no mash to transfer.")?;
        let definition = self
            .reg
            .preparations
            .get(&batch.preparation_id)
            .ok_or("The saved preparation definition is unavailable.")?;
        let next_step = definition.steps.get(usize::from(batch.step_index)).copied();
        let required_destination = match next_step {
            Some(ProcessStep::Grind) => ApparatusKind::Mortar,
            Some(ProcessStep::Filter) => ApparatusKind::FilterStand,
            Some(ProcessStep::Distill) => ApparatusKind::Alembic,
            _ => definition.process.apparatus(),
        };
        if destination_kind != required_destination {
            state
                .apparatus
                .get_mut(&source)
                .expect("source existed")
                .batch = Some(batch);
            return Err(format!(
                "{} must continue in its {:?}.",
                definition.label, required_destination
            ));
        }
        if state
            .apparatus
            .get(&destination)
            .is_some_and(|apparatus| apparatus.batch.is_some())
        {
            state
                .apparatus
                .get_mut(&source)
                .expect("source existed")
                .batch = Some(batch);
            return Err("The receiving apparatus already holds a batch.".into());
        }
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let batch_id = batch.id;
        let preparation_id = batch.preparation_id.clone();
        let now = self.alchemy_tick();
        let destination_apparatus = state
            .apparatus
            .get_mut(&destination)
            .expect("destination was ensured");
        batch.installation_id = destination_apparatus.installation_id;
        batch.revision = batch.revision.saturating_add(1);
        destination_apparatus.batch = Some(batch);
        destination_apparatus.revision = destination_apparatus.revision.saturating_add(1);
        destination_apparatus.last_operator = request.actor;
        let destination_installation = destination_apparatus.installation_id;
        let source_installation = {
            let source_apparatus = state.apparatus.get_mut(&source).expect("source existed");
            source_apparatus.revision = source_apparatus.revision.saturating_add(1);
            source_apparatus.last_operator = request.actor;
            source_apparatus.installation_id
        };
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id: destination_installation,
            batch_id,
            actor: request.actor,
            action: "transfer_mash".into(),
            preparation_id,
            volume_units: 0,
            current_units: 0,
            dross_units: 0,
            tick: now,
            note: format!("mash transferred from installation {source_installation}"),
        });
        result_for(
            &self.reg,
            state,
            destination,
            AlchemyCueKind::Pour,
            "The mash is physically transferred into its process vessel.",
            None,
        )
    }
}

fn result_for(
    registry: &crate::registry::Registry,
    state: &crate::alchemy::AlchemyState,
    pos: BlockPos,
    cue_kind: AlchemyCueKind,
    message: &str,
    produced: Option<ProducedStack>,
) -> Result<AlchemyResult, String> {
    let apparatus = state
        .apparatus
        .get(&pos)
        .ok_or("That apparatus is not in the authoritative census.")?;
    let batch = apparatus.batch.as_ref();
    let definition = batch.and_then(|batch| registry.preparations.get(&batch.preparation_id));
    let doses_remaining = batch.zip(definition).map_or(0, |(batch, definition)| {
        (batch.liquid.volume_units / definition.dose_units).min(u64::from(u16::MAX)) as u16
    });
    let outcome = batch.map(|batch| batch.outcome.clone());
    let intensity = match outcome {
        Some(BatchOutcome::Failed(BatchFailure::OverchargedBatch)) => 255,
        Some(BatchOutcome::Failed(_)) | Some(BatchOutcome::Spoiled) => 180,
        Some(BatchOutcome::Ready) => 120,
        _ => 72,
    };
    let color = match cue_kind {
        AlchemyCueKind::Overcharge | AlchemyCueKind::Pulse => [120, 180, 255],
        AlchemyCueKind::Leak | AlchemyCueKind::Spoil => [150, 80, 170],
        AlchemyCueKind::Filter | AlchemyCueKind::Clean => [120, 210, 180],
        _ => [220, 190, 120],
    };
    Ok(AlchemyResult {
        installation_id: apparatus.installation_id,
        batch_id: batch.map_or(0, |batch| batch.id),
        revision: apparatus.revision,
        preparation_id: batch.map(|batch| batch.preparation_id.clone()),
        outcome,
        volume_units: batch.map_or(0, |batch| batch.liquid.volume_units),
        doses_remaining,
        temperature_millic: apparatus.temperature_millic,
        cleanliness_permille: apparatus.cleanliness_permille,
        next_step: batch.zip(definition).and_then(|(batch, definition)| {
            definition.steps.get(usize::from(batch.step_index)).copied()
        }),
        cue: AlchemyCue {
            pos,
            installation_id: apparatus.installation_id,
            batch_id: batch.map_or(0, |batch| batch.id),
            revision: apparatus.revision,
            kind: cue_kind,
            intensity,
            color,
            message: message.into(),
        },
        produced,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_batch(definition: &crate::alchemy::PreparationDef) -> AlchemyBatch {
        AlchemyBatch {
            id: 1,
            preparation_id: definition.id.clone(),
            definition_version: definition.version,
            actor: [7; 16],
            actor_label: "fixture".into(),
            installation_id: 1,
            liquid: ExactLiquid::default(),
            residue_water: ReservoirMass::default(),
            ingredients: vec![BatchIngredientState {
                item: definition.ingredients[0].item.clone(),
                count: definition.ingredients[0].count,
                condition_permille: 1_000,
                retained_materials: MaterialVector::new(),
                residue_materials: MaterialVector::new(),
                source_arcane_ids: Vec::new(),
            }],
            charge_input_units: definition.charge_units,
            current_units: definition
                .charge_units
                .saturating_sub(definition.dross_units),
            dross_units: definition.dross_units,
            step_index: 0,
            observations: Vec::new(),
            started_tick: 10,
            due_tick: 20,
            born_tick: 10,
            expires_tick: 1_000,
            outcome: BatchOutcome::Processing,
            revision: 1,
        }
    }

    #[test]
    fn every_named_failure_is_deterministic_and_has_a_physical_output() {
        let registry = crate::registry::load(std::path::Path::new("/nonexistent-alchemy-mods"));
        let definition = registry.preparations.get("base:hearth_tonic").unwrap();
        let batch = fixture_batch(definition);
        let midpoint = definition.temperature_millic[0]
            + (definition.temperature_millic[1] - definition.temperature_millic[0]) / 2;

        assert_eq!(
            process_failure(
                definition,
                midpoint,
                definition.agitation,
                definition.cleanliness_min.saturating_sub(1),
                &batch,
                ProcessStep::Settle,
                batch.due_tick,
            ),
            Some(BatchFailure::FouledBatch)
        );
        let mut overcharged = batch.clone();
        overcharged.charge_input_units += 1;
        assert_eq!(
            process_failure(
                definition,
                midpoint,
                definition.agitation,
                1_000,
                &overcharged,
                ProcessStep::Settle,
                batch.due_tick,
            ),
            Some(BatchFailure::OverchargedBatch)
        );
        let mut spent = batch.clone();
        spent.charge_input_units -= 1;
        assert_eq!(
            process_failure(
                definition,
                midpoint,
                definition.agitation,
                1_000,
                &spent,
                ProcessStep::Charge,
                batch.due_tick,
            ),
            Some(BatchFailure::SpentLiquor)
        );
        assert_eq!(
            process_failure(
                definition,
                definition.temperature_millic[1] + 1,
                definition.agitation,
                1_000,
                &batch,
                ProcessStep::Heat,
                batch.due_tick,
            ),
            Some(BatchFailure::ScorchedMash)
        );
        assert_eq!(
            process_failure(
                definition,
                definition.temperature_millic[0] - 1,
                definition.agitation,
                1_000,
                &batch,
                ProcessStep::Heat,
                batch.due_tick,
            ),
            Some(BatchFailure::WeakExtraction)
        );
        let wrong_agitation = match definition.agitation {
            AgitationKind::Still => AgitationKind::Shaken,
            AgitationKind::Stirred | AgitationKind::Shaken => AgitationKind::Still,
        };
        assert_eq!(
            process_failure(
                definition,
                midpoint,
                wrong_agitation,
                1_000,
                &batch,
                ProcessStep::Agitate,
                batch.due_tick,
            ),
            Some(BatchFailure::BrokenEmulsion)
        );
        let mut stale = batch.clone();
        stale.ingredients[0].condition_permille = 249;
        assert_eq!(
            process_failure(
                definition,
                midpoint,
                definition.agitation,
                1_000,
                &stale,
                ProcessStep::Settle,
                batch.due_tick,
            ),
            Some(BatchFailure::WeakExtraction)
        );

        for failure in BatchFailure::ALL {
            let item = registry.item_id(failure.item_id()).unwrap();
            assert_eq!(registry.item(item).max_stack, 1);
            assert!(registry.item(item).arcane.is_some());
        }
    }

    #[test]
    fn valid_declared_steps_have_no_hidden_failure_roll() {
        let registry = crate::registry::load(std::path::Path::new("/nonexistent-alchemy-mods"));
        for definition in registry.preparations.values() {
            let batch = fixture_batch(definition);
            let midpoint = definition.temperature_millic[0]
                + (definition.temperature_millic[1] - definition.temperature_millic[0]) / 2;
            for step in &definition.steps {
                assert_eq!(
                    process_failure(
                        definition,
                        if *step == ProcessStep::Cool {
                            definition.storage_temperature_millic[1]
                        } else {
                            midpoint
                        },
                        definition.agitation,
                        1_000,
                        &batch,
                        *step,
                        batch.due_tick,
                    ),
                    None,
                    "{} failed a valid {:?} control",
                    definition.id,
                    step
                );
            }
        }
    }

    #[test]
    fn brine_distillation_keeps_exact_water_and_leaves_exact_salt_residue() {
        let registry = crate::registry::load(std::path::Path::new(
            "/nonexistent-alchemy-brine-distillation-mods",
        ));
        let definition = registry.preparations.get("base:storm_cordial").unwrap();
        let mut batch = fixture_batch(definition);
        batch.liquid = ExactLiquid {
            carrier: Some(CarrierKind::Brine),
            volume_units: 257,
            water: ReservoirMass {
                water_hu: 256,
                salt_mass: 65_537,
            },
            carrier_state: WaterCarrier {
                thermal_millic_hu: 18_000 * 257,
                dross_subunits: 13,
            },
            solutes: BTreeMap::from([("base:bucket_salt".into(), 257)]),
        };
        batch.residue_water = ReservoirMass {
            water_hu: 7,
            salt_mass: 11,
        };
        let water_before = batch.liquid.water.water_hu + batch.residue_water.water_hu;
        let salt_before = batch.liquid.water.salt_mass + batch.residue_water.salt_mass;

        assert_eq!(separate_brine_distillate(&mut batch).unwrap(), 65_537);
        assert_eq!(batch.liquid.carrier, Some(CarrierKind::FreshWater));
        assert_eq!(batch.liquid.water.salt_mass, 0);
        assert_eq!(batch.residue_water.salt_mass, salt_before);
        assert_eq!(
            batch.liquid.water.water_hu + batch.residue_water.water_hu,
            water_before
        );
        assert_eq!(
            batch.liquid.water.salt_mass + batch.residue_water.salt_mass,
            salt_before
        );
        batch.liquid.validate().unwrap();
    }

    #[test]
    fn declared_dissolved_matter_displaces_volume_without_conjuring_water() {
        let registry = crate::registry::load(std::path::Path::new(
            "/nonexistent-alchemy-displacement-mods",
        ));
        let mut definition = registry.preparations["base:hearth_tonic"].clone();
        definition.dissolved_units = 3;
        let mut liquid = ExactLiquid {
            carrier: Some(CarrierKind::FreshWater),
            volume_units: definition.solvent_units,
            water: ReservoirMass::fresh(definition.solvent_units),
            carrier_state: WaterCarrier {
                thermal_millic_hu: i64::try_from(definition.solvent_units).unwrap() * 20_000,
                dross_subunits: 0,
            },
            solutes: BTreeMap::from([(definition.solvent_item.clone(), definition.solvent_units)]),
        };
        let water_before = liquid.water;
        add_dissolved_displacement(&mut liquid, &definition, 20_000).unwrap();
        assert_eq!(
            liquid.volume_units,
            definition.solvent_units + definition.dissolved_units
        );
        assert_eq!(liquid.water, water_before);
        assert_eq!(
            definition
                .ingredients
                .iter()
                .map(|ingredient| liquid.solutes.get(&ingredient.item).copied().unwrap_or(0))
                .sum::<u64>(),
            definition.dissolved_units
        );
        liquid.validate().unwrap();
    }
}
