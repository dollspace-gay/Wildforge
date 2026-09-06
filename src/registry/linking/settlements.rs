//! Resolve settlement tiers and cross-check assembly reachability.

use super::lookups::lookup_item;
use crate::registry::schema::SettlementToml;
use crate::registry::{Registry, SettlementDef, SettlementNeed, SettlementTier, qualify};

pub(super) fn resolve(
    reg: &mut Registry,
    pending_settlements: Vec<(String, SettlementToml)>,
) -> Vec<String> {
    let mut settlement_errors = Vec::new();
    for (modid, s) in pending_settlements {
        let id = qualify(&modid, &s.id);
        if reg.settlements.iter().any(|existing| existing.id == id) {
            settlement_errors.push(format!("{id}: duplicate settlement id"));
            continue;
        }
        let mut tiers: Vec<SettlementTier> = s
            .tiers
            .iter()
            .map(|t| SettlementTier {
                tier: t.tier.max(2),
                threshold: t.threshold,
            })
            .collect();
        tiers.sort_by_key(|t| t.tier);
        tiers.dedup_by_key(|t| t.tier);
        if !tiers.windows(2).all(|w| w[0].threshold < w[1].threshold) {
            settlement_errors.push(format!(
                "{id}: settlement tiers must have strictly increasing thresholds"
            ));
            continue;
        }
        // Capability E13: resolve the delivery needs against the roster.
        let mut needs: Vec<SettlementNeed> = Vec::new();
        for need in &s.need {
            let Some(item) = lookup_item(reg, &modid, &need.item) else {
                settlement_errors.push(format!("{id}: need item {} does not resolve", need.item));
                continue;
            };
            needs.push(SettlementNeed {
                item,
                rep_per_unit: need.rep.unwrap_or(1).max(1),
            });
        }
        reg.settlements.push(SettlementDef {
            rep_key: s.rep_key.clone().unwrap_or_else(|| format!("rep_{id}")),
            id,
            tiers,
            needs,
        });
    }
    validate_wiring(reg, &mut settlement_errors);
    settlement_errors
}

fn validate_wiring(reg: &Registry, settlement_errors: &mut Vec<String>) {
    // Cross-validate settlement wiring (spec 3.4): every assembly that names
    // a settlement must resolve one, a piece tagged tier > 1 must be
    // reachable from a settlement assembly (a hidden tier that can never be
    // placed would silently never exist), and its tier must exist in that
    // settlement's declared tiers (else reveal could never happen).
    let mut assembly_settlements: Vec<Option<usize>> =
        reg.assemblies.iter().map(|_| None).collect();
    for (i, asm) in reg.assemblies.iter().enumerate() {
        if let Some(settlement) = &asm.settlement {
            match reg.settlements.iter().position(|s| &s.id == settlement) {
                Some(idx) => assembly_settlements[i] = Some(idx),
                None => settlement_errors.push(format!(
                    "{}: assembly names unknown settlement {settlement}",
                    asm.name
                )),
            }
        }
    }
    for piece in &reg.pieces {
        if piece.settlement_tier <= 1 {
            continue;
        }
        let mut reachable = false;
        for (i, asm) in reg.assemblies.iter().enumerate() {
            if assembly_settlements[i].is_none() {
                continue;
            }
            let in_pool = asm.pools.values().any(|pool| {
                reg.pools
                    .iter()
                    .find(|p| &p.id == pool)
                    .is_some_and(|p| p.entries.iter().any(|e| e.piece == piece.name))
            });
            if !in_pool {
                continue;
            }
            reachable = true;
            let settlement = assembly_settlements[i].expect("checked above");
            if !reg.settlements[settlement]
                .tiers
                .iter()
                .any(|t| t.tier == piece.settlement_tier)
            {
                settlement_errors.push(format!(
                    "{}: piece tier {} not declared in settlement {}",
                    piece.name, piece.settlement_tier, reg.settlements[settlement].id
                ));
            }
        }
        if !reachable {
            settlement_errors.push(format!(
                "{}: piece tagged settlement_tier {} but no settlement assembly reaches it",
                piece.name, piece.settlement_tier
            ));
        }
    }
}
