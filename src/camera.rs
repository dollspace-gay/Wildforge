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
    }

    /// Rotate a face-local simulation vector into embedded planet space.
    pub fn world_vector(&self, local: Vec3) -> Vec3 {
        self.east * local.x + self.up * local.y + self.north * local.z
    }

    pub fn forward(&self) -> Vec3 {
        let local = self.local_forward();
        (self.east * local.x + self.up * local.y + self.north * local.z).normalize()
    }

    /// Look direction expressed in the player's current face-local tangent
    /// frame. Simulation, voxel DDA, and locally stored entity velocity use
    /// this basis; rendering uses [`Self::forward`].
    pub fn local_forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
        .normalize()
    }

    /// Face-local horizontal forward, for movement and simulation.
    pub fn local_flat_forward(&self) -> Vec3 {
        Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin()).normalize()
    }

    /// Face-local right, for movement and simulation.
    pub fn local_right(&self) -> Vec3 {
        self.local_flat_forward().cross(Vec3::Y).normalize()
    }

    pub fn up(&self) -> Vec3 {
        self.up
    }

    pub fn turn(&mut self, dx: f32, dy: f32) {
        let sens = 0.0022 * self.sens * self.env_sens;
        self.yaw += dx * sens;
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
