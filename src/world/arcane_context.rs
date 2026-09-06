//! Bounded observations and ward inputs borrowed from existing Current owners.
//! This context cannot change Current, write saves, or mutate the physical world.

use crate::arcane::ArcaneLedger;
use crate::arcane_geography::{ArcaneGeography, ArcaneSurvey};
use crate::planet::BlockPos;
use crate::planet_atlas::{AtlasPos, PlanetAtlas};
use crate::workings::WorkingsState;

pub(super) struct ArcaneEnvironment<'a> {
    pub(super) geography: Option<&'a ArcaneGeography>,
    pub(super) ledger: Option<&'a ArcaneLedger>,
}

pub(super) struct WardPressure {
    pub(super) controller: BlockPos,
    pub(super) wake: u64,
    pub(super) dross: u64,
}

impl ArcaneEnvironment<'_> {
    pub(super) fn cue(&self, region: AtlasPos) -> [u8; 2] {
        let geographic = self
            .geography
            .as_ref()
            .map_or([0; 2], |geography| geography.local_bands(region));
        let sparse = self
            .ledger
            .as_ref()
            .map_or([0; 2], |ledger| ledger.local_bands(region));
        [geographic[0].max(sparse[0]), geographic[1].max(sparse[1])]
    }

    pub(super) fn sensory_cue(&self, region: AtlasPos) -> ([u8; 2], u8) {
        let bands = self.cue(region);
        let dominant = self
            .geography
            .as_ref()
            .map_or(0, |geography| geography.local_dominant_resonance(region));
        (bands, dominant)
    }

    pub(super) fn survey(&self, region: AtlasPos, tuning_lens: bool) -> Option<ArcaneSurvey> {
        self.geography
            .as_ref()
            .map(|geography| geography.survey(region, tuning_lens))
    }

    pub(super) fn item_current(&self, id: u64) -> Option<u64> {
        if id == 0 {
            return None;
        }
        self.ledger
            .as_ref()
            .and_then(|ledger| ledger.item_clean_total(id))
    }

    pub(super) fn ward_pressures(
        &self,
        atlas: &PlanetAtlas,
        workings: Option<&WorkingsState>,
    ) -> Vec<WardPressure> {
        let controllers = workings
            .into_iter()
            .flat_map(|state| state.active.values())
            .filter_map(|transaction| {
                if transaction.phase != crate::workings::WorkingPhase::Active {
                    return None;
                }
                match &transaction.effect {
                    crate::workings::WorkingEffect::Ward { controller, .. } => Some(*controller),
                    _ => None,
                }
            })
            .collect::<Vec<_>>();
        let pressures = controllers
            .into_iter()
            .filter_map(|controller| {
                let region = atlas.atlas_pos(controller.surface());
                let geography = self.geography?;
                let cell = geography
                    .dynamic
                    .cells
                    .get(region.index(geography.manifest.side))?;
                let wake = if cell.wake_id != 0 {
                    geography
                        .dynamic
                        .wakes
                        .iter()
                        .find(|wake| wake.id == u64::from(cell.wake_id))
                        .map(|wake| {
                            wake.charge
                                .into_iter()
                                .chain(wake.dross)
                                .map(u64::from)
                                .sum::<u64>()
                        })
                        .unwrap_or_default()
                } else {
                    0
                };
                let sparse_dross = self.ledger.map_or(0, |ledger| {
                    [
                        crate::arcane::DrossMedium::Soil,
                        crate::arcane::DrossMedium::Water,
                        crate::arcane::DrossMedium::Air,
                    ]
                    .into_iter()
                    .filter_map(|medium| {
                        ledger.account(&crate::arcane::ArcaneOwner::Dross { region, medium })
                    })
                    .fold(0u64, |sum, account| {
                        sum.saturating_add(account.current.total())
                    })
                });
                Some((
                    controller,
                    wake,
                    cell.dross_total().saturating_add(sparse_dross),
                ))
            })
            .collect::<Vec<_>>();
        let bounded = |units: u64| {
            if units == 0 {
                0
            } else {
                units.div_ceil(64).clamp(1, 32)
            }
        };
        pressures
            .into_iter()
            .map(|(controller, wake, dross)| WardPressure {
                controller,
                wake: bounded(wake),
                dross: bounded(dross),
            })
            .collect()
    }
}
