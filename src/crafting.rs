//! Shaped crafting: registry recipes matched at any offset (and mirrored)
//! inside a 2x2 or 3x3 grid.

use crate::inventory::ItemStack;
use crate::registry::{RecipeDef, Registry};

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
                    let want = r.pattern[y * r.w + px];
                    let have = grid[(min_y + y) * size + (min_x + x)].map(|s| s.item);
                    want == have
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
