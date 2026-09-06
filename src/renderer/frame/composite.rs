//! Composite stage of GPU frame encoding.

use crate::renderer::{Renderer, FrameInput};

impl Renderer {
    pub(in crate::renderer) fn composite_and_ui(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        f: &FrameInput<'_>,
    ) {
        self.post.composite(encoder, target);

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

}
