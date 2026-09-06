"""Freeze native executables and real production worlds before measuring."""

from pathlib import Path
import shutil
import subprocess

from .capture import copy_world
from .manifest import write
from .matrix import plan
from .provenance import clean_revision, inventory, sha256, source_sha256, write_json


def initialize(root: Path, work: Path, source: Path, baseline: Path, baseline_revision: str, candidate: Path, date: str) -> None:
    if (work / "campaign.json").exists():
        raise ValueError("campaign already initialized; use capture to resume")
    revision = clean_revision(root)
    if len(baseline_revision) != 40 or baseline_revision == revision:
        raise ValueError("baseline must name a different full commit")
    evidence = work / "evidence"
    sources = work / "sources"
    binaries = work / "frozen-bin"
    for directory in (evidence, sources, binaries):
        directory.mkdir(parents=True, exist_ok=False)
    config = {"date": date, "revision": revision, "source_sha256": source_sha256(root), "binaries": {}, "sources": {}}
    for name, executable, commit in (("baseline", baseline, baseline_revision), ("candidate", candidate, revision)):
        destination = binaries / name
        shutil.copy2(executable, destination)
        config["binaries"][name] = {"path": str(destination), "revision": commit, "sha256": sha256(destination)}
    for name in ("visual-polish-strata-baseline", "visual-polish-strata-perf", "visual-polish-foundation"):
        destination = sources / name
        copy_world(source, destination)
        config["sources"][name] = str(destination)
    (work / "empty-mods").mkdir()
    (sources / "closeout").mkdir()
    report_dir = evidence / "screenshots/visual-polish"
    report_dir.mkdir(parents=True)
    site = report_dir / "cracked-geode-site.toml"
    executable = config["binaries"]["candidate"]["path"]
    commands = [
        ["--locate-cracked-geode", config["sources"]["visual-polish-strata-baseline"], "--output", str(site)],
        ["--clone-cracked-geode-source", config["sources"]["visual-polish-strata-baseline"],
         "--destination", str(sources / "visual-polish-geode-sealed"), "--site", str(site)],
        ["--prepare-cracked-geode", str(sources / "visual-polish-geode-sealed"),
         "--destination", str(sources / "visual-polish-geode-opened"), "--site", str(site),
         "--output", str(report_dir / "geode-preparation.report.toml")],
        ["--prepare-cracked-geode", str(sources / "closeout/visual-polish-geode-sealed"),
         "--destination", str(sources / "closeout/visual-polish-geode-opened"), "--site", str(site),
         "--output", str(report_dir / "closeout-geode-preparation.report.toml")],
    ]
    source_before = inventory(source)
    for index, arguments in enumerate(commands):
        if index == 3:
            # The game's preparation transaction requires sibling saves.
            # Give the independently repeated closeout its own sealed input.
            copy_world(sources / "visual-polish-geode-sealed", sources / "closeout/visual-polish-geode-sealed")
        argv = [executable, *arguments, "--mods", str(work / "empty-mods")]
        print(f"Preparing production fixture {index + 1}/{len(commands)}", flush=True)
        write_json(work / f"preparation-{index}.argv.json", argv)
        with (work / f"preparation-{index}.log").open("w") as log:
            subprocess.run(argv, cwd=work, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=600)
    if inventory(source) != source_before:
        raise ValueError("source world changed during preparation")
    for name in ("visual-polish-geode-sealed", "visual-polish-geode-opened"):
        config["sources"][name] = str(sources / name)
    config["sources"]["closeout-opened"] = str(sources / "closeout/visual-polish-geode-opened")
    config["source_inventories"] = {name: inventory(Path(path)) for name, path in config["sources"].items()}
    manifest, rows = plan(root, date)
    for field in ("evidence_commit", "after_commit", "geode_evidence_commit"):
        manifest[field] = revision
    manifest["baseline_commit"] = baseline_revision
    manifest["closeout"]["commit"] = revision
    for field in ("qualification_source_sha256", "geode_qualification_source_sha256"):
        manifest[field] = config["source_sha256"]
    manifest["conversion_tool_sha256"] = sha256(root / manifest["conversion_tool"])
    manifest["geode_verifier_sha256"] = sha256(root / manifest["geode_verifier"])
    manifest["closeout"]["verifier_sha256"] = sha256(root / manifest["closeout"]["verifier"])
    write(evidence / "screenshots/visual-polish.toml", manifest)
    write_json(work / "matrix.json", rows)
    write_json(work / "campaign.json", config)
