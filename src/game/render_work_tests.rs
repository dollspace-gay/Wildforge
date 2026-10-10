//! Exact host/device joins, bounded completion, and settlement failure receipts.

use super::{RenderWorkError, RenderWorkWindow};
use crate::renderer::{GpuFrameTiming, GpuProfileStats, RenderWorkCapabilities, RenderWorkTiming};
use crate::visual_capture::RenderWorkEvidence;

fn host(id: u64) -> RenderWorkTiming {
    let cpu_submission_ms = 0.1 + id as f64 / 1000.0;
    let acquisition_ms = 14.0 + (id % 3) as f64 / 100.0;
    let present_ms = 0.01;
    RenderWorkTiming {
        renderer_frame_id: id,
        capture_frame: false,
        gpu_recorded: true,
        cpu_submission_ms,
        acquisition_ms,
        present_ms,
        wall_ms: cpu_submission_ms + acquisition_ms + present_ms,
    }
}

fn gpu(id: u64) -> GpuFrameTiming {
    GpuFrameTiming {
        frame_id: id,
        gpu_ms: 0.3 + id as f64 / 1000.0,
        capture_frame: false,
    }
}

fn capabilities() -> RenderWorkCapabilities {
    RenderWorkCapabilities {
        timestamp_period_ns: 2.0,
        timestamp_query: true,
        timestamp_query_inside_encoders: true,
    }
}

fn stats(submitted: u64, completed: u64) -> GpuProfileStats {
    GpuProfileStats {
        submitted,
        completed,
        skipped_ring_full: 0,
        pending_readbacks: (submitted - completed) as usize,
    }
}

fn observe(
    window: &mut RenderWorkWindow,
    id: u64,
    completed: &[GpuFrameTiming],
    counts: GpuProfileStats,
) {
    window
        .observe(
            Some(host(id)),
            true,
            completed,
            Some(counts),
            Some(capabilities()),
        )
        .unwrap();
}

fn complete_window() -> RenderWorkWindow {
    let mut window = RenderWorkWindow::default();
    for id in 1..=129 {
        observe(&mut window, id, &[gpu(id)], stats(id, id));
        assert_eq!(window.ready(), id == 129);
    }
    window
}

fn close(left: f64, right: f64) {
    assert!((left - right).abs() < 1e-12, "{left} != {right}");
}

#[test]
fn fixed_latest_window_retains_all_values_and_recomputes_even_medians() {
    let mut window = complete_window();
    for id in 130..=200 {
        observe(&mut window, id, &[gpu(id)], stats(id, id));
    }
    let evidence = window.evidence().unwrap();
    assert_eq!(evidence.sample_count, 120);
    assert_eq!(evidence.samples.len(), 120);
    assert_eq!(
        (
            evidence.first_renderer_frame_id,
            evidence.last_renderer_frame_id
        ),
        (81, 200)
    );
    assert_eq!(evidence.latest_renderer_frame_id, 200);
    assert_eq!(
        (
            evidence.gpu_submitted,
            evidence.gpu_completed,
            evidence.gpu_pending_readbacks
        ),
        (200, 200, 0)
    );
    close(evidence.median_cpu_submission_ms, 0.2405);
    close(evidence.median_gpu_ms, 0.4405);
    close(evidence.median_acquisition_ms, 14.01);
    close(evidence.median_present_ms, 0.01);
    let mut wall: Vec<_> = (81..=200).map(|id| host(id).wall_ms).collect();
    wall.sort_by(f64::total_cmp);
    close(evidence.median_wall_ms, (wall[59] + wall[60]) / 2.0);
    for (sample, id) in evidence.samples.iter().zip(81..=200) {
        assert_eq!(sample.renderer_frame_id, id);
        assert!(sample.settled && !sample.capture_frame);
        close(sample.cpu_submission_ms, host(id).cpu_submission_ms);
        close(sample.gpu_ms, gpu(id).gpu_ms);
        close(
            sample.wall_ms,
            sample.cpu_submission_ms + sample.acquisition_ms + sample.present_ms,
        );
    }
    let text = toml::to_string(&evidence).unwrap();
    assert_eq!(
        toml::from_str::<RenderWorkEvidence>(&text).unwrap(),
        evidence
    );
}

#[test]
fn delayed_and_out_of_order_results_join_exact_ids_without_waiting_or_selecting_values() {
    let mut delayed = RenderWorkWindow::default();
    for id in 1..=131 {
        let completed = if id > 2 { vec![gpu(id - 2)] } else { vec![] };
        observe(
            &mut delayed,
            id,
            &completed,
            stats(id, id.saturating_sub(2)),
        );
    }
    let evidence = delayed.evidence().unwrap();
    assert_eq!(
        (
            evidence.first_renderer_frame_id,
            evidence.last_renderer_frame_id,
            evidence.latest_renderer_frame_id,
            evidence.gpu_pending_readbacks
        ),
        (10, 129, 131, 2)
    );
    close(evidence.median_cpu_submission_ms, 0.1695);
    close(evidence.median_gpu_ms, 0.3695);

    let mut reordered = complete_window();
    observe(&mut reordered, 130, &[], stats(130, 129));
    observe(&mut reordered, 131, &[gpu(131)], stats(131, 130));
    assert_eq!(reordered.evidence().unwrap().last_renderer_frame_id, 129);
    observe(&mut reordered, 132, &[gpu(132), gpu(130)], stats(132, 132));
    let evidence = reordered.evidence().unwrap();
    assert_eq!(
        (
            evidence.first_renderer_frame_id,
            evidence.last_renderer_frame_id
        ),
        (13, 132)
    );
    close(
        evidence.samples[118].cpu_submission_ms,
        host(131).cpu_submission_ms,
    );
    close(evidence.samples[118].gpu_ms, gpu(131).gpu_ms);
}

#[test]
fn missing_duplicate_unjoined_or_invalid_device_samples_destroy_the_current_window() {
    for completed in [
        vec![gpu(130), gpu(130)],
        vec![gpu(129)],
        vec![gpu(131)],
        vec![GpuFrameTiming {
            gpu_ms: f64::NAN,
            ..gpu(130)
        }],
        vec![GpuFrameTiming {
            gpu_ms: -0.1,
            ..gpu(130)
        }],
        vec![GpuFrameTiming {
            capture_frame: true,
            ..gpu(130)
        }],
    ] {
        let mut window = complete_window();
        assert!(matches!(
            window.observe(
                Some(host(130)),
                true,
                &completed,
                Some(stats(130, 130)),
                Some(capabilities())
            ),
            Err(RenderWorkError::Device | RenderWorkError::Coverage)
        ));
        assert!(window.evidence().is_none());
    }
    let mut missing = complete_window();
    observe(&mut missing, 130, &[], stats(130, 129));
    for id in 131..=133 {
        observe(&mut missing, id, &[gpu(id)], stats(id, id - 1));
    }
    assert!(missing.ready()); // Four-frame tail is allowed; the missing ID is retained.
    assert!(matches!(
        missing.observe(
            Some(host(134)),
            true,
            &[gpu(134)],
            Some(stats(134, 133)),
            Some(capabilities())
        ),
        Err(RenderWorkError::CompletionGap)
    ));
    assert!(missing.evidence().is_none());
    assert!(missing.pending.is_empty() && missing.samples.is_empty());

    let mut lost_result = complete_window();
    assert!(matches!(
        lost_result.observe(
            Some(host(130)),
            true,
            &[],
            Some(stats(130, 130)),
            Some(capabilities())
        ),
        Err(RenderWorkError::Coverage)
    ));
    assert!(lost_result.evidence().is_none());
}

#[test]
fn missing_or_invalid_host_frames_and_unsettled_or_capture_frames_reset_the_window() {
    for invalid in [
        host(129),
        host(0),
        RenderWorkTiming {
            wall_ms: 0.0,
            ..host(130)
        },
        RenderWorkTiming {
            cpu_submission_ms: f64::INFINITY,
            ..host(130)
        },
        RenderWorkTiming {
            acquisition_ms: -1.0,
            ..host(130)
        },
    ] {
        let mut window = complete_window();
        assert!(matches!(
            window.observe(
                Some(invalid),
                true,
                &[gpu(130)],
                Some(stats(130, 130)),
                Some(capabilities())
            ),
            Err(RenderWorkError::Host)
        ));
        assert!(!window.ready());
    }
    for (next, world_ready) in [
        (None, true),
        (Some(host(131)), true),
        (Some(host(130)), false),
        (
            Some(RenderWorkTiming {
                capture_frame: true,
                ..host(130)
            }),
            true,
        ),
    ] {
        let mut window = complete_window();
        window
            .observe(
                next,
                world_ready,
                &[],
                Some(stats(130, 129)),
                Some(capabilities()),
            )
            .unwrap();
        assert!(window.evidence().is_none());
    }
}

#[test]
fn prior_warmup_skips_are_recorded_but_any_current_skip_resets_the_complete_window() {
    let mut window = complete_window();
    let skipped = GpuProfileStats {
        skipped_ring_full: 1,
        ..stats(129, 129)
    };
    window
        .observe(
            Some(RenderWorkTiming {
                gpu_recorded: false,
                ..host(130)
            }),
            true,
            &[],
            Some(skipped),
            Some(capabilities()),
        )
        .unwrap();
    assert!(window.evidence().is_none());
    for id in 131..=250 {
        observe(
            &mut window,
            id,
            &[gpu(id)],
            GpuProfileStats {
                skipped_ring_full: 1,
                ..stats(id - 1, id - 1)
            },
        );
    }
    let evidence = window.evidence().unwrap();
    assert_eq!(
        (
            evidence.first_renderer_frame_id,
            evidence.last_renderer_frame_id
        ),
        (131, 250)
    );
    assert_eq!(evidence.gpu_skipped_ring_full, 1);
    assert_eq!(evidence.skipped_ring_full_in_window, 0);
    assert!(
        evidence
            .samples
            .iter()
            .all(|sample| sample.renderer_frame_id != 130)
    );
}

#[test]
fn incomplete_hardware_coverage_and_caps_cannot_emit_an_old_ready_receipt() {
    for (counts, caps) in [
        (None, Some(capabilities())),
        (Some(stats(130, 130)), None),
        (
            Some(GpuProfileStats {
                pending_readbacks: 1,
                ..stats(130, 130)
            }),
            Some(capabilities()),
        ),
        (Some(stats(130, 125)), Some(capabilities())),
        (
            Some(stats(130, 130)),
            Some(RenderWorkCapabilities {
                timestamp_period_ns: 0.0,
                ..capabilities()
            }),
        ),
        (
            Some(stats(130, 130)),
            Some(RenderWorkCapabilities {
                timestamp_period_ns: f64::NAN,
                ..capabilities()
            }),
        ),
        (
            Some(stats(130, 130)),
            Some(RenderWorkCapabilities {
                timestamp_query: false,
                ..capabilities()
            }),
        ),
        (
            Some(stats(130, 130)),
            Some(RenderWorkCapabilities {
                timestamp_query_inside_encoders: false,
                ..capabilities()
            }),
        ),
    ] {
        let mut window = complete_window();
        assert!(matches!(
            window.observe(Some(host(130)), true, &[gpu(130)], counts, caps),
            Err(RenderWorkError::Coverage)
        ));
        assert!(window.evidence().is_none());
    }
}
