//! Stock, payment, and delivery form one stall purchase transaction.

use crate::inventory::{Inventory, ItemStack};
use crate::registry::Registry;
use crate::world::StallState;

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub(crate) enum PurchaseError {
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
    let have: u32 = buyer.slots.iter().flatten()
        .filter(|stack| stack.item == price.item)
        .map(|stack| stack.count)
        .sum();
    if have < price.count {
        return Err(PurchaseError::CannotAfford);
    }
    let goods = stall.goods.iter_mut().find(|slot| slot.is_some())
        .ok_or(PurchaseError::NoStock)?;
    let till = stall.till.iter_mut().find(|slot| match slot {
        None => true,
        Some(stack) => stack.item == price.item
            && stack.count + price.count <= registry.item(stack.item).max_stack,
    }).ok_or(PurchaseError::TillFull)?;

    let mut stock = goods.take().expect("selected occupied goods slot");
    let sold = ItemStack { count: 1, ..stock };
    stock.count -= 1;
    if stock.count > 0 {
        *goods = Some(stock);
    }
    match till {
        Some(stack) => stack.count += price.count,
        None => *till = Some(price),
    }
    let mut need = price.count;
    for slot in &mut buyer.slots {
        if need == 0 {
            break;
        }
        if let Some(stack) = slot && stack.item == price.item {
            let take = stack.count.min(need);
            need -= take;
            stack.count -= take;
            if stack.count == 0 {
                *slot = None;
            }
        }
    }
    let remainder = buyer.add_stack(registry, sold);
    Ok(Purchase {
        overflow: (remainder > 0).then_some(ItemStack { count: remainder, ..sold }),
    })
}
