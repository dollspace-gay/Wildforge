#!/usr/bin/env python3
"""Cross-scene closeout qualification for the visual polish arc (Goal 4).

Goals 2 and 3 each qualified on their own commit. This tool re-verifies both
plans' matrices captured again from ONE clean closeout commit on the same
native hardware, and analyzes the interactive motion walkthroughs that still
frames cannot judge. It reuses the shared evidence parser and the cracked-geode
composition machinery so the closeout cannot drift from the original gates.

Raw PPM/WFD frames stay untracked; only small deterministic TOML reports are
written for review and the Rust-side CI validator.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from pathlib import Path
from typing import Any

QUALIFICATION_SCHEMA_VERSION = 1
READABILITY_KIND = "closeout-readability"
STRATA_PERFORMANCE_KIND = "closeout-strata-performance"
GEODE_COMPOSITION_KIND = "closeout-geode-composition"
GEODE_PERFORMANCE_KIND = "closeout-geode-performance"
MOTION_KIND = "closeout-motion"
GEODE_SCENE_ID = "closeout-geode-20260806"
GEODE_PERFORMANCE_SCENE_ID = "closeout-geode-performance-20260806"
STRATA_BASELINE_COMMIT = "60486636fcfacd36de75e970a79ff6f806a0e9ac"
# Static holds still contain cosmetic animation — water surfaces, torch
# flame flicker, fading item labels. Exposure pumping is a full-frame swing
# well beyond that; 0.05 tolerates the animation and still catches pumping.
MAX_STATIC_LUMINANCE_DELTA = 0.050
MAX_SIMULATION_REGRESSION_MS = 0.10
# The strata closeout compares against the goal-2 baseline executable, which
# predates every magic-arc simulation system (weather-hour slicing, dross,
# alchemy, workings). Goal 2's 0.10 ms budget covered its tile-only change;
# the closeout's simulation budget covers the qualified cost of the arcs
# merged since. Draw keeps the goal-2 budget because tiles are draw-side.
STRATA_CLOSEOUT_SIMULATION_BUDGET_MS = 1.00


class CloseoutEvidenceError(RuntimeError):
    pass


def load_module(name: str, filename: str) -> Any:
    path = Path(__file__).with_name(filename)
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise CloseoutEvidenceError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


SHARED = load_module("wildforge_visual_evidence", "verify_visual_polish.py")
GEODE = load_module("wildforge_geode_evidence", "verify_cracked_geode.py")


# The shared converter owns byte identity and deterministic scalar encoding.
sha256 = SHARED.sha256
write_atomic = SHARED.write_atomic


def read_toml(path: Path) -> tuple[bytes, dict[str, Any]]:
    return SHARED.read_toml(path, error_type=CloseoutEvidenceError)


def toml_value(value: Any) -> str:
    return SHARED.toml_value(
        value, error_type=CloseoutEvidenceError,
        nonfinite_message="closeout reports cannot contain non-finite values",
    )


def render(report: dict[str, Any]) -> bytes:
    lines: list[str] = []
    tables: list[tuple[str, list[dict[str, Any]]]] = []
    for key, value in report.items():
        if isinstance(value, list) and value and isinstance(value[0], dict):
            tables.append((key, value))
        else:
            lines.append(f"{key} = {toml_value(value)}")
    for key, rows in tables:
        for row in rows:
            lines.append("")
            lines.append(f"[[{key}]]")
            lines.extend(f"{k} = {toml_value(v)}" for k, v in row.items())
    return ("\n".join(lines) + "\n").encode()


def sidecar_commit(report: dict[str, Any], commits: set[str]) -> None:
    sidecar_path = SHARED.safe_repo_path(report["sidecar"], "closeout sidecar")
    _bytes, sidecar = read_toml(sidecar_path)
    build = sidecar.get("build", {})
    if build.get("dirty") or len(str(build.get("commit", ""))) != 40:
        raise CloseoutEvidenceError(f"{sidecar_path} does not name a clean commit")
    if not sidecar.get("telemetry", {}).get("settled"):
        raise CloseoutEvidenceError(f"{sidecar_path} was captured before the world settled")
    commits.add(str(build["commit"]))


def single_commit(commits: set[str], label: str) -> str:
    if len(commits) != 1:
        raise CloseoutEvidenceError(f"{label} evidence does not share one clean commit: {sorted(commits)}")
    return next(iter(commits))


def build_closeout_readability(baseline_commit: str = STRATA_BASELINE_COMMIT) -> dict[str, Any]:
    cases = {
        "sandstone": ("sandstone-v4-near-noon-base", "sandstone-v12-prefog-overcast-gemini"),
        "limestone": ("limestone-v4-near-dawn-gemini", "limestone-v12-prefog-overcast-dusk"),
        "marble": ("marble-v4-near-rain-dusk", "marble-v4-near-rain-dusk"),
        "quartzite": ("quartzite-v4-near-overcast-base", "quartzite-v4-near-overcast-base"),
    }
    commits: set[str] = set()
    retention = []
    representatives: dict[tuple[str, str], dict[str, Any]] = {}
    source_reports: set[str] = set()
    for rock, (near_case, far_case) in cases.items():
        near_path, near_report = SHARED.named_report("closeout", near_case)
        far_path, far_report = SHARED.named_report("closeout", far_case)
        sidecar_commit(near_report, commits)
        sidecar_commit(far_report, commits)
        source_reports.update((near_path.as_posix(), far_path.as_posix()))
        near = SHARED.named_stratum(near_report, rock, "near")
        pre = SHARED.named_stratum(far_report, rock, "pre-fog")
        ratio4 = float(pre["rms_contrast_4px"]) / max(float(near["rms_contrast_4px"]), 1.0e-12)
        ratio16 = float(pre["rms_contrast_16px"]) / max(float(near["rms_contrast_16px"]), 1.0e-12)
        retention.append(
            {
                "rock": rock,
                "near_report": near_path.as_posix(),
                "pre_fog_report": far_path.as_posix(),
                "retention_4px": ratio4,
                "minimum_retention_4px": 0.70,
                "retention_16px": ratio16,
                "minimum_retention_16px": 0.50,
                "passed": ratio4 >= 0.70 and ratio16 >= 0.50,
            }
        )
        representatives[(rock, "near")] = near
        middle = next(
            (
                row
                for row in near_report.get("stratum", [])
                if row.get("rock") == rock and row.get("distance_band") == "middle"
            ),
            None,
        )
        if middle is None:
            raise CloseoutEvidenceError(f"{near_report.get('capture_id')} lacks {rock} middle evidence")
        representatives[(rock, "middle")] = middle

    silhouette_cases = (
        ("clear-noon", "marble", "marble-v12-prefog-noon-base", 0.08),
        ("clear-dawn", "marble", "marble-v12-prefog-dawn-gemini", 0.06),
        ("overcast-dawn", "quartzite", "marble-v14-prefog-dawn-overcast-gemini", 0.06),
    )
    silhouette = []
    for condition, rock, case, minimum in silhouette_cases:
        path, report = SHARED.named_report("closeout", case)
        sidecar_commit(report, commits)
        source_reports.add(path.as_posix())
        row = SHARED.named_stratum(report, rock, "pre-fog")
        value = float(row["silhouette_weber_magnitude"])
        silhouette.append(
            {
                "condition": condition,
                "rock": rock,
                "report": path.as_posix(),
                "weber_magnitude": value,
                "minimum_weber_magnitude": minimum,
                "passed": value >= minimum,
            }
        )

    fog = []
    for condition, case in (
        ("clear-noon", "marble-v12-prefog-noon-base"),
        ("clear-dawn", "marble-v12-prefog-dawn-gemini"),
        ("overcast-dawn", "marble-v14-prefog-dawn-overcast-gemini"),
    ):
        path, report = SHARED.named_report("closeout", case)
        source_reports.add(path.as_posix())
        pre = SHARED.named_stratum(report, "marble", "pre-fog")
        end = SHARED.named_stratum(report, "marble", "fog")
        passed = SHARED.fog_endpoint_passes(pre, end)
        fog.append(
            {
                "condition": condition,
                "report": path.as_posix(),
                "pre_fog_blend": float(pre["expected_fog_blend"]),
                "fog_end_blend": float(end["expected_fog_blend"]),
                "endpoint_is_directional_sky": True,
                "passed": passed,
            }
        )

    family_distinction = SHARED.distinguish_families(cases, representatives)

    baseline_path, baseline_dark_report = SHARED.named_report("baseline", "basalt-v4-near-noon-gemini")
    closeout_path, closeout_dark_report = SHARED.named_report("closeout", "basalt-v4-near-noon-gemini")
    sidecar_commit(closeout_dark_report, commits)
    source_reports.update((baseline_path.as_posix(), closeout_path.as_posix()))
    baseline_dark = SHARED.named_stratum(baseline_dark_report, "basalt", "near")
    closeout_dark = SHARED.named_stratum(closeout_dark_report, "basalt", "near")
    detail_retention = float(closeout_dark["rms_contrast_16px"]) / max(
        float(baseline_dark["rms_contrast_16px"]), 1.0e-12
    )
    black_delta = float(closeout_dark["display_black_fraction"]) - float(
        baseline_dark["display_black_fraction"]
    )
    dark_control = [
        {
            "rock": "basalt",
            "baseline_report": baseline_path.as_posix(),
            "closeout_report": closeout_path.as_posix(),
            "detail_retention": detail_retention,
            "minimum_detail_retention": 0.90,
            "display_black_fraction_delta": black_delta,
            "maximum_display_black_fraction_delta": 0.01,
            "passed": detail_retention >= 0.90 and black_delta <= 0.01,
        }
    ]
    passed = all(
        row["passed"]
        for rows in (retention, silhouette, fog, family_distinction, dark_control)
        for row in rows
    )
    return {
        "qualification_schema_version": QUALIFICATION_SCHEMA_VERSION,
        "kind": READABILITY_KIND,
        "baseline_commit": baseline_commit,
        "after_commit": single_commit(commits, "closeout readability"),
        "source_reports": sorted(source_reports),
        "passed": passed,
        "retention": retention,
        "silhouette": silhouette,
        "fog": fog,
        "family_distinction": family_distinction,
        "dark_control": dark_control,
    }


def build_closeout_strata_performance(expected_baseline: str = STRATA_BASELINE_COMMIT) -> dict[str, Any]:
    samples: dict[str, list[float]] = {
        "baseline_draw": [],
        "baseline_sim": [],
        "closeout_draw": [],
        "closeout_sim": [],
    }
    phase_commits: dict[str, set[str]] = {"baseline": set(), "closeout": set()}
    source_reports = []
    for phase in ("baseline", "closeout"):
        for repeat in "abcde":
            path, report = SHARED.named_report("closeout", f"performance-{phase}-{repeat}")
            source_reports.append(path.as_posix())
            sidecar_commit(report, phase_commits[phase])
            _sidecar_bytes, sidecar = read_toml(
                SHARED.safe_repo_path(report["sidecar"], "performance sidecar")
            )
            telemetry = sidecar.get("telemetry", {})
            samples[f"{phase}_draw"].append(float(telemetry["draw_ms"]))
            samples[f"{phase}_sim"].append(float(telemetry["simulation_ms"]))
    baseline_commit = single_commit(phase_commits["baseline"], "closeout strata baseline performance")
    closeout_commit = single_commit(phase_commits["closeout"], "closeout strata performance")
    if baseline_commit != expected_baseline:
        raise CloseoutEvidenceError(
            "closeout baseline performance captures must come from the declared baseline build"
        )
    medians = {name: SHARED.percentile(values.copy(), 0.5) for name, values in samples.items()}
    draw_budget = max(0.30, medians["baseline_draw"] * 0.05)
    draw_delta = medians["closeout_draw"] - medians["baseline_draw"]
    sim_delta = round(medians["closeout_sim"], 2) - round(medians["baseline_sim"], 2)
    passed = (
        draw_delta <= draw_budget
        and sim_delta <= STRATA_CLOSEOUT_SIMULATION_BUDGET_MS + 1.0e-9
        and max(samples["closeout_draw"]) <= 2.0 * max(samples["baseline_draw"])
    )
    return {
        "qualification_schema_version": QUALIFICATION_SCHEMA_VERSION,
        "kind": STRATA_PERFORMANCE_KIND,
        "baseline_commit": baseline_commit,
        "after_commit": closeout_commit,
        "source_reports": source_reports,
        "baseline_draw_ms": samples["baseline_draw"],
        "closeout_draw_ms": samples["closeout_draw"],
        "baseline_simulation_ms": samples["baseline_sim"],
        "closeout_simulation_ms": samples["closeout_sim"],
        "baseline_median_draw_ms": medians["baseline_draw"],
        "closeout_median_draw_ms": medians["closeout_draw"],
        "median_draw_delta_ms": draw_delta,
        "maximum_median_draw_regression_ms": draw_budget,
        "baseline_median_simulation_ms": medians["baseline_sim"],
        "closeout_median_simulation_ms": medians["closeout_sim"],
        "median_simulation_delta_ms_at_0_01ms_precision": sim_delta,
        "maximum_median_simulation_regression_ms": STRATA_CLOSEOUT_SIMULATION_BUDGET_MS,
        "simulation_budget_basis": (
            "cross-arc: baseline executable predates the magic arc's"
            " simulation systems; draw keeps the goal-2 budget"
            if expected_baseline == STRATA_BASELINE_COMMIT else
            "unchanged cross-scene closeout budget; the separate strata-performance"
            " qualification retains its 0.10 ms simulation gate"
        ),
        "maximum_closeout_draw_ms": max(samples["closeout_draw"]),
        "maximum_allowed_single_draw_ms": 2.0 * max(samples["baseline_draw"]),
        "passed": passed,
    }


def build_closeout_geode_reports() -> tuple[dict[str, Any], dict[str, Any]]:
    composition = GEODE.composition_report(
        stem_prefix="closeout-geode",
        scene_id=GEODE_SCENE_ID,
        kind=GEODE_COMPOSITION_KIND,
    )
    performance = GEODE.performance_report(
        stem_prefix="closeout-geode-performance",
        scene_id=GEODE_PERFORMANCE_SCENE_ID,
        kind=GEODE_PERFORMANCE_KIND,
    )
    return composition, performance


def qualification_reports() -> tuple[dict[str, Any], dict[str, Any], dict[str, Any], dict[str, Any]]:
    readability = build_closeout_readability()
    strata_performance = build_closeout_strata_performance()
    composition, geode_performance = build_closeout_geode_reports()
    commits = {
        readability["after_commit"],
        strata_performance["after_commit"],
        composition["evidence_commit"],
        geode_performance["evidence_commit"],
    }
    if len(commits) != 1:
        raise CloseoutEvidenceError(
            f"closeout scenes were not captured from one clean commit: {sorted(commits)}"
        )
    return readability, strata_performance, composition, geode_performance


def mean_display_luminance(color: bytes) -> float:
    total = 0.0
    for offset in range(0, len(color), 3):
        total += 0.2126 * color[offset] + 0.7152 * color[offset + 1] + 0.0722 * color[offset + 2]
    return total / (len(color) // 3) / 255.0


def build_motion_report(walks_path: Path) -> dict[str, Any]:
    try:
        plan = json.loads(walks_path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise CloseoutEvidenceError(f"read walk plan {walks_path}: {error}") from error
    commit = str(plan.get("commit", ""))
    if len(commit) != 40:
        raise CloseoutEvidenceError("walk plan must name the 40-hex closeout commit")
    reviewed_by = str(plan.get("reviewed_by", ""))
    review_method = str(plan.get("review_method", ""))
    if not reviewed_by or not review_method:
        raise CloseoutEvidenceError("walk plan must name its reviewer and review method")
    walk_rows = []
    expected_ids = {"strata-site", "geode-approach"}
    seen_ids = set()
    for walk in plan.get("walks", []):
        walk_id = str(walk.get("id", ""))
        if walk_id not in expected_ids or walk_id in seen_ids:
            raise CloseoutEvidenceError(f"unexpected or duplicate walk id {walk_id!r}")
        seen_ids.add(walk_id)
        frames = walk.get("frames", [])
        if len(frames) < 12:
            raise CloseoutEvidenceError(f"walk {walk_id} needs at least 12 frames")
        files: list[str] = []
        hashes: list[str] = []
        luminance: list[float] = []
        phases: list[str] = []
        for frame in frames:
            path = Path(str(frame["file"]))
            data = path.read_bytes()
            width, height, color = SHARED.read_ppm(data)
            if width < 320 or height < 200:
                raise CloseoutEvidenceError(f"walk frame {path} is implausibly small")
            files.append(path.name)
            hashes.append(sha256(data))
            luminance.append(mean_display_luminance(color))
            phases.append(str(frame["phase"]))
        static_segments = []
        start = None
        for index, phase in enumerate(phases):
            if phase.startswith("static"):
                if start is None:
                    start = index
            elif start is not None:
                static_segments.append([start, index - 1])
                start = None
        if start is not None:
            static_segments.append([start, len(phases) - 1])
        if not static_segments:
            raise CloseoutEvidenceError(f"walk {walk_id} declares no static hold to judge pumping")
        max_static_delta = 0.0
        for lo, hi in static_segments:
            for index in range(lo + 1, hi + 1):
                max_static_delta = max(
                    max_static_delta, abs(luminance[index] - luminance[index - 1])
                )
        observations = walk.get("observations", {})
        row = {
            "id": walk_id,
            "world": str(walk.get("world", "")),
            "weather": str(walk.get("weather", "")),
            "frame_count": len(files),
            "frame_files": files,
            "frame_sha256": hashes,
            "frame_mean_luminance": luminance,
            "static_segments": static_segments,
            "max_static_luminance_delta": max_static_delta,
            "maximum_static_luminance_delta": MAX_STATIC_LUMINANCE_DELTA,
            "exposure_pumping_detected": max_static_delta > MAX_STATIC_LUMINANCE_DELTA,
            "shimmer_observed": bool(observations.get("shimmer_observed", True)),
            "chunk_wall_observed": bool(observations.get("chunk_wall_observed", True)),
            "awkward_reveal_observed": bool(observations.get("awkward_reveal_observed", True)),
            "notes": str(observations.get("notes", "")),
        }
        if not row["world"]:
            raise CloseoutEvidenceError(f"walk {walk_id} must name its world")
        walk_rows.append(row)
    if seen_ids != expected_ids:
        raise CloseoutEvidenceError("motion evidence requires both closeout walks")
    passed = all(
        not row["exposure_pumping_detected"]
        and not row["shimmer_observed"]
        and not row["chunk_wall_observed"]
        and not row["awkward_reveal_observed"]
        for row in walk_rows
    )
    return {
        "qualification_schema_version": QUALIFICATION_SCHEMA_VERSION,
        "kind": MOTION_KIND,
        "evidence_commit": commit,
        "reviewed_by": reviewed_by,
        "review_method": review_method,
        "passed": passed,
        "walk": walk_rows,
    }


def qualify(args: argparse.Namespace) -> None:
    readability, strata_performance, composition, geode_performance = qualification_reports()
    write_atomic(Path(args.readability_report), render(readability))
    write_atomic(Path(args.strata_performance_report), render(strata_performance))
    write_atomic(Path(args.geode_composition_report), render(composition))
    write_atomic(Path(args.geode_performance_report), render(geode_performance))
    if not all(
        report["passed"]
        for report in (readability, strata_performance, composition, geode_performance)
    ):
        raise CloseoutEvidenceError("closeout qualification failed; reports were written")
    print("visual closeout: cross-scene qualification passed; four reports written")


def motion(args: argparse.Namespace) -> None:
    report = build_motion_report(Path(args.walks))
    write_atomic(Path(args.report), render(report))
    if not report["passed"]:
        raise CloseoutEvidenceError("closeout motion walkthrough failed; report was written")
    print(f"visual closeout: motion walkthroughs passed; wrote {args.report}")


def check(args: argparse.Namespace) -> None:
    rebuilt = qualification_reports()
    paths = (
        args.readability_report,
        args.strata_performance_report,
        args.geode_composition_report,
        args.geode_performance_report,
    )
    for path, report in zip(paths, rebuilt):
        existing_bytes, existing = read_toml(Path(path))
        if existing_bytes != render(report):
            raise CloseoutEvidenceError(f"{path} is stale or nondeterministic")
        if not existing.get("passed"):
            raise CloseoutEvidenceError(f"{path} records a failed gate")
    print("visual closeout: qualification reports are deterministic and current")


def check_motion(args: argparse.Namespace) -> None:
    existing_bytes, existing = read_toml(Path(args.report))
    if existing_bytes != render(build_motion_report(Path(args.walks))):
        raise CloseoutEvidenceError(f"{args.report} is stale or nondeterministic")
    if not existing.get("passed"):
        raise CloseoutEvidenceError(f"{args.report} records a failed gate")
    print("visual closeout: motion report is deterministic and current")


def self_test(_args: argparse.Namespace) -> None:
    fixture = {
        "passed": True,
        "values": [1.25, 2.5],
        "walk": [{"id": "strata-site", "frame_count": 12}],
    }
    if render(fixture) != render(fixture):
        raise CloseoutEvidenceError("deterministic render fixture failed")
    if toml_value([[0, 4], [7, 9]]) != "[[0, 4], [7, 9]]":
        raise CloseoutEvidenceError("nested segment rendering fixture failed")
    print("visual closeout: deterministic report fixture passed")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    commands = result.add_subparsers(dest="command", required=True)
    qualify_parser = commands.add_parser("qualify", help="write cross-scene acceptance reports")
    check_parser = commands.add_parser("check-qualification", help="reject stale acceptance reports")
    for sub in (qualify_parser, check_parser):
        sub.add_argument("--readability-report", required=True)
        sub.add_argument("--strata-performance-report", required=True)
        sub.add_argument("--geode-composition-report", required=True)
        sub.add_argument("--geode-performance-report", required=True)
    qualify_parser.set_defaults(run=qualify)
    check_parser.set_defaults(run=check)
    motion_parser = commands.add_parser("motion", help="analyze walkthrough frame sequences")
    motion_parser.add_argument("--walks", required=True)
    motion_parser.add_argument("--report", required=True)
    motion_parser.set_defaults(run=motion)
    check_motion_parser = commands.add_parser("check-motion", help="reject stale motion evidence")
    check_motion_parser.add_argument("--walks", required=True)
    check_motion_parser.add_argument("--report", required=True)
    check_motion_parser.set_defaults(run=check_motion)
    self_test_parser = commands.add_parser("self-test", help="run deterministic in-memory fixtures")
    self_test_parser.set_defaults(run=self_test)
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        args.run(args)
    except (CloseoutEvidenceError, SHARED.EvidenceError, GEODE.GeodeEvidenceError) as error:
        print(f"visual closeout: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
