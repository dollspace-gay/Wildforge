//! Artifact custody discovery transaction coordination.

use crate::discovery::CalibrationGrade;
use crate::discovery::DiscoveryError;
use crate::discovery::KnowledgeKind;
use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::world::World;

impl World {
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
}
