//! Inventory runtime for authenticated host operations.

use super::{Guest, ItemStack, ProfileStore};

pub(super) fn refresh_held(guest: &mut Guest) {
    guest.held = guest.inventory.slots[guest.hotbar]
        .map(|stack| stack.item.0)
        .unwrap_or(u16::MAX);
}

pub(super) fn server_item_armor_points(
    stack: &ItemStack,
    profiles: Option<&ProfileStore>,
) -> Option<u32> {
    profiles?
        .registry_hint()
        .item(stack.item)
        .armor
        .map(|(_, points)| points)
}

pub(super) fn take_item(
    inventory: &mut crate::inventory::Inventory,
    item: crate::registry::ItemId,
) -> bool {
    let Some(slot) = inventory
        .slots
        .iter()
        .position(|stack| stack.is_some_and(|stack| stack.item == item))
    else {
        return false;
    };
    inventory.take_one(slot).is_some()
}

pub(super) fn take_ammo(
    inventory: &mut crate::inventory::Inventory,
    reg: &crate::registry::Registry,
    class: &str,
    creative: bool,
) -> Option<crate::registry::ItemId> {
    let item = inventory
        .slots
        .iter()
        .flatten()
        .find(|stack| reg.item(stack.item).ammo.as_deref() == Some(class))?
        .item;
    if !creative {
        let _ = take_item(inventory, item);
    }
    Some(item)
}
