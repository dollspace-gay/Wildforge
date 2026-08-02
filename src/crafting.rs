//! Shaped crafting: registry recipes matched at any offset (and mirrored)
//! inside a 2x2 or 3x3 grid.

use crate::inventory::ItemStack;
use crate::registry::{MaterialVector, RecipeDef, Registry};

#[derive(Clone, Debug)]
pub struct RepairMatch {
    pub tool_slot: usize,
    pub part_slot: usize,
    pub output: ItemStack,
    /// The replaced worn fragment becomes unrecoverable scale. It is an
    /// explicit sink rather than an invisible durability deletion.
    pub scale_loss: MaterialVector,
}

pub fn match_repair(reg: &Registry, grid: &[Option<ItemStack>]) -> Option<RepairMatch> {
    let occupied = grid
        .iter()
        .enumerate()
        .filter_map(|(slot, stack)| stack.map(|stack| (slot, stack)))
        .collect::<Vec<_>>();
    if occupied.len() != 2 {
        return None;
    }
    for ((tool_slot, tool), (part_slot, part)) in
        [(occupied[0], occupied[1]), (occupied[1], occupied[0])]
    {
        let definition = reg.item(tool.item);
        if definition.durability == 0
            || tool.durability >= definition.durability
            || definition.materials.is_empty()
        {
            continue;
        }
        let part_materials = &reg.item(part.item).materials;
        let part_name = &reg.item(part.item).name;
        // Scale and slag are secondary stock, not magically clean repair
        // parts. They retain mass for a later recovery process, but accepting
        // them here would turn every advertised 75/90/95% yield back into
        // immediate 100% recovery.
        let secondary = part_name == "base:iron_slag"
            || part_name.ends_with("/primitive_scale")
            || part_name.ends_with("/forge_scale")
            || part_name.ends_with("/dismantling_scale");
        if part_materials.is_empty()
            || secondary
            || !part_materials.iter().all(|(material, units)| {
                definition
                    .materials
                    .get(material)
                    .is_some_and(|capacity| units <= capacity)
            })
        {
            continue;
        }
        let tool_units = definition.materials.values().sum::<u64>();
        let part_units = part_materials.values().sum::<u64>();
        if part_units >= tool_units {
            continue;
        }
        let restored = (u64::from(definition.durability) * part_units / tool_units)
            .max(1)
            .min(u64::from(definition.durability)) as u32;
        return Some(RepairMatch {
            tool_slot,
            part_slot,
            output: ItemStack {
                durability: tool
                    .durability
                    .saturating_add(restored)
                    .min(definition.durability),
                ..tool
            },
            scale_loss: part_materials.clone(),
        });
    }
    None
}

pub fn consume_repair(grid: &mut [Option<ItemStack>], repair: &RepairMatch) {
    grid[repair.tool_slot] = None;
    if let Some(part) = &mut grid[repair.part_slot] {
        part.count -= 1;
        if part.count == 0 {
            grid[repair.part_slot] = None;
        }
    }
}

pub fn match_recipe<'r>(
    reg: &'r Registry,
    grid: &[Option<ItemStack>],
    size: usize,
) -> Option<&'r RecipeDef> {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (size, size, 0usize, 0usize);
    for y in 0..size {
        for x in 0..size {
            if grid[y * size + x].is_some() {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x > max_x {
        return None;
    }
    let (bw, bh) = (max_x - min_x + 1, max_y - min_y + 1);

    'recipes: for r in &reg.recipes {
        if r.w != bw || r.h != bh || r.w > size || r.h > size {
            continue;
        }
        for mirror in [false, true] {
            let ok = (0..bh).all(|y| {
                (0..bw).all(|x| {
                    let px = if mirror { bw - 1 - x } else { x };
                    let want = &r.pattern[y * r.w + px];
                    let have = grid[(min_y + y) * size + (min_x + x)].map(|s| s.item);
                    match (want, have) {
                        (None, None) => true,
                        (Some(ing), Some(item)) => ing.matches(item),
                        _ => false,
                    }
                })
            });
            if ok {
                return Some(r);
            }
            if r.w == 1 {
                continue 'recipes;
            }
        }
    }
    None
}

/// Consume one item from every occupied cell (after a successful craft).
pub fn consume(grid: &mut [Option<ItemStack>]) {
    for slot in grid.iter_mut() {
        if let Some(s) = slot.as_mut() {
            s.count -= 1;
            if s.count == 0 {
                *slot = None;
            }
        }
    }
}
