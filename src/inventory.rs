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
}

impl ItemStack {
    pub fn new(reg: &Registry, item: ItemId, count: u32) -> ItemStack {
        ItemStack {
            item,
            count,
            durability: reg.item(item).durability,
        }
    }

    /// Stacks merge only if same item and neither is a tool.
    pub fn can_merge(&self, reg: &Registry, other: &ItemStack) -> bool {
        self.item == other.item && reg.item(self.item).tool.is_none()
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
                    && s.item == stack.item
                    && reg.item(s.item).tool.is_none()
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

    pub fn take_one(&mut self, slot: usize) -> Option<ItemId> {
        let s = self.slots[slot].as_mut()?;
        s.count -= 1;
        let item = s.item;
        if s.count == 0 {
            self.slots[slot] = None;
        }
        Some(item)
    }

    /// Wear the tool in `slot` by one use; destroys it at zero durability.
    pub fn wear_tool(&mut self, reg: &Registry, slot: usize) {
        if let Some(s) = self.slots[slot].as_mut() {
            // Anything with a durability pool wears: tools and swords.
            if reg.item(s.item).durability > 0 {
                s.durability = s.durability.saturating_sub(1);
                if s.durability == 0 {
                    self.slots[slot] = None;
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
}
