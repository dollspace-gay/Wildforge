//! Point-shadow cube resources, incremental cache, upload, and encoding.

use super::{
    CUBE_FACES, FrameInput, GpuChunk, MAX_PT_LIGHTS, PT_FACE_STRIDE, PointLight, chunk_in_range,
};
use crate::chunk::ChunkPos;
use glam::{Mat4, Vec3};

pub(super) struct PointShadowResources {
    pub(super) pt_shadow_pipeline: wgpu::RenderPipeline,
    pub(super) pt_tr_pipeline: wgpu::RenderPipeline,
    pub(super) pt_face_views: Vec<wgpu::TextureView>,
    pub(super) pt_tr_faces: Vec<wgpu::TextureView>,
    pub(super) pt_shadow_depth: wgpu::TextureView,
    pub(super) pt_face_buf: wgpu::Buffer,
    pub(super) pt_face_bg: wgpu::BindGroup,
}

pub(super) struct PointShadows {
    resources: PointShadowResources,
    cached: [Option<(u64, u64)>; MAX_PT_LIGHTS],
    progress: [u8; MAX_PT_LIGHTS],
}

impl PointShadows {
    pub(super) fn from_resources(resources: PointShadowResources) -> Self {
        Self {
            resources,
            cached: [None; MAX_PT_LIGHTS],
            progress: [0; MAX_PT_LIGHTS],
        }
    }

    pub(super) fn upload(&self, queue: &wgpu::Queue, lights: &[PointLight]) {
        // Per-face matrices for the point-shadow cube passes: a 90° perspective
        // per cube face, plus the light position for the distance write.
        let n_pt = lights.len().min(MAX_PT_LIGHTS);
        if n_pt > 0 {
            let stride = PT_FACE_STRIDE as usize;
            let mut data = vec![0u8; n_pt * 6 * stride];
            for (li, l) in lights.iter().take(MAX_PT_LIGHTS).enumerate() {
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
            queue.write_buffer(&self.resources.pt_face_buf, 0, &data);
        }
    }

    pub(super) fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        f: &FrameInput<'_>,
        visible: &[(&ChunkPos, &GpuChunk)],
        atlas: &wgpu::BindGroup,
    ) {
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
            if self.cached[li] == Some((l.key, l.epoch)) {
                self.progress[li] = 0;
                continue;
            }
            if face_budget == 0 {
                continue;
            }
            while self.progress[li] < 6 && face_budget > 0 {
                let face = self.progress[li] as usize;
                self.progress[li] += 1;
                face_budget -= 1;
                let layer = li * 6 + face;
                let mut pp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("pt-shadow"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.resources.pt_face_views[layer],
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
                        view: &self.resources.pt_shadow_depth,
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
                pp.set_pipeline(&self.resources.pt_shadow_pipeline);
                pp.set_bind_group(
                    0,
                    &self.resources.pt_face_bg,
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
                        view: &self.resources.pt_tr_faces[layer],
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
                        view: &self.resources.pt_shadow_depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                tp.set_pipeline(&self.resources.pt_tr_pipeline);
                tp.set_bind_group(
                    0,
                    &self.resources.pt_face_bg,
                    &[(layer as u32) * PT_FACE_STRIDE as u32],
                );
                tp.set_bind_group(1, atlas, &[]);
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
            if self.progress[li] >= 6 {
                self.cached[li] = Some((l.key, l.epoch));
                self.progress[li] = 0;
            }
        }
    }
}
