//! Link bounded observation, discovery item, and fixture declarations.

use crate::registry::{DiscoveryFixtureDef, DiscoveryItemDef, ObservationDef, qualify};
use crate::registry::schema::{DiscoveryFixtureToml, DiscoveryItemToml, ObservationToml};

pub(in crate::registry) fn observation_def(
    raw: Option<&ObservationToml>,
    mod_id: &str,
    content_id: &str,
) -> Result<Option<ObservationDef>, String> {
    const VISIBLE_PROPERTIES: &[&str] = &[
        "strength",
        "stability",
        "resonance",
        "dross",
        "drift",
        "capacity",
        "conductivity",
        "biological_response",
        "dross_response",
        "condition",
    ];
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.categories.is_empty() || raw.categories.len() > 8 || raw.properties.len() > 12 {
        return Err(format!(
            "{content_id}: observation needs 1..=8 categories and at most 12 visible properties"
        ));
    }
    let categories = raw
        .categories
        .iter()
        .map(|category| {
            let category = if category.contains(':')
                || matches!(
                    category.as_str(),
                    "region"
                        | "block"
                        | "item"
                        | "apparatus"
                        | "heart"
                        | "wake"
                        | "sample"
                        | "echo"
                        | "scar"
                        | "working"
                        | "organism"
                        | "mineral"
                        | "archaeology"
                ) {
                category.clone()
            } else {
                qualify(mod_id, category)
            };
            if category.len() > 64
                || !category.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b':' | b'-')
                })
            {
                return Err(format!(
                    "{content_id}: invalid observation category {category}"
                ));
            }
            Ok(category)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut properties = Vec::new();
    for property in &raw.properties {
        if !VISIBLE_PROPERTIES.contains(&property.as_str()) {
            return Err(format!(
                "{content_id}: observation property {property} is not a qualitative public facet"
            ));
        }
        if !properties.contains(property) {
            properties.push(property.clone());
        }
    }
    Ok(Some(ObservationDef {
        categories,
        properties,
    }))
}

pub(in crate::registry) fn discovery_item_def(
    raw: Option<&DiscoveryItemToml>,
    mod_id: &str,
    content_id: &str,
) -> Result<Option<DiscoveryItemDef>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    const KINDS: &[&str] = &[
        "tuning_lens",
        "lens_frame",
        "field_ledger",
        "survey_folio",
        "artifact",
        "calibration_plate",
        "reference_object",
    ];
    if !KINDS.contains(&raw.kind.as_str()) {
        return Err(format!(
            "{content_id}: unknown discovery item kind {}",
            raw.kind
        ));
    }
    if raw.kind == "artifact" && raw.evidence_class.is_none() {
        return Err(format!("{content_id}: an artifact needs an evidence_class"));
    }
    if raw.authored_text.len() > 16
        || raw
            .authored_text
            .iter()
            .any(|line| line.is_empty() || line.len() > 240 || line.chars().any(char::is_control))
    {
        return Err(format!(
            "{content_id}: artifact phrase tables allow at most 16 bounded printable lines"
        ));
    }
    let evidence_class = raw.evidence_class.as_ref().map(|class| {
        if class.contains(':') || crate::discovery::EVIDENCE_CLASSES.contains(&class.as_str()) {
            class.clone()
        } else {
            qualify(mod_id, class)
        }
    });
    if raw.kind == "calibration_plate" && raw.calibration.is_none() {
        return Err(format!(
            "{content_id}: a calibration plate needs a calibration grade"
        ));
    }
    if raw.kind == "reference_object" && raw.experiment.is_none() {
        return Err(format!(
            "{content_id}: a reference object needs an experiment family"
        ));
    }
    Ok(Some(DiscoveryItemDef {
        kind: raw.kind.clone(),
        evidence_class,
        authored_text: raw.authored_text.clone(),
        calibration: raw.calibration,
        experiment: raw.experiment,
    }))
}

pub(in crate::registry) fn discovery_fixture_def(
    raw: Option<&DiscoveryFixtureToml>,
    content_id: &str,
) -> Result<Option<DiscoveryFixtureDef>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    const KINDS: &[&str] = &[
        "survey_folio",
        "writing_surface",
        "experiment_apparatus",
        "lens_assembly",
    ];
    if !KINDS.contains(&raw.kind.as_str())
        || raw.experiments.len() > crate::discovery::ExperimentKind::ALL.len()
        || raw.record_capacity > crate::discovery::SURVEY_FOLIO_RECORDS as u16
    {
        return Err(format!(
            "{content_id}: invalid or over-budget discovery fixture"
        ));
    }
    if raw.kind == "experiment_apparatus" && raw.experiments.is_empty() {
        return Err(format!(
            "{content_id}: experiment apparatus has no experiments"
        ));
    }
    Ok(Some(DiscoveryFixtureDef {
        kind: raw.kind.clone(),
        experiments: raw.experiments.clone(),
        record_capacity: raw.record_capacity,
    }))
}

