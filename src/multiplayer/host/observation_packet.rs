//! Observation packet for authenticated host operations.

use super::{Guest, World};


pub(super) fn inspectable_arcane_items(
    world: &World,
    guest: &Guest,
) -> (
    Vec<(u64, u64)>,
    Vec<crate::implements::ImplementPublicState>,
    Vec<crate::implements::ApparatusCue>,
) {
    let mut ids = guest
        .inventory
        .slots
        .iter()
        .chain(guest.armor.iter())
        .chain(std::iter::once(&guest.cursor))
        .chain(guest.craft_grid.iter())
        .flatten()
        .filter_map(|stack| (stack.arcane_id != 0).then_some(stack.arcane_id))
        .collect::<std::collections::BTreeSet<_>>();
    if let Some(pos) = guest.container
        && let Some(entity) = world.block_entity_at(&pos)
    {
        ids.extend(
            World::block_entity_stacks(entity)
                .into_iter()
                .filter_map(|stack| (stack.arcane_id != 0).then_some(stack.arcane_id)),
        );
    }
    if let Some(mob_id) = guest.mob_cargo
        && let Some(cargo) = world.mob_by_id(mob_id).and_then(|mob| mob.cargo.as_ref())
    {
        ids.extend(
            cargo
                .iter()
                .flatten()
                .filter_map(|stack| (stack.arcane_id != 0).then_some(stack.arcane_id)),
        );
    }
    let charges = ids
        .iter()
        .map(|id| (*id, world.inspectable_item_current(*id).unwrap_or(0)))
        .collect();
    let implements = ids
        .iter()
        .filter_map(|id| {
            let instance = world
                .implements_state
                .as_ref()
                .and_then(|state| state.instance(*id))?;
            let dross = world
                .arcane_ledger
                .as_ref()
                .map_or(0, |ledger| ledger.item_dross_total(*id));
            Some(crate::implements::ImplementPublicState::from_authority(
                instance, dross,
            ))
        })
        .collect();
    let mut apparatus = world.apparatus_cues_near(guest.pos, 48.0);
    apparatus.sort_by_key(|cue| cue.pos);
    (charges, implements, apparatus)
}

