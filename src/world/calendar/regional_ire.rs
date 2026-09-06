//! Regional ire calendar transaction coordination.

use crate::world::RegionCell;
use crate::world::World;

impl World {
    // ---------------- ire (reciprocity) ----------------

    /// The land's local standing at a canonical planetary surface cell.
    pub fn regional_ire_at_surface(&self, pos: crate::planet::SurfacePos) -> f32 {
        self.regional_ire
            .get(&RegionCell::from_surface(pos))
            .copied()
            .unwrap_or(0.0)
    }

    /// The land's local standing: negative is tended, positive is
    /// aggrieved, clamped to a grudge the wild can actually hold.
    #[cfg(test)]
    pub fn regional_ire_at(&self, x: i32, z: i32) -> f32 {
        self.regional_ire_at_surface(
            crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
                .expect("legacy regional coordinate is within the bounded porting window"),
        )
    }

    pub(super) fn charge_cell(&mut self, cell: RegionCell, amt: f32) {
        let e = self.regional_ire.entry(cell).or_insert(0.0);
        *e = (*e + amt).clamp(-20.0, 20.0);
        if e.abs() < 0.01 {
            self.regional_ire.remove(&cell);
        }
    }

    /// Taking, placed: the world remembers, and so does the valley.
    #[cfg(test)]
    pub fn add_ire_at(&mut self, x: i32, z: i32, amt: f32) {
        let pos = crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
            .expect("legacy regional coordinate is within the bounded porting window");
        self.add_ire_at_surface(pos, amt);
    }

    pub fn add_ire_at_surface(&mut self, pos: crate::planet::SurfacePos, amt: f32) {
        self.add_ire(amt);
        self.charge_cell(RegionCell::from_surface(pos), amt);
    }

    /// Mending, placed: the global refund keeps its daily cap, but the
    /// valley always notices the hands that tend it.
    #[cfg(test)]
    pub fn plant_ire_at(&mut self, x: i32, z: i32, amt: f32) {
        let pos = crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
            .expect("legacy regional coordinate is within the bounded porting window");
        self.plant_ire_at_surface(pos, amt);
    }

    pub fn plant_ire_at_surface(&mut self, pos: crate::planet::SurfacePos, amt: f32) {
        self.plant_ire(amt);
        self.charge_cell(RegionCell::from_surface(pos), -amt);
        // Tending is also how a cell earns back its bloom.
        self.ease_bloom_debt_at_surface(pos, amt);
    }
}
