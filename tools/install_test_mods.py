#!/usr/bin/env python3
"""Install the exact external content used by Wildforge's integration tests."""

from pathlib import Path
import shutil
import subprocess
import tempfile


REPOSITORY = "https://github.com/eternaldensity/belt-quest.git"
REVISION = "0f090889f2de692b47526d4f9da9f5dd8864ab57"


def main():
    root = Path(__file__).resolve().parents[1]
    destination = root / "mods/belt_quest"
    if destination.exists():
        raise SystemExit(f"Refusing to overwrite existing mod: {destination}")
    with tempfile.TemporaryDirectory(prefix="wildforge-test-mods-") as temporary:
        checkout = Path(temporary)
        subprocess.run(["git", "init", "--quiet", str(checkout)], check=True)
        subprocess.run(
            ["git", "-C", str(checkout), "fetch", "--depth=1", REPOSITORY, REVISION],
            check=True,
        )
        subprocess.run(
            ["git", "-C", str(checkout), "checkout", "--quiet", "--detach", "FETCH_HEAD"],
            check=True,
        )
        actual = subprocess.check_output(
            ["git", "-C", str(checkout), "rev-parse", "HEAD"], text=True,
        ).strip()
        if actual != REVISION:
            raise RuntimeError(f"Expected {REVISION}, fetched {actual}")
        source = checkout / "mods/belt_quest"
        if not (source / "mod.toml").is_file():
            raise RuntimeError("Pinned checkout has no belt_quest mod")
        shutil.copytree(source, destination)
    print(f"Installed belt_quest at {REVISION} into {destination}")


if __name__ == "__main__":
    main()
