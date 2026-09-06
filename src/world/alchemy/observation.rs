//! Observation alchemy transaction coordination.

use crate::alchemy::BatchOutcome;
use crate::world::BlockPos;
use crate::inventory::ItemStack;
use crate::alchemy::PreparationHandler;
use crate::alchemy::PreparationModifiers;
use crate::world::World;

impl World {
    pub fn alchemy_state(&self) -> Option<&crate::alchemy::AlchemyState> {
        self.alchemy_state.as_ref()
    }

    /// Owner-visible state for one filled preparation card. Without a tuning
    /// lens the Current/dross reading remains qualitative, matching apparatus
    /// sampling rather than leaking exact hidden ledger values.
    pub fn preparation_tooltip(&self, stack: ItemStack, has_lens: bool) -> Vec<String> {
        let Some(dose) = self
            .alchemy_state
            .as_ref()
            .and_then(|state| state.containers.get(&stack.arcane_id))
            .filter(|dose| dose.item_name == self.reg.item(stack.item).name)
        else {
            return Vec::new();
        };
        let now = self.alchemy_tick();
        let condition = if now >= dose.expires_tick || dose.outcome == BatchOutcome::Spoiled {
            "SPOILED — SPENT LIQUOR / DISPOSAL ONLY".to_string()
        } else {
            match dose.outcome {
                BatchOutcome::Ready => "BATCH CONDITION: READY".into(),
                BatchOutcome::Failed(failure) => format!("FAILED BATCH: {failure:?}"),
                BatchOutcome::Processing => "INVALID UNFINISHED CONTAINER".into(),
                BatchOutcome::Spoiled => "SPOILED — SPENT LIQUOR / DISPOSAL ONLY".into(),
            }
        };
        let remaining_ticks = dose.expires_tick.saturating_sub(now);
        let remaining_days = remaining_ticks as f32 / 20.0 / crate::server::DAY_LENGTH.max(1.0);
        let mut lines = vec![
            condition,
            format!("ONE EXACT {}-UNIT DOSE", dose.liquid.volume_units),
            if remaining_days < 1.0 {
                "LIFE: UNDER ONE DAY".into()
            } else {
                format!("LIFE: ABOUT {} DAYS", remaining_days.floor() as u64)
            },
        ];
        let total = dose.current_units.saturating_add(dose.dross_units);
        if has_lens {
            lines.push(format!(
                "LENS: {} CURRENT / {} DROSS",
                crate::arcane::qualitative_current(total, crate::alchemy::MAX_PREPARATION_CHARGE)
                    .to_uppercase(),
                if dose.dross_units == 0 {
                    "CLEAR"
                } else if dose.dross_units.saturating_mul(4) <= total.max(1) {
                    "TRACE"
                } else {
                    "TURBID"
                }
            ));
        } else {
            lines.push("A TUNING LENS READS CHARGE + DROSS CONDITION".into());
        }
        lines
    }

    /// Read the bounded modifiers currently attached to one authoritative
    /// actor. This exposes only approved handler outputs; callers never see
    /// or reinterpret a preparation's private mixture.
    pub fn preparation_modifiers(&self, actor: [u8; 16]) -> PreparationModifiers {
        let now = self.alchemy_tick();
        let mut modifiers = PreparationModifiers::default();
        let Some(state) = &self.alchemy_state else {
            return modifiers;
        };
        for status in state.statuses.get(&actor).into_iter().flatten() {
            if now >= status.due_tick {
                continue;
            }
            let Some(definition) = self.reg.preparations.get(&status.preparation_id) else {
                continue;
            };
            match definition.handler {
                PreparationHandler::TraceSight => {
                    modifiers.trace_sight = modifiers
                        .trace_sight
                        .max(definition.effect.strength.min(u32::from(u16::MAX)) as u16);
                }
                PreparationHandler::StrainRelief => {
                    modifiers.strain_permille = modifiers
                        .strain_permille
                        .min(1_000u16.saturating_sub(definition.effect.strength.min(750) as u16));
                    modifiers.throughput_permille = modifiers
                        .throughput_permille
                        .min(definition.effect.throughput_permille);
                }
                PreparationHandler::ThroughputSurge => {
                    modifiers.throughput_permille = modifiers
                        .throughput_permille
                        .max(definition.effect.throughput_permille);
                    modifiers.drain_permille = modifiers
                        .drain_permille
                        .max(definition.effect.drain_permille);
                    modifiers.overdraw_permille = modifiers
                        .overdraw_permille
                        .max(definition.effect.overdraw_permille);
                    modifiers.storm_warning = true;
                }
                PreparationHandler::NaturalRecovery
                | PreparationHandler::RootUptake
                | PreparationHandler::DrossWash
                | PreparationHandler::PreserveSpecimen
                | PreparationHandler::DrossAntidote => {}
            }
        }
        modifiers
    }

    /// Bounded Root Wash uptake factor for an already viable plot. All
    /// ordinary light, season, temperature, moisture, soil, seed, and nutrient
    /// gates still run; the wash cannot create growth inputs or set age.
    pub fn root_uptake_multiplier_at(&self, plant: BlockPos) -> f32 {
        let now = self.alchemy_tick();
        let Some(state) = &self.alchemy_state else {
            return 1.0;
        };
        let treatment = state.root_treatments.get(&plant).or_else(|| {
            plant
                .offset(0, -1, 0)
                .and_then(|soil| state.root_treatments.get(&soil))
        });
        let Some(treatment) = treatment.filter(|treatment| now < treatment.expires_tick) else {
            return 1.0;
        };
        if treatment.concentration_permille > 1_000 {
            return (2_000u16.saturating_sub(treatment.concentration_permille) as f32 / 1_000.0)
                .clamp(0.1, 1.0);
        }
        (1.0 + f32::from(treatment.uptake_permille) / 1_000.0).clamp(1.0, 2.0)
    }

    /// Apply Frostlace's bounded rate to one elapsed-age decrement. Returning
    /// a smaller positive decrement can slow age; it can never return a
    /// negative value or add freshness.
    pub fn coated_specimen_age_advance(
        &mut self,
        item_id: u64,
        ordinary_ticks: u64,
        temperature_millic: i32,
    ) -> u64 {
        if ordinary_ticks == 0 {
            return 0;
        }
        let now = self.alchemy_tick();
        let Some(state) = &mut self.alchemy_state else {
            return ordinary_ticks;
        };
        let Some(coating) = state.coatings.get_mut(&item_id) else {
            return ordinary_ticks;
        };
        if now >= coating.expires_tick || temperature_millic > coating.maximum_temperature_millic {
            return ordinary_ticks;
        }
        let advanced = u64::try_from(
            u128::from(ordinary_ticks) * u128::from(coating.preservation_permille) / 1_000,
        )
        .unwrap_or(ordinary_ticks)
        .max(1)
        .min(ordinary_ticks);
        coating.age_paid = coating.age_paid.saturating_add(ordinary_ticks - advanced);
        advanced
    }
}
