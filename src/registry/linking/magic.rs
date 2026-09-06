//! Link declarative conserved Current and ecology contracts.

use crate::registry::schema::{ArcaneContentToml, ArcaneEcologyToml};
use crate::registry::{
    ArcaneContentDef, ArcaneEcologyDef, ArcaneEcologyKind, EcologyRole, EcologySource,
    ReproductionMode, qualify,
};
use std::collections::BTreeMap;

pub(in crate::registry) fn arcane_def(
    raw: Option<&ArcaneContentToml>,
    mod_id: &str,
    content_id: &str,
    registry: &crate::arcane::ResonanceRegistry,
) -> Result<Option<ArcaneContentDef>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.capacity == 0 {
        return Err(format!("{content_id}: arcane capacity must be positive"));
    }
    if raw.conductivity > 1_000 || raw.stability > 1_000 {
        return Err(format!(
            "{content_id}: arcane conductivity and stability must be integer permille in 0..=1000"
        ));
    }
    if raw.resonance.is_empty() {
        return Err(format!(
            "{content_id}: arcane resonance mixture is required"
        ));
    }
    let mut resonance = BTreeMap::<String, u16>::new();
    let mut total = 0u32;
    for (name, weight) in &raw.resonance {
        if *weight == 0 {
            return Err(format!("{content_id}: resonance {name} has zero weight"));
        }
        let name = qualify(mod_id, name);
        if !registry.definitions.contains_key(&name) {
            return Err(format!("{content_id}: unknown resonance {name}"));
        }
        total = total
            .checked_add(u32::from(*weight))
            .ok_or_else(|| format!("{content_id}: resonance weights overflow"))?;
        resonance.insert(name, *weight);
    }
    if total == 0 || total > u32::from(u16::MAX) {
        return Err(format!(
            "{content_id}: resonance weight sum must fit a positive u16"
        ));
    }
    Ok(Some(ArcaneContentDef {
        capacity: raw.capacity,
        conductivity_permille: raw.conductivity,
        stability_permille: raw.stability,
        resonance,
        on_destroy: raw.on_destroy,
    }))
}

pub(in crate::registry) fn arcane_ecology_def(
    raw: Option<&ArcaneEcologyToml>,
    mod_id: &str,
    content_id: &str,
    registry: &crate::arcane::ResonanceRegistry,
) -> Result<Option<ArcaneEcologyDef>, String> {
    const HABITAT_PREDICATES: &[&str] = &[
        "arid_spring",
        "cave",
        "cool_night",
        "cool_or_temperate",
        "dross_margin",
        "evaporite_host",
        "exposed",
        "fertile",
        "fire_disturbed",
        "freshwater_margin",
        "heartshadow",
        "host_rock",
        "mafic_host",
        "marine",
        "metamorphic_host",
        "moist_cave",
        "nutrient_rich",
        "old_forest",
        "old_organic",
        "permanent_cold",
        "rocky_soil",
        "sedimentary_host",
        "storm_exposed",
        "subsurface",
        "swamp",
        "temperate_ground",
        "warm_wet",
        "wetland",
    ];
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.roles.is_empty() {
        return Err(format!(
            "{content_id}: arcane ecology needs at least one causal role"
        ));
    }
    let unique = raw
        .roles
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    if unique.len() != raw.roles.len() {
        return Err(format!(
            "{content_id}: arcane ecology roles contain duplicates"
        ));
    }
    if raw.habitat.is_empty()
        || raw.habitat.iter().any(|tag| {
            tag.is_empty()
                || tag.len() > 48
                || !tag
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
    {
        return Err(format!(
            "{content_id}: arcane ecology requires lowercase ordinary habitat predicates"
        ));
    }
    if let Some(unknown) = raw
        .habitat
        .iter()
        .find(|tag| !HABITAT_PREDICATES.contains(&tag.as_str()))
    {
        return Err(format!(
            "{content_id}: unknown or unreachable ecology habitat predicate {unknown}"
        ));
    }
    if raw.charge_capacity == 0
        || raw.carrying_capacity == 0
        || raw.carrying_capacity > 4_096
        || raw.min_stability > raw.max_stability
        || raw.max_stability > 1_000
        || raw.min_richness > 1_000
    {
        return Err(format!(
            "{content_id}: ecology capacity/carrying/stability/richness bounds are invalid"
        ));
    }
    if !raw.seasons.into_iter().any(|active| active) {
        return Err(format!("{content_id}: ecology has no active growth season"));
    }
    if raw.resonance.is_empty() {
        return Err(format!(
            "{content_id}: ecology requires a resonance mixture"
        ));
    }
    let mut resonance = BTreeMap::new();
    let mut total = 0u32;
    for (name, weight) in &raw.resonance {
        let qualified = qualify(mod_id, name);
        if *weight == 0 || !registry.definitions.contains_key(&qualified) {
            return Err(format!(
                "{content_id}: unknown or zero-weight ecology resonance {qualified}"
            ));
        }
        if !crate::arcane::BASE_RESONANCES.contains(&qualified.as_str()) {
            return Err(format!(
                "{content_id}: ecology resonance {qualified} has no planetary geographic band"
            ));
        }
        total = total
            .checked_add(u32::from(*weight))
            .ok_or_else(|| format!("{content_id}: ecology resonance weights overflow"))?;
        resonance.insert(qualified, *weight);
    }
    if total == 0 || total > u32::from(u16::MAX) {
        return Err(format!(
            "{content_id}: ecology resonance mixture is invalid"
        ));
    }
    match raw.kind {
        ArcaneEcologyKind::Organism => {
            if raw.reproduction == ReproductionMode::None
                || raw.water_per_day_hu == 0
                || raw.nutrient_per_day == 0
            {
                return Err(format!(
                    "{content_id}: organism growth needs reproduction, nutrients, and a declared water demand"
                ));
            }
            if raw.crystal_stages != 0 || raw.preserving_tool_tier != 0 {
                return Err(format!(
                    "{content_id}: only crystals may declare stages or a preserving tool tier"
                ));
            }
        }
        ArcaneEcologyKind::Crystal => {
            if raw.reproduction != ReproductionMode::Bud
                || !(2..=8).contains(&raw.crystal_stages)
                || raw.preserving_tool_tier == 0
                || raw.uptake_per_day == 0
            {
                return Err(format!(
                    "{content_id}: crystals need bud reproduction, 2..=8 exact stages, uptake, and a preserving tool tier"
                ));
            }
        }
        ArcaneEcologyKind::FiniteMineral => {
            if raw.reproduction != ReproductionMode::None
                || raw.uptake_per_day != 0
                || raw.release_per_day != 0
                || raw.crystal_stages != 0
            {
                return Err(format!(
                    "{content_id}: finite minerals cannot reproduce, grow, release, or declare crystal stages"
                ));
            }
        }
    }
    if raw.uptake_per_day == 0
        && unique.contains(&EcologyRole::Gatherer)
        && raw.kind != ArcaneEcologyKind::FiniteMineral
    {
        return Err(format!(
            "{content_id}: a gatherer cannot have free zero-uptake growth"
        ));
    }
    if raw.source == EcologySource::Dross && !unique.contains(&EcologyRole::Transformer) {
        return Err(format!(
            "{content_id}: dross uptake requires the transformer role"
        ));
    }
    Ok(Some(ArcaneEcologyDef {
        roles: raw.roles.clone(),
        kind: raw.kind,
        habitat: raw.habitat.clone(),
        charge_capacity: raw.charge_capacity,
        uptake_per_day: raw.uptake_per_day,
        release_per_day: raw.release_per_day,
        source: raw.source,
        resonance,
        dross_tolerance: raw.dross_tolerance,
        water_per_day_hu: raw.water_per_day_hu,
        nutrient_per_day: raw.nutrient_per_day,
        reproduction: raw.reproduction,
        seasons: raw.seasons,
        carrying_capacity: raw.carrying_capacity,
        harvest: raw.harvest,
        regrowth_days: raw.regrowth_days,
        min_stability_permille: raw.min_stability,
        max_stability_permille: raw.max_stability,
        min_richness_permille: raw.min_richness,
        crystal_stages: raw.crystal_stages,
        preserving_tool_tier: raw.preserving_tool_tier,
    }))
}
