"""Replay the complete declared axes with today's recorded capture identities."""

import copy
from pathlib import Path
import tomllib


def read_toml(path: Path) -> dict:
    return tomllib.loads(path.read_text())


def plan(root: Path, date: str) -> tuple[dict, list[dict]]:
    manifest = copy.deepcopy(read_toml(root / "screenshots/visual-polish.toml"))
    manifest["schema_version"] = 4
    manifest["status"] = "pending"
    manifest["date"] = date
    manifest["closeout"]["date"] = date
    stamp = date.replace("-", "")
    rows = []

    def add(declaration, binary, scene, camera_template=None):
        template = read_toml(root / (camera_template or declaration["sidecar"]))
        rows.append({
            "id": declaration["id"], "sidecar": declaration["sidecar"],
            "report": declaration["report"], "binary": binary, "scene": scene,
            "template": template,
        })

    for declaration in manifest["capture"]:
        add(declaration, "candidate", manifest["scene_id"])
    # Pair the two builds at the same currently recorded camera. The old
    # baseline and closeout cameras intentionally differed after lake changes.
    for phase in ("baseline", "after"):
        for declaration in manifest["case"]:
            if declaration["phase"] == phase:
                suffix = declaration["id"].removeprefix(f"strata-{phase}-")
                add(declaration, "baseline" if phase == "baseline" else "candidate",
                    f"strata-production-{stamp}",
                    f"screenshots/strata-closeout-{suffix}.capture.toml")
    for declaration in manifest["performance"]:
        add(declaration, "baseline" if declaration["phase"] == "baseline" else "candidate",
            f"strata-performance-{stamp}",
            "screenshots/strata-closeout-performance-closeout-a.capture.toml")
    for declaration in manifest["geode_capture"]:
        add(declaration, "candidate", f"cracked-geode-{stamp}")
    for declaration in manifest["geode_performance_capture"]:
        add(declaration, "candidate", f"cracked-geode-performance-{stamp}")
    for declaration in manifest["closeout"]["case"]:
        add(declaration, "candidate", f"closeout-strata-{stamp}")
    for declaration in manifest["closeout"]["performance"]:
        add(declaration, "baseline" if declaration["phase"] == "baseline" else "candidate",
            f"closeout-strata-performance-{stamp}",
            "screenshots/strata-closeout-performance-closeout-a.capture.toml")
    for declaration in manifest["closeout"]["geode_capture"]:
        add(declaration, "candidate", f"closeout-geode-{stamp}")
    for declaration in manifest["closeout"]["geode_performance_capture"]:
        add(declaration, "candidate", f"closeout-geode-performance-{stamp}")
    # Performance comes last, so builds and scene preparation have completed.
    ordinary = [row for row in rows if "performance" not in row["id"]]
    performance = [row for row in rows if "performance" in row["id"]]
    performance.sort(key=lambda row: (row["id"].rsplit("-", 1)[-1], row["id"]))
    if len(rows) != 92 or len({row["id"] for row in rows}) != 92:
        raise ValueError("the complete campaign must contain 92 unique captures")
    return manifest, ordinary + performance
