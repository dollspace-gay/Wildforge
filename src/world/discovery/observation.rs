//! Observation discovery transaction coordination.

use crate::world::BlockEntity;
use crate::discovery::CalibrationGrade;
use crate::discovery::DiscoveryError;
use crate::discovery::ExperimentKind;
use crate::discovery::NewObservation;
use crate::discovery::ObservationSummary;
use crate::discovery::PlanetaryProvenance;
use crate::discovery::QualitativeReading;
use crate::world::World;
use super::ObservationTarget;
use super::band_of;
use super::conductivity_of;
use super::experiment_result;
use super::map_survey_condition;
use super::map_survey_strength;
use super::reading_uncertainty;
use super::stability_of;
use super::strength_of;

impl World {
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
}
