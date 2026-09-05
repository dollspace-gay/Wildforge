#!/usr/bin/env python3
"""Run the real client interaction tests with a disposable save and identity."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, help='new directory for native proof artifacts')
    args = parser.parse_args()
    output = args.output.resolve() if args.output else None
    if output is not None and output.exists():
        parser.error('the output directory already exists; use a new attempt directory')
    if sys.platform != "linux":
        raise SystemExit("The real client proof requires Linux and an X11 display.")
    root = Path(__file__).resolve().parents[1]
    env = os.environ.copy()
    if not shutil.which("sccache"):
        env["RUSTC_WRAPPER"] = ""
    built = subprocess.run(
        ["cargo", "test", "--locked", "--lib", "--no-run", "--message-format=json"],
        cwd=root, env=env, text=True, stdout=subprocess.PIPE,
    )
    executable = None
    for line in built.stdout.splitlines():
        message = json.loads(line)
        if message.get("reason") == "compiler-message":
            print(message["message"].get("rendered", ""), end="", flush=True)
        if message.get("reason") == "compiler-artifact" and message.get("profile", {}).get("test"):
            executable = message.get("executable") or executable
    if built.returncode:
        return built.returncode
    if not executable:
        raise RuntimeError("Cargo did not produce a test executable")
    if output is not None:
        output.mkdir(parents=True)
    with tempfile.TemporaryDirectory(prefix="wildforge-gameplay-proof-") as temporary:
        stage = Path(temporary)
        shutil.copytree(root / "test-fixtures/gameplay/mods", stage / "mods")
        (stage / "config.txt").write_text("volume=0\nview_dist=4\n", encoding="utf-8")
        for key in list(env):
            if key.startswith("WILDFORGE_"):
                del env[key]
        result = subprocess.run(
            [executable, "gameplay_proofs::", "--ignored", "--nocapture", "--test-threads=1"],
            cwd=stage, env=env,
        )
        if output is not None:
            artifacts = []
            for name in ('native-guest-entry.ppm', 'native-guest-entry.json'):
                path = stage / name
                if path.is_file():
                    shutil.copy2(path, output / name)
                    artifacts.append(name)
            with open(executable, 'rb') as binary:
                digest = hashlib.file_digest(binary, 'sha256').hexdigest()
            (output / 'run.json').write_text(json.dumps({
                'suite_exit_code': result.returncode,
                'binary_sha256': digest,
                'artifacts': artifacts,
            }, indent=2) + '\n', encoding='utf-8')
            print(f'Native proof artifacts: {output}', flush=True)
        return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
