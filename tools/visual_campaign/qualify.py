"""Apply the existing numeric gates to a complete newly captured campaign."""

import argparse
import contextlib
import json
import os
from pathlib import Path
import shutil

import verify_visual_polish as shared
import verify_cracked_geode as geode
import verify_visual_closeout as closeout

from .manifest import write
from .matrix import read_toml
from .provenance import inventory, source_sha256, write_json


@contextlib.contextmanager
def working_directory(path):
    previous = Path.cwd()
    os.chdir(path)
    try:
        yield
    finally:
        os.chdir(previous)


def measure(root: Path, work: Path, config: dict, rows: list[dict], only: str | None = None) -> None:
    evidence = work / "evidence"
    with working_directory(evidence):
        for row in rows:
            if only and row["id"] != only:
                continue
            report_path = Path(row["report"])
            args = argparse.Namespace(sidecar=row["sidecar"], report=row["report"], color_png=None, diagnostic_png=None)
            print(f"Measuring {row['id']}", flush=True)
            shared.report_command(args)
            shared.check_report_command(argparse.Namespace(report=str(report_path)))
        if only:
            return
        manifest_path = Path("screenshots/visual-polish.toml")
        manifest = read_toml(manifest_path)
        baseline = config["binaries"]["baseline"]["revision"]
        candidate = config["revision"]
        stamp = config["date"].replace("-", "")
        comparison = shared.build_comparison(Path(manifest["capture"][0]["report"]), Path(manifest["capture"][1]["report"]))
        write(Path(manifest["comparison"]), comparison)
        reports = {
            manifest["readability_report"]: shared.build_readability_qualification(baseline, candidate),
            manifest["performance_report"]: shared.build_performance_qualification(baseline, candidate),
            manifest["geode_composition_report"]: geode.composition_report(scene_id=f"cracked-geode-{stamp}"),
            manifest["geode_performance_report"]: geode.performance_report(scene_id=f"cracked-geode-performance-{stamp}"),
            manifest["closeout"]["readability_report"]: closeout.build_closeout_readability(baseline),
            manifest["closeout"]["strata_performance_report"]: closeout.build_closeout_strata_performance(baseline),
            manifest["closeout"]["geode_composition_report"]: geode.composition_report(
                stem_prefix="closeout-geode", scene_id=f"closeout-geode-{stamp}", kind=closeout.GEODE_COMPOSITION_KIND),
            manifest["closeout"]["geode_performance_report"]: geode.performance_report(
                stem_prefix="closeout-geode-performance", scene_id=f"closeout-geode-performance-{stamp}", kind=closeout.GEODE_PERFORMANCE_KIND),
            manifest["closeout"]["motion_report"]: closeout.build_motion_report(work / "motion-review.json"),
        }
        for path, report in reports.items():
            write(Path(path), report)
        failures = [path for path, report in reports.items() if not report["passed"]]
        if not comparison["equivalent"]:
            failures.append(manifest["comparison"])
        write_json(work / "qualification.json", {"source_sha256": config["source_sha256"], "failures": failures})
        if failures:
            raise ValueError("GPU qualification failed: " + ", ".join(failures))
        if source_sha256(root) != config["source_sha256"]:
            raise ValueError("source changed during qualification")
        for name, path in config["sources"].items():
            if inventory(Path(path)) != config["source_inventories"][name]:
                raise ValueError(f"campaign fixture changed: {name}")
        manifest["status"] = "accepted"
        write(manifest_path, manifest)


def publish(root: Path, work: Path) -> None:
    manifest = read_toml(work / "evidence/screenshots/visual-polish.toml")
    if manifest["status"] != "accepted" or manifest["qualification_source_sha256"] != source_sha256(root):
        raise ValueError("only an accepted, current campaign can be copied into the repository")
    selection = read_toml(root / "screenshots/current-campaign.toml")
    relative = Path(selection["root"])
    if relative.is_absolute() or ".." in relative.parts or relative.parts[0] != "screenshots":
        raise ValueError("unsafe campaign selection")
    destination = root / relative
    if destination.exists():
        raise ValueError("refusing to overwrite an existing campaign")
    for path in (work / "evidence").rglob("*.toml"):
        target = destination / path.relative_to(work / "evidence")
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, target)
    record = json.loads((work / "campaign.json").read_text())
    summary = {"date": record["date"], "source_sha256": record["source_sha256"],
               "binaries": {name: {key: value for key, value in binary.items() if key != "path"}
                            for name, binary in record["binaries"].items()},
               "capture_count": len(json.loads((work / "matrix.json").read_text()))}
    write_json(destination / "execution.json", summary)
    for directory in [destination, *sorted(path for path in destination.rglob("*") if path.is_dir())]:
        purpose = "Native GPU campaign metadata and deterministic qualification reports."
        (directory / "README.md").write_text(
            "<!-- wildforge:guide -->\n\n# Visual qualification evidence\n\n" + purpose + "\n\n"
            "These files belong to the campaign selected by `screenshots/current-campaign.toml`.\n"
            "Run `cargo test --locked visual_capture::tests::` from the repository root.\n"
            "Raw frames and fixture saves remain in the campaign's ignored work directory.\n")
        (directory / "AGENTS.md").write_text(
            "<!-- wildforge:guide -->\n\n# Working on campaign evidence\n\n"
            "Read the local README and repository instructions. Preserve measured values and\n"
            "their provenance. A different build needs fresh native GPU captures; do not\n"
            "rewrite existing timings or hashes to manufacture a passing result. Validate\n"
            "the complete campaign and retain failed attempts in working storage.\n")
