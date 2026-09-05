//! Player camera: first person, an over-shoulder chase view, and an orbit
//! "factory" camera for stepping back from the work.

use glam::{Mat4, Vec3};

/// The view the player uses. `Third` chases over the shoulder; `Orbit` is a
/// drag-to-spin, wheel-to-zoom camera floating around the player, the view
/// for inspecting a settlement or factory.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CameraMode {
    #[default]
    First,
    Third,
    Orbit,
}

impl CameraMode {
    /// The stable identifier stored in `world.toml` (the `camera` line).
    pub fn key(self) -> &'static str {
        match self {
            CameraMode::First => "first",
            CameraMode::Third => "third",
            CameraMode::Orbit => "orbit",
        }
    }

    /// Parse a persisted `camera` value; unknown values fall back to first.
    pub fn parse(value: &str) -> CameraMode {
        match value.trim() {
            "third" => CameraMode::Third,
            "orbit" => CameraMode::Orbit,
            _ => CameraMode::First,
        }
    }
}

pub struct Camera {
    pub pos: Vec3,
    pub yaw: f32,   // radians, 0 = +Z... we use standard: forward from yaw/pitch
    pub pitch: f32, // radians
    pub aspect: f32,
    pub fovy: f32,
    /// Base sensitivity multiplier from settings; WILDFORGE_SENS multiplies further.
    pub sens: f32,
    /// Active view. `turn` and placement are mode-aware.
    pub mode: CameraMode,
    /// Third-person chase geometry: how far behind the aim, over the
    /// shoulder, and above the eye the camera floats (in blocks).
    pub third_dist: f32,
    pub third_shoulder: f32,
    pub third_up: f32,
    /// Orbit camera angles around the player and the zoom distance.
    pub orbit_yaw: f32,
    pub orbit_pitch: f32,
    pub orbit_dist: f32,
    /// The point the orbit camera looks at (set each frame to the player).
    orbit_target: Vec3,
    env_sens: f32,
    east: Vec3,
    up: Vec3,
    north: Vec3,
    /// Chart-space deltas that move one embedded block along the camera's
    /// orthonormal east/north tangent axes. A spherified cube's `(u, v)` axes
    /// are not an orthonormal basis away from face center, so using tangent
    /// components directly as chart deltas can reverse screen-space strafing.
    chart_east: Vec3,
    chart_north: Vec3,
}

impl Camera {
    pub fn new(pos: Vec3, aspect: f32) -> Camera {
        Camera {
            pos,
            yaw: -std::f32::consts::FRAC_PI_2,
            pitch: 0.0,
            aspect,
            fovy: 75f32.to_radians(),
            sens: 1.0,
            mode: CameraMode::First,
            third_dist: 3.5,
            third_shoulder: 0.35,
            third_up: 0.15,
            orbit_yaw: 0.0,
            orbit_pitch: 0.55,
            orbit_dist: 6.5,
            orbit_target: Vec3::ZERO,
            // WILDFORGE_SENS scales look sensitivity on top of settings.
            env_sens: std::env::var("WILDFORGE_SENS")
                .ok()
                .and_then(|v| v.parse().ok())
                .filter(|s: &f32| *s > 0.0 && *s <= 10.0)
                .unwrap_or(1.0),
            east: Vec3::X,
            up: Vec3::Y,
            north: Vec3::Z,
            chart_east: Vec3::X,
            chart_north: Vec3::Z,
        }
    }

    pub fn follow_planet(&mut self, eye: crate::planet::EntityPos) {
        self.pos = eye.render_pos();
        let frame = crate::planet::local_frame(crate::planet::SurfacePoint {
            face: eye.face(),
            u: f64::from(eye.u()),
            v: f64::from(eye.v()),
        });
        self.east = frame.east.as_vec3();
        self.up = frame.up.as_vec3();
        self.north = frame.north.as_vec3();
        (self.chart_east, self.chart_north) = frame.chart_basis(eye);
    }

    /// Rotate a face-local simulation vector into embedded planet space.
    pub fn world_vector(&self, local: Vec3) -> Vec3 {
        self.east * local.x + self.up * local.y + self.north * local.z
    }

    pub fn forward(&self) -> Vec3 {
        if self.mode == CameraMode::Orbit {
            return (self.orbit_target - self.pos).normalize_or_zero();
        }
        let local = self.tangent_forward();
        (self.east * local.x + self.up * local.y + self.north * local.z).normalize()
    }

    /// Look direction in the orthonormal east/up/north tangent basis.
    pub fn tangent_forward(&self) -> Vec3 {
        if self.mode == CameraMode::Orbit {
            // The orbit camera looks at the player: express that embedded
            // direction in the tangent basis the chart helpers expect.
            let f = (self.orbit_target - self.pos).normalize_or_zero();
            return Vec3::new(f.dot(self.east), f.dot(self.up), f.dot(self.north)).normalize();
        }
        Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
        .normalize()
    }

    fn tangent_to_chart(&self, tangent: Vec3) -> Vec3 {
        self.chart_east * tangent.x + Vec3::Y * tangent.y + self.chart_north * tangent.z
    }

    /// Look direction expressed in the player's current face-local tangent
    /// frame. Simulation, voxel DDA, and locally stored entity velocity use
    /// this basis; rendering uses [`Self::forward`].
    pub fn local_forward(&self) -> Vec3 {
        self.tangent_to_chart(self.tangent_forward())
    }

    /// Face-local horizontal forward, for movement and simulation.
    pub fn local_flat_forward(&self) -> Vec3 {
        self.tangent_to_chart(Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin()))
    }

    /// Face-local right, for movement and simulation.
    pub fn local_right(&self) -> Vec3 {
        // `(east, up, north)` is left-handed, so forward × up has tangent
        // components `(forward.z, 0, -forward.x)`. Convert that physical
        // screen-right direction through the actual spherified-surface
        // Jacobian before physics treats it as a `(du, dv)` chart delta.
        self.tangent_to_chart(Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos()))
    }

    pub fn up(&self) -> Vec3 {
        self.up
    }

    pub fn turn(&mut self, dx: f32, dy: f32) {
        let sens = 0.0022 * self.sens * self.env_sens;
        // A positive pointer delta is motion toward screen-right. In this
        // tangent frame yaw increases toward screen-left, so pointer X must
        // subtract from yaw. Keeping that convention here also makes raw and
        // cursor-position look paths agree.
        if self.mode == CameraMode::Orbit {
            // Dragging spins the camera around the player with the same
            // feel as looking around; the wheel (see app.rs) zooms.
            self.orbit_yaw -= dx * sens;
            self.orbit_pitch = (self.orbit_pitch - dy * sens).clamp(-1.35, 1.35);
            return;
        }
        self.yaw -= dx * sens;
        self.pitch = (self.pitch - dy * sens).clamp(-1.55, 1.55);
    }

    #[allow(deprecated)]
    pub fn view_proj(&self) -> Mat4 {
        // All scene vertices subtract `self.pos` before this matrix is applied.
        // Keeping the eye at zero is the renderer's floating origin: depth and
        // projection never cancel two five-thousand-unit planet coordinates.
        let view = Mat4::look_to_rh(Vec3::ZERO, self.forward(), self.up);
        let proj = Mat4::perspective_rh(self.fovy, self.aspect.max(0.01), 0.05, 600.0);
        proj * view
    }

    /// Over-shoulder chase placement: float behind the aim direction, over
    /// the right shoulder, with a wall push-out so the eye never tunnels into
    /// terrain. The cast runs in the player's chart so the DDA crosses
    /// cube-face seams cleanly; on a hit the eye sits in the last free cell,
    /// pulled a hair toward the player to clear the near plane.
    pub fn place_chase(&mut self, eye: crate::planet::EntityPos, world: &(impl crate::world::TerrainRead + ?Sized)) {
        let base = eye.render_pos();
        let f = self.forward();
        let r = f.cross(self.up()).normalize_or_zero();
        let desired =
            base - f * self.third_dist + r * self.third_shoulder + self.up() * self.third_up;

        let d_chart = -self.local_forward() * self.third_dist
            + self.local_right() * self.third_shoulder
            + Vec3::Y * self.third_up;
        let len = d_chart.length();
        if len <= 0.001 {
            self.pos = desired;
            return;
        }
        let dir = d_chart / len;
        match crate::raycast::raycast_at(world, eye, dir, len + 0.6) {
            Some(hit) => {
                let adj = hit.adjacent;
                let center = crate::planet::EntityPos::new(
                    eye.face(),
                    adj.u() as f32 + 0.5,
                    adj.y() as f32 + 0.5,
                    adj.v() as f32 + 0.5,
                )
                .unwrap_or(eye);
                let toward = (base - center.render_pos()).normalize_or_zero();
                self.pos = center.render_pos() + toward * 0.18;
            }
            None => self.pos = desired,
        }
    }

    /// Orbit placement: a spherical camera floating around the player,
    /// looking back at them. `follow_planet` re-establishes the tangent
    /// frame first; this only overrides the eye position and target.
    pub fn place_orbit(&mut self, eye: crate::planet::EntityPos) {
        let up = self.up();
        let target = eye.render_pos() + up * 0.35;
        self.orbit_target = target;
        let (sp, cp) = self.orbit_pitch.sin_cos();
        let offset =
            Vec3::new(cp * self.orbit_yaw.cos(), sp, cp * self.orbit_yaw.sin()) * self.orbit_dist;
        self.pos = target + offset;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_right_matches_screen_right_on_every_planet_face() {
        for face in crate::planet::Face::ALL {
            for u in [512.5, 2048.5, 4096.5, 6144.5, 7680.5] {
                for v in [512.5, 2048.5, 4096.5, 6144.5, 7680.5] {
                    let eye = crate::planet::EntityPos::new(face, u, 96.0, v).unwrap();
                    let mut camera = Camera::new(Vec3::ZERO, 16.0 / 9.0);
                    camera.follow_planet(eye);
                    for yaw in [
                        0.0,
                        std::f32::consts::FRAC_PI_2,
                        std::f32::consts::PI,
                        -std::f32::consts::FRAC_PI_2,
                    ] {
                        camera.yaw = yaw;
                        let ahead = eye
                            .translated(camera.local_flat_forward() * 12.0)
                            .expect("small forward move remains canonical")
                            .pos;
                        let ahead_relative = ahead.render_pos() - camera.pos;
                        let ahead_clip = camera.view_proj() * ahead_relative.extend(1.0);
                        let moved = eye
                            .translated(
                                camera.local_flat_forward() * 12.0 + camera.local_right() * 4.0,
                            )
                            .expect("small strafe remains canonical")
                            .pos;
                        let relative = moved.render_pos() - camera.pos;
                        let clip = camera.view_proj() * relative.extend(1.0);
                        assert!(
                            clip.x / clip.w > ahead_clip.x / ahead_clip.w,
                            "right strafe projects left on {face:?} at {u},{v}, yaw {yaw}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn moving_the_mouse_right_turns_the_view_right() {
        let eye =
            crate::planet::EntityPos::new(crate::planet::Face::PosZ, 4096.5, 96.0, 4096.5).unwrap();
        let mut camera = Camera::new(Vec3::ZERO, 16.0 / 9.0);
        camera.follow_planet(eye);
        camera.yaw = -std::f32::consts::FRAC_PI_2;

        let old_forward = camera.forward();
        let old_screen_right = old_forward.cross(camera.up()).normalize();
        camera.turn(10.0, 0.0);

        assert!(
            camera.forward().dot(old_screen_right) > 0.0,
            "positive mouse X turned the camera toward screen-left"
        );
    }

    #[test]
    fn orbit_drag_rotates_the_camera_and_leaves_aim_alone() {
        let eye =
            crate::planet::EntityPos::new(crate::planet::Face::PosZ, 4096.5, 96.0, 4096.5).unwrap();
        let mut camera = Camera::new(Vec3::ZERO, 16.0 / 9.0);
        camera.follow_planet(eye);
        camera.mode = CameraMode::Orbit;
        camera.orbit_yaw = 0.4;
        camera.orbit_pitch = 0.5;
        camera.orbit_dist = 6.0;
        camera.yaw = 0.7;
        let aim_yaw = camera.yaw;

        camera.place_orbit(eye);
        let target = eye.render_pos() + camera.up() * 0.35;
        let to_target = (target - camera.pos).normalize_or_zero();
        assert!(
            to_target.dot(camera.forward()) > 0.999,
            "orbit camera looks at the player"
        );
        let dist = (target - camera.pos).length();
        assert!(
            (dist - camera.orbit_dist).abs() < 0.001,
            "orbit distance respected: {dist}"
        );

        camera.turn(50.0, 0.0);
        assert_ne!(camera.orbit_yaw, 0.4, "drag spins the orbit yaw");
        assert_eq!(camera.yaw, aim_yaw, "orbit drag must not move the aim");
    }

    #[test]
    fn camera_mode_keys_round_trip() {
        for (mode, key) in [
            (CameraMode::First, "first"),
            (CameraMode::Third, "third"),
            (CameraMode::Orbit, "orbit"),
        ] {
            assert_eq!(mode.key(), key);
            assert_eq!(CameraMode::parse(key), mode);
        }
        assert_eq!(CameraMode::parse("bogus"), CameraMode::First);
        assert_eq!(CameraMode::parse(""), CameraMode::First);
    }

    #[test]
    fn chase_camera_pushes_out_before_a_wall() {
        let reg = std::sync::Arc::new(crate::registry::load(std::path::Path::new(
            "/nonexistent-mods-dir",
        )));
        let dir =
            std::env::temp_dir().join(format!("wildforge-camera-chase-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut w = crate::world::World::new(42, dir.clone(), reg.clone());
        w.ensure_chunk(
            crate::chunk::ChunkPos::from_centered(crate::planet::Face::PosZ, 0, 0).expect("chunk"),
        );
        let stone = reg.block_id("base:stone").expect("stone");
        let air = crate::registry::AIR;
        // A sky corridor so generated terrain never interferes.
        for x in 6..=10 {
            for z in 7..=14 {
                for y in 198..=202 {
                    w.set_block_at(
                        crate::planet::BlockPos::of_world(x, y, z).expect("cell"),
                        air,
                    );
                }
            }
        }
        // A wall three blocks behind the eye (camera faces -Z).
        for x in 7..=9 {
            for z in 11..=13 {
                for y in 198..=202 {
                    w.set_block_at(
                        crate::planet::BlockPos::of_world(x, y, z).expect("cell"),
                        stone,
                    );
                }
            }
        }
        let eye = crate::planet::EntityPos::from_local(
            crate::planet::Face::PosZ,
            Vec3::new(8.0, 200.0, 8.0),
        )
        .expect("eye");
        let mut camera = Camera::new(Vec3::ZERO, 16.0 / 9.0);
        camera.follow_planet(eye);
        camera.mode = CameraMode::Third;
        camera.yaw = -std::f32::consts::FRAC_PI_2;
        camera.pitch = 0.0;
        camera.third_dist = 3.5;
        // Express the embedded camera position back in the player's local
        // chart so the wall bounds are directly comparable.
        let local_of = |camera: &Camera| -> Vec3 {
            let d = camera.pos - eye.render_pos();
            let tangent = Vec3::new(d.dot(camera.east), d.dot(camera.up), d.dot(camera.north));
            Vec3::new(8.0, 200.0, 8.0)
                + camera.chart_east * tangent.x
                + Vec3::Y * tangent.y
                + camera.chart_north * tangent.z
        };

        camera.place_chase(eye, &w);
        let local = local_of(&camera);
        assert!(
            (9.5..10.8).contains(&local.z),
            "chase pushed out before the wall: local z {} (wall at 11..13)",
            local.z
        );
        assert!(
            (local - Vec3::new(8.0, 200.0, 8.0)).length() < camera.third_dist - 0.2,
            "wall occlusion shortened the chase"
        );

        // Remove the wall: the chase floats at the full distance again.
        for x in 7..=9 {
            for z in 11..=13 {
                for y in 198..=202 {
                    w.set_block_at(
                        crate::planet::BlockPos::of_world(x, y, z).expect("cell"),
                        air,
                    );
                }
            }
        }
        camera.place_chase(eye, &w);
        let local = local_of(&camera);
        assert!(
            (local.z - 11.5).abs() < 0.35,
            "no wall: chase floats at local z {} (expected 11.5)",
            local.z
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
