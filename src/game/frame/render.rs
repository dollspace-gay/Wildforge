//! Render in the graphical frame pipeline.

use super::{LightingFrame, SelectionFrame};
use crate::game::Game;
use crate::renderer::FrameInput;
use std::time::Instant;

impl Game {
    pub(in crate::game) fn build_and_render_frame(&mut self, dt: f32, now: Instant) {
        let LightingFrame {
            daylight,
            sun_dir,
            sun_dir_true,
            sun_col,
            amb_col,
            gloom,
            sh_ambient,
            local_weather,
        } = self.prepare_frame_lighting(dt);
        let SelectionFrame {
            playing,
            outline,
            outline_color,
            underwater,
            fog,
        } = self.prepare_frame_selection(gloom);
        let mut entities = self.prepare_scene_members(dt);
        self.emit_scene_stations(&mut entities);
        self.emit_scene_precipitation(&mut entities, local_weather);
        let overlay = self.prepare_scene_overlay();
        let hand = self.prepare_scene_hand();
        self.prepare_frame_ui();
        let point_lights = self.prepare_frame_point_lights(dt);
        let (ambient_floor, amb_col) = self.prepare_frame_adaptation(daylight, amb_col);
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
            entity_verts: &entities.vertices,
            entity_idx: &entities.indices,
            overlay_verts: &overlay.vertices,
            overlay_idx: &overlay.indices,
            hand_verts: &hand.vertices,
            hand_idx: &hand.indices,
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

        self.finish_rendered_frame(fog, now);
    }
}
