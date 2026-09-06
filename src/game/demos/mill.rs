//! Mill capture scene construction.

use super::DemoChart;
use crate::game::Game;
use crate::inventory::ItemStack;
use crate::planet::EntityPos;
use crate::registry::AIR;

impl Game {
    pub(super) fn stage_capture_mill(&mut self, spawn: EntityPos, chart: DemoChart) {
        if std::env::var("WILDFORGE_DEMO_MILL").is_ok() {
            // A working millrace: an elevated pool spilling over a
            // lip, the wheel in the fall, gears walking the power
            // down to a millstone, a sawmill, and a helve hammer at
            // its anvil — plus a sail tower for the wind shot.
            let b = |n: &str| self.content.reg.block_id(n);
            let reg2 = self.content.reg.clone();
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            let y = demo_height!(self.runtime.local().world, chart, bx, bz);
            eprintln!("mill demo anchored at ({bx},{y},{bz})");
            if let Some(grass) = b("base:grass") {
                for dx in -10..=10i32 {
                    for dz in -2..=14i32 {
                        let (x, z) = (bx + dx, bz + dz);
                        demo_set!(self.runtime.local_mut().world, chart, x, y, z, grass);
                        for hh in 1..=10 {
                            if demo_get!(self.runtime.local().world, chart, x, y + hh, z) != AIR {
                                demo_set!(self.runtime.local_mut().world, chart, x, y + hh, z, AIR);
                            }
                        }
                    }
                }
            }
            let (mx, mz) = (bx - 3, bz + 5);
            if let (Some(stone), Some(shaft), Some(gear)) =
                (b("base:stone"), b("base:shaft"), b("base:gear"))
            {
                let w = &mut self.runtime.local_mut().world;
                // The raised race runs north-south so the wheel's
                // face greets a camera looking east: stone trough,
                // water pouring out the south lip under the wheel.
                for dz in 1..=8i32 {
                    for dy in 1..=2 {
                        demo_set!(w, chart, mx, y + dy, mz + dz, stone);
                    }
                    demo_set!(w, chart, mx - 1, y + 3, mz + dz, stone);
                    demo_set!(w, chart, mx + 1, y + 3, mz + dz, stone);
                }
                demo_set!(w, chart, mx, y + 3, mz + 9, stone);
                // The wall opens at the south end, downstream of the
                // wheel: the race spills there without starving the
                // cells the wheel actually rides.
                demo_set!(w, chart, mx - 1, y + 3, mz + 1, AIR);
                let water = reg2.water_block(0);
                for dz in 1..=8i32 {
                    demo_set!(w, chart, mx, y + 3, mz + dz, water);
                }
                // The wheel rides mid-race, axle running east — its
                // stream below it, its face to the camera.
                if let Some(wheel) = b("base:water_wheel") {
                    demo_set!(w, chart, mx, y + 4, mz + 3, wheel);
                    demo_insert!(
                        w,
                        chart,
                        (mx, y + 4, mz + 3),
                        crate::world::BlockEntity::Anvil(Default::default()),
                    );
                }
                // The axle line runs four clear blocks off the hub
                // before its down post, so the wheel's face stands
                // alone; stations rank along the working floor.
                for i in 1..=4 {
                    demo_set!(w, chart, mx + i, y + 4, mz + 3, shaft);
                }
                demo_set!(w, chart, mx + 5, y + 4, mz + 3, gear);
                demo_set!(w, chart, mx + 5, y + 3, mz + 3, shaft);
                demo_set!(w, chart, mx + 5, y + 2, mz + 3, shaft);
                demo_set!(w, chart, mx + 5, y + 1, mz + 3, gear);
                demo_set!(w, chart, mx + 6, y + 1, mz + 3, shaft);
                demo_set!(w, chart, mx + 7, y + 1, mz + 3, gear);
                demo_set!(w, chart, mx + 8, y + 1, mz + 3, shaft);
                demo_set!(w, chart, mx + 9, y + 1, mz + 3, gear);
                // Stations step south off their gears, facing camera.
                if let Some(mill) = b("base:millstone") {
                    demo_set!(w, chart, mx + 7, y + 1, mz + 2, mill);
                    if let Some(copper) = reg2.item_id("base:raw_copper") {
                        for _ in 0..4 {
                            demo_anvil_put!(
                                w,
                                chart,
                                (mx + 7, y + 1, mz + 2),
                                ItemStack::new(&reg2, copper, 1),
                            );
                        }
                    }
                }
                if let Some(saw) = b("base:sawmill") {
                    demo_set!(w, chart, mx + 9, y + 1, mz + 2, saw);
                    if let Some(log) = reg2.item_id("base:log") {
                        for _ in 0..3 {
                            demo_anvil_put!(
                                w,
                                chart,
                                (mx + 9, y + 1, mz + 2),
                                ItemStack::new(&reg2, log, 1),
                            );
                        }
                    }
                }
                if let (Some(helve), Some(anvil)) = (b("base:helve_hammer"), b("base:stone_anvil"))
                {
                    demo_set!(w, chart, mx + 9, y + 1, mz + 4, helve);
                    demo_set!(w, chart, mx + 9, y + 1, mz + 5, anvil);
                    if let Some(bl) = reg2.item_id("base:steel_bloom") {
                        demo_anvil_put!(
                            w,
                            chart,
                            (mx + 9, y + 1, mz + 5),
                            ItemStack::new(&reg2, bl, 1),
                        );
                    }
                }
                // The machine shop row: crude lathe west, iron lathe
                // east, the vice that lets precision cut at all.
                demo_set!(w, chart, mx + 5, y + 1, mz + 2, shaft);
                demo_set!(w, chart, mx + 5, y + 1, mz + 1, gear);
                if let (Some(lathe), Some(ilathe), Some(vice)) =
                    (b("base:lathe"), b("base:iron_lathe"), b("base:vice"))
                {
                    demo_set!(w, chart, mx + 4, y + 1, mz + 1, lathe);
                    demo_set!(w, chart, mx + 6, y + 1, mz + 1, ilathe);
                    demo_set!(w, chart, mx + 5, y + 1, mz, vice);
                    if let Some(cu) = reg2.item_id("base:copper_ingot") {
                        demo_anvil_put!(
                            w,
                            chart,
                            (mx + 4, y + 1, mz + 1),
                            ItemStack::new(&reg2, cu, 1),
                        );
                    }
                    if let Some(fe) = reg2.item_id("base:iron_ingot") {
                        demo_anvil_put!(
                            w,
                            chart,
                            (mx + 6, y + 1, mz + 1),
                            ItemStack::new(&reg2, fe, 1),
                        );
                    }
                }
                // The electric age: a generator off the shop gear,
                // arc lamps drinking its field, the steam corner,
                // and a separator on its firebrick stack.
                if let Some(dynamo) = b("base:generator") {
                    demo_set!(w, chart, mx + 7, y + 1, mz + 4, dynamo);
                    demo_insert!(
                        w,
                        chart,
                        (mx + 7, y + 1, mz + 4),
                        crate::world::BlockEntity::Anvil(Default::default()),
                    );
                }
                for (lamp, lx, lz) in [
                    ("base:arc_lamp", mx + 5, mz + 6),
                    ("base:blue_arc_lamp", mx + 7, mz + 6),
                    ("base:red_arc_lamp", mx + 9, mz + 6),
                ] {
                    if let Some(l) = b(lamp) {
                        demo_set!(w, chart, lx, y + 2, lz, l);
                        demo_set!(w, chart, lx, y + 1, lz, stone);
                    }
                }
                if let (Some(fbx), Some(boiler), Some(engine)) =
                    (b("base:firebox"), b("base:boiler"), b("base:steam_engine"))
                {
                    let (ex, ez) = (mx + 12, mz + 1);
                    demo_set!(w, chart, ex, y + 1, ez, fbx);
                    demo_set!(w, chart, ex, y + 2, ez, boiler);
                    demo_set!(w, chart, ex + 1, y + 2, ez, engine);
                    demo_insert!(
                        w,
                        chart,
                        (ex, y + 1, ez),
                        crate::world::BlockEntity::Steam(crate::world::SteamState {
                            fuel: 900.0,
                            water: crate::planet_atlas::ReservoirMass::fresh(
                                60 * crate::planet_atlas::HYDRO_UNITS_PER_BLOCK,
                            ),
                            draft_closed: false,
                            steam_numerator_remainder: 0,
                        }),
                    );
                }
                if let (Some(fb), Some(sep)) = (b("base:firebrick"), b("base:separator")) {
                    let (px, pz) = (bx - 8, bz + 1);
                    for ly in 1..=3 {
                        for rx in -1..=1i32 {
                            for rz in -1..=1i32 {
                                if rx == 0 && rz == 0 {
                                    continue;
                                }
                                demo_set!(w, chart, px + 1 + rx, y + ly, pz + rz, fb);
                            }
                        }
                    }
                    demo_set!(w, chart, px, y + 1, pz, sep);
                    demo_insert!(
                        w,
                        chart,
                        (px, y + 1, pz),
                        crate::world::BlockEntity::Multiblock(crate::world::MachineInstance {
                            kind: w.reg.machine_kind("base:separator").unwrap_or_default(),
                            powder: 4,
                            separator_fuel: 4,
                            ..Default::default()
                        }),
                    );
                }
                // Boring mill and pump join the shop floor.
                if let (Some(bore), Some(pump)) = (b("base:boring_mill"), b("base:pump")) {
                    demo_set!(w, chart, mx + 2, y + 1, mz + 1, bore);
                    demo_set!(w, chart, mx + 2, y + 1, mz, pump);
                    demo_insert!(
                        w,
                        chart,
                        (mx + 2, y + 1, mz),
                        crate::world::BlockEntity::Anvil(Default::default()),
                    );
                }
                // The sail tower: altitude is the windmill's river.
                if let Some(sail) = b("base:windmill_sail") {
                    let tx = bx + 8;
                    for ty in (y + 1)..=91 {
                        demo_set!(w, chart, tx, ty, bz + 10, stone);
                    }
                    demo_set!(w, chart, tx, 92, bz + 10, sail);
                    demo_insert!(
                        w,
                        chart,
                        (tx, 92, bz + 10),
                        crate::world::BlockEntity::Anvil(Default::default()),
                    );
                    // And one at eye level for the mesh to be judged
                    // (too low to ever turn; that's the point).
                    demo_set!(w, chart, bx + 6, y + 2, bz + 1, stone);
                    demo_set!(w, chart, bx + 6, y + 3, bz + 1, sail);
                }
            }
        }
    }
}
