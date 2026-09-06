//! Flow capture scene construction.

use super::DemoChart;
use crate::game::Game;
use crate::planet::EntityPos;
use crate::registry::AIR;
use glam::Vec3;

impl Game {
    pub(super) fn stage_capture_water(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: drop a water source on a pillar ahead of spawn to watch it flow.
        if std::env::var("WILDFORGE_DEMO_WATER").is_ok() {
            let (bx, bz) = (spawn.x as i32 - 6, spawn.z as i32 - 14);
            for cx in -1..=1 {
                for cz in -1..=1 {
                    self.runtime
                        .local_mut()
                        .world
                        .ensure_chunk(chart.chunk(bx, bz).offset(cx, cz));
                }
            }
            let by = demo_height!(self.runtime.local().world, chart, bx, bz);
            let stone = self.content.reg.block_id("base:stone").unwrap_or(AIR);
            let water = self.content.reg.block_id("base:water").unwrap_or(AIR);
            for y in by + 1..=by + 4 {
                demo_set!(self.runtime.local_mut().world, chart, bx, y, bz, stone);
            }
            demo_set!(self.runtime.local_mut().world, chart, bx, by + 5, bz, water);
            eprintln!(
                "demo water source at ({bx},{},{bz}), spawn {:?}",
                by + 5,
                spawn
            );
        }
    }

    pub(super) fn stage_capture_fire(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: a stand of trees over grass, lit at one corner, so a
        // burn can be watched running rather than inferred from a
        // test's counters. WILDFORGE_DEMO_FIRE=mine lights it as a
        // player's; anything else is the wild's.
        if let Ok(who) = std::env::var("WILDFORGE_DEMO_FIRE") {
            let b = |n: &str| self.content.reg.block_id(n);
            let (bx, bz) = (spawn.x as i32 + 10, spawn.z as i32);
            let g = demo_height!(self.runtime.local().world, chart, bx, bz);
            if let (Some(grass), Some(log), Some(leaves)) =
                (b("base:grass"), b("base:log"), b("base:leaves"))
            {
                let w = &mut self.runtime.local_mut().world;
                // The footprint spans several chunks, and any that are
                // not loaded yet will be GENERATED over the top of
                // whatever we build here.
                for cx in -1..=1 {
                    for cz in -1..=1 {
                        w.ensure_chunk(chart.chunk(bx + cx * 16, bz + cz * 16));
                    }
                }
                for x in -8..=8 {
                    for z in -8..=8 {
                        for y in (g - 2)..g {
                            demo_set!(w, chart, bx + x, y, bz + z, grass);
                        }
                        for y in (g + 1)..(g + 9) {
                            demo_set!(w, chart, bx + x, y, bz + z, AIR);
                        }
                        demo_set!(w, chart, bx + x, g, bz + z, grass);
                    }
                }
                // A copse: trunks on a lattice under one canopy.
                for x in (-6..=6).step_by(3) {
                    for z in (-6..=6).step_by(3) {
                        for y in 1..=4 {
                            demo_set!(w, chart, bx + x, g + y, bz + z, log);
                        }
                    }
                }
                for x in -7..=7 {
                    for z in -7..=7 {
                        for y in 4..=6 {
                            demo_set!(w, chart, bx + x, g + y, bz + z, leaves);
                        }
                    }
                }
                // Wilderness unless we say otherwise, so the wild's
                // own fire is willing to touch it.
                w.player_touched.clear();
                let mine = who == "mine";
                demo_fire!(w, chart, bx - 7, g + 1, bz - 7, mine);
                eprintln!("fire demo at ({bx},{g},{bz}), mine={mine}");
            }
            self.player.pos = self
                .player
                .pos
                .relocated_local(Vec3::new(
                    bx as f32 - 2.0,
                    g as f32 + 14.0,
                    bz as f32 + 26.0,
                ))
                .unwrap();
            self.player.vel = Vec3::ZERO;
            self.camera.yaw = -std::f32::consts::FRAC_PI_2;
            self.camera.pitch = -0.42;
            self.flying = true;
        }
    }

    pub(super) fn stage_capture_lava(&mut self, spawn: EntityPos, chart: DemoChart) -> bool {
        // Dev: a volcano flank — a staircase with a vent at the crest,
        // so a flow can be watched settling instead of guessed at.
        if std::env::var("WILDFORGE_DEMO_LAVA").is_ok() {
            let bx = spawn.x as i32 + 6;
            let bz = spawn.z as i32;
            let y0 = demo_height!(self.runtime.local().world, chart, bx, bz) + 14;
            let b = |n: &str| self.content.reg.block_id(n);
            let Some(stone) = b("base:basalt").or_else(|| b("base:stone")) else {
                return true;
            };
            let w = &mut self.runtime.local_mut().world;
            for step in 0..14i32 {
                let top = y0 - step;
                for x in (step * 2)..(step * 2 + 2) {
                    for z in -4..=4 {
                        for fill in 0..6 {
                            demo_set!(w, chart, bx + x, top - fill, bz + z, stone);
                        }
                    }
                }
            }
            let lava = self.content.reg.lava_for_volume(8);
            for z in -2..=2 {
                for x in 0..2 {
                    demo_set!(w, chart, bx + x, y0 + 1, bz + z, lava);
                }
            }
            // Stand the viewer off the flank looking along it, so the
            // shot frames the flow rather than the inside of the hill.
            self.player.pos = self
                .player
                .pos
                .relocated_local(Vec3::new(
                    bx as f32 + 13.0,
                    y0 as f32 + 3.0,
                    bz as f32 + 22.0,
                ))
                .unwrap();
            self.player.vel = Vec3::ZERO;
            self.camera.yaw = -std::f32::consts::FRAC_PI_2;
            self.camera.pitch = -0.42;
            self.flying = true;
            eprintln!("lava demo: crest at ({bx},{y0},{bz})");
        }
        false
    }
}
