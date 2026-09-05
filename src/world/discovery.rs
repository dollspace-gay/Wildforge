//! Authoritative world integration for physical magical discovery.

use super::*;
use crate::discovery::{
    CalibrationGrade, DiscoveryError, ExperimentKind, KnowledgeKind, NewObservation,
    ObservationSummary, PlanetaryProvenance, QualitativeReading, StabilityBand, StrengthBand,
};

#[derive(Clone, Copy, Debug)]
pub enum ObservationTarget {
    Region(BlockPos),
    Block(BlockPos),
    Item(ItemStack, BlockPos),
}

impl ObservationTarget {
    pub(crate) fn position(self) -> BlockPos {
        match self {
            Self::Region(pos) | Self::Block(pos) | Self::Item(_, pos) => pos,
        }
    }
}

impl World {
    pub fn exchange_experiment_item_at(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
        slot: usize,
    ) -> Result<String, String> {
        let fixture = self
            .reg
            .block(self.get_block_at(pos))
            .discovery_fixture
            .as_ref()
            .filter(|fixture| fixture.kind == "experiment_apparatus")
            .ok_or("There is no comparative apparatus there.")?;
        if fixture.experiments.is_empty() {
            return Err("That apparatus has no configured trials.".into());
        }
        let selected = inventory
            .slots
            .get(slot)
            .copied()
            .ok_or("That pack slot does not exist.")?;
        if let Some(stack) = selected {
            let definition = self.reg.item(stack.item);
            let reference_kind = definition
                .discovery
                .as_ref()
                .filter(|discovery| discovery.kind == "reference_object")
                .and_then(|discovery| discovery.experiment);
            let is_sample = definition.observation.is_some()
                || definition.arcane.is_some()
                || matches!(
                    definition.name.as_str(),
                    "base:dirt"
                        | "base:grass"
                        | "base:bucket_water"
                        | "base:bucket_brackish"
                        | "base:bucket_salt"
                );
            if reference_kind.is_none() && !is_sample {
                return Err("That is neither a measurable sample nor a reference standard.".into());
            }
            let occupied = match self.installations.get(&pos) {
                Some(BlockEntity::DiscoveryApparatus(apparatus)) => {
                    if reference_kind.is_some() {
                        apparatus.reference.is_some()
                    } else {
                        apparatus.sample.is_some()
                    }
                }
                _ => false,
            };
            if occupied {
                return Err(if reference_kind.is_some() {
                    "Retrieve the installed reference before fitting another one.".into()
                } else {
                    "Retrieve the held sample before loading another one.".into()
                });
            }
            let mut physical = inventory
                .take_one_stack(slot)
                .ok_or("The selected item moved before it could be loaded.")?;
            if reference_kind.is_some()
                && let Err(error) = self.bind_discovery_stack_at(pos, &mut physical)
            {
                let _ = inventory.add_stack(&self.reg, physical);
                return Err(error.to_string());
            }
            let entity = self.installations
                .entry(pos)
                .or_insert_with(|| BlockEntity::DiscoveryApparatus(Default::default()));
            let BlockEntity::DiscoveryApparatus(apparatus) = entity else {
                let _ = inventory.add_stack(&self.reg, physical);
                return Err("Another block entity occupies the apparatus.".into());
            };
            if let Some(reference_kind) = reference_kind {
                apparatus.reference = Some(physical);
                Ok(format!(
                    "Installed the {} reference standard.",
                    reference_kind.label()
                ))
            } else {
                apparatus.sample = Some(physical);
                Ok("Loaded one physical sample into the apparatus holder.".into())
            }
        } else {
            let Some(BlockEntity::DiscoveryApparatus(apparatus)) =
                self.installations.get_mut(&pos)
            else {
                return Err("The apparatus bays are empty.".into());
            };
            let physical = apparatus
                .sample
                .take()
                .or_else(|| apparatus.reference.take())
                .ok_or("The apparatus bays are empty.")?;
            inventory.slots[slot] = Some(physical);
            Ok("Retrieved one physical apparatus item.".into())
        }
    }

    pub fn experiment_sample_at(
        &self,
        pos: BlockPos,
        kind: ExperimentKind,
    ) -> Result<ItemStack, String> {
        let Some(BlockEntity::DiscoveryApparatus(apparatus)) = self.installations.get(&pos) else {
            return Err("Load a sample and calibrated reference into the apparatus.".into());
        };
        let sample = apparatus
            .sample
            .ok_or("Load a physical sample into the apparatus holder.")?;
        let reference = apparatus
            .reference
            .ok_or("Install a calibrated reference in the apparatus.")?;
        let reference_kind = self
            .reg
            .item(reference.item)
            .discovery
            .as_ref()
            .and_then(|definition| definition.experiment);
        if reference_kind != Some(kind) || reference.arcane_id == 0 {
            return Err(format!(
                "The installed reference is not calibrated for the {}.",
                kind.label()
            ));
        }
        Ok(sample)
    }

    pub fn save_discovery(&mut self) -> std::io::Result<()> {
        self.discovery_state
            .as_mut()
            .map_or(Ok(()), |state| state.save().map_err(std::io::Error::other))
    }

    pub(crate) fn claim_discovery_recovery(&mut self, pos: BlockPos) -> bool {
        let key = format!("brush:{}:{}:{}:{}", pos.face(), pos.u(), pos.y(), pos.v());
        let Some(state) = self.discovery_state.as_mut() else {
            // Atlas-free test/dev fixtures still get the ordinary once-only
            // guarantee from the remnant block transmuting immediately.
            return true;
        };
        if state.site_was_recovered(&key) {
            return false;
        }
        state.mark_site_recovered(key)
    }

    pub(crate) fn discovery_site_installed(&self, key: &str) -> bool {
        self.discovery_state
            .as_ref()
            .is_some_and(|state| state.site_was_installed(key))
    }

    pub(crate) fn mark_discovery_site_installed(&mut self, key: &str) {
        if let Some(state) = self.discovery_state.as_mut() {
            state.mark_site_installed(key);
        }
    }

    /// Give identity and immutable historical text to a physical discovery
    /// object before it enters an inventory, entity, or network message.
    pub fn bind_discovery_stack_at(
        &mut self,
        at: BlockPos,
        stack: &mut ItemStack,
    ) -> Result<(), DiscoveryError> {
        let content_id = self.reg.item(stack.item).name.clone();
        let Some(definition) = self.reg.item(stack.item).discovery.clone() else {
            return Ok(());
        };
        if matches!(definition.kind.as_str(), "tuning_lens" | "lens_frame") {
            return Ok(());
        }
        if stack.count != 1 {
            return Err(DiscoveryError::Corrupt(
                "physical knowledge objects must be single-item stacks".into(),
            ));
        }
        let state = self
            .discovery_state
            .as_mut()
            .ok_or_else(|| DiscoveryError::Corrupt("world has no discovery authority".into()))?;
        match definition.kind.as_str() {
            "field_ledger" => {
                if stack.arcane_id == 0 {
                    stack.arcane_id = state.create_object(
                        content_id,
                        KnowledgeKind::FieldLedger,
                        self.calendar_state.day(),
                        Some(at),
                    )?;
                } else {
                    state.ensure_object_with_id(
                        stack.arcane_id,
                        content_id,
                        KnowledgeKind::FieldLedger,
                        self.calendar_state.day(),
                        Some(at),
                    )?;
                }
            }
            "survey_folio" => {
                if stack.arcane_id == 0 {
                    stack.arcane_id = state.create_object(
                        content_id,
                        KnowledgeKind::SurveyFolio,
                        self.calendar_state.day(),
                        Some(at),
                    )?;
                } else {
                    state.ensure_object_with_id(
                        stack.arcane_id,
                        content_id,
                        KnowledgeKind::SurveyFolio,
                        self.calendar_state.day(),
                        Some(at),
                    )?;
                }
            }
            "artifact" => {
                let evidence = definition
                    .evidence_class
                    .unwrap_or_else(|| "unknown_artifact".into());
                if stack.arcane_id == 0 {
                    stack.arcane_id = state.create_artifact(
                        content_id,
                        evidence,
                        &definition.authored_text,
                        self.calendar_state.day(),
                        Some(at),
                    )?;
                } else {
                    let text = crate::discovery::artifact_phrase(
                        stack.arcane_id,
                        &evidence,
                        &definition.authored_text,
                    );
                    state.ensure_object_with_id(
                        stack.arcane_id,
                        content_id,
                        KnowledgeKind::Artifact {
                            evidence_class: evidence,
                            text,
                        },
                        self.calendar_state.day(),
                        Some(at),
                    )?;
                }
            }
            "calibration_plate" => {
                let grade = definition.calibration.unwrap_or(CalibrationGrade::Field);
                let evidence_class = definition.evidence_class.clone();
                if stack.arcane_id == 0 {
                    stack.arcane_id = state.create_calibration_plate(
                        content_id,
                        grade,
                        evidence_class,
                        &definition.authored_text,
                        self.calendar_state.day(),
                        Some(at),
                    )?;
                } else {
                    let text = evidence_class.as_deref().map(|class| {
                        crate::discovery::artifact_phrase(
                            stack.arcane_id,
                            class,
                            &definition.authored_text,
                        )
                    });
                    state.ensure_object_with_id(
                        stack.arcane_id,
                        content_id,
                        KnowledgeKind::CalibrationPlate {
                            grade,
                            evidence_class,
                            text,
                        },
                        self.calendar_state.day(),
                        Some(at),
                    )?;
                }
            }
            "reference_object" => {
                let kind = definition.experiment.ok_or_else(|| {
                    DiscoveryError::Corrupt("reference object has no experiment family".into())
                })?;
                if stack.arcane_id == 0 {
                    stack.arcane_id = state.create_object(
                        content_id,
                        KnowledgeKind::ReferenceObject { experiment: kind },
                        self.calendar_state.day(),
                        Some(at),
                    )?;
                } else {
                    state.ensure_object_with_id(
                        stack.arcane_id,
                        content_id,
                        KnowledgeKind::ReferenceObject { experiment: kind },
                        self.calendar_state.day(),
                        Some(at),
                    )?;
                }
            }
            other => {
                return Err(DiscoveryError::Corrupt(format!(
                    "unsupported discovery item behavior {other}"
                )));
            }
        }
        Ok(())
    }

    pub fn discovery_artifact_text(
        &mut self,
        stack: &mut ItemStack,
        at: BlockPos,
    ) -> Option<String> {
        if let Err(error) = self.bind_discovery_stack_at(at, stack) {
            eprintln!("discovery: could not bind artifact: {error}");
            return None;
        }
        self.discovery_state
            .as_ref()
            .and_then(|state| state.artifact_text(stack.arcane_id))
            .map(ToOwned::to_owned)
    }

    pub fn calibration_grade_for(&self, stack: ItemStack) -> Option<CalibrationGrade> {
        self.discovery_state
            .as_ref()
            .and_then(|state| state.calibration_of(stack.arcane_id))
            .or_else(|| self.reg.item(stack.item).discovery.as_ref()?.calibration)
    }

    /// Fit an industrial frame with one Echo Slate plate and one replaceable
    /// Wellglass element. The Wellglass item identity becomes the lens
    /// identity; the slate's Current returns locally while its physical
    /// mineral remains in the composite material vector.
    pub fn assemble_tuning_lens_at(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
    ) -> Result<ItemStack, String> {
        let frame = self
            .reg
            .item_id("base:tuning_lens_frame")
            .ok_or("content has no tuning lens frame")?;
        let mount = self
            .reg
            .item_id("base:tuning_lens_mount")
            .ok_or("content has no fitted tuning lens mount")?;
        let slate = self
            .reg
            .item_id("base:echo_slate")
            .ok_or("content has no Echo Slate plate")?;
        let wellglass = self
            .reg
            .item_id("base:wellglass_shard")
            .ok_or("content has no Wellglass element")?;
        let lens = self
            .reg
            .item_id("base:tuning_lens")
            .ok_or("content has no tuning lens")?;
        let find = |item| {
            inventory
                .slots
                .iter()
                .position(|slot| slot.is_some_and(|stack| stack.item == item))
        };
        let mount_slot = find(mount);
        let frame_slot = mount_slot
            .or_else(|| find(frame))
            .ok_or("Bring a fitted mount, or an unfitted frame with one Echo Slate plate.")?;
        let slate_slot = if mount_slot.is_none() {
            Some(find(slate).ok_or("Bring one Echo Slate plate for the unfitted frame.")?)
        } else {
            None
        };
        let wellglass_slot = find(wellglass).ok_or("Bring one charged Wellglass shard.")?;
        let mut wellglass_stack = inventory.slots[wellglass_slot].unwrap();
        let mut slate_stack = slate_slot.map(|slot| inventory.slots[slot].unwrap());
        if let Some(stack) = &mut slate_stack {
            self.bind_arcane_stack_at(pos, stack, "lens assembly slate")
                .map_err(|error| error.to_string())?;
        }
        self.bind_arcane_stack_at(pos, &mut wellglass_stack, "lens assembly element")
            .map_err(|error| error.to_string())?;
        if let (Some(slot), Some(stack)) = (slate_slot, slate_stack) {
            inventory.slots[slot] = Some(stack);
        }
        inventory.slots[wellglass_slot] = Some(wellglass_stack);
        let slate_stack = slate_slot
            .map(|slot| {
                inventory
                    .take_one_stack(slot)
                    .ok_or("Echo Slate moved during assembly.")
            })
            .transpose()?;
        let wellglass_stack = inventory
            .take_one_stack(wellglass_slot)
            .ok_or("Wellglass moved during assembly.")?;
        let frame_stack = inventory
            .take_one_stack(frame_slot)
            .ok_or("The lens mount moved during assembly.")?;
        let output = ItemStack {
            item: lens,
            count: 1,
            durability: self.reg.item(lens).durability,
            arcane_id: wellglass_stack.arcane_id,
        };
        if inventory.slots[frame_slot].is_some() {
            // A counted frame stack is forbidden by content, but preserve the
            // operation atomically if a mod or old save violates that rule.
            let left = inventory.add_stack(&self.reg, output);
            if left != 0 {
                if let Some(slate_stack) = slate_stack {
                    inventory.add_stack(&self.reg, slate_stack);
                }
                inventory.add_stack(&self.reg, frame_stack);
                inventory.add_stack(&self.reg, wellglass_stack);
                return Err("No room for the completed lens.".into());
            }
        } else {
            inventory.slots[frame_slot] = Some(output);
        }
        if let Some(slate_stack) = slate_stack {
            self.retire_arcane_stack_at(pos, slate_stack, "Echo Slate fitted into tuning lens");
        }
        Ok(output)
    }

    pub fn wear_tuning_lens_at(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
        slot: usize,
    ) -> bool {
        let Some(lens) = self.reg.item_id("base:tuning_lens") else {
            return false;
        };
        let Some(mut stack) = inventory.slots.get(slot).copied().flatten() else {
            return false;
        };
        if stack.item != lens {
            return false;
        }
        stack.durability = stack.durability.saturating_sub(1);
        if stack.durability != 0 {
            inventory.slots[slot] = Some(stack);
            return false;
        }
        self.retire_arcane_stack_at(pos, stack, "spent Wellglass lens element returned locally");
        inventory.slots[slot] = self
            .reg
            .item_id("base:tuning_lens_mount")
            .map(|mount| ItemStack::new(&self.reg, mount, 1));
        true
    }

    /// Make one host-authored measurement and put it in a physically present
    /// record holder. Callers are responsible for reach/LOS and inventory
    /// ownership; this function owns every measured field and signature.
    pub fn record_observation(
        &mut self,
        holder_id: u64,
        observer: (crate::identity::PlayerId, &str),
        target: ObservationTarget,
        calibration: CalibrationGrade,
        label: Option<String>,
        experiment: Option<ExperimentKind>,
    ) -> Result<ObservationSummary, DiscoveryError> {
        let (observer, observer_name) = observer;
        let at = target.position();
        let atlas = self
            .planet_atlas
            .as_ref()
            .ok_or_else(|| DiscoveryError::Corrupt("observation needs a planetary atlas".into()))?;
        let region = atlas.atlas_pos(at.surface());
        let survey = self
            .arcane_geography
            .as_ref()
            .map(|geography| geography.survey(region, true))
            .ok_or_else(|| DiscoveryError::Corrupt("observation needs arcane geography".into()))?;
        let item_instance = match target {
            ObservationTarget::Item(stack, _) => Some(stack.arcane_id),
            ObservationTarget::Block(pos) => {
                self.block_entity_at(&pos).and_then(|entity| match entity {
                    crate::world::BlockEntity::ChargeVessel(vessel) => {
                        vessel.vessel.map(|stack| stack.arcane_id)
                    }
                    crate::world::BlockEntity::BindingFrame(frame) => {
                        frame.output.map(|stack| stack.arcane_id)
                    }
                    _ => None,
                })
            }
            ObservationTarget::Region(_) => None,
        };
        let artifact_origin = item_instance.and_then(|id| {
            self.discovery_state
                .as_ref()
                .and_then(|state| state.artifact_origin(id))
        });
        let implement_instance = item_instance.and_then(|id| {
            self.implements_state
                .as_ref()
                .and_then(|state| state.instance(id))
        });
        let item_current = item_instance.and_then(|id| {
            self.arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.account(&crate::arcane::ArcaneOwner::Item(id)))
                .map(|account| account.current.clone())
        });
        let observed_item_units = item_current.as_ref().map(|current| {
            if implement_instance.is_some() {
                crate::implements::usable_charge(current.total())
            } else {
                current.total()
            }
        });
        let (phenomenon_id, observation, arcane, ecology, category) = match target {
            ObservationTarget::Region(_) => (
                "base:local_current_field".to_string(),
                None,
                None,
                None,
                "region".to_string(),
            ),
            ObservationTarget::Block(pos) => {
                let block = self.get_block_at(pos);
                let definition = self.reg.block(block);
                (
                    definition.name.clone(),
                    definition.observation.clone(),
                    definition.arcane.clone(),
                    definition.arcane_ecology.clone(),
                    definition
                        .observation
                        .as_ref()
                        .and_then(|definition| definition.categories.first().cloned())
                        .unwrap_or_else(|| "block".into()),
                )
            }
            ObservationTarget::Item(stack, _) => {
                let definition = self.reg.item(stack.item);
                (
                    definition.name.clone(),
                    definition.observation.clone(),
                    definition.arcane.clone(),
                    definition.arcane_ecology.clone(),
                    definition
                        .observation
                        .as_ref()
                        .and_then(|definition| definition.categories.first().cloned())
                        .unwrap_or_else(|| "item".into()),
                )
            }
        };
        let strength = observed_item_units
            .map(strength_of)
            .or_else(|| {
                arcane
                    .as_ref()
                    .map(|definition| strength_of(definition.capacity))
            })
            .unwrap_or_else(|| map_survey_strength(survey.strength));
        let implement_stability = implement_instance.map(|instance| match &instance.kind {
            crate::implements::ImplementKind::Wand { resolved, .. } => resolved.stability,
            crate::implements::ImplementKind::Charm { stability, .. } => *stability,
            crate::implements::ImplementKind::Vessel { containment, .. } => *containment,
            crate::implements::ImplementKind::Fragments { .. } => 0,
        });
        let stability = implement_stability
            .map(stability_of)
            .or_else(|| {
                arcane
                    .as_ref()
                    .map(|definition| stability_of(definition.stability_permille))
            })
            .unwrap_or_else(|| map_survey_condition(survey.condition));
        let mixture_components = item_current
            .as_ref()
            .map(|current| {
                if observed_item_units == Some(0) {
                    0
                } else {
                    current.parts().len()
                }
            })
            .or_else(|| arcane.as_ref().map(|definition| definition.resonance.len()))
            .unwrap_or(survey.dominant_resonances.len());
        let resonances = item_current
            .as_ref()
            .map(|current| {
                if observed_item_units == Some(0) {
                    return Vec::new();
                }
                let mut parts = current.parts().iter().collect::<Vec<_>>();
                parts.sort_by(|(a_name, a_units), (b_name, b_units)| {
                    b_units.cmp(a_units).then_with(|| a_name.cmp(b_name))
                });
                parts
                    .into_iter()
                    .take(2)
                    .map(|(name, _)| name.clone())
                    .collect()
            })
            .or_else(|| {
                arcane
                    .as_ref()
                    .map(|definition| definition.resonance.keys().take(2).cloned().collect())
            })
            .unwrap_or_else(|| {
                survey
                    .dominant_resonances
                    .iter()
                    .take(2)
                    .map(|value| (*value).to_string())
                    .collect()
            });
        let dross = item_instance
            .and_then(|id| {
                self.arcane_ledger
                    .as_ref()
                    .map(|ledger| ledger.item_dross_total(id))
            })
            .filter(|units| *units != 0)
            .map(strength_of)
            .unwrap_or_else(|| band_of(u64::from(self.arcane_cue_at(region)[1])));
        let mut properties = std::collections::BTreeMap::new();
        let visible = observation
            .as_ref()
            .map(|definition| definition.properties.as_slice())
            .unwrap_or(&[]);
        let source_signature = self.arcane_geography.as_ref().and_then(|geography| {
            let evidence = match target {
                ObservationTarget::Item(stack, _) => geography
                    .dynamic
                    .dross_state
                    .contained_provenance
                    .get(&stack.arcane_id),
                ObservationTarget::Block(pos) => geography
                    .dynamic
                    .dross_state
                    .materialized
                    .get(&pos)
                    .and_then(|scar_id| geography.dynamic.dross_state.scars.get(scar_id))
                    .map(|site| &site.provenance)
                    .or_else(|| {
                        item_instance.and_then(|id| {
                            geography.dynamic.dross_state.contained_provenance.get(&id)
                        })
                    }),
                ObservationTarget::Region(_) => {
                    geography.dynamic.dross_state.provenance.get(&region)
                }
            };
            evidence.and_then(crate::dross::DrossProvenance::qualitative_signature)
        });
        if let Some(signature) = source_signature {
            properties.insert("source signature".into(), signature);
        }
        if let Some(definition) = &arcane {
            if visible.iter().any(|property| property == "capacity") {
                properties.insert(
                    "capacity".into(),
                    strength_of(definition.capacity).to_string(),
                );
            }
            if visible.iter().any(|property| property == "conductivity") {
                properties.insert(
                    "conductivity".into(),
                    conductivity_of(definition.conductivity_permille).into(),
                );
            }
        }
        if let Some(ecology) = &ecology {
            if visible
                .iter()
                .any(|property| property == "biological_response")
            {
                properties.insert(
                    "biological response".into(),
                    ecology
                        .roles
                        .iter()
                        .map(|role| format!("{role:?}").to_lowercase())
                        .collect::<Vec<_>>()
                        .join(" / "),
                );
            }
            if visible.iter().any(|property| property == "dross_response") {
                properties.insert(
                    "dross response".into(),
                    if ecology.dross_tolerance >= 48 {
                        "tolerant"
                    } else if ecology.dross_tolerance >= 20 {
                        "sensitive"
                    } else {
                        "strongly averse"
                    }
                    .into(),
                );
            }
        }
        if visible.iter().any(|property| property == "condition") {
            match target {
                ObservationTarget::Block(pos) => {
                    let definition = self.reg.block(self.get_block_at(pos));
                    let condition = if definition.name.ends_with("_dead") {
                        "silent"
                    } else if definition.name.ends_with("_sick") {
                        "strained"
                    } else {
                        match self.installations.get(&pos) {
                            Some(BlockEntity::DiscoveryApparatus(apparatus)) => {
                                match (apparatus.sample.is_some(), apparatus.reference.is_some()) {
                                    (true, true) => "sample and reference installed",
                                    (true, false) => "sample installed; reference absent",
                                    (false, true) => "reference installed; sample absent",
                                    (false, false) => "empty and settled",
                                }
                            }
                            Some(BlockEntity::Furnace(state)) if state.burn_left > 0.0 => {
                                "active transfer"
                            }
                            Some(BlockEntity::Multiblock(state)) if state.lit => "active transfer",
                            Some(BlockEntity::Steam(state)) if state.fuel > 0.0 => {
                                "active transfer"
                            }
                            Some(BlockEntity::Multiblock(state)) if state.progress > 0.0 => {
                                "active transfer"
                            }
                            Some(_) => "assembled and idle",
                            None if definition.interaction.as_deref() == Some("heart") => {
                                "living and settled"
                            }
                            None => "resting physical sample",
                        }
                    };
                    properties.insert("condition".into(), condition.into());
                    if condition == "active transfer" {
                        properties.insert(
                            "working behavior".into(),
                            "physical input is moving toward a configured output".into(),
                        );
                    }
                }
                ObservationTarget::Item(_, _) => {
                    properties.insert("condition".into(), "portable sealed sample".into());
                }
                ObservationTarget::Region(_) => {}
            }
        }
        if let Some(experiment) = experiment {
            let result = experiment_result(experiment, arcane.as_ref(), ecology.as_ref(), dross);
            properties.insert(experiment.label().into(), result);
        }
        if let Some(origin) = artifact_origin {
            let origin_region = atlas.atlas_pos(origin.surface());
            properties.insert(
                "ruin echo".into(),
                if origin_region == region {
                    "anchored in this immediate field"
                } else {
                    "displaced from its surviving field echo"
                }
                .into(),
            );
        }
        let place = survey
            .nearby_place
            .as_ref()
            .map(|(_, _, name)| name.clone());
        let biome = self.generator.biome_at(at.surface()).name().to_lowercase();
        let uncertainty = reading_uncertainty(survey.uncertainty, mixture_components, calibration);
        let record = NewObservation {
            phenomenon_id,
            category,
            position: Some(at),
            provenance: PlanetaryProvenance {
                face: region.face.name().to_string(),
                atlas_u: Some(region.u),
                atlas_v: Some(region.v),
                biome,
                place,
            },
            day: self.calendar_state.day(),
            time_permille: ((self.calendar_state.clock() / f64::from(crate::server::DAY_LENGTH)).fract() * 1_000.0)
                as u16,
            season: crate::world::SEASONS[self.season_at_surface(at.surface())].to_lowercase(),
            calibration,
            reading: QualitativeReading {
                strength,
                stability,
                resonances,
                dross,
                drift: survey
                    .drift
                    .map(|direction| format!("{direction:?}").to_lowercase()),
                uncertainty,
                properties,
            },
            observer,
            observer_name: observer_name.to_string(),
            label,
        };
        let state = self
            .discovery_state
            .as_mut()
            .ok_or_else(|| DiscoveryError::Corrupt("world has no discovery authority".into()))?;
        let record_id = state.create_observation(holder_id, record)?;
        state
            .summaries(holder_id, true, self.reg.content_hash)?
            .into_iter()
            .find(|summary| summary.record_id == record_id)
            .ok_or(DiscoveryError::MissingRecord(record_id))
    }

    pub fn discovery_summaries(
        &self,
        holder_id: u64,
        reveal_locations: bool,
    ) -> Result<Vec<ObservationSummary>, DiscoveryError> {
        self.discovery_state
            .as_ref()
            .ok_or_else(|| DiscoveryError::Corrupt("world has no discovery authority".into()))?
            .summaries(holder_id, reveal_locations, self.reg.content_hash)
    }

    pub fn discovery_library_index(
        &self,
        holder_id: u64,
        reveal_locations: bool,
    ) -> Result<crate::discovery::LibraryIndex, DiscoveryError> {
        self.discovery_state
            .as_ref()
            .ok_or_else(|| DiscoveryError::Corrupt("world has no discovery authority".into()))?
            .library_index(holder_id, reveal_locations, self.reg.content_hash)
    }

    pub fn copy_discovery_record(
        &mut self,
        source_holder: u64,
        record: u64,
        destination_holder: u64,
        include_location: bool,
    ) -> Result<u64, DiscoveryError> {
        self.discovery_state
            .as_mut()
            .ok_or_else(|| DiscoveryError::Corrupt("world has no discovery authority".into()))?
            .copy_record(source_holder, record, destination_holder, include_location)
    }
}

fn map_survey_strength(value: crate::arcane_geography::SurveyStrength) -> StrengthBand {
    use crate::arcane_geography::SurveyStrength as Source;
    match value {
        Source::Still => StrengthBand::Still,
        Source::Faint => StrengthBand::Faint,
        Source::Steady => StrengthBand::Steady,
        Source::Strong => StrengthBand::Strong,
        Source::Saturated => StrengthBand::Saturated,
    }
}

fn map_survey_condition(value: crate::arcane_geography::SurveyCondition) -> StabilityBand {
    use crate::arcane_geography::SurveyCondition as Source;
    match value {
        Source::Stable => StabilityBand::Stable,
        Source::Strained => StabilityBand::Strained,
        Source::Fouled => StabilityBand::Fouled,
    }
}

fn strength_of(units: u64) -> StrengthBand {
    match units {
        0 => StrengthBand::Still,
        1..=128 => StrengthBand::Faint,
        129..=512 => StrengthBand::Steady,
        513..=2_048 => StrengthBand::Strong,
        _ => StrengthBand::Saturated,
    }
}

fn band_of(band: u64) -> StrengthBand {
    match band.min(4) {
        0 => StrengthBand::Still,
        1 => StrengthBand::Faint,
        2 => StrengthBand::Steady,
        3 => StrengthBand::Strong,
        _ => StrengthBand::Saturated,
    }
}

fn stability_of(permille: u16) -> StabilityBand {
    match permille {
        800.. => StabilityBand::Stable,
        500..=799 => StabilityBand::Variable,
        250..=499 => StabilityBand::Strained,
        _ => StabilityBand::Fouled,
    }
}

fn conductivity_of(permille: u16) -> &'static str {
    match permille {
        800.. => "highly conductive",
        500..=799 => "conductive",
        250..=499 => "resistant",
        _ => "strongly resistant",
    }
}

fn reading_uncertainty(
    baseline: u8,
    mixture_components: usize,
    calibration: CalibrationGrade,
) -> u8 {
    let ambiguity = mixture_components
        .saturating_sub(1)
        .saturating_mul(8)
        .min(24) as u8;
    baseline
        .saturating_add(ambiguity)
        .saturating_sub(calibration.uncertainty_reduction())
        .min(100)
}

fn experiment_result(
    kind: ExperimentKind,
    arcane: Option<&crate::registry::ArcaneContentDef>,
    ecology: Option<&crate::registry::ArcaneEcologyDef>,
    dross: StrengthBand,
) -> String {
    match kind {
        ExperimentKind::Capacity => arcane
            .map(|definition| strength_of(definition.capacity).to_string())
            .unwrap_or_else(|| "no retained response".into()),
        ExperimentKind::Conductivity => arcane
            .map(|definition| conductivity_of(definition.conductivity_permille).into())
            .unwrap_or_else(|| "no repeatable trace".into()),
        ExperimentKind::Stability => arcane
            .map(|definition| stability_of(definition.stability_permille).to_string())
            .unwrap_or_else(|| "no repeatable response".into()),
        ExperimentKind::BiologicalResponse => ecology
            .map(|definition| {
                definition
                    .roles
                    .iter()
                    .map(|role| format!("{role:?}").to_lowercase())
                    .collect::<Vec<_>>()
                    .join(" / ")
            })
            .unwrap_or_else(|| "biologically inert in this trial".into()),
        ExperimentKind::DrossResponse => ecology
            .map(|definition| {
                if definition.dross_tolerance >= 48 {
                    "retains function under a foul reference"
                } else if dross >= StrengthBand::Strong {
                    "response collapses in the foul reference"
                } else {
                    "response weakens near the foul reference"
                }
                .into()
            })
            .unwrap_or_else(|| "no biological dross response".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualitative_bands_never_reveal_exact_values() {
        assert_eq!(strength_of(129), StrengthBand::Steady);
        assert_eq!(stability_of(500), StabilityBand::Variable);
        assert_eq!(conductivity_of(999), "highly conductive");
    }

    #[test]
    fn ambiguity_widens_and_physical_calibration_narrows_uncertainty() {
        let plain = reading_uncertainty(52, 1, CalibrationGrade::Uncalibrated);
        let mixture = reading_uncertainty(52, 3, CalibrationGrade::Uncalibrated);
        let field = reading_uncertainty(52, 3, CalibrationGrade::Field);
        let plate = reading_uncertainty(52, 3, CalibrationGrade::Plate);
        assert!(mixture > plain);
        assert!(field < mixture);
        assert!(plate < field);
    }

    #[test]
    fn every_experiment_family_is_a_repeatable_qualitative_comparison() {
        for kind in ExperimentKind::ALL {
            let first = experiment_result(kind, None, None, StrengthBand::Faint);
            let second = experiment_result(kind, None, None, StrengthBand::Faint);
            assert_eq!(first, second);
            assert!(!first.is_empty());
        }
    }
}
