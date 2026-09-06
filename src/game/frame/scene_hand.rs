//! Scene hand in the graphical frame pipeline.

use super::{Geometry, portrait_depth};
use crate::game::Game;
use crate::game::navigation::Screen;
use crate::mobs;
use glam::Vec3;

impl Game {
    pub(in crate::game) fn prepare_scene_hand(&self) -> Geometry {
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

        Geometry {
            vertices: hand_verts,
            indices: hand_idx,
        }
    }
}
