//! Stewardship capture scene construction.

use crate::world::TerrainRead;
use crate::inventory::ItemStack;
use crate::registry::AIR;
use crate::world;
use glam::Vec3;
use crate::game::Game;
use crate::planet::{BlockPos, EntityPos, Face, SurfacePos};
use super::DemoChart;

impl Game {
    pub(in crate::game) fn stage_capture_ecology(&mut self, spawn: EntityPos, chart: DemoChart) {
        if std::env::var("WILDFORGE_DEMO_ECO").is_ok() {
            // The living-soil field: four fertility bands, palest dust
            // to deepest loam, wheat standing on the two rich bands —
            // the tint gradient is the shot.
            let b = |n: &str| self.content.reg.block_id(n);
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            let y = demo_height!(self.runtime.local().world, chart, bx, bz);
            eprintln!("eco demo anchored at ({bx},{y},{bz})");
            if let (Some(grass), Some(farm)) = (b("base:grass"), b("base:farmland")) {
                let w = &mut self.runtime.local_mut().world;
                for dx in -8..=8i32 {
                    for dz in -2..=12i32 {
                        let (x, z) = (bx + dx, bz + dz);
                        demo_set!(w, chart, x, y, z, grass);
                        for hh in 1..=8 {
                            if demo_get!(w, chart, x, y + hh, z) != AIR {
                                demo_set!(w, chart, x, y + hh, z, AIR);
                            }
                        }
                    }
                }
                // Bands run away from the camera, three columns each.
                for (band, fert) in [(0i32, 8u8), (1, 24), (2, 40), (3, 60)] {
                    for dx in 0..3i32 {
                        for dz in 2..=9i32 {
                            let x = bx - 6 + band * 3 + dx;
                            demo_meta!(
                                w,
                                chart,
                                x,
                                y,
                                bz + dz,
                                farm,
                                crate::world::soil::soil_meta(fert, 0),
                            );
                        }
                    }
                }
                if let Some(ripe) = b("base:wheat_seeds/stage2") {
                    for band in [2i32, 3] {
                        for dx in 0..3i32 {
                            for dz in 2..=9i32 {
                                if (dx + dz) % 2 == 0 {
                                    let x = bx - 6 + band * 3 + dx;
                                    demo_set!(w, chart, x, y + 1, bz + dz, ripe);
                                }
                            }
                        }
                    }
                }
                // The belly's corner: a compost heap pair by the field
                // and a plank pen with two deer (their dung on the
                // ground), plus one hungry doe loose by the dust band
                // — the raid, caught walking.
                if let (Some(heap), Some(ready)) =
                    (b("base:compost_heap"), b("base:compost_heap_ready"))
                {
                    demo_meta!(w, chart, bx + 7, y + 1, bz + 2, heap, 6);
                    demo_set!(w, chart, bx + 7, y + 1, bz + 4, ready);
                }
                if let Some(plank) = b("base:planks") {
                    for dx in -8..=-4i32 {
                        for dz in 10..=13i32 {
                            if dx == -8 || dx == -4 || dz == 10 || dz == 13 {
                                demo_set!(w, chart, bx + dx, y + 1, bz + dz, plank);
                            }
                        }
                    }
                }
            }
            let reg2 = self.content.reg.clone();
            if let Some(si) = reg2.animal_id("base:deer") {
                let w = &mut self.runtime.local_mut().world;
                // A built demo is tended country: calm animals mind
                // the pen walls here, as they would around any base.
                for cx in -2..=2i32 {
                    for cz in -2..=2i32 {
                        let cp = chart.chunk(bx + cx * 16, bz + cz * 16);
                        w.player_touched.insert(cp);
                    }
                }
                for (dx, dz, hungry) in [
                    (-6.5f32, 11.5f32, false),
                    (-5.5, 12.5, false),
                    (7.5, 11.5, true),
                ] {
                    let mut m = demo_mob!(
                        chart,
                        si,
                        glam::Vec3::new(bx as f32 + dx, y as f32 + 1.0, bz as f32 + dz),
                        2.0,
                    );
                    m.health = reg2.animals[si].health;
                    m.tamed = !hungry;
                    if hungry {
                        m.belly = -1.0;
                    }
                    w.spawn_mob(m);
                }
                if let Some(dung) = reg2.item_id("base:dung") {
                    let stack = ItemStack::new(&reg2, dung, 1);
                    demo_drop!(w, chart, (bx - 7, y + 1, bz + 11), stack);
                }
                // The pond: dug two deep, sealed in stone, water to
                // the brim — cattails on the bank, a lily on the
                // glass, a trout below and the heron above it.
                if let (Some(stone), Some(reeds), Some(lily)) =
                    (b("base:stone"), b("base:cattail"), b("base:water_lily"))
                {
                    let water = reg2.water_block(0);
                    for dx in 3..=7i32 {
                        for dz in 10..=13i32 {
                            let (x, z) = (bx + dx, bz + dz);
                            let rim = dx == 3 || dx == 7 || dz == 10 || dz == 13;
                            for dy in [-2i32, -1] {
                                demo_set!(w, chart, x, y + dy, z, if rim { stone } else { water });
                            }
                            demo_set!(w, chart, x, y - 3, z, stone);
                            if demo_get!(w, chart, x, y, z) != AIR {
                                demo_set!(w, chart, x, y, z, AIR);
                            }
                        }
                    }
                    demo_set!(w, chart, bx + 3, y, bz + 10, reeds);
                    demo_set!(w, chart, bx + 7, y, bz + 13, reeds);
                    // The pad floats on the water surface: the first
                    // air cell above the fill.
                    demo_set!(w, chart, bx + 5, y, bz + 12, lily);
                    for (name, dx, dz, dy) in [
                        ("base:trout", 5.5f32, 11.5f32, -1.6f32),
                        ("base:heron", 5.5, 11.5, 1.0),
                    ] {
                        if let Some(si) = reg2.animal_id(name) {
                            let mut m = demo_mob!(
                                chart,
                                si,
                                glam::Vec3::new(bx as f32 + dx, y as f32 + dy, bz as f32 + dz),
                                1.2,
                            );
                            m.health = reg2.animals[si].health;
                            m.belly = 9000.0;
                            w.spawn_mob(m);
                        }
                    }
                }
                // The storm's aftermath, staged a week on: char scars
                // in the grass, flowers erupting around them, saplings
                // on the march — wrath and renewal as one event.
                if let (Some(charred), Some(mb), Some(ep), Some(sap)) = (
                    b("base:charred_soil"),
                    b("base:meadow_bloom"),
                    b("base:ember_poppy"),
                    b("base:oak_sapling"),
                ) {
                    let (ax, az) = (bx - 6, bz + 15);
                    for dx in 0..8i32 {
                        for dz in 0..6i32 {
                            let (x, z) = (ax + dx, az + dz);
                            let g = b("base:grass").unwrap();
                            demo_set!(w, chart, x, y, z, g);
                            for hh in 1..=4 {
                                if demo_get!(w, chart, x, y + hh, z) != AIR {
                                    demo_set!(w, chart, x, y + hh, z, AIR);
                                }
                            }
                            let roll = (dx * 7 + dz * 13) % 17;
                            match roll {
                                0 | 8 => {
                                    demo_set!(w, chart, x, y, z, charred);
                                }
                                2 | 9 | 14 => {
                                    demo_set!(w, chart, x, y + 1, z, mb);
                                }
                                4 | 11 => {
                                    demo_set!(w, chart, x, y + 1, z, ep);
                                }
                                6 => {
                                    demo_set!(w, chart, x, y + 1, z, sap);
                                }
                                _ => {}
                            }
                        }
                    }
                    demo_bloom!(w, chart, ax, az, 3.0);
                }
                // The grotto: a hollow cut under the pad's east edge,
                // lantern fungus glowing inside, a bat at roost and
                // its guano on the floor — the cave's corner of the
                // tour, no cave required.
                if let (Some(stone), Some(lf)) = (b("base:stone"), b("base:lantern_fungus")) {
                    for dx in 8..=12i32 {
                        for dz in -2..=2i32 {
                            for dy in -4..=-1i32 {
                                let (x, yy, z) = (bx + dx, y + dy, bz + dz);
                                let shell = dx == 12 || dz == -2 || dz == 2 || dy == -4;
                                // Open face toward the west (the pad).
                                if dx == 8 && dy >= -3 {
                                    if demo_get!(w, chart, x, yy, z) != AIR {
                                        demo_set!(w, chart, x, yy, z, AIR);
                                    }
                                    continue;
                                }
                                demo_set!(w, chart, x, yy, z, if shell { stone } else { AIR });
                            }
                        }
                    }
                    demo_set!(w, chart, bx + 11, y - 3, bz, lf);
                    demo_set!(w, chart, bx + 10, y - 3, bz + 1, lf);
                    if let Some(si) = reg2.animal_id("base:bat") {
                        let mut m = demo_mob!(
                            chart,
                            si,
                            glam::Vec3::new(bx as f32 + 10.5, y as f32 - 2.5, bz as f32 - 0.5),
                            0.0,
                        );
                        m.health = reg2.animals[si].health;
                        w.spawn_mob(m);
                    }
                    if let Some(g) = reg2.item_id("base:guano") {
                        let stack = ItemStack::new(&reg2, g, 2);
                        demo_drop!(w, chart, (bx + 10, y - 3, bz), stack);
                    }
                }
                // The neighbors: the wider roster lined up along the
                // north strip for the camera.
                for (i, name) in [
                    "base:bison",
                    "base:camel",
                    "base:musk_ox",
                    "base:antelope",
                    "base:mouflon",
                    "base:pheasant",
                    "base:duck",
                ]
                .iter()
                .enumerate()
                {
                    if let Some(si) = reg2.animal_id(name) {
                        let mut m = demo_mob!(
                            chart,
                            si,
                            glam::Vec3::new(
                                bx as f32 - 7.0 + i as f32 * 2.2,
                                y as f32 + 1.0,
                                bz as f32 - 1.0,
                            ),
                            std::f32::consts::PI,
                        );
                        m.health = reg2.animals[si].health;
                        m.tamed = true;
                        m.belly = 9000.0;
                        w.spawn_mob(m);
                    }
                }
                // Fang and carrion: a fox on stand by the field, and
                // the vultures' table set east of the pen.
                for (name, dx, dz) in [
                    ("base:fox", 8.5f32, 6.5f32),
                    ("base:carcass", -2.5, 13.5),
                    ("base:vulture", -2.5, 13.5),
                ] {
                    if let Some(si) = reg2.animal_id(name) {
                        let mut m = demo_mob!(
                            chart,
                            si,
                            glam::Vec3::new(bx as f32 + dx, y as f32 + 1.0, bz as f32 + dz),
                            3.6,
                        );
                        m.health = reg2.animals[si].health;
                        m.belly = 9000.0; // props don't eat the props
                        if name == "base:carcass" {
                            m.rot = 9000.0;
                        }
                        w.spawn_mob(m);
                    }
                }
            }
        }
    }
}
