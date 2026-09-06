//! Preparation stage of GPU frame encoding.

use crate::renderer::{
    CASCADE_RADII, CASCADE_STRIDE, FrameInput, MAX_PT_LIGHTS, OCC_GRID, Renderer, SHADOW_CASCADES,
    Uniforms,
};
use glam::{Mat4, Vec3};

/// Dev shadow-debug viz mode (WILDFORGE_SHADOW_DEBUG), read once. 0 = off;
/// 1 = red-where-occluded, 2 = raw shadow factor, 3 = cube-face id,
/// 4 = depth-compare margin. Rides in the uniform's pt_count.z.
/// How much of the ambient floor a fully occluded corner keeps. 1 disables
/// corner darkening entirely; 0 lets corners reach black. `WILDFORGE_AO_FLOOR`.
fn ao_floor() -> f32 {
    use std::sync::OnceLock;
    static A: OnceLock<f32> = OnceLock::new();
    *A.get_or_init(|| {
        std::env::var("WILDFORGE_AO_FLOOR")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|v: &f32| (0.0..=1.0).contains(v))
            .unwrap_or(0.1)
    })
}

fn shadow_debug() -> u32 {
    use std::sync::OnceLock;
    static M: OnceLock<u32> = OnceLock::new();
    *M.get_or_init(|| {
        std::env::var("WILDFORGE_SHADOW_DEBUG")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    })
}

impl Renderer {
    pub(in crate::renderer) fn frame_uniforms(
        &self,
        f: &FrameInput<'_>,
    ) -> (Uniforms, [Mat4; SHADOW_CASCADES]) {
        // Sun light-space matrix: an orthographic box centered near the camera,
        // looking from the sun toward that center. Covers the near field; beyond
        // its radius the shader treats fragments as lit (shadows fade out).
        // One ortho box per cascade, all centered on the camera and looking from
        // the sun, sized tightest-first (see CASCADE_RADII): a dense near map for
        // crisp contact shadows / indoor sunbeams out to a wide map for distance.
        let light_vp: [Mat4; SHADOW_CASCADES] = {
            let dist = 160.0f32;
            let center = f.cam_pos;
            let eye = center + f.sun_dir * dist;
            let up = if f.sun_dir.y.abs() > 0.95 {
                Vec3::Z
            } else {
                Vec3::Y
            };
            let view = glam::camera::rh::view::look_at_mat4(eye, center, up);
            let mut a = [Mat4::IDENTITY; SHADOW_CASCADES];
            for (i, &radius) in CASCADE_RADII.iter().enumerate() {
                let proj = glam::camera::rh::proj::directx::orthographic(
                    -radius,
                    radius,
                    -radius,
                    radius,
                    1.0,
                    dist + radius * 2.0,
                );
                a[i] = proj * view;
            }
            a
        };

        let inv_view_proj = f.view_proj.inverse();
        let uniforms = Uniforms {
            view_proj: f.view_proj.to_cols_array_2d(),
            cam: [f.cam_pos.x, f.cam_pos.y, f.cam_pos.z, f.fog_dist],
            origin: [f.cam_pos.x, f.cam_pos.y, f.cam_pos.z, 0.0],
            local_up: [f.local_up.x, f.local_up.y, f.local_up.z, 0.0],
            sky: [
                self.sky_color[0],
                self.sky_color[1],
                self.sky_color[2],
                f.gloom,
            ],
            misc: [
                if f.underwater { 1.0 } else { 0.0 },
                f.daylight,
                self.config.width as f32,
                self.config.height as f32,
            ],
            sun_dir: [f.sun_dir.x, f.sun_dir.y, f.sun_dir.z, 0.0],
            sun_col: [f.sun_col.x, f.sun_col.y, f.sun_col.z, ao_floor()],
            amb_col: [f.amb_col.x, f.amb_col.y, f.amb_col.z, f.ambient_floor],
            light_vp: light_vp.map(|m| m.to_cols_array_2d()),
            pt_count: [
                f.point_lights.len().min(MAX_PT_LIGHTS) as u32,
                0,
                shadow_debug(),
                f.dda_shadow as u32,
            ],
            pt_pos: {
                let mut a = [[0.0f32; 4]; MAX_PT_LIGHTS];
                for (i, l) in f.point_lights.iter().take(MAX_PT_LIGHTS).enumerate() {
                    a[i] = [l.pos.x, l.pos.y, l.pos.z, l.range];
                }
                a
            },
            pt_col: {
                let mut a = [[0.0f32; 4]; MAX_PT_LIGHTS];
                for (i, l) in f.point_lights.iter().take(MAX_PT_LIGHTS).enumerate() {
                    a[i] = [l.color.x, l.color.y, l.color.z, 0.0];
                }
                a
            },
            pt_misc: {
                let mut a = [[0.0f32; 4]; MAX_PT_LIGHTS];
                for (i, l) in f.point_lights.iter().take(MAX_PT_LIGHTS).enumerate() {
                    a[i] = [
                        l.suppress.0,
                        l.suppress.1,
                        if l.shadows { 1.0 } else { 0.0 },
                        l.radius,
                    ];
                }
                a
            },
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            sun_dir_true: [f.sun_dir_true.x, f.sun_dir_true.y, f.sun_dir_true.z, 0.0],
            sh: {
                let mut a = [[0.0f32; 4]; 9];
                for (i, c) in f.sh_ambient.iter().enumerate() {
                    a[i] = [c.x, c.y, c.z, 0.0];
                }
                a
            },
            occ_origin: [
                f.occ_origin[0],
                f.occ_origin[1],
                f.occ_origin[2],
                self.atlas_interior_base as i32,
            ],
            layer: self.atlas_layer_params,
            room_sh: {
                let mut a = [[0.0f32; 4]; 4];
                for (i, c) in f.room_sh.iter().enumerate() {
                    a[i] = [c.x, c.y, c.z, 0.0];
                }
                a[0][3] = f.room_intensity;
                a
            },
        };
        (uniforms, light_vp)
    }
    pub(in crate::renderer) fn upload_frame(
        &mut self,
        f: &FrameInput<'_>,
        uniforms: &Uniforms,
        light_vp: &[Mat4; SHADOW_CASCADES],
    ) {
        // Upload a fresh occupancy grid when the camera crossed into a new region.
        if let Some(bytes) = f.occ_update {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.occ_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(OCC_GRID as u32 * 4),
                    rows_per_image: Some(OCC_GRID as u32),
                },
                wgpu::Extent3d {
                    width: OCC_GRID as u32,
                    height: OCC_GRID as u32,
                    depth_or_array_layers: OCC_GRID as u32,
                },
            );
        }
        self.entity_vbuf.upload(
            &self.device,
            &self.queue,
            bytemuck::cast_slice(f.entity_verts),
        );
        self.entity_ibuf.upload(
            &self.device,
            &self.queue,
            bytemuck::cast_slice(f.entity_idx),
        );
        self.overlay_vbuf.upload(
            &self.device,
            &self.queue,
            bytemuck::cast_slice(f.overlay_verts),
        );
        self.overlay_ibuf.upload(
            &self.device,
            &self.queue,
            bytemuck::cast_slice(f.overlay_idx),
        );
        self.hand_vbuf.upload(
            &self.device,
            &self.queue,
            bytemuck::cast_slice(f.hand_verts),
        );
        self.hand_ibuf
            .upload(&self.device, &self.queue, bytemuck::cast_slice(f.hand_idx));
        self.ui_vbuf
            .upload(&self.device, &self.queue, bytemuck::cast_slice(f.ui_verts));
        self.queue
            .write_buffer(&self.uniforms_buf, 0, bytemuck::bytes_of(uniforms));
        // Per-cascade matrix for the depth pass, one dynamic-offset slot each.
        for (i, m) in light_vp.iter().enumerate() {
            self.queue.write_buffer(
                &self.shadow_casc_buf,
                i as u64 * CASCADE_STRIDE,
                bytemuck::cast_slice(&m.to_cols_array()),
            );
        }

        self.point_shadows.upload(&self.queue, f.point_lights);
    }
}
