//! Shadow targets GPU resource construction.

use crate::renderer::{MAX_PT_LIGHTS, OCC_GRID, PT_SHADOW_RES, SHADOW_CASCADES, SHADOW_RES};

pub(super) struct ShadowTargets {
    pub(super) shadow_layer_views: Vec<wgpu::TextureView>,
    pub(super) shadow_view: wgpu::TextureView,
    pub(super) shadow_sampler: wgpu::Sampler,
    pub(super) pt_face_views: Vec<wgpu::TextureView>,
    pub(super) pt_tr_faces: Vec<wgpu::TextureView>,
    pub(super) pt_shadow_depth: wgpu::TextureView,
    pub(super) pt_cube_view: wgpu::TextureView,
    pub(super) pt_tr_view: wgpu::TextureView,
    pub(super) pt_sampler: wgpu::Sampler,
    pub(super) occ_tex: wgpu::Texture,
    pub(super) occ_view: wgpu::TextureView,
}

pub(super) fn create(device: &wgpu::Device) -> ShadowTargets {
    // Sun shadow map: a depth texture rendered from the light's POV and
    // sampled (with hardware PCF via a comparison sampler) in the main pass.
    let shadow_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shadow"),
        size: wgpu::Extent3d {
            width: SHADOW_RES,
            height: SHADOW_RES,
            depth_or_array_layers: SHADOW_CASCADES as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    // One array view for sampling all cascades, plus a single-layer view per
    // cascade to render into.
    let shadow_view = shadow_tex.create_view(&wgpu::TextureViewDescriptor {
        label: Some("shadow-sample"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let shadow_layer_views: Vec<wgpu::TextureView> = (0..SHADOW_CASCADES as u32)
        .map(|layer| {
            shadow_tex.create_view(&wgpu::TextureViewDescriptor {
                label: Some("shadow-layer"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("shadow-cmp"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        compare: Some(wgpu::CompareFunction::LessEqual),
        ..Default::default()
    });
    // Point-light distance cube maps: one R32Float cube (6 layers) per
    // light, packed into an array texture. Each fragment stores its
    // distance to the light; the main shader compares to decide occlusion.
    let pt_cube_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pt-cube"),
        size: wgpu::Extent3d {
            width: PT_SHADOW_RES,
            height: PT_SHADOW_RES,
            depth_or_array_layers: 6 * MAX_PT_LIGHTS as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let pt_cube_view = pt_cube_tex.create_view(&wgpu::TextureViewDescriptor {
        label: Some("pt-cube-sample"),
        dimension: Some(wgpu::TextureViewDimension::CubeArray),
        ..Default::default()
    });
    let pt_tr_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pt-tr"),
        size: wgpu::Extent3d {
            width: PT_SHADOW_RES,
            height: PT_SHADOW_RES,
            depth_or_array_layers: 6 * MAX_PT_LIGHTS as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let pt_tr_view = pt_tr_tex.create_view(&wgpu::TextureViewDescriptor {
        label: Some("pt-tr-sample"),
        dimension: Some(wgpu::TextureViewDimension::CubeArray),
        ..Default::default()
    });
    let pt_tr_faces: Vec<wgpu::TextureView> = (0..6 * MAX_PT_LIGHTS as u32)
        .map(|layer| {
            pt_tr_tex.create_view(&wgpu::TextureViewDescriptor {
                label: Some("pt-tr-face"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    let pt_face_views: Vec<wgpu::TextureView> = (0..6 * MAX_PT_LIGHTS as u32)
        .map(|layer| {
            pt_cube_tex.create_view(&wgpu::TextureViewDescriptor {
                label: Some("pt-face"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    let pt_shadow_depth = device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("pt-shadow-depth"),
            size: wgpu::Extent3d {
                width: PT_SHADOW_RES,
                height: PT_SHADOW_RES,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default());
    let pt_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("pt-cube-smp"),
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    // Voxel occupancy grid for DDA point-light shadows: a cube of the world
    // around the camera, 1 byte per cell (1 = opaque). Refilled by the game
    // when the camera crosses a grid step.
    let occ_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("occ-grid"),
        size: wgpu::Extent3d {
            width: OCC_GRID as u32,
            height: OCC_GRID as u32,
            depth_or_array_layers: OCC_GRID as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::Rgba8Uint,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let occ_view = occ_tex.create_view(&wgpu::TextureViewDescriptor::default());

    ShadowTargets {
        shadow_layer_views,
        shadow_view,
        shadow_sampler,
        pt_face_views,
        pt_tr_faces,
        pt_shadow_depth,
        pt_cube_view,
        pt_tr_view,
        pt_sampler,
        occ_tex,
        occ_view,
    }
}
