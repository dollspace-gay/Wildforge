//! Scene precipitation in the graphical frame pipeline.

use crate::world::TerrainRead;
use crate::atlas;
use crate::mesher;
use crate::world;
use glam::Vec3;
use crate::game::Game;
use super::{Geometry};

impl Game {
    pub(in crate::game) fn emit_scene_precipitation(&mut self, geometry: &mut Geometry, local_weather: Option<crate::planet_atlas::LocalWeatherSample>) {
        // Precipitation: a cylinder of falling quads around the camera.
        // Each streak owns a column; roofed columns stay dry.
        if local_weather.is_some_and(|weather| weather.kind.precipitating()) {
            let t = self.time_abs;
            let ts = 1.0 / atlas::ATLAS_TILES as f32;
            let inset = ts / 32.0;
            let rain_slot = *atlas::builtin_slots().get("rain_streak").unwrap_or(&0);
            let snow_slot = *atlas::builtin_slots().get("snow_flake").unwrap_or(&0);
            for i in 0..150u32 {
                let h = i.wrapping_mul(2654435761);
                let a = (h >> 8 & 0xffff) as f32 / 65536.0 * std::f32::consts::TAU;
                let r = 2.0 + (h >> 16 & 0xff) as f32 / 255.0 * 13.0;
                let phase = (h & 0xff) as f32 / 255.0;
                let Ok(sample) =
                    self.player
                        .pos
                        .translated(Vec3::new(a.cos() * r, 0.0, a.sin() * r))
                else {
                    continue;
                };
                let sample = sample.pos;
                let Some(column) = sample.block() else {
                    continue;
                };
                if !self.runtime.view().rains_at_surface(column.surface()) {
                    continue;
                }
                let snow = self.runtime.view().snows_at_surface(column.surface());
                let speed = if snow { 3.0 } else { 13.0 };
                let span = 14.0;
                let y = self.player.pos.y() + 7.0 - (t * speed + phase * span) % span;
                let Ok(streak) =
                    crate::planet::EntityPos::new(sample.face(), sample.u(), y, sample.v())
                else {
                    continue;
                };
                let Some(streak_block) = streak.block() else {
                    continue;
                };
                if self.runtime.view().light_at_pos(streak_block).1 < 15 {
                    continue; // a roof owns this column
                }
                let frame = crate::planet::local_frame(streak.surface_point());
                let east = frame.east.as_vec3();
                let up = frame.up.as_vec3();
                let north = frame.north.as_vec3();
                let center = streak.render_pos();
                let slot = if snow { snow_slot } else { rain_slot };
                let (tx, ty) = (
                    slot as u32 % atlas::ATLAS_TILES,
                    slot as u32 / atlas::ATLAS_TILES,
                );
                let (w2, h2) = if snow { (0.09, 0.09) } else { (0.035, 0.55) };
                let drift = if snow {
                    (t * 1.3 + phase * 9.0).sin() * 0.25
                } else {
                    0.0
                };
                for horizontal in [east, north] {
                    for flip in [false, true] {
                        let base = geometry.vertices.len() as u32;
                        let (u0, u1) = if flip {
                            ((tx + 1) as f32 * ts - inset, tx as f32 * ts + inset)
                        } else {
                            (tx as f32 * ts + inset, (tx + 1) as f32 * ts - inset)
                        };
                        let sgn = if flip { -1.0 } else { 1.0 };
                        for (o, dy, uu) in [
                            (-0.5 * w2 * sgn, 0.0, u0),
                            (0.5 * w2 * sgn, 0.0, u1),
                            (0.5 * w2 * sgn, h2, u1),
                            (-0.5 * w2 * sgn, h2, u0),
                        ] {
                            let vv = if dy == 0.0 {
                                (ty + 1) as f32 * ts - inset
                            } else {
                                ty as f32 * ts + inset
                            };
                            let world = center + horizontal * (drift + o) + up * dy;
                            geometry.vertices.push(mesher::Vertex {
                                pos: world.to_array(),
                                uv: [uu, vv],
                                normal: [0.0, 0.0, 0.0],
                                light: [0.5; 3],
                                sky: 0.9,
                                ao: 1.0,
                            });
                        }
                        geometry.indices.extend_from_slice(&[
                            base,
                            base + 1,
                            base + 2,
                            base,
                            base + 2,
                            base + 3,
                        ]);
                    }
                }
            }
        }
    }
}
