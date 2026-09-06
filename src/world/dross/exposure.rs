//! Exposure dross transaction coordination.

use crate::planet::BlockPos;
use crate::world::World;
use std::collections::BTreeSet;

impl World {
    pub(crate) fn environmental_dross_band_at(&self, pos: BlockPos) -> crate::dross::DrossBand {
        let Some(atlas) = self.planet_atlas.as_ref() else {
            return crate::dross::DrossBand::Clear;
        };
        let region = atlas.atlas_pos(pos.surface());
        self.arcane_geography
            .as_ref()
            .map_or(crate::dross::DrossBand::Clear, |geography| {
                geography.dross_band_at(region)
            })
    }

    pub(crate) fn apply_environmental_dross_exposure(
        &mut self,
        actor: [u8; 16],
        pos: BlockPos,
        now: u64,
        band: crate::dross::DrossBand,
        physiology: &mut crate::alchemy::PreparationPhysiology,
        modifiers: &mut crate::alchemy::PreparationModifiers,
    ) {
        let previous = self.dross_exposure_tick.entry(actor).or_insert(now);
        let elapsed = now.saturating_sub(*previous) / 20;
        if elapsed != 0 {
            *previous = previous.saturating_add(elapsed.saturating_mul(20));
            let rate = match band {
                crate::dross::DrossBand::Seep => 1,
                crate::dross::DrossBand::Scar => 2,
                crate::dross::DrossBand::BreachRisk => 4,
                crate::dross::DrossBand::Clear
                | crate::dross::DrossBand::Trace
                | crate::dross::DrossBand::Strained => 0,
            };
            if rate == 0 {
                physiology.bodily_dross = physiology.bodily_dross.saturating_sub(elapsed);
            } else {
                physiology.bodily_dross = physiology
                    .bodily_dross
                    .saturating_add(elapsed.saturating_mul(rate))
                    .min(4_096);
            }
        }
        modifiers.dross_band = band.ordinal();
        modifiers.dross_pattern = band.ordinal();
        let (recovery, perception, stamina) = match band {
            crate::dross::DrossBand::Clear
            | crate::dross::DrossBand::Trace
            | crate::dross::DrossBand::Strained => (1_000, 1_000, 1_000),
            crate::dross::DrossBand::Seep => (850, 950, 950),
            crate::dross::DrossBand::Scar => (650, 850, 800),
            crate::dross::DrossBand::BreachRisk => (450, 700, 650),
        };
        modifiers.recovery_permille = recovery;
        modifiers.perception_permille = perception;
        modifiers.stamina_permille = stamina;
        if band >= crate::dross::DrossBand::Seep {
            for status in self.dross_scar_statuses_at(pos) {
                match status {
                    crate::dross::ScarStatusHandler::RecoveryDrag => {
                        modifiers.recovery_permille =
                            modifiers.recovery_permille.saturating_sub(100);
                    }
                    crate::dross::ScarStatusHandler::PerceptionWarp => {
                        modifiers.perception_permille =
                            modifiers.perception_permille.saturating_sub(100);
                    }
                    crate::dross::ScarStatusHandler::StaminaDrag => {
                        modifiers.stamina_permille = modifiers.stamina_permille.saturating_sub(100);
                    }
                    crate::dross::ScarStatusHandler::WorkingInstability => {
                        modifiers.strain_permille =
                            modifiers.strain_permille.saturating_add(100).min(1_500);
                    }
                }
            }
        }
    }

    pub(super) fn dross_scar_statuses_at(
        &self,
        pos: BlockPos,
    ) -> BTreeSet<crate::dross::ScarStatusHandler> {
        let (Some(atlas), Some(geography)) = (&self.planet_atlas, &self.arcane_geography) else {
            return BTreeSet::new();
        };
        let region = atlas.atlas_pos(pos.surface());
        geography
            .dynamic
            .dross_state
            .scars
            .values()
            .filter(|site| site.region == region && site.resolved_step.is_none())
            .filter_map(|site| {
                self.reg
                    .resolve_dross_scar(&site.content_id, site.kind)
                    .and_then(|definition| definition.status)
            })
            .collect()
    }
}
