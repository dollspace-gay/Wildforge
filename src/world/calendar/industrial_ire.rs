//! Industrial ire calendar transaction coordination.

use crate::world::BlockEntity;
use crate::world::RegionCell;
use crate::world::World;

impl World {
    /// Charge the region for every lit fire machine on a one-second beat.
    pub fn tick_industrial_ire(&mut self, dt: f32) {
        if !self.ruleset().ire || !self.ruleset().industrial_ire {
            return;
        }
        let Some(step) = self.installations.industrial_cycle(dt) else {
            return;
        };
        let lit: Vec<crate::planet::SurfacePos> = self
            .installations
            .iter()
            .filter_map(|(pos, e)| {
                let BlockEntity::Multiblock(m) = e else {
                    return None;
                };
                let handler = m.kind.handler(&self.reg)?;
                (handler.has_fire() && m.lit).then(|| pos.surface())
            })
            .collect();
        let amt = step * Self::INDUSTRIAL_IRE_PER_SEC * lit.len() as f32;
        if amt <= 0.0 {
            return;
        }
        // One charge per distinct region cell: a workshop row smokes as
        // one chimney, not four.
        let mut cells: std::collections::HashSet<RegionCell> = std::collections::HashSet::new();
        for surface in &lit {
            cells.insert(RegionCell::from_surface(*surface));
        }
        for cell in cells {
            let Some(surface) = cell.any_surface() else {
                continue;
            };
            self.add_ire_at_surface(surface, amt);
        }
    }
}
