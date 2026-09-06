//! Diagnostic stage of GPU frame encoding.

use super::DiagnosticReadback;
use crate::chunk::ChunkPos;
use crate::renderer::{FrameInput, GpuChunk, Renderer, chunk_visible, frustum_planes};

impl Renderer {
    pub(in crate::renderer) fn encode_diagnostic_replay(
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
}
