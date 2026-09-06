//! Authoritative world integration for physical magical discovery.
use crate::planet::BlockPos;
use crate::discovery::CalibrationGrade;
use crate::discovery::ExperimentKind;
use crate::inventory::ItemStack;
use crate::discovery::StabilityBand;
use crate::discovery::StrengthBand;



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


mod experiments;
mod persistence;
mod artifact_custody;
mod lenses;
mod observation;
mod catalogue;

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
