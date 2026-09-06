//! Capture stage of GPU frame encoding.

use super::ScreenshotReadback;
use crate::chunk::ChunkPos;
use crate::renderer::{FrameInput, GpuChunk, Renderer};

impl Renderer {
    pub(in crate::renderer) fn encode_capture(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        f: &FrameInput<'_>,
        visible: &[(&ChunkPos, &GpuChunk)],
        path: String,
        metadata: Option<crate::visual_capture::CaptureMetadata>,
    ) -> ScreenshotReadback {
        let w = self.config.width;
        let h = self.config.height;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("screenshot-target"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let capture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.composite_and_ui(encoder, &capture_view, f);
        let bpr = (w * 4).div_ceil(256) * 256; // COPY_BYTES_PER_ROW_ALIGNMENT
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screenshot"),
            size: (bpr * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
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
        let diagnostic = metadata
            .as_ref()
            .map(|_| self.encode_diagnostic_replay(encoder, f, visible, f.outline));
        ScreenshotReadback {
            path,
            buf,
            texture,
            w,
            h,
            bpr,
            metadata,
            diagnostic,
        }
    }
    pub(in crate::renderer) fn write_capture(&self, shot: ScreenshotReadback) {
        let ScreenshotReadback {
            path,
            buf,
            texture: _texture,
            w,
            h,
            bpr,
            metadata,
            diagnostic,
        } = shot;
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
        if let Some(metadata) = metadata {
            let paths = crate::visual_capture::evidence_paths(std::path::Path::new(&path))
                .unwrap_or_else(|error| panic!("visual evidence path failed: {error}"));
            let diagnostic = diagnostic.expect("visual evidence diagnostic readback");
            let slice = diagnostic.buffer.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            self.device
                .poll(wgpu::PollType::Wait)
                .expect("wait for visual diagnostic readback");
            let data = slice.get_mapped_range();
            let wfd = crate::visual_capture::encode_wfd(
                diagnostic.width,
                diagnostic.height,
                diagnostic.padded_bytes_per_row,
                &data,
            )
            .unwrap_or_else(|error| panic!("encode visual diagnostic: {error}"));
            drop(data);
            diagnostic.buffer.unmap();
            drop(diagnostic.texture);

            crate::identity::atomic_write(&paths.ppm, &file, false)
                .unwrap_or_else(|error| panic!("write visual color evidence: {error}"));
            crate::identity::atomic_write(&paths.diagnostic, &wfd, false)
                .unwrap_or_else(|error| panic!("write visual diagnostic evidence: {error}"));
            let sidecar = crate::visual_capture::sidecar_text(
                metadata,
                &paths.ppm,
                &file,
                &paths.diagnostic,
                &wfd,
            )
            .unwrap_or_else(|error| panic!("serialize visual evidence: {error}"));
            crate::identity::atomic_write(&paths.sidecar, sidecar.as_bytes(), false)
                .unwrap_or_else(|error| panic!("write visual evidence identity: {error}"));
            println!(
                "visual evidence saved: {}, {}, {}",
                paths.ppm.display(),
                paths.diagnostic.display(),
                paths.sidecar.display()
            );
        } else if let Err(error) = std::fs::write(&path, file) {
            eprintln!("screenshot failed: {error}");
        } else {
            println!("screenshot saved: {path}");
        }
    }
}
