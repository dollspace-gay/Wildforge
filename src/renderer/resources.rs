//! Atlas replacement, resize handling, and dynamic mesh resources.

use super::*;

impl Renderer {
    pub fn clear_chunks(&mut self) {
        self.chunks.clear();
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
