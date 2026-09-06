//! Surface, device, pipeline, shadow-map, and post-chain construction.

use super::{DynBuf, LineVertex, PostProcess, Renderer, create_depth};
use std::{collections::HashMap, sync::Arc};
use winit::window::Window;

impl Renderer {
    pub async fn new(
        window: Arc<Window>,
        atlas_data: Vec<u8>,
        atlas_material: Vec<u8>,
        atlas_normal: Vec<u8>,
        atlas_px: u32,
    ) -> Renderer {
        let super::device::DeviceState {
            surface,
            device,
            queue,
            config,
            adapter_name,
            adapter_backend,
            adapter_hardware,
        } = super::device::DeviceState::new(window).await;
        let depth = create_depth(&device, &config);

        let uniforms::UniformBindings {
            uniforms_buf,
            uniform_bgl,
            uniform_bg,
        } = uniforms::create(&device);
        let atlas::AtlasBindings {
            atlas_bg,
            atlas_bgl,
            sampler,
        } = atlas::create(
            &device,
            &queue,
            &atlas_data,
            &atlas_material,
            &atlas_normal,
            atlas_px,
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(crate::shader::WORLD.into()),
        });
        // The point-shadow pass has its own group-0 uniform (per-face matrix +
        // light position), so it lives in a separate module.
        let pt_shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pt-shadow-shader"),
            source: wgpu::ShaderSource::Wgsl(crate::shader::POINT_SHADOW.into()),
        });

        let targets = shadow_targets::create(&device);
        let shadow_bindings::ShadowBindings {
            shadow_bgl,
            shadow_bg,
        } = shadow_bindings::create(&device, &targets);
        let shadow_targets::ShadowTargets {
            shadow_layer_views,
            pt_face_views,
            pt_tr_faces,
            pt_shadow_depth,
            occ_tex,
            ..
        } = targets;
        let point_uniforms::PointUniforms {
            pt_face_buf,
            pt_face_bgl,
            pt_face_bg,
        } = point_uniforms::create(&device);
        // Main-pass pipelines bind [uniforms, atlas, shadow]. Line/UI pipelines
        // share this layout and simply ignore the shadow group.
        let chunk_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&uniform_bgl, &atlas_bgl, &shadow_bgl],
            push_constant_ranges: &[],
        });
        let cascade::CascadeBindings {
            csm_shader,
            shadow_casc_buf,
            shadow_casc_bg,
            shadow_layout,
        } = cascade::create(&device);
        let scene_pipelines::ScenePipelines {
            chunk_pipeline,
            sky_pipeline,
            water_pipeline,
            line_world_pipeline,
            line_screen_pipeline,
            ui_pipeline,
        } = scene_pipelines::create(&device, &shader, &chunk_layout, config.format);
        let diagnostic_pipelines = crate::visual_capture::evidence_enabled()
            .then(|| diagnostic_pipelines::create(&device, &shader, &chunk_layout));
        let shadow_pipelines::ShadowPipelines {
            shadow_pipeline,
            pt_shadow_pipeline,
            pt_tr_pipeline,
        } = shadow_pipelines::create(
            &device,
            &shadow_layout,
            &csm_shader,
            &pt_shadow_shader,
            &pt_face_bgl,
            &atlas_bgl,
        );
        let post = PostProcess::new(&device, &config);

        let outline_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("outline"),
            size: (24 * std::mem::size_of::<LineVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let crosshair_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("crosshair"),
            size: (4 * std::mem::size_of::<LineVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let entity_vbuf = DynBuf::new(&device, wgpu::BufferUsages::VERTEX);
        let entity_ibuf = DynBuf::new(&device, wgpu::BufferUsages::INDEX);
        let overlay_vbuf = DynBuf::new(&device, wgpu::BufferUsages::VERTEX);
        let overlay_ibuf = DynBuf::new(&device, wgpu::BufferUsages::INDEX);
        let hand_vbuf = DynBuf::new(&device, wgpu::BufferUsages::VERTEX);
        let hand_ibuf = DynBuf::new(&device, wgpu::BufferUsages::INDEX);
        let ui_vbuf = DynBuf::new(&device, wgpu::BufferUsages::VERTEX);

        let mut r = Renderer {
            atlas_interior_base: 0,
            atlas_layer_params: [[0.0; 4]; crate::atlas::MAX_LAYERS as usize * 2],
            adapter_name,
            adapter_backend,
            adapter_hardware,
            surface,
            device,
            queue,
            config,
            depth,
            uniforms_buf,
            uniform_bg,
            atlas_bg,
            atlas_bgl,
            atlas_sampler: sampler,
            chunk_pipeline,
            sky_pipeline,
            water_pipeline,
            line_world_pipeline,
            line_screen_pipeline,
            ui_pipeline,
            diagnostic_pipelines,
            shadow_pipeline,
            shadow_layer_views,
            shadow_casc_buf,
            shadow_casc_bg,
            shadow_bg,
            occ_tex,
            point_shadows: super::point_shadows::PointShadows::from_resources(
                super::point_shadows::PointShadowResources {
                    pt_shadow_pipeline,
                    pt_tr_pipeline,
                    pt_face_views,
                    pt_tr_faces,
                    pt_shadow_depth,
                    pt_face_buf,
                    pt_face_bg,
                },
            ),
            post,
            outline_buf,
            crosshair_buf,
            entity_vbuf,
            entity_ibuf,
            overlay_vbuf,
            overlay_ibuf,
            hand_vbuf,
            hand_ibuf,
            ui_vbuf,
            chunks: HashMap::new(),
            sky_color: [0.55, 0.75, 0.95],
            pending_screenshot: None,
            pending_capture_metadata: None,
        };
        r.update_crosshair();
        r
    }
}

mod atlas;
mod cascade;
mod diagnostic_pipelines;
mod point_uniforms;
mod raster;
mod scene_pipelines;
mod shadow_bindings;
mod shadow_pipelines;
mod shadow_targets;
mod uniforms;
