//! Reciprocity calendar transaction coordination.

use crate::registry::AIR;
use crate::world::BLOOM_EXHAUSTION;
use crate::world::RegionCell;
use crate::world::World;

impl World {
    // ---------------- the bloom (wrath as renewal) ----------------

    /// Days of bloom left in a cell: lightning strikes and fallen
    /// wardens charge it; charged country erupts — the green tide
    /// runs hot, flowers and fungi sprout, bushes refruit. The titan
    /// levels the valley and the jungle follows it home.
    #[cfg(test)]
    pub fn bloom_at(&self, x: i32, z: i32) -> f32 {
        let pos = crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
            .expect("legacy regional coordinate is within the bounded porting window");
        self.bloom_at_surface(pos)
    }

    pub fn bloom_at_surface(&self, pos: crate::planet::SurfacePos) -> f32 {
        self.bloom
            .get(&RegionCell::from_surface(pos))
            .copied()
            .unwrap_or(0.0)
    }

    /// Bank a bloom — but the ground's willingness is finite. A cell
    /// bloomed over and over and never tended gives less each time,
    /// and finally nothing: the storm's gift is not a faucet, and
    /// farming the wild's rage spends something real.
    #[cfg(test)]
    pub fn add_bloom(&mut self, x: i32, z: i32, days: f32) {
        let pos = crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
            .expect("legacy regional coordinate is within the bounded porting window");
        self.add_bloom_at_surface(pos, days);
    }

    pub fn add_bloom_at_surface(&mut self, pos: crate::planet::SurfacePos, days: f32) {
        let cell = RegionCell::from_surface(pos);
        let spent = self.bloom_spent.get(&cell).copied().unwrap_or(0.0);
        let yield_frac = (1.0 - spent / BLOOM_EXHAUSTION).clamp(0.0, 1.0);
        let given = days * yield_frac;
        if given <= 0.01 {
            return;
        }
        *self.bloom_spent.entry(cell).or_insert(0.0) += given;
        let e = self.bloom.entry(cell).or_insert(0.0);
        *e = (*e + given).min(9.0);
    }

    /// Tending pays the ground back its willingness to bloom.
    pub fn ease_bloom_debt_at_surface(&mut self, pos: crate::planet::SurfacePos, amount: f32) {
        let cell = RegionCell::from_surface(pos);
        if let Some(v) = self.bloom_spent.get_mut(&cell) {
            *v = (*v - amount).max(0.0);
            if *v <= 0.01 {
                self.bloom_spent.remove(&cell);
            }
        }
    }

    /// A hostile fell here: the wild reclaims its own, extravagantly.
    /// Dryads put up a sapling where they stood.
    #[cfg(test)]
    pub fn wild_falls(&mut self, species_name: &str, x: i32, y: i32, z: i32) {
        if let Some(pos) = crate::planet::BlockPos::of_world(x, y, z) {
            self.wild_falls_at(species_name, pos);
        }
    }

    pub fn wild_falls_at(&mut self, species_name: &str, pos: crate::planet::BlockPos) {
        self.add_bloom_at_surface(pos.surface(), 1.0);
        if species_name.contains("dryad")
            && self.get_block_at(pos) == AIR
            && pos.offset(0, -1, 0).is_some_and(|below| {
                self.reg
                    .block(self.get_block_at(below))
                    .name
                    .contains("grass")
            })
            && let Some(sap) = self.reg.block_id("base:oak_sapling")
        {
            self.set_block_at(pos, sap);
        }
    }
}
