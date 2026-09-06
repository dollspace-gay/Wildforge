//! Discovery context for authenticated host operations.

use super::{BlockEntity, BlockPos, EntityPos, Guest, REACH, Vec3, World, net};

pub(super) fn discovery_reachable(world: &World, guest: &Guest, pos: BlockPos) -> bool {
    discovery_reachable_from(world, guest.pos, pos)
}

pub(super) fn discovery_reachable_from(world: &World, actor: EntityPos, pos: BlockPos) -> bool {
    if actor.distance_to(pos.entity_center()) > REACH {
        return false;
    }
    let Ok(eye) = actor.translated(Vec3::new(0.0, crate::physics::EYE_HEIGHT, 0.0)) else {
        return false;
    };
    let delta = eye.pos.local_delta_to(pos.entity_center());
    crate::raycast::raycast_at(world, eye.pos, delta, delta.length() + 0.15)
        .is_some_and(|hit| hit.block == pos)
}

pub(super) fn discovery_holder_id(
    world: &mut World,
    guest: &mut Guest,
    holder: net::RecordHolderSnap,
) -> Result<u64, String> {
    match holder {
        net::RecordHolderSnap::Inventory { slot } => {
            let index = usize::from(slot);
            let mut stack = guest
                .inventory
                .slots
                .get(index)
                .copied()
                .flatten()
                .ok_or_else(|| "That pack slot is empty.".to_string())?;
            let is_record_holder =
                world
                    .reg
                    .item(stack.item)
                    .discovery
                    .as_ref()
                    .is_some_and(|definition| {
                        matches!(definition.kind.as_str(), "field_ledger" | "survey_folio")
                    });
            if !is_record_holder {
                return Err("That item cannot hold observations.".into());
            }
            let at = guest
                .pos
                .block()
                .ok_or_else(|| "Your position is outside the world.".to_string())?;
            world
                .bind_discovery_stack_at(at, &mut stack)
                .map_err(|error| error.to_string())?;
            guest.inventory.slots[index] = Some(stack);
            Ok(stack.arcane_id)
        }
        net::RecordHolderSnap::Folio { pos } => {
            if !discovery_reachable(world, guest, pos) {
                return Err("The folio is out of reach or sight.".into());
            }
            let is_folio = world
                .reg
                .block(world.get_block_at(pos))
                .discovery_fixture
                .as_ref()
                .is_some_and(|fixture| fixture.kind == "survey_folio");
            if !is_folio {
                return Err("There is no survey folio there.".into());
            }
            match world.block_entities().find(|(at, _)| **at == pos) {
                Some((_, BlockEntity::SurveyFolio(folio))) if folio.object_id != 0 => {
                    Ok(folio.object_id)
                }
                _ => Err("That survey folio has no recoverable record identity.".into()),
            }
        }
    }
}

pub(super) fn discovery_holder_capacity(
    world: &World,
    guest: &Guest,
    holder: net::RecordHolderSnap,
) -> usize {
    match holder {
        net::RecordHolderSnap::Folio { .. } => crate::discovery::SURVEY_FOLIO_RECORDS,
        net::RecordHolderSnap::Inventory { slot } => {
            if guest
                .inventory
                .slots
                .get(usize::from(slot))
                .and_then(Option::as_ref)
                .and_then(|stack| world.reg.item(stack.item).discovery.as_ref())
                .is_some_and(|definition| definition.kind == "survey_folio")
            {
                crate::discovery::SURVEY_FOLIO_RECORDS
            } else {
                crate::discovery::FIELD_LEDGER_RECORDS
            }
        }
    }
}

pub(super) fn discovery_holder_at_writing_surface(
    holder: net::RecordHolderSnap,
    writing_pos: BlockPos,
) -> bool {
    match holder {
        net::RecordHolderSnap::Inventory { .. } => true,
        net::RecordHolderSnap::Folio { pos } => [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .into_iter()
            .filter_map(|(du, dv)| writing_pos.offset(du, 0, dv))
            .any(|adjacent| adjacent == pos),
    }
}

pub(super) fn discovery_calibration(
    world: &mut World,
    guest: &mut Guest,
    slot: Option<u8>,
) -> Result<crate::discovery::CalibrationGrade, String> {
    let Some(slot) = slot else {
        return Ok(crate::discovery::CalibrationGrade::Field);
    };
    let index = usize::from(slot);
    let mut stack = guest
        .inventory
        .slots
        .get(index)
        .copied()
        .flatten()
        .ok_or_else(|| "That calibration slot is empty.".to_string())?;
    let at = guest
        .pos
        .block()
        .ok_or_else(|| "Your position is outside the world.".to_string())?;
    world
        .bind_discovery_stack_at(at, &mut stack)
        .map_err(|error| error.to_string())?;
    let calibration = world
        .calibration_grade_for(stack)
        .ok_or_else(|| "That is not a calibration plate.".to_string())?;
    guest.inventory.slots[index] = Some(stack);
    Ok(calibration)
}
