//! Lightning calendar transaction coordination.

use crate::world::World;

impl World {
    /// The wild's own hand: a bolt out of an ire storm. Strikes only
    /// natural, untouched country; chars grass or dirt to max-fertile
    /// scorch and banks bloom in the cell. Returns the struck cell.
    pub fn lightning_strike_at(
        &mut self,
        surface: crate::planet::SurfacePos,
    ) -> Option<crate::planet::BlockPos> {
        let cp = crate::planet::ChunkPos::from_surface(surface);
        // The invariant, absolute: the wild never touches what
        // players BUILT — a touched chunk is off the target list.
        if self.player_touched.contains(&cp) {
            return None;
        }
        let y = self.surface_height_at(surface);
        if y <= 2 {
            return None;
        }
        let struck =
            crate::planet::BlockPos::new(surface.face(), surface.u(), y as u8, surface.v()).ok()?;
        let name = self.reg.block(self.get_block_at(struck)).name.clone();
        self.add_bloom_at_surface(surface, 3.0);
        if (name == "base:grass" || name == "base:dirt")
            && let Some(ch) = self.reg.block_id("base:charred_soil")
        {
            self.set_block_at(struck, ch);
        }
        // And it starts a fire, which is the wild's to own: it pays
        // bloom where it burns and will not cross onto worked ground.
        if let Some(above) = struck.offset(0, 1, 0) {
            self.light_fire_at(above, false);
        }
        Some(struck)
    }

    #[cfg(test)]
    pub fn lightning_strike(&mut self, x: i32, z: i32) -> Option<(i32, i32, i32)> {
        let surface =
            crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z).ok()?;
        self.lightning_strike_at(surface)
            .map(crate::planet::BlockPos::centered)
    }
}
