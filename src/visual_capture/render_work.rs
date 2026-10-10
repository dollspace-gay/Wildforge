//! Independent validation of completed rendering work in a native capture.

use super::{CaptureTelemetry, RenderWorkEvidence};

const WINDOW_FRAMES: usize = 120;
const READBACK_SLOTS: u64 = 4;

pub(super) fn validate_capture(telemetry: &CaptureTelemetry) -> Result<(f64, f64), String> {
    let work = telemetry
        .render_work
        .as_ref()
        .ok_or("cracked-geode performance has no completed render-work window")?;
    if telemetry.eligible_frame < 1200
        || telemetry.eligible_frame >= telemetry.frame
        || work.latest_renderer_frame_id.checked_add(1) != Some(telemetry.frame)
        || !telemetry.settled
        || work
            .latest_renderer_frame_id
            .checked_sub(work.last_renderer_frame_id)
            .and_then(|tail| tail.checked_add(WINDOW_FRAMES as u64 + 10 - 1))
            .is_none_or(|minimum| telemetry.settled_frames < minimum)
    {
        return Err("render-work window is stale or precedes the declared warmup".into());
    }
    validate(work)
}

pub(super) fn close(left: f64, right: f64) -> bool {
    left.is_finite() && right.is_finite() && (left - right).abs() <= 0.000_002
}

pub(super) fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    }
}

pub(super) fn validate(value: &RenderWorkEvidence) -> Result<(f64, f64), String> {
    if value.schema_version != 1
        || value.sample_count != WINDOW_FRAMES
        || value.samples.len() != WINDOW_FRAMES
        || value.first_renderer_frame_id == 0
        || value
            .last_renderer_frame_id
            .checked_sub(value.first_renderer_frame_id)
            != Some(WINDOW_FRAMES as u64 - 1)
        || value
            .latest_renderer_frame_id
            .checked_sub(value.last_renderer_frame_id)
            .is_none_or(|tail| tail > READBACK_SLOTS)
        || value.gpu_completed < WINDOW_FRAMES as u64
        || value
            .gpu_submitted
            .checked_add(value.gpu_skipped_ring_full)
            .is_none_or(|attempts| attempts > value.latest_renderer_frame_id)
        || value.gpu_pending_readbacks > READBACK_SLOTS as usize
        || value
            .gpu_completed
            .checked_add(value.gpu_pending_readbacks as u64)
            != Some(value.gpu_submitted)
        || value.skipped_ring_full_in_window != 0
        || !value.timestamp_period_ns.is_finite()
        || value.timestamp_period_ns <= 0.0
        || !value.timestamp_query
        || !value.timestamp_query_inside_encoders
    {
        return Err("render-work window has incomplete or stale hardware coverage".into());
    }
    let mut columns: [Vec<f64>; 5] = std::array::from_fn(|_| Vec::with_capacity(WINDOW_FRAMES));
    for (index, sample) in value.samples.iter().enumerate() {
        let row = [
            sample.cpu_submission_ms,
            sample.gpu_ms,
            sample.acquisition_ms,
            sample.present_ms,
            sample.wall_ms,
        ];
        if sample.renderer_frame_id != value.first_renderer_frame_id + index as u64
            || !sample.settled
            || sample.capture_frame
            || row
                .iter()
                .any(|number| !number.is_finite() || *number < 0.0)
            || !close(
                sample.wall_ms,
                sample.cpu_submission_ms + sample.acquisition_ms + sample.present_ms,
            )
        {
            return Err("render-work sample is invalid, unjoined, or includes capture work".into());
        }
        for (column, number) in columns.iter_mut().zip(row) {
            column.push(number);
        }
    }
    let recomputed = columns.each_ref().map(|values| median(values));
    let reported = [
        value.median_cpu_submission_ms,
        value.median_gpu_ms,
        value.median_acquisition_ms,
        value.median_present_ms,
        value.median_wall_ms,
    ];
    if !recomputed.iter().zip(reported).all(|(a, b)| close(*a, b)) {
        return Err("render-work medians disagree with completed samples".into());
    }
    Ok((recomputed[0], recomputed[1]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn receipt() -> Value {
        let samples: Vec<_> = (1000..1120)
            .map(|id| {
                json!({
                    "renderer_frame_id": id, "cpu_submission_ms": 0.4, "gpu_ms": 0.7,
                    "acquisition_ms": 14.0, "present_ms": 0.1, "wall_ms": 14.5,
                    "settled": true, "capture_frame": false,
                })
            })
            .collect();
        json!({"schema_version": 1, "sample_count": 120,
            "first_renderer_frame_id": 1000, "last_renderer_frame_id": 1119,
            "latest_renderer_frame_id": 1121, "samples": samples,
            "median_cpu_submission_ms": 0.4, "median_gpu_ms": 0.7,
            "median_acquisition_ms": 14.0, "median_present_ms": 0.1,
            "median_wall_ms": 14.5, "timestamp_period_ns": 1.0,
            "timestamp_query": true, "timestamp_query_inside_encoders": true,
            "gpu_submitted": 1121, "gpu_completed": 1119,
            "gpu_skipped_ring_full": 0, "gpu_pending_readbacks": 2,
            "skipped_ring_full_in_window": 0})
    }

    #[test]
    fn completed_work_reconciles_without_counting_presentation() {
        let evidence = serde_json::from_value(receipt()).unwrap();
        assert_eq!(validate(&evidence).unwrap(), (0.4, 0.7));
    }

    #[test]
    fn rejects_missing_unjoined_unsettled_capture_and_false_medians() {
        for (pointer, replacement) in [
            ("/samples/60/renderer_frame_id", json!(1059)),
            ("/samples/0/settled", json!(false)),
            ("/samples/0/capture_frame", json!(true)),
            ("/samples/0/cpu_submission_ms", json!(0.0)),
            ("/latest_renderer_frame_id", json!(1124)),
            ("/gpu_completed", json!(119)),
            ("/gpu_skipped_ring_full", json!(1)),
            ("/skipped_ring_full_in_window", json!(1)),
            ("/timestamp_query", json!(false)),
            ("/timestamp_period_ns", json!(0)),
            ("/median_gpu_ms", json!(0.01)),
        ] {
            let mut value = receipt();
            *value.pointer_mut(pointer).unwrap() = replacement;
            let evidence = serde_json::from_value(value).unwrap();
            assert!(validate(&evidence).is_err(), "{pointer}");
        }
        let mut value = receipt();
        value["samples"].as_array_mut().unwrap().pop();
        let evidence = serde_json::from_value(value).unwrap();
        assert!(validate(&evidence).is_err());
    }

    #[test]
    fn capture_rejects_an_old_window_and_insufficient_warmup() {
        let mut value = receipt();
        for sample in value["samples"].as_array_mut().unwrap() {
            sample["renderer_frame_id"] =
                json!(sample["renderer_frame_id"].as_u64().unwrap() + 1000);
        }
        for field in [
            "first_renderer_frame_id",
            "last_renderer_frame_id",
            "latest_renderer_frame_id",
            "gpu_submitted",
            "gpu_completed",
        ] {
            value[field] = json!(value[field].as_u64().unwrap() + 1000);
        }
        let mut telemetry = CaptureTelemetry {
            frame: 2122,
            eligible_frame: 1200,
            settled: true,
            settled_frames: 131,
            fps: 60,
            simulation_ms: 0.1,
            draw_ms: 14.5,
            resident_chunks: 1,
            gpu_chunks: 1,
            opaque_chunks: 1,
            water_chunks: 0,
            empty_chunks: 0,
            dirty_chunks: 0,
            render_work: Some(serde_json::from_value(value).unwrap()),
        };
        assert_eq!(validate_capture(&telemetry).unwrap(), (0.4, 0.7));
        telemetry.frame += 1;
        assert!(validate_capture(&telemetry).is_err());
        telemetry.frame -= 1;
        telemetry.settled_frames = 130;
        assert!(validate_capture(&telemetry).is_err());
        telemetry.settled_frames = 131;
        telemetry.eligible_frame = 1199;
        assert!(validate_capture(&telemetry).is_err());
        telemetry.eligible_frame = telemetry.frame;
        assert!(validate_capture(&telemetry).is_err());
    }
}
