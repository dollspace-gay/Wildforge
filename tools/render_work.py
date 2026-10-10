"""Validate versioned, completed CPU/GPU work independently of presentation waits."""

import math
import statistics

WINDOW_FRAMES = 120
READBACK_SLOTS = 4


def validate_capture_render_work(telemetry: dict) -> tuple[float, float]:
    """Anchor the completed window to its native capture and fixed warmup."""
    if not isinstance(telemetry, dict):
        raise ValueError("invalid render-work evidence: missing native telemetry")
    work = telemetry.get("render_work")
    result = validate_render_work(work)
    frame, eligible, settled = (telemetry.get(name) for name in ("frame", "eligible_frame", "settled_frames"))
    if (type(frame) is not int or type(eligible) is not int
            or not 1200 <= eligible < frame
            or work["latest_renderer_frame_id"] + 1 != frame
            or telemetry.get("settled") is not True
            or type(settled) is not int
            or settled < WINDOW_FRAMES + 10 - 1 + work["latest_renderer_frame_id"] - work["last_renderer_frame_id"]):
        raise ValueError("invalid render-work evidence: stale window or insufficient warmup")
    return result


def rendering_regression(cpu_delta: float, gpu_delta: float) -> float:
    """Keep CPU improvements from concealing additional GPU cost, and vice versa."""
    return max(0.0, cpu_delta) + max(0.0, gpu_delta)


def validate_render_work(value: dict) -> tuple[float, float]:
    """Recompute a native window; refuse missing, stale, or inconsistent coverage."""
    def fail(reason):
        raise ValueError(f"invalid render-work evidence: {reason}")

    if (not isinstance(value, dict) or type(value.get("schema_version")) is not int
            or value["schema_version"] != 1):
        fail("missing or unsupported schema")

    def integer(name, minimum=0):
        result = value.get(name)
        if type(result) is not int or result < minimum:
            fail(name)
        return result

    first = integer("first_renderer_frame_id", 1)
    last = integer("last_renderer_frame_id", 1)
    latest = integer("latest_renderer_frame_id", 1)
    submitted = integer("gpu_submitted", WINDOW_FRAMES)
    completed = integer("gpu_completed", WINDOW_FRAMES)
    pending = integer("gpu_pending_readbacks")
    skipped = integer("gpu_skipped_ring_full")
    if (integer("sample_count") != WINDOW_FRAMES
            or integer("skipped_ring_full_in_window") != 0
            or last - first != WINDOW_FRAMES - 1
            or not 0 <= latest - last <= READBACK_SLOTS
            or not 0 <= pending <= READBACK_SLOTS
            or completed + pending != submitted
            or submitted + skipped > latest):
        fail("incomplete or stale window")
    period = value.get("timestamp_period_ns")
    if (type(period) not in (int, float) or not math.isfinite(period) or period <= 0
            or value.get("timestamp_query") is not True
            or value.get("timestamp_query_inside_encoders") is not True):
        fail("no hardware timestamp capability")
    samples = value.get("samples")
    if not isinstance(samples, list) or len(samples) != WINDOW_FRAMES:
        fail("sample coverage")
    fields = ("cpu_submission_ms", "gpu_ms", "acquisition_ms", "present_ms", "wall_ms")
    columns = {name: [] for name in fields}
    for index, sample in enumerate(samples):
        if (not isinstance(sample, dict)
                or type(sample.get("renderer_frame_id")) is not int
                or sample["renderer_frame_id"] != first + index
                or sample.get("settled") is not True
                or sample.get("capture_frame") is not False):
            fail("unjoined, duplicate, unsettled, or capture frame")
        for name in fields:
            number = sample.get(name)
            if type(number) not in (int, float) or not math.isfinite(number) or number < 0:
                fail(name)
            columns[name].append(number)
        if abs(sample["wall_ms"] - sample["cpu_submission_ms"]
               - sample["acquisition_ms"] - sample["present_ms"]) > 0.000002:
            fail("CPU work does not reconcile with presentation and render wall")
    medians = {name: statistics.median(numbers) for name, numbers in columns.items()}
    for name, recomputed in medians.items():
        reported = value.get("median_" + name)
        if (type(reported) not in (int, float) or not math.isfinite(reported)
                or abs(reported - recomputed) > 0.000002):
            fail("median_" + name)
    return medians["cpu_submission_ms"], medians["gpu_ms"]
