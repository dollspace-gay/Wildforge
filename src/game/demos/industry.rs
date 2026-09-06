//! Industry capture scene construction.

use super::DemoChart;
use crate::game::Game;
use crate::inventory::ItemStack;
use crate::planet::EntityPos;
use crate::registry::AIR;
use crate::world;

impl Game {
    pub(super) fn stage_capture_steelworks(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: a ready steelworks near spawn (bloomery shell + anvil +
        // materials) for screenshots and hands-on QA.
        if std::env::var("WILDFORGE_DEMO_STEELWORKS").is_ok() {
            let b = |n: &str| self.content.reg.block_id(n);
            if let (Some(fb), Some(mouth), Some(anvil), Some(floor)) = (
                b("base:firebrick"),
                b("base:bloomery"),
                b("base:stone_anvil"),
                b("base:cobblestone"),
            ) {
                let (sx, sz) = (spawn.x as i32 + 6, spawn.z as i32 + 4);
                // Build the fixture from the player's actual ground plane.
                // `surface_height` includes leaves, which used to perch a
                // bloomery on a tree canopy on forested seeds. A thick,
                // cleared terrace is deterministic on coasts, hills, and
                // wooded starts alike.
                let floor_y = spawn.y.floor() as i32 - 1;
                for x in (spawn.x as i32 + 1)..=(spawn.x as i32 + 9) {
                    for z in (spawn.z as i32 + 1)..=(spawn.z as i32 + 15) {
                        for y in (floor_y - 2)..=floor_y {
                            demo_set!(self.runtime.local_mut().world, chart, x, y, z, floor);
                        }
                        for y in (floor_y + 1)..=(floor_y + 14) {
                            demo_set!(self.runtime.local_mut().world, chart, x, y, z, AIR);
                        }
                    }
                }
                let sy = floor_y + 1;
                // Core at (sx, sy, sz); mouth on its -X side.
                for ly in 0..3 {
                    for rx in -1..=1i32 {
                        for rz in -1..=1i32 {
                            if rx == 0 && rz == 0 {
                                continue;
                            }
                            demo_set!(
                                self.runtime.local_mut().world,
                                chart,
                                sx + rx,
                                sy + ly,
                                sz + rz,
                                fb
                            );
                        }
                    }
                    demo_set!(
                        self.runtime.local_mut().world,
                        chart,
                        sx,
                        sy + ly,
                        sz,
                        crate::registry::AIR
                    );
                }
                demo_set!(self.runtime.local_mut().world, chart, sx - 1, sy, sz, mouth);
                demo_set!(
                    self.runtime.local_mut().world,
                    chart,
                    sx - 3,
                    sy,
                    sz + 2,
                    anvil
                );
                // A second stack, already charged and burning.
                let (lx, lz) = (sx, sz + 8);
                let ly = floor_y + 1;
                for dy in 0..3 {
                    for rx in -1..=1i32 {
                        for rz in -1..=1i32 {
                            if rx == 0 && rz == 0 {
                                continue;
                            }
                            demo_set!(
                                self.runtime.local_mut().world,
                                chart,
                                lx + rx,
                                ly + dy,
                                lz + rz,
                                fb
                            );
                        }
                    }
                    demo_set!(
                        self.runtime.local_mut().world,
                        chart,
                        lx,
                        ly + dy,
                        lz,
                        crate::registry::AIR
                    );
                }
                demo_set!(self.runtime.local_mut().world, chart, lx - 1, ly, lz, mouth);
                let reg2 = self.content.reg.clone();
                if let (Some(iron), Some(coal)) = (
                    reg2.item_id("base:iron_ingot"),
                    reg2.item_id("base:charcoal"),
                ) {
                    let mut st = world::MachineInstance {
                        kind: reg2.machine_kind("base:bloomery").unwrap_or_default(),
                        ..Default::default()
                    };
                    for i in 0..4 {
                        st.charge[i] = Some(ItemStack::new(&reg2, iron, 2));
                        st.fuel[i] = Some(ItemStack::new(&reg2, coal, 2));
                    }
                    demo_insert!(
                        self.runtime.local_mut().world,
                        chart,
                        (lx - 1, ly, lz),
                        world::BlockEntity::Multiblock(st)
                    );
                    let _ = self
                        .runtime
                        .local_mut()
                        .world
                        .light_bloomery_at(chart.block(lx - 1, ly, lz));
                }
                // A bloom resting on the anvil, ready for the hammer.
                if let Some(bl) = reg2.item_id("base:steel_bloom") {
                    demo_anvil_put!(
                        self.runtime.local_mut().world,
                        chart,
                        (sx - 3, sy, sz + 2),
                        ItemStack::new(&reg2, bl, 1),
                    );
                }
                let reg = self.content.reg.clone();
                for (name, n) in [
                    ("base:iron_ingot", 8),
                    ("base:charcoal", 8),
                    ("base:ember", 2),
                    ("base:smith_hammer", 1),
                    ("base:steel_bloom", 2),
                    ("base:log", 8),
                    ("base:dirt", 32),
                ] {
                    if let Some(item) = reg.item_id(name) {
                        self.give_dev_item(&reg, item, n);
                    }
                }
            }
        }
    }

    pub(super) fn stage_capture_glassworks(&mut self, spawn: EntityPos, chart: DemoChart) {
        if std::env::var("WILDFORGE_DEMO_GLASSWORKS").is_ok() {
            let reg = self.content.reg.clone();
            let b = |n: &str| reg.block_id(n);
            if let (Some(fb), Some(kiln), Some(quern)) =
                (b("base:firebrick"), b("base:kiln"), b("base:quern"))
            {
                let (sx, sz) = (spawn.x as i32 + 6, spawn.z as i32 - 6);
                let sy = demo_height!(self.runtime.local().world, chart, sx, sz) + 1;
                for ly in 0..3 {
                    for rx in -1..=1i32 {
                        for rz in -1..=1i32 {
                            if rx == 0 && rz == 0 {
                                continue;
                            }
                            demo_set!(
                                self.runtime.local_mut().world,
                                chart,
                                sx + rx,
                                sy + ly,
                                sz + rz,
                                fb
                            );
                        }
                    }
                    demo_set!(
                        self.runtime.local_mut().world,
                        chart,
                        sx,
                        sy + ly,
                        sz,
                        crate::registry::AIR
                    );
                }
                demo_set!(self.runtime.local_mut().world, chart, sx - 1, sy, sz, kiln);
                demo_set!(
                    self.runtime.local_mut().world,
                    chart,
                    sx - 3,
                    sy,
                    sz + 2,
                    quern
                );
                if let (Some(sand), Some(coal), Some(pow)) = (
                    reg.item_id("base:sand"),
                    reg.item_id("base:charcoal"),
                    reg.item_id("base:cobalt_powder"),
                ) {
                    let mut st = world::MachineInstance {
                        kind: reg.machine_kind("base:kiln").unwrap_or_default(),
                        ..Default::default()
                    };
                    for i in 0..4 {
                        st.charge[i] = Some(ItemStack::new(&reg, sand, 2));
                        st.fuel[i] = Some(ItemStack::new(&reg, coal, 2));
                    }
                    st.reagent = Some(ItemStack::new(&reg, pow, 1));
                    demo_insert!(
                        self.runtime.local_mut().world,
                        chart,
                        (sx - 1, sy, sz),
                        world::BlockEntity::Multiblock(st)
                    );
                    let _ =
                        self.runtime
                            .local_mut()
                            .world
                            .light_kiln_at(chart.block(sx - 1, sy, sz));
                }
                for (name, n) in [
                    ("base:sand", 16),
                    ("base:raw_cobalt", 4),
                    ("base:raw_cinnabar", 4),
                    ("base:charcoal", 8),
                    ("base:ember", 2),
                    ("base:blue_glass", 8),
                    ("base:glass", 8),
                ] {
                    if let Some(item) = reg.item_id(name) {
                        self.give_dev_item(&reg, item, n);
                    }
                }
                // Torches behind stained panes: the light comes out
                // the color of the glass (stage 5's proof).
                let (tx2, tz2) = (spawn.x as i32 - 8, spawn.z as i32 + 2);
                let ty2 = demo_height!(self.runtime.local().world, chart, tx2, tz2) + 1;
                if let (Some(stone), Some(torch), Some(rg), Some(bg)) = (
                    b("base:stone"),
                    b("base:torch"),
                    b("base:red_glass"),
                    b("base:blue_glass"),
                ) {
                    for (i, pane) in [rg, bg].iter().enumerate() {
                        let z = tz2 + i as i32 * 3;
                        // A stone alcove holding a torch, glazed shut.
                        for dy in -1..=1i32 {
                            for dz in -1..=1i32 {
                                demo_set!(
                                    self.runtime.local_mut().world,
                                    chart,
                                    tx2 - 1,
                                    ty2 + dy,
                                    z + dz,
                                    stone
                                );
                                if dy != 0 || dz != 0 {
                                    demo_set!(
                                        self.runtime.local_mut().world,
                                        chart,
                                        tx2,
                                        ty2 + dy,
                                        z + dz,
                                        stone
                                    );
                                }
                            }
                        }
                        demo_set!(self.runtime.local_mut().world, chart, tx2, ty2, z, torch);
                        demo_set!(
                            self.runtime.local_mut().world,
                            chart,
                            tx2 + 1,
                            ty2,
                            z,
                            *pane
                        );
                    }
                }
                // A stained window row so the tint shows in shots.
                let (wx, wz) = (spawn.x as i32 - 5, spawn.z as i32);
                let wy = demo_height!(self.runtime.local().world, chart, wx, wz) + 1;
                for (i, g) in [
                    "base:glass",
                    "base:teal_glass",
                    "base:amber_glass",
                    "base:blue_glass",
                    "base:red_glass",
                    "base:violet_glass",
                ]
                .iter()
                .enumerate()
                {
                    if let Some(gb) = b(g) {
                        demo_set!(
                            self.runtime.local_mut().world,
                            chart,
                            wx,
                            wy,
                            wz + i as i32,
                            gb
                        );
                        demo_set!(
                            self.runtime.local_mut().world,
                            chart,
                            wx,
                            wy + 1,
                            wz + i as i32,
                            gb
                        );
                    }
                }
            }
        }
    }
}
