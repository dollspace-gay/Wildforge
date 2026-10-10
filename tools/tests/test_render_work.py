"""Reject evidence that cannot establish completed rendering work."""

import copy
from pathlib import Path
import sys
import tomllib
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from render_work import validate_capture_render_work, validate_render_work, rendering_regression
import verify_cracked_geode as geode


def receipt():
    samples = [dict(renderer_frame_id=1000 + index, cpu_submission_ms=0.4,
                    gpu_ms=0.7, acquisition_ms=14.0, present_ms=0.1,
                    wall_ms=14.5, settled=True, capture_frame=False)
               for index in range(120)]
    return dict(schema_version=1, sample_count=120, first_renderer_frame_id=1000,
                last_renderer_frame_id=1119, latest_renderer_frame_id=1121,
                samples=samples, median_cpu_submission_ms=0.4, median_gpu_ms=0.7,
                median_acquisition_ms=14.0, median_present_ms=0.1, median_wall_ms=14.5,
                timestamp_period_ns=1.0, timestamp_query=True,
                timestamp_query_inside_encoders=True, gpu_submitted=1121,
                gpu_completed=1119, gpu_skipped_ring_full=0, gpu_pending_readbacks=2,
                skipped_ring_full_in_window=0)


class RenderWorkTests(unittest.TestCase):
    def test_geode_qualification_requires_native_work_not_a_low_wall_timer(self):
        sidecar = dict(scene_id=geode.PERFORMANCE_SCENE_ID,
                       build=dict(commit="a" * 40, dirty=False),
                       render=dict(hardware=True, backend="Vulkan", adapter="DiscreteGpu"),
                       camera={}, environment={},
                       telemetry=dict(settled=True, draw_ms=0.01, simulation_ms=0.01))
        with patch.object(geode, "require_capture", return_value=(None, b"", sidecar, None, b"", {})):
            with self.assertRaisesRegex(geode.GeodeEvidenceError, "render-work"):
                geode.performance_report()

    def test_complete_window_excludes_pacing_without_hiding_work(self):
        value = receipt()
        self.assertEqual(validate_render_work(value), (0.4, 0.7))
        # A CPU improvement must not cancel a GPU regression.
        self.assertAlmostEqual(rendering_regression(-0.3, 0.21), 0.21)
        self.assertAlmostEqual(rendering_regression(0.11, 0.10), 0.21)

    def test_capture_anchors_the_window_and_checks_fixed_warmup(self):
        value = receipt()
        for sample in value["samples"]:
            sample["renderer_frame_id"] += 1000
        for field in ("first_renderer_frame_id", "last_renderer_frame_id",
                      "latest_renderer_frame_id", "gpu_submitted", "gpu_completed"):
            value[field] += 1000
        telemetry = dict(frame=2122, eligible_frame=1200, settled=True, settled_frames=131, render_work=value)
        self.assertEqual(validate_capture_render_work(telemetry), (0.4, 0.7))
        for change in (dict(frame=2123), dict(eligible_frame=1199), dict(eligible_frame=2122),
                       dict(settled_frames=130), dict(settled=False)):
            with self.subTest(change=change), self.assertRaises(ValueError):
                validate_capture_render_work({**telemetry, **change})

    def test_geode_report_rejects_mismatched_camera_settings_and_environment(self):
        root = Path(__file__).resolve().parents[2]
        reference = tomllib.loads((root / "screenshots/geode-sealed-context.capture.toml").read_text())
        performance = tomllib.loads((root / "screenshots/geode-performance-sealed-a.capture.toml").read_text())
        work = receipt()
        for sample in work["samples"]:
            sample["renderer_frame_id"] += 1000
        for field in ("first_renderer_frame_id", "last_renderer_frame_id",
                      "latest_renderer_frame_id", "gpu_submitted", "gpu_completed"):
            work[field] += 1000
        performance["scene_id"] = geode.PERFORMANCE_SCENE_ID
        performance["telemetry"].update(frame=2122, eligible_frame=1200, settled_frames=131, render_work=work)

        def captures(change=None):
            def load(stem):
                value = copy.deepcopy(reference if stem.endswith("-sealed-context") else performance)
                if not stem.endswith("-sealed-context"):
                    value["world"]["name"] = "visual-polish-geode-" + ("opened" if "opened" in stem else "sealed")
                    if change and stem.endswith("opened-b"):
                        change(value)
                return None, b"", value, None, b"", {}
            return load

        with patch.object(geode, "require_capture", side_effect=captures()):
            self.assertTrue(geode.performance_report()["passed"])
        changes = [lambda v: v["camera"].update(yaw=0.0),
                   lambda v: v["render"].update(lights=0),
                   lambda v: v["environment"].update(wind=[0.0, 0.0])]
        for change in changes:
            with self.subTest(change=change), patch.object(geode, "require_capture", side_effect=captures(change)):
                with self.assertRaises(geode.GeodeEvidenceError):
                    geode.performance_report()

    def test_missing_unjoined_or_selected_old_frames_are_rejected(self):
        changes = [lambda v: v["samples"].pop(),
                   lambda v: v["samples"][60].update(renderer_frame_id=1059),
                   lambda v: v.update(latest_renderer_frame_id=1124),
                   lambda v: v.update(gpu_completed=119)]
        for change in changes:
            with self.subTest(change=change):
                value = receipt()
                change(value)
                with self.assertRaises(ValueError):
                    validate_render_work(value)

    def test_incomplete_gpu_capture_unsettled_and_forged_medians_are_rejected(self):
        changes = [lambda v: v.update(timestamp_query=False),
                   lambda v: v.update(timestamp_query_inside_encoders=False),
                   lambda v: v.update(timestamp_period_ns=0.0),
                   lambda v: v.update(skipped_ring_full_in_window=1),
                   lambda v: v.update(gpu_skipped_ring_full=1),
                   lambda v: v["samples"][0].update(capture_frame=True),
                   lambda v: v["samples"][0].update(settled=False),
                   lambda v: v["samples"][0].update(cpu_submission_ms=0.0),
                   lambda v: v.update(median_gpu_ms=0.01),
                   lambda v: v["samples"][0].update(gpu_ms=float("nan"))]
        for change in changes:
            with self.subTest(change=change):
                value = copy.deepcopy(receipt())
                change(value)
                with self.assertRaises(ValueError):
                    validate_render_work(value)


if __name__ == "__main__":
    unittest.main()
