//! Clamps machines transaction coordination.

use crate::planet::BlockPos;
use crate::world::BlockEntity;
use crate::world::CLAMP_SECS_PER_LOG;
use crate::world::ClampState;
use crate::world::World;
use std::collections::HashSet;

impl World {
    /// Flood-fill a covered log pile from the clicked log and light it.
    /// Exactly one face (the lighting face) may be exposed.
    pub fn try_light_clamp_at(&mut self, pos: BlockPos) -> Result<usize, &'static str> {
        let logs_tag = self.reg.tags.get("base:logs").cloned().unwrap_or_default();
        let is_log = |w: &World, p: BlockPos| {
            let b = w.get_block_at(p);
            w.reg
                .item_id(&w.reg.block(b).name)
                .is_some_and(|i| logs_tag.contains(&i))
        };
        if !is_log(self, pos) {
            return Err("light a log");
        }
        let mut set = HashSet::from([pos]);
        let mut logs = vec![pos];
        let mut queue = vec![pos];
        while let Some(p) = queue.pop() {
            for n in crate::planet::neighbors6(p) {
                if !set.contains(&n) && is_log(self, n) {
                    set.insert(n);
                    logs.push(n);
                    if set.len() > 8 {
                        return Err("the pile is too big to smolder (8 logs at most)");
                    }
                    queue.push(n);
                }
            }
        }
        if set.len() < 2 {
            return Err("a clamp needs at least 2 logs");
        }
        let mut exposed = 0;
        for p in &set {
            for n in crate::planet::neighbors6(*p) {
                if set.contains(&n) {
                    continue;
                }
                if !self.reg.is_solid(self.get_block_at(n)) {
                    exposed += 1;
                }
            }
        }
        if exposed > 1 {
            return Err("cover the pile with earth (one face open)");
        }
        let n = set.len();
        self.installations.insert(
            pos,
            BlockEntity::Clamp(ClampState {
                logs,
                timer: n as f32 * CLAMP_SECS_PER_LOG,
            }),
        );
        Ok(n)
    }
}
