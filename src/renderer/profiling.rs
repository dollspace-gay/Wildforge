//! Optional completed-GPU frame timings with bounded asynchronous readback.
//!
//! `WILDFORGE_GPU_PROFILE=1` enables timestamps; unset or `0` disables them.
//! The interval covers the frame's actual rendering commands, including capture
//! replay when `capture_frame` is true. Uploads before encoding, query resolve,
//! readback, host frame work, and presentation wait are outside the interval.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

const READBACK_SLOTS: usize = 4;
const TIMESTAMPS_PER_FRAME: u32 = 2;
const READBACK_BYTES: u64 = TIMESTAMPS_PER_FRAME as u64 * wgpu::QUERY_SIZE as u64;
const MAX_COMPLETED: usize = 256;

/// A completed timestamp interval from the selected hardware GPU.
#[derive(Debug, serde::Serialize)]
pub(crate) struct GpuFrameTiming {
    /// Actual `Renderer::render` call number, beginning at one.
    pub(crate) frame_id: u64,
    /// Completed device-clock elapsed time, converted with its timestamp period.
    pub(crate) gpu_ms: f64,
    /// Screenshot/diagnostic replay contributed to this interval.
    pub(crate) capture_frame: bool,
}

/// Sampling coverage, including backpressure without a render-thread wait.
#[derive(Debug, serde::Serialize)]
pub(crate) struct GpuProfileStats {
    pub(crate) submitted: u64,
    pub(crate) completed: u64,
    pub(crate) skipped_ring_full: u64,
    pub(crate) pending_readbacks: usize,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum GpuProfileError {
    #[error("WILDFORGE_GPU_PROFILE must be 0 or 1, received {0:?}")]
    InvalidSetting(String),
    #[error("could not read WILDFORGE_GPU_PROFILE: {0}")]
    Setting(#[source] std::env::VarError),
    #[error("GPU profiling requires {missing:?} on selected adapter {adapter}")]
    MissingFeatures {
        adapter: String,
        missing: wgpu::Features,
    },
    #[error("GPU profiling received invalid device timestamp period {0}")]
    InvalidTimestampPeriod(f32),
    #[error("GPU profiling device poll failed: {0}")]
    Poll(#[source] wgpu::PollError),
    #[error("GPU timestamp readback failed for frame {frame_id}: {source}")]
    Readback {
        frame_id: u64,
        #[source]
        source: wgpu::BufferAsyncError,
    },
    #[error("GPU timestamp callback disconnected for frame {0}")]
    CallbackDisconnected(u64),
    #[error("GPU timestamp readback has {0} bytes, expected 16")]
    ReadbackSize(usize),
    #[error("GPU timestamps moved backward for frame {frame_id}: {start} -> {end}")]
    NonMonotonic { frame_id: u64, start: u64, end: u64 },
    #[error("GPU timestamp ring state does not match frame {0}")]
    RingState(u64),
    #[error("GPU profiling results were not consumed; completed-result bound reached")]
    ResultsNotConsumed,
}

pub(super) fn requested() -> Result<bool, GpuProfileError> {
    match std::env::var("WILDFORGE_GPU_PROFILE") {
        Ok(value) => parse_setting(Some(&value)),
        Err(std::env::VarError::NotPresent) => Ok(false),
        Err(error) => Err(GpuProfileError::Setting(error)),
    }
}

fn parse_setting(value: Option<&str>) -> Result<bool, GpuProfileError> {
    match value {
        None | Some("0") => Ok(false),
        Some("1") => Ok(true),
        Some(value) => Err(GpuProfileError::InvalidSetting(value.to_owned())),
    }
}

pub(super) fn required_features(
    available: wgpu::Features,
    adapter: &str,
) -> Result<wgpu::Features, GpuProfileError> {
    let required =
        wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
    let missing = required - available;
    if !missing.is_empty() {
        return Err(GpuProfileError::MissingFeatures {
            adapter: adapter.to_owned(),
            missing,
        });
    }
    Ok(required)
}

#[derive(Debug)]
enum SlotState {
    Idle,
    Recording(u64),
    Mapping {
        frame_id: u64,
        capture_frame: bool,
        completion: Receiver<Result<(), wgpu::BufferAsyncError>>,
    },
}

#[derive(Debug)]
struct ReadbackSlot {
    buffer: wgpu::Buffer,
    state: SlotState,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FrameTicket {
    slot: usize,
    frame_id: u64,
}

#[derive(Debug)]
pub(super) struct GpuProfiler {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    slots: [ReadbackSlot; READBACK_SLOTS],
    timestamp_period: f32,
    measurements: VecDeque<GpuFrameTiming>,
    failure: Option<Arc<GpuProfileError>>,
    submitted: u64,
    completed: u64,
    skipped_ring_full: u64,
}

impl GpuProfiler {
    pub(super) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self, GpuProfileError> {
        required_features(device.features(), "configured device")?;
        let timestamp_period = queue.get_timestamp_period();
        if !timestamp_period.is_finite() || timestamp_period <= 0.0 {
            return Err(GpuProfileError::InvalidTimestampPeriod(timestamp_period));
        }
        let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("frame GPU timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: READBACK_SLOTS as u32 * TIMESTAMPS_PER_FRAME,
        });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame GPU timestamp resolve"),
            size: READBACK_SLOTS as u64 * wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let slots = std::array::from_fn(|_| ReadbackSlot {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("frame GPU timestamp readback"),
                size: READBACK_BYTES,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            state: SlotState::Idle,
        });
        Ok(Self {
            queries,
            resolve,
            slots,
            timestamp_period,
            measurements: VecDeque::with_capacity(READBACK_SLOTS),
            failure: None,
            submitted: 0,
            completed: 0,
            skipped_ring_full: 0,
        })
    }

    pub(super) fn begin(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        frame_id: u64,
    ) -> Option<FrameTicket> {
        self.poll(device);
        if self.failure.is_some() {
            return None;
        }
        let Some(slot) = self
            .slots
            .iter()
            .position(|slot| matches!(slot.state, SlotState::Idle))
        else {
            self.skipped_ring_full += 1;
            return None;
        };
        self.slots[slot].state = SlotState::Recording(frame_id);
        encoder.write_timestamp(&self.queries, slot as u32 * TIMESTAMPS_PER_FRAME);
        Some(FrameTicket { slot, frame_id })
    }

    pub(super) fn end(&mut self, encoder: &mut wgpu::CommandEncoder, ticket: FrameTicket) {
        if !self.is_recording(ticket) {
            self.fail(GpuProfileError::RingState(ticket.frame_id));
            return;
        }
        let first = ticket.slot as u32 * TIMESTAMPS_PER_FRAME;
        let offset = ticket.slot as u64 * wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT;
        encoder.write_timestamp(&self.queries, first + 1);
        encoder.resolve_query_set(
            &self.queries,
            first..first + TIMESTAMPS_PER_FRAME,
            &self.resolve,
            offset,
        );
        encoder.copy_buffer_to_buffer(
            &self.resolve,
            offset,
            &self.slots[ticket.slot].buffer,
            0,
            READBACK_BYTES,
        );
    }

    pub(super) fn submitted(&mut self, ticket: FrameTicket, capture_frame: bool) {
        if !self.is_recording(ticket) {
            self.fail(GpuProfileError::RingState(ticket.frame_id));
            return;
        }
        let (send, completion) = channel();
        let slot = &mut self.slots[ticket.slot];
        slot.buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                // At most one callback message per ring slot; sending never waits.
                // The receiver can disappear only when the profiling owner leaves.
                let _ = send.send(result);
            });
        slot.state = SlotState::Mapping {
            frame_id: ticket.frame_id,
            capture_frame,
            completion,
        };
        self.submitted += 1;
    }

    pub(super) fn cancel(&mut self, ticket: FrameTicket) {
        if self.is_recording(ticket) {
            self.slots[ticket.slot].state = SlotState::Idle;
        } else {
            self.fail(GpuProfileError::RingState(ticket.frame_id));
        }
    }

    fn is_recording(&self, ticket: FrameTicket) -> bool {
        matches!(self.slots[ticket.slot].state, SlotState::Recording(frame_id) if frame_id == ticket.frame_id)
    }

    fn fail(&mut self, error: GpuProfileError) {
        Self::retain_failure(&mut self.failure, error);
    }

    fn retain_failure(failure: &mut Option<Arc<GpuProfileError>>, error: GpuProfileError) {
        if failure.is_none() {
            eprintln!("GPU profiling failed: {error}");
            *failure = Some(Arc::new(error));
        }
    }

    fn poll(&mut self, device: &wgpu::Device) {
        if self.failure.is_some() {
            return;
        }
        if let Err(error) = device.poll(wgpu::PollType::Poll) {
            self.fail(GpuProfileError::Poll(error));
            return;
        }
        for slot in &mut self.slots {
            let SlotState::Mapping {
                frame_id,
                capture_frame,
                completion,
            } = &slot.state
            else {
                continue;
            };
            let (frame_id, capture_frame) = (*frame_id, *capture_frame);
            let result = match completion.try_recv() {
                Ok(Ok(())) => {
                    let bytes = slot.buffer.slice(..).get_mapped_range();
                    let elapsed = elapsed_ms(frame_id, &bytes, self.timestamp_period);
                    drop(bytes);
                    elapsed.map(|gpu_ms| GpuFrameTiming {
                        frame_id,
                        gpu_ms,
                        capture_frame,
                    })
                }
                Ok(Err(source)) => Err(GpuProfileError::Readback { frame_id, source }),
                Err(TryRecvError::Disconnected) => {
                    Err(GpuProfileError::CallbackDisconnected(frame_id))
                }
                Err(TryRecvError::Empty) => continue,
            };
            slot.buffer.unmap();
            slot.state = SlotState::Idle;
            match result {
                Ok(measurement) if self.measurements.len() < MAX_COMPLETED => {
                    self.measurements.push_back(measurement);
                    self.completed += 1;
                }
                Ok(_) => {
                    Self::retain_failure(&mut self.failure, GpuProfileError::ResultsNotConsumed);
                }
                Err(error) => Self::retain_failure(&mut self.failure, error),
            }
        }
    }

    pub(super) fn take_completed(
        &mut self,
        device: &wgpu::Device,
    ) -> Result<Vec<GpuFrameTiming>, Arc<GpuProfileError>> {
        self.poll(device);
        if let Some(error) = &self.failure {
            return Err(Arc::clone(error));
        }
        Ok(self.measurements.drain(..).collect())
    }

    pub(super) fn stats(&self) -> GpuProfileStats {
        GpuProfileStats {
            submitted: self.submitted,
            completed: self.completed,
            skipped_ring_full: self.skipped_ring_full,
            pending_readbacks: self
                .slots
                .iter()
                .filter(|slot| matches!(slot.state, SlotState::Mapping { .. }))
                .count(),
        }
    }
}

fn elapsed_ms(frame_id: u64, bytes: &[u8], timestamp_period: f32) -> Result<f64, GpuProfileError> {
    if bytes.len() != READBACK_BYTES as usize {
        return Err(GpuProfileError::ReadbackSize(bytes.len()));
    }
    let mut start = [0u8; 8];
    let mut end = [0u8; 8];
    start.copy_from_slice(&bytes[..8]);
    end.copy_from_slice(&bytes[8..]);
    let (start, end) = (u64::from_ne_bytes(start), u64::from_ne_bytes(end));
    let ticks = end
        .checked_sub(start)
        .ok_or(GpuProfileError::NonMonotonic {
            frame_id,
            start,
            end,
        })?;
    Ok(ticks as f64 * f64::from(timestamp_period) / 1_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::{GpuProfileError, elapsed_ms, parse_setting, required_features};

    #[test]
    fn profiling_requires_an_explicit_one_and_rejects_unsupported_device_features() {
        assert!(!parse_setting(None).unwrap());
        assert!(!parse_setting(Some("0")).unwrap());
        assert!(parse_setting(Some("1")).unwrap());
        for value in ["", "true", "false", "2", " 1"] {
            assert!(parse_setting(Some(value)).is_err());
        }
        assert!(matches!(
            required_features(wgpu::Features::TIMESTAMP_QUERY, "test adapter"),
            Err(GpuProfileError::MissingFeatures { .. })
        ));
        let both =
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        assert_eq!(required_features(both, "test adapter").unwrap(), both);
    }

    #[test]
    fn completed_device_ticks_use_the_reported_period_and_refuse_invalid_readback() {
        let bytes = [100u64.to_ne_bytes(), 1_000_100u64.to_ne_bytes()].concat();
        assert_eq!(elapsed_ms(9, &bytes, 2.0).unwrap(), 2.0);
        assert!(matches!(
            elapsed_ms(9, &bytes[..8], 2.0),
            Err(GpuProfileError::ReadbackSize(8))
        ));
        let backwards = [200u64.to_ne_bytes(), 100u64.to_ne_bytes()].concat();
        assert!(matches!(
            elapsed_ms(9, &backwards, 2.0),
            Err(GpuProfileError::NonMonotonic { frame_id: 9, .. })
        ));
    }
}
