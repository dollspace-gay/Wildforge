//! Stock, payment, and delivery form one stall purchase transaction.

use crate::inventory::{Inventory, ItemStack};
use crate::registry::Registry;
use crate::world::StallState;

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub(crate) enum PurchaseError {
    #[error("stall transaction contains an invalid item stack")]
    InvalidStack,
    #[error("stall has no price")]
    NoPrice,
    #[error("buyer cannot afford the price")]
    CannotAfford,
    #[error("stall has no stock")]
    NoStock,
    #[error("stall till cannot hold the payment")]
    TillFull,
}

pub(crate) struct Purchase {
    /// The adapter delivers an item that did not fit to its ordinary drop path.
    pub(crate) overflow: Option<ItemStack>,
}

/// The caller checks reach, stall structure, and seller access before borrowing
/// the transaction state. Rejection leaves the buyer, shelf, and till untouched.
pub(crate) fn purchase(
    registry: &Registry,
    stall: &mut StallState,
    buyer: &mut Inventory,
) -> Result<Purchase, PurchaseError> {
    let price = stall.price.ok_or(PurchaseError::NoPrice)?;
    if price.count == 0 {
        return Err(PurchaseError::InvalidStack);
    }
    let goods = stall.goods.iter().position(Option::is_some)
        .ok_or(PurchaseError::NoStock)?;
    let stock = stall.goods[goods].ok_or(PurchaseError::NoStock)?;
    validate_stack(registry, stock)?;
    let sold = ItemStack { count: 1, ..stock };

    // Stage a bounded payment using the buyer's actual stacks. The price slot
    // is a quote; copying it into the till would mint its durable identity and
    // discard the identity/durability of the objects actually paid.
    let mut inventory = buyer.clone();
    let mut till = stall.till;
    let mut need = price.count;
    for slot in &mut inventory.slots {
        if need == 0 {
            break;
        }
        let Some(stack) = *slot else { continue };
        if stack.item != price.item {
            continue;
        }
        validate_stack(registry, stack)?;
        let take = stack.count.min(need);
        deposit(registry, &mut till, ItemStack { count: take, ..stack })?;
        *slot = (stack.count > take).then_some(ItemStack { count: stack.count - take, ..stack });
        need -= take;
    }
    if need != 0 {
        return Err(PurchaseError::CannotAfford);
    }
    let remainder = inventory.add_stack(registry, sold);
    // No fallible operation follows the first committed mutation.
    stall.goods[goods] = (stock.count > 1).then_some(ItemStack { count: stock.count - 1, ..stock });
    stall.till = till;
    *buyer = inventory;
    Ok(Purchase {
        overflow: (remainder > 0).then_some(ItemStack { count: remainder, ..sold }),
    })
}

fn validate_stack(registry: &Registry, stack: ItemStack) -> Result<(), PurchaseError> {
    let Some(definition) = registry.items.get(usize::from(stack.item.0)) else {
        return Err(PurchaseError::InvalidStack);
    };
    if stack.count == 0 || stack.count > definition.max_stack
        || ((stack.arcane_id != 0 || definition.tool.is_some()) && stack.count != 1)
    {
        return Err(PurchaseError::InvalidStack);
    }
    Ok(())
}

fn deposit(
    registry: &Registry,
    till: &mut [Option<ItemStack>; 6],
    payment: ItemStack,
) -> Result<(), PurchaseError> {
    // Retain first-fit placement for ordinary currency. Distinct physical
    // instances require their own slots, and their payload must survive barter.
    let slot = till.iter_mut().find(|slot| match slot {
        None => true,
        Some(stack) => stack.can_merge(registry, &payment)
            && stack.durability == payment.durability
            && stack.count.checked_add(payment.count)
                .is_some_and(|count| count <= registry.item(stack.item).max_stack),
    }).ok_or(PurchaseError::TillFull)?;
    match slot {
        Some(stack) => stack.count += payment.count,
        None => *slot = Some(payment),
    }
    Ok(())
}
