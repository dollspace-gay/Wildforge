//! Surface, device, pipeline, shadow-map, and post-chain construction.

use super::*;

impl Renderer {
    pub async fn new(
        window: Arc<Window>,
        atlas_data: Vec<u8>,
        atlas_material: Vec<u8>,
        atlas_normal: Vec<u8>,
        atlas_px: u32,
    ) -> Renderer {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window).expect("create surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("no GPU adapter found");
        let info = adapter.get_info();
        let adapter_name = format!("{} [{:?}]", info.name, info.backend);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults()
                    .using_resolution(adapter.limits()),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .expect("request device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let depth = create_depth(&device, &config);

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

        // Texture atlas
        let atlas_view = upload_atlas(&device, &queue, &atlas_data, atlas_px, true, "atlas");
        let material_view = upload_atlas(
            &device,
            &queue,
            &atlas_material,
            atlas_px,
            false,
            "atlas-material",
        );
        let normal_view = upload_atlas(
            &device,
            &queue,
            &atlas_normal,
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
            &device,
            &atlas_bgl,
            &atlas_view,
            &material_view,
            &normal_view,
            &sampler,
        );

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shader.wgsl").into()),
        });
        // The point-shadow pass has its own group-0 uniform (per-face matrix +
        // light position), so it lives in a separate module.
        let pt_shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pt-shadow-shader"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
struct PtFace {
    view_proj: mat4x4<f32>,
    light_pos: vec4<f32>,
};
@group(0) @binding(0) var<uniform> f: PtFace;

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
};

@vertex
fn vs_pt_shadow(@location(0) pos: vec3<f32>) -> VOut {
    var o: VOut;
    o.clip = f.view_proj * vec4<f32>(pos, 1.0);
    o.world = pos;
    return o;
}

@fragment
fn fs_pt_shadow(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(length(in.world - f.light_pos.xyz), 0.0, 0.0, 0.0);
}

// Transmission pass: glass surfaces multiply their filter color into the
// light's tint cube (order-independent), alpha Min-keeps the nearest
// glass distance so tint applies only between light and fragment.
@group(1) @binding(0) var atlas_tex: texture_2d<f32>;
@group(1) @binding(1) var atlas_smp: sampler;

struct TrOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_pt_tr(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>) -> TrOut {
    var o: TrOut;
    o.clip = f.view_proj * vec4<f32>(pos, 1.0);
    o.world = pos;
    o.uv = uv;
    return o;
}

@fragment
fn fs_pt_tr(in: TrOut) -> @location(0) vec4<f32> {
    let tex = textureSample(atlas_tex, atlas_smp, in.uv);
    // Panes are mostly-transparent texels, so raw alpha would wash the
    // tint to white. Take the tile's hue at full saturation and let
    // alpha set how strongly the pane stains the beam.
    let m = max(tex.r, max(tex.g, tex.b));
    let hue = tex.rgb / max(m, 1e-3);
    let strength = clamp(tex.a * 2.2, 0.0, 0.92);
    let tint = mix(vec3<f32>(1.0), hue, strength);
    return vec4<f32>(tint, length(in.world - f.light_pos.xyz));
}
"#
                .into(),
            ),
        });

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

        let shadow_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::CubeArray,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::CubeArray,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let shadow_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow-bg"),
            layout: &shadow_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&pt_cube_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&pt_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&pt_tr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&occ_view),
                },
            ],
        });

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

        // Main-pass pipelines bind [uniforms, atlas, shadow]. Line/UI pipelines
        // share this layout and simply ignore the shadow group.
        let chunk_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&uniform_bgl, &atlas_bgl, &shadow_bgl],
            push_constant_ranges: &[],
        });
        // Depth-only cascade shader: one matrix per pass, selected by dynamic
        // offset. Separate module because its group-0 uniform is just the
        // cascade's view_proj, not the full scene Uniforms.
        let csm_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("csm-shader"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
struct Casc { view_proj: mat4x4<f32> };
@group(0) @binding(0) var<uniform> c: Casc;
@vertex
fn vs_shadow(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    return c.view_proj * vec4<f32>(pos, 1.0);
}
"#
                .into(),
            ),
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

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x3, 3 => Float32x3, 4 => Float32],
        };
        let line_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LineVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
        };
        let ui_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<UiVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4],
        };

        let depth_state = |write: bool| wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: write,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };

        let make_pipeline = |label: &str,
                             vs: &str,
                             fs: &str,
                             vlayout: &wgpu::VertexBufferLayout,
                             blend: Option<wgpu::BlendState>,
                             cull: Option<wgpu::Face>,
                             topology: wgpu::PrimitiveTopology,
                             depth_stencil: Option<wgpu::DepthStencilState>,
                             target: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&chunk_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: std::slice::from_ref(vlayout),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: cull,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };

        // Scene pipelines render into the linear HDR target; the crosshair and
        // UI draw straight to the swapchain after the composite.
        let chunk_pipeline = make_pipeline(
            "chunk",
            "vs_chunk",
            "fs_chunk",
            &vertex_layout,
            None,
            Some(wgpu::Face::Back),
            wgpu::PrimitiveTopology::TriangleList,
            Some(depth_state(true)),
            HDR_FORMAT,
        );
        // Background sky: a fullscreen triangle (no vertex buffer) drawn first
        // in the main pass. Depth-write off + compare Always fills every pixel
        // at the far plane; terrain then paints over it. Shares the chunk
        // layout so it reads the same uniform group.
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky"),
            layout: Some(&chunk_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_sky"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
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
        let water_pipeline = make_pipeline(
            "water",
            "vs_chunk",
            "fs_water",
            &vertex_layout,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            None,
            wgpu::PrimitiveTopology::TriangleList,
            Some(depth_state(false)),
            HDR_FORMAT,
        );
        let line_world_pipeline = make_pipeline(
            "line-world",
            "vs_line_world",
            "fs_line",
            &line_layout,
            None,
            None,
            wgpu::PrimitiveTopology::LineList,
            Some(depth_state(false)),
            HDR_FORMAT,
        );
        let line_screen_pipeline = make_pipeline(
            "line-screen",
            "vs_line_screen",
            "fs_line",
            &line_layout,
            None,
            None,
            wgpu::PrimitiveTopology::LineList,
            Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            config.format,
        );
        let ui_pipeline = make_pipeline(
            "ui",
            "vs_ui",
            "fs_ui",
            &ui_layout,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            None,
            wgpu::PrimitiveTopology::TriangleList,
            Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            config.format,
        );

        // Depth-only sun pass: reads only position from the chunk vertex buffer,
        // writes the shadow depth texture. Constant + slope depth bias pushes
        // occluders back to keep shadow acne off lit faces.
        let shadow_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3],
        };
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(&shadow_layout),
            vertex: wgpu::VertexState {
                module: &csm_shader,
                entry_point: Some("vs_shadow"),
                compilation_options: Default::default(),
                buffers: std::slice::from_ref(&shadow_vertex_layout),
            },
            fragment: None,
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
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.5,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Point-light cube-face pass: writes distance-to-light into an R32Float
        // face. No culling (robust for 1-block-thin occluders); a depth bias in
        // the compare keeps acne off lit faces.
        let pt_shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pt-shadow-layout"),
            bind_group_layouts: &[&pt_face_bgl],
            push_constant_ranges: &[],
        });
        let pt_shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pt-shadow"),
            layout: Some(&pt_shadow_layout),
            vertex: wgpu::VertexState {
                module: &pt_shadow_shader,
                entry_point: Some("vs_pt_shadow"),
                compilation_options: Default::default(),
                buffers: std::slice::from_ref(&shadow_vertex_layout),
            },
            fragment: Some(wgpu::FragmentState {
                module: &pt_shadow_shader,
                entry_point: Some("fs_pt_shadow"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R32Float,
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
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Glass transmission into the tint cube: multiplicative color
        // (commutative — no sorting), Min alpha keeps the nearest pane's
        // distance. Tests against the distance pass's depth, read-only,
        // so glass behind an opaque wall never tints.
        let pt_tr_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pt-tr-layout"),
            bind_group_layouts: &[&pt_face_bgl, &atlas_bgl],
            push_constant_ranges: &[],
        });
        let tr_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2],
        };
        let pt_tr_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pt-tr"),
            layout: Some(&pt_tr_layout),
            vertex: wgpu::VertexState {
                module: &pt_shadow_shader,
                entry_point: Some("vs_pt_tr"),
                compilation_options: Default::default(),
                buffers: std::slice::from_ref(&tr_vertex_layout),
            },
            fragment: Some(wgpu::FragmentState {
                module: &pt_shadow_shader,
                entry_point: Some("fs_pt_tr"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Dst,
                            dst_factor: wgpu::BlendFactor::Zero,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Min,
                        },
                    }),
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
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ---- HDR + bloom post chain ----
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../post.wgsl").into()),
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
        let bright_pipeline = make_post("bright", &bloom_layout, "fs_bright", HDR_FORMAT);
        let blur_h_pipeline = make_post("blur-h", &bloom_layout, "fs_blur_h", HDR_FORMAT);
        let blur_v_pipeline = make_post("blur-v", &bloom_layout, "fs_blur_v", HDR_FORMAT);
        let composite_pipeline = make_post(
            "composite",
            &composite_layout,
            "fs_composite",
            config.format,
        );
        let post =
            create_post_targets(&device, &config, &post_in_bgl, &post_tex_bgl, &post_sampler);

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
            adapter_name,
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
            shadow_pipeline,
            shadow_layer_views,
            shadow_casc_buf,
            shadow_casc_bg,
            shadow_bg,
            occ_tex,
            pt_cached: [None; MAX_PT_LIGHTS],
            pt_progress: [0; MAX_PT_LIGHTS],
            pt_shadow_pipeline,
            pt_tr_pipeline,
            pt_face_views,
            pt_tr_faces,
            pt_shadow_depth,
            pt_face_buf,
            pt_face_bg,
            post_in_bgl,
            post_tex_bgl,
            post_sampler,
            post_params_buf,
            post_params_bg,
            bright_pipeline,
            blur_h_pipeline,
            blur_v_pipeline,
            composite_pipeline,
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
        };
        r.update_crosshair();
        r
    }
}
