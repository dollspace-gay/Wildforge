//! Host-authoritative execution of magical workings.
//!
//! This module is the only bridge from declarative working shells to world
//! mutation. Each public entry point constructs one named domain effect; the
//! reservation/settlement machinery never accepts scripts or arbitrary block
//! edits.
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use crate::world::BlockPos;
use crate::arcane::DrossMedium;
use std::collections::VecDeque;
use crate::workings::WorkingEffect;
use crate::workings::WorkingHandler;
use crate::workings::WorkingPhase;
use crate::workings::WorkingTargetSnapshot;
use crate::workings::WorkingTransaction;




const AMBIENT_SAFE_FLOOR: u64 = 64;
const ROOTWAKE_WATER_HU: u64 = crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL;
const ROOTWAKE_NUTRIENT_UNITS: u64 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Settlement {
    Complete,
    Cancel,
    Interrupt,
}

fn ward_local_positions(controller: BlockPos, radius: i32) -> BTreeMap<BlockPos, (i32, i32)> {
    let mut positions = BTreeMap::new();
    for du in -radius..=radius {
        for dv in -radius..=radius {
            if let Some(pos) = controller.offset(du, 0, dv) {
                positions
                    .entry(pos)
                    .and_modify(|saved: &mut (i32, i32)| {
                        *saved = (*saved).min((du, dv));
                    })
                    .or_insert((du, dv));
            }
        }
    }
    positions
}

fn ward_horizontal_neighbors(pos: BlockPos) -> Vec<BlockPos> {
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .filter_map(|(du, dv)| pos.offset(du, 0, dv))
        .collect()
}

/// Return the finite component trapped around the controller, or `None` when
/// the supplied segments do not actually close. This is stricter than a
/// bounding-box test and handles concave player-built boundaries correctly.
fn ward_interior(boundary: &BTreeSet<(i32, i32)>) -> Option<BTreeSet<(i32, i32)>> {
    if boundary.is_empty() || boundary.contains(&(0, 0)) {
        return None;
    }
    let limit = boundary
        .iter()
        .map(|(u, v)| u.abs().max(v.abs()))
        .max()?
        .saturating_add(1);
    let mut interior = BTreeSet::<(i32, i32)>::from([(0, 0)]);
    let mut queue = VecDeque::<(i32, i32)>::from([(0, 0)]);
    while let Some((u, v)) = queue.pop_front() {
        if u.abs() == limit || v.abs() == limit {
            return None;
        }
        for next in [(u + 1, v), (u - 1, v), (u, v + 1), (u, v - 1)] {
            if next.0.abs() <= limit
                && next.1.abs() <= limit
                && !boundary.contains(&next)
                && interior.insert(next)
            {
                queue.push_back(next);
            }
        }
    }
    Some(interior)
}

fn ward_radius(controller: BlockPos, boundary: &[BlockPos]) -> u16 {
    let local = ward_local_positions(controller, 16);
    boundary
        .iter()
        .filter_map(|pos| local.get(pos))
        .map(|(u, v)| u.abs().max(v.abs()) as u16)
        .max()
        .unwrap_or(1)
}

fn inside_ward(
    controller: BlockPos,
    pos: BlockPos,
    segments: &[crate::workings::WardSegment],
) -> bool {
    let radius = ward_radius(
        controller,
        &segments
            .iter()
            .map(|segment| segment.pos)
            .collect::<Vec<_>>(),
    );
    if i32::from(pos.y()).abs_diff(i32::from(controller.y())) > u32::from(radius) {
        return false;
    }
    let local = ward_local_positions(controller, i32::from(radius).saturating_add(1));
    let Some(target) = local.get(&pos.with_y(controller.y())).copied() else {
        return false;
    };
    let Some(boundary) = segments
        .iter()
        .map(|segment| local.get(&segment.pos).copied())
        .collect::<Option<BTreeSet<_>>>()
    else {
        return false;
    };
    ward_interior(&boundary).is_some_and(|interior| interior.contains(&target))
}

fn reservoir_from_snapshots(
    snapshots: &[WorkingTargetSnapshot],
    pos: BlockPos,
) -> Option<crate::planet_atlas::ReservoirMass> {
    snapshots.iter().find_map(|snapshot| match snapshot {
        WorkingTargetSnapshot::Reservoir {
            pos: at,
            water_hu,
            salt_mass,
            ..
        } if *at == pos => Some(crate::planet_atlas::ReservoirMass {
            water_hu: *water_hu,
            salt_mass: *salt_mass,
        }),
        _ => None,
    })
}

fn carrier_from_snapshots(
    snapshots: &[WorkingTargetSnapshot],
    pos: BlockPos,
) -> Option<crate::workings::WaterCarrier> {
    snapshots.iter().find_map(|snapshot| match snapshot {
        WorkingTargetSnapshot::Reservoir {
            pos: at,
            thermal_millic_hu,
            dross_units,
            carrier_remainder,
            ..
        } if *at == pos => Some(crate::workings::WaterCarrier {
            thermal_millic_hu: *thermal_millic_hu,
            dross_subunits: dross_units
                .checked_mul(256)?
                .checked_add(*carrier_remainder)?,
        }),
        _ => None,
    })
}

fn validate_water_carrier(
    mass: crate::planet_atlas::ReservoirMass,
    carrier: crate::workings::WaterCarrier,
) -> Result<(), String> {
    if (mass.water_hu == 0 && carrier != crate::workings::WaterCarrier::default())
        || mass.water_hu > crate::planet_atlas::HYDRO_UNITS_PER_BLOCK
        || carrier.thermal_millic_hu.unsigned_abs() > mass.water_hu.saturating_mul(100_000)
        || carrier.dross_subunits > u64::from(u32::MAX).saturating_mul(256)
    {
        return Err("A detailed water carrier is unbounded or detached from water.".into());
    }
    Ok(())
}

fn repair_inventory_slots(transaction: &WorkingTransaction) -> Result<(usize, usize), String> {
    let WorkingEffect::RepairItem {
        item_id,
        repair_material,
        ..
    } = &transaction.effect
    else {
        return Err("That transaction is not an inventory repair.".into());
    };
    let mut target = None;
    let mut material = None;
    for snapshot in &transaction.targets {
        if let WorkingTargetSnapshot::Item {
            stable_id,
            item_name,
            version,
            ..
        } = snapshot
        {
            let slot = usize::try_from(*version)
                .map_err(|_| "Saved Fieldmend inventory slot overflowed.")?;
            if *stable_id == *item_id {
                target = Some(slot);
            } else if item_name == repair_material {
                material = Some(slot);
            }
        }
    }
    let target = target.ok_or("Fieldmend target slot snapshot is missing.")?;
    let material = material.ok_or("Fieldmend material slot snapshot is missing.")?;
    if target >= crate::inventory::TOTAL_SLOTS
        || material >= crate::inventory::TOTAL_SLOTS
        || target == material
    {
        return Err("Fieldmend saved invalid inventory slots.".into());
    }
    Ok((target, material))
}

fn dross_medium(handler: WorkingHandler) -> DrossMedium {
    match handler {
        WorkingHandler::Draw | WorkingHandler::Rootwake | WorkingHandler::RootingBed => {
            DrossMedium::Water
        }
        WorkingHandler::Ignite
        | WorkingHandler::Nudge
        | WorkingHandler::Trace
        | WorkingHandler::Gleam
        | WorkingHandler::Holdfast
        | WorkingHandler::WardBoundary => DrossMedium::Air,
        WorkingHandler::Fieldmend
        | WorkingHandler::SettlingRite
        | WorkingHandler::TransferCircle => DrossMedium::Soil,
    }
}

fn effect_path(effect: &WorkingEffect) -> Vec<BlockPos> {
    match effect {
        WorkingEffect::Observe { origin, .. } => vec![*origin],
        WorkingEffect::PointLight { source, target, .. } => vec![*source, *target],
        WorkingEffect::Ignite {
            fuel, fire_cell, ..
        } => vec![*fuel, *fire_cell],
        WorkingEffect::AdvancePlant(advance) => {
            let mut path = vec![advance.pos];
            path.extend(advance.soil_pos);
            path.extend(advance.water_source);
            path
        }
        WorkingEffect::TransferWater { from, to, .. } => vec![*from, *to],
        WorkingEffect::Settle { controller, .. } => vec![*controller],
        WorkingEffect::AdvanceBed {
            controller, plants, ..
        } => std::iter::once(*controller)
            .chain(plants.iter().map(|plant| plant.pos))
            .collect(),
        WorkingEffect::Ward {
            controller,
            segments,
            ..
        } => std::iter::once(*controller)
            .chain(segments.iter().map(|segment| segment.pos))
            .collect(),
        WorkingEffect::Impulse { source, target, .. }
        | WorkingEffect::OperateMechanism { source, target, .. } => vec![*source, *target],
        WorkingEffect::RepairItem { .. }
        | WorkingEffect::Preserve { .. }
        | WorkingEffect::TransferCurrent { .. } => Vec::new(),
    }
}

fn working_completion(tick: u64, transaction: &WorkingTransaction) -> u16 {
    let duration = transaction
        .due_tick
        .saturating_sub(transaction.started_tick);
    if duration == 0 {
        return if transaction.phase == WorkingPhase::Charging {
            0
        } else {
            1_000
        };
    }
    u16::try_from(
        tick.saturating_sub(transaction.started_tick)
            .min(duration)
            .saturating_mul(1_000)
            / duration,
    )
    .unwrap_or(1_000)
}

fn working_distance(from: BlockPos, to: BlockPos) -> u16 {
    let distance = from
        .entity_center()
        .render_pos()
        .distance(to.entity_center().render_pos())
        .ceil();
    if !distance.is_finite() || distance <= 0.0 {
        0
    } else {
        distance.min(f32::from(u16::MAX)) as u16
    }
}

fn vec3_milli(value: glam::Vec3) -> [i32; 3] {
    value
        .to_array()
        .map(|component| (component * 1_000.0).round().clamp(-80_000.0, 80_000.0) as i32)
}

fn milli_vec3(value: [i32; 3]) -> glam::Vec3 {
    glam::Vec3::from_array(value.map(|component| component as f32 / 1_000.0))
}

fn inventory_target_id(actor: [u8; 16], slot: usize, item: u16) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in actor
        .into_iter()
        .chain((slot as u64).to_le_bytes())
        .chain(item.to_le_bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash.max(1)
}

fn mounted_target_id(pos: BlockPos, bay: u8, item: u16) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in [pos.face() as u8]
        .into_iter()
        .chain(pos.u().to_le_bytes())
        .chain([pos.y()])
        .chain(pos.v().to_le_bytes())
        .chain([bay])
        .chain(item.to_le_bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash.max(1)
}


mod wand_admission;
mod trace_gleam;
mod kindle;
mod nudge;
mod fieldmend;
mod holdfast;
mod transfer_ritual;
mod settling_rite;
mod ritual_dispatch;
mod rooting_bed;
mod ward;
mod draw;
mod rootwake;
mod lifecycle;
mod inventory_completion;
mod preservation;
mod fragile_custody;
mod observation;
mod ritual_layout;
mod ritual_reservation;
mod wand_reservation;
mod settlement;
mod target_validation;
mod physical_effects;
mod effect_persistence;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dross_media_are_explicit_for_every_native_handler() {
        for handler in WorkingHandler::ALL {
            let _ = dross_medium(handler);
        }
    }
}
