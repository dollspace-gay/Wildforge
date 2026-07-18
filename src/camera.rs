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
        }
    }

    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
        .normalize()
    }

    /// Horizontal forward (for movement).
    pub fn flat_forward(&self) -> Vec3 {
        Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin()).normalize()
    }

    pub fn right(&self) -> Vec3 {
        self.flat_forward().cross(Vec3::Y).normalize()
    }

    pub fn turn(&mut self, dx: f32, dy: f32) {
        let sens = 0.0022 * self.sens * self.env_sens;
        self.yaw += dx * sens;
        self.pitch = (self.pitch - dy * sens).clamp(-1.55, 1.55);
    }

    #[allow(deprecated)]
    pub fn view_proj(&self) -> Mat4 {
        let view = Mat4::look_to_rh(self.pos, self.forward(), Vec3::Y);
        let proj = Mat4::perspective_rh(self.fovy, self.aspect.max(0.01), 0.05, 600.0);
        proj * view
    }
}
