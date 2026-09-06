//! Bounce capture scene construction.

use super::DemoChart;
use crate::game::Game;
use crate::planet::EntityPos;
use crate::registry::AIR;
use crate::world::World;
use glam::Vec3;

impl Game {
    pub(super) fn stage_capture_bounce_room(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: the bounce-light room. A sealed white-plaster box with one high
        // window in the east wall and two saturated bands laid across the
        // floor. The morning sun throws a single bright quad through the
        // window, and over the hours that quad crawls from the red band to the
        // blue one. Nothing else in the room is coloured and no other light
        // gets in, so whatever tint the white walls take has exactly one
        // possible cause. Sweep it with WILDFORGE_TIME (~0.05 lands on red,
        // ~0.15 on blue); WILDFORGE_DEMO_GI=plain drops the bands for the
        // control shot that says what the room looks like with nothing to
        // bounce off.
        if let Ok(mode) = std::env::var("WILDFORGE_DEMO_GI")
            && let Some(white) = self.content.reg.block_id("base:white_plaster")
        {
            let (bx, bz) = (spawn.x as i32, spawn.z as i32);
            // The box straddles chunk borders, and a set into a chunk that was
            // never loaded is silently dropped — which shows up as a wall that
            // isn't there and daylight pouring through the gap.
            let center = chart.chunk(bx, bz);
            for cx in -2..=2 {
                for cz in -2..=2 {
                    self.runtime
                        .local_mut()
                        .world
                        .ensure_chunk(center.offset(cx, cz));
                }
            }
            let fy = demo_height!(self.runtime.local().world, chart, bx, bz);
            // Level everything above the floor for a good margin around the
            // box first. Spawn lands wherever the worldgen puts it, and a rise
            // on the east side buries the window — which reads as "the sun
            // stopped working" rather than "there is a hill there". Clearing
            // makes the scene say the same thing whatever world it runs in.
            //
            // Only where it changes something. set_block relights and cascades
            // whatever it is handed, including air laid over air, and most of
            // this volume is already open sky — asking anyway cost the demo
            // most of its half-minute startup.
            let place = |w: &mut World, x: i32, y: i32, z: i32, b: crate::registry::BlockId| {
                let at = chart.block(x, y, z);
                if w.get_block_at(at) != b {
                    w.set_block_authored_at(at, b, "development capture scene");
                }
            };
            for dx in -12..=16 {
                for dz in -12..=12 {
                    for dy in 1..=22 {
                        place(
                            &mut self.runtime.local_mut().world,
                            bx + dx,
                            fy + dy,
                            bz + dz,
                            AIR,
                        );
                    }
                }
            }
            // Long in -z, because the sun's azimuth carries the quad that way
            // as much as it carries it east: a square room loses the spot into
            // a corner before it has crossed both bands.
            for dx in -6..=6 {
                for dz in -6..=6 {
                    for dy in 0..=7 {
                        let shell =
                            dx == -6 || dx == 6 || dz == -6 || dz == 6 || dy == 0 || dy == 7;
                        let b = if shell { white } else { AIR };
                        place(
                            &mut self.runtime.local_mut().world,
                            bx + dx,
                            fy + dy,
                            bz + dz,
                            b,
                        );
                    }
                }
            }
            // A wide window high in the +x (east) wall, set well towards +z.
            // Wide on purpose: a narrow slit admits so little light that what it
            // throws on the floor cannot colour a room, and it makes the beam
            // hard for anything to find. This is a scene about bounced light, so
            // it lets enough in to have some.
            // The sun's azimuth has a fixed +z component, so the quad slides
            // steadily -z as the morning goes on; entering at the +z end buys
            // the whole traverse before it reaches the far wall. Small on
            // purpose — the quad has to stay a quad, or there is no moving
            // spot to follow.
            for wy in 3..=5 {
                for wz in 1..=4 {
                    place(
                        &mut self.runtime.local_mut().world,
                        bx + 6,
                        fy + wy,
                        bz + wz,
                        AIR,
                    );
                }
            }
            if mode != "plain"
                && let (Some(red), Some(blue)) = (
                    self.content.reg.block_id("base:red_plaster"),
                    self.content.reg.block_id("base:blue_plaster"),
                )
            {
                // Bands run the full width in z, so the quad's sideways drift
                // over the morning doesn't walk it off the colour. Their x is
                // where the quad actually lands early and late — see the
                // capture sweep in docs/, not a derivation.
                for dz in -5..=5 {
                    for dx in -4..=-2 {
                        place(
                            &mut self.runtime.local_mut().world,
                            bx + dx,
                            fy,
                            bz + dz,
                            red,
                        );
                    }
                    for dx in 1..=3 {
                        place(
                            &mut self.runtime.local_mut().world,
                            bx + dx,
                            fy,
                            bz + dz,
                            blue,
                        );
                    }
                }
            }
            // Stand at the far +z end looking back down the room, so the frame
            // is mostly white wall and ceiling — the surfaces that report the
            // bounce — with both bands in the lower field of view. An explicit
            // WILDFORGE_POS wins, so a reported coordinate can be reproduced
            // inside the scene rather than only outside it.
            if std::env::var("WILDFORGE_POS").is_err() {
                self.player.pos = self
                    .player
                    .pos
                    .relocated_local(Vec3::new(
                        bx as f32 + 0.5,
                        (fy + 1) as f32 + 0.2,
                        bz as f32 + 5.0,
                    ))
                    .unwrap();
            }
            self.camera.follow_planet(self.player.eye());
            self.camera.yaw = -std::f32::consts::FRAC_PI_2;
            self.camera.pitch = -0.18;
            eprintln!(
                "demo-gi room at ({bx},{fy},{bz}), player {:?}",
                self.player.pos
            );
        }
    }
}
