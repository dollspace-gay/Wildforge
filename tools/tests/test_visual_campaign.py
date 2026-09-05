"""Contracts needed before running an expensive native qualification campaign."""

import copy
from pathlib import Path
import sys
import tempfile
import tomllib
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from visual_campaign.capture import configure, environment, validate_sidecar
from visual_campaign.manifest import render
from visual_campaign.matrix import plan
from visual_campaign.provenance import SOURCE_DIRECTORIES, source_sha256


class CampaignTests(unittest.TestCase):
    root = Path(__file__).resolve().parents[2]

    def test_full_matrix_preserves_axes_and_pairs_baseline_camera(self):
        manifest, rows = plan(self.root, "2026-09-05")
        self.assertEqual(tomllib.loads(render(manifest)), manifest)
        self.assertEqual(len(rows), 92)
        by_id = {row["id"]: row for row in rows}
        for declaration in manifest["case"]:
            if declaration["phase"] == "baseline":
                baseline = by_id[declaration["id"]]
                candidate = by_id[declaration["id"].replace("strata-baseline-", "strata-after-")]
                self.assertEqual(baseline["template"]["camera"], candidate["template"]["camera"])
                self.assertEqual(baseline["template"]["render"], candidate["template"]["render"])
        self.assertEqual(len([row for row in rows if "performance" in row["id"]]), 40)

    def test_capture_inputs_use_game_configuration_values(self):
        _, rows = plan(self.root, "2026-09-05")
        row = copy.deepcopy(rows[0])
        row["template"]["render"].update(lights=0, point_grid=False, stark=False, bloom=False)
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary)
            configure(path, row["template"]["render"])
            config = (path / "config.txt").read_text()
            for line in ("lights=off", "point_shadows=cube", "darkness=soft", "bloom=off"):
                self.assertIn(line, config)
        env = environment(row, "screenshots/test.ppm")
        self.assertEqual(env["WILDFORGE_SHOT_ALTITUDE"], "0")
        self.assertEqual(env["WILDFORGE_CAPTURE_SCENE"], row["scene"])

    def test_sidecar_rejects_wrong_build_software_gpu_and_unsettled_frames(self):
        _, rows = plan(self.root, "2026-09-05")
        row = rows[0]
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "capture.toml"
            for change in ({"build": {"commit": "0" * 40, "dirty": False}},
                           {"render": {"hardware": False, "backend": "Vulkan", "adapter": "CPU"}},
                           {"telemetry": {"settled": False, "settled_frames": 0, "dirty_chunks": 1}}):
                metadata = copy.deepcopy(row["template"])
                metadata["capture_id"] = row["id"]
                metadata["scene_id"] = row["scene"]
                metadata.update(change)
                path.write_text(render(metadata))
                with self.assertRaises(ValueError):
                    validate_sidecar(path, row, row["template"]["build"]["commit"])

    def test_fingerprint_detects_new_modules_and_ignores_guides(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for directory in SOURCE_DIRECTORIES:
                (root / directory).mkdir(parents=True)
            for name in ("Cargo.toml", "Cargo.lock", "build.rs"):
                (root / name).write_text(name)
            before = source_sha256(root)
            (root / "src/README.md").write_text("guide")
            self.assertEqual(source_sha256(root), before)
            (root / "src/new.rs").write_text("new source")
            self.assertNotEqual(source_sha256(root), before)


if __name__ == "__main__":
    unittest.main()
