//! Viewmodel in the graphical frame pipeline.

use crate::world::TerrainRead;
use crate::atlas;
use crate::mesher;
use crate::registry::ItemId;
use crate::style;
use glam::Vec3;
use crate::game::Game;
use crate::game::navigation::Screen;
use super::{VIEWMODEL_SLEEVE_MIN, VIEWMODEL_SLEEVE_MAX, VIEWMODEL_HAND_MIN, VIEWMODEL_HAND_MAX};

impl Game {
    /// First-person viewmodel: your arm, or the block/item it holds,
    /// anchored low-right of the camera, walk-bobbed, and swung on use.
    /// Emitted in world space; the renderer draws it depth-cleared so
    /// it never sinks into a wall you're standing against.
    pub(in crate::game) fn emit_hand(&self, verts: &mut Vec<mesher::Vertex>, idx: &mut Vec<u32>) {
        if !self.in_world
            || self.survival.health <= 0.0
            || self.ui_state.screen != Screen::Playing
            || self.camera.mode != crate::camera::CameraMode::First
        {
            return;
        }
        let reg = self.content.reg.clone();
        let held = self.inventory.slots[self.input.hotbar_sel];
        let implement_visual = held.and_then(|stack| self.runtime.view().implement_visual(stack));

        // Camera basis: f forward, r screen-right, u screen-up.
        let f = self.camera.forward();
        let mut r = f.cross(self.camera.up());
        if r.length_squared() < 1e-6 {
            r = Vec3::new(-self.camera.yaw.sin(), 0.0, self.camera.yaw.cos());
        }
        let r = r.normalize();
        let u = r.cross(f).normalize();

        // Swing arc peaks mid-animation (progress runs 1 -> 0).
        // Dev: WILDFORGE_POSE freezes the swing for screenshots.
        let swing = std::env::var("WILDFORGE_POSE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(self.presentation.swing);
        let arc = ((1.0 - swing) * std::f32::consts::PI).sin();
        let bow_charge = if held.is_some_and(|s| reg.item(s.item).bow.is_some()) {
            self.interaction.bow_draw.min(1.0)
        } else {
            0.0
        };

        // Anchor low-right, breathing with the walk.
        let moving = (Vec3::new(self.player.vel.x, 0.0, self.player.vel.z)
            .length()
            .min(4.0))
            / 4.0;
        let bob_x = self.presentation.hand_bob.sin() * 0.02 * moving;
        let bob_y = -(self.presentation.hand_bob * 2.0).sin().abs() * 0.025 * moving;
        let mut anchor = self.camera.pos + f * 0.60 + r * (0.47 + bob_x) + u * (-0.46 + bob_y);
        // Implements settle into the sight line and then relax; other tools
        // keep the established strike swing. A wand is an instrument, not a
        // gun whose muzzle automatically kicks on every use.
        if implement_visual.is_some() {
            anchor += (f * 0.10 + u * 0.10 - r * 0.15) * arc;
        } else {
            anchor += (f * 0.08 - u * 0.05 - r * 0.08) * arc;
        }
        anchor += (-r * 0.16 + f * 0.04) * bow_charge;
        if self.survival.eating > 0.0 {
            // Nibbling: toward the face, jittering.
            anchor += -r * 0.14 + u * (0.04 + (self.time_abs * 16.0).sin() * 0.02);
        }
        if self.interaction.brushing > 0.0 {
            // Scrubbing side to side.
            anchor += r * (self.time_abs * 22.0).sin() * 0.03;
        }

        // Local space: x right, y up, z forward. The swing dips the tip
        // forward-down and sweeps it inward, hinged at the wrist.
        let a = arc
            * if implement_visual.is_some() {
                0.16
            } else {
                0.85
            };
        let b = arc
            * if implement_visual.is_some() {
                0.12
            } else {
                0.8
            };
        let xf = |q: Vec3| -> Vec3 {
            let (sa, ca) = a.sin_cos();
            let q = Vec3::new(q.x, q.y * ca - q.z * sa, q.y * sa + q.z * ca);
            let (sb, cb) = b.sin_cos();
            let q = Vec3::new(q.x * cb - q.z * sb, q.y, q.x * sb + q.z * cb);
            anchor + r * q.x + u * q.y + f * q.z
        };

        // Lit like anything standing where the camera stands.
        let (bl, sl) = self
            .player
            .eye()
            .block()
            .map_or((0, 15), |pos| self.runtime.view().light_at_pos(pos));
        let lum = (bl as f32 / 15.0, sl as f32 / 15.0);

        // A local-space box textured one tile per face (arm, held block).
        fn cube(
            verts: &mut Vec<mesher::Vertex>,
            idx: &mut Vec<u32>,
            xf: &dyn Fn(Vec3) -> Vec3,
            min: Vec3,
            max: Vec3,
            tiles: [u16; 6],
            lum: (f32, f32),
        ) {
            let ts = 1.0 / atlas::ATLAS_TILES as f32;
            let inset = ts / 32.0;
            for (face, (&slot, corners)) in tiles.iter().zip(mesher::CORNERS.iter()).enumerate() {
                let (tx, ty) = (
                    slot as u32 % atlas::ATLAS_TILES,
                    slot as u32 / atlas::ATLAS_TILES,
                );
                let base = verts.len() as u32;
                for c in corners.iter() {
                    let lp = Vec3::new(
                        min.x + c[0] * (max.x - min.x),
                        min.y + c[1] * (max.y - min.y),
                        min.z + c[2] * (max.z - min.z),
                    );
                    let wp = xf(lp);
                    let (uu, vv) = match face {
                        0 | 1 => (c[2], 1.0 - c[1]),
                        4 | 5 => (c[0], 1.0 - c[1]),
                        _ => (c[0], c[2]),
                    };
                    let shade = mesher::FACE_SHADE[face].max(0.7);
                    verts.push(mesher::Vertex {
                        pos: [wp.x, wp.y, wp.z],
                        uv: [
                            tx as f32 * ts + inset + uu * (ts - 2.0 * inset),
                            ty as f32 * ts + inset + vv * (ts - 2.0 * inset),
                        ],
                        normal: [0.0, 0.0, 0.0],
                        light: [shade * lum.0, shade * lum.0, shade * lum.0],
                        sky: shade * lum.1,
                        ao: 1.0,
                    });
                }
                idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
        // A thin, double-sided material panel used for first-person focus
        // silhouettes. Degenerate corners are intentional for spear tips.
        fn panel(
            verts: &mut Vec<mesher::Vertex>,
            idx: &mut Vec<u32>,
            xf: &dyn Fn(Vec3) -> Vec3,
            corners: [Vec3; 4],
            slot: u16,
            lum: (f32, f32),
            glow: u8,
        ) {
            let ts = 1.0 / atlas::ATLAS_TILES as f32;
            let inset = ts / 32.0;
            let (tx, ty) = (
                slot as u32 % atlas::ATLAS_TILES,
                slot as u32 / atlas::ATLAS_TILES,
            );
            for flip in [false, true] {
                let base = verts.len() as u32;
                let order = if flip {
                    [1usize, 0, 3, 2]
                } else {
                    [0, 1, 2, 3]
                };
                for (corner, (uu, vv)) in
                    order
                        .into_iter()
                        .zip([(0.0f32, 1.0f32), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)])
                {
                    let wp = xf(corners[corner]);
                    let shine = f32::from(glow) * 0.16;
                    verts.push(mesher::Vertex {
                        pos: wp.to_array(),
                        uv: [
                            tx as f32 * ts + inset + uu * (ts - 2.0 * inset),
                            ty as f32 * ts + inset + vv * (ts - 2.0 * inset),
                        ],
                        normal: [0.0, 0.0, 0.0],
                        light: [
                            (lum.0 + shine * 0.65).min(1.4),
                            (lum.0 + shine * 0.85).min(1.4),
                            (lum.0 + shine).min(1.4),
                        ],
                        sky: lum.1,
                        ao: 1.0,
                    });
                }
                idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
        // Pre-rotations in local space (3/4 view for blocks, arm angle).
        let pre_y = |ang: f32| {
            move |q: Vec3| {
                let (s, c) = ang.sin_cos();
                Vec3::new(q.x * c - q.z * s, q.y, q.x * s + q.z * c)
            }
        };
        let pre_x = |ang: f32| {
            move |q: Vec3| {
                let (s, c) = ang.sin_cos();
                Vec3::new(q.x, q.y * c - q.z * s, q.y * s + q.z * c)
            }
        };

        match (held, implement_visual) {
            (Some(_), Some(visual)) => {
                let item_icon = |wire: u16| reg.item(ItemId(wire)).icon;
                let body = item_icon(visual.body);
                let reservoir = item_icon(visual.reservoir);
                let focus = item_icon(visual.focus);
                let binding = item_icon(visual.binding);
                let charge = visual.charge_band.min(3);

                // A long, material-textured body with visibly separate
                // reservoir and ferrules. The focus geometry, not merely its
                // colour, identifies the family of future workings.
                cube(
                    verts,
                    idx,
                    &xf,
                    Vec3::new(-0.035, -0.18, 0.035),
                    Vec3::new(0.035, 0.48, 0.105),
                    [body; 6],
                    lum,
                );
                cube(
                    verts,
                    idx,
                    &xf,
                    Vec3::new(-0.105, -0.07, 0.015),
                    Vec3::new(0.105, 0.13, 0.125),
                    [reservoir; 6],
                    ((lum.0 + f32::from(charge) * 0.10).min(1.35), lum.1),
                );
                for y in [0.10f32, 0.34] {
                    cube(
                        verts,
                        idx,
                        &xf,
                        Vec3::new(-0.075, y, 0.020),
                        Vec3::new(0.075, y + 0.045, 0.120),
                        [binding; 6],
                        lum,
                    );
                }
                // Item art does not share a common opaque footprint: the
                // mineral focus tiles fill most of their square while the
                // reservoir/body sprites have transparent margins. Keep the
                // focus geometry physically narrower than the shaft assembly
                // so an opaque Echo Slate does not become a detached shield.
                let center = Vec3::new(0.0, 0.525, 0.07);
                let rect = |cx: f32, cy: f32, hw: f32, hh: f32| {
                    [
                        Vec3::new(cx - hw, cy - hh, center.z),
                        Vec3::new(cx + hw, cy - hh, center.z),
                        Vec3::new(cx + hw, cy + hh, center.z),
                        Vec3::new(cx - hw, cy + hh, center.z),
                    ]
                };
                match visual.focus_shape {
                    1 => panel(
                        verts,
                        idx,
                        &xf,
                        [
                            center + Vec3::new(0.0, -0.075, 0.0),
                            center + Vec3::new(0.075, 0.0, 0.0),
                            center + Vec3::new(0.0, 0.075, 0.0),
                            center + Vec3::new(-0.075, 0.0, 0.0),
                        ],
                        focus,
                        lum,
                        charge,
                    ),
                    2 => {
                        panel(
                            verts,
                            idx,
                            &xf,
                            rect(0.0, center.y, 0.095, 0.018),
                            focus,
                            lum,
                            charge,
                        );
                        for x in [-0.068f32, 0.068] {
                            panel(
                                verts,
                                idx,
                                &xf,
                                rect(x, center.y + 0.048, 0.018, 0.062),
                                focus,
                                lum,
                                charge,
                            );
                        }
                    }
                    3 => panel(
                        verts,
                        idx,
                        &xf,
                        [
                            center + Vec3::new(-0.052, -0.062, 0.0),
                            center + Vec3::new(0.052, -0.062, 0.0),
                            center + Vec3::new(0.0, 0.103, 0.0),
                            center + Vec3::new(0.0, 0.103, 0.0),
                        ],
                        focus,
                        lum,
                        charge,
                    ),
                    _ => {
                        panel(
                            verts,
                            idx,
                            &xf,
                            rect(0.0, center.y, 0.032, 0.062),
                            focus,
                            lum,
                            charge,
                        );
                        for x in [-0.058f32, 0.058] {
                            panel(
                                verts,
                                idx,
                                &xf,
                                rect(x, center.y + 0.038, 0.016, 0.055),
                                focus,
                                lum,
                                charge,
                            );
                        }
                    }
                }
            }
            // A block rides as a mini-cube, turned for a 3/4 view.
            (Some(st), None)
                if reg
                    .item(st.item)
                    .places
                    .is_some_and(|pb| !reg.block(pb).cross) =>
            {
                let pb = reg.item(st.item).places.unwrap();
                let tilt = pre_y(0.65);
                let x2 = |q: Vec3| xf(tilt(q));
                cube(
                    verts,
                    idx,
                    &x2,
                    Vec3::new(-0.10, -0.12, -0.01),
                    Vec3::new(0.10, 0.08, 0.19),
                    reg.block(pb).tiles,
                    lum,
                );
            }
            // Anything else shows as its icon: a flat angled card,
            // drawn double-sided like dropped item sprites.
            (Some(st), None) => {
                let slot = reg.item(st.item).icon;
                let ts = 1.0 / atlas::ATLAS_TILES as f32;
                let inset = ts / 32.0;
                let (tx, ty) = (
                    slot as u32 % atlas::ATLAS_TILES,
                    slot as u32 / atlas::ATLAS_TILES,
                );
                let ax = Vec3::new(0.44, 0.07, -0.21);
                let ay = Vec3::new(-0.10, 0.44, 0.17);
                let origin = Vec3::new(0.02, -0.16, 0.06);
                for flip in [false, true] {
                    let base = verts.len() as u32;
                    let (u0, u1) = if flip {
                        ((tx + 1) as f32 * ts - inset, tx as f32 * ts + inset)
                    } else {
                        (tx as f32 * ts + inset, (tx + 1) as f32 * ts - inset)
                    };
                    let s = if flip { -1.0 } else { 1.0 };
                    for (i, j, uu) in [
                        (-0.5, 0.0, u0),
                        (0.5, 0.0, u1),
                        (0.5, 1.0, u1),
                        (-0.5, 1.0, u0),
                    ] {
                        let lp = origin + ax * (i * s) + ay * j;
                        let wp = xf(lp);
                        let vv = if j < 0.5 {
                            (ty + 1) as f32 * ts - inset
                        } else {
                            ty as f32 * ts + inset
                        };
                        verts.push(mesher::Vertex {
                            pos: [wp.x, wp.y, wp.z],
                            uv: [uu, vv],
                            normal: [0.0, 0.0, 0.0],
                            light: [0.95 * lum.0, 0.95 * lum.0, 0.95 * lum.0],
                            sky: 0.95 * lum.1,
                            ao: 1.0,
                        });
                    }
                    idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                }
            }
            // Bare hand: your forearm — sleeve in your shirt color,
            // hand in your skin tone (the same body others see).
            (None, _) => {
                let skin = style::skin_tile(&self.style);
                let sleeve = style::shirt_tile(&self.style);
                let ty = pre_y(-0.30);
                let tx = pre_x(-0.55);
                let x2 = |q: Vec3| xf(ty(tx(q)));
                cube(
                    verts,
                    idx,
                    &x2,
                    VIEWMODEL_SLEEVE_MIN,
                    VIEWMODEL_SLEEVE_MAX,
                    [sleeve; 6],
                    lum,
                );
                cube(
                    verts,
                    idx,
                    &x2,
                    VIEWMODEL_HAND_MIN,
                    VIEWMODEL_HAND_MAX,
                    [skin; 6],
                    lum,
                );
            }
        }
    }
}
