//! Uniforms GPU resource construction.

use crate::renderer::Uniforms;

pub(super) struct UniformBindings {
    pub(super) uniforms_buf: wgpu::Buffer,
    pub(super) uniform_bgl: wgpu::BindGroupLayout,
    pub(super) uniform_bg: wgpu::BindGroup,
}

pub(super) fn create(device: &wgpu::Device) -> UniformBindings {
        // Uniforms
        let uniforms_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms_buf.as_entire_binding(),
            }],
        });

    UniformBindings { uniforms_buf, uniform_bgl, uniform_bg }
}
