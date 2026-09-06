//! Armor/charm slot eligibility and cursor exchange.

use crate::inventory::ItemStack;
use crate::registry::Registry;

/// Components already attached to local equipment are returned by the loadout
/// owner before this exchange. This operation owns only slot eligibility.
pub(crate) fn exchange(
    registry: &Registry,
    armor: &mut [Option<ItemStack>],
    cursor: &mut Option<ItemStack>,
    index: usize,
) {
    let Some(slot) = armor.get_mut(index) else {
        return;
    };
    if let Some(held) = cursor {
        let definition = registry.item(held.item);
        let fits = if index == 4 {
            definition.charm.is_some()
        } else {
            definition.armor.map(|(kind, _)| kind as usize) == Some(index)
        };
        if !fits {
            return;
        }
    }
    std::mem::swap(slot, cursor);
}
