//! Opt-in host work intervals accompanying the existing asynchronous GPU ring.

use super::Renderer;

/// Successful render-call work before screenshot readback or file writing.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub(crate) struct RenderWorkTiming {
    pub(crate) renderer_frame_id: u64,
    pub(crate) capture_frame: bool,
    /// False when this call could not reserve an asynchronous timestamp slot.
    pub(crate) gpu_recorded: bool,
    pub(crate) cpu_submission_ms: f64,
    pub(crate) acquisition_ms: f64,
    pub(crate) present_ms: f64,
    pub(crate) wall_ms: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RenderWorkCapabilities {
    pub(crate) timestamp_period_ns: f64,
    pub(crate) timestamp_query: bool,
    pub(crate) timestamp_query_inside_encoders: bool,
}

impl Renderer {
    pub(crate) fn render_work_enabled(&self) -> bool {
        self.render_work_enabled
    }

    pub(crate) fn take_render_work_timing(&mut self) -> Option<RenderWorkTiming> {
        self.last_render_work.take()
    }

    pub(crate) fn render_work_capabilities(&self) -> Option<RenderWorkCapabilities> {
        self.profiling.as_ref().map(|profiling| {
            let features = self.device.features();
            RenderWorkCapabilities {
                timestamp_period_ns: profiling.timestamp_period_ns(),
                timestamp_query: features.contains(wgpu::Features::TIMESTAMP_QUERY),
                timestamp_query_inside_encoders: features
                    .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS),
            }
        })
    }
}
