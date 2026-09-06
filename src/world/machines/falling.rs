//! Falling machines transaction coordination.

use crate::chunk::CHUNK_Y;
use crate::inventory::ItemStack;
use crate::registry::AIR;
use crate::registry::BlockId;
use crate::world::FallingBlock;
use crate::world::World;

impl World {
    pub fn falling_blocks(&self) -> &[FallingBlock] {
        &self.falling
    }

    /// Lift a block out of the grid and into the air (atomically: the
    /// cell empties in the same call, so it can't be duped).
    pub(in crate::world) fn detach_at(&mut self, pos: crate::planet::BlockPos, b: BlockId) {
        self.set_block_at(pos, AIR);
        self.falling.push(FallingBlock {
            pos: crate::planet::EntityPos::new(
                pos.face(),
                pos.u() as f32,
                pos.y() as f32,
                pos.v() as f32,
            )
            .expect("block corner is a canonical entity position"),
            vel: 0.0,
            block: b,
        });
    }

    /// Advance airborne blocks; landings re-plant (popping any plant or
    /// layer they crush) and re-trigger the cell above the launch site
    /// through the normal edit cascade.
    pub fn tick_falling(&mut self, dt: f32) {
        if self.falling.is_empty() {
            return;
        }
        // Landings apply immediately so a stacked column settles one on
        // top of the other instead of racing into the same cell.
        let mut fallen = std::mem::take(&mut self.falling);
        let mut still = Vec::with_capacity(fallen.len());
        for mut f in fallen.drain(..) {
            f.vel = (f.vel + 20.0 * dt).min(30.0);
            let Ok(moved) = f.pos.translated(glam::Vec3::new(0.0, -f.vel * dt, 0.0)) else {
                continue;
            };
            f.pos = moved.pos;
            let surface = crate::planet::SurfacePos::new(
                f.pos.face(),
                f.pos.u().floor() as u16,
                f.pos.v().floor() as u16,
            )
            .expect("canonical falling position has a valid surface cell");
            let below = f.pos.y.floor() as i32;
            if below < 0 {
                continue; // out of the world (should be impossible)
            }
            let at = |y: i32| {
                crate::planet::BlockPos::new(surface.face(), surface.u(), y as u8, surface.v())
                    .expect("falling block height is inside the shell")
            };
            if !self.reg.is_solid(self.get_block_at(at(below))) {
                still.push(f);
                continue;
            }
            // Land on the first free cell above the obstruction - a
            // second sand in the same column stacks instead of popping.
            let mut y = below + 1;
            while y < CHUNK_Y as i32 - 1 && self.reg.is_solid(self.get_block_at(at(y))) {
                y += 1;
            }
            let b = f.block;
            let cur = self.get_block_at(at(y));
            if cur != AIR {
                // Crushed: the plant/layer pops as its drop first.
                if let Some((item, n)) = self.reg.block(cur).drops {
                    let reg = self.reg.clone();
                    self.push_drop_at(at(y), ItemStack::new(&reg, item, n));
                }
            }
            self.set_block_at(at(y), b);
        }
        // Landings may have detached more (rare); keep both sets.
        self.falling.extend(still);
    }

    /// Land every airborne block instantly (world save/quit).
    pub fn settle_falling(&mut self) {
        while !self.falling.is_empty() {
            self.tick_falling(0.5);
        }
    }
}
