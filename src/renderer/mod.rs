//! Renderer ownership, frame inputs, and GPU resource types.

use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::chunk::ChunkPos;
use crate::mesher::{ChunkMesh, Vertex};
use crate::ui::UiVertex;

mod frame;
mod post;
mod resources;
mod setup;

use post::*;
use resources::{atlas_bind_group, upload_atlas};

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
    /// x = active point-light count.
    pt_count: [u32; 4],
    /// Per light: xyz = world position, w = range.
    pt_pos: [[f32; 4]; MAX_PT_LIGHTS],
    /// Per light: rgb = color × intensity, w unused.
    pt_col: [[f32; 4]; MAX_PT_LIGHTS],
    /// Per light: x = suppression scale, y = suppression range,
    /// z = shadows enabled, w unused.
    pt_misc: [[f32; 4]; MAX_PT_LIGHTS],
}

/// Sun shadow-map resolution (square). Keep in sync with SHADOW_RES in the shader.
const SHADOW_RES: u32 = 2048;

/// Max shadow-casting/accumulated point lights per frame. Keep in sync with the
/// shader's MAX_PT_LIGHTS.
const MAX_PT_LIGHTS: usize = 8;

/// Per-face resolution of each point light's distance cube map.
const PT_SHADOW_RES: u32 = 512;
/// Bytes per per-face uniform slot (256 = min dynamic-offset alignment).
const PT_FACE_STRIDE: u64 = 256;
/// The scene renders here (linear, unclamped) so emitters and stacked lights
/// keep energy past 1.0 for the bloom pass; a composite tonemaps to sRGB.
const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// The six cube-map face camera basis vectors (look dir, up), matching the
/// standard cube layout (+X -X +Y -Y +Z -Z).
const CUBE_FACES: [([f32; 3], [f32; 3]); 6] = [
    ([1.0, 0.0, 0.0], [0.0, -1.0, 0.0]),
    ([-1.0, 0.0, 0.0], [0.0, -1.0, 0.0]),
    ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
    ([0.0, -1.0, 0.0], [0.0, 0.0, -1.0]),
    ([0.0, 0.0, 1.0], [0.0, -1.0, 0.0]),
    ([0.0, 0.0, -1.0], [0.0, -1.0, 0.0]),
];

/// A colored point light accumulated (and shadow-mapped) in the chunk
/// shader. The lights::Director decides who gets one each frame.
#[derive(Clone, Copy)]
pub struct PointLight {
    pub pos: Vec3,
    pub range: f32,
    /// Color premultiplied by intensity.
    pub color: Vec3,
    /// Stable identity + revision: the cube-map cache re-renders a slot
    /// only when (key, epoch) changes. Bump epoch to invalidate.
    pub key: u64,
    pub epoch: u64,
    /// false = shadowless (no cube passes, no cube sampling).
    pub shadows: bool,
    /// Rendered flood-fill suppression: (scale, range). The shader
    /// subtracts `color * scale * max(0, 1 - d/range)` from the soft
    /// torch term so the hard direct light reads. (0, _) disables.
    pub suppress: (f32, f32),
    /// Source size in world units. 0 is a hard point; larger softens the
    /// shadow penumbra (approximate area light). Sampling-only — doesn't
    /// affect the cached cube.
    pub radius: f32,
}

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
    /// Absolute darkness floor (stark ~0.04, soft ~0.12).
    pub ambient_floor: f32,
    /// Dynamic colored point lights (accumulated in the chunk shader).
    pub point_lights: &'a [PointLight],
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
    /// Bloom intensity added over the scene (0 disables the effect).
    pub bloom: f32,
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

/// Is any part of the chunk's AABB within `range` of the light?
fn chunk_in_range(pos: ChunkPos, light: Vec3, range: f32) -> bool {
    let min = Vec3::new(pos.x as f32 * 16.0, 0.0, pos.z as f32 * 16.0);
    let max = min + Vec3::new(16.0, 256.0, 16.0);
    light.distance(light.clamp(min, max)) <= range
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

    // Point-light distance cube maps (one cube = 6 layers per light).
    // pt_cached remembers (key, epoch) per slot; a slot's 6 faces
    // re-render only when that changes (the static-light cache).
    pt_cached: [Option<(u64, u64)>; MAX_PT_LIGHTS],
    pt_shadow_pipeline: wgpu::RenderPipeline,
    pt_tr_pipeline: wgpu::RenderPipeline,
    pt_face_views: Vec<wgpu::TextureView>, // 6 * MAX_PT_LIGHTS render targets
    pt_tr_faces: Vec<wgpu::TextureView>,   // matching tint-cube targets
    pt_shadow_depth: wgpu::TextureView,    // shared scratch depth
    pt_face_buf: wgpu::Buffer,             // per-face {view_proj, light_pos}
    pt_face_bg: wgpu::BindGroup,           // dynamic-offset bind of pt_face_buf

    // HDR + bloom post chain. The pipelines are size-independent; the targets
    // and their bind groups are rebuilt on resize by `create_post_targets`.
    post_in_bgl: wgpu::BindGroupLayout,
    post_tex_bgl: wgpu::BindGroupLayout,
    post_sampler: wgpu::Sampler,
    post_params_buf: wgpu::Buffer,
    post_params_bg: wgpu::BindGroup,
    bright_pipeline: wgpu::RenderPipeline,
    blur_h_pipeline: wgpu::RenderPipeline,
    blur_v_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    post: PostTargets,

    outline_buf: wgpu::Buffer,
    crosshair_buf: wgpu::Buffer,
    entity_vbuf: DynBuf,
    entity_ibuf: DynBuf,
    overlay_vbuf: DynBuf,
    overlay_ibuf: DynBuf,
    hand_vbuf: DynBuf,
    hand_ibuf: DynBuf,
    ui_vbuf: DynBuf,

    chunks: HashMap<ChunkPos, GpuChunk>,
    pub sky_color: [f32; 3],
    pub pending_screenshot: Option<String>,
}
