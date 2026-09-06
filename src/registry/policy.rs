//! Runtime selection of declared scar definitions and named rulesets.

use super::{ModeDef, Registry};
use std::collections::BTreeMap;

impl Registry {
    /// Select one eligible declarative scar shell with a stable key. Existing
    /// sites persist the returned content id, so later pack reordering cannot
    /// repaint them. Per-definition regional caps prevent a mod from
    /// declaring an unbounded self-replicator.
    pub fn select_dross_scar(
        &self,
        kind: crate::dross::ScarKind,
        carrier: crate::dross::DrossCarrier,
        band: crate::dross::DrossBand,
        existing_in_region: &BTreeMap<String, usize>,
        stable_key: u64,
    ) -> Option<&crate::dross::DrossScarDef> {
        let eligible = self
            .dross_scars
            .values()
            .filter(|definition| {
                definition.kind == kind
                    && definition.carriers.contains(&carrier)
                    && band >= definition.min_band
                    && existing_in_region
                        .get(&definition.content_id)
                        .copied()
                        .unwrap_or_default()
                        < usize::from(definition.max_sites_per_region)
            })
            .collect::<Vec<_>>();
        if eligible.is_empty() {
            return self.dross_scars.values().find(|definition| {
                definition.provider == "base"
                    && definition.kind == kind
                    && band >= definition.min_band
                    && existing_in_region
                        .get(&definition.content_id)
                        .copied()
                        .unwrap_or_default()
                        < usize::from(definition.max_sites_per_region)
            });
        }
        Some(eligible[stable_key as usize % eligible.len()])
    }

    /// Resolve a persisted site. A removed provider leaves its stable identity
    /// in the save, but materialization uses the safe base shell for the same
    /// climate kind until that provider returns.
    pub fn resolve_dross_scar(
        &self,
        content_id: &str,
        kind: crate::dross::ScarKind,
    ) -> Option<&crate::dross::DrossScarDef> {
        self.dross_scars.get(content_id).or_else(|| {
            self.dross_scars
                .values()
                .find(|definition| definition.provider == "base" && definition.kind == kind)
        })
    }

    /// Resolve a world's `mode` string to its survival ruleset (capability
    /// E1). Built-ins are `survival` and `creative`; anything else is a
    /// mod-declared `[[mode]]`, chained through its `base`. A missing or
    /// broken mode falls back to survival so an unknown mode string never
    /// strips safety netting.
    pub fn ruleset_for(&self, mode: &str) -> crate::ruleset::Ruleset {
        if mode == "creative" {
            return crate::ruleset::Ruleset::creative();
        }
        let mut chain: Vec<&ModeDef> = Vec::new();
        let mut current = mode;
        for _ in 0..=self.modes.len() {
            let Some(def) = self.modes.iter().find(|d| d.id == current) else {
                break;
            };
            if chain.iter().any(|d| d.id == def.id) {
                return crate::ruleset::Ruleset::survival();
            }
            chain.push(def);
            current = def.base.as_deref().unwrap_or("survival");
        }
        let mut ruleset = crate::ruleset::Ruleset::survival();
        for def in chain.into_iter().rev() {
            ruleset.apply_overrides(def);
        }
        ruleset
    }
}
