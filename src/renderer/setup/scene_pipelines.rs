//! Scene pipelines GPU resource construction.

use crate::renderer::HDR_FORMAT;
use super::raster::{RasterFactory, RasterSpec, vertex_layout, line_layout, ui_layout, depth_state};

pub(super) struct ScenePipelines {
    pub(super) chunk_pipeline: wgpu::RenderPipeline,
    pub(super) sky_pipeline: wgpu::RenderPipeline,
    pub(super) water_pipeline: wgpu::RenderPipeline,
    pub(super) line_world_pipeline: wgpu::RenderPipeline,
    pub(super) line_screen_pipeline: wgpu::RenderPipeline,
    pub(super) ui_pipeline: wgpu::RenderPipeline,
}

pub(super) fn create(device: &wgpu::Device, shader: &wgpu::ShaderModule, chunk_layout: &wgpu::PipelineLayout, format: wgpu::TextureFormat) -> ScenePipelines {
        let vertex_layout = vertex_layout();
        let line_layout = line_layout();
        let ui_layout = ui_layout();
        let factory = RasterFactory { device, shader, layout: chunk_layout };
        // Scene pipelines render into the linear HDR target; the crosshair and
        // UI draw straight to the swapchain after the composite.
        let chunk_pipeline = factory.create(RasterSpec {
            label: "chunk",
            vs: "vs_chunk",
            fs: "fs_chunk",
            vlayout: &vertex_layout,
            blend: None,
            cull: Some(wgpu::Face::Back),
            topology: wgpu::PrimitiveTopology::TriangleList,
            depth_stencil: Some(depth_state(true)),
            target: HDR_FORMAT,
        });
        // Background sky: a fullscreen triangle (no vertex buffer) drawn first
        // in the main pass. Depth-write off + compare Always fills every pixel
        // at the far plane; terrain then paints over it. Shares the chunk
        // layout so it reads the same uniform group.
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky"),
            layout: Some(chunk_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_sky"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_sky"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let water_pipeline = factory.create(RasterSpec {
            label: "water",
            vs: "vs_chunk",
            fs: "fs_water",
            vlayout: &vertex_layout,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            cull: None,
            topology: wgpu::PrimitiveTopology::TriangleList,
            depth_stencil: Some(depth_state(false)),
            target: HDR_FORMAT,
        });
        let line_world_pipeline = factory.create(RasterSpec {
            label: "line-world",
            vs: "vs_line_world",
            fs: "fs_line",
            vlayout: &line_layout,
            blend: None,
            cull: None,
            topology: wgpu::PrimitiveTopology::LineList,
            depth_stencil: Some(depth_state(false)),
            target: HDR_FORMAT,
        });
        let line_screen_pipeline = factory.create(RasterSpec {
            label: "line-screen",
            vs: "vs_line_screen",
            fs: "fs_line",
            vlayout: &line_layout,
            blend: None,
            cull: None,
            topology: wgpu::PrimitiveTopology::LineList,
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            target: format,
        });
        let ui_pipeline = factory.create(RasterSpec {
            label: "ui",
            vs: "vs_ui",
            fs: "fs_ui",
            vlayout: &ui_layout,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            cull: None,
            topology: wgpu::PrimitiveTopology::TriangleList,
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            target: format,
        });

    ScenePipelines { chunk_pipeline, sky_pipeline, water_pipeline, line_world_pipeline, line_screen_pipeline, ui_pipeline }
}
