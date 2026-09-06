//! Ordered GPU preparation, shadow, scene, post, and capture submission.

use super::{Renderer, FrameInput, GpuChunk};
use crate::chunk::ChunkPos;

pub(in crate::renderer) struct DiagnosticReadback {
    buffer: wgpu::Buffer,
    texture: wgpu::Texture,
    width: u32,
    height: u32,
    padded_bytes_per_row: u32,
}

pub(in crate::renderer) struct ScreenshotReadback {
    path: String,
    buf: wgpu::Buffer,
    texture: wgpu::Texture,
    w: u32,
    h: u32,
    bpr: u32,
    metadata: Option<crate::visual_capture::CaptureMetadata>,
    diagnostic: Option<DiagnosticReadback>,
}

impl Renderer {
    pub fn render(&mut self, f: FrameInput<'_>) -> Result<(), wgpu::SurfaceError> {
        let (uniforms, light_vp) = self.frame_uniforms(&f);
        self.upload_frame(&f, &uniforms, &light_vp);
        self.upload_outline(f.outline, f.outline_color);
        let mut encoder = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: None },
        );
        // Reuse one loaded-chunk list; each pass keeps its own culling rule.
        let visible: Vec<(&ChunkPos, &GpuChunk)> = self.chunks.iter().collect();
        self.encode_sun_shadows(&mut encoder, &f, &visible);
        self.point_shadows.encode(&mut encoder, &f, &visible, &self.atlas_bg);
        self.encode_world(&mut encoder, &f, &visible);
        self.encode_hand(&mut encoder, &f);
        self.post.bloom(&self.queue, &mut encoder, f.bloom, f.daylight);
        // Acquire only after scene encoding, preserving late swapchain admission.
        let frame = self.surface.get_current_texture()?;
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.composite_and_ui(&mut encoder, &view, &f);
        let requested_shot = self.pending_screenshot.take()
            .map(|path| (path, self.pending_capture_metadata.take()));
        let shot = requested_shot.map(|(path, metadata)| {
            self.encode_capture(&mut encoder, &f, &visible, path, metadata)
        });
        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        if let Some(shot) = shot { self.write_capture(shot); }
        Ok(())
    }
}

mod preparation;
mod outline;
mod sun;
mod scene;
mod composite;
mod diagnostic;
mod capture;
