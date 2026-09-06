//! Scene stations in the graphical frame pipeline.

use crate::world::TerrainRead;
use crate::atlas;
use crate::entity;
use crate::mesher;
use crate::world;
use crate::game::Game;
use super::{Geometry};

impl Game {
    pub(in crate::game) fn emit_scene_stations(&mut self, geometry: &mut Geometry) {
        // The steelworks show their work: a rested bloom on the anvil,
        // smoke over lit bloomeries and smoldering clamps.
        if self.in_world {
            let ts = 1.0 / atlas::ATLAS_TILES as f32;
            let inset = ts / 32.0;
            let smoke_slot = *atlas::builtin_slots().get("snow_flake").unwrap_or(&0);
            let t = self.time_abs;
            let sprite = |slot: u16,
                          pos: crate::planet::EntityPos,
                          size: f32,
                          lum: f32,
                          verts: &mut Vec<mesher::Vertex>,
                          idx: &mut Vec<u32>| {
                let center = pos.render_pos();
                let frame = crate::planet::local_frame(pos.surface_point());
                let east = frame.east.as_vec3();
                let up = frame.up.as_vec3();
                let north = frame.north.as_vec3();
                let (tx, ty) = (
                    slot as u32 % atlas::ATLAS_TILES,
                    slot as u32 / atlas::ATLAS_TILES,
                );
                for (dx, dz) in [(1.0f32, 0.0f32), (0.0, 1.0)] {
                    for flip in [false, true] {
                        let base = verts.len() as u32;
                        let (u0, u1) = if flip {
                            ((tx + 1) as f32 * ts - inset, tx as f32 * ts + inset)
                        } else {
                            (tx as f32 * ts + inset, (tx + 1) as f32 * ts - inset)
                        };
                        let sgn = if flip { -1.0 } else { 1.0 };
                        for (o, dy, uu) in [
                            (-0.5 * size * sgn, 0.0, u0),
                            (0.5 * size * sgn, 0.0, u1),
                            (0.5 * size * sgn, size, u1),
                            (-0.5 * size * sgn, size, u0),
                        ] {
                            let vv = if dy == 0.0 {
                                (ty + 1) as f32 * ts - inset
                            } else {
                                ty as f32 * ts + inset
                            };
                            verts.push(mesher::Vertex {
                                pos: (center + east * (dx * o) + up * dy + north * (dz * o))
                                    .to_array(),
                                uv: [uu, vv],
                                normal: [0.0, 0.0, 0.0],
                                light: [lum; 3],
                                sky: lum,
                                ao: 1.0,
                            });
                        }
                        idx.extend_from_slice(&[
                            base,
                            base + 1,
                            base + 2,
                            base,
                            base + 2,
                            base + 3,
                        ]);
                    }
                }
            };
            let mut work: Vec<(u16, crate::planet::EntityPos, f32, f32)> = Vec::new();
            for (&pos, e) in self.runtime.view().block_entities() {
                match e {
                    world::BlockEntity::Anvil(a) => {
                        if let Some(b) = a.bloom {
                            let icon = self.content.reg.item(b.item).icon;
                            let at = crate::planet::EntityPos::new(
                                pos.face(),
                                f32::from(pos.u()) + 0.5,
                                f32::from(pos.y()) + 0.78,
                                f32::from(pos.v()) + 0.5,
                            )
                            .expect("block entity sprite remains in its cell");
                            work.push((icon, at, 0.32, 1.0));
                        }
                    }
                    world::BlockEntity::Multiblock(b)
                        if b.lit
                            && b.kind.handler(&self.content.reg)
                                != Some(crate::machines::MachineHandler::Separator) =>
                    {
                        for k in 0..3 {
                            let rise = (t * 0.7 + k as f32 * 0.65) % 2.0;
                            let drift = (t * 0.9 + k as f32 * 2.1).sin() * 0.2;
                            let at = crate::planet::EntityPos::new(
                                pos.face(),
                                f32::from(pos.u()) + 0.5 + drift,
                                f32::from(pos.y()) + 3.2 + rise,
                                f32::from(pos.v()) + 0.5,
                            )
                            .expect("machine smoke remains near its source");
                            work.push((smoke_slot, at, 0.5 + rise * 0.3, 0.12));
                        }
                    }
                    world::BlockEntity::Clamp(_) => {
                        for k in 0..2 {
                            let rise = (t * 0.5 + k as f32 * 0.9) % 1.8;
                            let drift = (t * 0.8 + k as f32 * 1.7).sin() * 0.15;
                            let at = crate::planet::EntityPos::new(
                                pos.face(),
                                f32::from(pos.u()) + 0.5 + drift,
                                f32::from(pos.y()) + 1.2 + rise,
                                f32::from(pos.v()) + 0.5,
                            )
                            .expect("clamp smoke remains near its source");
                            work.push((smoke_slot, at, 0.4 + rise * 0.25, 0.12));
                        }
                    }
                    _ => {}
                }
            }
            for (slot, pos, size, lum) in work {
                sprite(slot, pos, size, lum, &mut geometry.vertices, &mut geometry.indices);
            }
        }
    }
}
