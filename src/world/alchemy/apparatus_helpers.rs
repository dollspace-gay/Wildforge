//! Apparatus helpers shared alchemy rules.

use crate::alchemy::AgitationKind;
use crate::alchemy::AlchemyApparatusState;
use crate::alchemy::AlchemyCue;
use crate::alchemy::AlchemyCueKind;
use crate::alchemy::AlchemyError;
use crate::alchemy::AlchemyResult;
use crate::alchemy::ApparatusKind;
use crate::arcane::ArcaneOwner;
use std::collections::BTreeMap;
use crate::alchemy::BatchFailure;
use crate::alchemy::BatchOutcome;
use crate::planet::BlockPos;
use crate::arcane::Current;
use crate::registry::MaterialVector;
use crate::alchemy::ProducedStack;
use super::produced;
use super::APPARATUS_REACH;

pub(super) fn add_current_map(
    map: &mut BTreeMap<ArcaneOwner, Current>,
    owner: ArcaneOwner,
    current: &Current,
) -> Result<(), String> {
    map.entry(owner)
        .or_default()
        .checked_add(current)
        .map_err(|error| error.to_string())
}

pub(super) fn next_block_key<T>(
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

pub(super) fn next_u64_key<T>(map: &std::collections::BTreeMap<u64, T>, cursor: u64) -> Option<u64> {
    use std::ops::Bound::{Excluded, Unbounded};

    map.range((Excluded(cursor), Unbounded))
        .next()
        .or_else(|| map.first_key_value())
        .map(|(key, _)| *key)
}

pub(super) fn ensure_apparatus(
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

pub(super) fn near(a: BlockPos, b: BlockPos) -> bool {
    if a.face() != b.face() {
        return false;
    }
    i32::from(a.u()).abs_diff(i32::from(b.u()))
        + i32::from(a.y()).abs_diff(i32::from(b.y()))
        + i32::from(a.v()).abs_diff(i32::from(b.v()))
        <= APPARATUS_REACH as u32
}

pub(super) fn result_for(
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

