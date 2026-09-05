"""Run one native game process against one copy of an immutable fixture."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import tomllib

from .provenance import sha256, write_json

FACES = {f"{sign}{axis}": f"{sign.lower()}_{axis.lower()}" for sign in ("Pos", "Neg") for axis in "XYZ"}


def copy_world(source: Path, destination: Path) -> None:
    if destination.exists():
        raise ValueError(f"refusing to overwrite a campaign world: {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["cp", "--reflink=auto", "-a", "--", str(source), str(destination)], check=True)


def configure(directory: Path, render: dict) -> None:
    values = {
        "display_name": "Qualification", "profile_complete": "yes", "volume": 0,
        "view_dist": render["view_distance_chunks"], "fov": 75, "pack": render["pack"],
        "lights": ("off", "on", "shadows")[render["lights"]],
        "point_shadows": "grid" if render["point_grid"] else "cube",
        "darkness": "stark" if render["stark"] else "soft",
        "bloom": "on" if render["bloom"] else "off",
    }
    (directory / "config.txt").write_text("".join(f"{key}={value}\n" for key, value in values.items()))


def environment(row: dict, output: str | None) -> dict[str, str]:
    template = row["template"]
    camera, render, weather = (template[key] for key in ("camera", "render", "environment"))
    result = {key: value for key, value in os.environ.items()
              if not key.startswith("WILDFORGE_") and key != "WAYLAND_DISPLAY"}
    result.update({
        "WILDFORGE_WORLD": template["world"]["name"],
        "WILDFORGE_FACE": FACES[camera["face"]],
        "WILDFORGE_POS": f"{camera['u'] - 4096},{camera['y']},{camera['v'] - 4096}",
        "WILDFORGE_LOOK": f"{camera['yaw']},{camera['pitch']}",
        "WILDFORGE_SHOT_ALTITUDE": "0",
        "WILDFORGE_TIME": str(weather["time_of_day"]),
        "WILDFORGE_DAY": str(weather["day"]),
        "WILDFORGE_WEATHER": weather["precipitation"] if weather["weather"] == "precipitation" else weather["weather"],
        "WILDFORGE_VIEW_DIST": str(render["view_distance_chunks"]),
        "WILDFORGE_SHOT_SIZE": f"{render['width']}x{render['height']}",
        "WILDFORGE_SHOT_MIN_FRAME": "1200" if "performance" in row["id"] else "180",
        "WILDFORGE_PROFILE": "1",
    })
    if output is not None:
        result.update({"WILDFORGE_SHOT": output, "WILDFORGE_VISUAL_EVIDENCE": "1",
                       "WILDFORGE_CAPTURE_ID": row["id"], "WILDFORGE_CAPTURE_SCENE": row["scene"]})
    return result


def validate_sidecar(path: Path, row: dict, revision: str) -> dict:
    metadata = tomllib.loads(path.read_text())
    if metadata["build"]["commit"] != revision or metadata["build"]["dirty"]:
        raise ValueError(f"{row['id']}: wrong or dirty executable identity")
    render, telemetry = metadata["render"], metadata["telemetry"]
    if (not render["hardware"] or render["backend"] not in ("Vulkan", "Dx12")
            or "DiscreteGpu" not in render["adapter"]):
        raise ValueError(f"{row['id']}: no native discrete GPU evidence")
    # The native writer and Rust validator own SHOT_SETTLE_FRAMES. Already
    # visible chunks can be dirtied again by live water, weather, and ecology.
    if not telemetry["settled"] or telemetry["settled_frames"] <= 0:
        raise ValueError(f"{row['id']}: unsettled capture")
    if metadata["capture_id"] != row["id"] or metadata["scene_id"] != row["scene"]:
        raise ValueError(f"{row['id']}: wrong capture identity")
    template = row["template"]
    for key in ("name", "seed", "generator_version", "atlas_format_version", "atlas_algorithm_version"):
        if metadata["world"][key] != template["world"][key]:
            raise ValueError(f"{row['id']}: captured world {key} differs from the requested scene")
    for key in ("width", "height", "pack", "view_distance_chunks", "lights", "point_grid", "stark", "bloom"):
        if render[key] != template["render"][key]:
            raise ValueError(f"{row['id']}: captured render {key} differs from the requested scene")
    for key in ("weather", "precipitation"):
        if metadata["environment"][key] != template["environment"][key]:
            raise ValueError(f"{row['id']}: captured {key} differs from the requested scene")
    return metadata


def execute(root: Path, work: Path, config: dict, row: dict) -> None:
    attempts = work / "attempts" / row["id"]
    attempts.mkdir(parents=True, exist_ok=True)
    previous = list(attempts.glob("*/result.json"))
    for path in previous:
        result = json.loads(path.read_text())
        if result.get("accepted"):
            for artifact, digest in result["artifacts"].items():
                if sha256(work / "evidence" / artifact) != digest:
                    raise ValueError(f"accepted artifact changed: {artifact}")
            print(f"Already captured {row['id']}", flush=True)
            return
    directory = attempts / str(len(list(attempts.iterdir())) + 1)
    directory.mkdir()
    template = row["template"]
    world_name = template["world"]["name"]
    source_key = "closeout-opened" if row["id"].startswith("closeout-geode") and world_name.endswith("-opened") else world_name
    copy_world(Path(config["sources"][source_key]), directory / "saves" / world_name)
    (directory / "mods").mkdir()
    (directory / "packs").symlink_to(root / "packs", target_is_directory=True)
    (directory / "screenshots").mkdir()
    configure(directory, template["render"])
    stem = row["sidecar"].removesuffix(".capture.toml")
    env = environment(row, stem + ".ppm")
    binary = config["binaries"][row["binary"]]
    if sha256(Path(binary["path"])) != binary["sha256"]:
        raise ValueError("campaign executable changed")
    result = {"id": row["id"], "binary": binary,
              "environment": {key: value for key, value in env.items() if key.startswith("WILDFORGE_")},
              "accepted": False}
    started = time.monotonic()
    print(f"Capturing {row['id']} ({row['binary']})", flush=True)
    with (directory / "game.log").open("w") as log:
        process = subprocess.Popen([binary["path"]], cwd=directory, env=env, stdout=log, stderr=subprocess.STDOUT)
        result["pid"] = process.pid
        write_json(directory / "result.json", result)
        try:
            result["exit_code"] = process.wait(timeout=600)
            if result["exit_code"]:
                raise RuntimeError(f"native capture exited {result['exit_code']}")
            metadata = validate_sidecar(directory / row["sidecar"], row, binary["revision"])
            result["adapter"] = metadata["render"]["adapter"]
            result["telemetry"] = metadata["telemetry"]
            result["artifacts"] = {}
            for suffix in (".ppm", ".wfd", ".capture.toml"):
                relative = stem + suffix
                destination = work / "evidence" / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(directory / relative, destination)
                result["artifacts"][relative] = sha256(destination)
            result["accepted"] = True
        except Exception as error:
            result["error"] = str(error)
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            raise
        finally:
            result["seconds"] = round(time.monotonic() - started, 3)
            result["process_absent"] = not (Path("/proc") / str(process.pid)).exists()
            write_json(directory / "result.json", result)
    print(f"Captured {row['id']}: {result['seconds']} seconds", flush=True)
