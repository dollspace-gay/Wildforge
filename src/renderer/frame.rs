//! Ordered GPU preparation, shadow, scene, post, and capture submission.

use super::{FrameInput, GpuChunk, Renderer};
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
    /// Render-call identity for associating host frame records with delayed GPU results.
    pub(crate) fn profile_frame_id(&self) -> u64 {
        self.render_frame_id
    }

    /// Collect completed GPU intervals without waiting for unfinished work.
    pub(crate) fn take_gpu_timings(
        &mut self,
    ) -> Result<Vec<super::GpuFrameTiming>, std::sync::Arc<super::GpuProfileError>> {
        self.profiling.as_mut().map_or_else(
            || Ok(Vec::new()),
            |profiling| profiling.take_completed(&self.device),
        )
    }

    /// Coverage records expose skipped samples if all asynchronous slots were busy.
    pub(crate) fn gpu_profile_stats(&self) -> Option<super::GpuProfileStats> {
        self.profiling
            .as_ref()
            .map(super::profiling::GpuProfiler::stats)
    }

    pub fn render(&mut self, f: FrameInput<'_>) -> Result<(), wgpu::SurfaceError> {
        self.render_frame_id = self.render_frame_id.saturating_add(1);
        self.last_render_work = None;
        // Start before uniforms and uploads, preserving ordinary render order.
        let work_start = self.render_work_enabled.then(std::time::Instant::now);
        let (uniforms, light_vp) = self.frame_uniforms(&f);
        self.upload_frame(&f, &uniforms, &light_vp);
        self.upload_outline(f.outline, f.outline_color);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        // Reuse one loaded-chunk list; each pass keeps its own culling rule.
        let visible: Vec<(&ChunkPos, &GpuChunk)> = self.chunks.iter().collect();
        let timing = self.profiling.as_mut().and_then(|profiling| {
            profiling.begin(&self.device, &mut encoder, self.render_frame_id)
        });
        self.encode_sun_shadows(&mut encoder, &f, &visible);
        self.point_shadows
            .encode(&mut encoder, &f, &visible, &self.atlas_bg);
        self.encode_world(&mut encoder, &f, &visible);
        self.encode_hand(&mut encoder, &f);
        self.post
            .bloom(&self.queue, &mut encoder, f.bloom, f.daylight);
        // Acquire only after scene encoding, preserving late swapchain admission.
        let acquisition_start = work_start.map(|_| std::time::Instant::now());
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(error) => {
                if let (Some(profiling), Some(ticket)) = (&mut self.profiling, timing) {
                    profiling.cancel(ticket);
                }
                return Err(error);
            }
        };
        let acquisition_ms = acquisition_start.map(|start| start.elapsed().as_secs_f64() * 1000.0);
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.composite_and_ui(&mut encoder, &view, &f);
        let requested_shot = self
            .pending_screenshot
            .take()
            .map(|path| (path, self.pending_capture_metadata.take()));
        let capture_frame = requested_shot.is_some();
        let shot = requested_shot.map(|(path, metadata)| {
            self.encode_capture(&mut encoder, &f, &visible, path, metadata)
        });
        if let (Some(profiling), Some(ticket)) = (&mut self.profiling, timing) {
            profiling.end(&mut encoder, ticket);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        if let (Some(profiling), Some(ticket)) = (&mut self.profiling, timing) {
            profiling.submitted(ticket, capture_frame);
        }
        let present_start = work_start.map(|_| std::time::Instant::now());
        frame.present();
        if let (Some(start), Some(acquisition_ms), Some(present_start)) =
            (work_start, acquisition_ms, present_start)
        {
            let present_ms = present_start.elapsed().as_secs_f64() * 1000.0;
            let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
            self.last_render_work = Some(super::RenderWorkTiming {
                renderer_frame_id: self.render_frame_id,
                capture_frame,
                gpu_recorded: timing.is_some(),
                cpu_submission_ms: wall_ms - acquisition_ms - present_ms,
                acquisition_ms,
                present_ms,
                wall_ms,
            });
        }
        if let Some(shot) = shot {
            self.write_capture(shot);
        }
        Ok(())
    }
}

mod capture;
mod composite;
mod diagnostic;
mod outline;
mod preparation;
mod scene;
mod sun;
