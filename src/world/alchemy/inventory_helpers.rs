//! Inventory helpers shared alchemy rules.

use crate::inventory::Inventory;
use crate::inventory::ItemStack;
use crate::alchemy::ProducedStack;

pub(super) fn take_count(
    inventory: &mut Inventory,
    slot: usize,
    expected_item: crate::registry::ItemId,
    count: u32,
) -> Result<ItemStack, String> {
    let stack = inventory
        .slots
        .get(slot)
        .copied()
        .flatten()
        .ok_or("That authoritative inventory slot is empty.")?;
    if stack.item != expected_item || stack.count < count || stack.arcane_id != 0 {
        return Err("That slot cannot fund the measured ordinary input count.".into());
    }
    let taken = ItemStack { count, ..stack };
    let remaining = stack.count - count;
    inventory.slots[slot] = (remaining != 0).then_some(ItemStack {
        count: remaining,
        ..stack
    });
    Ok(taken)
}

pub(super) fn produced(
    registry: &crate::registry::Registry,
    item_name: &str,
    count: u32,
    arcane_id: u64,
) -> Result<ProducedStack, String> {
    let item = registry
        .item_id(item_name)
        .ok_or_else(|| format!("Alchemy output {item_name} is missing from the registry."))?;
    Ok(ProducedStack {
        item_name: item_name.into(),
        count,
        durability: registry.item(item).durability,
        arcane_id,
    })
}

pub(super) fn take_exact_slot(
    inventory: &mut Inventory,
    slot: usize,
    expected_item: crate::registry::ItemId,
) -> Result<ItemStack, String> {
    let stack = inventory
        .slots
        .get(slot)
        .copied()
        .flatten()
        .ok_or("That authoritative inventory slot is empty.")?;
    if stack.item != expected_item {
        return Err("That slot does not contain the required physical input.".into());
    }
    inventory
        .take_one_stack(slot)
        .ok_or("The input changed before it could be reserved.".into())
}

