//! Conservative atmosphere transport stencils and bounded overflow redistribution.

use glam::DVec3;
use crate::planet_atlas::{AtlasPos, DynamicCell, AtlasError};
use crate::planet::surface_to_unit;

pub(super) fn chart_components(pos: AtlasPos, side: u16, vector: DVec3) -> [f32; 2] {
    let frame = crate::planet::local_frame(pos.center(side));
    [
        vector.dot(frame.east) as f32,
        vector.dot(frame.north) as f32,
    ]
}

pub(crate) fn chart_vector(pos: AtlasPos, side: u16, components: [f32; 2]) -> DVec3 {
    let frame = crate::planet::local_frame(pos.center(side));
    (frame.east * f64::from(components[0]) + frame.north * f64::from(components[1]))
        .normalize_or_zero()
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct TransportTarget {
    pub(super) index: u32,
    pub(super) weight: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct TransportStencil {
    pub(super) targets: [TransportTarget; 4],
    pub(super) len: u8,
}

/// Conservative semi-Lagrangian stencil over the forward half-plane. A
/// single winning neighbor turns convergent flow into pixel-width moisture
/// rails; sharing flux across the 3–4 forward neighbors gives fronts and rain
/// shadows an atmospheric width while preserving every unit.
pub(super) fn transport_stencil(pos: AtlasPos, side: u16, wind: DVec3) -> TransportStencil {
    let here = surface_to_unit(pos.center(side));
    let mut candidates: Vec<(f64, usize)> = pos
        .neighbors8(side)
        .into_iter()
        .filter_map(|candidate| {
            let unit = surface_to_unit(candidate.center(side));
            let tangent = (unit - here * unit.dot(here)).normalize_or_zero();
            let score = wind.dot(tangent);
            (score > 1.0e-6).then_some((score * score, candidate.index(side)))
        })
        .collect();
    candidates.sort_by(|(a_score, a_index), (b_score, b_index)| {
        b_score
            .total_cmp(a_score)
            .then_with(|| a_index.cmp(b_index))
    });
    candidates.truncate(4);
    if candidates.is_empty() {
        return TransportStencil {
            targets: [TransportTarget {
                index: pos.index(side) as u32,
                weight: 1.0,
            }; 4],
            len: 1,
        };
    }
    let total = candidates.iter().map(|(score, _)| *score).sum::<f64>();
    let mut stencil = TransportStencil {
        len: candidates.len() as u8,
        ..TransportStencil::default()
    };
    for (slot, (score, index)) in candidates.into_iter().enumerate() {
        stencil.targets[slot] = TransportTarget {
            index: index as u32,
            weight: (score / total) as f32,
        };
    }
    stencil
}

pub(super) fn distribute_u32(amount: u32, stencil: TransportStencil, mut add: impl FnMut(usize, u32)) {
    let len = usize::from(stencil.len);
    let mut remaining = amount;
    for (slot, target) in stencil.targets[..len].iter().enumerate() {
        let share = if slot + 1 == len {
            remaining
        } else {
            ((amount as f64 * f64::from(target.weight)).floor() as u32).min(remaining)
        };
        remaining -= share;
        add(target.index as usize, share);
    }
}

pub(super) fn spill_vapor(cells: &mut [DynamicCell], start: usize, mut amount: u32) -> Result<(), AtlasError> {
    for offset in 0..cells.len() {
        let index = (start + offset) % cells.len();
        let room = u32::MAX - cells[index].atmospheric_vapor;
        let accepted = amount.min(room);
        cells[index].atmospheric_vapor += accepted;
        amount -= accepted;
        if amount == 0 {
            return Ok(());
        }
    }
    Err(AtlasError::Corrupt(
        "whole-planet vapor storage capacity exhausted".into(),
    ))
}

pub(super) fn spill_cloud(cells: &mut [DynamicCell], start: usize, mut amount: u32) -> Result<(), AtlasError> {
    for offset in 0..cells.len() {
        let index = (start + offset) % cells.len();
        let room = u32::MAX - cells[index].cloud_water;
        let accepted = amount.min(room);
        cells[index].cloud_water += accepted;
        amount -= accepted;
        if amount == 0 {
            return Ok(());
        }
    }
    Err(AtlasError::Corrupt(
        "whole-planet cloud storage capacity exhausted".into(),
    ))
}

pub(super) fn add_i16(destination: &mut i16, amount: i32) {
    *destination =
        (i32::from(*destination) + amount).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
}
