//! Bounded exact joins and the latest completed native rendering-work window.

use std::collections::VecDeque;

use crate::renderer::{
    GPU_PROFILE_READBACK_SLOTS, GpuFrameTiming, GpuProfileStats, RenderWorkCapabilities,
    RenderWorkTiming,
};
use crate::visual_capture::{RenderWorkEvidence, RenderWorkSample};

use super::{Game, SHOT_SETTLE_FRAMES};

const WINDOW_FRAMES: usize = 120;

#[derive(Debug, thiserror::Error)]
pub(super) enum RenderWorkError {
    #[error("render-work hardware capabilities or coverage are incomplete")]
    Coverage,
    #[error("render-work host frame is invalid or duplicated")]
    Host,
    #[error("render-work device sample is invalid, missing its host, or duplicated")]
    Device,
    #[error("render-work asynchronous completion exceeded the four-slot bound")]
    CompletionGap,
}

#[derive(Debug)]
struct PendingFrame {
    host: RenderWorkTiming,
    gpu_ms: Option<f64>,
}

/// No waits, timing-based filtering, or unbounded retention are permitted here.
#[derive(Debug, Default)]
pub(super) struct RenderWorkWindow {
    samples: VecDeque<RenderWorkSample>,
    pending: VecDeque<PendingFrame>,
    start_id: Option<u64>,
    latest_renderer_frame_id: u64,
    last_observed_id: Option<u64>,
    settled_frames: u64,
    stats: Option<GpuProfileStats>,
    capabilities: Option<RenderWorkCapabilities>,
}

impl RenderWorkWindow {
    fn clear_window(&mut self) {
        self.samples.clear();
        self.pending.clear();
        self.start_id = None;
    }

    pub(super) fn reset(&mut self) {
        self.clear_window();
        self.last_observed_id = None;
        self.settled_frames = 0;
    }

    pub(super) fn observe(
        &mut self,
        host: Option<RenderWorkTiming>,
        world_ready: bool,
        completed: &[GpuFrameTiming],
        stats: Option<GpuProfileStats>,
        capabilities: Option<RenderWorkCapabilities>,
    ) -> Result<(), RenderWorkError> {
        let result = self.observe_inner(host, world_ready, completed, stats, capabilities);
        if result.is_err() {
            self.reset();
        }
        result
    }

    fn observe_inner(
        &mut self,
        host: Option<RenderWorkTiming>,
        world_ready: bool,
        completed: &[GpuFrameTiming],
        stats: Option<GpuProfileStats>,
        capabilities: Option<RenderWorkCapabilities>,
    ) -> Result<(), RenderWorkError> {
        let previous_stats = self.stats;
        let previous_capabilities = self.capabilities;
        self.stats = stats;
        self.capabilities = capabilities;
        let (Some(stats), Some(capabilities)) = (stats, capabilities) else {
            return Err(RenderWorkError::Coverage);
        };
        if !valid_coverage(stats, capabilities)
            || previous_stats.is_some_and(|previous| {
                stats.submitted < previous.submitted
                    || stats.completed < previous.completed
                    || stats.skipped_ring_full < previous.skipped_ring_full
                    || stats.completed.checked_sub(previous.completed)
                        != Some(completed.len() as u64)
            })
            || previous_capabilities.is_some_and(|previous| capabilities != previous)
        {
            return Err(RenderWorkError::Coverage);
        }
        let Some(host) = host else {
            self.reset();
            return Ok(());
        };
        if !valid_host(host)
            || self
                .last_observed_id
                .is_some_and(|previous| host.renderer_frame_id <= previous)
        {
            return Err(RenderWorkError::Host);
        }
        if self
            .last_observed_id
            .is_some_and(|previous| previous.checked_add(1) != Some(host.renderer_frame_id))
        {
            self.reset();
        }
        self.latest_renderer_frame_id = host.renderer_frame_id;
        self.last_observed_id = Some(host.renderer_frame_id);
        if !world_ready || host.capture_frame {
            self.clear_window();
            self.settled_frames = 0;
            return Ok(());
        }
        self.settled_frames = self.settled_frames.saturating_add(1);
        if !host.gpu_recorded
            || previous_stats
                .is_some_and(|previous| stats.skipped_ring_full != previous.skipped_ring_full)
            || self.settled_frames < SHOT_SETTLE_FRAMES
        {
            self.clear_window();
            return Ok(());
        }
        self.start_id.get_or_insert(host.renderer_frame_id);
        self.pending.push_back(PendingFrame { host, gpu_ms: None });
        for gpu in completed {
            // Results preceding this window belong to warmup or a reset frame.
            if self.start_id.is_some_and(|first| gpu.frame_id < first) {
                continue;
            }
            let pending = self
                .pending
                .iter_mut()
                .find(|frame| frame.host.renderer_frame_id == gpu.frame_id)
                .ok_or(RenderWorkError::Device)?;
            if gpu.capture_frame
                || !gpu.gpu_ms.is_finite()
                || gpu.gpu_ms < 0.0
                || pending.gpu_ms.is_some()
            {
                return Err(RenderWorkError::Device);
            }
            pending.gpu_ms = Some(gpu.gpu_ms);
        }
        while self
            .pending
            .front()
            .is_some_and(|frame| frame.gpu_ms.is_some())
        {
            let frame = self
                .pending
                .pop_front()
                .expect("completed front is present");
            self.samples.push_back(RenderWorkSample {
                renderer_frame_id: frame.host.renderer_frame_id,
                settled: true,
                capture_frame: false,
                cpu_submission_ms: frame.host.cpu_submission_ms,
                gpu_ms: frame.gpu_ms.expect("completed front has a device interval"),
                acquisition_ms: frame.host.acquisition_ms,
                present_ms: frame.host.present_ms,
                wall_ms: frame.host.wall_ms,
            });
            if self.samples.len() > WINDOW_FRAMES {
                self.samples.pop_front();
            }
        }
        if self.pending.len() > GPU_PROFILE_READBACK_SLOTS
            || self.pending.front().is_some_and(|frame| {
                host.renderer_frame_id - frame.host.renderer_frame_id
                    >= GPU_PROFILE_READBACK_SLOTS as u64
            })
        {
            return Err(RenderWorkError::CompletionGap);
        }
        Ok(())
    }

    pub(super) fn ready(&self) -> bool {
        self.samples.len() == WINDOW_FRAMES
            && self
                .stats
                .is_some_and(|stats| stats.completed >= WINDOW_FRAMES as u64)
            && self.samples.back().is_some_and(|last| {
                self.latest_renderer_frame_id
                    .checked_sub(last.renderer_frame_id)
                    .is_some_and(|tail| tail <= GPU_PROFILE_READBACK_SLOTS as u64)
            })
    }

    pub(super) fn evidence(&self) -> Option<RenderWorkEvidence> {
        if !self.ready() {
            return None;
        }
        let stats = self.stats?;
        let capabilities = self.capabilities?;
        let samples: Vec<_> = self.samples.iter().cloned().collect();
        Some(RenderWorkEvidence {
            schema_version: 1,
            sample_count: WINDOW_FRAMES,
            first_renderer_frame_id: samples.first()?.renderer_frame_id,
            last_renderer_frame_id: samples.last()?.renderer_frame_id,
            latest_renderer_frame_id: self.latest_renderer_frame_id,
            median_cpu_submission_ms: median(&samples, |sample| sample.cpu_submission_ms),
            median_gpu_ms: median(&samples, |sample| sample.gpu_ms),
            median_acquisition_ms: median(&samples, |sample| sample.acquisition_ms),
            median_present_ms: median(&samples, |sample| sample.present_ms),
            median_wall_ms: median(&samples, |sample| sample.wall_ms),
            samples,
            timestamp_period_ns: capabilities.timestamp_period_ns,
            timestamp_query: capabilities.timestamp_query,
            timestamp_query_inside_encoders: capabilities.timestamp_query_inside_encoders,
            gpu_submitted: stats.submitted,
            gpu_completed: stats.completed,
            gpu_skipped_ring_full: stats.skipped_ring_full,
            gpu_pending_readbacks: stats.pending_readbacks,
            // Every observed skip clears the current window before any joins.
            skipped_ring_full_in_window: 0,
        })
    }
}

fn valid_coverage(stats: GpuProfileStats, capabilities: RenderWorkCapabilities) -> bool {
    capabilities.timestamp_period_ns.is_finite()
        && capabilities.timestamp_period_ns > 0.0
        && capabilities.timestamp_query
        && capabilities.timestamp_query_inside_encoders
        && stats.pending_readbacks <= GPU_PROFILE_READBACK_SLOTS
        && stats.completed.checked_add(stats.pending_readbacks as u64) == Some(stats.submitted)
}

fn valid_host(host: RenderWorkTiming) -> bool {
    let values = [
        host.cpu_submission_ms,
        host.acquisition_ms,
        host.present_ms,
        host.wall_ms,
    ];
    host.renderer_frame_id > 0
        && values
            .iter()
            .all(|value| value.is_finite() && *value >= 0.0)
        && (host.wall_ms - host.cpu_submission_ms - host.acquisition_ms - host.present_ms).abs()
            <= 0.000_002
}

fn median(samples: &[RenderWorkSample], value: impl Fn(&RenderWorkSample) -> f64) -> f64 {
    let mut values: Vec<_> = samples.iter().map(value).collect();
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    (values[middle - 1] + values[middle]) / 2.0
}

impl Game {
    /// Consume the GPU owner once; feed both receipts and the optional JSONL writer.
    pub(in crate::game) fn collect_render_work(&mut self) -> serde_json::Value {
        if self.render_work.is_none()
            && !self.frame_profile.enabled()
            && self.renderer.gpu_profile_stats().is_none()
        {
            return serde_json::Value::Null;
        }
        let host = self.renderer.take_render_work_timing();
        match self.renderer.take_gpu_timings() {
            Ok(samples) => {
                let world_ready =
                    self.render_work.is_some() && self.in_world && self.chunk_work_pending() == 0;
                if let Some(window) = &mut self.render_work
                    && let Err(error) = window.observe(
                        host,
                        world_ready,
                        &samples,
                        self.renderer.gpu_profile_stats(),
                        self.renderer.render_work_capabilities(),
                    )
                {
                    eprintln!("render work: {error}; current window reset");
                }
                if self.frame_profile.enabled() {
                    serde_json::json!({"samples": samples, "render_work_host": host})
                } else {
                    serde_json::Value::Null
                }
            }
            Err(error) => {
                if let Some(window) = &mut self.render_work {
                    window.reset();
                }
                eprintln!("frame profiling: GPU measurement failed: {error}");
                if self.frame_profile.enabled() {
                    serde_json::json!({"error": error.to_string()})
                } else {
                    serde_json::Value::Null
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "render_work_tests.rs"]
mod tests;
