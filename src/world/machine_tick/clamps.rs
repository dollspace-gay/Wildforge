//! Clamps machine_tick transaction coordination.

use crate::registry::AIR;
use crate::world::BlockEntity;
use crate::planet::BlockPos;
use crate::world::CLAMP_SECS_PER_LOG;
use crate::world::World;

impl World {
    /// Smolder every clamp; venting burns the exposed log away.
    pub(in crate::world) fn tick_clamps(&mut self, dt: f32) {
        let keys: Vec<BlockPos> = self.installations
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Clamp(_)))
            .map(|(k, _)| *k)
            .collect();
        let logs_tag = self.reg.tags.get("base:logs").cloned().unwrap_or_default();
        for pos in keys {
            let Some(BlockEntity::Clamp(mut c)) = self.installations.remove(&pos) else {
                continue;
            };
            // Logs that stopped being logs (mined) leave the pile.
            c.logs.retain(|at| {
                let b = self.get_block_at(*at);
                self.reg
                    .item_id(&self.reg.block(b).name)
                    .is_some_and(|i| logs_tag.contains(&i))
            });
            // A newly exposed log burns to nothing.
            let mut vented: Option<BlockPos> = None;
            let mut exposed = 0;
            'scan: for p in &c.logs {
                for n in crate::planet::neighbors6(*p) {
                    if c.logs.contains(&n) {
                        continue;
                    }
                    if !self.reg.is_solid(self.get_block_at(n)) {
                        exposed += 1;
                        if exposed > 1 {
                            vented = Some(*p);
                            break 'scan;
                        }
                    }
                }
            }
            if let Some(p) = vented {
                self.set_block_at(p, AIR);
                c.logs.retain(|l| *l != p);
                c.timer -= CLAMP_SECS_PER_LOG;
            }
            if c.logs.is_empty() {
                continue; // the pile is gone; so is the burn
            }
            c.timer -= dt;
            if c.timer <= 0.0 {
                if let Some(cc) = self.reg.block_id("base:charcoal_block") {
                    for p in c.logs.clone() {
                        self.set_block_at(p, cc);
                    }
                }
                continue; // done; entity retires
            }
            self.installations.insert(pos, BlockEntity::Clamp(c));
        }
    }
}
