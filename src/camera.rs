//! First-person camera.

use glam::{Mat4, Vec3};

pub struct Camera {
    pub pos: Vec3,
    pub yaw: f32,   // radians, 0 = +Z... we use standard: forward from yaw/pitch
    pub pitch: f32, // radians
    pub aspect: f32,
    pub fovy: f32,
    /// Base sensitivity multiplier from settings; WILDFORGE_SENS multiplies further.
    pub sens: f32,
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
        let point = eye.surface_point();
        let sample = |u: f64, v: f64| {
            crate::planet::block_to_render(
                crate::planet::SurfacePoint {
                    face: point.face,
                    u,
                    v,
                },
                f64::from(eye.y()),
            )
            .as_vec3()
        };
        const EPSILON: f64 = 0.25;
        let side = f64::from(crate::planet::FACE_BLOCKS);
        let (u0, u1) = ((point.u - EPSILON).max(0.0), (point.u + EPSILON).min(side));
        let (v0, v1) = ((point.v - EPSILON).max(0.0), (point.v + EPSILON).min(side));
        let chart_u = (sample(u1, point.v) - sample(u0, point.v)) / (u1 - u0) as f32;
        let chart_v = (sample(point.u, v1) - sample(point.u, v0)) / (v1 - v0) as f32;
        let solve = |wanted: Vec3| {
            let uu = chart_u.dot(chart_u);
            let uv = chart_u.dot(chart_v);
            let vv = chart_v.dot(chart_v);
            let determinant = uu * vv - uv * uv;
            if determinant.abs() < 1.0e-8 {
                return Vec3::ZERO;
            }
            let ur = chart_u.dot(wanted);
            let vr = chart_v.dot(wanted);
            Vec3::new(
                (ur * vv - vr * uv) / determinant,
                0.0,
                (vr * uu - ur * uv) / determinant,
            )
        };
        self.chart_east = solve(self.east);
        self.chart_north = solve(self.north);
    }

    /// Rotate a face-local simulation vector into embedded planet space.
    pub fn world_vector(&self, local: Vec3) -> Vec3 {
        self.east * local.x + self.up * local.y + self.north * local.z
    }

    pub fn forward(&self) -> Vec3 {
        let local = self.tangent_forward();
        (self.east * local.x + self.up * local.y + self.north * local.z).normalize()
    }

    /// Look direction in the orthonormal east/up/north tangent basis.
    pub fn tangent_forward(&self) -> Vec3 {
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
}
