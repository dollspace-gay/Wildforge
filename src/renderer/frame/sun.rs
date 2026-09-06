//! Sun stage of GPU frame encoding.

use crate::renderer::{Renderer, FrameInput, GpuChunk, CASCADE_RADII, CASCADE_STRIDE, chunk_in_range};
use crate::chunk::ChunkPos;

impl Renderer {
    pub(in crate::renderer) fn encode_sun_shadows(&self, encoder: &mut wgpu::CommandEncoder, f: &FrameInput<'_>, visible: &[(&ChunkPos, &GpuChunk)]) {
        // Shadow pass: opaque terrain depth from the sun's POV, once per cascade
        // into its own layer. No color target. Every loaded chunk is a potential
        // caster (occluders behind the camera still shadow what's in view), so
        // this pass is range-culled per cascade rather than frustum-culled.
        //
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

    }
}
