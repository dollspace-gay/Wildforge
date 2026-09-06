//! Observation workings transaction coordination.

use super::working_completion;
use crate::workings::WorkingCue;
use crate::workings::WorkingCueKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingPhase;
use crate::workings::WorkingTransaction;
use crate::world::World;

impl World {
    /// Visible active paths for local rendering and interest-managed network
    /// replication. Exact costs and target snapshots stay host-side.
    pub fn working_cues(&self) -> Vec<WorkingCue> {
        self.workings_state
            .as_ref()
            .into_iter()
            .flat_map(|state| state.active.values())
            .map(|transaction| WorkingCue {
                stable_id: transaction.id,
                working_id: transaction.definition.id.clone(),
                handler: transaction.definition.handler,
                source: transaction.source,
                path: transaction.path.clone(),
                kind: match transaction.phase {
                    WorkingPhase::Charging => WorkingCueKind::Settle,
                    WorkingPhase::Active => WorkingCueKind::Active,
                    WorkingPhase::PendingApply => WorkingCueKind::Complete,
                },
                warning_band: transaction.strain.warning_band,
                completion_permille: working_completion(self.working_tick(), transaction),
            })
            .collect()
    }

    pub(super) fn working_tick(&self) -> u64 {
        (self.calendar_state.clock().max(0.0) * 20.0).round() as u64
    }

    pub(super) fn local_capacity_permille(&self, region: crate::planet_atlas::AtlasPos) -> u16 {
        self.arcane_geography
            .as_ref()
            .map(|geography| {
                let index = region.index(geography.manifest.side);
                let cell = geography.dynamic.cells[index];
                let baseline = geography.controls[index].capacity.max(1);
                u16::try_from(
                    cell.ambient_total()
                        .saturating_mul(1_000)
                        .checked_div(u64::from(baseline))
                        .unwrap_or_default()
                        .min(1_000),
                )
                .unwrap_or(1_000)
            })
            .unwrap_or(1_000)
    }

    /// Bounded, qualitative information unlocked while Trace actually holds
    /// its reserved Current. This intentionally reports neither identities,
    /// exact historic actions, inventories, balances, nor a global map.
    pub(super) fn trace_report(&self, transaction: &WorkingTransaction) -> Result<String, String> {
        let WorkingEffect::Observe {
            origin,
            expires_tick,
        } = transaction.effect
        else {
            return Err("Trace lost its bounded observation target.".into());
        };
        if transaction.phase != WorkingPhase::Active || self.working_tick() > expires_tick {
            return Err("Trace is no longer holding Current around its lens.".into());
        }
        let atlas = self
            .planet_atlas
            .as_ref()
            .ok_or("Trace needs the finite planetary Current atlas.")?;
        let region = atlas.atlas_pos(origin.surface());
        let survey = self
            .arcane_geography
            .as_ref()
            .map(|geography| geography.survey(region, true))
            .ok_or("Trace needs the authoritative Current geography.")?;
        let drift = survey.drift.map_or_else(
            || "no stable drift".to_string(),
            |direction| format!("weak drift {direction:?}").to_ascii_lowercase(),
        );
        let recent_window = 20 * 60 * 5;
        let now = self.working_tick();
        let nearby =
            self.workings_state
                .as_ref()
                .into_iter()
                .flat_map(|state| state.history.iter())
                .filter(|event| {
                    event.working_id != "base:trace"
                        && now.saturating_sub(event.completed_tick) <= recent_window
                        && event.path.iter().any(|pos| {
                            pos.entity_center().distance_to(origin.entity_center()) <= 8.0
                        })
                })
                .collect::<Vec<_>>();
        let traces = match nearby.len() {
            0 => "no recent working trace".to_string(),
            1 => "one faint recent working trace".to_string(),
            count => format!("{} overlapping recent working traces", count.min(9)),
        };
        let local_dross = self.arcane_cue_at(region)[1];
        let warning = nearby
            .iter()
            .map(|event| event.warning_band)
            .max()
            .unwrap_or_default();
        let leakage = match local_dross.max(warning) {
            0 => "no resolved charge leakage",
            1 => "a faint charge leak",
            2 => "a discordant charge leak",
            _ => "a fouled charge leak",
        };
        let uncertainty = survey.uncertainty.saturating_sub(7).max(5);
        Ok(format!(
            "Trace resolves {drift}; {traces}; {leakage}; uncertainty {uncertainty}%."
        ))
    }
}
