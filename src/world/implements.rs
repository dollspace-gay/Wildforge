//! Authoritative world integration for physical magical implements.
use crate::arcane::AccountRead;
use crate::arcane::ArcaneAuthority;
use crate::arcane::ArcaneMove;
use crate::arcane::ArcaneOwner;
use crate::arcane::ArcaneTransaction;
use crate::arcane::Current;
use crate::implements::ImplementComponent;
use crate::implements::ImplementKind;
use crate::world::BlockPos;
use crate::world::ItemStack;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

const ASSEMBLY_CALIBRATION_UNITS: u64 = 16;
const VESSEL_INITIAL_CHARGE: u64 = 64;

fn horizontal_neighbors(pos: BlockPos) -> Vec<BlockPos> {
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .filter_map(|(du, dv)| pos.offset(du, 0, dv))
        .collect()
}

fn all_neighbors(pos: BlockPos) -> Vec<BlockPos> {
    let mut out = horizontal_neighbors(pos);
    if let Some(up) = pos.offset(0, 1, 0) {
        out.push(up);
    }
    if let Some(down) = pos.offset(0, -1, 0) {
        out.push(down);
    }
    out
}

fn charm_resonance_preference(effect: crate::implements::CharmEffect) -> Vec<String> {
    match effect {
        crate::implements::CharmEffect::Quiet => [crate::arcane::ECHO, crate::arcane::GALE],
        crate::implements::CharmEffect::Bark => [crate::arcane::ROOT, crate::arcane::STONE],
        crate::implements::CharmEffect::Hunger => [crate::arcane::ROOT, crate::arcane::TIDE],
    }
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn implement_transfer_properties(kind: &ImplementKind) -> (u64, u64, u16) {
    match kind {
        ImplementKind::Wand { resolved, .. } => (
            resolved.capacity,
            resolved.safe_transfer,
            resolved.dross_per_thousand,
        ),
        ImplementKind::Charm {
            capacity,
            stability,
            dross_per_transfer,
            ..
        } => (
            *capacity,
            32,
            (*dross_per_transfer)
                .saturating_add((1_000u16.saturating_sub(*stability)) / 10)
                .clamp(1, 500),
        ),
        ImplementKind::Vessel {
            capacity,
            safe_transfer,
            containment,
        } => (
            *capacity,
            *safe_transfer,
            (1_000u16.saturating_sub(*containment) / 5).clamp(1, 500),
        ),
        ImplementKind::Fragments { .. } => (0, 0, 500),
    }
}

fn apparatus_neighbors(pos: BlockPos) -> Vec<BlockPos> {
    let mut out = horizontal_neighbors(pos);
    out.extend([1, -1].into_iter().filter_map(|dy| pos.offset(0, dy, 0)));
    out
}

pub(super) fn add_current(
    map: &mut BTreeMap<ArcaneOwner, Current>,
    owner: ArcaneOwner,
    current: &Current,
) -> Result<(), crate::arcane::ArcaneError> {
    map.entry(owner).or_default().checked_add(current)
}

fn physical_component(reg: &crate::registry::Registry, stack: ItemStack) -> ImplementComponent {
    ImplementComponent {
        content_id: reg.item(stack.item).name.clone(),
        materials: crate::materials::stack_materials(reg, ItemStack { count: 1, ..stack }),
    }
}

pub(super) fn transaction_from_maps(
    ledger: &mut crate::arcane::ArcaneLedger,
    debits: BTreeMap<ArcaneOwner, Current>,
    credits: BTreeMap<ArcaneOwner, Current>,
    content_id: &str,
    reason: &str,
) -> Result<ArcaneTransaction, String> {
    if debits.len().saturating_add(credits.len()) > crate::implements::MAX_IMPLEMENT_TRANSFER_OWNERS
    {
        return Err(format!(
            "implement transfer touches {} owner entries; local budget is {}",
            debits.len().saturating_add(credits.len()),
            crate::implements::MAX_IMPLEMENT_TRANSFER_OWNERS
        ));
    }
    let touched = debits
        .keys()
        .chain(credits.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let credit_content = credits
        .keys()
        .filter(|owner| matches!(owner, ArcaneOwner::Item(_) | ArcaneOwner::ItemDross(_)))
        .map(|owner| {
            let existing = ledger.account(owner);
            let fully_replaced = existing.is_some_and(|account| {
                debits
                    .get(owner)
                    .is_some_and(|removed| removed == &account.current)
            });
            let identity = if fully_replaced {
                content_id.to_string()
            } else {
                existing
                    .and_then(|account| account.content_id.clone())
                    .unwrap_or_else(|| content_id.to_string())
            };
            (owner.clone(), identity)
        })
        .collect::<BTreeMap<_, _>>();
    let reads = touched
        .into_iter()
        .map(|owner| AccountRead {
            expected_version: ledger.version_of(&owner),
            owner,
        })
        .collect();
    let debits = debits
        .into_iter()
        .filter(|(_, current)| !current.is_empty())
        .map(|(owner, current)| ArcaneMove {
            owner,
            current,
            content_id: None,
        })
        .collect();
    let credits = credits
        .into_iter()
        .filter(|(_, current)| !current.is_empty())
        .map(|(owner, current)| {
            let identity = credit_content.get(&owner).cloned();
            ArcaneMove {
                owner,
                current,
                content_id: identity,
            }
        })
        .collect();
    Ok(ArcaneTransaction {
        id: ledger
            .system_transaction_id()
            .map_err(|error| error.to_string())?,
        reads,
        debits,
        credits,
        transforms: Vec::new(),
        authority: ArcaneAuthority::System,
        reason: reason.into(),
        content_id: content_id.into(),
        linked: Vec::new(),
    })
}

mod ambient_custody;
mod calibration;
mod charm_binding;
mod charm_migration;
mod charms;
mod conductor_transfer;
mod disassembly;
mod discharge;
mod failure;
mod focus_swap;
mod frame_break;
mod frame_dispatch;
mod frame_layout;
mod item_transfer;
mod observation;
mod repair;
mod retirement;
mod source_transfer;
mod vessel_tick;
mod wand_assembly;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apparatus_neighbor_walk_is_bounded_and_unique() {
        let pos = BlockPos::of_world(2, 80, 2).unwrap();
        let neighbors = apparatus_neighbors(pos);
        assert_eq!(neighbors.len(), 6);
        assert_eq!(neighbors.iter().copied().collect::<BTreeSet<_>>().len(), 6);
    }
}
