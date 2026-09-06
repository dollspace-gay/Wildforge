//! Failure outcomes shared alchemy rules.

use crate::alchemy::AgitationKind;
use crate::alchemy::AlchemyBatch;
use crate::alchemy::BatchFailure;
use crate::alchemy::ProcessStep;

pub(super) fn process_failure(
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

