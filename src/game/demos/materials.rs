//! Materials capture scene construction.

use super::DemoChart;
use crate::game::Game;
use crate::lights;
use crate::planet::EntityPos;
use crate::registry::AIR;
use glam::Vec3;

impl Game {
    pub(super) fn stage_capture_ice(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: a flat ice rink plus a low kerb for parallax verification.
        if std::env::var("WILDFORGE_DEMO_ICE").is_ok()
            && let Some(ice) = self.content.reg.block_id("base:ice")
        {
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            for dx in [-8i32, 0, 8] {
                for dz in [-8i32, 0, 8] {
                    self.runtime
                        .local_mut()
                        .world
                        .ensure_chunk(chart.chunk(bx + dx, bz + dz));
                }
            }
            let yf = (-10..=10)
                .flat_map(|dx| (-10..=10).map(move |dz| (dx, dz)))
                .map(|(dx, dz)| demo_height!(self.runtime.local().world, chart, bx + dx, bz + dz))
                .max()
                .unwrap_or(spawn.y as i32);
            for dx in -10..=10i32 {
                for dz in -10..=10i32 {
                    demo_set!(
                        self.runtime.local_mut().world,
                        chart,
                        bx + dx,
                        yf,
                        bz + dz,
                        ice
                    );
                }
                demo_set!(
                    self.runtime.local_mut().world,
                    chart,
                    bx + dx,
                    yf + 1,
                    bz + 10,
                    ice
                );
                demo_set!(
                    self.runtime.local_mut().world,
                    chart,
                    bx + dx,
                    yf + 2,
                    bz + 10,
                    ice
                );
            }
            let strafe: f32 = std::env::var("WILDFORGE_DEMO_STRAFE")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0);
            let stand = Vec3::new(bx as f32 + 0.5 + strafe, yf as f32 + 1.0, bz as f32 - 9.0);
            self.player.pos = self.player.pos.relocated_local(stand).unwrap();
            self.survival.spawn_point = self.player.pos;
            self.camera.follow_planet(self.player.eye());
            self.camera.yaw = std::f32::consts::FRAC_PI_2;
            self.camera.pitch = -0.35;
        }
    }

    pub(super) fn stage_capture_glowglass(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: a dark chamber with a glowglass window and a torch behind
        // it — emissive glass plus the transmission tint in one frame.
        if std::env::var("WILDFORGE_DEMO_GLOWGLASS").is_ok()
            && let Some(glow) = self.content.reg.block_id("base:glow_glass")
        {
            let stone = self.content.reg.block_id("base:stone").unwrap_or(AIR);
            let torch = self.content.reg.block_id("base:torch").unwrap_or(AIR);
            let (bx, bz) = (spawn.x as i32, spawn.z as i32 + 8);
            for dx in [-8i32, 0, 8] {
                for dz in [-8i32, 0, 8] {
                    self.runtime
                        .local_mut()
                        .world
                        .ensure_chunk(chart.chunk(bx + dx, bz + dz));
                }
            }
            let yf = (-5..=5)
                .flat_map(|dx| (-4..=4).map(move |dz| (dx, dz)))
                .map(|(dx, dz)| demo_height!(self.runtime.local().world, chart, bx + dx, bz + dz))
                .max()
                .unwrap_or(spawn.y as i32);
            for dx in -4..=4i32 {
                for dz in -3..=3i32 {
                    for dy in 0..=4i32 {
                        let edge = dx.abs() == 4 || dz.abs() == 3 || dy == 0 || dy == 4;
                        let b = if edge { stone } else { AIR };
                        demo_set!(
                            self.runtime.local_mut().world,
                            chart,
                            bx + dx,
                            yf + 1 + dy,
                            bz + dz,
                            b
                        );
                    }
                }
            }
            // The window in the far wall, glowing green.
            for dx in -2..=2i32 {
                for dy in 2..=3i32 {
                    demo_set!(
                        self.runtime.local_mut().world,
                        chart,
                        bx + dx,
                        yf + 1 + dy,
                        bz + 3,
                        glow
                    );
                }
            }
            // A torch on the outside sill: its beam crosses the pane.
            demo_set!(
                self.runtime.local_mut().world,
                chart,
                bx,
                yf + 2,
                bz + 5,
                torch
            );
            let stand = Vec3::new(bx as f32 + 0.5, yf as f32 + 1.2, bz as f32 - 1.5);
            self.player.pos = self.player.pos.relocated_local(stand).unwrap();
            self.survival.spawn_point = self.player.pos;
            self.camera.follow_planet(self.player.eye());
            self.camera.yaw = std::f32::consts::FRAC_PI_2;
            self.camera.pitch = 0.05;
        }
    }

    pub(super) fn stage_capture_rock(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: a stone wall lit from the side for authored-normal verification.
        if std::env::var("WILDFORGE_DEMO_ROCK").is_ok()
            && let Some(stone) = self.content.reg.block_id("base:stone")
        {
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            for dx in [-8i32, 0, 8] {
                for dz in [-8i32, 0, 8] {
                    self.runtime
                        .local_mut()
                        .world
                        .ensure_chunk(chart.chunk(bx + dx, bz + dz));
                }
            }
            let yf = (-10..=10)
                .flat_map(|dx| (-10..=10).map(move |dz| (dx, dz)))
                .map(|(dx, dz)| demo_height!(self.runtime.local().world, chart, bx + dx, bz + dz))
                .max()
                .unwrap_or(spawn.y as i32);
            for dx in -10..=10i32 {
                for dz in -10..=10i32 {
                    demo_set!(
                        self.runtime.local_mut().world,
                        chart,
                        bx + dx,
                        yf,
                        bz + dz,
                        stone
                    );
                }
                for dy in 1..=6i32 {
                    demo_set!(
                        self.runtime.local_mut().world,
                        chart,
                        bx + dx,
                        yf + dy,
                        bz - 8,
                        stone
                    );
                }
            }
            for dy in 1..=3i32 {
                demo_set!(
                    self.runtime.local_mut().world,
                    chart,
                    bx + 3,
                    yf + dy,
                    bz - 4,
                    stone
                );
            }
            let dist: f32 = std::env::var("WILDFORGE_DEMO_DIST")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(12.0);
            let stand = Vec3::new(bx as f32 + 0.5, yf as f32 + 1.0, bz as f32 - 7.0 + dist);
            self.player.pos = self.player.pos.relocated_local(stand).unwrap();
            self.survival.spawn_point = self.player.pos;
            self.camera.follow_planet(self.player.eye());
            self.camera.yaw = -std::f32::consts::FRAC_PI_2;
            self.camera.pitch = -0.10;
        }
    }

    pub(super) fn stage_capture_corner(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: a sub-voxel surface-sand dune for octant substrate checks.
        if std::env::var("WILDFORGE_DEMO_CORNER").is_ok()
            && let Some(stone) = self.content.reg.block_id("base:cobblestone")
        {
            let bx = spawn.x as i32;
            let bz = spawn.z as i32 + 6;
            let y = demo_height!(self.runtime.local().world, chart, bx, bz);
            // Carve a clean flat arena: cobblestone floor, air above, so
            // grass and trees don't intrude on the shadow.
            for dx in -11..=11 {
                for dz in -9..=15 {
                    demo_set!(
                        self.runtime.local_mut().world,
                        chart,
                        bx + dx,
                        y,
                        bz + dz,
                        stone
                    );
                    for h in 1..=9 {
                        demo_set!(
                            self.runtime.local_mut().world,
                            chart,
                            bx + dx,
                            y + h,
                            bz + dz,
                            AIR
                        );
                    }
                }
            }
            // Wall across X at z=bz, 5 tall, with a 1-wide doorway at bx.
            for dx in -11..=11 {
                if dx == 0 {
                    continue;
                }
                for h in 1..=5 {
                    demo_set!(
                        self.runtime.local_mut().world,
                        chart,
                        bx + dx,
                        y + h,
                        bz,
                        stone
                    );
                }
            }
            // Warm light on the far side of the wall — it blares through the
            // doorway and lights the far room, leaving the near side dark.
            self.presentation.demo_lights = vec![lights::DynLight {
                key: lights::Key::Demo(0),
                pos: chart
                    .entity(Vec3::new(
                        bx as f32 + 0.5,
                        (y + 2) as f32 + 0.5,
                        bz as f32 + 5.5,
                    ))
                    .render_pos(),
                range: 24.0,
                color: Vec3::new(2.4, 1.7, 0.8),
            }];
        }
    }

    pub(super) fn stage_capture_pool(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: a flat water pool ahead of spawn (specular-glint verification).
        if std::env::var("WILDFORGE_DEMO_POOL").is_ok()
            && let Some(water) = self.content.reg.block_id("base:water")
        {
            let cx = spawn.x as i32;
            let cz = spawn.z as i32 + 10;
            let y = demo_height!(self.runtime.local().world, chart, cx, cz);
            for dx in -8..=8 {
                for dz in -8..=8 {
                    demo_set!(
                        self.runtime.local_mut().world,
                        chart,
                        cx + dx,
                        y,
                        cz + dz,
                        water
                    );
                }
            }
        }
    }
}
