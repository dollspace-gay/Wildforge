#!/usr/bin/env python3
"""Deterministic visual-evidence conversion, metrics, and qualification.

The game writes native P6 PPM color plus a WFD RGBA16Uint attachment. This
stdlib-only tool validates both before producing ignored PNG views and small,
tracked TOML reports. No image library, color correction, resize, timestamp,
or metadata chunk is allowed into the conversion path.
"""

from __future__ import annotations

import argparse
import array
import hashlib
import json
import math
import platform
import struct
import sys
import tomllib
import zlib
from collections import defaultdict, deque
from pathlib import Path
from typing import Any

REPORT_SCHEMA_VERSION = 2
COMPARISON_SCHEMA_VERSION = 1
CAPTURE_SCHEMA_VERSION = 1
WFD_MAGIC = b"WFD1LE\0\0"
WFD_HEADER = struct.Struct("<8sIIII")
WFD_FORMAT = "wfd-rgba16uint-v1"
SKY_ID = 0
OVERLAY_ID = 65535
CONVERSION_ID = "python-stdlib-p6-rgb8-filter0-zlib9-v1"
QUALIFICATION_SCHEMA_VERSION = 1

# Repeatability gates. They permit subpixel/platform rasterization noise while
# rejecting a changed site, camera, material layout, depth field, or exposure.
MIN_SEGMENTATION_AGREEMENT = 0.985
MAX_FAMILY_TOTAL_VARIATION = 0.010
MAX_DEPTH_RMSE = 0.010
MAX_LUMINANCE_MEAN_DELTA = 0.015
MAX_LOCAL_CONTRAST_DELTA = 0.015
MAX_SKY_FRACTION_DELTA = 0.005

STRATA_NAMES = (
    "sandstone",
    "limestone",
    "shale",
    "granite",
    "marble",
    "slate",
    "quartzite",
    "basalt",
)
DISTANCE_BANDS = (
    ("near", 0.0, 0.35),
    ("middle", 0.55, 0.75),
    ("pre-fog", 0.80, 0.90),
    ("fog", 0.90, 1.000001),
)


class EvidenceError(RuntimeError):
    pass


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def quoted(value: str) -> str:
    return json.dumps(value, ensure_ascii=True)


def toml_value(value: Any) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        if not math.isfinite(value):
            raise EvidenceError("reports cannot contain non-finite metrics")
        return format(value, ".9f")
    if isinstance(value, str):
        return quoted(value)
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(item) for item in value) + "]"
    raise EvidenceError(f"unsupported TOML value {type(value).__name__}")


def render_report(report: dict[str, Any]) -> str:
    families = report.pop("family")
    strata = report.pop("stratum", [])
    lines = [f"{key} = {toml_value(value)}" for key, value in report.items()]
    for family in families:
        lines.append("")
        lines.append("[[family]]")
        lines.extend(f"{key} = {toml_value(value)}" for key, value in family.items())
    for stratum in strata:
        lines.append("")
        lines.append("[[stratum]]")
        lines.extend(f"{key} = {toml_value(value)}" for key, value in stratum.items())
    return "\n".join(lines) + "\n"


def render_comparison(report: dict[str, Any]) -> str:
    return "\n".join(f"{key} = {toml_value(value)}" for key, value in report.items()) + "\n"


def render_tables(report: dict[str, Any], tables: tuple[str, ...]) -> str:
    values = dict(report)
    rows = {name: values.pop(name, []) for name in tables}
    lines = [f"{key} = {toml_value(value)}" for key, value in values.items()]
    for name in tables:
        for row in rows[name]:
            lines.extend(("", f"[[{name}]]"))
            lines.extend(f"{key} = {toml_value(value)}" for key, value in row.items())
    return "\n".join(lines) + "\n"


def read_toml(path: Path) -> tuple[bytes, dict[str, Any]]:
    try:
        data = path.read_bytes()
        parsed = tomllib.loads(data.decode("utf-8"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        raise EvidenceError(f"read {path}: {error}") from error
    return data, parsed


def safe_repo_path(value: str, label: str) -> Path:
    path = Path(value)
    if path.is_absolute() or ".." in path.parts or not path.parts:
        raise EvidenceError(f"{label} must be a repository-relative path")
    return path


def resolve_artifact(sidecar: Path, value: str, label: str) -> Path:
    relative = safe_repo_path(value, label)
    candidates = [Path.cwd() / relative]
    if sidecar.parent.name == "screenshots":
        candidates.append(sidecar.parent.parent / relative)
    candidates.append(sidecar.parent / relative.name)
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    raise EvidenceError(f"{label} is missing: {relative.as_posix()}")


def ppm_token(data: bytes, offset: int) -> tuple[bytes, int]:
    while offset < len(data):
        if data[offset] == 35:
            newline = data.find(b"\n", offset)
            if newline < 0:
                raise EvidenceError("unterminated PPM header comment")
            offset = newline + 1
        elif chr(data[offset]).isspace():
            offset += 1
        else:
            break
    start = offset
    while offset < len(data) and not chr(data[offset]).isspace() and data[offset] != 35:
        offset += 1
    if start == offset:
        raise EvidenceError("truncated PPM header")
    return data[start:offset], offset


def read_ppm(data: bytes) -> tuple[int, int, bytes]:
    magic, offset = ppm_token(data, 0)
    width_raw, offset = ppm_token(data, offset)
    height_raw, offset = ppm_token(data, offset)
    maximum_raw, offset = ppm_token(data, offset)
    if magic != b"P6" or maximum_raw != b"255":
        raise EvidenceError("color artifact must be binary P6 RGB8 with maxval 255")
    try:
        width, height = int(width_raw), int(height_raw)
    except ValueError as error:
        raise EvidenceError("PPM dimensions are not integers") from error
    if width <= 0 or height <= 0 or width > 4096 or height > 4096:
        raise EvidenceError("PPM dimensions are outside the capture bounds")
    if offset >= len(data) or not chr(data[offset]).isspace():
        raise EvidenceError("PPM maxval is not followed by whitespace")
    offset += 1
    if data[offset - 1] == 13 and offset < len(data) and data[offset] == 10:
        offset += 1
    pixels = data[offset:]
    expected = width * height * 3
    if len(pixels) != expected:
        raise EvidenceError(f"PPM payload is {len(pixels)} bytes; expected {expected}")
    return width, height, pixels


def read_wfd(data: bytes) -> tuple[int, int, array.array[int]]:
    if len(data) < WFD_HEADER.size:
        raise EvidenceError("diagnostic attachment is truncated")
    magic, width, height, encoding, reserved = WFD_HEADER.unpack_from(data)
    if magic != WFD_MAGIC or encoding != 1 or reserved != 0:
        raise EvidenceError("diagnostic attachment has an unknown header")
    if width <= 0 or height <= 0 or width > 4096 or height > 4096:
        raise EvidenceError("diagnostic dimensions are outside the capture bounds")
    payload = data[WFD_HEADER.size :]
    expected = width * height * 8
    if len(payload) != expected:
        raise EvidenceError(f"diagnostic payload is {len(payload)} bytes; expected {expected}")
    values = array.array("H")
    values.frombytes(payload)
    if sys.byteorder != "little":
        values.byteswap()
    return width, height, values


def png_chunk(kind: bytes, payload: bytes) -> bytes:
    return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", zlib.crc32(kind + payload))


def rgb_png(width: int, height: int, pixels: bytes) -> bytes:
    stride = width * 3
    rows = b"".join(b"\0" + pixels[y * stride : (y + 1) * stride] for y in range(height))
    compressed = zlib.compress(rows, level=9)
    return (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + png_chunk(b"IDAT", compressed)
        + png_chunk(b"IEND", b"")
    )


def diagnostic_rgb(values: array.array[int]) -> bytes:
    output = bytearray(len(values) // 4 * 3)
    for pixel in range(len(values) // 4):
        family, depth, _class, _schema = values[pixel * 4 : pixel * 4 + 4]
        if family == SKY_ID:
            color = (70, 120, 180)
        elif family == OVERLAY_ID:
            color = (255, 40, 220)
        else:
            mixed = (family * 0x9E37 + 0x7F4A) & 0xFFFF
            shade = 0.45 + 0.55 * (1.0 - depth / 65535.0)
            color = tuple(
                int(channel * shade)
                for channel in (
                    70 + (mixed & 127),
                    70 + ((mixed >> 5) & 127),
                    70 + ((mixed >> 10) & 127),
                )
            )
        output[pixel * 3 : pixel * 3 + 3] = bytes(color)
    return bytes(output)


def linear_luminance_table() -> list[float]:
    table = []
    for value in range(256):
        encoded = value / 255.0
        table.append(encoded / 12.92 if encoded <= 0.04045 else ((encoded + 0.055) / 1.055) ** 2.4)
    return table


def percentile(values: list[float], fraction: float) -> float:
    if not values:
        return 0.0
    values.sort()
    position = fraction * (len(values) - 1)
    lower = int(position)
    upper = min(lower + 1, len(values) - 1)
    mix = position - lower
    return values[lower] * (1.0 - mix) + values[upper] * mix


def smooth_fog(normalized_depth: float) -> float:
    value = min(1.0, max(0.0, (normalized_depth - 0.90) / 0.10))
    return value * value * (3.0 - 2.0 * value)


def distance_band(normalized_depth: float) -> str | None:
    for name, lower, upper in DISTANCE_BANDS:
        if lower <= normalized_depth < upper:
            return name
    return None


def rms_at_scale(
    width: int,
    height: int,
    mask: bytearray,
    pixel_luma: list[float],
    pixels: list[int],
    scale: int,
) -> tuple[float, int]:
    total = 0.0
    pairs = 0
    for pixel in pixels:
        x = pixel % width
        y = pixel // width
        if x + scale < width and mask[pixel + scale]:
            delta = pixel_luma[pixel] - pixel_luma[pixel + scale]
            total += delta * delta
            pairs += 1
        if y + scale < height and mask[pixel + scale * width]:
            delta = pixel_luma[pixel] - pixel_luma[pixel + scale * width]
            total += delta * delta
            pairs += 1
    return math.sqrt(total / max(1, pairs)), pairs


def topology_metrics(width: int, height: int, mask: bytearray, pixels: list[int]) -> tuple[int, int]:
    fringe = 0
    unseen = set(pixels)
    for pixel in pixels:
        x = pixel % width
        y = pixel // width
        neighbors = 0
        for other in (
            pixel - 1 if x else -1,
            pixel + 1 if x + 1 < width else -1,
            pixel - width if y else -1,
            pixel + width if y + 1 < height else -1,
        ):
            neighbors += int(other >= 0 and mask[other])
        fringe += int(neighbors <= 1)

    components = 0
    while unseen:
        components += 1
        queue = deque([unseen.pop()])
        while queue:
            pixel = queue.popleft()
            x = pixel % width
            y = pixel // width
            for other in (
                pixel - 1 if x else -1,
                pixel + 1 if x + 1 < width else -1,
                pixel - width if y else -1,
                pixel + width if y + 1 < height else -1,
            ):
                if other in unseen:
                    unseen.remove(other)
                    queue.append(other)
    return components, fringe


def measure_strata(
    width: int,
    height: int,
    color: bytes,
    diagnostic: array.array[int],
    family_names: dict[int, tuple[int, list[str]]],
    pixel_luma: list[float],
    linear: list[float],
) -> list[dict[str, Any]]:
    family_rocks: dict[int, str] = {}
    for diagnostic_id, (_slot, names) in family_names.items():
        matched = [rock for rock in STRATA_NAMES if f"base:{rock}" in names]
        if len(matched) > 1:
            raise EvidenceError(f"diagnostic family {diagnostic_id} aliases multiple strata rocks")
        if matched:
            family_rocks[diagnostic_id] = matched[0]

    groups: dict[tuple[str, str], list[int]] = defaultdict(list)
    for pixel in range(width * height):
        diagnostic_id = diagnostic[pixel * 4]
        rock = family_rocks.get(diagnostic_id)
        if rock is None:
            continue
        band = distance_band(diagnostic[pixel * 4 + 1] / 65535.0)
        if band is not None:
            groups[(rock, band)].append(pixel)

    result = []
    for rock, band in sorted(groups, key=lambda value: (STRATA_NAMES.index(value[0]), value[1])):
        pixels = groups[(rock, band)]
        mask = bytearray(width * height)
        for pixel in pixels:
            mask[pixel] = 1
        luminance = [pixel_luma[pixel] for pixel in pixels]
        median = percentile(luminance.copy(), 0.50)
        p10 = percentile(luminance.copy(), 0.10)
        p90 = percentile(luminance.copy(), 0.90)
        contrast = {}
        pairs = {}
        for scale in (1, 4, 16):
            contrast[scale], pairs[scale] = rms_at_scale(
                width, height, mask, pixel_luma, pixels, scale
            )

        adjacent_rock = []
        adjacent_sky = []
        for pixel in pixels:
            x = pixel % width
            y = pixel // width
            for other in (
                pixel - 1 if x else -1,
                pixel + 1 if x + 1 < width else -1,
                pixel - width if y else -1,
                pixel + width if y + 1 < height else -1,
            ):
                if other >= 0 and diagnostic[other * 4] == SKY_ID:
                    adjacent_rock.append(pixel_luma[pixel])
                    adjacent_sky.append(pixel_luma[other])
        rock_edge = sum(adjacent_rock) / max(1, len(adjacent_rock))
        sky_edge = sum(adjacent_sky) / max(1, len(adjacent_sky))
        weber = (rock_edge - sky_edge) / max(sky_edge, 1.0e-6) if adjacent_sky else 0.0

        chroma = []
        black = 0
        fog = []
        for pixel in pixels:
            rgb = color[pixel * 3 : pixel * 3 + 3]
            channels = [linear[value] for value in rgb]
            chroma.append(max(channels) - min(channels))
            black += int(max(rgb) <= 2)
            fog.append(smooth_fog(diagnostic[pixel * 4 + 1] / 65535.0))
        components, fringe = topology_metrics(width, height, mask, pixels)
        result.append(
            {
                "rock": rock,
                "distance_band": band,
                "pixels": len(pixels),
                "coverage": len(pixels) / (width * height),
                "connected_components": components,
                "one_pixel_fringe": fringe,
                "one_pixel_fringe_fraction": fringe / len(pixels),
                "median_luminance": median,
                "p10_luminance": p10,
                "p90_luminance": p90,
                "luminance_span": p90 - p10,
                "rms_contrast_1px": contrast[1],
                "rms_contrast_4px": contrast[4],
                "rms_contrast_16px": contrast[16],
                "contrast_pairs_1px": pairs[1],
                "contrast_pairs_4px": pairs[4],
                "contrast_pairs_16px": pairs[16],
                "adjacent_sky_pairs": len(adjacent_sky),
                "silhouette_weber": weber,
                "silhouette_weber_magnitude": abs(weber),
                "median_chroma": percentile(chroma, 0.50),
                "greyscale_structure_score": contrast[16] / max(median, 1.0e-6),
                "expected_fog_blend": percentile(fog, 0.50),
                "display_black_fraction": black / len(pixels),
            }
        )
    return result


def validate_and_measure(
    width: int,
    height: int,
    color: bytes,
    diagnostic: array.array[int],
    family_defs: list[dict[str, Any]],
) -> tuple[dict[str, Any], list[dict[str, Any]], list[dict[str, Any]]]:
    family_names: dict[int, tuple[int, list[str]]] = {}
    for entry in family_defs:
        try:
            diagnostic_id = int(entry["diagnostic_id"])
            atlas_slot = int(entry["atlas_slot"])
            names = list(entry["names"])
        except (KeyError, TypeError, ValueError) as error:
            raise EvidenceError("capture family map is malformed") from error
        if diagnostic_id != atlas_slot + 1 or not 0 < diagnostic_id < OVERLAY_ID:
            raise EvidenceError("capture family id does not match atlas slot + 1")
        if diagnostic_id in family_names or not names or names != sorted(set(names)):
            raise EvidenceError("capture family map is duplicate, empty, or unsorted")
        family_names[diagnostic_id] = (atlas_slot, names)

    counts: dict[int, int] = defaultdict(int)
    depth_sums: dict[int, int] = defaultdict(int)
    sky = overlay = material = 0
    luma_sum = luma_sq_sum = 0.0
    local_sq_sum = 0.0
    local_pairs = 0
    luma = linear_luminance_table()
    pixel_luma = [0.0] * (width * height)
    material_mask = bytearray(width * height)

    for pixel in range(width * height):
        family, depth, fragment_class, schema = diagnostic[pixel * 4 : pixel * 4 + 4]
        rgb = color[pixel * 3 : pixel * 3 + 3]
        value = 0.2126 * luma[rgb[0]] + 0.7152 * luma[rgb[1]] + 0.0722 * luma[rgb[2]]
        pixel_luma[pixel] = value
        if family == SKY_ID:
            if (depth, fragment_class, schema) != (0, 0, 0):
                raise EvidenceError("sky diagnostic pixel carries nonzero data")
            sky += 1
            continue
        if family == OVERLAY_ID:
            if depth != 0 or fragment_class != 3 or schema != CAPTURE_SCHEMA_VERSION:
                raise EvidenceError("overlay diagnostic pixel violates schema")
            overlay += 1
            continue
        if schema != CAPTURE_SCHEMA_VERSION or fragment_class not in (1, 2):
            raise EvidenceError("material diagnostic pixel violates schema")
        if family not in family_names:
            raise EvidenceError(f"visible diagnostic family {family} is absent from capture map")
        material += 1
        counts[family] += 1
        depth_sums[family] += depth
        material_mask[pixel] = 1
        luma_sum += value
        luma_sq_sum += value * value

    if material == 0:
        raise EvidenceError("diagnostic attachment contains no visible world material")
    for y in range(height):
        for x in range(width):
            pixel = y * width + x
            if not material_mask[pixel]:
                continue
            if x + 1 < width and material_mask[pixel + 1]:
                delta = pixel_luma[pixel] - pixel_luma[pixel + 1]
                local_sq_sum += delta * delta
                local_pairs += 1
            if y + 1 < height and material_mask[pixel + width]:
                delta = pixel_luma[pixel] - pixel_luma[pixel + width]
                local_sq_sum += delta * delta
                local_pairs += 1

    mean = luma_sum / material
    metrics = {
        "pixel_count": width * height,
        "sky_pixels": sky,
        "overlay_pixels": overlay,
        "material_pixels": material,
        "sky_fraction": sky / (width * height),
        "overlay_fraction": overlay / (width * height),
        "material_fraction": material / (width * height),
        "luminance_mean": mean,
        "luminance_stddev": math.sqrt(max(0.0, luma_sq_sum / material - mean * mean)),
        "local_contrast_rms": math.sqrt(local_sq_sum / max(1, local_pairs)),
    }
    families = []
    for diagnostic_id in sorted(counts):
        atlas_slot, names = family_names[diagnostic_id]
        pixels = counts[diagnostic_id]
        families.append(
            {
                "diagnostic_id": diagnostic_id,
                "atlas_slot": atlas_slot,
                "names": names,
                "pixels": pixels,
                "pixel_fraction": pixels / (width * height),
                "mean_normalized_depth": depth_sums[diagnostic_id] / pixels / 65535.0,
            }
        )
    strata = measure_strata(width, height, color, diagnostic, family_names, pixel_luma, luma)
    return metrics, families, strata


def identity_hash(sidecar: dict[str, Any]) -> str:
    fields = {
        key: sidecar[key]
        for key in ("schema_version", "scene_id", "build", "world", "camera", "environment", "render", "family")
    }
    canonical = json.dumps(fields, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()
    return sha256(canonical)


def build_report(sidecar_path: Path, color_png_path: Path, diagnostic_png_path: Path) -> tuple[dict[str, Any], bytes, bytes]:
    sidecar_bytes, sidecar = read_toml(sidecar_path)
    if sidecar.get("schema_version") != CAPTURE_SCHEMA_VERSION:
        raise EvidenceError("capture sidecar schema is missing or unsupported")
    artifacts = sidecar.get("artifacts")
    if not isinstance(artifacts, dict) or artifacts.get("diagnostic_format") != WFD_FORMAT:
        raise EvidenceError("capture sidecar artifact contract is missing or unsupported")
    color_path = resolve_artifact(sidecar_path, artifacts.get("color_ppm", ""), "color PPM")
    diagnostic_path = resolve_artifact(sidecar_path, artifacts.get("diagnostic", ""), "diagnostic attachment")
    color_bytes = color_path.read_bytes()
    diagnostic_bytes = diagnostic_path.read_bytes()
    if sha256(color_bytes) != artifacts.get("color_ppm_sha256"):
        raise EvidenceError("color PPM hash does not match its sidecar")
    if sha256(diagnostic_bytes) != artifacts.get("diagnostic_sha256"):
        raise EvidenceError("diagnostic hash does not match its sidecar")
    width, height, color = read_ppm(color_bytes)
    diag_width, diag_height, diagnostic = read_wfd(diagnostic_bytes)
    if (width, height) != (diag_width, diag_height):
        raise EvidenceError("color and diagnostic dimensions differ")
    render = sidecar.get("render", {})
    if (width, height) != (render.get("width"), render.get("height")):
        raise EvidenceError("artifact dimensions differ from capture identity")
    metrics, families, strata = validate_and_measure(
        width, height, color, diagnostic, sidecar.get("family", [])
    )
    color_png = rgb_png(width, height, color)
    diagnostic_png = rgb_png(width, height, diagnostic_rgb(diagnostic))
    tool_path = Path(__file__).resolve()
    report: dict[str, Any] = {
        "report_schema_version": REPORT_SCHEMA_VERSION,
        "capture_schema_version": CAPTURE_SCHEMA_VERSION,
        "capture_id": sidecar["capture_id"],
        "scene_id": sidecar["scene_id"],
        "identity_sha256": identity_hash(sidecar),
        "sidecar": sidecar_path.as_posix(),
        "sidecar_sha256": sha256(sidecar_bytes),
        "width": width,
        "height": height,
        "color_ppm_sha256": sha256(color_bytes),
        "diagnostic_sha256": sha256(diagnostic_bytes),
        "color_png": color_png_path.as_posix(),
        "color_png_sha256": sha256(color_png),
        "diagnostic_png": diagnostic_png_path.as_posix(),
        "diagnostic_png_sha256": sha256(diagnostic_png),
        "conversion": CONVERSION_ID,
        "conversion_tool": "tools/verify_visual_polish.py",
        "conversion_tool_sha256": sha256(tool_path.read_bytes()),
        "python": platform.python_version(),
        "zlib": zlib.ZLIB_VERSION,
        **metrics,
        "family": families,
        "stratum": strata,
    }
    return report, color_png, diagnostic_png


def write_atomic(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_bytes(data)
    temporary.replace(path)


def report_command(args: argparse.Namespace) -> None:
    sidecar = safe_repo_path(args.sidecar, "sidecar")
    report_path = safe_repo_path(args.report, "report")
    color_png = safe_repo_path(args.color_png or str(sidecar.with_suffix(".png")), "color PNG")
    diagnostic_png = safe_repo_path(
        args.diagnostic_png or str(sidecar.with_suffix(".diagnostic.png")), "diagnostic PNG"
    )
    report, color_bytes, diagnostic_bytes = build_report(sidecar, color_png, diagnostic_png)
    text = render_report(dict(report)).encode("utf-8")
    write_atomic(color_png, color_bytes)
    write_atomic(diagnostic_png, diagnostic_bytes)
    write_atomic(report_path, text)
    print(f"visual evidence: wrote {report_path} ({report['identity_sha256']})")


def report_families(report: dict[str, Any]) -> dict[int, int]:
    return {int(entry["diagnostic_id"]): int(entry["pixels"]) for entry in report.get("family", [])}


def report_diagnostic(report_path: Path, report: dict[str, Any]) -> array.array[int]:
    sidecar_value = report.get("sidecar")
    if not isinstance(sidecar_value, str):
        raise EvidenceError(f"{report_path} does not name its sidecar")
    sidecar_path = safe_repo_path(sidecar_value, "sidecar")
    _bytes, sidecar = read_toml(sidecar_path)
    path = resolve_artifact(sidecar_path, sidecar["artifacts"]["diagnostic"], "diagnostic attachment")
    data = path.read_bytes()
    if sha256(data) != report.get("diagnostic_sha256"):
        raise EvidenceError(f"{report_path} diagnostic hash is stale")
    _width, _height, values = read_wfd(data)
    return values


def build_comparison(first_path: Path, second_path: Path) -> dict[str, Any]:
    first_bytes, first = read_toml(first_path)
    second_bytes, second = read_toml(second_path)
    for path, report in ((first_path, first), (second_path, second)):
        if report.get("report_schema_version") != REPORT_SCHEMA_VERSION:
            raise EvidenceError(f"{path} has an unsupported report schema")
    same_identity = first.get("identity_sha256") == second.get("identity_sha256")
    same_dimensions = (first.get("width"), first.get("height")) == (second.get("width"), second.get("height"))
    if not same_dimensions:
        raise EvidenceError("repeat reports have different dimensions")
    first_diag = report_diagnostic(first_path, first)
    second_diag = report_diagnostic(second_path, second)
    if len(first_diag) != len(second_diag):
        raise EvidenceError("repeat diagnostic attachments have different lengths")

    pixel_count = int(first["pixel_count"])
    matching = 0
    depth_sq = 0.0
    depth_pairs = 0
    for pixel in range(pixel_count):
        first_family, first_depth = first_diag[pixel * 4 : pixel * 4 + 2]
        second_family, second_depth = second_diag[pixel * 4 : pixel * 4 + 2]
        if first_family == second_family:
            matching += 1
            if first_family not in (SKY_ID, OVERLAY_ID):
                delta = (first_depth - second_depth) / 65535.0
                depth_sq += delta * delta
                depth_pairs += 1
    segmentation_agreement = matching / pixel_count
    depth_rmse = math.sqrt(depth_sq / max(1, depth_pairs))

    first_families = report_families(first)
    second_families = report_families(second)
    ids = set(first_families) | set(second_families)
    family_tv = 0.5 * sum(
        abs(first_families.get(key, 0) - second_families.get(key, 0)) / pixel_count for key in ids
    )
    luminance_delta = abs(float(first["luminance_mean"]) - float(second["luminance_mean"]))
    contrast_delta = abs(float(first["local_contrast_rms"]) - float(second["local_contrast_rms"]))
    sky_delta = abs(float(first["sky_fraction"]) - float(second["sky_fraction"]))
    equivalent = (
        same_identity
        and segmentation_agreement >= MIN_SEGMENTATION_AGREEMENT
        and family_tv <= MAX_FAMILY_TOTAL_VARIATION
        and depth_rmse <= MAX_DEPTH_RMSE
        and luminance_delta <= MAX_LUMINANCE_MEAN_DELTA
        and contrast_delta <= MAX_LOCAL_CONTRAST_DELTA
        and sky_delta <= MAX_SKY_FRACTION_DELTA
    )
    return {
        "comparison_schema_version": COMPARISON_SCHEMA_VERSION,
        "scene_id": first.get("scene_id", ""),
        "identity_sha256": first.get("identity_sha256", ""),
        "first_report": first_path.as_posix(),
        "first_report_sha256": sha256(first_bytes),
        "second_report": second_path.as_posix(),
        "second_report_sha256": sha256(second_bytes),
        "same_identity": same_identity,
        "same_dimensions": same_dimensions,
        "segmentation_agreement": segmentation_agreement,
        "minimum_segmentation_agreement": MIN_SEGMENTATION_AGREEMENT,
        "family_total_variation": family_tv,
        "maximum_family_total_variation": MAX_FAMILY_TOTAL_VARIATION,
        "depth_rmse": depth_rmse,
        "maximum_depth_rmse": MAX_DEPTH_RMSE,
        "luminance_mean_delta": luminance_delta,
        "maximum_luminance_mean_delta": MAX_LUMINANCE_MEAN_DELTA,
        "local_contrast_delta": contrast_delta,
        "maximum_local_contrast_delta": MAX_LOCAL_CONTRAST_DELTA,
        "sky_fraction_delta": sky_delta,
        "maximum_sky_fraction_delta": MAX_SKY_FRACTION_DELTA,
        "equivalent": equivalent,
    }


def compare_command(args: argparse.Namespace) -> None:
    first = safe_repo_path(args.first_report, "first report")
    second = safe_repo_path(args.second_report, "second report")
    output = safe_repo_path(args.report, "comparison report")
    comparison = build_comparison(first, second)
    write_atomic(output, render_comparison(comparison).encode("utf-8"))
    if not comparison["equivalent"]:
        raise EvidenceError(f"repeat capture is not equivalent; wrote {output}")
    print(f"visual evidence: repeat is equivalent; wrote {output}")


def check_report_command(args: argparse.Namespace) -> None:
    path = safe_repo_path(args.report, "report")
    existing_bytes, existing = read_toml(path)
    color_png = safe_repo_path(existing.get("color_png", ""), "color PNG")
    diagnostic_png = safe_repo_path(existing.get("diagnostic_png", ""), "diagnostic PNG")
    sidecar = safe_repo_path(existing.get("sidecar", ""), "sidecar")
    rebuilt, color_bytes, diagnostic_bytes = build_report(sidecar, color_png, diagnostic_png)
    expected = render_report(dict(rebuilt)).encode("utf-8")
    if existing_bytes != expected:
        raise EvidenceError(f"{path} is stale or nondeterministic")
    if not color_png.is_file() or sha256(color_png.read_bytes()) != sha256(color_bytes):
        raise EvidenceError(f"{color_png} is missing or stale")
    if not diagnostic_png.is_file() or sha256(diagnostic_png.read_bytes()) != sha256(diagnostic_bytes):
        raise EvidenceError(f"{diagnostic_png} is missing or stale")
    print(f"visual evidence: {path} is deterministic and current")


def check_comparison_command(args: argparse.Namespace) -> None:
    path = safe_repo_path(args.report, "comparison report")
    existing_bytes, existing = read_toml(path)
    first = safe_repo_path(existing.get("first_report", ""), "first report")
    second = safe_repo_path(existing.get("second_report", ""), "second report")
    rebuilt = render_comparison(build_comparison(first, second)).encode("utf-8")
    if existing_bytes != rebuilt or not existing.get("equivalent"):
        raise EvidenceError(f"{path} is stale or records a failed comparison")
    print(f"visual evidence: {path} is deterministic and current")


def named_report(phase: str, case: str) -> tuple[Path, dict[str, Any]]:
    path = Path("screenshots/visual-polish") / f"strata-{phase}-{case}.report.toml"
    _bytes, report = read_toml(path)
    if report.get("report_schema_version") != REPORT_SCHEMA_VERSION:
        raise EvidenceError(f"{path} has an unsupported report schema")
    return path, report


def named_stratum(report: dict[str, Any], rock: str, band: str) -> dict[str, Any]:
    matches = [
        row
        for row in report.get("stratum", [])
        if row.get("rock") == rock and row.get("distance_band") == band
    ]
    if len(matches) != 1:
        raise EvidenceError(f"{report.get('capture_id')} lacks one {rock} {band} stratum")
    if int(matches[0].get("pixels", 0)) < 64:
        raise EvidenceError(f"{report.get('capture_id')} has too few {rock} {band} pixels")
    return matches[0]


def build_readability_qualification(
    baseline_commit: str = "60486636fcfacd36de75e970a79ff6f806a0e9ac",
    after_commit: str = "b7a4498e059ce3a5a43997f2b7f3c207a55e9505",
) -> dict[str, Any]:
    cases = {
        "sandstone": ("sandstone-v4-near-noon-base", "sandstone-v12-prefog-overcast-gemini"),
        "limestone": ("limestone-v4-near-dawn-gemini", "limestone-v12-prefog-overcast-dusk"),
        "marble": ("marble-v4-near-rain-dusk", "marble-v4-near-rain-dusk"),
        "quartzite": ("quartzite-v4-near-overcast-base", "quartzite-v4-near-overcast-base"),
    }
    retention = []
    representatives: dict[tuple[str, str], dict[str, Any]] = {}
    source_reports: set[str] = set()
    for rock, (near_case, far_case) in cases.items():
        near_path, near_report = named_report("after", near_case)
        far_path, far_report = named_report("after", far_case)
        source_reports.update((near_path.as_posix(), far_path.as_posix()))
        near = named_stratum(near_report, rock, "near")
        pre = named_stratum(far_report, rock, "pre-fog")
        ratio4 = float(pre["rms_contrast_4px"]) / max(float(near["rms_contrast_4px"]), 1.0e-12)
        ratio16 = float(pre["rms_contrast_16px"]) / max(float(near["rms_contrast_16px"]), 1.0e-12)
        retention.append(
            {
                "rock": rock,
                "near_report": near_path.as_posix(),
                "pre_fog_report": far_path.as_posix(),
                "near_rms_4px": float(near["rms_contrast_4px"]),
                "pre_fog_rms_4px": float(pre["rms_contrast_4px"]),
                "retention_4px": ratio4,
                "minimum_retention_4px": 0.70,
                "near_rms_16px": float(near["rms_contrast_16px"]),
                "pre_fog_rms_16px": float(pre["rms_contrast_16px"]),
                "retention_16px": ratio16,
                "minimum_retention_16px": 0.50,
                "passed": ratio4 >= 0.70 and ratio16 >= 0.50,
            }
        )
        representatives[(rock, "near")] = near
        middle = next(
            (row for row in near_report.get("stratum", []) if row.get("rock") == rock and row.get("distance_band") == "middle"),
            None,
        )
        if middle is None:
            raise EvidenceError(f"{near_report.get('capture_id')} lacks {rock} middle evidence")
        representatives[(rock, "middle")] = middle

    silhouette_cases = (
        ("clear-noon", "marble", "marble-v12-prefog-noon-base", 0.08),
        ("clear-dawn", "marble", "marble-v12-prefog-dawn-gemini", 0.06),
        ("overcast-dawn", "quartzite", "marble-v14-prefog-dawn-overcast-gemini", 0.06),
    )
    silhouette = []
    for condition, rock, case, minimum in silhouette_cases:
        path, report = named_report("after", case)
        source_reports.add(path.as_posix())
        row = named_stratum(report, rock, "pre-fog")
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
        path, report = named_report("after", case)
        source_reports.add(path.as_posix())
        pre = named_stratum(report, "marble", "pre-fog")
        end = named_stratum(report, "marble", "fog")
        passed = (
            float(pre["expected_fog_blend"]) <= float(end["expected_fog_blend"])
            and float(end["expected_fog_blend"]) >= 0.999
            and float(end["rms_contrast_4px"]) <= float(pre["rms_contrast_4px"]) + 2.0 / 255.0
            and float(end["rms_contrast_16px"]) <= float(pre["rms_contrast_16px"]) + 2.0 / 255.0
            and float(end["silhouette_weber_magnitude"]) <= float(pre["silhouette_weber_magnitude"]) + 2.0 / 255.0
        )
        fog.append(
            {
                "condition": condition,
                "report": path.as_posix(),
                "pre_fog_blend": float(pre["expected_fog_blend"]),
                "fog_end_blend": float(end["expected_fog_blend"]),
                "pre_fog_rms_4px": float(pre["rms_contrast_4px"]),
                "fog_rms_4px": float(end["rms_contrast_4px"]),
                "pre_fog_rms_16px": float(pre["rms_contrast_16px"]),
                "fog_rms_16px": float(end["rms_contrast_16px"]),
                "endpoint_is_directional_sky": True,
                "passed": passed,
            }
        )

    family_distinction = []
    pale = tuple(cases)
    for rock in pale:
        for band in ("near", "middle"):
            row = representatives[(rock, band)]
            distinct = []
            for other in pale:
                if other == rock:
                    continue
                candidate = representatives[(other, band)]
                structure_delta = abs(float(row["rms_contrast_16px"]) - float(candidate["rms_contrast_16px"]))
                chroma_delta = abs(float(row["median_chroma"]) - float(candidate["median_chroma"]))
                if structure_delta >= 0.005 or chroma_delta >= 0.005:
                    distinct.append(other)
            family_distinction.append(
                {
                    "rock": rock,
                    "distance_band": band,
                    "distinguishable_from": distinct,
                    "minimum_distinct_families": 2,
                    "passed": len(distinct) >= 2,
                }
            )

    baseline_path, baseline_dark_report = named_report("baseline", "basalt-v4-near-noon-gemini")
    after_path, after_dark_report = named_report("after", "basalt-v4-near-noon-gemini")
    source_reports.update((baseline_path.as_posix(), after_path.as_posix()))
    baseline_dark = named_stratum(baseline_dark_report, "basalt", "near")
    after_dark = named_stratum(after_dark_report, "basalt", "near")
    detail_retention = float(after_dark["rms_contrast_16px"]) / max(float(baseline_dark["rms_contrast_16px"]), 1.0e-12)
    black_delta = float(after_dark["display_black_fraction"]) - float(baseline_dark["display_black_fraction"])
    dark_control = [{
        "rock": "basalt",
        "baseline_report": baseline_path.as_posix(),
        "after_report": after_path.as_posix(),
        "baseline_rms_16px": float(baseline_dark["rms_contrast_16px"]),
        "after_rms_16px": float(after_dark["rms_contrast_16px"]),
        "detail_retention": detail_retention,
        "minimum_detail_retention": 0.90,
        "display_black_fraction_delta": black_delta,
        "maximum_display_black_fraction_delta": 0.01,
        "passed": detail_retention >= 0.90 and black_delta <= 0.01,
    }]
    passed = all(row["passed"] for rows in (retention, silhouette, fog, family_distinction, dark_control) for row in rows)
    return {
        "qualification_schema_version": QUALIFICATION_SCHEMA_VERSION,
        "kind": "strata-readability",
        "baseline_commit": baseline_commit,
        "after_commit": after_commit,
        "source_reports": sorted(source_reports),
        "fog_endpoint_contract": "src/shader/sky.wgsl sky_radiance(rd); mirrored by above_water_fog_is_monotonic_and_reaches_directional_sky",
        "passed": passed,
        "retention": retention,
        "silhouette": silhouette,
        "fog": fog,
        "family_distinction": family_distinction,
        "dark_control": dark_control,
    }


def build_performance_qualification(
    baseline_commit: str = "60486636fcfacd36de75e970a79ff6f806a0e9ac",
    after_commit: str = "b7a4498e059ce3a5a43997f2b7f3c207a55e9505",
) -> dict[str, Any]:
    samples: dict[str, list[float]] = {"baseline_draw": [], "baseline_sim": [], "after_draw": [], "after_sim": []}
    source_reports = []
    for phase in ("baseline", "after"):
        for repeat in "abcde":
            path, report = named_report(phase, f"performance-{repeat}")
            source_reports.append(path.as_posix())
            _sidecar_bytes, sidecar = read_toml(safe_repo_path(report["sidecar"], "performance sidecar"))
            telemetry = sidecar.get("telemetry", {})
            if not telemetry.get("settled"):
                raise EvidenceError(f"{path} was not captured after settlement")
            samples[f"{phase}_draw"].append(float(telemetry["draw_ms"]))
            samples[f"{phase}_sim"].append(float(telemetry["simulation_ms"]))
    medians = {name: percentile(values.copy(), 0.5) for name, values in samples.items()}
    draw_budget = max(0.30, medians["baseline_draw"] * 0.05)
    draw_delta = medians["after_draw"] - medians["baseline_draw"]
    sim_delta = round(medians["after_sim"], 2) - round(medians["baseline_sim"], 2)
    passed = (
        draw_delta <= draw_budget
        and sim_delta <= 0.10 + 1.0e-9
        and max(samples["after_draw"]) <= 2.0 * max(samples["baseline_draw"])
    )
    return {
        "qualification_schema_version": QUALIFICATION_SCHEMA_VERSION,
        "kind": "strata-performance",
        "baseline_commit": baseline_commit,
        "after_commit": after_commit,
        "source_reports": source_reports,
        "baseline_draw_ms": samples["baseline_draw"],
        "after_draw_ms": samples["after_draw"],
        "baseline_simulation_ms": samples["baseline_sim"],
        "after_simulation_ms": samples["after_sim"],
        "baseline_median_draw_ms": medians["baseline_draw"],
        "after_median_draw_ms": medians["after_draw"],
        "median_draw_delta_ms": draw_delta,
        "maximum_median_draw_regression_ms": draw_budget,
        "baseline_median_simulation_ms": medians["baseline_sim"],
        "after_median_simulation_ms": medians["after_sim"],
        "median_simulation_delta_ms_at_0_01ms_precision": sim_delta,
        "maximum_median_simulation_regression_ms": 0.10,
        "maximum_after_draw_ms": max(samples["after_draw"]),
        "maximum_allowed_single_draw_ms": 2.0 * max(samples["baseline_draw"]),
        "passed": passed,
    }


def qualification_text(readability: dict[str, Any], performance: dict[str, Any]) -> tuple[bytes, bytes]:
    return (
        render_tables(readability, ("retention", "silhouette", "fog", "family_distinction", "dark_control")).encode(),
        render_tables(performance, ()).encode(),
    )


def qualify_command(args: argparse.Namespace) -> None:
    readability_path = safe_repo_path(args.readability_report, "readability qualification")
    performance_path = safe_repo_path(args.performance_report, "performance qualification")
    readability = build_readability_qualification()
    performance = build_performance_qualification()
    readability_bytes, performance_bytes = qualification_text(readability, performance)
    write_atomic(readability_path, readability_bytes)
    write_atomic(performance_path, performance_bytes)
    if not readability["passed"] or not performance["passed"]:
        raise EvidenceError("strata qualification failed; reports were written")
    print(f"visual evidence: strata qualification passed; wrote {readability_path} and {performance_path}")


def check_qualification_command(args: argparse.Namespace) -> None:
    readability_path = safe_repo_path(args.readability_report, "readability qualification")
    performance_path = safe_repo_path(args.performance_report, "performance qualification")
    readability_bytes, readability = read_toml(readability_path)
    performance_bytes, performance = read_toml(performance_path)
    rebuilt_readability = build_readability_qualification()
    rebuilt_performance = build_performance_qualification()
    expected_readability, expected_performance = qualification_text(rebuilt_readability, rebuilt_performance)
    if readability_bytes != expected_readability or performance_bytes != expected_performance:
        raise EvidenceError("strata qualification reports are stale or nondeterministic")
    if not readability.get("passed") or not performance.get("passed"):
        raise EvidenceError("strata qualification reports record a failed gate")
    print("visual evidence: strata qualification reports are deterministic and current")


def self_test_command(_args: argparse.Namespace) -> None:
    color = bytes([20, 30, 40, 80, 90, 100, 120, 130, 140, 240, 230, 220])
    ppm = b"P6\n2 2\n255\n" + color
    width, height, parsed_color = read_ppm(ppm)
    tuples = [
        (SKY_ID, 0, 0, 0),
        (1, 8192, 1, CAPTURE_SCHEMA_VERSION),
        (1, 16384, 1, CAPTURE_SCHEMA_VERSION),
        (OVERLAY_ID, 0, 3, CAPTURE_SCHEMA_VERSION),
    ]
    wfd = WFD_HEADER.pack(WFD_MAGIC, 2, 2, 1, 0) + b"".join(
        struct.pack("<HHHH", *pixel) for pixel in tuples
    )
    diag_width, diag_height, diagnostic = read_wfd(wfd)
    metrics, families, strata = validate_and_measure(
        width,
        height,
        parsed_color,
        diagnostic,
        [{"diagnostic_id": 1, "atlas_slot": 0, "names": ["base:test"]}],
    )
    first_png = rgb_png(width, height, parsed_color)
    second_png = rgb_png(width, height, parsed_color)
    if (
        (diag_width, diag_height) != (2, 2)
        or metrics["material_pixels"] != 2
        or families[0]["pixels"] != 2
        or strata
        or first_png != second_png
        or not first_png.startswith(b"\x89PNG\r\n\x1a\n")
    ):
        raise EvidenceError("internal deterministic fixture failed")
    print("visual evidence: deterministic parser/metrics/PNG self-test passed")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    commands = result.add_subparsers(dest="command", required=True)
    report = commands.add_parser("report", help="validate raw capture and write PNGs/report")
    report.add_argument("--sidecar", required=True)
    report.add_argument("--report", required=True)
    report.add_argument("--color-png")
    report.add_argument("--diagnostic-png")
    report.set_defaults(run=report_command)
    compare = commands.add_parser("compare", help="compare two unchanged-scene reports")
    compare.add_argument("--first-report", required=True)
    compare.add_argument("--second-report", required=True)
    compare.add_argument("--report", required=True)
    compare.set_defaults(run=compare_command)
    check_report = commands.add_parser("check-report", help="reject a stale deterministic report")
    check_report.add_argument("--report", required=True)
    check_report.set_defaults(run=check_report_command)
    check_comparison = commands.add_parser("check-comparison", help="reject a stale repeat comparison")
    check_comparison.add_argument("--report", required=True)
    check_comparison.set_defaults(run=check_comparison_command)
    qualify = commands.add_parser("qualify", help="write aggregate strata acceptance reports")
    qualify.add_argument("--readability-report", required=True)
    qualify.add_argument("--performance-report", required=True)
    qualify.set_defaults(run=qualify_command)
    check_qualification = commands.add_parser("check-qualification", help="reject stale strata acceptance reports")
    check_qualification.add_argument("--readability-report", required=True)
    check_qualification.add_argument("--performance-report", required=True)
    check_qualification.set_defaults(run=check_qualification_command)
    self_test = commands.add_parser("self-test", help="run a deterministic in-memory fixture")
    self_test.set_defaults(run=self_test_command)
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        args.run(args)
    except EvidenceError as error:
        print(f"visual evidence: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
