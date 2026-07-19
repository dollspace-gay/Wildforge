//! wgpu renderer: surface, pipelines, per-chunk GPU meshes.

use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::chunk::ChunkPos;
use crate::mesher::{ChunkMesh, Vertex};
use crate::ui::UiVertex;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    cam: [f32; 4],
    sky: [f32; 4],
    misc: [f32; 4],
    sun_dir: [f32; 4],
    sun_col: [f32; 4],
    amb_col: [f32; 4],
    light_vp: [[f32; 4]; 4],
}

/// Sun shadow-map resolution (square). Keep in sync with SHADOW_RES in the shader.
const SHADOW_RES: u32 = 2048;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct LineVertex {
    pub pos: [f32; 3],
    pub color: [f32; 3],
}

struct GpuMesh {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    count: u32,
}

/// A growable GPU buffer re-uploaded every frame.
struct DynBuf {
    buf: wgpu::Buffer,
    cap: u64,
    usage: wgpu::BufferUsages,
}

impl DynBuf {
    fn new(device: &wgpu::Device, usage: wgpu::BufferUsages) -> DynBuf {
        let cap = 16 * 1024;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: cap,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        DynBuf { buf, cap, usage }
    }

    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        if data.len() as u64 > self.cap {
            self.cap = (data.len() as u64).next_power_of_two();
            self.buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: self.cap,
                usage: self.usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.buf, 0, data);
    }
}

/// Everything main hands to the renderer for one frame.
pub struct FrameInput<'a> {
    pub view_proj: Mat4,
    pub cam_pos: Vec3,
    pub fog_dist: f32,
    pub underwater: bool,
    pub daylight: f32,
    /// Normalized direction toward the sun (world space).
    pub sun_dir: Vec3,
    /// Warm direct-sun color, already scaled by daylight.
    pub sun_col: Vec3,
    /// Cool sky-ambient color, already scaled by daylight.
    pub amb_col: Vec3,
    pub outline: Option<(i32, i32, i32)>,
    /// Opaque world-space extras (item entities), drawn with the chunk shader.
    pub entity_verts: &'a [Vertex],
    pub entity_idx: &'a [u32],
    /// Alpha-blended world-space extras (mining crack overlay).
    pub overlay_verts: &'a [Vertex],
    pub overlay_idx: &'a [u32],
    /// First-person viewmodel (your hand and what it holds): drawn in
    /// its own depth-cleared pass so it never clips into walls.
    pub hand_verts: &'a [Vertex],
    pub hand_idx: &'a [u32],
    /// 2D UI triangles in pixel coordinates.
    pub ui_verts: &'a [UiVertex],
    /// Draw the line crosshair (hidden when a menu is open).
    pub crosshair: bool,
}

/// Frustum planes from a view-projection matrix (Gribb-Hartmann).
fn frustum_planes(m: &Mat4) -> [glam::Vec4; 6] {
    let r0 = m.row(0);
    let r1 = m.row(1);
    let r2 = m.row(2);
    let r3 = m.row(3);
    [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r3 + r2, r3 - r2]
}

/// Is the chunk's AABB at least partially inside the frustum?
fn chunk_visible(planes: &[glam::Vec4; 6], pos: ChunkPos) -> bool {
    let min = glam::Vec3::new(pos.x as f32 * 16.0, 0.0, pos.z as f32 * 16.0);
    let max = min + glam::Vec3::new(16.0, 256.0, 16.0);
    for p in planes {
        // Positive vertex: the AABB corner furthest along the plane normal.
        let v = glam::Vec3::new(
            if p.x >= 0.0 { max.x } else { min.x },
            if p.y >= 0.0 { max.y } else { min.y },
            if p.z >= 0.0 { max.z } else { min.z },
        );
        if p.x * v.x + p.y * v.y + p.z * v.z + p.w < 0.0 {
            return false;
        }
    }
    true
}

fn upload_mesh(device: &wgpu::Device, verts: &[Vertex], idx: &[u32]) -> Option<GpuMesh> {
    if idx.is_empty() {
        return None;
    }
    Some(GpuMesh {
        vbuf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(verts),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        ibuf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(idx),
            usage: wgpu::BufferUsages::INDEX,
        }),
        count: idx.len() as u32,
    })
}

pub struct GpuChunk {
    opaque: Option<GpuMesh>,
    water: Option<GpuMesh>,
}

pub struct Renderer {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,

    uniforms_buf: wgpu::Buffer,
    uniform_bg: wgpu::BindGroup,
    atlas_bg: wgpu::BindGroup,
    atlas_bgl: wgpu::BindGroupLayout,
    atlas_sampler: wgpu::Sampler,

    chunk_pipeline: wgpu::RenderPipeline,
    water_pipeline: wgpu::RenderPipeline,
    line_world_pipeline: wgpu::RenderPipeline,
    line_screen_pipeline: wgpu::RenderPipeline,
    ui_pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    shadow_view: wgpu::TextureView,
    shadow_bg: wgpu::BindGroup,

    outline_buf: wgpu::Buffer,
    crosshair_buf: wgpu::Buffer,
    entity_vbuf: DynBuf,
    entity_ibuf: DynBuf,
    overlay_vbuf: DynBuf,
    overlay_ibuf: DynBuf,
    hand_vbuf: DynBuf,
    hand_ibuf: DynBuf,
    ui_vbuf: DynBuf,

    pub chunks: HashMap<ChunkPos, GpuChunk>,
    pub sky_color: [f32; 3],
    pub pending_screenshot: Option<String>,
}

impl Renderer {
    pub async fn new(window: Arc<Window>, atlas_data: Vec<u8>, atlas_px: u32) -> Renderer {
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
        let atlas_size = wgpu::Extent3d {
            width: atlas_px,
            height: atlas_px,
            depth_or_array_layers: 1,
        };
        let atlas_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size: atlas_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas_px * 4),
                rows_per_image: Some(atlas_px),
            },
            atlas_size,
        );
        let atlas_view = atlas_tex.create_view(&wgpu::TextureViewDescriptor::default());
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
            ],
        });
        let atlas_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &atlas_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        // Sun shadow map: a depth texture rendered from the light's POV and
        // sampled (with hardware PCF via a comparison sampler) in the main pass.
        let shadow_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow"),
            size: wgpu::Extent3d {
                width: SHADOW_RES,
                height: SHADOW_RES,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow-cmp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let shadow_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
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
            ],
        });

        // Main-pass pipelines bind [uniforms, atlas, shadow]. Line/UI pipelines
        // share this layout and simply ignore the shadow group.
        let chunk_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&uniform_bgl, &atlas_bgl, &shadow_bgl],
            push_constant_ranges: &[],
        });
        // The depth-only shadow pass needs only the uniforms (for light_vp).
        let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow-layout"),
            bind_group_layouts: &[&uniform_bgl],
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

        let make_pipeline =
            |label: &str,
             vs: &str,
             fs: &str,
             vlayout: &wgpu::VertexBufferLayout,
             blend: Option<wgpu::BlendState>,
             cull: Option<wgpu::Face>,
             topology: wgpu::PrimitiveTopology,
             depth_stencil: Option<wgpu::DepthStencilState>| {
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
                            format: config.format,
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

        let chunk_pipeline = make_pipeline(
            "chunk",
            "vs_chunk",
            "fs_chunk",
            &vertex_layout,
            None,
            Some(wgpu::Face::Back),
            wgpu::PrimitiveTopology::TriangleList,
            Some(depth_state(true)),
        );
        let water_pipeline = make_pipeline(
            "water",
            "vs_chunk",
            "fs_water",
            &vertex_layout,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            None,
            wgpu::PrimitiveTopology::TriangleList,
            Some(depth_state(false)),
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
                module: &shader,
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
            water_pipeline,
            line_world_pipeline,
            line_screen_pipeline,
            ui_pipeline,
            shadow_pipeline,
            shadow_view,
            shadow_bg,
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

    /// Replace the atlas texture (hot reload).
    pub fn set_atlas(&mut self, data: &[u8], px: u32) {
        let size = wgpu::Extent3d {
            width: px,
            height: px,
            depth_or_array_layers: 1,
        };
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(px * 4),
                rows_per_image: Some(px),
            },
            size,
        );
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        self.atlas_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.atlas_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.atlas_sampler),
                },
            ],
        });
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.config.width = w.max(1);
        self.config.height = h.max(1);
        self.surface.configure(&self.device, &self.config);
        self.depth = create_depth(&self.device, &self.config);
        self.update_crosshair();
    }

    fn update_crosshair(&mut self) {
        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        let s = 0.025;
        let c = [1.0, 1.0, 1.0];
        let verts = [
            LineVertex {
                pos: [-s / aspect, 0.0, 0.0],
                color: c,
            },
            LineVertex {
                pos: [s / aspect, 0.0, 0.0],
                color: c,
            },
            LineVertex {
                pos: [0.0, -s, 0.0],
                color: c,
            },
            LineVertex {
                pos: [0.0, s, 0.0],
                color: c,
            },
        ];
        self.queue
            .write_buffer(&self.crosshair_buf, 0, bytemuck::cast_slice(&verts));
    }

    pub fn upload_chunk(&mut self, pos: ChunkPos, mesh: &ChunkMesh) {
        let gpu = GpuChunk {
            opaque: upload_mesh(&self.device, &mesh.opaque_verts, &mesh.opaque_idx),
            water: upload_mesh(&self.device, &mesh.water_verts, &mesh.water_idx),
        };
        self.chunks.insert(pos, gpu);
    }

    pub fn drop_chunk(&mut self, pos: ChunkPos) {
        self.chunks.remove(&pos);
    }

    pub fn render(&mut self, f: FrameInput) -> Result<(), wgpu::SurfaceError> {
        let outline = f.outline;

        // Sun light-space matrix: an orthographic box centered near the camera,
        // looking from the sun toward that center. Covers the near field; beyond
        // its radius the shader treats fragments as lit (shadows fade out).
        let light_vp = {
            let radius = 90.0f32;
            let dist = 160.0f32;
            let center = f.cam_pos;
            let eye = center + f.sun_dir * dist;
            let up = if f.sun_dir.y.abs() > 0.95 {
                Vec3::Z
            } else {
                Vec3::Y
            };
            let view = glam::camera::rh::view::look_at_mat4(eye, center, up);
            let proj = glam::camera::rh::proj::directx::orthographic(
                -radius,
                radius,
                -radius,
                radius,
                1.0,
                dist + radius * 2.0,
            );
            proj * view
        };

        let uniforms = Uniforms {
            view_proj: f.view_proj.to_cols_array_2d(),
            cam: [f.cam_pos.x, f.cam_pos.y, f.cam_pos.z, f.fog_dist],
            sky: [self.sky_color[0], self.sky_color[1], self.sky_color[2], 1.0],
            misc: [
                if f.underwater { 1.0 } else { 0.0 },
                f.daylight,
                self.config.width as f32,
                self.config.height as f32,
            ],
            sun_dir: [f.sun_dir.x, f.sun_dir.y, f.sun_dir.z, 0.0],
            sun_col: [f.sun_col.x, f.sun_col.y, f.sun_col.z, 0.0],
            amb_col: [f.amb_col.x, f.amb_col.y, f.amb_col.z, 0.0],
            light_vp: light_vp.to_cols_array_2d(),
        };
        self.entity_vbuf.upload(
            &self.device,
            &self.queue,
            bytemuck::cast_slice(f.entity_verts),
        );
        self.entity_ibuf.upload(
            &self.device,
            &self.queue,
            bytemuck::cast_slice(f.entity_idx),
        );
        self.overlay_vbuf.upload(
            &self.device,
            &self.queue,
            bytemuck::cast_slice(f.overlay_verts),
        );
        self.overlay_ibuf.upload(
            &self.device,
            &self.queue,
            bytemuck::cast_slice(f.overlay_idx),
        );
        self.hand_vbuf.upload(
            &self.device,
            &self.queue,
            bytemuck::cast_slice(f.hand_verts),
        );
        self.hand_ibuf
            .upload(&self.device, &self.queue, bytemuck::cast_slice(f.hand_idx));
        self.ui_vbuf
            .upload(&self.device, &self.queue, bytemuck::cast_slice(f.ui_verts));
        self.queue
            .write_buffer(&self.uniforms_buf, 0, bytemuck::bytes_of(&uniforms));

        if let Some((bx, by, bz)) = outline {
            let e = 0.003f32;
            let (x0, y0, z0) = (bx as f32 - e, by as f32 - e, bz as f32 - e);
            let (x1, y1, z1) = (
                bx as f32 + 1.0 + e,
                by as f32 + 1.0 + e,
                bz as f32 + 1.0 + e,
            );
            let c = [0.05, 0.05, 0.05];
            let p = |x: f32, y: f32, z: f32| LineVertex {
                pos: [x, y, z],
                color: c,
            };
            let verts = [
                // bottom
                p(x0, y0, z0),
                p(x1, y0, z0),
                p(x1, y0, z0),
                p(x1, y0, z1),
                p(x1, y0, z1),
                p(x0, y0, z1),
                p(x0, y0, z1),
                p(x0, y0, z0),
                // top
                p(x0, y1, z0),
                p(x1, y1, z0),
                p(x1, y1, z0),
                p(x1, y1, z1),
                p(x1, y1, z1),
                p(x0, y1, z1),
                p(x0, y1, z1),
                p(x0, y1, z0),
                // pillars
                p(x0, y0, z0),
                p(x0, y1, z0),
                p(x1, y0, z0),
                p(x1, y1, z0),
                p(x1, y0, z1),
                p(x1, y1, z1),
                p(x0, y0, z1),
                p(x0, y1, z1),
            ];
            self.queue
                .write_buffer(&self.outline_buf, 0, bytemuck::cast_slice(&verts));
        }

        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // Shadow pass: opaque terrain depth from the sun's POV. No color target.
        // Every loaded chunk is a potential caster (occluders behind the camera
        // still shadow what's in view), so this pass is not frustum-culled.
        {
            let mut sp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            sp.set_pipeline(&self.shadow_pipeline);
            sp.set_bind_group(0, &self.uniform_bg, &[]);
            for gpu in self.chunks.values() {
                if let Some(m) = &gpu.opaque {
                    sp.set_vertex_buffer(0, m.vbuf.slice(..));
                    sp.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    sp.draw_indexed(0..m.count, 0, 0..1);
                }
            }
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: self.sky_color[0] as f64,
                            g: self.sky_color[1] as f64,
                            b: self.sky_color[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_bind_group(0, &self.uniform_bg, &[]);
            pass.set_bind_group(1, &self.atlas_bg, &[]);
            pass.set_bind_group(2, &self.shadow_bg, &[]);

            // Opaque terrain (frustum-culled)
            let planes = frustum_planes(&f.view_proj);
            pass.set_pipeline(&self.chunk_pipeline);
            for (pos, gpu) in &self.chunks {
                if !chunk_visible(&planes, *pos) {
                    continue;
                }
                if let Some(m) = &gpu.opaque {
                    pass.set_vertex_buffer(0, m.vbuf.slice(..));
                    pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..m.count, 0, 0..1);
                }
            }

            // Item entities (opaque mini-cubes)
            if !f.entity_idx.is_empty() {
                pass.set_vertex_buffer(0, self.entity_vbuf.buf.slice(..));
                pass.set_index_buffer(self.entity_ibuf.buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..f.entity_idx.len() as u32, 0, 0..1);
            }

            // Water
            pass.set_pipeline(&self.water_pipeline);
            for (pos, gpu) in &self.chunks {
                if !chunk_visible(&planes, *pos) {
                    continue;
                }
                if let Some(m) = &gpu.water {
                    pass.set_vertex_buffer(0, m.vbuf.slice(..));
                    pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..m.count, 0, 0..1);
                }
            }

            // Mining crack overlay (alpha-blended, reuses the water pipeline)
            if !f.overlay_idx.is_empty() {
                pass.set_vertex_buffer(0, self.overlay_vbuf.buf.slice(..));
                pass.set_index_buffer(self.overlay_ibuf.buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..f.overlay_idx.len() as u32, 0, 0..1);
            }

            // Targeted block outline
            if outline.is_some() {
                pass.set_pipeline(&self.line_world_pipeline);
                pass.set_vertex_buffer(0, self.outline_buf.slice(..));
                pass.draw(0..24, 0..1);
            }
        }

        // Second pass, depth cleared: the first-person hand draws over
        // the world no matter how close a wall is, then the flat UI.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hand+ui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.uniform_bg, &[]);
            pass.set_bind_group(1, &self.atlas_bg, &[]);
            // The chunk/line/ui pipelines share a 3-group layout; the shadow
            // group must stay bound here even though the hand doesn't sample it.
            pass.set_bind_group(2, &self.shadow_bg, &[]);

            if !f.hand_idx.is_empty() {
                pass.set_pipeline(&self.chunk_pipeline);
                pass.set_vertex_buffer(0, self.hand_vbuf.buf.slice(..));
                pass.set_index_buffer(self.hand_ibuf.buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..f.hand_idx.len() as u32, 0, 0..1);
            }

            // Crosshair
            if f.crosshair {
                pass.set_pipeline(&self.line_screen_pipeline);
                pass.set_vertex_buffer(0, self.crosshair_buf.slice(..));
                pass.draw(0..4, 0..1);
            }

            // 2D UI
            if !f.ui_verts.is_empty() {
                pass.set_pipeline(&self.ui_pipeline);
                pass.set_vertex_buffer(0, self.ui_vbuf.buf.slice(..));
                pass.draw(0..f.ui_verts.len() as u32, 0..1);
            }
        }

        let shot = self.pending_screenshot.take().map(|path| {
            let w = self.config.width;
            let h = self.config.height;
            let bpr = (w * 4).div_ceil(256) * 256; // COPY_BYTES_PER_ROW_ALIGNMENT
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("screenshot"),
                size: (bpr * h) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &frame.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buf,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(bpr),
                        rows_per_image: Some(h),
                    },
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
            (path, buf, w, h, bpr)
        });

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();

        if let Some((path, buf, w, h, bpr)) = shot {
            let bgra = matches!(
                self.config.format,
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
            );
            let slice = buf.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            let _ = self.device.poll(wgpu::PollType::Wait);
            let data = slice.get_mapped_range();
            let mut out = Vec::with_capacity((w * h * 3) as usize);
            for y in 0..h {
                let row = &data[(y * bpr) as usize..];
                for x in 0..w {
                    let p = &row[(x * 4) as usize..(x * 4 + 4) as usize];
                    if bgra {
                        out.extend_from_slice(&[p[2], p[1], p[0]]);
                    } else {
                        out.extend_from_slice(&[p[0], p[1], p[2]]);
                    }
                }
            }
            drop(data);
            buf.unmap();
            let header = format!("P6\n{w} {h}\n255\n");
            let mut file = header.into_bytes();
            file.extend_from_slice(&out);
            if let Err(e) = std::fs::write(&path, file) {
                eprintln!("screenshot failed: {e}");
            } else {
                println!("screenshot saved: {path}");
            }
        }
        Ok(())
    }
}

fn create_depth(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}
