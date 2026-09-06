//! Point uniforms GPU resource construction.

use crate::renderer::{MAX_PT_LIGHTS, PT_FACE_STRIDE};

pub(super) struct PointUniforms {
    pub(super) pt_face_buf: wgpu::Buffer,
    pub(super) pt_face_bgl: wgpu::BindGroupLayout,
    pub(super) pt_face_bg: wgpu::BindGroup,
}

pub(super) fn create(device: &wgpu::Device) -> PointUniforms {
    // Per-face uniform for the point-shadow pass: {view_proj, light_pos},
    // one slot per cube face, addressed by dynamic offset.
    let pt_face_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("pt-face"),
        size: PT_FACE_STRIDE * 6 * MAX_PT_LIGHTS as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let pt_face_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("pt-face-bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: core::num::NonZeroU64::new(80),
            },
            count: None,
        }],
    });
    let pt_face_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("pt-face-bg"),
        layout: &pt_face_bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &pt_face_buf,
                offset: 0,
                size: core::num::NonZeroU64::new(PT_FACE_STRIDE),
            }),
        }],
    });

    PointUniforms {
        pt_face_buf,
        pt_face_bgl,
        pt_face_bg,
    }
}
