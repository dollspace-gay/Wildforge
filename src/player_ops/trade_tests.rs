//! Purchases conserve the actual paid instances and roll back partial staging.

use super::trade::{PurchaseError, purchase};
use crate::inventory::{Inventory, ItemStack};
use crate::registry::{self, Registry};
use crate::world::StallState;

fn registry() -> Registry {
    registry::load(std::path::Path::new("/nonexistent-mods-dir"))
}

fn stack(reg: &Registry, name: &str, count: u32) -> ItemStack {
    ItemStack::new(reg, reg.item_id(name).unwrap(), count)
}

fn shop(price: ItemStack, goods: ItemStack) -> StallState {
    let mut shop = StallState {
        price: Some(price),
        ..StallState::default()
    };
    shop.goods[0] = Some(goods);
    shop
}

#[test]
fn barter_preserves_distinct_paid_identities_and_never_mints_the_quote() {
    let reg = registry();
    let mut quote = stack(&reg, "base:stick", 2);
    quote.arcane_id = 999;
    let goods = stack(&reg, "base:stone", 3);
    let mut stall = shop(quote, goods);
    let mut buyer = Inventory::new();
    let first = ItemStack {
        arcane_id: 41,
        durability: 7,
        ..stack(&reg, "base:stick", 1)
    };
    let second = ItemStack {
        arcane_id: 42,
        durability: 9,
        ..first
    };
    buyer.slots[0] = Some(first);
    buyer.slots[1] = Some(second);
    let result = purchase(&reg, &mut stall, &mut buyer).unwrap();
    assert_eq!(result.overflow, None);
    assert_eq!(&stall.till[..2], &[Some(first), Some(second)]);
    assert!(
        stall
            .till
            .iter()
            .flatten()
            .all(|paid| paid.arcane_id != 999)
    );
    assert_eq!(stall.goods[0], Some(ItemStack { count: 2, ..goods }));
    assert_eq!(
        buyer.slots.iter().flatten().map(|s| s.count).sum::<u32>(),
        1
    );
}

#[test]
fn a_full_till_after_one_staged_payment_rolls_back_every_owner() {
    let reg = registry();
    let price = stack(&reg, "base:stick", 2);
    let mut stall = shop(price, stack(&reg, "base:stone", 3));
    let occupied = stack(&reg, "base:dirt", 64);
    stall.till[..5].fill(Some(occupied));
    let mut buyer = Inventory::new();
    buyer.slots[0] = Some(ItemStack {
        arcane_id: 41,
        ..price
    });
    buyer.slots[0].as_mut().unwrap().count = 1;
    buyer.slots[1] = Some(ItemStack {
        arcane_id: 42,
        count: 1,
        ..price
    });
    let before = (buyer.slots, stall.goods, stall.till);
    assert!(matches!(
        purchase(&reg, &mut stall, &mut buyer),
        Err(PurchaseError::TillFull)
    ));
    assert_eq!((buyer.slots, stall.goods, stall.till), before);
}

#[test]
fn insufficient_payment_rolls_back_even_when_the_first_stack_was_accepted() {
    let reg = registry();
    let price = stack(&reg, "base:stick", 2);
    let mut stall = shop(price, stack(&reg, "base:stone", 3));
    let mut buyer = Inventory::new();
    buyer.slots[0] = Some(ItemStack { count: 1, ..price });
    let before = (buyer.slots, stall.goods, stall.till);
    assert!(matches!(
        purchase(&reg, &mut stall, &mut buyer),
        Err(PurchaseError::CannotAfford)
    ));
    assert_eq!((buyer.slots, stall.goods, stall.till), before);
}

#[test]
fn exact_till_capacity_and_full_buyer_return_one_physical_overflow() {
    let reg = registry();
    let price = stack(&reg, "base:stick", 1);
    let sold = ItemStack {
        arcane_id: 83,
        durability: 17,
        ..stack(&reg, "base:stone", 1)
    };
    let mut stall = shop(price, sold);
    stall.till.fill(Some(stack(&reg, "base:stick", 64)));
    stall.till[5].as_mut().unwrap().count = 63;
    let mut buyer = Inventory::new();
    buyer.slots.fill(Some(stack(&reg, "base:dirt", 64)));
    buyer.slots[0] = Some(ItemStack { count: 2, ..price });
    let result = purchase(&reg, &mut stall, &mut buyer).unwrap();
    assert_eq!(result.overflow, Some(sold));
    assert_eq!(stall.goods[0], None);
    assert_eq!(stall.till[5].unwrap().count, 64);
    assert_eq!(buyer.slots[0], Some(price));
}
