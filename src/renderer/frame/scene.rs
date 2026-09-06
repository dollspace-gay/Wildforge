//! Scene stage of GPU frame encoding.

use crate::chunk::ChunkPos;
use crate::renderer::{FrameInput, GpuChunk, Renderer, chunk_visible, frustum_planes};

impl Renderer {
    pub(in crate::renderer) fn encode_world(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        f: &FrameInput<'_>,
        visible: &[(&ChunkPos, &GpuChunk)],
    ) {
        let outline = f.outline;
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.post.scene_view(),
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
    }
    pub(in crate::renderer) fn encode_hand(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        f: &FrameInput<'_>,
    ) {
        // The first-person hand draws over the world (its own cleared depth)
        // into the same HDR target, so it tonemaps and blooms with the scene.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hand"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.post.scene_view(),
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
    }
}
