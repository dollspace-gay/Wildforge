//! Size-independent post pipelines and their initial targets.

use super::{PostProcess, create_post_targets};
use crate::renderer::HDR_FORMAT;

impl PostProcess {
    pub(in crate::renderer) fn new(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> Self {
        // ---- HDR + bloom post chain ----
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../post.wgsl").into()),
        });
        // group 0: a sampled input texture + the shared linear sampler.
        let post_in_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post-in-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        // group 1: a second sampled texture (composite's bloom input).
        let post_tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post-tex-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        // group 2: composite params (bloom intensity).
        let post_params_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post-params-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let post_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("post-linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let post_params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post-params"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let post_params_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post-params-bg"),
            layout: &post_params_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: post_params_buf.as_entire_binding(),
            }],
        });
        let bloom_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bloom-layout"),
            bind_group_layouts: &[&post_in_bgl],
            push_constant_ranges: &[],
        });
        let composite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("composite-layout"),
            bind_group_layouts: &[&post_in_bgl, &post_tex_bgl, &post_params_bgl],
            push_constant_ranges: &[],
        });
        let make_post =
            |label: &str, layout: &wgpu::PipelineLayout, fs: &str, target: wgpu::TextureFormat| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(layout),
                    vertex: wgpu::VertexState {
                        module: &post_shader,
                        entry_point: Some("vs_fullscreen"),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &post_shader,
                        entry_point: Some(fs),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: target,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        unclipped_depth: false,
                        conservative: false,
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                })
            };
        // The bright pass reads the composite params too: its threshold is in
        // exposed terms, so it needs the same exposure the composite applies.
        let bright_pipeline = make_post("bright", &composite_layout, "fs_bright", HDR_FORMAT);
        let blur_h_pipeline = make_post("blur-h", &bloom_layout, "fs_blur_h", HDR_FORMAT);
        let blur_v_pipeline = make_post("blur-v", &bloom_layout, "fs_blur_v", HDR_FORMAT);
        let composite_pipeline = make_post(
            "composite",
            &composite_layout,
            "fs_composite",
            config.format,
        );
        let post =
            create_post_targets(device, config, &post_in_bgl, &post_tex_bgl, &post_sampler);

        Self {
            post_in_bgl,
            post_tex_bgl,
            post_sampler,
            post_params_buf,
            post_params_bg,
            bright_pipeline,
            blur_h_pipeline,
            blur_v_pipeline,
            composite_pipeline,
            targets: post,
        }
    }
}
