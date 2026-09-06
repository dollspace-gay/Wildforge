//! Atlas GPU resource construction.

use crate::renderer::resources::{upload_atlas, atlas_bind_group};

pub(super) struct AtlasBindings {
    pub(super) atlas_bg: wgpu::BindGroup,
    pub(super) atlas_bgl: wgpu::BindGroupLayout,
    pub(super) sampler: wgpu::Sampler,
}

pub(super) fn create(device: &wgpu::Device, queue: &wgpu::Queue, atlas_data: &[u8], atlas_material: &[u8], atlas_normal: &[u8], atlas_px: u32) -> AtlasBindings {
        // Texture atlas
        let atlas_view = upload_atlas(device, queue, atlas_data, atlas_px, true, "atlas");
        let material_view = upload_atlas(
            device,
            queue,
            atlas_material,
            atlas_px,
            false,
            "atlas-material",
        );
        let normal_view = upload_atlas(
            device,
            queue,
            atlas_normal,
            atlas_px,
            false,
            "atlas-normal",
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let atlas_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let atlas_bg = atlas_bind_group(
            device,
            &atlas_bgl,
            &atlas_view,
            &material_view,
            &normal_view,
            &sampler,
        );

    AtlasBindings { atlas_bg, atlas_bgl, sampler }
}
