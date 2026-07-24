//! Ordered shadow, world, viewmodel, post-processing, and UI frame passes.

use super::*;

impl Renderer {
    pub fn render(&mut self, f: FrameInput) -> Result<(), wgpu::SurfaceError> {
        let outline = f.outline;

        // Sun light-space matrix: an orthographic box centered near the camera,
        // looking from the sun toward that center. Covers the near field; beyond
        // its radius the shader treats fragments as lit (shadows fade out).
        let light_vp = {
            let radius = 90.0f32;
            let dist = 160.0f32;
            let center = f.cam_pos;
            let eye = center + f.sun_dir * dist;
            let up = if f.sun_dir.y.abs() > 0.95 {
                Vec3::Z
            } else {
                Vec3::Y
            };
            let view = glam::camera::rh::view::look_at_mat4(eye, center, up);
            let proj = glam::camera::rh::proj::directx::orthographic(
                -radius,
                radius,
                -radius,
                radius,
                1.0,
                dist + radius * 2.0,
            );
            proj * view
        };

        let inv_view_proj = f.view_proj.inverse();
        let uniforms = Uniforms {
            view_proj: f.view_proj.to_cols_array_2d(),
            cam: [f.cam_pos.x, f.cam_pos.y, f.cam_pos.z, f.fog_dist],
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
            sun_col: [f.sun_col.x, f.sun_col.y, f.sun_col.z, 0.0],
            amb_col: [f.amb_col.x, f.amb_col.y, f.amb_col.z, f.ambient_floor],
            light_vp: light_vp.to_cols_array_2d(),
            pt_count: [f.point_lights.len().min(MAX_PT_LIGHTS) as u32, 0, 0, 0],
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
        };
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

        if let Some((bx, by, bz)) = outline {
            let e = 0.003f32;
            let (x0, y0, z0) = (bx as f32 - e, by as f32 - e, bz as f32 - e);
            let (x1, y1, z1) = (
                bx as f32 + 1.0 + e,
                by as f32 + 1.0 + e,
                bz as f32 + 1.0 + e,
            );
            let c = [0.05, 0.05, 0.05];
            let p = |x: f32, y: f32, z: f32| LineVertex {
                pos: [x, y, z],
                color: c,
            };
            let verts = [
                // bottom
                p(x0, y0, z0),
                p(x1, y0, z0),
                p(x1, y0, z0),
                p(x1, y0, z1),
                p(x1, y0, z1),
                p(x0, y0, z1),
                p(x0, y0, z1),
                p(x0, y0, z0),
                // top
                p(x0, y1, z0),
                p(x1, y1, z0),
                p(x1, y1, z0),
                p(x1, y1, z1),
                p(x1, y1, z1),
                p(x0, y1, z1),
                p(x0, y1, z1),
                p(x0, y1, z0),
                // pillars
                p(x0, y0, z0),
                p(x0, y1, z0),
                p(x1, y0, z0),
                p(x1, y1, z0),
                p(x1, y0, z1),
                p(x1, y1, z1),
                p(x0, y0, z1),
                p(x0, y1, z1),
            ];
            self.queue
                .write_buffer(&self.outline_buf, 0, bytemuck::cast_slice(&verts));
        }

        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // Shadow pass: opaque terrain depth from the sun's POV. No color target.
        // Every loaded chunk is a potential caster (occluders behind the camera
        // still shadow what's in view), so this pass is not frustum-culled.
        {
            let mut sp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
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
            sp.set_bind_group(0, &self.uniform_bg, &[]);
            // The ortho box only covers ~90 blocks around the camera —
            // chunks beyond it can't cast into the map, so don't draw
            // them (with a big view distance this was most of them).
            for (pos, gpu) in &self.chunks {
                if !chunk_in_range(*pos, f.cam_pos, 120.0) {
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
        let mut face_budget = 6usize;
        for (li, l) in f.point_lights.iter().take(MAX_PT_LIGHTS).enumerate() {
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
                for (pos, gpu) in &self.chunks {
                    if let Some(m) = &gpu.opaque {
                        if !chunk_in_range(*pos, l.pos, l.range) {
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
                for (pos, gpu) in &self.chunks {
                    if let Some(m) = &gpu.water {
                        if !chunk_in_range(*pos, l.pos, l.range) {
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
            for (pos, gpu) in &self.chunks {
                if !chunk_visible(&planes, *pos) {
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
            for (pos, gpu) in &self.chunks {
                if !chunk_visible(&planes, *pos) {
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
            bytemuck::cast_slice(&[f.bloom.max(0.0), night, 0.0, 0.0]),
        );
        {
            let mut bp = post_pass(&mut encoder, "bloom-bright", &self.post.bloom_a);
            if bloom_on {
                bp.set_pipeline(&self.bright_pipeline);
                bp.set_bind_group(0, &self.post.bright_bg, &[]);
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
        {
            let mut pass = post_pass(&mut encoder, "composite", &view);
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &self.post.composite_scene_bg, &[]);
            pass.set_bind_group(1, &self.post.composite_bloom_bg, &[]);
            pass.set_bind_group(2, &self.post_params_bg, &[]);
            pass.draw(0..3, 0..1);
        }

        // Flat UI over the tonemapped image: crosshair, then the 2D batch.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui"),
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

        let shot = self.pending_screenshot.take().map(|path| {
            let w = self.config.width;
            let h = self.config.height;
            let bpr = (w * 4).div_ceil(256) * 256; // COPY_BYTES_PER_ROW_ALIGNMENT
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("screenshot"),
                size: (bpr * h) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &frame.texture,
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
            (path, buf, w, h, bpr)
        });

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();

        if let Some((path, buf, w, h, bpr)) = shot {
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
            if let Err(e) = std::fs::write(&path, file) {
                eprintln!("screenshot failed: {e}");
            } else {
                println!("screenshot saved: {path}");
            }
        }
        Ok(())
    }
}
