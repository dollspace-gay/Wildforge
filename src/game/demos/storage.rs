//! Storage capture scene construction.

use crate::world::TerrainRead;
use crate::inventory::ItemStack;
use crate::world;
use crate::game::Game;
use crate::game::navigation::Screen;
use crate::planet::{BlockPos, EntityPos, Face, SurfacePos};
use super::DemoChart;

impl Game {
    pub(in crate::game) fn stage_capture_chest(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: a stocked chest next to spawn, screen open (UI verification).
        if std::env::var("WILDFORGE_DEMO_CHEST").is_ok() {
            let p = (spawn.x as i32 - 2, spawn.y as i32, spawn.z as i32);
            let reg = self.content.reg.clone();
            if let Some(cb) = reg.block_id("base:chest") {
                demo_set!(self.runtime.local_mut().world, chart, p.0, p.1, p.2, cb);
                let mut st = world::ChestState::default();
                for (i, (name, n)) in [
                    ("base:bread", 5),
                    ("base:torch", 12),
                    ("base:bronze_sword", 1),
                ]
                .iter()
                .enumerate()
                {
                    if let Some(item) = reg.item_id(name) {
                        st.slots[i * 4] = Some(ItemStack::new(&reg, item, *n));
                    }
                }
                demo_insert!(self.runtime.local_mut().world, chart, p, world::BlockEntity::Chest(st));
                self.set_screen(Screen::Chest(chart.block_tuple(p)));
            }
        }
    }

    pub(in crate::game) fn stage_capture_furnace(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: a stocked furnace next to spawn, screen open (UI verification).
        if std::env::var("WILDFORGE_DEMO_FURNACE").is_ok() {
            let p = (spawn.x as i32 + 2, spawn.y as i32, spawn.z as i32);
            let reg = self.content.reg.clone();
            if let (Some(fb), Some(raw), Some(log)) = (
                reg.block_id("base:furnace"),
                reg.item_id("base:raw_copper"),
                reg.item_id("base:log"),
            ) {
                demo_set!(self.runtime.local_mut().world, chart, p.0, p.1, p.2, fb);
                demo_insert!(
                    self.runtime.local_mut().world,
                    chart,
                    p,
                    world::BlockEntity::Furnace(world::FurnaceState {
                        input: Some(ItemStack::new(&reg, raw, 5)),
                        fuel: Some(ItemStack::new(&reg, log, 3)),
                        ..Default::default()
                    }),
                );
                self.give_dev_item(&reg, reg.item_id("base:copper_ingot").unwrap(), 7);
                self.set_screen(Screen::Furnace(chart.block_tuple(p)));
            }
        }
    }
}
