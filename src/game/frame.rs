//! Per-frame client update, scene assembly, and renderer submission.

use crate::atlas;
use crate::audio;
use crate::audio::Sfx;
use crate::chunk::CHUNK_X;
use crate::entity;
use crate::entity::ItemEntity;
use crate::lights;
use crate::mesher;
use crate::mobs;
use crate::mp;
use crate::net;
use crate::physics;
use crate::raycast;
use crate::registry::ItemId;
use crate::renderer::FrameInput;
use crate::server;
use crate::style;
use crate::visual_capture;
use crate::world;
use crate::world::World;
use glam::Vec3;
use std::time::Instant;
use super::Game;
use super::advance_capture_clock;
use super::SHOT_FIXED_DT;
use super::SHOT_MAX_FRAMES;
use super::SHOT_SETTLE_FRAMES;
use super::combat;
use super::content_watch::content_tree_stamp;
use super::input::KeysDown;
use super::navigation::Screen;

/// Local authority pauses for solo play, but a windowed host keeps serving
/// guests. Remote guests never enter this path at all.
fn local_sim_should_advance(paused: bool, hosting: bool) -> bool {
    !paused || hosting
}

fn movement_axes(keys: &KeysDown) -> (f32, f32) {
    static AUTO_MOVE: std::sync::OnceLock<Option<(f32, f32)>> = std::sync::OnceLock::new();
    if let Some(axes) = *AUTO_MOVE.get_or_init(|| {
        std::env::var("WILDFORGE_AUTO_MOVE").ok().and_then(|value| {
            match value.to_ascii_uppercase().as_str() {
                "A" => Some((0.0, -1.0)),
                "D" => Some((0.0, 1.0)),
                "W" => Some((1.0, 0.0)),
                "S" => Some((-1.0, 0.0)),
                _ => None,
            }
        })
    }) {
        return axes;
    }
    (
        (keys.w as i32 - keys.s as i32) as f32,
        (keys.d as i32 - keys.a as i32) as f32,
    )
}

const VIEWMODEL_SLEEVE_MIN: Vec3 = Vec3::new(-0.055, -0.09, -0.46);
const VIEWMODEL_SLEEVE_MAX: Vec3 = Vec3::new(0.055, 0.02, 0.10);
const VIEWMODEL_HAND_MIN: Vec3 = Vec3::new(-0.055, -0.09, 0.10);
const VIEWMODEL_HAND_MAX: Vec3 = Vec3::new(0.055, 0.02, 0.26);

/// How far in front of the camera to stand the paper doll so its body
/// covers exactly `target_px` of screen height. The UI rect is fixed
/// pixels and the projection is not, so this has to be solved per frame
/// rather than picked once: a constant only ever framed one resolution.
///
/// The doll stands along world Y while the screen's vertical runs along
/// camera up, so a pitched camera foreshortens it — pulling in by the
/// same cosine keeps the framing whichever way the player was looking.
pub(super) fn portrait_depth(fovy: f32, pitch: f32, screen_h: f32, target_px: f32) -> f32 {
    let lean = pitch.cos().abs().max(0.25);
    mobs::HUMANOID_HEIGHT * lean * (screen_h * 0.5)
        / ((fovy * 0.5).tan() * target_px.max(1.0)).max(0.001)
}

/// How much brighter direct sun is than the old parity-with-sky default.
/// `WILDFORGE_SUN` overrides it.
fn sun_scale() -> f32 {
    use std::sync::OnceLock;
    static S: OnceLock<f32> = OnceLock::new();
    *S.get_or_init(|| {
        std::env::var("WILDFORGE_SUN")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|v: &f32| *v > 0.0 && *v <= 100.0)
            .unwrap_or(30.0)
    })
}

impl Game {
    /// First-person viewmodel: your arm, or the block/item it holds,
    /// anchored low-right of the camera, walk-bobbed, and swung on use.
    /// Emitted in world space; the renderer draws it depth-cleared so
    /// it never sinks into a wall you're standing against.
    pub(super) fn emit_hand(&self, verts: &mut Vec<mesher::Vertex>, idx: &mut Vec<u32>) {
        if !self.in_world
            || self.survival.health <= 0.0
            || self.ui_state.screen != Screen::Playing
            || self.camera.mode != crate::camera::CameraMode::First
        {
            return;
        }
        let reg = self.content.reg.clone();
        let held = self.inventory.slots[self.input.hotbar_sel];
        let implement_visual = held.and_then(|stack| self.server.world.implement_visual(stack));

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
            .map_or((0, 15), |pos| self.server.world.light_at_pos(pos));
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

    /// Dev: WILDFORGE_LOOK pins yaw,pitch (applied at spawn, and every
    /// frame during WILDFORGE_SHOT runs — synthetic WSLg mouse events
    /// drift the camera over long headless warmups otherwise).
    pub(super) fn apply_look_env(&mut self) {
        if let Ok(l) = std::env::var("WILDFORGE_LOOK")
            && let Some((y, p)) = l.split_once(',')
            && let (Ok(y), Ok(p)) = (y.trim().parse::<f32>(), p.trim().parse::<f32>())
        {
            self.camera.yaw = y;
            self.camera.pitch = p;
        }
    }

    fn begin_frame(&mut self) -> (Instant, f32, bool) {
        let now = Instant::now();
        let dt = if self.auto_shot.is_some() {
            SHOT_FIXED_DT
        } else {
            (now - self.last_frame).as_secs_f32().min(0.05)
        };
        self.last_frame = now;
        self.time_abs += dt;

        let paused = self.ui_state.screen == Screen::Paused || !self.in_world;
        if !paused {
            self.combat
                .tick(dt, self.stamina_max(), self.stamina_regen());
            self.input.action_cooldown = (self.input.action_cooldown - dt).max(0.0);
            self.input.attack_cooldown = (self.input.attack_cooldown - dt).max(0.0);
            self.input.scroll_cooldown = (self.input.scroll_cooldown - dt).max(0.0);
            // The hand-hitch holds the swing at its peak on a connect.
            if self.presentation.hitch > 0.0 {
                self.presentation.hitch -= dt;
            } else {
                self.presentation.swing = (self.presentation.swing - dt / 0.3).max(0.0);
            }
            let hv = Vec3::new(self.player.vel.x, 0.0, self.player.vel.z).length();
            if self.player.on_ground {
                self.presentation.hand_bob += hv.min(6.0) * dt * 1.6;
            }
        }

        (now, dt, paused)
    }

    fn advance_feedback(&mut self, dt: f32, paused: bool) {
        // The juice layer's clock: particles, pulses, streaks, motion.
        self.presentation.pool.tick(dt);
        // Ambience nobody profits from: a fluttering speck near the
        // canopy by day, a dragonfly skimming the water. Client-side
        // only — some of the world is just living here.
        self.presentation.ambient_timer -= dt;
        if !paused && self.presentation.ambient_timer <= 0.0 && self.presentation.juice {
            self.presentation.ambient_timer = 1.6 + self.presentation.vary() * 2.4;
            let day = self
                .server
                .world
                .daylight_at_surface(self.player.pos.surface())
                > 0.5;
            let r1 = self.presentation.vary();
            let r2 = self.presentation.vary();
            let r3 = self.presentation.vary();
            let Ok(sample) = self.player.eye().translated(Vec3::new(
                (r1 - 0.5) * 24.0,
                (r2 - 0.3) * 8.0,
                (r3 - 0.5) * 24.0,
            )) else {
                return;
            };
            let sample = sample.pos;
            let p = sample.render_pos();
            let Some(center) = sample.block() else {
                return;
            };
            let reg = self.content.reg.clone();
            let near = |pred: &dyn Fn(&str) -> bool| -> bool {
                (-2..=2i32).any(|dx| {
                    (-2..=2i32).any(|dy| {
                        (-2..=2i32).any(|dz| {
                            center.offset(dx, dy, dz).is_some_and(|at| {
                                pred(&reg.block(self.server.world.get_block_at(at)).name)
                            })
                        })
                    })
                })
            };
            if day && near(&|n: &str| n.contains("leaves")) {
                // A songbird-or-butterfly speck breaking from the canopy.
                if let Some(b) = reg.block_id("base:berry_bush") {
                    self.presentation.puff(p, reg.block(b).tiles[0], 1);
                }
            } else if day && near(&|n: &str| n.contains("water")) {
                // A dragonfly working the surface.
                if let Some(b) = reg.block_id("base:kelp_frond") {
                    self.presentation.puff(p, reg.block(b).tiles[0], 1);
                }
            }
        }
        self.presentation.screen_age = (self.presentation.screen_age + dt / 0.14).min(1.0);
        self.presentation.sel_bounce = (self.presentation.sel_bounce + dt / 0.12).min(1.0);
        self.presentation.press_dip = (self.presentation.press_dip - dt).max(0.0);
        self.presentation.nudge.1 = (self.presentation.nudge.1 - dt).max(0.0);
        for p in self.presentation.slot_pulse.iter_mut() {
            *p = (*p - dt).max(0.0);
        }
        self.presentation.pickup_streak.1 = (self.presentation.pickup_streak.1 - dt).max(0.0);
        if self.presentation.pickup_streak.1 <= 0.0 {
            self.presentation.pickup_streak.0 = 0;
        }
        for f in self.presentation.ui_flies.iter_mut() {
            f.3 += dt;
        }
        self.presentation.ui_flies.retain(|f| f.3 < 0.22);
        // A hostile you haven't met yet announces itself now and then:
        // hearing the threat before seeing it is the point.
        self.presentation.presence_timer -= dt;
        if self.presentation.presence_timer <= 0.0
            && self.in_world
            && self.presentation.juice
            && self.ui_state.screen == Screen::Playing
        {
            self.presentation.presence_timer = 6.0 + (self.presentation.vary() - 0.9) * 20.0;
            let reg = self.content.reg.clone();
            let lurker = self
                .server
                .world
                .mobs()
                .iter()
                .filter(|m| {
                    reg.animals[m.species].hostile && m.state != crate::mobs::MobState::Hunt
                })
                .map(|m| ((m.pos - self.player.pos).length(), m.species))
                .filter(|(d, _)| *d < 20.0)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((d, sp)) = lurker {
                let vol = (1.0 - d / 24.0).max(0.2);
                self.sfx_vol(Sfx::Presence(reg.animals[sp].sound_pitch), vol);
            }
        }
        // The stomach speaks before the bar empties.
        self.presentation.hunger_timer -= dt;
        if self.presentation.hunger_timer <= 0.0 {
            let gap = if self.survival.hunger < 2.0 {
                10.0
            } else {
                20.0
            };
            self.presentation.hunger_timer = gap;
            if self.survival.hunger < 5.0
                && self.in_world
                && self.presentation.juice
                && self.ui_state.screen == Screen::Playing
            {
                self.sfx_vol(Sfx::Rumble, 0.7);
            }
        }

        if let Some((at, tile)) = self.presentation.demo_burst
            && self.total_frames.is_multiple_of(10)
        {
            self.presentation.burst(at, tile, 10, 2.2);
            self.survival.damage_flash = 0.4;
        }

        // Footprints in snow: not juice — the trail is world state.
        if self.in_world
            && !paused
            && self.multiplayer.remote.is_none()
            && self.player.on_ground
            && let Some(at) = self.player.pos.block()
        {
            self.server.world.tread_at(at);
        }

        // Footsteps: mine, my fellow players', and the creatures'.
        if self.in_world && !paused && self.presentation.juice {
            let hv = Vec3::new(self.player.vel.x, 0.0, self.player.vel.z).length();
            if self.player.on_ground && hv > 0.5 {
                self.presentation.step_accum += hv * dt;
                if self.presentation.step_accum >= 2.2 {
                    self.presentation.step_accum = 0.0;
                    let m = self.step_mat_at(self.player.pos);
                    let pitch = self.presentation.vary();
                    self.sfx(Sfx::Step(m, pitch));
                }
            } else if hv <= 0.5 {
                self.presentation.step_accum = 0.0;
            }
            // Mobs step when their stride phase crosses a beat.
            let cam = self.camera.pos;
            let mut steps: Vec<(audio::StepMat, f32, f32)> = Vec::new();
            for m in self.server.world.mobs() {
                let d = (m.pos.render_pos() - cam).length();
                if d > 16.0 || m.id == 0 {
                    continue;
                }
                let beat = (m.anim_phase / std::f32::consts::PI).floor();
                let last = self.presentation.mob_strides.insert(m.id, beat);
                if last.is_some_and(|l| beat > l) {
                    let mat = self.step_mat_at(m.pos);
                    let pitch = self.content.reg.animals[m.species].sound_pitch;
                    steps.push((mat, pitch, 1.0 - d / 18.0));
                }
            }
            // Remote players step by distance walked, like we do.
            let remote_players: Vec<(u32, crate::planet::EntityPos)> = self
                .multiplayer
                .remote
                .as_ref()
                .map(|r| {
                    r.player_positions
                        .iter()
                        .map(|(id, pos)| (*id, *pos))
                        .collect()
                })
                .unwrap_or_default();
            for (id, pos) in remote_players {
                let (last, mut accum) = self
                    .presentation
                    .remote_strides
                    .get(&id)
                    .copied()
                    .unwrap_or((pos, 0.0));
                let moved = last.horizontal_distance_to(pos);
                accum += moved;
                if accum >= 2.2 {
                    accum = 0.0;
                    let d = (pos.render_pos() - cam).length();
                    if d < 16.0 {
                        let mat = self.step_mat_at(pos);
                        steps.push((mat, 1.0, 1.0 - d / 18.0));
                    }
                }
                self.presentation.remote_strides.insert(id, (pos, accum));
            }
            for (mat, pitch, vol) in steps {
                let p = pitch * self.presentation.vary();
                self.sfx_vol(Sfx::Step(mat, p), vol * 0.6);
            }
        }
    }

    fn advance_session_authority(&mut self, dt: f32, paused: bool) {
        let t0 = std::time::Instant::now();
        self.advance_session_authority_inner(dt, paused);
        let ms = t0.elapsed().as_secs_f32() * 1000.0;
        self.frame_ms.0 = self.frame_ms.0 * 0.95 + ms * 0.05;
    }

    fn advance_session_authority_inner(&mut self, dt: f32, paused: bool) {
        // No autosave. Doll asked for this the first time it stuttered
        // and I argued to keep it; the hitch came back and chasing it
        // is not worth the feature. The world is written on the paths
        // that already existed and always have: closing the window,
        // quitting to the title, sleeping the night, and every chunk
        // that leaves the view (save_chunk_if_modified on unload).
        //
        // Why it returned after being fixed: that fix stopped rewriting
        // UNCHANGED chunks, which is most of them at a seven-chunk
        // view. At twenty-four the loaded set is ten times larger, so
        // ten times as many chunks are genuinely changed each pass by
        // fluids and random ticks — the same wall, reached from the
        // other side. A save that scales with view distance was never
        // going to sit quietly on a timer.
        //
        // The exposure is an unclean exit: a crash or a kill loses the
        // work since the last of those points. On a machine that is
        // being played rather than shipped, that is the better trade.
        if !self.in_world && self.multiplayer.remote.is_some() {
            self.remote_pump(dt);
        }
        if self.in_world {
            let profile_frame = self.total_frames.is_multiple_of(60)
                && std::env::var_os("WILDFORGE_PROFILE").is_some();
            let stream_started = std::time::Instant::now();
            self.stream_chunks();
            if profile_frame {
                eprintln!(
                    "profile: stream {:.2}ms, resident {}, dirty {}",
                    stream_started.elapsed().as_secs_f64() * 1_000.0,
                    self.server.world.chunk_count(),
                    self.server.world.dirty_chunks().len(),
                );
            }
            // The authoritative simulation steps at its fixed tick; the
            // client applies the results as presentation.
            if self.multiplayer.remote.is_some() {
                self.remote_pump(dt);
            } else if local_sim_should_advance(paused, self.multiplayer.host.is_some()) {
                let ctx = server::PlayerCtx {
                    id: 0,
                    pos: self.player.pos,
                    spawn: self.survival.spawn_point,
                    attackable: self.survival.attackable(self.creative),
                    aggro_mod: if self.charm("quiet") {
                        -crate::implements::QUIET_CHARM_AGGRO_REDUCTION
                    } else {
                        0.0
                    },
                    quiet_charm: self.survival.armor[4]
                        .filter(|stack| self.server.world.charm_can_pay(*stack, "quiet")),
                };
                // Hosting: guests are simulated players too, and their
                // requests apply before the tick.
                let players = if let Some(mut sess) = self.multiplayer.host.take() {
                    self.server.world.set_edit_logging(true);
                    let held = self.inventory.slots[self.input.hotbar_sel];
                    let fx = sess.pump_with_host_stack(
                        &mut self.server,
                        Some((
                            self.player.pos,
                            self.camera.yaw,
                            self.multiplayer.host_sleeping,
                            held,
                            self.style.pack(),
                        )),
                        dt,
                    );
                    for f in fx {
                        match f {
                            mp::HostFx::Chat { from, msg } => {
                                self.toast(format!("{from}: {msg}"));
                            }
                            mp::HostFx::Joined(n) => self.toast(format!("{n} joined.")),
                            mp::HostFx::Left(n) => self.toast(format!("{n} left.")),
                            mp::HostFx::ImplementActivation { pos, cue, visual } => {
                                self.present_implement_activation(pos, cue, visual, None);
                            }
                            mp::HostFx::WorkingEvent(cue) => self.present_working_cue(cue),
                            mp::HostFx::AlchemyEvent(cue) => self.present_alchemy_cue(cue),
                            mp::HostFx::AllSlept => {
                                self.multiplayer.host_sleeping = false;
                                self.survival.spawn_point = self.player.pos;
                                self.toast("Dawn. The camp wakes.".to_string());
                            }
                            mp::HostFx::ScreenClick { screen, action } => {
                                // Capability E11: a guest's mod-screen
                                // button. Ids were validated host-side;
                                // dispatch the mod's hook here where the
                                // scripts live.
                                if self.content.scripts.wants("on_screen_click") {
                                    self.content.scripts.dispatch(
                                        &self.server.world,
                                        "on_screen_click",
                                        (screen, action),
                                    );
                                    self.apply_script_cmds();
                                }
                            }
                        }
                    }
                    let players = sess.authoritative_player_ctxs(&self.server.world, Some(ctx));
                    self.multiplayer.host = Some(sess);
                    players
                } else {
                    vec![ctx]
                };
                let mut evs = Vec::new();
                let server_started = std::time::Instant::now();
                self.server.advance(dt, &players, &mut evs);
                if profile_frame {
                    eprintln!(
                        "profile: server {:.2}ms, mobs {}, projectiles {}",
                        server_started.elapsed().as_secs_f64() * 1_000.0,
                        self.server.world.mobs().len(),
                        self.server.world.projectiles().len(),
                    );
                }
                for ev in evs {
                    match ev {
                        server::SimEvent::PlayerHit {
                            who,
                            dmg,
                            dmg_type,
                            attack,
                            from,
                        } => {
                            if who == 0 && self.multiplayer.remote.is_none() {
                                if self.content.scripts.wants("on_attack") {
                                    self.content.scripts.dispatch(
                                        &self.server.world,
                                        "on_attack",
                                        (
                                            attack.clone(),
                                            dmg as f64,
                                            dmg_type.clone().unwrap_or_default(),
                                        ),
                                    );
                                    self.apply_script_cmds();
                                }
                                self.hurt_player_from_wild(dmg, from, dmg_type.as_deref());
                            } else if let Some(sess) = &mut self.multiplayer.host {
                                // `who` is that guest's own net id.
                                sess.hurt_guest(&mut self.server, who, dmg, from);
                            }
                        }
                        server::SimEvent::BoltCast => self.sfx(Sfx::Bolt(1.2)),
                        server::SimEvent::Lightning(at) => {
                            // A LANDED bolt: a longer flash, thunder
                            // timed by distance, and a white column
                            // standing on the strike for a beat.
                            self.presentation.lightning = 0.3;
                            let at = at.render_pos();
                            let dist = (at - self.camera.pos).length();
                            self.presentation.thunder_delay = (dist / 110.0).clamp(0.1, 2.0);
                            let white = *atlas::builtin_slots().get("snow").unwrap_or(&39);
                            for dy in 0..26 {
                                self.presentation.burst(
                                    at + Vec3::new(0.0, dy as f32 * 1.1, 0.0),
                                    white,
                                    2,
                                    0.5,
                                );
                            }
                        }
                        server::SimEvent::Bred => {
                            self.sfx(Sfx::Pickup);
                            self.toast("New life stirs in the wild.".to_string());
                        }
                        server::SimEvent::QuietSheltered { who } => {
                            if who == 0 {
                                self.sfx(Sfx::Click);
                            }
                        }
                        server::SimEvent::MobDied(death) => {
                            self.present_settled_mob_death(death);
                            self.grant_xp("kill");
                        }
                        server::SimEvent::Dawn { offering_refund } => {
                            if offering_refund > 0.0 {
                                self.sfx(Sfx::Pickup);
                                self.toast("The wild has accepted your offering.".to_string());
                            }
                        }
                        server::SimEvent::LongWinter(fell) => {
                            self.toast(if fell {
                                "The year has stopped turning. Spring does not come.".to_string()
                            } else {
                                "The year turns again.".to_string()
                            });
                        }
                        server::SimEvent::IreTier { rose, tier } => {
                            let name = world::IRE_TIERS[tier.min(world::IRE_TIERS.len() - 1)];
                            self.toast(if rose {
                                format!("The wild stirs against you - {name}.")
                            } else {
                                format!("The wild settles - {name}.")
                            });
                        }
                        server::SimEvent::Working(result, cue) => {
                            if let Some(session) = &self.multiplayer.host {
                                session.broadcast_working_cue(cue.clone());
                            }
                            self.present_working_cue(cue);
                            self.toast(result.message);
                        }
                        server::SimEvent::Alchemy(cue) => {
                            if let Some(session) = &self.multiplayer.host {
                                session.broadcast_alchemy_cue(cue.clone());
                            }
                            self.present_alchemy_cue(cue);
                        }
                        server::SimEvent::Dross(cue) => {
                            if let Some(session) = &self.multiplayer.host {
                                session.broadcast_dross_cue(&self.server.world, cue);
                            }
                            let local_region = self
                                .server
                                .world
                                .planet_atlas()
                                .map(|atlas| atlas.atlas_pos(self.player.pos.surface()));
                            if local_region == Some(cue.region) {
                                self.present_dross_cue(cue);
                            }
                        }
                    }
                }
                for (pos, s) in self.server.world.take_pending_drops() {
                    let center = pos.entity_center();
                    let a = self.rand01() * std::f32::consts::TAU;
                    let v = Vec3::new(a.cos() * 1.5, 2.5, a.sin() * 1.5);
                    let mut entity = ItemEntity::new(center, v, s.item, s.count);
                    entity.durability = s.durability;
                    entity.arcane_id = s.arcane_id;
                    self.server.world.spawn_loose_item(entity);
                }
                // The wild's whispers reach the ear as toasts.
                for line in std::mem::take(&mut self.server.world.whispers) {
                    self.toast(line);
                }
                // Crossing into marked country: one line per region
                // per session, hostile or blessed.
                let surface = self.player.pos.surface();
                let cell = world::RegionCell::from_surface(surface);
                if self.presentation.last_ire_cell != Some(cell) {
                    self.presentation.last_ire_cell = Some(cell);
                    let standing = self.server.world.regional_ire_at_surface(surface);
                    if standing.abs() >= 8.0 && self.presentation.whispered_cells.insert(cell) {
                        self.toast(
                            if standing > 0.0 {
                                "The trees here remember the axe."
                            } else {
                                "This ground knows you."
                            }
                            .to_string(),
                        );
                    }
                }
                // Before a tuning lens exists, magical geography is learned
                // through signs rather than a debug number or raw atlas map.
                // Guests use only the host's coarse local bands; solo/host
                // players may ask their authoritative atlas for the same
                // unaided qualitative vocabulary.
                let arcane_sign = if self.multiplayer.remote.is_some() {
                    Some(crate::arcane_geography::coarse_sensory_cue(
                        self.server.world.remote_arcane_cue(),
                        self.server.world.remote_arcane_dominant(),
                    ))
                } else {
                    self.server.world.planet_atlas().and_then(|atlas| {
                        self.server
                            .world
                            .arcane_survey_at(atlas.atlas_pos(surface), false)
                            .map(|survey| survey.sensory_cue())
                    })
                };
                if let Some(sign) = arcane_sign
                    && self.presentation.arcane_signs.insert(sign.clone())
                {
                    self.toast(sign);
                }
                if let Some(observation) = self
                    .server
                    .world
                    .perceived_arcane_ecology_at(surface, self.scan_range())
                    && self
                        .presentation
                        .arcane_signs
                        .insert(observation.text.clone())
                {
                    self.toast(observation.text);
                }
                // Close container screens if their block vanished.
                if let Screen::Furnace(pos)
                | Screen::Chest(pos)
                | Screen::Offering(pos)
                | Screen::Bloomery(pos) = self.ui_state.screen
                    && self.server.world.block_entity_at(&pos).is_none()
                {
                    self.set_screen(Screen::Playing);
                }
                // Mod tick at 10 Hz.
                if self.content.scripts.wants("on_tick") {
                    self.multiplayer.tick_accum += dt;
                    if self.multiplayer.tick_accum >= 0.1 {
                        let t = self.multiplayer.tick_accum;
                        self.multiplayer.tick_accum = 0.0;
                        self.content
                            .scripts
                            .dispatch(&self.server.world, "on_tick", (t as f64,));
                        self.apply_script_cmds();
                    }
                }
            }
        }
    }

    fn refresh_content_and_toasts(&mut self, dt: f32) {
        // The turning of the season repaints the leaves.
        let local_season = self
            .server
            .world
            .season_at_surface(self.player.pos.surface());
        if self.in_world && local_season != self.presentation.atlas_season {
            let mut atlas = atlas::build_atlas(
                &self.content.reg.tex_files,
                &atlas::pack_chain(&self.active_pack_id()),
                &self.content.reg.tex_names,
            );
            atlas::season_tint(&mut atlas.color, atlas.px, local_season);
            self.presentation.atlas_season = local_season;
            self.content.pack_warnings = atlas.warnings;
            self.renderer.set_atlas(
                &atlas.color,
                &atlas.material,
                &atlas.normal,
                atlas.px,
                atlas.interior_base,
                &atlas.layer_params,
            );
        }

        // Hot reload: poll the mods + packs trees once a second.
        self.content.mods_poll += dt;
        if self.content.mods_poll >= 1.0 {
            self.content.mods_poll = 0.0;
            let stamp = content_tree_stamp();
            if stamp != self.content.mods_stamp {
                self.content.mods_stamp = stamp;
                self.reload_mods(false);
            }
        }
        for t in self.presentation.toasts.iter_mut() {
            t.1 -= dt;
        }
        self.presentation.toasts.retain(|t| t.1 > 0.0);
    }

    fn advance_player(&mut self, dt: f32, paused: bool) {
        // Physics — only once the chunk under the player exists.
        let Some(pchunk) = self.player.pos.chunk() else {
            return;
        };
        let can_sim = self.server.world.has_chunk(pchunk) && !paused;
        if can_sim && self.ui_state.screen != Screen::Dead {
            self.update_blocking();
            let (forward, strafe) = movement_axes(&self.input.keys);
            let sprinting = self.input.keys.sprint
                && self.survival.hunger >= 6.0
                && self.survival.preparation_modifiers.stamina_permille >= 900
                && (self.creative || self.combat.stamina >= combat::SPRINT_MIN_STAMINA);
            self.stamina_tick(dt, sprinting);
            // Dodge consumes the edge-triggered request at the top of the
            // move step so the burst applies before physics integrates.
            if self.input.dodge_pressed {
                self.input.dodge_pressed = false;
                self.try_dodge();
            }
            let guard = if self.combat.blocking { 0.45 } else { 1.0 };
            let input = physics::Input {
                forward: forward * guard,
                strafe: strafe * guard,
                jump: self.input.keys.space,
                sprint: sprinting,
                speed_mult: self.move_speed(),
            };
            if self.input.keys.space && self.player.on_ground {
                self.survival.hunger = (self.survival.hunger - 0.005).max(0.0);
            }
            // Getting up: any movement withdraws a pending sleep vote.
            if input.forward != 0.0 || input.strafe != 0.0 || input.jump {
                if self.multiplayer.host_sleeping {
                    self.multiplayer.host_sleeping = false;
                    self.toast("You get up.".to_string());
                }
                if self.multiplayer.remote.as_ref().is_some_and(|r| r.sleeping) {
                    let r = self.multiplayer.remote.as_mut().unwrap();
                    r.sleeping = false;
                    r.session.send(&net::C2S::SleepCancel);
                    self.toast("You get up.".to_string());
                }
            }
            self.update_food(dt, &input);
            if self.flying {
                let intent_length = input.forward.hypot(input.strafe).max(1.0);
                let wish = (self.camera.local_flat_forward() * input.forward
                    + self.camera.local_right() * input.strafe)
                    / intent_length;
                let mut v = wish * 9.0;
                if self.input.keys.space {
                    v.y += 8.0;
                }
                if self.input.keys.sprint {
                    v.y -= 8.0;
                }
                self.player.fly(&self.server.world, v, dt);
            }
            let was_in_water = self.player.in_water;
            let fall_speed = self.player.vel.y;
            if !self.flying {
                self.player.update(
                    &self.server.world,
                    &input,
                    self.camera.local_flat_forward(),
                    self.camera.local_right(),
                    dt,
                );
            }
            self.camera.yaw = self.player.frame_rotation.rotate_yaw(self.camera.yaw);
            // Aboard a boat: the hull carries you — float at the
            // surface, glide fast, and the boat glues underneath.
            // Jump steps off.
            if let Some(bid) = self.interaction.riding {
                let gone = self.server.world.mob_by_id(bid).is_none();
                if gone || input.jump {
                    if !gone && self.multiplayer.remote.is_some() {
                        if let Some(rc) = &self.multiplayer.remote {
                            rc.session.send(&net::C2S::RideMob {
                                id: bid,
                                mount: false,
                            });
                        }
                    } else if let Some(m) = self.server.world.mob_by_id_mut(bid) {
                        m.ridden_by = None;
                    }
                    self.interaction.riding = None;
                    self.player.vel.y = self.player.vel.y.max(4.0);
                } else {
                    if self.player.in_water {
                        // Buoyant hull: ride the surface, shed drag.
                        self.player.vel.y = self.player.vel.y.max(1.2);
                        self.player.vel.x *= 1.9;
                        self.player.vel.z *= 1.9;
                    }
                    let at = self
                        .player
                        .pos
                        .translated(Vec3::new(0.0, -0.35, 0.0))
                        .expect("ridden vehicle stays below its rider")
                        .pos;
                    let yaw = self.camera.yaw;
                    if let Some(m) = self.server.world.mob_by_id_mut(bid) {
                        m.pos = at;
                        m.vel = Vec3::ZERO;
                        m.yaw = -yaw + std::f32::consts::FRAC_PI_2;
                    }
                }
            }
            if !was_in_water && self.player.in_water && fall_speed < -4.0 {
                self.sfx(Sfx::Splash);
            }
            self.update_survival(dt);
        }
        if can_sim {
            self.update_items(dt);
        }
        self.camera.follow_planet(self.player.eye());
        match self.camera.mode {
            crate::camera::CameraMode::First => {}
            crate::camera::CameraMode::Third => {
                self.camera
                    .place_chase(self.player.eye(), &self.server.world);
            }
            crate::camera::CameraMode::Orbit => {
                self.camera.place_orbit(self.player.eye());
            }
        }

        if self.ui_state.screen == Screen::Playing && self.input.captured() {
            self.interact(dt);
        }
    }

    fn build_and_render_frame(&mut self, dt: f32, now: Instant) {
        let local_up = self.camera.up();
        let sun_dir_true = if self.in_world {
            self.server.world.sun_direction().as_vec3()
        } else {
            self.camera
                .world_vector(Vec3::new(0.0, 1.0, 0.45))
                .normalize()
        };
        let elev = sun_dir_true.dot(local_up);
        // Near-black floor: night is now carried by the moon (below), not a flat
        // ambient, so a new-moon night goes genuinely dark while a full moon
        // stays navigable. Torch light is unaffected (its own vertex channel).
        // This is the render brightness only; the sim's own daylight() (mob
        // spawns etc.) keeps its 0.12 floor untouched.
        let daylight = if self.in_world {
            (elev * 2.5 + 0.5).clamp(0.02, 1.0)
        } else {
            1.0
        };
        let day_sky = [0.55, 0.75, 0.95];
        let night_sky = [0.02, 0.03, 0.08];
        let f = daylight;
        self.renderer.sky_color = [
            night_sky[0] + (day_sky[0] - night_sky[0]) * f,
            night_sky[1] + (day_sky[1] - night_sky[1]) * f,
            night_sky[2] + (day_sky[2] - night_sky[2]) * f,
        ];

        // Sun for directional lighting. The sun arcs east->west over the day
        // (noon at time 0.25); we keep it a touch above the horizon while up so
        // shadows never degenerate. Warm direct light, cool sky-ambient fill,
        // both faded by `daylight` so night is lit only by the moonlit floor
        // and torches.
        // Warm sun, clamped just over the horizon so its shadow never
        // degenerates while it's the active light.
        let sun_tangent = (sun_dir_true - local_up * elev).normalize_or_zero();
        let warm_sun_dir = (sun_tangent + local_up * elev.max(0.05)).normalize();
        let sun_vis = elev.clamp(0.0, 1.0).sqrt(); // 0 below horizon
        // Golden hour: the sun's hue warms from near-white at noon to deep
        // orange as it nears the horizon.
        let noon = Vec3::new(1.0, 0.96, 0.86);
        let horizon = Vec3::new(1.0, 0.54, 0.26);
        // Direct sun against skylight. Real sun is one to two orders of
        // magnitude above the sky; ours sat at roughly parity, which is why
        // shadows filled in flat and a window out-lit its own sunbeam. Raising
        // it is only meaningful now that the composite tone-maps instead of
        // clamping — the range has somewhere to go.
        let warm_sun_col = horizon.lerp(noon, sun_vis) * (0.64 * sun_vis * sun_scale());
        let mut amb_col = Vec3::new(0.60, 0.68, 0.82) * (0.42 * daylight);

        // Moon: rides the anti-solar arc (up while the sun is down), cold and
        // dim, its strength set by the deterministic lunar phase — a new moon is
        // near-dark, a full moon lights the night. Clamped like the sun so its
        // shadow holds up.
        let illum = if self.in_world {
            self.server.world.moon_illumination()
        } else {
            0.0
        };
        let moon_elev = -elev;
        let moon_tangent = -sun_tangent;
        let moon_dir = (moon_tangent + local_up * moon_elev.max(0.05)).normalize();
        let moon_vis = moon_elev.clamp(0.0, 1.0).sqrt() * illum;
        // A strong, distinctly cold key so full-moon-lit faces clearly read as
        // lit — paired with a near-nothing fill (below) so shadows stay genuinely
        // dark. High contrast, a real directional light, not a flat ambient lift.
        let moon_col = Vec3::new(0.45, 0.60, 1.0) * (0.42 * moon_vis);
        // Barely any cold fill, folded into the sky ambient below: enough that a
        // full moon's shadowed faces read cold-dark rather than dead black, but
        // not enough to wash out the shadows.
        let moon_fill = Vec3::new(0.015, 0.022, 0.05) * moon_vis;

        // Surface lighting uses whichever body is dominant as the single
        // directional light, so the shadow map follows it for free: warm sun
        // while it's up, cold moon once it sets. Both intensities have faded to
        // ~zero at the crossover, so the swap is invisible. (The sky gradient
        // tracks the true sun via `sun_dir_true`, independently.)
        let sun_dir = if elev > 0.0 { warm_sun_dir } else { moon_dir };
        let mut sun_col = if elev > 0.0 { warm_sun_col } else { moon_col };

        // Weather gloom: fronts dim the direct sun hard and the ambient
        // gently, gray the sky, and pull the fog in. Lerped over ~10 s
        // so transitions read as skies changing, not a light switch.
        let local_weather = self.in_world.then(|| {
            self.server
                .world
                .weather_at_surface(self.player.pos.surface())
        });
        let gloom_target = if let Some(weather) = local_weather {
            match weather.kind {
                crate::planet_atlas::LocalWeather::Clear => 0.0,
                crate::planet_atlas::LocalWeather::Overcast => 0.4,
                crate::planet_atlas::LocalWeather::Precipitation => 0.55,
                crate::planet_atlas::LocalWeather::Storm => 0.7,
            }
        } else {
            0.0
        };
        self.presentation.weather_vis +=
            (gloom_target - self.presentation.weather_vis) * (dt / 10.0).min(1.0);
        let gloom = self.presentation.weather_vis;
        sun_col *= 1.0 - gloom;
        amb_col *= 1.0 - gloom * 0.45;
        let gray = [0.36 * f, 0.39 * f, 0.44 * f];
        let mix = (gloom * 1.4).min(1.0);
        for (c, g) in self.renderer.sky_color.iter_mut().zip(gray) {
            *c += (g - *c) * mix;
        }

        // Storms flash: two frames of borrowed noon, thunder later.
        let mut daylight = daylight;
        if local_weather
            .is_some_and(|weather| weather.kind == crate::planet_atlas::LocalWeather::Storm)
        {
            self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
            if ((self.rng >> 8) as f32 / (1 << 24) as f32) < dt / 25.0 {
                self.presentation.lightning = 0.12;
                self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
                self.presentation.thunder_delay =
                    0.5 + ((self.rng >> 8) as f32 / (1 << 24) as f32) * 2.5;
            }
        }
        if self.presentation.lightning > 0.0 {
            self.presentation.lightning -= dt;
            daylight = 1.0;
            self.renderer.sky_color = [0.85, 0.88, 0.95];
            sun_col = Vec3::new(0.9, 0.92, 1.0);
        }
        if self.presentation.thunder_delay >= 0.0 {
            self.presentation.thunder_delay -= dt;
            if self.presentation.thunder_delay < 0.0 {
                self.sfx(Sfx::Thunder);
            }
        }

        // Project the finished sky into SH ambient — the colored, directional
        // fill light — from the same values that drive the visible dome.
        let sky_params = crate::sky::SkyParams {
            sun_dir: sun_dir_true,
            up: self.camera.up(),
            gloom,
            overcast: Vec3::from_array(self.renderer.sky_color),
            moon_fill,
        };
        let sh_ambient = crate::sky::project(&sky_params);
        // And ask the room what colour its light has become. One probe, from
        // where the player is standing — see bounce.rs for why it is only one.
        //
        // The probe works in the player's local chart — the flat, Y-up frame
        // the whole estimate is written in — so the render-space eye and sun
        // are expressed there first, through the same tangent basis the camera
        // uses. Its answer is a colour and an amount, with no direction (see
        // bounce.rs), and that rotation leaves both untouched, so nothing
        // downstream needs to know which chart it was measured in.
        let eye = self.player.eye();
        let lf = crate::planet::local_frame(eye.surface_point());
        let (east, up, north) = (lf.east.as_vec3(), lf.up.as_vec3(), lf.north.as_vec3());
        let to_chart = |d: Vec3| Vec3::new(d.dot(east), d.dot(up), d.dot(north));
        let sun_true_chart = to_chart(sun_dir_true);
        let warm_sun_chart = to_chart(warm_sun_dir);
        let sky_chart = crate::sky::SkyParams {
            sun_dir: sun_true_chart,
            up: Vec3::Y,
            gloom,
            overcast: Vec3::from_array(self.renderer.sky_color),
            moon_fill,
        };
        self.room_light.update(
            &self.server.world,
            &self.block_albedo,
            eye.local(),
            warm_sun_chart,
            sun_true_chart,
            warm_sun_col,
            &sky_chart,
            // What the sun's brightness would be straight overhead, so
            // "fully lit" means the same thing at any sun-strength setting.
            0.64 * sun_scale(),
            dt,
            eye.face(),
        );
        if std::env::var("WILDFORGE_DEBUG").is_ok() && self.total_frames.is_multiple_of(60) {
            let sh = self.room_light.sh();
            eprintln!(
                "room L0 {:.4},{:.4},{:.4}  L1y {:.4},{:.4},{:.4}  nonzero {}/128  cols {:?}  intensity {:.3}",
                sh[0].x,
                sh[0].y,
                sh[0].z,
                sh[1].x,
                sh[1].y,
                sh[1].z,
                self.room_light.lit,
                self.room_light.stats,
                self.room_light.intensity
            );
        }

        // Weather wins while it is audible; in fair conditions nearby living
        // Current supplies its own restrained harmonic bed.
        let ecology_ambience = self
            .server
            .world
            .perceived_arcane_ecology_at(self.player.pos.surface(), self.scan_range());
        if let Some(a) = &self.audio {
            let want = if self.ui_state.screen == Screen::Paused {
                // The pause menu holds the world's breath: no rain,
                // no wind, no crickets until you come back.
                None
            } else if local_weather.is_some_and(|weather| {
                weather.precipitation == crate::planet_atlas::PrecipitationForm::Rain
            }) {
                Some(
                    if local_weather.is_some_and(|weather| {
                        weather.kind == crate::planet_atlas::LocalWeather::Storm
                    }) {
                        audio::Ambience::Storm
                    } else {
                        audio::Ambience::Rain
                    },
                )
            } else if self.in_world
                && self.presentation.juice
                && local_weather.is_some_and(|weather| {
                    weather.kind == crate::planet_atlas::LocalWeather::Overcast
                })
            {
                // Wind is the forecast: every rain passes through it.
                Some(audio::Ambience::Wind)
            } else if self.in_world
                && self.presentation.juice
                && let Some(observation) = &ecology_ambience
            {
                Some(audio::Ambience::Current(observation.damped))
            } else if self.in_world && self.presentation.juice && daylight < 0.25 {
                // The night bed: crickets while the wild is calm; a low
                // hush once it turns wrathful. The ire meter, diegetic.
                Some(audio::Ambience::Night(
                    // The night bed reads the land underfoot: crickets
                    // in tended country, the wrathful hush where the
                    // ground remembers (legible escalation, stage 2).
                    self.server
                        .world
                        .ire_tier_at_surface(self.player.pos.surface())
                        < 2,
                ))
            } else {
                None
            };
            a.set_ambience(want);
        }
        // Ambient is the engine's stark<->accessible knob. Dev override:
        // WILDFORGE_AMBIENT="r,g,b" pins a flat ambient (crush it to make
        // point-light contrast legible in tests). Applied last so it wins over
        // weather gloom.
        if let Ok(s) = std::env::var("WILDFORGE_AMBIENT") {
            let v: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
            if v.len() == 3 {
                amb_col = Vec3::new(v[0], v[1], v[2]);
            }
        }

        let playing = self.ui_state.screen == Screen::Playing;
        let outline = if playing && self.config.outline {
            raycast::raycast_at(
                &self.server.world,
                self.player.eye(),
                self.camera.local_forward(),
                self.reach(),
            )
            .map(|h| h.block)
        } else {
            None
        };
        let held_wand = self.inventory.slots[self.input.hotbar_sel].is_some_and(|stack| {
            stack.arcane_id != 0
                && self
                    .content
                    .reg
                    .item(stack.item)
                    .implement
                    .as_ref()
                    .is_some_and(|definition| {
                        definition.kind == crate::implements::ImplementItemKind::Wand
                    })
        });
        let active_warning = if self.multiplayer.remote.is_some() {
            self.presentation
                .working_cues
                .values()
                .map(|(cue, _)| cue.warning_band)
                .max()
                .unwrap_or_default()
        } else {
            self.server
                .world
                .working_cues()
                .into_iter()
                .map(|cue| cue.warning_band)
                .max()
                .unwrap_or_default()
        };
        let outline_color = if active_warning >= 2 {
            let phase = (self.time_abs * 12.0).sin() * 0.16;
            [0.92, 0.20 + phase.max(0.0), 0.12]
        } else if self.interaction.lens_settle > 0.0 {
            let phase = (self.time_abs * 10.0).sin() * 0.12;
            [0.62 + phase, 0.48 + phase, 0.88]
        } else if held_wand {
            outline.map_or([0.42, 0.34, 0.68], |pos| {
                let block = self.server.world.get_block_at(pos);
                let definition = self.content.reg.block(block);
                if self.content.reg.is_water(block) {
                    [0.18, 0.64, 0.92]
                } else if definition.crop_next.is_some() || definition.sapling.is_some() {
                    [0.26, 0.78, 0.38]
                } else if definition.burns != 0 {
                    [0.94, 0.46, 0.14]
                } else {
                    [0.42, 0.34, 0.68]
                }
            })
        } else {
            [0.05, 0.05, 0.05]
        };
        let underwater = self.player.head_underwater(&self.server.world);
        let fog = (self.config.view_dist as f32 - 0.5) * CHUNK_X as f32 * (1.0 - 0.35 * gloom);

        // World-space extras: item entities + mining crack overlay.
        let mut entity_verts = Vec::new();
        let mut entity_idx = Vec::new();
        let sample = |w: &World, p: crate::planet::EntityPos| -> ([f32; 3], f32) {
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
        for it in self.server.world.loose_items() {
            let lum = sample(&self.server.world, it.pos);
            it.emit(&self.content.reg, lum, &mut entity_verts, &mut entity_idx);
        }
        for m in self.server.world.mobs() {
            let lum = sample(&self.server.world, m.pos);
            m.emit(&self.content.reg, lum, &mut entity_verts, &mut entity_idx);
        }
        for p in self.server.world.projectiles() {
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
                let py = self.server.world.surface_height_at(surface) as f32 + 1.0;
                let at = crate::planet::EntityPos::new(
                    surface.face(),
                    translated.u(),
                    py,
                    translated.v(),
                )
                .expect("demo player surface is canonical");
                let lum = sample(&self.server.world, at);
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
                let lum = sample(&self.server.world, logical);
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
                                .and_then(|stack| self.server.world.implement_visual(stack));
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
                let lum = sample(&self.server.world, logical);
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
            let implement = held.and_then(|stack| self.server.world.implement_visual(stack));
            let lum = sample(&self.server.world, self.player.eye());
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
        for f in self.server.world.falling_blocks().to_vec() {
            let lum = sample(&self.server.world, f.pos);
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
            for (&pos, e) in self.server.world.block_entities() {
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
                sprite(slot, pos, size, lum, &mut entity_verts, &mut entity_idx);
            }
        }
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
                if !self.server.world.rains_at_surface(column.surface()) {
                    continue;
                }
                let snow = self.server.world.snows_at_surface(column.surface());
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
                if self.server.world.light_at_pos(streak_block).1 < 15 {
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
                        let base = entity_verts.len() as u32;
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
                            entity_verts.push(mesher::Vertex {
                                pos: world.to_array(),
                                uv: [uu, vv],
                                normal: [0.0, 0.0, 0.0],
                                light: [0.5; 3],
                                sky: 0.9,
                                ao: 1.0,
                            });
                        }
                        entity_idx.extend_from_slice(&[
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
        let mut overlay_verts = Vec::new();
        let mut overlay_idx = Vec::new();
        if let Some((target, progress)) = self.interaction.breaking {
            let world_pos = match target {
                super::BreakTarget::World(p) => Some(p),
                super::BreakTarget::Structure(id, offset) => self
                    .server
                    .world
                    .local_structure(id)
                    .and_then(|s| s.world_position(offset)),
            };
            if let Some(p) = world_pos {
                entity::emit_crack(p, progress, &mut overlay_verts, &mut overlay_idx);
            }
        }
        // The quern's top face turns while you grind (bare-hand station
        // channels only; hammer stations flash sparks instead).
        if self.presentation.juice
            && self.input.right_held
            && let Some(t) = self.interaction.anvil_pos
            && !self.inventory.slots[self.input.hotbar_sel]
                .is_some_and(|st| self.content.reg.item(st.item).hammer)
        {
            let b = self.server.world.get_block_at(t);
            let slot = self.content.reg.block(b).tiles[2];
            let ts = 1.0 / atlas::ATLAS_TILES as f32;
            let (tx, ty) = (
                slot as u32 % atlas::ATLAS_TILES,
                slot as u32 / atlas::ATLAS_TILES,
            );
            let ang = self.interaction.anvil_work * std::f32::consts::PI;
            let (sa, ca) = ang.sin_cos();
            let center = crate::planet::EntityPos::new(
                t.face(),
                f32::from(t.u()) + 0.5,
                f32::from(t.y()) + 1.01,
                f32::from(t.v()) + 0.5,
            )
            .expect("station overlay is inside the shell");
            let c = center.render_pos();
            let local = crate::planet::local_frame(center.surface_point());
            let east = local.east.as_vec3();
            let north = local.north.as_vec3();
            let base = overlay_verts.len() as u32;
            for (lx, lz, u, v) in [
                (-0.5f32, -0.5f32, 0.0f32, 0.0f32),
                (0.5, -0.5, 1.0, 0.0),
                (0.5, 0.5, 1.0, 1.0),
                (-0.5, 0.5, 0.0, 1.0),
            ] {
                let rx = lx * ca - lz * sa;
                let rz = lx * sa + lz * ca;
                overlay_verts.push(mesher::Vertex {
                    pos: (c + east * rx + north * rz).to_array(),
                    uv: [(tx as f32 + u) * ts, (ty as f32 + v) * ts],
                    normal: [0.0, 0.0, 0.0],
                    light: [1.0; 3],
                    sky: 1.0,
                    ao: 1.0,
                });
            }
            overlay_idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        let mut hand_verts = Vec::new();
        let mut hand_idx = Vec::new();
        self.emit_hand(&mut hand_verts, &mut hand_idx);
        if self.ui_state.screen == Screen::Appearance {
            // Live preview: you, slowly turning, left of the swatches.
            let f = self.camera.local_forward();
            let rgt = self.camera.local_right();
            let feet = self
                .player
                .eye()
                .translated(f * 2.6 - rgt * 0.9 - Vec3::Y * 1.35)
                .expect("appearance preview stays in the local planetary frame")
                .pos;
            mobs::emit_humanoid(
                feet,
                self.time_abs * 0.8,
                &Self::humanoid_art(self.style),
                (self.time_abs * 2.2, 0.35),
                self.held_art_stack(self.inventory.slots[self.input.hotbar_sel]),
                ([0.95, 0.93, 0.90], 0.0),
                &mut hand_verts,
                &mut hand_idx,
            );
        } else if self.ui_state.screen == Screen::Inventory && !self.ui_state.inventory_status_open
        {
            // Inventory paper doll: the active local identity gets a body,
            // not just a line of account text on a settings screen.
            //
            // The doll is world geometry projected into a UI rect, so its
            // on-screen size is set by the window height and the fov while
            // the rect is fixed pixels. A hardcoded depth therefore only
            // framed it correctly at one resolution — at 720p and up the
            // head was clipped by the name plate and the feet by the
            // storage rows, leaving a torso that read as "no sprite".
            // Solve for the depth that makes the body the height we want.
            let w = self.renderer.config.width as f32;
            let h = self.renderer.config.height as f32;
            let (ax, ay, aw, ah) = self.inventory_layout().avatar_rect();
            // The name plate owns the top of the frame; keep clear of it
            // and leave the feet a margin off the bottom edge.
            const PLATE: f32 = 42.0;
            const MARGIN: f32 = 10.0;
            let target_px = (ah - PLATE - MARGIN).max(24.0);
            let depth = portrait_depth(self.camera.fovy, self.camera.pitch, h, target_px);
            let (center_x, center_y) = (ax + aw * 0.5, ay + PLATE + target_px * 0.5);
            let ndc_x = center_x / w * 2.0 - 1.0;
            let ndc_y = 1.0 - center_y / h * 2.0;
            let half_h = (self.camera.fovy * 0.5).tan() * depth;
            let f = self.camera.local_forward();
            let rgt = self.camera.local_right();
            let up = rgt.cross(f).normalize_or_zero();
            let body_delta =
                f * depth + rgt * (ndc_x * half_h * self.camera.aspect) + up * (ndc_y * half_h);
            let feet = self
                .player
                .eye()
                .translated(body_delta - Vec3::Y * (mobs::HUMANOID_HEIGHT * 0.5))
                .expect("inventory preview stays in the local planetary frame")
                .pos;
            let face_camera = -std::f32::consts::FRAC_PI_2 - self.camera.yaw;
            mobs::emit_humanoid(
                feet,
                face_camera,
                &Self::humanoid_art(self.style),
                (0.0, 0.0),
                self.held_art_stack(self.inventory.slots[self.input.hotbar_sel]),
                ([0.95, 0.93, 0.90], 0.0),
                &mut hand_verts,
                &mut hand_idx,
            );
        }

        self.build_ui();
        // Screen-open ease: scale from 0.96 and fade in over ~140ms.
        // Animation this short reads as *faster* than a snap, not slower.
        if self.presentation.juice
            && self.ui_state.screen != Screen::Playing
            && self.presentation.screen_age < 1.0
        {
            let t = self.presentation.screen_age;
            let e = 1.0 - (1.0 - t) * (1.0 - t);
            let sc = 0.96 + 0.04 * e;
            let al = 0.85 + 0.15 * e;
            let cx = self.renderer.config.width as f32 / 2.0;
            let cy = self.renderer.config.height as f32 / 2.0;
            for v in &mut self.ui.verts {
                v.pos[0] = cx + (v.pos[0] - cx) * sc;
                v.pos[1] = cy + (v.pos[1] - cy) * sc;
                v.color[3] *= al;
            }
        }

        if self.auto_shot.is_some() {
            self.apply_look_env();
        }
        // Point lights: promote nearby emitters + the dynamic set.
        let mut dyn_lights = self.presentation.demo_lights.clone();
        self.presentation
            .working_cues
            .retain(|_, (_, seen)| self.time_abs - *seen <= 2.5);
        let active_workings = if self.multiplayer.remote.is_some() {
            self.presentation
                .working_cues
                .values()
                .map(|(cue, _)| cue.clone())
                .collect::<Vec<_>>()
        } else {
            self.server.world.working_cues()
        };
        for cue in active_workings
            .iter()
            .filter(|cue| cue.handler == crate::workings::WorkingHandler::Gleam)
        {
            let Some(target) = cue.path.last().copied() else {
                continue;
            };
            let warning = f32::from(cue.warning_band.min(3)) / 3.0;
            dyn_lights.push(lights::DynLight {
                key: lights::Key::Working(cue.stable_id),
                pos: target.entity_center().render_pos(),
                color: Vec3::new(
                    0.56 + warning * 0.25,
                    0.74 - warning * 0.18,
                    1.0 - warning * 0.25,
                ),
                range: 5.0,
            });
        }
        // The held torch: your own body of light, real shadows and all.
        // Anchored to the body center, never the facing — a camera-
        // relative offset made the light orbit the head when turning,
        // so shadows stuck then snapped with every look-around (and
        // thrashed the cube cache). Remote helds anchor the same way.
        if self.in_world
            && let Some(stack) = self.inventory.slots[self.input.hotbar_sel]
        {
            let glow = self
                .server
                .world
                .implement_visual(stack)
                .and_then(|visual| self.implement_glow(visual))
                .or_else(|| self.held_glow(stack.item));
            if let Some((color, range)) = glow {
                let held_pos = if self.camera.mode == crate::camera::CameraMode::First {
                    // First person: the light rides the camera, near the hand.
                    self.camera.pos - self.camera.up() * 0.15
                } else {
                    // Chase / orbit: the body carries the held item at its
                    // right hand, chest-high and a little ahead of the feet.
                    let f = self.camera.forward();
                    let r = f.cross(self.camera.up()).normalize_or_zero();
                    self.player.pos.render_pos() + self.camera.up() * 1.2 + r * 0.35 + f * 0.35
                };
                dyn_lights.push(lights::DynLight {
                    key: lights::Key::Held,
                    pos: held_pos,
                    color,
                    range,
                });
            }
        }
        // Placed charge vessels are not unconditional glowing blocks. Their
        // restrained light follows the authoritative qualitative charge band;
        // guests receive only these nearby bands, never exact custody. Damage
        // warms the hue and a nearby strained vessel gives a sparse warning
        // envelope even when nobody has a frame screen open.
        let apparatus_cues = if self.in_world {
            self.server.world.apparatus_cues_near(self.player.pos, 48.0)
        } else {
            Vec::new()
        };
        for cue in &apparatus_cues {
            if cue.charge_band == 0 {
                continue;
            }
            let strength = 0.35 + f32::from(cue.charge_band.min(3)) * 0.24;
            let strain = f32::from(cue.strain_band.min(3)) / 3.0;
            let color = Vec3::new(
                0.35 + strain * 0.35,
                0.62 - strain * 0.12,
                0.95 - strain * 0.22,
            ) * strength;
            dyn_lights.push(lights::DynLight {
                key: lights::Key::Block(cue.pos),
                pos: cue.pos.entity_center().render_pos(),
                color,
                range: 2.5 + f32::from(cue.charge_band.min(3)) * 1.7,
            });
        }
        if self.total_frames.is_multiple_of(300)
            && apparatus_cues.iter().any(|cue| {
                cue.strain_band >= 2 && self.player.pos.distance_to(cue.pos.entity_center()) <= 12.0
            })
        {
            self.sfx(Sfx::ImplementStrain);
        }
        // The remaining dynamic slots go to whatever is closest: other
        // players' torches or glowing wardens.
        if self.in_world {
            let cam = self.camera.pos;
            let mut tail: Vec<(f32, lights::DynLight)> = Vec::new();
            if let Some(r) = &self.multiplayer.remote {
                for id in r.players.keys() {
                    let Some(&held) = r.player_held.get(id) else {
                        continue;
                    };
                    let local = r.session.content().item(held);
                    let glow = r
                        .player_implement
                        .get(id)
                        .copied()
                        .and_then(|visual| self.implement_glow(visual))
                        .or_else(|| local.and_then(|item| self.held_glow(item)));
                    if let Some((color, range)) = glow
                        && let Some(logical) = r.player_positions.get(id).copied()
                    {
                        let pos = logical
                            .translated(Vec3::new(0.0, 1.4, 0.0))
                            .expect("held light stays in the voxel shell")
                            .pos
                            .render_pos();
                        tail.push((
                            pos.distance(cam),
                            lights::DynLight {
                                key: lights::Key::RemoteHeld(*id),
                                pos,
                                color,
                                range,
                            },
                        ));
                    }
                }
            }
            if let Some(sess) = &self.multiplayer.host {
                for (id, g) in &sess.guests {
                    if !g.is_active() {
                        continue;
                    }
                    let stack = g.inventory.slots[g.hotbar];
                    let glow = stack
                        .and_then(|stack| self.server.world.implement_visual(stack))
                        .and_then(|visual| self.implement_glow(visual))
                        .or_else(|| {
                            (g.held != u16::MAX)
                                .then_some(ItemId(g.held))
                                .and_then(|item| self.held_glow(item))
                        });
                    if let Some((color, range)) = glow {
                        let p = g
                            .render_entity_pos()
                            .translated(Vec3::new(0.0, 1.4, 0.0))
                            .expect("guest held light stays in the voxel shell")
                            .pos
                            .render_pos();
                        tail.push((
                            p.distance(cam),
                            lights::DynLight {
                                key: lights::Key::RemoteHeld(*id),
                                pos: p,
                                color,
                                range,
                            },
                        ));
                    }
                }
            }
            for m in self.server.world.mobs().iter().filter(|m| m.id != 0) {
                let Some(g) = self.content.reg.animals[m.species].glow else {
                    continue;
                };
                let d = (m.pos.render_pos() - cam).length();
                if d < 32.0 {
                    let pos = m
                        .pos
                        .translated(Vec3::new(0.0, 0.7, 0.0))
                        .expect("mob light stays in the voxel shell")
                        .pos
                        .render_pos();
                    tail.push((
                        d,
                        lights::DynLight {
                            key: lights::Key::Mob(m.id),
                            pos,
                            color: Vec3::from(g),
                            range: 12.0,
                        },
                    ));
                }
            }
            tail.sort_by(|a, b| a.0.total_cmp(&b.0));
            let spare = lights::MAX_DYNAMIC.saturating_sub(dyn_lights.len());
            dyn_lights.extend(tail.into_iter().take(spare).map(|(_, l)| l));
        }
        let point_lights = if self.in_world && self.config.lights > 0 {
            self.presentation.lights.frame(
                self.camera.pos,
                &dyn_lights,
                dt,
                self.config.lights >= 2,
            )
        } else {
            Vec::new()
        };
        // The flat fill under everything. It and the room tint are answering
        // the same question — what lights a surface no lamp reaches — and at
        // 0.12 the flat one is several times the honest one, so the room's own
        // colour cannot be seen past it. WILDFORGE_AMBIENT_FLOOR to explore.
        let mut ambient_floor = std::env::var("WILDFORGE_AMBIENT_FLOOR")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| (0.0..=1.0).contains(v))
            .unwrap_or(if self.config.stark { 0.04 } else { 0.12 });
        // Clear-eye is adaptation, not x-ray vision: it lifts only the final
        // low-light floor and gives already-authorized local Current signs a
        // very faint resonance tint. Guests use the same coarse strength and
        // dominant-category packet they receive without the preparation; no
        // ore, entity, inventory, exact mixture, or server-hidden state enters
        // the frame.
        let trace_strength = f32::from(self.survival.preparation_modifiers.trace_sight).min(250.0)
            / 250.0
            * f32::from(self.survival.preparation_modifiers.perception_permille)
            / 1_000.0;
        let darkness = (1.0 - daylight).clamp(0.0, 1.0);
        let trace_adaptation = trace_strength * 0.055 * darkness;
        ambient_floor = (ambient_floor + trace_adaptation).min(0.18);
        if trace_strength > 0.0 {
            let (bands, dominant) = if self.multiplayer.remote.is_some() {
                (
                    self.server.world.remote_arcane_cue(),
                    self.server.world.remote_arcane_dominant(),
                )
            } else if let Some(atlas) = self.server.world.planet_atlas() {
                self.server
                    .world
                    .arcane_sensory_cue_at(atlas.atlas_pos(self.player.pos.surface()))
            } else {
                ([0; 2], 0)
            };
            let local_sign = f32::from(bands[0].max(bands[1]).min(4)) / 4.0;
            let resonance = match dominant {
                1 => Vec3::new(0.55, 0.92, 0.52), // root
                2 => Vec3::new(0.36, 0.76, 0.94), // tide
                3 => Vec3::new(1.00, 0.56, 0.30), // ember
                4 => Vec3::new(0.78, 0.78, 0.72), // stone
                5 => Vec3::new(0.62, 0.80, 1.00), // gale
                6 => Vec3::new(0.48, 0.72, 1.00), // echo
                _ => Vec3::splat(0.72),
            };
            amb_col += resonance * (0.025 * trace_strength * local_sign * darkness);
        }

        let saved_cam = self.camera.pos;
        if self.presentation.nudge.1 > 0.0 {
            self.camera.pos +=
                self.presentation.nudge.0 * (self.presentation.nudge.1 / 0.08) * 0.04;
        }
        let frame_vp = self.camera.view_proj();
        let frame_cam = self.camera.pos;
        self.camera.pos = saved_cam;

        // Voxel occupancy grid for DDA point-light shadows. The origin snaps to
        // OCC_STEP and keeps the camera near the cube's centre; we only rebuild
        // (a full region scan) when the camera crosses into a new snapped cell.
        // The old occupancy texture is a Cartesian lattice and cannot
        // represent cube-sphere cell adjacency. Planetary point lights use the
        // distance-cube shadow path until that optional acceleration is rebuilt
        // on top of topology-aware DDA.
        let dda_shadow = false;
        let occ_origin = [0; 3];
        let occ_grid: Option<Vec<u8>> = None;

        let render_t0 = std::time::Instant::now();
        match self.renderer.render(FrameInput {
            view_proj: frame_vp,
            cam_pos: frame_cam,
            local_up: self.camera.up(),
            fog_dist: fog,
            underwater,
            daylight,
            sun_dir,
            sun_dir_true,
            gloom,
            sh_ambient,
            room_sh: self.room_light.sh(),
            room_intensity: self.room_light.intensity,
            sun_col,
            amb_col,
            ambient_floor,
            dda_shadow,
            occ_origin,
            occ_update: occ_grid.as_deref(),
            point_lights: &point_lights,
            outline,
            outline_color,
            entity_verts: &entity_verts,
            entity_idx: &entity_idx,
            overlay_verts: &overlay_verts,
            overlay_idx: &overlay_idx,
            hand_verts: &hand_verts,
            hand_idx: &hand_idx,
            ui_verts: &self.ui.verts,
            crosshair: playing,
            // How much of the isolated overbright energy bleeds back as glow.
            bloom: if self.config.bloom { 1.5 } else { 0.0 },
        }) {
            Ok(()) => {
                let ms = render_t0.elapsed().as_secs_f32() * 1000.0;
                self.frame_ms.1 = self.frame_ms.1 * 0.95 + ms * 0.05;
            }
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                let size = self.window.inner_size();
                self.renderer.resize(size.width, size.height);
            }
            Err(e) => eprintln!("render error: {e:?}"),
        }

        // Headless verification: WILDFORGE_SHOT=path.ppm captures a frame once
        // the world is meshed, then exits.
        self.total_frames += 1;
        if let Some(path) = self.auto_shot.clone() {
            let forced: Option<u64> = std::env::var("WILDFORGE_SHOT_FRAME")
                .ok()
                .and_then(|v| v.parse().ok());
            let minimum: u64 = std::env::var("WILDFORGE_SHOT_MIN_FRAME")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            // Direct world/join captures must not mistake a responsive
            // pre-entry loading screen for a settled world. World opening is
            // intentionally asynchronous now, so `chunk_work_pending()` can
            // be zero before the background entry task hands over its first
            // playable session.
            let awaiting_direct_entry = !self.in_world
                && (std::env::var_os("WILDFORGE_WORLD").is_some()
                    || std::env::var_os("WILDFORGE_JOIN").is_some());
            self.capture_frames = advance_capture_clock(self.capture_frames, awaiting_direct_entry);
            if !awaiting_direct_entry && self.chunk_work_pending() == 0 {
                self.settled_frames += 1;
            } else {
                self.settled_frames = 0;
            }
            if self.settled_frames == SHOT_SETTLE_FRAMES && visual_capture::evidence_enabled() {
                eprintln!("visual evidence: initial chunk uploads settled");
            }
            // A minimum frame keeps the ordinary settled-world requirement.
            // It is useful when the visual under test needs simulation warmup;
            // unlike SHOT_FRAME, it never captures half-meshed terrain.
            let ready = !awaiting_direct_entry
                && self.capture_frames >= minimum
                && match forced {
                    Some(frame) => self.capture_frames >= frame,
                    None => {
                        self.settled_frames >= SHOT_SETTLE_FRAMES
                            || self.capture_frames >= SHOT_MAX_FRAMES
                    }
                };
            match self.shot_at {
                Some(at) if self.total_frames > at + 1 => std::process::exit(0),
                None if ready => {
                    let player = self.player.pos;
                    let surface = crate::planet::SurfacePos::new(
                        player.face(),
                        player.u().floor() as u16,
                        player.v().floor() as u16,
                    )
                    .expect("canonical player has a valid capture column");
                    let column_top = self.server.world.surface_height_at(surface);
                    let (opaque_chunks, water_chunks, empty_chunks) =
                        self.renderer.chunk_mesh_counts();
                    eprintln!(
                        "capture at frame {} (eligible {}, {}), fps {}, sim {:.2}ms draw {:.2}ms; player {:?} {:.1},{:.1},{:.1}, column top {}; chunks resident {}, gpu {}, opaque {}, water {}, empty {}, dirty {}",
                        self.total_frames,
                        self.capture_frames,
                        if forced.is_some() {
                            "forced frame".to_string()
                        } else if self.settled_frames >= SHOT_SETTLE_FRAMES {
                            format!("world settled {} frames", self.settled_frames)
                        } else {
                            "TIMED OUT, world still changing".to_string()
                        },
                        self.fps,
                        self.frame_ms.0,
                        self.frame_ms.1,
                        player.face(),
                        player.u(),
                        player.y(),
                        player.v(),
                        column_top,
                        self.server.world.chunk_count(),
                        self.renderer.chunk_count(),
                        opaque_chunks,
                        water_chunks,
                        empty_chunks,
                        self.server.world.dirty_chunks().len(),
                    );
                    if visual_capture::evidence_enabled() {
                        let metadata = self
                            .visual_capture_metadata(fog)
                            .unwrap_or_else(|error| panic!("visual evidence refused: {error}"));
                        self.renderer.pending_capture_metadata = Some(metadata);
                    }
                    self.renderer.pending_screenshot = Some(path);
                    self.shot_at = Some(self.total_frames);
                }
                _ => {}
            }
        }

        // Window-title HUD.
        self.frames += 1;
        if (now - self.last_title).as_secs_f32() > 0.5 {
            self.fps = (self.frames as f32 / (now - self.last_title).as_secs_f32()) as u32;
            self.frames = 0;
            self.last_title = now;
            let p = self.player.pos;
            let biome = if self.in_world {
                format!(
                    " | {}",
                    p.block()
                        .map(|at| self.server.world.biome_here_at(at.surface()).name())
                        .unwrap_or("Beyond the world")
                )
            } else {
                String::new()
            };
            let key_probe = match (self.input.keys.a, self.input.keys.d) {
                (true, false) => " | KEY A",
                (false, true) => " | KEY D",
                (true, true) => " | KEYS A+D",
                (false, false) => "",
            };
            self.window.set_title(&format!(
                "Wildforge {BUILD_MARKER} — {} fps (sim {:.1}ms, draw {:.1}ms, {}) | XYZ {:.1} / {:.1} / {:.1}{biome}{key_probe}{}",
                self.fps,
                self.frame_ms.0,
                self.frame_ms.1,
                self.renderer.adapter_name,
                p.x,
                p.y,
                p.z,
                if self.input.captured() || self.ui_state.screen != Screen::Playing {
                    ""
                } else {
                    "  [click to capture mouse]"
                },
            ));
        }
    }

    pub(super) fn update(&mut self) {
        let (now, dt, paused) = self.begin_frame();

        self.advance_feedback(dt, paused);

        self.advance_session_authority(dt, paused);

        // Native qualification runs may warm for hundreds of authoritative
        // ticks while a view-12 horizon fills. The one-shot setup override in
        // `run_demos` is not enough: live planetary weather can advance to a
        // different category before the accepted frame. Reapply only for an
        // automated capture, after simulation and immediately before lighting
        // samples the world. Ordinary play keeps the normal gradual weather.
        if self.auto_shot.is_some()
            && let Ok(requested) = std::env::var("WILDFORGE_WEATHER")
        {
            self.server.world.force_local_weather(&requested);
            self.presentation.weather_vis = match requested.as_str() {
                "overcast" => 0.4,
                "precip" | "rain" | "snow" => 0.55,
                "storm" => 0.7,
                _ => 0.0,
            };
        }

        self.refresh_content_and_toasts(dt);

        self.advance_player(dt, paused);

        self.build_and_render_frame(dt, now);
    }

    // ---------- UI layout ----------

    pub(super) const SLOT: f32 = super::inventory_panel::SLOT;
}

#[cfg(test)]
mod characterization {
    use glam::Vec3;

    use super::{
        KeysDown, VIEWMODEL_HAND_MIN, VIEWMODEL_SLEEVE_MAX, local_sim_should_advance, movement_axes,
    };

    #[test]
    fn pausing_stops_solo_sim_but_not_a_windowed_host() {
        assert!(!local_sim_should_advance(true, false));
        assert!(local_sim_should_advance(true, true));
        assert!(local_sim_should_advance(false, false));
    }

    #[test]
    fn bare_viewmodel_skin_meets_the_sleeve() {
        assert_eq!(VIEWMODEL_SLEEVE_MAX.z, VIEWMODEL_HAND_MIN.z);
    }

    #[test]
    fn d_key_moves_the_player_to_screen_right() {
        let eye =
            crate::planet::EntityPos::new(crate::planet::Face::PosZ, 1616.5, 82.0, 3312.5).unwrap();
        let mut camera = crate::camera::Camera::new(Vec3::ZERO, 16.0 / 9.0);
        camera.follow_planet(eye);
        camera.yaw = -std::f32::consts::FRAC_PI_2;

        let keys = KeysDown {
            d: true,
            ..KeysDown::default()
        };
        let (forward, strafe) = movement_axes(&keys);
        let ahead = eye
            .translated(camera.local_flat_forward() * 12.0)
            .unwrap()
            .pos;
        let moved = eye
            .translated(
                camera.local_flat_forward() * (12.0 + forward) + camera.local_right() * strafe,
            )
            .unwrap()
            .pos;
        let ahead_clip = camera.view_proj() * (ahead.render_pos() - camera.pos).extend(1.0);
        let moved_clip = camera.view_proj() * (moved.render_pos() - camera.pos).extend(1.0);
        assert!(
            moved_clip.x / moved_clip.w > ahead_clip.x / ahead_clip.w,
            "D projected left: key-to-strafe sign and camera-right sign cancel"
        );

        let keys = KeysDown {
            a: true,
            ..KeysDown::default()
        };
        let (forward, strafe) = movement_axes(&keys);
        let moved = eye
            .translated(
                camera.local_flat_forward() * (12.0 + forward) + camera.local_right() * strafe,
            )
            .unwrap()
            .pos;
        let moved_clip = camera.view_proj() * (moved.render_pos() - camera.pos).extend(1.0);
        assert!(
            moved_clip.x / moved_clip.w < ahead_clip.x / ahead_clip.w,
            "A projected right: key-to-strafe sign and camera-right sign cancel"
        );
    }
}

#[cfg(test)]
mod portrait_tests {
    use super::portrait_depth;
    use crate::mobs::HUMANOID_HEIGHT;

    /// How many pixels tall the doll actually lands, given the depth
    /// the framing solver picked. This is the projection the renderer
    /// applies, run backwards.
    fn projected_px(fovy: f32, pitch: f32, screen_h: f32, target_px: f32) -> f32 {
        let depth = portrait_depth(fovy, pitch, screen_h, target_px);
        let half_world = (fovy * 0.5).tan() * depth;
        HUMANOID_HEIGHT * pitch.cos().abs() * (screen_h * 0.5) / half_world
    }

    /// The doll is world geometry in a fixed-pixel UI rect, so a
    /// hardcoded depth framed exactly one resolution. At 720p and up
    /// the head was clipped by the name plate and the feet by the
    /// storage rows, leaving a torso that read as "no sprite at all".
    #[test]
    fn the_paper_doll_fills_its_frame_at_any_resolution() {
        let fovy = 75f32.to_radians();
        let target = 132.0;
        for screen_h in [480.0, 720.0, 1080.0, 1440.0, 2160.0] {
            let got = projected_px(fovy, 0.0, screen_h, target);
            assert!(
                (got - target).abs() < 0.5,
                "{screen_h}p framed the body at {got:.1}px, wanted {target}"
            );
        }
    }

    /// The body stands along world Y while the screen's vertical runs
    /// along camera up, so looking up or down foreshortens it. The
    /// solver pulls in by the same cosine.
    #[test]
    fn the_framing_survives_the_camera_being_pitched() {
        let fovy = 75f32.to_radians();
        let target = 132.0;
        for pitch in [-1.2f32, -0.6, 0.0, 0.6, 1.2] {
            let got = projected_px(fovy, pitch, 1080.0, target);
            assert!(
                (got - target).abs() < 0.5,
                "pitch {pitch} framed the body at {got:.1}px, wanted {target}"
            );
        }
        // Straight up or down would divide the doll away entirely; the
        // clamp keeps it a sane size instead of infinitely close.
        assert!(portrait_depth(fovy, std::f32::consts::FRAC_PI_2, 1080.0, target).is_finite());
    }

    /// Different fields of view, same frame.
    #[test]
    fn the_framing_survives_a_different_field_of_view() {
        for fov in [60f32, 75.0, 100.0] {
            let got = projected_px(fov.to_radians(), 0.0, 1080.0, 132.0);
            assert!((got - 132.0).abs() < 0.5, "fov {fov} gave {got:.1}px");
        }
    }
}
