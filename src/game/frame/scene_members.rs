//! Scene members in the graphical frame pipeline.

use crate::world::TerrainRead;
use crate::atlas;
use crate::mesher;
use crate::mobs;
use crate::registry::ItemId;
use crate::style;
use crate::world;
use glam::Vec3;
use crate::game::Game;
use super::{Geometry};

impl Game {
    pub(in crate::game) fn prepare_scene_members(&mut self, dt: f32) -> Geometry {
        // World-space extras: item entities + mining crack overlay.
        let mut entity_verts = Vec::new();
        let mut entity_idx = Vec::new();
        let sample = |w: &dyn TerrainRead, p: crate::planet::EntityPos| -> ([f32; 3], f32) {
            let sample = p
                .translated(Vec3::new(0.0, 0.4, 0.0))
                .ok()
                .and_then(|moved| moved.pos.block());
            let (b, s) = sample.map_or(([0; 3], 15), |pos| w.light_rgb_at_pos(pos));
            (
                [b[0] as f32 / 15.0, b[1] as f32 / 15.0, b[2] as f32 / 15.0],
                s as f32 / 15.0,
            )
        };
        for it in self.runtime.view().loose_items() {
            let lum = sample(&self.runtime.view(), it.pos);
            it.emit(&self.content.reg, lum, &mut entity_verts, &mut entity_idx);
        }
        for m in self.runtime.view().mobs() {
            let lum = sample(&self.runtime.view(), m.pos);
            m.emit(&self.content.reg, lum, &mut entity_verts, &mut entity_idx);
        }
        for p in self.runtime.view().projectiles() {
            p.emit(&mut entity_verts, &mut entity_idx);
        }
        // Cosmetic debris and dust ride the same batch.
        self.presentation
            .pool
            .emit(&mut entity_verts, &mut entity_idx);
        // Fellow players, dressed and striding.
        // Dev: stand-ins a few blocks ahead — three styles side by side,
        // mid-stride, one holding a torch (model and seam iteration).
        let planet_player_shot = std::env::var("WILDFORGE_PLANET_SHOT").as_deref() == Ok("players");
        if (std::env::var("WILDFORGE_DEMO_PLAYER").is_ok() || planet_player_shot) && self.in_world {
            let torch_art = self.held_art(self.content.reg.item_id("base:torch"));
            for (i, st) in [
                style::Style {
                    hair_style: 3,
                    legwear: 1,
                    shirt: 3,
                    ..Default::default()
                },
                style::Style {
                    skin: 4,
                    hair: 3,
                    shirt: 6,
                    trousers: 1,
                    beard: 3,
                    build: 2,
                    ..Default::default()
                },
                style::Style {
                    skin: 2,
                    hair: 7,
                    shirt: 1,
                    trousers: 5,
                    hair_style: 2,
                    build: 1,
                    ..Default::default()
                },
            ]
            .into_iter()
            .enumerate()
            {
                let translated = self
                    .player
                    .pos
                    .translated(Vec3::new([-1.2, 0.0, 1.2][i], 0.0, 3.5))
                    .expect("demo player translation stays canonical")
                    .pos;
                let surface = crate::planet::SurfacePos::new(
                    translated.face(),
                    translated.u().floor() as u16,
                    translated.v().floor() as u16,
                )
                .expect("canonical demo position has a surface cell");
                let py = self.runtime.view().surface_height_at(surface) as f32 + 1.0;
                let at = crate::planet::EntityPos::new(
                    surface.face(),
                    translated.u(),
                    py,
                    translated.v(),
                )
                .expect("demo player surface is canonical");
                let lum = sample(&self.runtime.view(), at);
                let held = if i == 1 {
                    torch_art
                } else {
                    mobs::HeldArt::None
                };
                mobs::emit_humanoid(
                    at,
                    std::f32::consts::PI,
                    &Self::humanoid_art(st),
                    (self.time_abs * 3.0, 0.8),
                    held,
                    lum,
                    &mut entity_verts,
                    &mut entity_idx,
                );
            }
        }
        if self.multiplayer.remote.is_some() {
            let entries: Vec<(u32, Vec3, crate::planet::EntityPos, f32)> = self
                .multiplayer
                .remote
                .as_ref()
                .map(|r| {
                    r.players
                        .iter()
                        .filter_map(|(id, (_, p, y))| {
                            r.player_positions
                                .get(id)
                                .copied()
                                .map(|logical| (*id, *p, logical, *y))
                        })
                        .collect()
                })
                .unwrap_or_default();
            for (id, pos, logical, yaw) in entries {
                let gait = self.presentation.gait_for(id, pos, dt);
                let (held, implement, st) = {
                    let r = self.multiplayer.remote.as_ref().unwrap();
                    let held = r
                        .player_held
                        .get(&id)
                        .and_then(|w| r.session.content().item(*w));
                    let implement = r.player_implement.get(&id).copied().map(|visual| {
                        self.held_art_implement(visual, |wire| r.session.content().item(wire))
                    });
                    let st = r
                        .player_style
                        .get(&id)
                        .map(|v| style::Style::unpack(*v))
                        .unwrap_or_default();
                    (held, implement, st)
                };
                let lum = sample(&self.runtime.view(), logical);
                mobs::emit_humanoid_interpolated(
                    logical,
                    pos,
                    yaw,
                    &Self::humanoid_art(st),
                    gait,
                    implement.unwrap_or_else(|| self.held_art(held)),
                    lum,
                    &mut entity_verts,
                    &mut entity_idx,
                );
            }
        }
        if self.multiplayer.host.is_some() {
            type GuestRenderEntry = (
                u32,
                Vec3,
                crate::planet::EntityPos,
                f32,
                u16,
                u32,
                Option<crate::implements::ImplementVisual>,
            );
            let entries: Vec<GuestRenderEntry> = self
                .multiplayer
                .host
                .as_ref()
                .map(|h| {
                    h.guests
                        .iter()
                        .filter(|(_, guest)| guest.is_active())
                        .map(|(id, g)| {
                            let (p, y) = g.render_pos();
                            let implement = g.inventory.slots[g.hotbar]
                                .and_then(|stack| self.runtime.view().implement_visual(stack));
                            (*id, p, g.render_entity_pos(), y, g.held, g.style, implement)
                        })
                        .collect()
                })
                .unwrap_or_default();
            for (id, pos, logical, yaw, held_wire, pstyle, implement) in entries {
                let gait = self.presentation.gait_for(id, pos, dt);
                let held = if held_wire == u16::MAX {
                    None
                } else {
                    Some(ItemId(held_wire))
                };
                let lum = sample(&self.runtime.view(), logical);
                mobs::emit_humanoid_interpolated(
                    logical,
                    pos,
                    yaw,
                    &Self::humanoid_art(style::Style::unpack(pstyle)),
                    gait,
                    implement
                        .map(|visual| self.held_art_implement(visual, |wire| Some(ItemId(wire))))
                        .unwrap_or_else(|| self.held_art(held)),
                    lum,
                    &mut entity_verts,
                    &mut entity_idx,
                );
            }
        }
        // In chase / orbit views the local player finally has a body. The
        // first-person hand model is suppressed (see emit_hand), so this is
        // the sole on-screen avatar of your own character.
        if self.in_world && self.camera.mode != crate::camera::CameraMode::First {
            let logical = self.player.pos;
            let render = logical.render_pos();
            let held = self.inventory.slots[self.input.hotbar_sel];
            let implement = held.and_then(|stack| self.runtime.view().implement_visual(stack));
            let lum = sample(&self.runtime.view(), self.player.eye());
            let gait = self.presentation.gait_for(u32::MAX, render, dt);
            mobs::emit_humanoid_interpolated(
                logical,
                render,
                self.camera.yaw,
                &Self::humanoid_art(self.style),
                gait,
                implement
                    .map(|visual| self.held_art_implement(visual, |wire| Some(ItemId(wire))))
                    .unwrap_or_else(|| self.held_art_stack(held)),
                lum,
                &mut entity_verts,
                &mut entity_idx,
            );
        }
        // Airborne sand tumbles as full-size cubes.
        for f in self.runtime.view().falling_blocks().to_vec() {
            let lum = sample(&self.runtime.view(), f.pos);
            let origin = f.pos.render_pos();
            let local_frame = crate::planet::local_frame(f.pos.surface_point());
            let east = local_frame.east.as_vec3();
            let up = local_frame.up.as_vec3();
            let north = local_frame.north.as_vec3();
            let d = self.content.reg.block(f.block);
            let ts = 1.0 / atlas::ATLAS_TILES as f32;
            let inset = ts / 32.0;
            for (face, corners) in mesher::CORNERS.iter().enumerate() {
                let slot = d.tiles[face];
                let (tx, ty) = (
                    slot as u32 % atlas::ATLAS_TILES,
                    slot as u32 / atlas::ATLAS_TILES,
                );
                let base = entity_verts.len() as u32;
                for c in corners.iter() {
                    let (uu, vv) = match face {
                        0 | 1 => (c[2], 1.0 - c[1]),
                        4 | 5 => (c[0], 1.0 - c[1]),
                        _ => (c[0], c[2]),
                    };
                    let n = mesher::NORMALS[face];
                    let local_n = Vec3::new(n[0] as f32, n[1] as f32, n[2] as f32);
                    let normal = east * local_n.x + up * local_n.y + north * local_n.z;
                    let world = origin + east * c[0] + up * c[1] + north * c[2];
                    entity_verts.push(mesher::Vertex {
                        pos: world.to_array(),
                        uv: [
                            tx as f32 * ts + inset + uu * (ts - 2.0 * inset),
                            ty as f32 * ts + inset + vv * (ts - 2.0 * inset),
                        ],
                        normal: normal.to_array(),
                        light: lum.0,
                        sky: lum.1,
                        ao: 1.0,
                    });
                }
                entity_idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }

        Geometry { vertices: entity_verts, indices: entity_idx }
    }
}
