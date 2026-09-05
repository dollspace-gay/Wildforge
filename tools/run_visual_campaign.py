#!/usr/bin/env python3
"""Rebuild the full native visual qualification campaign without changing old evidence."""

import argparse
import datetime
import json
from pathlib import Path

from visual_campaign.capture import execute
from visual_campaign.prepare import initialize
from visual_campaign.provenance import inventory, source_sha256
from visual_campaign.motion import record
from visual_campaign.qualify import measure, publish


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", required=True, type=Path)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare = commands.add_parser("prepare", help="freeze clean binaries and prepare real geode worlds")
    prepare.add_argument("--source-world", required=True, type=Path)
    prepare.add_argument("--baseline", required=True, type=Path)
    prepare.add_argument("--baseline-revision", required=True)
    prepare.add_argument("--candidate", required=True, type=Path)
    prepare.add_argument("--date", required=True, type=datetime.date.fromisoformat)
    capture = commands.add_parser("capture", help="capture or resume the declared matrix")
    capture.add_argument("--only", help="one capture ID for a focused retry")
    commands.add_parser("motion", help="record both native camera walkthroughs")
    metrics = commands.add_parser("qualify", help="convert captures and apply every acceptance gate")
    metrics.add_argument("--only", help="convert and inspect one capture before full qualification")
    commands.add_parser("publish", help="copy an accepted current campaign into a new evidence directory")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    if args.command == "prepare":
        initialize(root, work, args.source_world.resolve(), args.baseline.resolve(),
                   args.baseline_revision, args.candidate.resolve(), str(args.date))
        return
    config = json.loads((work / "campaign.json").read_text())
    if source_sha256(root) != config["source_sha256"]:
        raise ValueError("source changed after campaign initialization")
    rows = json.loads((work / "matrix.json").read_text())
    only = getattr(args, "only", None)
    if only:
        rows = [row for row in rows if row["id"] == args.only]
        if not rows:
            raise ValueError("unknown capture ID")
    if args.command == "motion":
        record(root, work, config, rows)
        return
    if args.command == "qualify":
        measure(root, work, config, rows, only)
        return
    if args.command == "publish":
        publish(root, work)
        return
    for row in rows:
        execute(root, work, config, row)
    for name, path in config["sources"].items():
        if inventory(Path(path)) != config["source_inventories"][name]:
            raise ValueError(f"immutable campaign fixture changed: {name}")


if __name__ == "__main__":
    main()
