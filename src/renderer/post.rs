//! HDR targets, bloom passes, composite, and depth-target creation.

use super::*;

/// Size-dependent post targets: the HDR scene buffer, the two half-res bloom
/// ping-pong buffers, and the bind groups wiring them through the post passes.
/// Rebuilt whenever the surface resizes.
pub(super) struct PostTargets {
    pub(super) hdr_view: wgpu::TextureView,
    pub(super) bloom_a: wgpu::TextureView,
    pub(super) bloom_b: wgpu::TextureView,
    pub(super) bright_bg: wgpu::BindGroup, // hdr  -> bloom_a
    pub(super) blur_h_bg: wgpu::BindGroup, // bloom_a -> bloom_b
    pub(super) blur_v_bg: wgpu::BindGroup, // bloom_b -> bloom_a
    pub(super) composite_scene_bg: wgpu::BindGroup, // hdr  (group 0)
    pub(super) composite_bloom_bg: wgpu::BindGroup, // bloom_a (group 1)
}

pub(super) fn create_post_targets(
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
        hdr_view,
        bloom_a,
        bloom_b,
    }
}

/// A fullscreen post pass that clears its target to black (the fullscreen
/// triangle then overwrites every pixel). Callers bind a pipeline and draw.
pub(super) fn post_pass<'e>(
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
