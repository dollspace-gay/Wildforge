//! Recompute the versioned paired geode budget from native capture samples.

use std::collections::BTreeMap;

use super::GeodePerformanceReport;
use super::render_work::{close, median};

fn rendering_regression(cpu_delta: f64, gpu_delta: f64) -> f64 {
    cpu_delta.max(0.0) + gpu_delta.max(0.0)
}

pub(super) fn validate_report(
    report: &GeodePerformanceReport,
    kind: &str,
    commit: &str,
    samples: &BTreeMap<&str, Vec<f64>>,
) -> Result<(), String> {
    let invalid = || "cracked-geode performance qualification is incomplete or failed".to_owned();
    let pairs = [
        (&report.sealed_draw_ms, "sealed_draw"),
        (&report.opened_draw_ms, "opened_draw"),
        (&report.sealed_simulation_ms, "sealed_simulation"),
        (&report.opened_simulation_ms, "opened_simulation"),
        (&report.sealed_cpu_submission_ms, "sealed_cpu_submission"),
        (&report.opened_cpu_submission_ms, "opened_cpu_submission"),
        (&report.sealed_gpu_ms, "sealed_gpu"),
        (&report.opened_gpu_ms, "opened_gpu"),
    ];
    if report.qualification_schema_version != 2
        || report.performance_measurement != "render-work-v1"
        || !report.legacy_draw_is_pacing_diagnostic
        || report.kind != kind
        || report.evidence_commit != commit
        || report.sample_count_per_phase != 5
    {
        return Err(invalid());
    }
    for (reported, key) in pairs {
        let expected = samples.get(key).ok_or_else(invalid)?;
        if reported.len() != 5
            || expected.len() != 5
            || !reported.iter().zip(expected).all(|(a, b)| close(*a, *b))
        {
            return Err(invalid());
        }
    }
    let sealed_draw = median(&samples["sealed_draw"]);
    let opened_draw = median(&samples["opened_draw"]);
    let sealed_simulation = median(&samples["sealed_simulation"]);
    let opened_simulation = median(&samples["opened_simulation"]);
    let sealed_cpu = median(&samples["sealed_cpu_submission"]);
    let opened_cpu = median(&samples["opened_cpu_submission"]);
    let sealed_gpu = median(&samples["sealed_gpu"]);
    let opened_gpu = median(&samples["opened_gpu"]);
    let cpu_delta = opened_cpu - sealed_cpu;
    let gpu_delta = opened_gpu - sealed_gpu;
    let work_delta = rendering_regression(cpu_delta, gpu_delta);
    let derived = [
        (report.sealed_median_draw_ms, sealed_draw),
        (report.opened_median_draw_ms, opened_draw),
        (report.median_draw_regression_ms, opened_draw - sealed_draw),
        (report.sealed_median_simulation_ms, sealed_simulation),
        (report.opened_median_simulation_ms, opened_simulation),
        (
            report.median_simulation_regression_ms,
            opened_simulation - sealed_simulation,
        ),
        (report.sealed_median_cpu_submission_ms, sealed_cpu),
        (report.opened_median_cpu_submission_ms, opened_cpu),
        (report.median_cpu_submission_regression_ms, cpu_delta),
        (report.sealed_median_gpu_ms, sealed_gpu),
        (report.opened_median_gpu_ms, opened_gpu),
        (report.median_gpu_regression_ms, gpu_delta),
        (report.rendering_regression_ms, work_delta),
    ];
    if !derived.into_iter().all(|(a, b)| close(a, b))
        || report.maximum_rendering_regression_ms != 0.20
        || work_delta > report.maximum_rendering_regression_ms
        || report.maximum_median_simulation_regression_ms != 0.10
        || opened_simulation - sealed_simulation > report.maximum_median_simulation_regression_ms
        || !report.passed
    {
        return Err(invalid());
    }
    Ok(())
}

#[test]
fn improvements_cannot_conceal_growth_in_the_other_rendering_component() {
    assert!(rendering_regression(-0.3, 0.21) > 0.20);
    assert!(rendering_regression(0.11, 0.10) > 0.20);
    assert!(rendering_regression(0.01, 0.18) <= 0.20);
}
