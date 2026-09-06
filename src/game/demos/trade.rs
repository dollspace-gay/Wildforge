//! Trade capture scene construction.

use crate::world::TerrainRead;
use crate::inventory::ItemStack;
use crate::registry::AIR;
use crate::world;
use glam::Vec3;
use crate::game::Game;
use crate::game::navigation::Screen;
use crate::planet::{BlockPos, EntityPos, Face, SurfacePos};
use super::DemoChart;

impl Game {
    pub(in crate::game) fn stage_capture_trade(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: a dusk campsite for the README hero shot — torch posts
        // throwing hard shadows across the grass, a blue-glass lantern
        // staining its pool, a chest and anvil for life.
        // Dev: the whole trade arc in one clearing — stall (stocked),
        // sign, waystone, saddlebagged deer, and a boat on a dug pond.
        if std::env::var("WILDFORGE_DEMO_TRADE").is_ok() {
            let b = |n: &str| self.content.reg.block_id(n);
            let reg2 = self.content.reg.clone();
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            for dx in [-16i32, 0, 16] {
                for dz in [-16i32, 0, 16] {
                    self.runtime.local_mut().world.ensure_chunk(chart.chunk(bx + dx, bz + dz));
                }
            }
            let y = demo_height!(self.runtime.local().world, chart, bx, bz);
            // Clear and floor the clearing.
            if let Some(grass) = b("base:grass") {
                for dx in -10..=10i32 {
                    for dz in -4..=14i32 {
                        let (x, z) = (bx + dx, bz + dz);
                        demo_set!(self.runtime.local_mut().world, chart, x, y, z, grass);
                        for h in 1..=8 {
                            if demo_get!(self.runtime.local().world, chart, x, y + h, z) != AIR {
                                demo_set!(self.runtime.local_mut().world, chart, x, y + h, z, AIR);
                            }
                        }
                    }
                }
            }
            // The stall, stocked and priced.
            if let (Some(counter), Some(log), Some(planks)) =
                (b("base:stall_counter"), b("base:log"), b("base:planks"))
            {
                let (sx, sz) = (bx - 4, bz + 6);

                demo_set!(self.runtime.local_mut().world, chart, sx, y + 1, sz, counter);
                for side in [-1i32, 1] {
                    demo_set!(self.runtime.local_mut().world, chart, sx + side, y + 1, sz, log);
                    demo_set!(self.runtime.local_mut().world, chart, sx + side, y + 2, sz, log);
                }
                for i in -1i32..=1 {
                    demo_set!(self.runtime.local_mut().world, chart, sx + i, y + 3, sz, planks);
                }
                let mut st = crate::world::StallState {
                    owner: [7; 16],
                    owner_name: "MERI".to_string(),
                    ..Default::default()
                };
                if let (Some(salt), Some(silver)) = (
                    reg2.item_id("base:salted_meat"),
                    reg2.item_id("base:silver_ingot"),
                ) {
                    st.goods[0] = Some(ItemStack::new(&reg2, salt, 12));
                    st.price = Some(ItemStack::new(&reg2, silver, 1));
                }
                demo_insert!(
                    self.runtime.local_mut().world,
                    chart,
                    (sx, y + 1, sz),
                    crate::world::BlockEntity::Stall(st)
                );
            }
            // A sign and a named waystone.
            if let Some(sign) = b("base:sign") {
                demo_set!(self.runtime.local_mut().world, chart, bx, y + 1, bz + 6, sign);
                demo_insert!(
                    self.runtime.local_mut().world,
                    chart,
                    (bx, y + 1, bz + 6),
                    crate::world::BlockEntity::Sign(crate::world::SignState {
                        lines: [
                            "SALT FAIR".to_string(),
                            "PRICES".to_string(),
                            "ASK MERI".to_string(),
                        ],
                    }),
                );
            }
            if let Some(ws) = b("base:waystone") {
                demo_set!(self.runtime.local_mut().world, chart, bx + 3, y + 1, bz + 6, ws);
                demo_insert!(
                    self.runtime.local_mut().world,
                    chart,
                    (bx + 3, y + 1, bz + 6),
                    crate::world::BlockEntity::Sign(crate::world::SignState {
                        lines: ["THREE PINES".to_string(), String::new(), String::new()],
                    }),
                );
            }
            // A saddlebagged deer at the hitching post.
            if let Some(di) = reg2.animal_id("base:deer") {
                let mut deer = demo_mob!(
                    chart,
                    di,
                    glam::Vec3::new(bx as f32 + 5.5, y as f32 + 1.0, bz as f32 + 7.5),
                    2.4,
                );
                deer.health = reg2.animals[di].health;
                deer.tamed = true;
                deer.calm = 100000.0;
                deer.cargo = Some(Default::default());
                self.runtime.local_mut().world.spawn_mob(deer);
            }
            // A dug pond with a boat riding it.
            if let (Some(water), Some(dirt), Some(bi)) =
                (b("base:water"), b("base:dirt"), reg2.animal_id("base:boat"))
            {
                for dx in 6..=9i32 {
                    for dz in 0..=3i32 {
                        // A sealed bowl: solid under the water so the
                        // pond can't drain into a cave.
                        demo_set!(self.runtime.local_mut().world, chart, bx + dx, y - 1, bz + dz, dirt);
                        demo_set!(self.runtime.local_mut().world, chart, bx + dx, y, bz + dz, water);
                    }
                }
                let mut boat = demo_mob!(
                    chart,
                    bi,
                    glam::Vec3::new(bx as f32 + 7.5, y as f32 + 0.9, bz as f32 + 1.5),
                    0.8,
                );
                boat.health = reg2.animals[bi].health;
                boat.tamed = true;
                self.runtime.local_mut().world.spawn_mob(boat);
            }
            // Screen shortcuts want the scene to exist first.
            match std::env::var("WILDFORGE_SCREEN").as_deref() {
                Ok("stall") => self.set_screen(Screen::Stall(chart.block(bx - 4, y + 1, bz + 6))),
                Ok("signedit") => {
                    self.ui_state.sign_lines =
                        ["SALT FAIR".to_string(), "PRICES".to_string(), String::new()];
                    self.ui_state.sign_line = 2;
                    self.set_screen(Screen::SignEdit(chart.block(bx, y + 1, bz + 6)));
                }
                Ok("mobcargo") => {
                    // Ids are sim-assigned: run one tick so the demo
                    // deer exists on the wire before we key by id.
                    let mut rng = 1u32;
                    let _ = self.runtime.local_mut().world.tick_mobs(
                        &[crate::server::PlayerCtx {
                            id: 0,
                            pos: spawn,
                            spawn,
                            attackable: false,
                            aggro_mod: 0.0,
                            quiet_charm: None,
                        }],
                        1.0,
                        0.01,
                        &mut rng,
                    );
                    let id = self.runtime.view().mobs()
                        .iter()
                        .find(|m| m.cargo.is_some() && m.id != 0)
                        .map(|m| m.id);
                    if let Some(id) = id {
                        // A little salt rides along for the screenshot.
                        if let Some(salt) = reg2.item_id("base:salt_crystal")
                            && let Some(m) = self.runtime.local_mut().world.mob_by_id_mut(id)
                            && let Some(cargo) = m.cargo.as_mut()
                        {
                            cargo[0] = Some(ItemStack::new(&reg2, salt, 24));
                            cargo[5] = Some(ItemStack::new(&reg2, salt, 8));
                        }
                        for count in [24, 8] {
                            if let Some(salt) = reg2.item_id("base:salt_crystal")
                                && let Err(error) = self.runtime.local_mut().world.record_external_stack(
                                    ItemStack::new(&reg2, salt, count),
                                    "development capture animal cargo",
                                )
                            {
                                eprintln!("materials: capture cargo source failed: {error}");
                            }
                        }
                        self.set_screen(Screen::MobCargo(id));
                    }
                }
                _ => {}
            }
        }
    }
}
