//! Cascade GPU resource construction.

use crate::renderer::{CASCADE_STRIDE, SHADOW_CASCADES};

pub(super) struct CascadeBindings {
    pub(super) csm_shader: wgpu::ShaderModule,
    pub(super) shadow_casc_buf: wgpu::Buffer,
    pub(super) shadow_casc_bg: wgpu::BindGroup,
    pub(super) shadow_layout: wgpu::PipelineLayout,
}

pub(super) fn create(device: &wgpu::Device) -> CascadeBindings {
    // Depth-only cascade shader: one matrix per pass, selected by dynamic
    // offset. Separate module because its group-0 uniform is just the
    // cascade's view_proj, not the full scene Uniforms.
    let csm_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("csm-shader"),
        source: wgpu::ShaderSource::Wgsl(crate::shader::CASCADE_SHADOW.into()),
    });
    // Per-cascade light_vp, one 256-aligned slot each, addressed by offset.
    let shadow_casc_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("shadow-cascade"),
        size: CASCADE_STRIDE * SHADOW_CASCADES as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let shadow_casc_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("shadow-cascade-bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: core::num::NonZeroU64::new(64),
            },
            count: None,
        }],
    });
    let shadow_casc_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("shadow-cascade-bg"),
        layout: &shadow_casc_bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &shadow_casc_buf,
                offset: 0,
                size: core::num::NonZeroU64::new(64),
            }),
        }],
    });
    // The depth-only shadow pass binds just the per-cascade matrix.
    let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("shadow-layout"),
        bind_group_layouts: &[&shadow_casc_bgl],
        push_constant_ranges: &[],
    });

    CascadeBindings {
        csm_shader,
        shadow_casc_buf,
        shadow_casc_bg,
        shadow_layout,
    }
}
