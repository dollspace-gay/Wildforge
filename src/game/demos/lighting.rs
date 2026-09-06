//! Lighting capture scene construction.

use super::DemoChart;
use crate::game::Game;
use crate::lights;
use crate::planet::EntityPos;
use crate::registry::AIR;
use crate::world::World;
use glam::Vec3;

impl Game {
    pub(super) fn stage_capture_colored_shadows(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: a ring of torches near spawn (lighting verification).
        if std::env::var("WILDFORGE_DEMO_TORCH").is_ok()
            && let Some(torch) = self.content.reg.block_id("base:torch")
        {
            for (dx, dz) in [(3, 0), (-3, 2), (0, 4), (2, -4)] {
                let (x, z) = (spawn.x as i32 + dx, spawn.z as i32 + dz);
                let y = demo_height!(self.runtime.local().world, chart, x, z);
                demo_set!(self.runtime.local_mut().world, chart, x, y + 1, z, torch);
            }
        }
        // Dev: two pillars flanked by a blue and a red lamp — colored-shadow
        // test (each pillar should cast a blue shadow away from the blue lamp
        // and a red one away from the red lamp, purple where both reach).
        if std::env::var("WILDFORGE_DEMO_COLORSHADOW").is_ok() {
            let blue = self.content.reg.block_id("base:blue_lamp");
            let red = self.content.reg.block_id("base:red_lamp");
            let stone = self.content.reg.block_id("base:cobblestone");
            let bx = spawn.x as i32;
            let bz = spawn.z as i32 + 4;
            let y = demo_height!(self.runtime.local().world, chart, bx, bz);
            if let Some(stone) = stone {
                // A neutral grey floor reads colored light far better than grass.
                for dx in -8..=8 {
                    for dz in -6..=8 {
                        demo_set!(
                            self.runtime.local_mut().world,
                            chart,
                            bx + dx,
                            y,
                            bz + dz,
                            stone
                        );
                    }
                }
                // Two pillars as occluders.
                for px in [-2i32, 2] {
                    for h in 1..=3 {
                        demo_set!(
                            self.runtime.local_mut().world,
                            chart,
                            bx + px,
                            y + h,
                            bz,
                            stone
                        );
                    }
                }
            }
            // Low colored lamps to either side so shadows rake across the floor.
            if let Some(b) = blue {
                demo_set!(self.runtime.local_mut().world, chart, bx - 5, y + 2, bz, b);
            }
            if let Some(r) = red {
                demo_set!(self.runtime.local_mut().world, chart, bx + 5, y + 2, bz, r);
            }
        }
    }

    pub(super) fn stage_capture_room(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: an enclosed cobblestone room with a 1-wide door and a 2x2 east
        // window, for eyeballing interior lighting — the sky occlusion (walls go
        // dark away from the openings) and the cascaded-shadow sunbeam that
        // tracks across the floor through the window. `=torch` also plants one.
        if std::env::var("WILDFORGE_DEMO_ROOM").is_ok()
            && let Some(stone) = self.content.reg.block_id("base:cobblestone")
        {
            let (bx, bz) = (spawn.x as i32, spawn.z as i32);
            let fy = demo_height!(self.runtime.local().world, chart, bx, bz);
            for dx in -4..=4 {
                for dz in -4..=4 {
                    for dy in 0..=6 {
                        let shell =
                            dx == -4 || dx == 4 || dz == -4 || dz == 4 || dy == 0 || dy == 6;
                        let b = if shell { stone } else { AIR };
                        demo_set!(
                            self.runtime.local_mut().world,
                            chart,
                            bx + dx,
                            fy + dy,
                            bz + dz,
                            b
                        );
                    }
                }
            }
            // A 1-wide, 2-tall door in the +z wall.
            demo_set!(
                self.runtime.local_mut().world,
                chart,
                bx,
                fy + 1,
                bz + 4,
                AIR
            );
            demo_set!(
                self.runtime.local_mut().world,
                chart,
                bx,
                fy + 2,
                bz + 4,
                AIR
            );
            // A 2x2 window high in the +x (east) wall — the morning sun throws
            // a bright quad onto the floor that tracks across it.
            for wy in 3..=4 {
                for wz in -1..=0 {
                    demo_set!(
                        self.runtime.local_mut().world,
                        chart,
                        bx + 4,
                        fy + wy,
                        bz + wz,
                        AIR
                    );
                }
            }
            if std::env::var("WILDFORGE_DEMO_ROOM").as_deref() == Ok("torch")
                && let Some(torch) = self.content.reg.block_id("base:torch")
            {
                demo_set!(
                    self.runtime.local_mut().world,
                    chart,
                    bx + 2,
                    fy + 1,
                    bz,
                    torch
                );
            }
            // Stand the player inside (this world has a saved position).
            self.player.pos = self
                .player
                .pos
                .relocated_local(Vec3::new(
                    bx as f32 + 0.5,
                    (fy + 1) as f32 + 0.2,
                    bz as f32 - 2.5,
                ))
                .unwrap();
            self.camera.follow_planet(self.player.eye());
        }
    }

    pub(super) fn stage_capture_point_lights(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: two pillars on a grey floor lit by a blue and a red dynamic
        // point light (sharp per-light shadows). Pair with
        // WILDFORGE_AMBIENT=0.05,0.05,0.05 for stark contrast.
        if std::env::var("WILDFORGE_DEMO_PTLIGHT").is_ok() {
            let stone = self.content.reg.block_id("base:cobblestone");
            let bx = spawn.x as i32;
            let bz = spawn.z as i32 + 5;
            let y = demo_height!(self.runtime.local().world, chart, bx, bz);
            if let Some(stone) = stone {
                for dx in -9..=9 {
                    for dz in -7..=9 {
                        demo_set!(
                            self.runtime.local_mut().world,
                            chart,
                            bx + dx,
                            y,
                            bz + dz,
                            stone
                        );
                    }
                }
                for px in [-2i32, 2] {
                    for h in 1..=3 {
                        demo_set!(
                            self.runtime.local_mut().world,
                            chart,
                            bx + px,
                            y + h,
                            bz,
                            stone
                        );
                    }
                }
            }
            let fy = (y + 2) as f32 + 0.5;
            self.presentation.demo_lights = vec![
                lights::DynLight {
                    key: lights::Key::Demo(0),
                    pos: chart
                        .entity(Vec3::new(bx as f32 - 5.0 + 0.5, fy, bz as f32 + 0.5))
                        .render_pos(),
                    range: 16.0,
                    color: Vec3::new(0.35, 0.6, 2.0),
                },
                lights::DynLight {
                    key: lights::Key::Demo(1),
                    pos: chart
                        .entity(Vec3::new(bx as f32 + 5.0 + 0.5, fy, bz as f32 + 0.5))
                        .render_pos(),
                    range: 16.0,
                    color: Vec3::new(2.0, 0.35, 0.3),
                },
            ];
        }
    }

    pub(super) fn stage_capture_camp(&mut self, spawn: EntityPos, chart: DemoChart) {
        if std::env::var("WILDFORGE_DEMO_CAMP").is_ok() {
            let b = |n: &str| self.content.reg.block_id(n);
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
            if let (Some(log), Some(torch)) = (b("base:log"), b("base:torch")) {
                // A clearing: no trunks photobombing the campfire.
                for dx in -6..=6i32 {
                    for dz in -1..=12i32 {
                        let (x, z) = (bx + dx, bz + dz);
                        let y = demo_height!(self.runtime.local().world, chart, x, z);
                        for h in 1..=9 {
                            if demo_get!(self.runtime.local().world, chart, x, y + h, z) != AIR {
                                demo_set!(self.runtime.local_mut().world, chart, x, y + h, z, AIR);
                            }
                        }
                    }
                }
                // Torch posts: a 2-log stake with the flame on top.
                for (px, pz) in [(4i32, 4i32), (-4, 6), (0, 10)] {
                    let (x, z) = (bx + px, bz + pz);
                    let y = demo_height!(self.runtime.local().world, chart, x, z);
                    demo_set!(self.runtime.local_mut().world, chart, x, y + 1, z, log);
                    demo_set!(self.runtime.local_mut().world, chart, x, y + 2, z, log);
                    demo_set!(self.runtime.local_mut().world, chart, x, y + 3, z, torch);
                }
            }
            for (name, px, pz) in [("base:chest", 2i32, 7i32), ("base:stone_anvil", -2, 4)] {
                if let Some(blk) = b(name) {
                    let (x, z) = (bx + px, bz + pz);
                    let y = demo_height!(self.runtime.local().world, chart, x, z);
                    demo_set!(self.runtime.local_mut().world, chart, x, y + 1, z, blk);
                }
            }
            let reg = self.content.reg.clone();
            if let Some(t) = reg.item_id("base:torch") {
                self.give_dev_item(&reg, t, 5);
            }
        }
    }

    pub(super) fn stage_capture_torch_room(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: an enclosed torch-lit room — the full static pipeline
        // (mesher emitters -> promotion -> cached cube shadows), with two
        // pillars to throw hard shadows and a red-glazed alcove (stained
        // transmission). Real torch blocks, no demo lights. Built on the
        // footprint's highest ground so hills never poke through.
        if std::env::var("WILDFORGE_DEMO_TORCHROOM").is_ok()
            && let (Some(stone), Some(torch)) = (
                self.content.reg.block_id("base:cobblestone"),
                self.content.reg.block_id("base:torch"),
            )
        {
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            // The footprint may straddle chunks that don't exist yet —
            // writes into missing chunks vanish, leaving open walls.
            for dx in [-8i32, 0, 8] {
                for dz in [-8i32, 0, 8] {
                    self.runtime
                        .local_mut()
                        .world
                        .ensure_chunk(chart.chunk(bx + dx, bz + dz));
                }
            }
            let yf = (-7..=7)
                .flat_map(|dx| (-7..=7).map(move |dz| (dx, dz)))
                .map(|(dx, dz)| demo_height!(self.runtime.local().world, chart, bx + dx, bz + dz))
                .max()
                .unwrap_or(spawn.y as i32);
            for dx in -7..=7i32 {
                for dz in -7..=7i32 {
                    demo_set!(
                        self.runtime.local_mut().world,
                        chart,
                        bx + dx,
                        yf,
                        bz + dz,
                        stone
                    );
                    let wall = dx.abs() == 7 || dz.abs() == 7;
                    for h in 1..=8 {
                        let b = if (wall && h <= 3) || h == 4 {
                            stone
                        } else {
                            AIR
                        };
                        let b = if h > 4 { AIR } else { b };
                        demo_set!(
                            self.runtime.local_mut().world,
                            chart,
                            bx + dx,
                            yf + h,
                            bz + dz,
                            b
                        );
                    }
                }
            }
            for px in [-3i32, 3] {
                for h in 1..=3 {
                    demo_set!(
                        self.runtime.local_mut().world,
                        chart,
                        bx + px,
                        yf + h,
                        bz + 3,
                        stone
                    );
                }
            }
            for (tx, tz) in [(-6i32, -6i32), (6, -6), (0, 6)] {
                demo_set!(
                    self.runtime.local_mut().world,
                    chart,
                    bx + tx,
                    yf + 1,
                    bz + tz,
                    torch
                );
            }
            // A red-glazed alcove: torch sealed behind a stained pane —
            // its pool outside should come out the color of the glass.
            if let Some(rg) = self.content.reg.block_id("base:red_glass") {
                let (ax, az) = (bx + 4, bz - 4);
                demo_set!(self.runtime.local_mut().world, chart, ax, yf + 1, az, stone);
                demo_set!(self.runtime.local_mut().world, chart, ax, yf + 2, az, torch);
                demo_set!(self.runtime.local_mut().world, chart, ax, yf + 3, az, stone);
                demo_set!(
                    self.runtime.local_mut().world,
                    chart,
                    ax - 1,
                    yf + 2,
                    az,
                    stone
                );
                demo_set!(
                    self.runtime.local_mut().world,
                    chart,
                    ax + 1,
                    yf + 2,
                    az,
                    stone
                );
                demo_set!(
                    self.runtime.local_mut().world,
                    chart,
                    ax,
                    yf + 2,
                    az - 1,
                    stone
                );
                demo_set!(
                    self.runtime.local_mut().world,
                    chart,
                    ax,
                    yf + 2,
                    az + 1,
                    rg
                );
            }
            // Stand in the room, whatever the terrain wanted.
            let inside = Vec3::new(bx as f32 + 0.5, yf as f32 + 1.2, bz as f32 + 0.5);
            self.player.pos = self.player.pos.relocated_local(inside).unwrap();
            self.survival.spawn_point = self.player.pos;
            self.camera.follow_planet(self.player.eye());
            // A torch in slot 0: WILDFORGE_SEL=0 holds it (held-light
            // shots), WILDFORGE_SEL=8 keeps the hand empty.
            let reg = self.content.reg.clone();
            if let Some(t) = reg.item_id("base:torch") {
                self.give_dev_item(&reg, t, 5);
            }
        }
    }

    pub(super) fn stage_capture_colored_light(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: a warm torch and a red ruby block side by side (colored-light
        // verification — pools of warm and red that blend where they meet).
        if std::env::var("WILDFORGE_DEMO_COLORLIGHT").is_ok() {
            let place = |w: &mut World, name: &str, dx: i32, dz: i32| {
                if let Some(b) = w.reg.block_id(name) {
                    let (x, z) = (spawn.x as i32 + dx, spawn.z as i32 + dz);
                    let y = demo_height!(w, chart, x, z);
                    demo_set!(w, chart, x, y + 1, z, b);
                }
            };
            place(&mut self.runtime.local_mut().world, "base:torch", -2, 5);
            place(&mut self.runtime.local_mut().world, "gems:ruby_block", 2, 5);
        }
    }

    pub(super) fn stage_capture_pillars(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: a few tall pillars near spawn (shadow-casting verification).
        if std::env::var("WILDFORGE_DEMO_PILLARS").is_ok()
            && let Some(stone) = self.content.reg.block_id("base:cobblestone")
        {
            for (dx, dz, h) in [(4, 2, 6), (7, -3, 8), (-2, 6, 5), (10, 4, 7)] {
                let (x, z) = (spawn.x as i32 + dx, spawn.z as i32 + dz);
                let base = demo_height!(self.runtime.local().world, chart, x, z);
                for i in 1..=h {
                    demo_set!(self.runtime.local_mut().world, chart, x, base + i, z, stone);
                }
            }
        }
    }
}
