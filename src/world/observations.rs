//! Bounded host observations. No conservation ledger or simulation state lives here.

use std::collections::HashMap;

use crate::arcane_ecology::EcologyObservation;
use crate::implements::{ApparatusCue, ImplementPublicState};
use crate::planet::{BlockPos, EntityPos, SurfacePos};
use crate::planet_atlas::{AtlasPos, LocalWeatherSample};

#[derive(Default)]
pub(crate) struct ReplicaObservations {
    weather_side: u16,
    weather: HashMap<AtlasPos, LocalWeatherSample>,
    arcane_bands: [u8; 2],
    arcane_dominant: u8,
    ecology: Option<(String, bool)>,
    charges: HashMap<u64, u64>,
    implements: HashMap<u64, ImplementPublicState>,
    apparatus: HashMap<BlockPos, ApparatusCue>,
}

impl ReplicaObservations {
    pub(crate) fn set_weather(&mut self, side: u16, cells: Vec<(AtlasPos, LocalWeatherSample)>) {
        self.weather_side = side;
        self.weather.clear();
        self.weather.extend(cells);
    }

    pub(crate) fn weather_at(&self, position: SurfacePos) -> Option<LocalWeatherSample> {
        if self.weather_side == 0 {
            return None;
        }
        self.weather.get(&AtlasPos::from_surface(position, self.weather_side)).copied()
    }

    pub(crate) fn set_arcane_cue(
        &mut self,
        bands: [u8; 2],
        dominant: u8,
        ecology: Option<(String, bool)>,
    ) {
        self.arcane_bands = bands.map(|band| band.min(4));
        self.arcane_dominant = dominant.min(crate::arcane::BASE_RESONANCES.len() as u8);
        self.ecology = ecology.map(|(mut text, damped)| {
            text.truncate(240);
            (text, damped)
        });
    }

    pub(crate) fn arcane_bands(&self) -> [u8; 2] { self.arcane_bands }
    pub(crate) fn arcane_dominant(&self) -> u8 { self.arcane_dominant }

    pub(crate) fn ecology(&self) -> Option<EcologyObservation> {
        self.ecology.as_ref().map(|(text, damped)| EcologyObservation {
            text: text.clone(), damped: *damped,
        })
    }

    pub(crate) fn charge(&self, id: u64) -> Option<u64> {
        if id == 0 { return None; }
        self.charges.get(&id).copied()
    }

    pub(crate) fn set_charge(&mut self, id: u64, units: u64) {
        if id != 0 { self.charges.insert(id, units); }
    }

    pub(crate) fn implement(&self, id: u64) -> Option<&ImplementPublicState> {
        self.implements.get(&id)
    }

    pub(crate) fn clear_items(&mut self) {
        self.charges.clear();
        self.implements.clear();
        self.apparatus.clear();
    }

    pub(crate) fn extend_charges(&mut self, charges: Vec<(u64, u64)>) {
        self.charges.extend(charges.into_iter().filter(|(id, _)| *id != 0));
    }

    pub(crate) fn extend_implements(&mut self, states: Vec<ImplementPublicState>) {
        self.implements.extend(states.into_iter()
            .filter(|state| state.instance_id != 0).map(|state| (state.instance_id, state)));
    }

    pub(crate) fn extend_apparatus(&mut self, cues: Vec<ApparatusCue>) {
        self.apparatus.extend(cues.into_iter().take(128).map(|cue| (cue.pos, cue)));
    }

    pub(crate) fn apparatus_near(&self, observer: EntityPos, radius: f32) -> Vec<ApparatusCue> {
        let radius = radius.clamp(1.0, 96.0);
        self.apparatus.values().copied()
            .filter(|cue| observer.distance_to(cue.pos.entity_center()) <= radius)
            .take(128).collect()
    }

    #[cfg(test)]
    pub(crate) fn replace_charges(&mut self, charges: Vec<(u64, u64)>) {
        self.charges.clear();
        self.extend_charges(charges);
    }

    #[cfg(test)]
    pub(crate) fn replace_implements(&mut self, states: Vec<ImplementPublicState>) {
        self.implements.clear();
        self.extend_implements(states);
    }

    #[cfg(test)]
    pub(crate) fn replace_apparatus(&mut self, cues: Vec<ApparatusCue>) {
        self.apparatus.clear();
        self.extend_apparatus(cues);
    }
}
