"""Record native input and frame sequences; visual review is a separate step."""

import copy
from pathlib import Path
import subprocess
import time

from .capture import configure, copy_world, environment
from .display import isolated_display, request_close
from .provenance import sha256, write_json


def wait_for(test, process, timeout=180):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"native motion process exited {process.returncode}")
        result = test()
        if result:
            return result
        time.sleep(0.1)
    raise TimeoutError("native motion condition was not reached")


def window_for(pid, env=None):
    result = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(pid)], env=env, capture_output=True, text=True)
    windows = result.stdout.split()
    return windows[0] if result.returncode == 0 and len(windows) == 1 else None


def record(root: Path, work: Path, config: dict, rows: list[dict]) -> None:
    with isolated_display(work) as display:
        record_walks(root, work, config, rows, display)


def record_walks(root: Path, work: Path, config: dict, rows: list[dict], display: str) -> None:
    plan = {"commit": config["revision"], "reviewed_by": "", "review_method": "", "walks": []}
    cases = (("strata-site", "closeout-motion-strata", "strata-closeout-sandstone-v4-near-noon-base"),
             ("geode-approach", "closeout-motion-geode", "closeout-geode-aperture-proof"))
    for walk_id, world_name, capture_id in cases:
        row = copy.deepcopy(next(row for row in rows if row["id"] == capture_id))
        source_key = row["template"]["world"]["name"]
        if capture_id.startswith("closeout-geode") and source_key.endswith("-opened"):
            source_key = "closeout-opened"
        row["template"]["world"]["name"] = world_name
        row["id"] = walk_id
        row["scene"] = f"motion-{config['date']}"
        attempts = work / "motion" / walk_id
        attempts.mkdir(parents=True, exist_ok=True)
        directory = attempts / str(len(list(attempts.iterdir())) + 1)
        directory.mkdir()
        copy_world(Path(config["sources"][source_key]), directory / "saves" / world_name)
        (directory / "mods").mkdir()
        (directory / "packs").symlink_to(root / "packs", target_is_directory=True)
        configure(directory, row["template"]["render"])
        env = environment(row, "unreached-auto-shot.ppm")
        # The existing capture flyover keeps height and time fixed. F2 records
        # actual incremental camera travel; no frame teleports the camera.
        env["WILDFORGE_SHOT_MIN_FRAME"] = "1000000000"
        env["WILDFORGE_INPUT_TRACE"] = "1"
        env["DISPLAY"] = display
        binary = config["binaries"]["candidate"]
        if sha256(Path(binary["path"])) != binary["sha256"]:
            raise ValueError("motion executable changed")
        result = {"id": walk_id, "world": world_name, "binary": binary, "frames": [],
                  "movement": "native WASD steps at fixed height, 25 ms key holds", "ui_commands": [], "display": display,
                  "environment": {key: value for key, value in env.items() if key.startswith("WILDFORGE_")}}

        def action(*arguments):
            argv = ["xdotool", *arguments]
            completed = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=10, check=True)
            result["ui_commands"].append({"argv": argv, "stdout": completed.stdout,
                                          "seconds": round(time.monotonic() - started, 3)})
            return completed.stdout.strip()

        print(f"Recording motion {walk_id}", flush=True)
        started = time.monotonic()
        log_path = directory / "game.log"
        with log_path.open("w") as log:
            process = subprocess.Popen([binary["path"]], cwd=directory, env=env, stdout=log, stderr=subprocess.STDOUT)
            result["pid"] = process.pid
            try:
                window = wait_for(lambda: window_for(process.pid, env), process)
                wait_for(lambda: "visual evidence: initial chunk uploads settled" in log_path.read_text(), process)
                adapter = next(line for line in log_path.read_text().splitlines() if line.startswith("renderer: using "))
                if "Vulkan, DiscreteGpu" not in adapter:
                    raise RuntimeError("motion capture did not use the native discrete GPU")
                result["adapter"] = adapter
                time.sleep(3)
                action("windowfocus", "--sync", window)
                if action("getwindowfocus") != window:
                    raise RuntimeError("motion window did not receive focus")
                known = set()
                for phase, count, key in (("static-start", 4, None), ("forward", 8, "w"),
                                          ("static-near", 4, None), ("backward", 8, "s"),
                                          ("static-end", 4, None)):
                    for _ in range(count):
                        time.sleep(1.1)
                        # Small steps traverse the aperture without spending
                        # the walkthrough pushing against its far wall.
                        if key:
                            action("keydown", "--window", window, key)
                            time.sleep(0.025)
                            action("keyup", "--window", window, key)
                        action("key", "--window", window, "F2")
                        paths = wait_for(lambda: set(directory.glob("screenshot-*.ppm")) - known, process, 10)
                        path = next(iter(paths))
                        render = row["template"]["render"]
                        minimum_size = render["width"] * render["height"] * 3
                        wait_for(lambda: path.stat().st_size >= minimum_size, process, 10)
                        time.sleep(0.1)
                        known.add(path)
                        result["frames"].append({"file": str(path), "phase": phase, "sha256": sha256(path)})
                trace = log_path.read_text()
                for code in ("KeyW", "KeyS", "F2"):
                    for edge in ("pressed", "released"):
                        if f"input: {code} {edge}" not in trace:
                            raise RuntimeError(f"native input trace is missing {code} {edge}")
                request_close(display, window)
                result["close_method"] = "ICCCM WM_DELETE_WINDOW"
                result["exit_code"] = process.wait(timeout=60)
                if result["exit_code"]:
                    raise RuntimeError("motion process failed during normal close")
                result["completed"] = True
            finally:
                if process.poll() is None:
                    try:
                        action("keyup", "w", "s")
                    finally:
                        process.terminate()
                        try:
                            process.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            process.kill()
                            process.wait()
                result["seconds"] = round(time.monotonic() - started, 3)
                write_json(directory / "result.json", result)
        plan["walks"].append({"id": walk_id, "world": world_name, "frames": result["frames"]})
        write_json(work / "motion-review.json", plan)
    print("Motion frames captured. Record visual observations in motion-review.json before qualification.", flush=True)
