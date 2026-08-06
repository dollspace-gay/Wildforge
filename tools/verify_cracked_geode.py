#!/usr/bin/env python3
"""Qualify the cracked-geode GPU evidence by material identity and depth.

Raw PPM/WFD files remain untracked beside the native Windows executable.  This
tool consumes them with the shared visual-evidence parser and emits only two
small deterministic TOML acceptance reports for review and Rust-side CI gates.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
import statistics
import sys
import tomllib
from pathlib import Path
from typing import Any


QUALIFICATION_SCHEMA_VERSION = 1
COMPOSITION_KIND = "cracked-geode-composition"
PERFORMANCE_KIND = "cracked-geode-performance"
SCENE_ID = "cracked-geode-20260805"
PERFORMANCE_SCENE_ID = "cracked-geode-performance-20260805"
MIN_HOST_FRACTION = 0.15
MIN_LINING_PIXELS = 16 * 16
MIN_HEART_PIXELS = 16 * 16
MAX_OVERLAY_FRACTION = 0.04
MAX_DRAW_REGRESSION_MS = 0.20
MAX_SIMULATION_REGRESSION_MS = 0.10


class GeodeEvidenceError(RuntimeError):
    pass


def load_shared_tool() -> Any:
    path = Path(__file__).with_name("verify_visual_polish.py")
    spec = importlib.util.spec_from_file_location("wildforge_visual_evidence", path)
    if spec is None or spec.loader is None:
        raise GeodeEvidenceError(f"cannot load shared evidence parser {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


SHARED = load_shared_tool()


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_toml(path: Path) -> tuple[bytes, dict[str, Any]]:
    try:
        data = path.read_bytes()
        return data, tomllib.loads(data.decode("utf-8"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        raise GeodeEvidenceError(f"read {path}: {error}") from error


def quoted(value: str) -> str:
    return json.dumps(value, ensure_ascii=True)


def toml_value(value: Any) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        if not math.isfinite(value):
            raise GeodeEvidenceError("qualification reports cannot contain non-finite values")
        return format(value, ".9f")
    if isinstance(value, str):
        return quoted(value)
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(item) for item in value) + "]"
    raise GeodeEvidenceError(f"unsupported TOML value {type(value).__name__}")


def render(report: dict[str, Any]) -> bytes:
    return ("\n".join(f"{key} = {toml_value(value)}" for key, value in report.items()) + "\n").encode()


def write_atomic(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_bytes(data)
    temporary.replace(path)


def require_capture(stem: str) -> tuple[Path, bytes, dict[str, Any], Path, bytes, dict[str, Any]]:
    sidecar_path = Path("screenshots") / f"{stem}.capture.toml"
    report_path = Path("screenshots") / f"{stem}.report.toml"
    sidecar_bytes, sidecar = read_toml(sidecar_path)
    report_bytes, report = read_toml(report_path)
    if sidecar.get("capture_id") != stem or report.get("capture_id") != stem:
        raise GeodeEvidenceError(f"{stem} identity does not match its filename")
    if report.get("sidecar_sha256") != sha256(sidecar_bytes):
        raise GeodeEvidenceError(f"{stem} report is stale relative to its sidecar")
    if report.get("diagnostic_sha256") != sidecar.get("artifacts", {}).get("diagnostic_sha256"):
        raise GeodeEvidenceError(f"{stem} report and sidecar disagree on diagnostic identity")
    return sidecar_path, sidecar_bytes, sidecar, report_path, report_bytes, report


def family_ids(sidecar: dict[str, Any], material: str) -> set[int]:
    result = {
        int(entry["diagnostic_id"])
        for entry in sidecar.get("family", [])
        if material in entry.get("names", [])
    }
    if not result:
        raise GeodeEvidenceError(f"capture family map does not contain {material}")
    return result


def raw_capture(sidecar_path: Path, sidecar: dict[str, Any]) -> tuple[int, int, bytes, Any]:
    artifacts = sidecar.get("artifacts", {})
    try:
        color_path = SHARED.resolve_artifact(sidecar_path, artifacts["color_ppm"], "color PPM")
        diagnostic_path = SHARED.resolve_artifact(sidecar_path, artifacts["diagnostic"], "diagnostic")
        color_bytes = color_path.read_bytes()
        diagnostic_bytes = diagnostic_path.read_bytes()
    except (KeyError, OSError, SHARED.EvidenceError) as error:
        raise GeodeEvidenceError(f"resolve raw capture {sidecar_path}: {error}") from error
    if sha256(color_bytes) != artifacts.get("color_ppm_sha256"):
        raise GeodeEvidenceError(f"{sidecar_path} color hash mismatch")
    if sha256(diagnostic_bytes) != artifacts.get("diagnostic_sha256"):
        raise GeodeEvidenceError(f"{sidecar_path} diagnostic hash mismatch")
    width, height, color = SHARED.read_ppm(color_bytes)
    diag_width, diag_height, diagnostic = SHARED.read_wfd(diagnostic_bytes)
    if (width, height) != (diag_width, diag_height):
        raise GeodeEvidenceError(f"{sidecar_path} color/diagnostic dimensions differ")
    return width, height, color, diagnostic


def count_ids(diagnostic: Any, ids: set[int]) -> int:
    return sum(diagnostic[offset] in ids for offset in range(0, len(diagnostic), 4))


def bounds(diagnostic: Any, width: int, ids: set[int]) -> tuple[int, int, int, int]:
    points = [
        (pixel % width, pixel // width)
        for pixel in range(len(diagnostic) // 4)
        if diagnostic[pixel * 4] in ids
    ]
    if not points:
        raise GeodeEvidenceError("required material has no visible pixels")
    return (
        min(point[0] for point in points),
        min(point[1] for point in points),
        max(point[0] for point in points),
        max(point[1] for point in points),
    )


def display_luminance(color: bytes, pixel: int) -> float:
    red, green, blue = color[pixel * 3 : pixel * 3 + 3]
    return (0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255.0


def composition_report(
    stem_prefix: str = "geode",
    scene_id: str = SCENE_ID,
    kind: str = COMPOSITION_KIND,
) -> dict[str, Any]:
    hero = require_capture(f"{stem_prefix}-cracked-hero")
    proof = require_capture(f"{stem_prefix}-aperture-proof")
    sealed = require_capture(f"{stem_prefix}-sealed-context")
    reloaded = require_capture(f"{stem_prefix}-reload-proof")
    hero_sidecar_path, _hero_sidecar_bytes, hero_sidecar, hero_report_path, hero_report_bytes, hero_report = hero
    proof_sidecar_path, _proof_sidecar_bytes, proof_sidecar, proof_report_path, proof_report_bytes, proof_report = proof

    build = hero_sidecar.get("build", {})
    if build.get("dirty") or len(str(build.get("commit", ""))) != 40:
        raise GeodeEvidenceError("hero capture does not name a clean evidence commit")
    for item in (proof, sealed, reloaded):
        sidecar = item[2]
        if sidecar.get("build") != build or sidecar.get("scene_id") != scene_id:
            raise GeodeEvidenceError("the four geode captures do not share one clean build and scene")

    sealed_camera = sealed[2].get("camera")
    if reloaded[2].get("camera") != sealed_camera:
        raise GeodeEvidenceError("sealed and reload proof cameras are not identical")
    for key in ("environment", "render"):
        if sealed[2].get(key) != reloaded[2].get(key):
            raise GeodeEvidenceError(f"sealed and reload proof {key} identities differ")

    width, height, color, diagnostic = raw_capture(hero_sidecar_path, hero_sidecar)
    host_ids = family_ids(hero_sidecar, "base:limestone") | family_ids(hero_sidecar, "base:marble")
    quartz_ids = family_ids(hero_sidecar, "base:quartz_block")
    amethyst_ids = family_ids(hero_sidecar, "base:amethyst_block")
    host_pixels = count_ids(diagnostic, host_ids)
    quartz_pixels = count_ids(diagnostic, quartz_ids)
    amethyst_pixels = count_ids(diagnostic, amethyst_ids)
    total_pixels = width * height
    host_fraction = host_pixels / total_pixels

    amethyst_bounds = bounds(diagnostic, width, amethyst_ids)
    ax0, ay0, ax1, ay1 = amethyst_bounds
    lip_counts = {"left": 0, "right": 0, "top": 0, "bottom": 0}
    heart_pixels = 0
    for pixel in range(total_pixels):
        family = diagnostic[pixel * 4]
        depth = diagnostic[pixel * 4 + 1] / 65535.0
        x, y = pixel % width, pixel // width
        if family in quartz_ids:
            if x < ax0 and ay0 <= y <= ay1:
                lip_counts["left"] += 1
            if x > ax1 and ay0 <= y <= ay1:
                lip_counts["right"] += 1
            if y < ay0 and ax0 <= x <= ax1:
                lip_counts["top"] += 1
            if y > ay1 and ax0 <= x <= ax1:
                lip_counts["bottom"] += 1
        # The heart is air, so the diagnostic records the far quartz/amethyst
        # wall seen through it. Requiring both geode identity and greater view
        # depth proves these are recessed cavity pixels, not a black texture.
        if (
            family in quartz_ids | amethyst_ids
            and ax0 <= x <= ax1
            and ay0 <= y <= ay1
            and depth >= 0.038
            and display_luminance(color, pixel) <= 0.05
        ):
            heart_pixels += 1
    lip_sides = [side for side in ("left", "right", "top", "bottom") if lip_counts[side] >= MIN_LINING_PIXELS]

    proof_width, proof_height, _proof_color, proof_diagnostic = raw_capture(proof_sidecar_path, proof_sidecar)
    proof_host_ids = family_ids(proof_sidecar, "base:limestone") | family_ids(proof_sidecar, "base:marble")
    proof_quartz_ids = family_ids(proof_sidecar, "base:quartz_block")
    proof_amethyst_ids = family_ids(proof_sidecar, "base:amethyst_block")
    proof_host_fraction = count_ids(proof_diagnostic, proof_host_ids) / (proof_width * proof_height)
    proof_quartz_pixels = count_ids(proof_diagnostic, proof_quartz_ids)
    proof_amethyst_pixels = count_ids(proof_diagnostic, proof_amethyst_ids)

    hero_overlay_fraction = int(hero_report["overlay_pixels"]) / total_pixels
    passed = (
        (width, height) == (1920, 1080)
        and hero_report.get("sky_pixels") == 0
        and hero_overlay_fraction <= MAX_OVERLAY_FRACTION
        and host_fraction >= MIN_HOST_FRACTION
        and quartz_pixels >= MIN_LINING_PIXELS
        and amethyst_pixels >= MIN_LINING_PIXELS
        and heart_pixels >= MIN_HEART_PIXELS
        and len(lip_sides) >= 3
        and proof_host_fraction >= MIN_HOST_FRACTION
        and proof_quartz_pixels >= MIN_LINING_PIXELS
        and proof_amethyst_pixels >= MIN_LINING_PIXELS
    )
    return {
        "qualification_schema_version": QUALIFICATION_SCHEMA_VERSION,
        "kind": kind,
        "evidence_commit": str(build["commit"]),
        "hero_capture_id": f"{stem_prefix}-cracked-hero",
        "hero_report": (Path("screenshots/visual-polish") / hero_report_path.name).as_posix(),
        "hero_report_sha256": sha256(hero_report_bytes),
        "proof_capture_id": f"{stem_prefix}-aperture-proof",
        "proof_report": (Path("screenshots/visual-polish") / proof_report_path.name).as_posix(),
        "proof_report_sha256": sha256(proof_report_bytes),
        "width": width,
        "height": height,
        "host_pixels": host_pixels,
        "host_fraction": host_fraction,
        "minimum_host_fraction": MIN_HOST_FRACTION,
        "quartz_pixels": quartz_pixels,
        "amethyst_pixels": amethyst_pixels,
        "minimum_lining_pixels": MIN_LINING_PIXELS,
        "heart_dark_deep_pixels": heart_pixels,
        "minimum_heart_dark_deep_pixels": MIN_HEART_PIXELS,
        "lip_left_pixels": lip_counts["left"],
        "lip_right_pixels": lip_counts["right"],
        "lip_top_pixels": lip_counts["top"],
        "lip_bottom_pixels": lip_counts["bottom"],
        "lip_sides": lip_sides,
        "minimum_lip_sides": 3,
        "sky_pixels": int(hero_report["sky_pixels"]),
        "overlay_fraction": hero_overlay_fraction,
        "maximum_overlay_fraction": MAX_OVERLAY_FRACTION,
        "proof_host_fraction": proof_host_fraction,
        "proof_quartz_pixels": proof_quartz_pixels,
        "proof_amethyst_pixels": proof_amethyst_pixels,
        "sealed_reload_camera_match": True,
        "sealed_reload_environment_match": True,
        "sealed_reload_render_match": True,
        "passed": passed,
    }


def performance_report(
    stem_prefix: str = "geode-performance",
    scene_id: str = PERFORMANCE_SCENE_ID,
    kind: str = PERFORMANCE_KIND,
) -> dict[str, Any]:
    samples: dict[str, list[float]] = {
        "sealed_draw": [],
        "opened_draw": [],
        "sealed_simulation": [],
        "opened_simulation": [],
    }
    commits: set[str] = set()
    for phase in ("sealed", "opened"):
        for run in "abcde":
            stem = f"{stem_prefix}-{phase}-{run}"
            _sidecar_path, _sidecar_bytes, sidecar, _report_path, _report_bytes, _report = require_capture(stem)
            if sidecar.get("scene_id") != scene_id:
                raise GeodeEvidenceError(f"{stem} uses the wrong performance scene")
            if sidecar.get("build", {}).get("dirty"):
                raise GeodeEvidenceError(f"{stem} came from a dirty build")
            if not sidecar.get("telemetry", {}).get("settled"):
                raise GeodeEvidenceError(f"{stem} was captured before the world settled")
            commits.add(str(sidecar["build"]["commit"]))
            samples[f"{phase}_draw"].append(float(sidecar["telemetry"]["draw_ms"]))
            samples[f"{phase}_simulation"].append(float(sidecar["telemetry"]["simulation_ms"]))
    if len(commits) != 1:
        raise GeodeEvidenceError("performance captures do not share one clean build")
    sealed_draw = statistics.median(samples["sealed_draw"])
    opened_draw = statistics.median(samples["opened_draw"])
    sealed_simulation = statistics.median(samples["sealed_simulation"])
    opened_simulation = statistics.median(samples["opened_simulation"])
    draw_delta = opened_draw - sealed_draw
    simulation_delta = opened_simulation - sealed_simulation
    passed = draw_delta <= MAX_DRAW_REGRESSION_MS and simulation_delta <= MAX_SIMULATION_REGRESSION_MS
    return {
        "qualification_schema_version": QUALIFICATION_SCHEMA_VERSION,
        "kind": kind,
        "evidence_commit": next(iter(commits)),
        "sample_count_per_phase": 5,
        "sealed_draw_ms": samples["sealed_draw"],
        "opened_draw_ms": samples["opened_draw"],
        "sealed_simulation_ms": samples["sealed_simulation"],
        "opened_simulation_ms": samples["opened_simulation"],
        "sealed_median_draw_ms": sealed_draw,
        "opened_median_draw_ms": opened_draw,
        "median_draw_regression_ms": draw_delta,
        "maximum_median_draw_regression_ms": MAX_DRAW_REGRESSION_MS,
        "sealed_median_simulation_ms": sealed_simulation,
        "opened_median_simulation_ms": opened_simulation,
        "median_simulation_regression_ms": simulation_delta,
        "maximum_median_simulation_regression_ms": MAX_SIMULATION_REGRESSION_MS,
        "passed": passed,
    }


def reports() -> tuple[dict[str, Any], dict[str, Any]]:
    return composition_report(), performance_report()


def qualify(args: argparse.Namespace) -> None:
    composition, performance = reports()
    write_atomic(Path(args.composition_report), render(composition))
    write_atomic(Path(args.performance_report), render(performance))
    if not composition["passed"] or not performance["passed"]:
        raise GeodeEvidenceError("cracked-geode qualification failed; reports were written")
    print(
        "cracked geode: composition and performance qualification passed; "
        f"wrote {args.composition_report} and {args.performance_report}"
    )


def check(args: argparse.Namespace) -> None:
    composition_bytes, composition = read_toml(Path(args.composition_report))
    performance_bytes, performance = read_toml(Path(args.performance_report))
    rebuilt_composition, rebuilt_performance = reports()
    if composition_bytes != render(rebuilt_composition) or performance_bytes != render(rebuilt_performance):
        raise GeodeEvidenceError("cracked-geode qualification reports are stale or nondeterministic")
    if not composition.get("passed") or not performance.get("passed"):
        raise GeodeEvidenceError("cracked-geode qualification report records a failed gate")
    print("cracked geode: qualification reports are deterministic and current")


def self_test(_args: argparse.Namespace) -> None:
    values = [5.0, 1.0, 3.0, 2.0, 4.0]
    fixture = {"passed": True, "median": statistics.median(values), "values": values}
    if render(fixture) != render(fixture) or statistics.median(values) != 3.0:
        raise GeodeEvidenceError("deterministic in-memory fixture failed")
    print("cracked geode: deterministic report fixture passed")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    commands = result.add_subparsers(dest="command", required=True)
    qualify_parser = commands.add_parser("qualify", help="write aggregate acceptance reports")
    qualify_parser.add_argument("--composition-report", required=True)
    qualify_parser.add_argument("--performance-report", required=True)
    qualify_parser.set_defaults(run=qualify)
    check_parser = commands.add_parser("check-qualification", help="reject stale acceptance reports")
    check_parser.add_argument("--composition-report", required=True)
    check_parser.add_argument("--performance-report", required=True)
    check_parser.set_defaults(run=check)
    self_test_parser = commands.add_parser("self-test", help="run deterministic in-memory fixtures")
    self_test_parser.set_defaults(run=self_test)
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        args.run(args)
    except (GeodeEvidenceError, SHARED.EvidenceError) as error:
        print(f"cracked geode: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
