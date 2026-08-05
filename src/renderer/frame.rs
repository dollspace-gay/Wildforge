//! Ordered shadow, world, viewmodel, post-processing, and UI frame passes.

use super::*;

struct DiagnosticReadback {
    buffer: wgpu::Buffer,
    texture: wgpu::Texture,
    width: u32,
    height: u32,
    padded_bytes_per_row: u32,
}

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

/// Scene value that maps to display white; past it the curve compresses rather
/// than clips. Lower means the highlights roll sooner and the image reads
/// brighter overall — at 8 the picture goes grey, because almost nothing in it
/// is ever allowed to reach white. `WILDFORGE_WHITE` overrides it.
fn white_point() -> f32 {
    use std::sync::OnceLock;
    static W: OnceLock<f32> = OnceLock::new();
    *W.get_or_init(|| {
        std::env::var("WILDFORGE_WHITE")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|v: &f32| *v >= 0.25 && *v <= 64.0)
            .unwrap_or(2.0)
    })
}

/// What the exposure stops down to under full daylight.
const DAY_EXPOSURE: f32 = 0.45;

/// Scene exposure applied before the tone curve. `WILDFORGE_EXPOSURE` overrides
/// it — the knob to reach for when the whole image reads too dark or too hot,
/// as distinct from any one light being wrong.
fn exposure(daylight: f32) -> f32 {
    use std::sync::OnceLock;
    static E: OnceLock<Option<f32>> = OnceLock::new();
    let forced = *E.get_or_init(|| {
        std::env::var("WILDFORGE_EXPOSURE")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|v: &f32| *v > 0.0 && *v <= 64.0)
    });
    if let Some(e) = forced {
        return e;
    }
    // Stopped down by day, open at night — an eye adapting, and the only way
    // to spend a brighter sun on contrast rather than on brightness. Exposing
    // for daylight is what makes its shadows deep; holding exposure flat just
    // makes the whole image paler. Night keeps the old exposure exactly, so
    // torchlight and moonlight read as they always did.
    1.0 - (1.0 - DAY_EXPOSURE) * daylight.clamp(0.0, 1.0)
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
    fn composite_and_ui(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        f: &FrameInput<'_>,
    ) {
        {
            let mut pass = post_pass(encoder, "composite", target);
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &self.post.composite_scene_bg, &[]);
            pass.set_bind_group(1, &self.post.composite_bloom_bg, &[]);
            pass.set_bind_group(2, &self.post_params_bg, &[]);
            pass.draw(0..3, 0..1);
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_bind_group(0, &self.uniform_bg, &[]);
        pass.set_bind_group(1, &self.atlas_bg, &[]);
        pass.set_bind_group(2, &self.shadow_bg, &[]);

        if f.crosshair {
            pass.set_pipeline(&self.line_screen_pipeline);
            pass.set_vertex_buffer(0, self.crosshair_buf.slice(..));
            pass.draw(0..4, 0..1);
        }
        if !f.ui_verts.is_empty() {
            pass.set_pipeline(&self.ui_pipeline);
            pass.set_vertex_buffer(0, self.ui_vbuf.buf.slice(..));
            pass.draw(0..f.ui_verts.len() as u32, 0..1);
        }
    }

    fn encode_diagnostic_replay(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        f: &FrameInput<'_>,
        visible: &[(&ChunkPos, &GpuChunk)],
        outline: Option<crate::planet::BlockPos>,
    ) -> DiagnosticReadback {
        let pipelines = self
            .diagnostic_pipelines
            .as_ref()
            .expect("visual evidence requested without diagnostic pipelines");
        let width = self.config.width;
        let height = self.config.height;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("visible-fragment-diagnostic"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Uint,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("visible-fragment-diagnostic-depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let planes = frustum_planes(&f.view_proj);

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("visible-fragment-diagnostic-world"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.uniform_bg, &[]);
            pass.set_bind_group(1, &self.atlas_bg, &[]);
            pass.set_bind_group(2, &self.shadow_bg, &[]);
            pass.set_pipeline(&pipelines.chunk);
            for (_position, gpu) in visible.iter().copied() {
                if !chunk_visible(&planes, gpu, f.cam_pos) {
                    continue;
                }
                if let Some(mesh) = &gpu.opaque {
                    pass.set_vertex_buffer(0, mesh.vbuf.slice(..));
                    pass.set_index_buffer(mesh.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.count, 0, 0..1);
                }
            }
            if !f.entity_idx.is_empty() {
                pass.set_pipeline(&pipelines.chunk_overlay);
                pass.set_vertex_buffer(0, self.entity_vbuf.buf.slice(..));
                pass.set_index_buffer(self.entity_ibuf.buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..f.entity_idx.len() as u32, 0, 0..1);
            }

            pass.set_pipeline(&pipelines.water);
            for (_position, gpu) in visible.iter().copied() {
                if !chunk_visible(&planes, gpu, f.cam_pos) {
                    continue;
                }
                if let Some(mesh) = &gpu.water {
                    pass.set_vertex_buffer(0, mesh.vbuf.slice(..));
                    pass.set_index_buffer(mesh.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.count, 0, 0..1);
                }
            }
            if !f.overlay_idx.is_empty() {
                pass.set_pipeline(&pipelines.chunk_overlay);
                pass.set_vertex_buffer(0, self.overlay_vbuf.buf.slice(..));
                pass.set_index_buffer(self.overlay_ibuf.buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..f.overlay_idx.len() as u32, 0, 0..1);
            }
            if outline.is_some() {
                pass.set_pipeline(&pipelines.line_world);
                pass.set_vertex_buffer(0, self.outline_buf.slice(..));
                pass.draw(0..24, 0..1);
            }
        }

        // Match the shipping hand pass: load world ids but clear its private
        // depth so the viewmodel replaces whatever is behind it.
        if !f.hand_idx.is_empty() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("visible-fragment-diagnostic-hand"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.uniform_bg, &[]);
            pass.set_bind_group(1, &self.atlas_bg, &[]);
            pass.set_bind_group(2, &self.shadow_bg, &[]);
            pass.set_pipeline(&pipelines.chunk_overlay);
            pass.set_vertex_buffer(0, self.hand_vbuf.buf.slice(..));
            pass.set_index_buffer(self.hand_ibuf.buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..f.hand_idx.len() as u32, 0, 0..1);
        }

        // UI/crosshair are not block families. Mark their actual submitted
        // fragments with the reserved overlay id so analysis never mistakes
        // the obscured terrain for visible rock.
        if f.crosshair || !f.ui_verts.is_empty() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("visible-fragment-diagnostic-ui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.uniform_bg, &[]);
            pass.set_bind_group(1, &self.atlas_bg, &[]);
            pass.set_bind_group(2, &self.shadow_bg, &[]);
            if f.crosshair {
                pass.set_pipeline(&pipelines.line_screen);
                pass.set_vertex_buffer(0, self.crosshair_buf.slice(..));
                pass.draw(0..4, 0..1);
            }
            if !f.ui_verts.is_empty() {
                pass.set_pipeline(&pipelines.ui);
                pass.set_vertex_buffer(0, self.ui_vbuf.buf.slice(..));
                pass.draw(0..f.ui_verts.len() as u32, 0..1);
            }
        }

        let padded_bytes_per_row = (width * 8).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("visible-fragment-diagnostic-readback"),
            size: u64::from(padded_bytes_per_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        DiagnosticReadback {
            buffer,
            texture,
            width,
            height,
            padded_bytes_per_row,
        }
    }

    pub fn render(&mut self, f: FrameInput) -> Result<(), wgpu::SurfaceError> {
        let outline = f.outline;
        let outline_color = f.outline_color;

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
            .write_buffer(&self.uniforms_buf, 0, bytemuck::bytes_of(&uniforms));
        // Per-cascade matrix for the depth pass, one dynamic-offset slot each.
        for (i, m) in light_vp.iter().enumerate() {
            self.queue.write_buffer(
                &self.shadow_casc_buf,
                i as u64 * CASCADE_STRIDE,
                bytemuck::cast_slice(&m.to_cols_array()),
            );
        }

        // Per-face matrices for the point-shadow cube passes: a 90° perspective
        // per cube face, plus the light position for the distance write.
        let n_pt = f.point_lights.len().min(MAX_PT_LIGHTS);
        if n_pt > 0 {
            let stride = PT_FACE_STRIDE as usize;
            let mut data = vec![0u8; n_pt * 6 * stride];
            for (li, l) in f.point_lights.iter().take(MAX_PT_LIGHTS).enumerate() {
                // Same deprecated-but-stable glam camera API as Camera::view_proj.
                #[allow(deprecated)]
                let proj =
                    Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1, l.range.max(1.0));
                for (face, (dir, up)) in CUBE_FACES.iter().enumerate() {
                    #[allow(deprecated)]
                    let view = Mat4::look_at_rh(l.pos, l.pos + Vec3::from(*dir), Vec3::from(*up));
                    let vp = (proj * view).to_cols_array();
                    let lp = [l.pos.x, l.pos.y, l.pos.z, 0.0f32];
                    let s = (li * 6 + face) * stride;
                    data[s..s + 64].copy_from_slice(bytemuck::cast_slice(&vp));
                    data[s + 64..s + 80].copy_from_slice(bytemuck::cast_slice(&lp));
                }
            }
            self.queue.write_buffer(&self.pt_face_buf, 0, &data);
        }

        if let Some(block) = outline {
            let e = 0.003f32;
            let c = outline_color;
            let p = |du: f64, dy: f64, dv: f64| {
                let surface = crate::planet::SurfacePoint {
                    face: block.face(),
                    u: f64::from(block.u()) + du,
                    v: f64::from(block.v()) + dv,
                };
                let mut pos =
                    crate::planet::block_to_render(surface, f64::from(block.y()) + dy).as_vec3();
                pos += pos.normalize_or_zero() * e;
                LineVertex {
                    pos: pos.to_array(),
                    color: c,
                }
            };
            let verts = [
                // bottom
                p(0.0, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(1.0, 0.0, 1.0),
                p(1.0, 0.0, 1.0),
                p(0.0, 0.0, 1.0),
                p(0.0, 0.0, 1.0),
                p(0.0, 0.0, 0.0),
                // top
                p(0.0, 1.0, 0.0),
                p(1.0, 1.0, 0.0),
                p(1.0, 1.0, 0.0),
                p(1.0, 1.0, 1.0),
                p(1.0, 1.0, 1.0),
                p(0.0, 1.0, 1.0),
                p(0.0, 1.0, 1.0),
                p(0.0, 1.0, 0.0),
                // pillars
                p(0.0, 0.0, 0.0),
                p(0.0, 1.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(1.0, 1.0, 0.0),
                p(1.0, 0.0, 1.0),
                p(1.0, 1.0, 1.0),
                p(0.0, 0.0, 1.0),
                p(0.0, 1.0, 1.0),
            ];
            self.queue
                .write_buffer(&self.outline_buf, 0, bytemuck::cast_slice(&verts));
        }

        // The swapchain image is acquired as late as possible (just
        // before the composite pass) — acquiring here used to block on
        // vsync while the whole shadow/world encode still lay ahead.
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // Shadow pass: opaque terrain depth from the sun's POV, once per cascade
        // into its own layer. No color target. Every loaded chunk is a potential
        // caster (occluders behind the camera still shadow what's in view), so
        // this pass is range-culled per cascade rather than frustum-culled.
        //
        // Built once and shared by every pass below. Each pass used to walk the
        // whole loaded map itself — five scans of up to sixteen thousand entries
        // a frame at a wide view, to draw a few hundred.
        let visible: Vec<(&ChunkPos, &GpuChunk)> = self.chunks.iter().collect();
        for (c, &casc_radius) in CASCADE_RADII.iter().enumerate() {
            let mut sp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_layer_views[c],
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            sp.set_pipeline(&self.shadow_pipeline);
            sp.set_bind_group(
                0,
                &self.shadow_casc_bg,
                &[(c as u64 * CASCADE_STRIDE) as u32],
            );
            // Each cascade's ortho box only covers its radius around
            // the camera — chunks beyond it can't cast into this
            // layer, so don't draw them (the near cascade skips
            // nearly the whole loaded set).
            let reach = casc_radius + 30.0;
            for (_pos, gpu) in visible.iter().copied() {
                if !chunk_in_range(gpu, f.cam_pos, reach) {
                    continue;
                }
                if let Some(m) = &gpu.opaque {
                    sp.set_vertex_buffer(0, m.vbuf.slice(..));
                    sp.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    sp.draw_indexed(0..m.count, 0, 0..1);
                }
            }
        }

        // Point-light shadow passes: for each active light, render terrain
        // distance into its 6 cube faces (range-culled to the light's
        // reach). The cache makes static scenes free: a slot re-renders
        // only when its (key, epoch) changed since the cube was drawn.
        // Rebuilds are amortized under a global per-frame face budget —
        // several lights invalidating at once (a walk through a lit
        // camp) used to stack ~50 passes into one frame and blow the
        // vsync deadline; now the update spreads across frames, each
        // face serving its old picture until replaced.
        // One cube face per frame keeps shadowed point lights fully featured
        // while preventing a newly discovered torch (or a remeshed emitter
        // chunk) from stacking six terrain passes into one visible hitch. The
        // cache retains the previous complete cube until the replacement has
        // converged over the next six frames.
        let mut face_budget = 1usize;
        for (li, l) in f.point_lights.iter().take(MAX_PT_LIGHTS).enumerate() {
            // DDA marches the voxel occupancy grid for occlusion, so the
            // distance cube (and its tint companion) go unused — skip the whole
            // rebuild. (Glass tint via DDA is a follow-up; see occ ray.)
            if f.dda_shadow {
                break;
            }
            if !l.shadows {
                continue;
            }
            if self.pt_cached[li] == Some((l.key, l.epoch)) {
                self.pt_progress[li] = 0;
                continue;
            }
            if face_budget == 0 {
                continue;
            }
            while self.pt_progress[li] < 6 && face_budget > 0 {
                let face = self.pt_progress[li] as usize;
                self.pt_progress[li] += 1;
                face_budget -= 1;
                let layer = li * 6 + face;
                let mut pp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("pt-shadow"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.pt_face_views[layer],
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            // Clear "far" so untouched texels read as lit.
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 1.0e6,
                                g: 0.0,
                                b: 0.0,
                                a: 0.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.pt_shadow_depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            // Kept for the transmission pass below.
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pp.set_pipeline(&self.pt_shadow_pipeline);
                pp.set_bind_group(
                    0,
                    &self.pt_face_bg,
                    &[(layer as u32) * PT_FACE_STRIDE as u32],
                );
                for (_pos, gpu) in visible.iter().copied() {
                    if let Some(m) = &gpu.opaque {
                        if !chunk_in_range(gpu, l.pos, l.range) {
                            continue;
                        }
                        pp.set_vertex_buffer(0, m.vbuf.slice(..));
                        pp.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                        pp.draw_indexed(0..m.count, 0, 0..1);
                    }
                }
                drop(pp);
                // Stained transmission: glass in range multiplies its
                // color into the tint cube, gated by the stored opaque
                // depth so walls still win.
                let mut tp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("pt-tr"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.pt_tr_faces[layer],
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            // White = untinted; far alpha = no glass.
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 1.0,
                                g: 1.0,
                                b: 1.0,
                                a: 60000.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.pt_shadow_depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                tp.set_pipeline(&self.pt_tr_pipeline);
                tp.set_bind_group(
                    0,
                    &self.pt_face_bg,
                    &[(layer as u32) * PT_FACE_STRIDE as u32],
                );
                tp.set_bind_group(1, &self.atlas_bg, &[]);
                for (_pos, gpu) in visible.iter().copied() {
                    if let Some(m) = &gpu.water {
                        if !chunk_in_range(gpu, l.pos, l.range) {
                            continue;
                        }
                        tp.set_vertex_buffer(0, m.vbuf.slice(..));
                        tp.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                        tp.draw_indexed(0..m.count, 0, 0..1);
                    }
                }
            }
            // Only a fully rebuilt cube claims the cache; a partial one
            // resumes next frame from where it stopped.
            if self.pt_progress[li] >= 6 {
                self.pt_cached[li] = Some((l.key, l.epoch));
                self.pt_progress[li] = 0;
            }
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.post.hdr_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: self.sky_color[0] as f64,
                            g: self.sky_color[1] as f64,
                            b: self.sky_color[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_bind_group(0, &self.uniform_bg, &[]);
            pass.set_bind_group(1, &self.atlas_bg, &[]);
            pass.set_bind_group(2, &self.shadow_bg, &[]);

            // Background sky gradient (fills every pixel; terrain paints over).
            pass.set_pipeline(&self.sky_pipeline);
            pass.draw(0..3, 0..1);

            // Opaque terrain (frustum-culled)
            let planes = frustum_planes(&f.view_proj);
            pass.set_pipeline(&self.chunk_pipeline);
            for (_pos, gpu) in visible.iter().copied() {
                if !chunk_visible(&planes, gpu, f.cam_pos) {
                    continue;
                }
                if let Some(m) = &gpu.opaque {
                    pass.set_vertex_buffer(0, m.vbuf.slice(..));
                    pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..m.count, 0, 0..1);
                }
            }

            // Item entities (opaque mini-cubes)
            if !f.entity_idx.is_empty() {
                pass.set_vertex_buffer(0, self.entity_vbuf.buf.slice(..));
                pass.set_index_buffer(self.entity_ibuf.buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..f.entity_idx.len() as u32, 0, 0..1);
            }

            // Water
            pass.set_pipeline(&self.water_pipeline);
            for (_pos, gpu) in visible.iter().copied() {
                if !chunk_visible(&planes, gpu, f.cam_pos) {
                    continue;
                }
                if let Some(m) = &gpu.water {
                    pass.set_vertex_buffer(0, m.vbuf.slice(..));
                    pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..m.count, 0, 0..1);
                }
            }

            // Mining crack overlay (alpha-blended, reuses the water pipeline)
            if !f.overlay_idx.is_empty() {
                pass.set_vertex_buffer(0, self.overlay_vbuf.buf.slice(..));
                pass.set_index_buffer(self.overlay_ibuf.buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..f.overlay_idx.len() as u32, 0, 0..1);
            }

            // Targeted block outline
            if outline.is_some() {
                pass.set_pipeline(&self.line_world_pipeline);
                pass.set_vertex_buffer(0, self.outline_buf.slice(..));
                pass.draw(0..24, 0..1);
            }
        }

        // The first-person hand draws over the world (its own cleared depth)
        // into the same HDR target, so it tonemaps and blooms with the scene.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hand"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.post.hdr_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.uniform_bg, &[]);
            pass.set_bind_group(1, &self.atlas_bg, &[]);
            // The chunk pipeline shares a 3-group layout; the shadow group must
            // stay bound here even though the hand doesn't sample it.
            pass.set_bind_group(2, &self.shadow_bg, &[]);

            if !f.hand_idx.is_empty() {
                pass.set_pipeline(&self.chunk_pipeline);
                pass.set_vertex_buffer(0, self.hand_vbuf.buf.slice(..));
                pass.set_index_buffer(self.hand_ibuf.buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..f.hand_idx.len() as u32, 0, 0..1);
            }
        }

        // Bloom: isolate the HDR headroom, then separable blur at half res.
        // The bright pass clears bloom_a even with bloom off, so the composite
        // always samples a defined texture (times a zero intensity).
        let bloom_on = f.bloom > 0.0;
        // Night factor for the composite's cold grade: ramps 0 -> 1 as daylight
        // falls from ~dusk (0.30) to deep night (0.05), so the sunset's warm
        // sky is never cooled — only true night is.
        let night = ((0.30 - f.daylight) / 0.25).clamp(0.0, 1.0);
        self.queue.write_buffer(
            &self.post_params_buf,
            0,
            bytemuck::cast_slice(&[f.bloom.max(0.0), night, exposure(f.daylight), white_point()]),
        );
        {
            let mut bp = post_pass(&mut encoder, "bloom-bright", &self.post.bloom_a);
            if bloom_on {
                bp.set_pipeline(&self.bright_pipeline);
                bp.set_bind_group(0, &self.post.bright_bg, &[]);
                bp.set_bind_group(1, &self.post.bright_aux_bg, &[]);
                bp.set_bind_group(2, &self.post_params_bg, &[]);
                bp.draw(0..3, 0..1);
            }
        }
        if bloom_on {
            {
                let mut bp = post_pass(&mut encoder, "bloom-blur-h", &self.post.bloom_b);
                bp.set_pipeline(&self.blur_h_pipeline);
                bp.set_bind_group(0, &self.post.blur_h_bg, &[]);
                bp.draw(0..3, 0..1);
            }
            {
                let mut bp = post_pass(&mut encoder, "bloom-blur-v", &self.post.bloom_a);
                bp.set_pipeline(&self.blur_v_pipeline);
                bp.set_bind_group(0, &self.post.blur_v_bg, &[]);
                bp.draw(0..3, 0..1);
            }
        }

        // Composite HDR + bloom into the sRGB swapchain (the tonemap/encode).
        // Only now does the swapchain image matter: acquire it here so the
        // vsync wait overlaps all the encoding above instead of preceding it.
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.composite_and_ui(&mut encoder, &view, &f);

        let requested_shot = self
            .pending_screenshot
            .take()
            .map(|path| (path, self.pending_capture_metadata.take()));
        let shot = requested_shot.map(|(path, metadata)| {
            let w = self.config.width;
            let h = self.config.height;
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("screenshot-target"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let capture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.composite_and_ui(&mut encoder, &capture_view, &f);
            let bpr = (w * 4).div_ceil(256) * 256; // COPY_BYTES_PER_ROW_ALIGNMENT
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("screenshot"),
                size: (bpr * h) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buf,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(bpr),
                        rows_per_image: Some(h),
                    },
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
            let diagnostic = metadata
                .as_ref()
                .map(|_| self.encode_diagnostic_replay(&mut encoder, &f, &visible, outline));
            (path, buf, texture, w, h, bpr, metadata, diagnostic)
        });

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();

        if let Some((path, buf, _texture, w, h, bpr, metadata, diagnostic)) = shot {
            let bgra = matches!(
                self.config.format,
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
            );
            let slice = buf.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            let _ = self.device.poll(wgpu::PollType::Wait);
            let data = slice.get_mapped_range();
            let mut out = Vec::with_capacity((w * h * 3) as usize);
            for y in 0..h {
                let row = &data[(y * bpr) as usize..];
                for x in 0..w {
                    let p = &row[(x * 4) as usize..(x * 4 + 4) as usize];
                    if bgra {
                        out.extend_from_slice(&[p[2], p[1], p[0]]);
                    } else {
                        out.extend_from_slice(&[p[0], p[1], p[2]]);
                    }
                }
            }
            drop(data);
            buf.unmap();
            let header = format!("P6\n{w} {h}\n255\n");
            let mut file = header.into_bytes();
            file.extend_from_slice(&out);
            if let Some(metadata) = metadata {
                let paths = crate::visual_capture::evidence_paths(std::path::Path::new(&path))
                    .unwrap_or_else(|error| panic!("visual evidence path failed: {error}"));
                let diagnostic = diagnostic.expect("visual evidence diagnostic readback");
                let slice = diagnostic.buffer.slice(..);
                slice.map_async(wgpu::MapMode::Read, |_| {});
                self.device
                    .poll(wgpu::PollType::Wait)
                    .expect("wait for visual diagnostic readback");
                let data = slice.get_mapped_range();
                let wfd = crate::visual_capture::encode_wfd(
                    diagnostic.width,
                    diagnostic.height,
                    diagnostic.padded_bytes_per_row,
                    &data,
                )
                .unwrap_or_else(|error| panic!("encode visual diagnostic: {error}"));
                drop(data);
                diagnostic.buffer.unmap();
                drop(diagnostic.texture);

                crate::identity::atomic_write(&paths.ppm, &file, false)
                    .unwrap_or_else(|error| panic!("write visual color evidence: {error}"));
                crate::identity::atomic_write(&paths.diagnostic, &wfd, false)
                    .unwrap_or_else(|error| panic!("write visual diagnostic evidence: {error}"));
                let sidecar = crate::visual_capture::sidecar_text(
                    metadata,
                    &paths.ppm,
                    &file,
                    &paths.diagnostic,
                    &wfd,
                )
                .unwrap_or_else(|error| panic!("serialize visual evidence: {error}"));
                crate::identity::atomic_write(&paths.sidecar, sidecar.as_bytes(), false)
                    .unwrap_or_else(|error| panic!("write visual evidence identity: {error}"));
                println!(
                    "visual evidence saved: {}, {}, {}",
                    paths.ppm.display(),
                    paths.diagnostic.display(),
                    paths.sidecar.display()
                );
            } else if let Err(error) = std::fs::write(&path, file) {
                eprintln!("screenshot failed: {error}");
            } else {
                println!("screenshot saved: {path}");
            }
        }
        Ok(())
    }
}
