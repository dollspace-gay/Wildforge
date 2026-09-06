//! Loot chunks transaction coordination.

use crate::inventory::ItemStack;
use crate::world::World;

impl World {
    /// Weighted rolls from a loot table.
    pub fn roll_loot(&self, table: &str, rolls: u32, rng: &mut u32) -> Vec<ItemStack> {
        let Some(entries) = self.reg.loots.get(table) else {
            return Vec::new();
        };
        let total: u32 = entries.iter().map(|e| e.weight).sum();
        if total == 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        for _ in 0..rolls {
            *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            let mut pick = (*rng >> 8) % total;
            for e in entries {
                if pick < e.weight {
                    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    let span = e.count.1.saturating_sub(e.count.0) + 1;
                    let n = e.count.0 + (*rng >> 8) % span;
                    let instances = if self.reg.item(e.item).arcane.is_some()
                        || self.reg.item(e.item).discovery.is_some()
                    {
                        n.max(1)
                    } else {
                        1
                    };
                    for _ in 0..instances {
                        let count = if instances == 1 { n.max(1) } else { 1 };
                        let mut stack = ItemStack::new(&self.reg, e.item, count);
                        if let Some(frac) = e.durability_frac {
                            let max = self.reg.item(e.item).durability;
                            if max > 0 {
                                stack.durability = ((max as f32 * frac) as u32).max(1);
                            }
                        }
                        out.push(stack);
                    }
                    break;
                }
                pick -= e.weight;
            }
        }
        out
    }
}
