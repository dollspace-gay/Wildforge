//! Shared qualification coordinates and ordered scenario selection.

use crate::planet::FACE_BLOCKS;
use crate::planet_atlas::{AtlasPos, PlanetAtlas};
use serde::Serialize;
use std::collections::BTreeMap;

mod arid_habitats;
mod climate;
mod countries;
mod geology;
mod hydrology;
mod weather;

#[derive(Clone, Debug, PartialEq, Serialize)]
struct QualificationSite {
    face: String,
    atlas_u: u16,
    atlas_v: u16,
    surface_u: u16,
    surface_v: u16,
    spawn: String,
    local_noon: f32,
    evidence: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub(super) struct QualificationSites {
    sites: BTreeMap<String, QualificationSite>,
}

pub(in crate::planet_atlas::diagnostics) fn qualification_sites(
    atlas: &PlanetAtlas,
) -> QualificationSites {
    let mut result = QualificationSites::default();
    let mut insert = |label: &str, pos: AtlasPos, evidence: String| {
        let center = pos.center(atlas.side());
        let surface_u = center.u.round().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16;
        let surface_v = center.v.round().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16;
        let unit = crate::planet::surface_to_unit(center);
        let local_noon =
            (0.25 + unit.x.atan2(unit.z) / std::f64::consts::TAU).rem_euclid(1.0) as f32;
        result.sites.insert(
            label.into(),
            QualificationSite {
                face: pos.face.name().into(),
                atlas_u: pos.u,
                atlas_v: pos.v,
                surface_u,
                surface_v,
                spawn: format!("{},{surface_u},{surface_v}", pos.face.name()),
                local_noon,
                evidence,
            },
        );
    };

    geology::collect(atlas, &mut insert);
    climate::collect(atlas, &mut insert);
    arid_habitats::collect(atlas, &mut insert);
    countries::collect(atlas, &mut insert);
    hydrology::collect(atlas, &mut insert);
    weather::collect(atlas, &mut insert);
    result
}
