//! HDR targets, bloom passes, composite, and depth-target creation.

use super::HDR_FORMAT;

mod setup;

/// One lifetime for post pipelines, exposure parameters, and size-dependent
/// attachments. Only the HDR scene view crosses into the main frame encoder.
pub(super) struct PostProcess {
    post_in_bgl: wgpu::BindGroupLayout,
    post_tex_bgl: wgpu::BindGroupLayout,
    post_sampler: wgpu::Sampler,
    post_params_buf: wgpu::Buffer,
    post_params_bg: wgpu::BindGroup,
    bright_pipeline: wgpu::RenderPipeline,
    blur_h_pipeline: wgpu::RenderPipeline,
    blur_v_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    targets: PostTargets,
}

/// Size-dependent post targets: the HDR scene buffer, the two half-res bloom
/// ping-pong buffers, and the bind groups wiring them through the post passes.
/// Rebuilt whenever the surface resizes.
struct PostTargets {
    hdr_view: wgpu::TextureView,
    bloom_a: wgpu::TextureView,
    bloom_b: wgpu::TextureView,
    bright_bg: wgpu::BindGroup,          // hdr  -> bloom_a
    blur_h_bg: wgpu::BindGroup,          // bloom_a -> bloom_b
    blur_v_bg: wgpu::BindGroup,          // bloom_b -> bloom_a
    composite_scene_bg: wgpu::BindGroup, // hdr  (group 0)
    composite_bloom_bg: wgpu::BindGroup, // bloom_a (group 1)
    /// Filler for group 1 of the bright pass, which shares the composite's
    /// layout so it can read the exposure at group 2 but never touches group 1.
    /// Points at the scene rather than at bloom_a, because bloom_a is what that
    /// pass is writing and a pass may not sample its own target.
    bright_aux_bg: wgpu::BindGroup,
}

fn create_post_targets(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
    in_bgl: &wgpu::BindGroupLayout,
    tex_bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
) -> PostTargets {
    let target = |label: &str, w: u32, h: u32| -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: w.max(1),
                    height: h.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: HDR_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    };
    let hdr_view = target("hdr-scene", config.width, config.height);
    // Bloom runs at half resolution: cheaper, and a wider effective blur.
    let (bw, bh) = (config.width / 2, config.height / 2);
    let bloom_a = target("bloom-a", bw, bh);
    let bloom_b = target("bloom-b", bw, bh);

    let in_bg = |label: &str, view: &wgpu::TextureView| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: in_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    };
    let tex_bg = |label: &str, view: &wgpu::TextureView| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: tex_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            }],
        })
    };
    PostTargets {
        bright_bg: in_bg("bright-bg", &hdr_view),
        blur_h_bg: in_bg("blur-h-bg", &bloom_a),
        blur_v_bg: in_bg("blur-v-bg", &bloom_b),
        composite_scene_bg: in_bg("composite-scene-bg", &hdr_view),
        composite_bloom_bg: tex_bg("composite-bloom-bg", &bloom_a),
        bright_aux_bg: tex_bg("bright-aux-bg", &hdr_view),
        hdr_view,
        bloom_a,
        bloom_b,
    }
}

/// A fullscreen post pass that clears its target to black (the fullscreen
/// triangle then overwrites every pixel). Callers bind a pipeline and draw.
fn post_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    label: &str,
    target: &'e wgpu::TextureView,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    })
}

pub(super) fn create_depth(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

impl PostProcess {
    pub(super) fn scene_view(&self) -> &wgpu::TextureView {
        &self.targets.hdr_view
    }

    pub(super) fn resize(&mut self, device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) {
        self.targets = create_post_targets(
            device,
            config,
            &self.post_in_bgl,
            &self.post_tex_bgl,
            &self.post_sampler,
        );
    }

    pub(super) fn bloom(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        bloom: f32,
        daylight: f32,
    ) {
        // Bloom: isolate the HDR headroom, then separable blur at half res.
        // The bright pass clears bloom_a even with bloom off, so the composite
        // always samples a defined texture (times a zero intensity).
        let bloom_on = bloom > 0.0;
        // Night factor for the composite's cold grade: ramps 0 -> 1 as daylight
        // falls from ~dusk (0.30) to deep night (0.05), so the sunset's warm
        // sky is never cooled — only true night is.
        let night = ((0.30 - daylight) / 0.25).clamp(0.0, 1.0);
        queue.write_buffer(
            &self.post_params_buf,
            0,
            bytemuck::cast_slice(&[bloom.max(0.0), night, exposure(daylight), white_point()]),
        );
        {
            let mut bp = post_pass(encoder, "bloom-bright", &self.targets.bloom_a);
            if bloom_on {
                bp.set_pipeline(&self.bright_pipeline);
                bp.set_bind_group(0, &self.targets.bright_bg, &[]);
                bp.set_bind_group(1, &self.targets.bright_aux_bg, &[]);
                bp.set_bind_group(2, &self.post_params_bg, &[]);
                bp.draw(0..3, 0..1);
            }
        }
        if bloom_on {
            {
                let mut bp = post_pass(encoder, "bloom-blur-h", &self.targets.bloom_b);
                bp.set_pipeline(&self.blur_h_pipeline);
                bp.set_bind_group(0, &self.targets.blur_h_bg, &[]);
                bp.draw(0..3, 0..1);
            }
            {
                let mut bp = post_pass(encoder, "bloom-blur-v", &self.targets.bloom_a);
                bp.set_pipeline(&self.blur_v_pipeline);
                bp.set_bind_group(0, &self.targets.blur_v_bg, &[]);
                bp.draw(0..3, 0..1);
            }
        }
    }

    pub(super) fn composite(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        {
            let mut pass = post_pass(encoder, "composite", target);
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &self.targets.composite_scene_bg, &[]);
            pass.set_bind_group(1, &self.targets.composite_bloom_bg, &[]);
            pass.set_bind_group(2, &self.post_params_bg, &[]);
            pass.draw(0..3, 0..1);
        }
    }
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
