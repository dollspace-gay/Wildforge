//! Renewable growth calendar transaction coordination.

use crate::registry::AIR;
use crate::registry::BlockId;
use crate::world::World;

impl World {
    /// Grow a planted sapling into a full tree, mirroring the worldgen
    /// shapes. Returns false (sapling stays) if the trunk is blocked.
    pub fn grow_tree_at(&mut self, pos: crate::planet::BlockPos, species: &str, rnd: u32) -> bool {
        let reg = self.reg.clone();
        let ids = |l: &str, f: &str| Some((reg.block_id(l)?, reg.block_id(f)?));
        let Some((log, leaf)) = (match species {
            "birch" => ids("base:birch_log", "base:birch_leaves"),
            "spruce" => ids("base:spruce_log", "base:spruce_leaves"),
            "jungle" => ids("base:jungle_log", "base:jungle_leaves"),
            "acacia" => ids("base:acacia_log", "base:acacia_leaves"),
            _ => ids("base:log", "base:leaves"),
        }) else {
            return false;
        };
        let trunk_h = match species {
            "acacia" => 1,
            "spruce" => 5 + (rnd % 3) as i32,
            "jungle" => 6 + (rnd % 3) as i32,
            _ => 4 + (rnd % 3) as i32,
        };
        // Clearance: the trunk column (above the sapling cell) must be open.
        for dy in 1..=trunk_h + 1 {
            if pos
                .offset(0, dy, 0)
                .is_none_or(|at| self.get_block_at(at) != AIR)
            {
                return false;
            }
        }
        let leaf_at = |w: &mut World, dx: i32, dy: i32, dz: i32| {
            if let Some(at) = pos.offset(dx, dy, dz)
                && at.y() > 0
                && w.get_block_at(at) == AIR
            {
                w.set_block_at(at, leaf);
            }
        };
        for dy in 0..trunk_h {
            if let Some(at) = pos.offset(0, dy, 0) {
                self.set_block_at(at, log);
            }
        }
        match species {
            "acacia" => {
                for dx in -1..=1 {
                    for dz in -1..=1 {
                        leaf_at(self, dx, trunk_h, dz);
                    }
                }
            }
            "spruce" => {
                for (dy, r) in [(-3i32, 2i32), (-2, 1), (-1, 2), (0, 1), (1, 1)] {
                    for dx in -r..=r {
                        for dz in -r..=r {
                            if dx.abs() == r && dz.abs() == r && r > 1 {
                                continue;
                            }
                            if dx == 0 && dz == 0 && dy < 0 {
                                continue;
                            }
                            leaf_at(self, dx, trunk_h + dy, dz);
                        }
                    }
                }
                leaf_at(self, 0, trunk_h + 2, 0);
            }
            _ => {
                let big: i32 = if species == "jungle" { 3 } else { 2 };
                for (dy, r) in [(-2i32, big), (-1, big), (0, 1), (1, 1)] {
                    for dx in -r..=r {
                        for dz in -r..=r {
                            if dx == 0 && dz == 0 && dy < 0 {
                                continue;
                            }
                            leaf_at(self, dx, trunk_h + dy, dz);
                        }
                    }
                }
            }
        }
        true
    }

    /// Ire cost of breaking a block, by what it is.
    pub fn ire_for_block(&self, b: BlockId) -> f32 {
        let name = &self.reg.block(b).name;
        if name.ends_with("_log") || name.ends_with(":log") {
            0.3
        } else if name.contains("ore") {
            0.4
        } else if name.ends_with("stone") && !name.contains("cobble") {
            0.05
        } else if name.contains("leaves") || name.ends_with("dirt") || name.ends_with("grass") {
            0.02
        } else {
            0.0
        }
    }
}
