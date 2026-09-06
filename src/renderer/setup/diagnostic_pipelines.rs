//! Capture-only diagnostic raster pipelines.

use super::raster::{
    RasterFactory, RasterSpec, depth_state, line_layout, ui_layout, vertex_layout,
};
use crate::renderer::DiagnosticPipelines;

pub(super) fn create(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    chunk_layout: &wgpu::PipelineLayout,
) -> DiagnosticPipelines {
    let vertex_layout = vertex_layout();
    let line_layout = line_layout();
    let ui_layout = ui_layout();
    let factory = RasterFactory {
        device,
        shader,
        layout: chunk_layout,
    };
    let screen_depth = || wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth32Float,
        depth_write_enabled: false,
        depth_compare: wgpu::CompareFunction::Always,
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    };
    DiagnosticPipelines {
        chunk: factory.create(RasterSpec {
            label: "diagnostic-chunk",
            vs: "vs_chunk",
            fs: "fs_diagnostic_chunk",
            vlayout: &vertex_layout,
            cull: Some(wgpu::Face::Back),
            topology: wgpu::PrimitiveTopology::TriangleList,
            depth_stencil: Some(depth_state(true)),
            blend: None,
            target: wgpu::TextureFormat::Rgba16Uint,
        }),
        chunk_overlay: factory.create(RasterSpec {
            label: "diagnostic-chunk-overlay",
            vs: "vs_chunk",
            fs: "fs_diagnostic_chunk_overlay",
            vlayout: &vertex_layout,
            cull: None,
            topology: wgpu::PrimitiveTopology::TriangleList,
            depth_stencil: Some(depth_state(true)),
            blend: None,
            target: wgpu::TextureFormat::Rgba16Uint,
        }),
        water: factory.create(RasterSpec {
            label: "diagnostic-water",
            vs: "vs_chunk",
            fs: "fs_diagnostic_water",
            vlayout: &vertex_layout,
            cull: None,
            topology: wgpu::PrimitiveTopology::TriangleList,
            depth_stencil: Some(depth_state(true)),
            blend: None,
            target: wgpu::TextureFormat::Rgba16Uint,
        }),
        line_world: factory.create(RasterSpec {
            label: "diagnostic-line-world",
            vs: "vs_line_world",
            fs: "fs_diagnostic_line",
            vlayout: &line_layout,
            cull: None,
            topology: wgpu::PrimitiveTopology::LineList,
            depth_stencil: Some(depth_state(false)),
            blend: None,
            target: wgpu::TextureFormat::Rgba16Uint,
        }),
        line_screen: factory.create(RasterSpec {
            label: "diagnostic-line-screen",
            vs: "vs_line_screen",
            fs: "fs_diagnostic_line",
            vlayout: &line_layout,
            cull: None,
            topology: wgpu::PrimitiveTopology::LineList,
            depth_stencil: Some(screen_depth()),
            blend: None,
            target: wgpu::TextureFormat::Rgba16Uint,
        }),
        ui: factory.create(RasterSpec {
            label: "diagnostic-ui",
            vs: "vs_ui",
            fs: "fs_diagnostic_ui",
            vlayout: &ui_layout,
            cull: None,
            topology: wgpu::PrimitiveTopology::TriangleList,
            depth_stencil: Some(screen_depth()),
            blend: None,
            target: wgpu::TextureFormat::Rgba16Uint,
        }),
    }
}
