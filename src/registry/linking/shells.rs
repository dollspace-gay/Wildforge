//! Resolve resonance, working, geography, and preparation identities.

use crate::registry::schema::RawMod;
use crate::registry::{ArcaneSiteRule, Registry, RetrogenPolicy, qualify};

pub(super) fn resonances(reg: &mut Registry, raws: &[RawMod]) {
    for raw in raws {
        for resonance in &raw.resonances {
            let id = qualify(&raw.info.id, &resonance.id);
            if id.len() > 96
                || !id.contains(':')
                || !id.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b":_-".contains(&byte)
                })
            {
                reg.arcane_errors.push(format!(
                    "{id}: resonance id must be a lowercase qualified content id"
                ));
                continue;
            }
            if reg.arcane_registry.definitions.contains_key(&id) {
                reg.arcane_errors
                    .push(format!("{id}: duplicate resonance identity"));
                continue;
            }
            reg.arcane_registry.definitions.insert(
                id.clone(),
                crate::arcane::ResonanceDefinition {
                    id,
                    label: resonance
                        .label
                        .clone()
                        .unwrap_or_else(|| resonance.id.clone()),
                    provider: raw.info.id.clone(),
                    active: true,
                },
            );
        }
    }
}

pub(super) fn workings(reg: &mut Registry, raws: &[RawMod]) {
    // Working shells resolve only after every provider's resonance identities
    // exist. A bad shell is never installed, and the shared content error gate
    // prevents authoritative worlds from opening with only part of a pack.
    for raw in raws {
        for working in &raw.workings {
            match crate::workings::WorkingDef::from_raw(&raw.info.id, working.clone()) {
                Ok(definition) => {
                    if !reg
                        .arcane_registry
                        .definitions
                        .contains_key(&definition.focus)
                    {
                        reg.arcane_errors.push(format!(
                            "{}: unknown focus resonance {}",
                            definition.id, definition.focus
                        ));
                    } else if reg.workings.contains_key(&definition.id) {
                        reg.arcane_errors
                            .push(format!("{}: duplicate working identity", definition.id));
                    } else {
                        reg.workings.insert(definition.id.clone(), definition);
                    }
                }
                Err(error) => reg.arcane_errors.push(error.to_string()),
            }
        }
    }
}

pub(super) fn sites(reg: &mut Registry, raws: &[RawMod]) {
    const SITE_REQUIREMENTS: [&str; 9] = [
        "fault",
        "carbonate_rock",
        "groundwater",
        "volcanic",
        "river",
        "coast",
        "old_crust",
        "heart",
        "wetland",
    ];
    for raw in raws {
        for site in &raw.arcane_sites {
            let id = qualify(&raw.info.id, &site.id);
            let mut invalid = false;
            if id.len() > 96
                || !id.contains(':')
                || !id.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b":_-".contains(&byte)
                })
            {
                reg.arcane_errors.push(format!(
                    "{id}: arcane site id must be a lowercase qualified content id"
                ));
                invalid = true;
            }
            if reg.arcane_sites.iter().any(|rule| rule.id == id) {
                reg.arcane_errors
                    .push(format!("{id}: duplicate arcane site identity"));
                invalid = true;
            }
            for requirement in &site.requires {
                if !SITE_REQUIREMENTS.contains(&requirement.as_str()) {
                    reg.arcane_errors.push(format!(
                        "{id}: unknown arcane site requirement {requirement}"
                    ));
                    invalid = true;
                }
            }
            if !site.capacity_factor.is_finite()
                || !(0.25..=4.0).contains(&site.capacity_factor)
                || !site.rarity.is_finite()
                || !(0.0..=1.0).contains(&site.rarity)
                || site.radius_cells == 0
                || site.radius_cells > 64
            {
                reg.arcane_errors.push(format!(
                    "{id}: capacity_factor must be 0.25..=4, rarity 0..=1, and radius_cells 1..=64"
                ));
                invalid = true;
            }
            let mut resonance = [0u16; 6];
            for (name, weight) in &site.resonance {
                let qualified = qualify(&raw.info.id, name);
                let Some(slot) = crate::arcane::BASE_RESONANCES
                    .iter()
                    .position(|candidate| *candidate == qualified)
                else {
                    reg.arcane_errors.push(format!(
                        "{id}: geography genesis currently accepts only the six base resonances; found {qualified}"
                    ));
                    invalid = true;
                    continue;
                };
                resonance[slot] = *weight;
            }
            if invalid {
                continue;
            }
            reg.arcane_sites.push(ArcaneSiteRule {
                id,
                provider: raw.info.id.clone(),
                requires: site.requires.clone(),
                capacity_factor_permille: (site.capacity_factor * 1_000.0).round() as u16,
                base_resonance_bias: resonance,
                rarity_per_million: (site.rarity * 1_000_000.0).round() as u32,
                radius_cells: site.radius_cells,
                retrogen: raw.info.retrogen.unwrap_or(RetrogenPolicy::NoRetrogen),
            });
        }
    }
}

pub(super) fn preparations(reg: &mut Registry, raws: &[RawMod]) {
    // Preparations resolve after items so their physical solvent, ingredient,
    // vessel, residue, and output identities can all be proven. Invalid data
    // never installs a partial effect shell.
    for raw in raws {
        for preparation in &raw.preparations {
            if reg.preparations.len() >= crate::alchemy::MAX_PREPARATION_DEFINITIONS {
                reg.arcane_errors.push(format!(
                    "{}: preparation registry exceeds its {}-definition safety bound",
                    raw.info.id,
                    crate::alchemy::MAX_PREPARATION_DEFINITIONS
                ));
                continue;
            }
            match crate::alchemy::PreparationDef::from_raw(&raw.info.id, preparation.clone()) {
                Ok(definition) => {
                    if reg.preparations.contains_key(&definition.id) {
                        reg.arcane_errors
                            .push(format!("{}: duplicate preparation identity", definition.id));
                    } else if let Err(error) = definition.validate_registry(reg) {
                        reg.arcane_errors.push(error.to_string());
                    } else {
                        reg.preparations.insert(definition.id.clone(), definition);
                    }
                }
                Err(error) => reg.arcane_errors.push(error.to_string()),
            }
        }
    }
}
