//! Per-frame client update, scene assembly, and renderer submission.

use super::*;

/// Local authority pauses for solo play, but a windowed host keeps serving
/// guests. Remote guests never enter this path at all.
fn local_sim_should_advance(paused: bool, hosting: bool) -> bool {
    !paused || hosting
}

/// Point-light shadows via voxel-grid DDA (config `point_shadows=grid`) rather
/// than the legacy distance cube map. WILDFORGE_DDA_SHADOW=0/1 forces either
/// path for A/B testing, overriding the setting; read once.
fn dda_shadow_enabled(from_config: bool) -> bool {
    use std::sync::OnceLock;
    static E: OnceLock<Option<bool>> = OnceLock::new();
    let override_ = *E.get_or_init(|| {
        std::env::var("WILDFORGE_DDA_SHADOW")
            .ok()
            .map(|s| s.trim() != "0")
    });
    override_.unwrap_or(from_config)
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

impl Game {
    /// First-person viewmodel: your arm, or the block/item it holds,
    /// anchored low-right of the camera, walk-bobbed, and swung on use.
    /// Emitted in world space; the renderer draws it depth-cleared so
    /// it never sinks into a wall you're standing against.
    pub(super) fn emit_hand(&self, verts: &mut Vec<mesher::Vertex>, idx: &mut Vec<u32>) {
        if !self.in_world || self.survival.health <= 0.0 || self.ui_state.screen != Screen::Playing
        {
            return;
        }
        let reg = self.content.reg.clone();
        let held = self.inventory.slots[self.input.hotbar_sel];

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
        // Swing sweeps toward where you're aiming; a drawn bow comes
        // toward center.
        anchor += (f * 0.08 - u * 0.05 - r * 0.08) * arc;
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
        let a = arc * 0.85;
        let b = arc * 0.8;
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

        match held {
            // A block rides as a mini-cube, turned for a 3/4 view.
            Some(st)
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
            Some(st) => {
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
                        });
                    }
                    idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                }
            }
            // Bare hand: your forearm — sleeve in your shirt color,
            // hand in your skin tone (the same body others see).
            None => {
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
            self.presentation.ambient_timer = 1.6 + self.vary() * 2.4;
            let day = self.server.daylight() > 0.5;
            let r1 = self.vary();
            let r2 = self.vary();
            let r3 = self.vary();
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
                    self.juice_puff(p, reg.block(b).tiles[0], 1);
                }
            } else if day && near(&|n: &str| n.contains("water")) {
                // A dragonfly working the surface.
                if let Some(b) = reg.block_id("base:kelp_frond") {
                    self.juice_puff(p, reg.block(b).tiles[0], 1);
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
            self.presentation.presence_timer = 6.0 + (self.vary() - 0.9) * 20.0;
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
            self.juice_burst(at, tile, 10, 2.2);
            self.survival.damage_flash = 0.4;
        }

        // Footprints in snow: not juice — the trail is world state.
        if self.in_world && !paused && self.multiplayer.remote.is_none() && self.player.on_ground {
            if let Some(at) = self.player.pos.block() {
                self.server.world.tread_at(at);
            }
        }

        // Footsteps: mine, my fellow players', and the creatures'.
        if self.in_world && !paused && self.presentation.juice {
            let hv = Vec3::new(self.player.vel.x, 0.0, self.player.vel.z).length();
            if self.player.on_ground && hv > 0.5 {
                self.presentation.step_accum += hv * dt;
                if self.presentation.step_accum >= 2.2 {
                    self.presentation.step_accum = 0.0;
                    let m = self.step_mat_at(self.player.pos);
                    let pitch = self.vary();
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
                let p = pitch * self.vary();
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
            self.stream_chunks();
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
                    aggro_mod: if self.charm("quiet") { -2.0 } else { 0.0 },
                };
                // Hosting: guests are simulated players too, and their
                // requests apply before the tick.
                let players = if let Some(mut sess) = self.multiplayer.host.take() {
                    self.server.world.set_edit_logging(true);
                    let held = self.inventory.slots[self.input.hotbar_sel]
                        .map(|st| st.item.0)
                        .unwrap_or(u16::MAX);
                    let fx = sess.pump(
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
                            mp::HostFx::AllSlept => {
                                self.multiplayer.host_sleeping = false;
                                self.survival.spawn_point = self.player.pos;
                                self.toast("Dawn. The camp wakes.".to_string());
                            }
                        }
                    }
                    let players = sess.player_ctxs(Some(ctx));
                    self.multiplayer.host = Some(sess);
                    players
                } else {
                    vec![ctx]
                };
                let mut evs = Vec::new();
                self.server.advance(dt, &players, &mut evs);
                for ev in evs {
                    match ev {
                        server::SimEvent::PlayerHit { who, dmg, from } => {
                            if who == 0 && self.multiplayer.remote.is_none() {
                                self.hurt_player_from_wild(dmg, from);
                            } else if let Some(sess) = &mut self.multiplayer.host {
                                // `who` is that guest's own net id.
                                sess.hurt_guest(who, dmg, from);
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
                                self.juice_burst(
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
                        server::SimEvent::Dawn { offering_refund } => {
                            if offering_refund > 0.0 {
                                self.sfx(Sfx::Pickup);
                                self.toast("The wild has accepted your offering.".to_string());
                            }
                        }
                        server::SimEvent::WeatherChanged(w) => {
                            // Presentation lerps from world.weather every
                            // frame; only a breaking storm needs a latch:
                            // no flash or rumble after the sky clears.
                            if w != world::Weather::Storm {
                                self.presentation.lightning = 0.0;
                                self.presentation.thunder_delay = -1.0;
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
                    }
                }
                if self.multiplayer.remote.is_none() {
                    self.sweep_dead_mobs();
                }
                for (pos, s) in self.server.world.take_pending_drops() {
                    let center = pos.entity_center();
                    let a = self.rand01() * std::f32::consts::TAU;
                    let v = Vec3::new(a.cos() * 1.5, 2.5, a.sin() * 1.5);
                    self.interaction
                        .items
                        .push(ItemEntity::new(center, v, s.item, s.count));
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
        if self.in_world && self.server.world.season() != self.presentation.atlas_season {
            let mut atlas = atlas::build_atlas(
                &self.content.reg.tex_files,
                &atlas::pack_chain(&self.active_pack_id()),
                &self.content.reg.tex_names,
            );
            atlas::season_tint(&mut atlas.color, atlas.px, self.server.world.season());
            self.presentation.atlas_season = self.server.world.season();
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
            let input = physics::Input {
                forward: (self.input.keys.w as i32 - self.input.keys.s as i32) as f32,
                strafe: (self.input.keys.d as i32 - self.input.keys.a as i32) as f32,
                jump: self.input.keys.space,
                sprint: self.input.keys.sprint && self.survival.hunger >= 6.0,
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
                    r.client.send(&net::C2S::SleepCancel);
                    self.toast("You get up.".to_string());
                }
            }
            self.update_food(dt, &input);
            if self.flying {
                let mut wish = self.camera.local_flat_forward() * input.forward
                    + self.camera.local_right() * input.strafe;
                if wish.length_squared() > 1.0 {
                    wish = wish.normalize();
                }
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
                            rc.client.send(&net::C2S::RideMob {
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

        if self.ui_state.screen == Screen::Playing && self.input.mouse_captured {
            self.interact(dt);
        }
    }

    fn build_and_render_frame(&mut self, dt: f32, now: Instant) {
        // Day/night: daylight factor from a sun curve (full day on menus).
        let sun = (self.server.time_of_day * std::f32::consts::TAU).sin();
        // Near-black floor: night is now carried by the moon (below), not a flat
        // ambient, so a new-moon night goes genuinely dark while a full moon
        // stays navigable. Torch light is unaffected (its own vertex channel).
        // This is the render brightness only; the sim's own daylight() (mob
        // spawns etc.) keeps its 0.12 floor untouched.
        let daylight = if self.in_world {
            (sun * 2.5 + 0.5).clamp(0.02, 1.0)
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
        let ang = self.server.time_of_day * std::f32::consts::TAU;
        let elev = ang.sin(); // 1 at noon, -1 at midnight
        let horiz = ang.cos(); // +1 dawn -> 0 noon -> -1 dusk
        // Warm sun, clamped just over the horizon so its shadow never
        // degenerates while it's the active light.
        let warm_sun_dir = self
            .camera
            .world_vector(Vec3::new(horiz * 0.8, elev.max(0.05) + 0.15, 0.45))
            .normalize();
        // Same azimuth/tilt but the true elevation (dips below the horizon at
        // night), so the sky gradient can actually set and darken.
        let sun_dir_true = self
            .camera
            .world_vector(Vec3::new(horiz * 0.8, elev, 0.45))
            .normalize();
        let sun_vis = elev.clamp(0.0, 1.0).sqrt(); // 0 below horizon
        // Golden hour: the sun's hue warms from near-white at noon to deep
        // orange as it nears the horizon.
        let noon = Vec3::new(1.0, 0.96, 0.86);
        let horizon = Vec3::new(1.0, 0.54, 0.26);
        let warm_sun_col = horizon.lerp(noon, sun_vis) * (0.64 * sun_vis);
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
        let moon_horiz = -horiz;
        let moon_dir = self
            .camera
            .world_vector(Vec3::new(
                moon_horiz * 0.8,
                moon_elev.max(0.05) + 0.15,
                0.45,
            ))
            .normalize();
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
        let gloom_target = if self.in_world {
            match self.server.world.weather {
                world::Weather::Clear => 0.0,
                world::Weather::Overcast => 0.4,
                world::Weather::Precip => 0.55,
                world::Weather::Storm => 0.7,
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
        if self.in_world && self.server.world.weather == world::Weather::Storm {
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
        let sh_ambient = crate::sky::project(&crate::sky::SkyParams {
            sun_dir: sun_dir_true,
            up: self.camera.up(),
            gloom,
            overcast: Vec3::from_array(self.renderer.sky_color),
            moon_fill,
        });

        // The weather bed follows what's actually falling where you stand.
        if let Some(a) = &self.audio {
            let want = if self.ui_state.screen == Screen::Paused {
                // The pause menu holds the world's breath: no rain,
                // no wind, no crickets until you come back.
                None
            } else if self.in_world
                && self.server.world.weather.precipitating()
                && self
                    .player
                    .pos
                    .block()
                    .is_some_and(|pos| self.server.world.rains_at_surface(pos.surface()))
            {
                Some(if self.server.world.weather == world::Weather::Storm {
                    audio::Ambience::Storm
                } else {
                    audio::Ambience::Rain
                })
            } else if self.in_world
                && self.presentation.juice
                && self.server.world.weather == world::Weather::Overcast
            {
                // Wind is the forecast: every rain passes through it.
                Some(audio::Ambience::Wind)
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
                REACH,
            )
            .map(|h| h.block)
        } else {
            None
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
        for it in &self.interaction.items {
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
                let gait = self.gait_for(id, pos, dt);
                let (held, st) = {
                    let r = self.multiplayer.remote.as_ref().unwrap();
                    let held = r
                        .player_held
                        .get(&id)
                        .and_then(|w| r.item_map.get(*w as usize).copied().flatten());
                    let st = r
                        .player_style
                        .get(&id)
                        .map(|v| style::Style::unpack(*v))
                        .unwrap_or_default();
                    (held, st)
                };
                let lum = sample(&self.server.world, logical);
                mobs::emit_humanoid_interpolated(
                    logical,
                    pos,
                    yaw,
                    &Self::humanoid_art(st),
                    gait,
                    self.held_art(held),
                    lum,
                    &mut entity_verts,
                    &mut entity_idx,
                );
            }
        }
        if self.multiplayer.host.is_some() {
            let entries: Vec<(u32, Vec3, crate::planet::EntityPos, f32, u16, u32)> = self
                .multiplayer
                .host
                .as_ref()
                .map(|h| {
                    h.guests
                        .iter()
                        .map(|(id, g)| {
                            let (p, y) = g.render_pos();
                            (*id, p, g.render_entity_pos(), y, g.held, g.style)
                        })
                        .collect()
                })
                .unwrap_or_default();
            for (id, pos, logical, yaw, held_wire, pstyle) in entries {
                let gait = self.gait_for(id, pos, dt);
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
                    self.held_art(held),
                    lum,
                    &mut entity_verts,
                    &mut entity_idx,
                );
            }
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
                    world::BlockEntity::Bloomery(b) if b.lit => {
                        for k in 0..3 {
                            let rise = (t * 0.7 + k as f32 * 0.65) % 2.0;
                            let drift = (t * 0.9 + k as f32 * 2.1).sin() * 0.2;
                            let at = crate::planet::EntityPos::new(
                                pos.face(),
                                f32::from(pos.u()) + 0.5 + drift,
                                f32::from(pos.y()) + 3.2 + rise,
                                f32::from(pos.v()) + 0.5,
                            )
                            .expect("bloomery smoke remains near its source");
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
        if self.in_world && self.server.world.weather.precipitating() {
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
            entity::emit_crack(target, progress, &mut overlay_verts, &mut overlay_idx);
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
                self.held_art(self.inventory.slots[self.input.hotbar_sel].map(|st| st.item)),
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
            let (ax, ay, aw, ah) = self.inventory_avatar_rect();
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
                self.held_art(self.inventory.slots[self.input.hotbar_sel].map(|st| st.item)),
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
        // The held torch: your own body of light, real shadows and all.
        // Anchored to the body center, never the facing — a camera-
        // relative offset made the light orbit the head when turning,
        // so shadows stuck then snapped with every look-around (and
        // thrashed the cube cache). Remote helds anchor the same way.
        if self.in_world
            && let Some(stack) = self.inventory.slots[self.input.hotbar_sel]
            && let Some((color, range)) = self.held_glow(stack.item)
        {
            dyn_lights.push(lights::DynLight {
                key: lights::Key::Held,
                pos: self.camera.pos - self.camera.up() * 0.15,
                color,
                range,
            });
        }
        // The remaining dynamic slots go to whatever is closest: other
        // players' torches or glowing wardens.
        if self.in_world {
            let cam = self.camera.pos;
            let mut tail: Vec<(f32, lights::DynLight)> = Vec::new();
            if let Some(r) = &self.multiplayer.remote {
                for (id, _) in &r.players {
                    let Some(&held) = r.player_held.get(id) else {
                        continue;
                    };
                    let local = r.item_map.get(held as usize).copied().flatten();
                    if let Some(item) = local
                        && let Some((color, range)) = self.held_glow(item)
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
                    if g.held == u16::MAX {
                        continue;
                    }
                    if let Some((color, range)) = self.held_glow(ItemId(g.held)) {
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
        let ambient_floor = if self.config.stark { 0.04 } else { 0.12 };

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
            sun_col,
            amb_col,
            ambient_floor,
            dda_shadow,
            occ_origin,
            occ_update: occ_grid.as_deref(),
            point_lights: &point_lights,
            outline,
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
            if self.chunk_work_pending() == 0 {
                self.settled_frames += 1;
            } else {
                self.settled_frames = 0;
            }
            let ready = match forced {
                Some(frame) => self.total_frames >= frame,
                None => {
                    self.settled_frames >= SHOT_SETTLE_FRAMES
                        || self.total_frames >= SHOT_MAX_FRAMES
                }
            };
            match self.shot_at {
                Some(at) if self.total_frames > at + 1 => std::process::exit(0),
                None if ready => {
                    eprintln!(
                        "capture at frame {} ({}), fps {}, sim {:.2}ms draw {:.2}ms",
                        self.total_frames,
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
                    );
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
            self.window.set_title(&format!(
                "Wildforge — {} fps (sim {:.1}ms, draw {:.1}ms, {}) | XYZ {:.1} / {:.1} / {:.1}{biome}{}",
                self.fps,
                self.frame_ms.0,
                self.frame_ms.1,
                self.renderer.adapter_name,
                p.x,
                p.y,
                p.z,
                if self.input.mouse_captured || self.ui_state.screen != Screen::Playing {
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

        self.refresh_content_and_toasts(dt);

        self.advance_player(dt, paused);

        self.build_and_render_frame(dt, now);
    }

    // ---------- UI layout ----------

    pub(super) const SLOT: f32 = 46.0;
}

#[cfg(test)]
mod characterization {
    use super::{VIEWMODEL_HAND_MIN, VIEWMODEL_SLEEVE_MAX, local_sim_should_advance};

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
