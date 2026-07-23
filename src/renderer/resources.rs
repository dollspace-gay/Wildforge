//! Atlas replacement, resize handling, and dynamic mesh resources.

use super::*;

impl Renderer {
    pub fn clear_chunks(&mut self) {
        self.chunks.clear();
    }

    /// Replace the synchronized atlas textures during hot reload.
    pub fn set_atlas(&mut self, data: &[u8], material: &[u8], normal: &[u8], px: u32) {
        let color = upload_atlas(&self.device, &self.queue, data, px, true, "atlas");
        let mat = upload_atlas(
            &self.device,
            &self.queue,
            material,
            px,
            false,
            "atlas-material",
        );
        let nrm = upload_atlas(&self.device, &self.queue, normal, px, false, "atlas-normal");
        self.atlas_bg = atlas_bind_group(
            &self.device,
            &self.atlas_bgl,
            &color,
            &mat,
            &nrm,
            &self.atlas_sampler,
        );
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.config.width = w.max(1);
        self.config.height = h.max(1);
        self.surface.configure(&self.device, &self.config);
        self.depth = create_depth(&self.device, &self.config);
        self.post = create_post_targets(
            &self.device,
            &self.config,
            &self.post_in_bgl,
            &self.post_tex_bgl,
            &self.post_sampler,
        );
        self.update_crosshair();
    }

    pub(super) fn update_crosshair(&mut self) {
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
}

/// Upload one square atlas image and return its texture view.
pub(super) fn upload_atlas(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    data: &[u8],
    px: u32,
    srgb: bool,
    label: &str,
) -> wgpu::TextureView {
    let size = wgpu::Extent3d {
        width: px,
        height: px,
        depth_or_array_layers: 1,
    };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: if srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        },
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
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
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Bind color, material, and normal atlases with their shared sampler.
pub(super) fn atlas_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    color: &wgpu::TextureView,
    material: &wgpu::TextureView,
    normal: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("atlas-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(color),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(material),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(normal),
            },
        ],
    })
}
