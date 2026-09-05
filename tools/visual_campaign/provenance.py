"""Source identity shared by the campaign driver and Rust freshness validator."""

import hashlib
import json
from pathlib import Path
import subprocess

SOURCE_DIRECTORIES = ("src", "base", "packs", "mods/gems", "tools")
SOURCE_EXTENSIONS = {".rs", ".wgsl", ".png", ".toml", ".rhai", ".py"}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_sha256(root: Path) -> str:
    files = {Path(name) for name in ("Cargo.toml", "Cargo.lock", "build.rs")}
    for directory in SOURCE_DIRECTORIES:
        parent = root / directory
        if not parent.is_dir():
            raise ValueError(f"missing qualification source directory: {directory}")
        files.update(
            path.relative_to(root)
            for path in parent.rglob("*")
            if path.is_file() and path.suffix in SOURCE_EXTENSIONS
        )
    digest = hashlib.sha256()
    for relative in sorted(files):
        data = (root / relative).read_bytes()
        digest.update(relative.as_posix().encode())
        digest.update(b"\0")
        digest.update(len(data).to_bytes(8, "little"))
        digest.update(data)
    return digest.hexdigest()


def clean_revision(root: Path) -> str:
    status = subprocess.check_output(
        ["git", "status", "--porcelain", "--untracked-files=normal"], cwd=root, text=True
    )
    if status.strip():
        raise ValueError("qualification requires a clean source checkout")
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()


def inventory(root: Path) -> list[dict]:
    return [
        {"path": path.relative_to(root).as_posix(), "bytes": path.stat().st_size, "sha256": sha256(path)}
        for path in sorted(root.rglob("*")) if path.is_file()
    ]


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
    temporary.replace(path)
