//! Player inventory: 9 hotbar slots + 27 storage slots of item stacks.
//! Item properties come from the registry.

use crate::registry::{ItemId, Registry};

pub const HOTBAR_SLOTS: usize = 9;
pub const STORAGE_SLOTS: usize = 27;
pub const TOTAL_SLOTS: usize = HOTBAR_SLOTS + STORAGE_SLOTS;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ItemStack {
    pub item: ItemId,
    pub count: u32,
    /// Remaining uses for tools; 0 for everything else.
    pub durability: u32,
    /// Stable server-assigned identity of the corresponding Current account;
    /// zero means this instance carries no bound Current.
    pub arcane_id: u64,
}

impl ItemStack {
    pub fn new(reg: &Registry, item: ItemId, count: u32) -> ItemStack {
        ItemStack {
            item,
            count,
            durability: reg.item(item).durability,
            arcane_id: 0,
        }
    }

    /// Stacks merge only if same item, neither is a tool, and neither carries
    /// stable instance state.  An identity names one physical object and may
    /// never be copied into a larger count by an inventory convenience path.
    pub fn can_merge(&self, reg: &Registry, other: &ItemStack) -> bool {
        self.item == other.item
            && reg.item(self.item).tool.is_none()
            && self.arcane_id == 0
            && other.arcane_id == 0
    }
}

/// One UI click on a slot holding `cur` with `held` on the cursor.
pub fn click_stack(
    reg: &Registry,
    cur: Option<ItemStack>,
    held: Option<ItemStack>,
    right: bool,
) -> (Option<ItemStack>, Option<ItemStack>) {
    match (held, cur, right) {
        (Some(held), Some(s), false) if s.can_merge(reg, &held) => {
            let max = reg.item(s.item).max_stack;
            let move_n = held.count.min(max - s.count);
            let slot = Some(ItemStack {
                count: s.count + move_n,
                ..s
            });
            let held = if held.count > move_n {
                Some(ItemStack {
                    count: held.count - move_n,
                    ..held
                })
            } else {
                None
            };
            (slot, held)
        }
        (Some(held), other, false) => (Some(held), other),
        (None, Some(s), false) => (None, Some(s)),
        (Some(held), cur, true) => {
            let can_place = match cur {
                None => true,
                Some(s) => s.can_merge(reg, &held) && s.count < reg.item(s.item).max_stack,
            };
            if can_place {
                let count = cur.map_or(0, |s| s.count) + 1;
                let slot = Some(ItemStack { count, ..held });
                let held = if held.count > 1 {
                    Some(ItemStack {
                        count: held.count - 1,
                        ..held
                    })
                } else {
                    None
                };
                (slot, held)
            } else {
                (cur, Some(held))
            }
        }
        (None, Some(s), true) => {
            let take = s.count.div_ceil(2);
            let slot = if s.count > take {
                Some(ItemStack {
                    count: s.count - take,
                    ..s
                })
            } else {
                None
            };
            (slot, Some(ItemStack { count: take, ..s }))
        }
        (h, c, _) => (c, h),
    }
}

#[derive(Clone)]
pub struct Inventory {
    pub slots: [Option<ItemStack>; TOTAL_SLOTS],
}

impl Inventory {
    pub fn new() -> Inventory {
        Inventory {
            slots: [None; TOTAL_SLOTS],
        }
    }

    /// Add a stack; returns the count that did not fit.
    pub fn add_stack(&mut self, reg: &Registry, stack: ItemStack) -> u32 {
        let mut count = stack.count;
        let max = reg.item(stack.item).max_stack;
        if max > 1 {
            for slot in self.slots.iter_mut() {
                if count == 0 {
                    break;
                }
                if let Some(s) = slot
                    && s.can_merge(reg, &stack)
                    && s.count < max
                {
                    let take = count.min(max - s.count);
                    s.count += take;
                    count -= take;
                }
            }
        }
        for slot in self.slots.iter_mut() {
            if count == 0 {
                break;
            }
            if slot.is_none() {
                let take = count.min(max);
                *slot = Some(ItemStack {
                    count: take,
                    ..stack
                });
                count -= take;
            }
        }
        count
    }

    pub fn add(&mut self, reg: &Registry, item: ItemId, count: u32) -> u32 {
        self.add_stack(reg, ItemStack::new(reg, item, count))
    }

    /// Total carry weight in units: each stack contributes
    /// `count * item.carry_weight`. Creative-only items are weightless
    /// by default only when their declaration says so (carry_weight 0).
    pub fn total_weight(&self, reg: &Registry) -> u64 {
        self.slots
            .iter()
            .flatten()
            .map(|s| u64::from(s.count) * u64::from(reg.item(s.item).carry_weight))
            .sum()
    }

    pub fn take_one(&mut self, slot: usize) -> Option<ItemId> {
        self.take_one_stack(slot).map(|stack| stack.item)
    }

    /// Remove one physical item without discarding its durable identity.
    /// Charged stacks are singular, but keeping this general makes every
    /// consuming caller safe if another identity-bearing item type appears.
    pub fn take_one_stack(&mut self, slot: usize) -> Option<ItemStack> {
        let s = self.slots[slot].as_mut()?;
        let taken = ItemStack { count: 1, ..*s };
        s.count -= 1;
        if s.count == 0 {
            self.slots[slot] = None;
        }
        Some(taken)
    }

    /// Wear the tool in `slot` by one use. Finite tools become a full-mass
    /// damaged object at zero durability; renewable/stone tools may still
    /// break apart because they are outside the exact metal ledger.
    pub fn wear_tool(&mut self, reg: &Registry, slot: usize) {
        if let Some(s) = self.slots[slot].as_mut() {
            // Anything with a durability pool wears: tools and swords.
            if reg.item(s.item).durability > 0 {
                s.durability = s.durability.saturating_sub(1);
                if s.durability == 0 {
                    self.slots[slot] = reg.item(s.item).broken_into.map(|broken| ItemStack {
                        item: broken,
                        count: 1,
                        durability: 0,
                        arcane_id: s.arcane_id,
                    });
                }
            }
        }
    }

    pub fn drain(&mut self) -> Vec<ItemStack> {
        let mut out = Vec::new();
        for slot in self.slots.iter_mut() {
            if let Some(s) = slot.take() {
                out.push(s);
            }
        }
        out
    }

    /// Total count of `item` spread across the whole inventory, regardless
    /// of stack fragmentation. Summation only — explicitly not a mutation.
    ///
    /// This is the "has enough of many items at once" primitive the template
    /// instant-stamp (spec Part 1.4) needs; nothing in the crafting/recipe
    /// system already does a whole-inventory, multi-item check, so it lives
    /// beside the rest of the inventory's bulk operations.
    pub fn count_of(&self, item: ItemId) -> u32 {
        self.slots
            .iter()
            .flatten()
            .filter(|stack| stack.item == item)
            .map(|stack| stack.count)
            .sum()
    }

    /// Can the whole `cost` (a list of `(item, count)` pairs) be paid at
    /// once? All-or-nothing, like the multi-material checks that gate the
    /// template stamp.
    pub fn can_afford(&self, cost: &[(ItemId, u32)]) -> bool {
        cost.iter()
            .all(|&(item, count)| self.count_of(item) >= count)
    }

    /// Deduct `cost` from the inventory. Returns `false` (making no change)
    /// when any item is short, so a caller can check-and-consume atomically
    /// without a separate afford pass.
    pub fn try_consume(&mut self, cost: &[(ItemId, u32)]) -> bool {
        if !self.can_afford(cost) {
            return false;
        }
        for &(item, count) in cost {
            let mut remaining = count;
            for slot in self.slots.iter_mut() {
                if remaining == 0 {
                    break;
                }
                if let Some(stack) = slot {
                    if stack.item != item {
                        continue;
                    }
                    let take = stack.count.min(remaining);
                    remaining -= take;
                    stack.count -= take;
                    if stack.count == 0 {
                        *slot = None;
                    }
                }
            }
        }
        true
    }
}
