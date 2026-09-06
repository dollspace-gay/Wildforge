//! Status rules shared alchemy rules.

use crate::alchemy::PreparationHandler;
use crate::arcane::ArcaneOwner;
use crate::arcane::Current;

pub(super) fn incompatible_status_groups(left: &str, right: &str) -> bool {
    matches!(
        (left, right),
        ("base:strain_control", "base:wand_throughput")
            | ("base:wand_throughput", "base:strain_control")
    )
}

pub(super) fn settle_immediate_current(
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

pub(super) fn preparation_color(handler: PreparationHandler) -> [u8; 3] {
    match handler {
        PreparationHandler::TraceSight => [130, 190, 255],
        PreparationHandler::NaturalRecovery | PreparationHandler::RootUptake => [110, 210, 100],
        PreparationHandler::StrainRelief | PreparationHandler::PreserveSpecimen => [170, 210, 255],
        PreparationHandler::DrossWash | PreparationHandler::DrossAntidote => [170, 120, 210],
        PreparationHandler::ThroughputSurge => [120, 180, 255],
    }
}

pub(super) fn debit_nutrition(nutrition: &mut [f32; 5], mut amount: f32) {
    for value in nutrition.iter_mut() {
        if amount <= 0.0 {
            break;
        }
        let debit = value.max(0.0).min(amount);
        *value -= debit;
        amount -= debit;
    }
}
