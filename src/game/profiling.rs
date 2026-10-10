//! Opt-in complete-frame CPU measurements. Ordinary play has no writer or samples.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::time::Instant;

use serde_json::{Value, json};

#[derive(Debug, Default)]
pub(super) struct FrameProfiler {
    writer: Option<BufWriter<std::fs::File>>,
    active: Option<FrameSample>,
    previous_start: Option<Instant>,
}

#[derive(Debug)]
struct FrameSample {
    start: Instant,
    stage_start: Instant,
    interval_ms: Option<f64>,
    stages: BTreeMap<&'static str, f64>,
    capture: bool,
}

impl FrameProfiler {
    pub(super) fn from_env() -> Self {
        let Some(path) = std::env::var_os("WILDFORGE_FRAME_PROFILE") else {
            return Self::default();
        };
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => Self {
                writer: Some(BufWriter::new(file)),
                active: None,
                previous_start: None,
            },
            Err(error) => {
                eprintln!("frame profiling: cannot create {path:?}: {error}");
                Self::default()
            }
        }
    }

    pub(super) fn enabled(&self) -> bool {
        self.writer.is_some()
    }

    pub(super) fn begin(&mut self) {
        if !self.enabled() {
            return;
        }
        let now = Instant::now();
        let interval_ms = self
            .previous_start
            .replace(now)
            .map(|previous| (now - previous).as_secs_f64() * 1000.0);
        self.active = Some(FrameSample {
            start: now,
            stage_start: now,
            interval_ms,
            stages: BTreeMap::new(),
            capture: false,
        });
    }

    pub(super) fn mark(&mut self, stage: &'static str) {
        if let Some(sample) = &mut self.active {
            let now = Instant::now();
            sample
                .stages
                .insert(stage, (now - sample.stage_start).as_secs_f64() * 1000.0);
            sample.stage_start = now;
        }
    }

    /// Nested measurements do not alter the ordered whole-frame stage boundaries.
    pub(super) fn record_elapsed(&mut self, name: &'static str, start: Option<Instant>) {
        if let (Some(sample), Some(start)) = (&mut self.active, start) {
            sample
                .stages
                .insert(name, start.elapsed().as_secs_f64() * 1000.0);
        }
    }

    pub(super) fn mark_capture(&mut self, capture: bool) {
        if let Some(sample) = &mut self.active {
            sample.capture |= capture;
        }
    }

    pub(super) fn finish(&mut self, context: Value, gpu: Value) {
        let Some(sample) = self.active.take() else {
            return;
        };
        let row = json!({
            "schema": 1,
            "cpu_total_ms": sample.start.elapsed().as_secs_f64() * 1000.0,
            "interval_ms": sample.interval_ms,
            "cpu_stages_ms": sample.stages,
            "capture_frame": sample.capture,
            "context": context,
            "completed_gpu": gpu,
        });
        let Some(writer) = &mut self.writer else {
            return;
        };
        let result = serde_json::to_writer(&mut *writer, &row)
            .map_err(std::io::Error::other)
            .and_then(|()| writer.write_all(b"\n"));
        if let Err(error) = result {
            eprintln!("frame profiling: write failed: {error}");
            self.writer = None;
        }
    }
}

impl Drop for FrameProfiler {
    fn drop(&mut self) {
        if let Some(writer) = &mut self.writer
            && let Err(error) = writer.flush()
        {
            eprintln!("frame profiling: final flush failed: {error}");
        }
    }
}
