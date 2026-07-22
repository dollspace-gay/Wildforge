#!/usr/bin/env python3
"""Split a multi-panel material contact sheet into Wildforge pack tiles.

Image models asked for "albedo + height + normal" tend to answer with a single
image holding the three maps side by side. This turns one of those sheets into
the companion-map trio a texture pack wants:

    <name>.png     albedo        (sRGB, the tile itself)
    <name>_h.png   height        (greyscale; 255 = surface, 0 = deepest)
    <name>_n.png   normal        (tangent space, OpenGL / +Y "green up")

A panel usually holds a *grid* of separate tiles rather than one texture, so
`--grid COLSxROWS` cuts it up and `--tile NAME=COL,ROW` says which cell becomes
which tile. Cells are numbered from the top left.

Three things the sheets get wrong, all handled here:

* **Nothing lands on a clean pixel boundary.** Generated sheets drift a few
  pixels off exact fractions. Panel seams are found from column-difference
  peaks, and the grid origin is found by sliding the cut lines to wherever the
  image changes most — for masonry that is the mortar, which is exactly where
  you want to cut, since a tile bounded by mortar meets itself invisibly.
* **The panel is rarely square.** A voxel tile is, so a `1x1` grid must crop.
  `--crop best` scans every offset for the window whose top row best continues
  from its bottom row, and reports the seam against a shuffled baseline so you
  can see whether the crop actually tiles.
* **The normal map is rarely a unit field.** Models hallucinate normals rather
  than deriving them, so lengths drift and some texels even face backwards.
  We renormalize, clamp z to the front hemisphere, and — when a height panel is
  present — check the encoded handedness against the height gradient and say so.

Usage:

    tools/split_material_sheet.py sheet.png --out packs/hewn/tiles \\
        --grid 2x6 --tile stone=1,0 --tile cobblestone=0,3
"""

import argparse
import pathlib
import sys

import numpy as np
from PIL import Image

# A normal map is OpenGL convention when green brightens toward the *top* of the
# image, i.e. n.y tracks +dh/dy with y counted downward in image rows. Wildforge
# stores tiles top-row-first and its tangent bitangent runs down the image, so
# the shader negates green on decode; an OpenGL sheet drops in unflipped.
OPENGL = "opengl"
DIRECTX = "directx"


def find_panels(img: np.ndarray, count: int) -> list[tuple[int, int]]:
    """Locate `count` vertical panels by their strongest column seams."""
    w = img.shape[1]
    if count == 1:
        return [(0, w)]
    diff = np.abs(np.diff(img.astype(np.int16), axis=1)).mean(axis=(0, 2))
    # Candidate cuts near each expected boundary; a generated sheet drifts a
    # little but never reorders, so a local search beats a global peak pick.
    cuts = []
    span = max(4, w // (count * 20))
    for k in range(1, count):
        guess = round(w * k / count)
        lo, hi = max(1, guess - span), min(w - 1, guess + span)
        # diff[i] measures the step between columns i and i+1, so the panel
        # starts at i+1.
        cuts.append(lo + int(np.argmax(diff[lo:hi])) + 1)
    bounds = [0, *cuts, w]
    return [(bounds[i], bounds[i + 1]) for i in range(count)]


def _row_mad(a: np.ndarray, b: np.ndarray) -> float:
    return float(np.abs(a.astype(float) - b.astype(float)).mean())


def pick_crop(panel: np.ndarray, size: int, mode: str) -> tuple[int, float, float]:
    """Return (y offset, seam score, baseline) for a `size`-tall square window.

    Score is the mean absolute difference between the window's first and last
    row: how badly the tile fails to meet itself when stacked. The baseline is
    the same measure over far-apart rows, i.e. what "no relationship at all"
    looks like for this texture. seam << baseline means the crop tiles.
    """
    h = panel.shape[0]
    baseline = float(
        np.mean([_row_mad(panel[i], panel[(i + h // 2) % h]) for i in range(0, h, 7)])
    )
    if mode == "top":
        y = 0
    elif mode == "center":
        y = max(0, (h - size) // 2)
    elif mode == "bottom":
        y = max(0, h - size)
    elif mode == "best":
        y = min(range(max(1, h - size + 1)), key=lambda o: _row_mad(panel[o], panel[o + size - 1]))
    else:
        raise ValueError(f"unknown crop mode {mode}")
    return y, _row_mad(panel[y], panel[y + size - 1]), baseline


def find_grid_origin(panel: np.ndarray, cols: int, rows: int) -> tuple[int, int]:
    """Slide the cut lines to where the image changes most.

    A sheet of separate tiles is drawn with something between them — mortar, a
    gutter, a dark rule. That divider is both the strongest gradient in the
    image and the correct place to cut: a tile whose border *is* mortar butts
    against itself without a visible seam even when the pixels don't match.
    """
    h, w = panel.shape[:2]
    ph, pw = h / rows, w / cols
    rd = np.abs(np.diff(panel.astype(np.int16), axis=0)).mean(axis=(1, 2))
    cd = np.abs(np.diff(panel.astype(np.int16), axis=1)).mean(axis=(0, 2))

    def energy(profile: np.ndarray, pitch: float, count: int, off: int) -> float:
        return sum(profile[int(round(k * pitch + off)) % len(profile)] for k in range(count))

    oy = max(range(int(ph)), key=lambda o: energy(rd, ph, rows, o))
    ox = max(range(int(pw)), key=lambda o: energy(cd, pw, cols, o))
    return oy, ox


def cut_cell(panel: np.ndarray, col: int, row: int, cols: int, rows: int, oy: int, ox: int):
    """Extract one grid cell, wrapping — the origin shift pushes the last cell
    past the edge, and the sheet as a whole tiles, so wrapping is well defined."""
    h, w = panel.shape[:2]
    ph, pw = h / rows, w / cols
    y0, x0 = int(round(row * ph + oy)), int(round(col * pw + ox))
    ys = [(y0 + k) % h for k in range(int(ph))]
    xs = [(x0 + k) % w for k in range(int(pw))]
    return panel[np.ix_(ys, xs)]


def detect_handedness(normal: np.ndarray, height: np.ndarray) -> tuple[str, float]:
    """Correlate the encoded green against the height gradient.

    Returns (convention, |correlation|). A generated normal map is only loosely
    tied to its own height map, so the magnitude is usually modest — it is the
    *sign* that decides the convention, and the magnitude that says how much to
    trust it.
    """
    h = height.astype(float).mean(axis=2) / 255.0
    ny = normal[:, :, 1].astype(float) / 255.0 * 2 - 1
    dh_dy = np.gradient(h, axis=0)  # y counted downward in image rows
    corr = float(np.corrcoef(dh_dy.ravel(), ny.ravel())[0, 1])
    return (OPENGL if corr > 0 else DIRECTX), abs(corr)


def resample(panel: np.ndarray, size: int, area: bool) -> np.ndarray:
    """Box-average when downscaling (keeps detail out of the aliasing bin)."""
    im = Image.fromarray(panel)
    return np.asarray(im.resize((size, size), Image.BOX if area else Image.NEAREST))


def clean_normal(n: np.ndarray, flip_green: bool) -> np.ndarray:
    """Decode, optionally flip green, force a unit front-facing field, re-encode."""
    v = n.astype(np.float32) / 255.0 * 2 - 1
    if flip_green:
        v[:, :, 1] = -v[:, :, 1]
    # A hallucinated map can point *into* the surface; that is never meaningful
    # tangent-space data, so fold it back to the front hemisphere before
    # normalizing rather than letting it flip a lit face inside out.
    v[:, :, 2] = np.maximum(v[:, :, 2], 0.05)
    v /= np.maximum(np.linalg.norm(v, axis=2, keepdims=True), 1e-6)
    return np.clip((v + 1) * 0.5 * 255.0 + 0.5, 0, 255).astype(np.uint8)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("sheet", type=pathlib.Path)
    ap.add_argument("--out", type=pathlib.Path, required=True, help="pack tiles/ directory")
    ap.add_argument("--name", help="single tile name; shorthand for --tile NAME=0,0")
    ap.add_argument(
        "--tile",
        action="append",
        default=[],
        metavar="NAME=COL,ROW",
        help="emit grid cell (COL,ROW) as tile NAME; repeatable",
    )
    ap.add_argument("--grid", default="1x1", metavar="COLSxROWS", help="tiles per panel (default 1x1)")
    ap.add_argument(
        "--layout",
        default="albedo,height,normal",
        help="comma-separated panel order, left to right (default: albedo,height,normal)",
    )
    ap.add_argument("--size", type=int, default=128, help="output tile size in px (default 128)")
    ap.add_argument("--crop", default="best", choices=["best", "top", "center", "bottom"])
    ap.add_argument("--flip-green", action="store_true", help="force a DirectX-convention flip")
    ap.add_argument("--no-flip", action="store_true", help="never flip, whatever detection says")
    args = ap.parse_args()

    try:
        cols, rows = (int(v) for v in args.grid.lower().split("x"))
    except ValueError:
        print(f"--grid wants COLSxROWS, got {args.grid!r}", file=sys.stderr)
        return 2
    wanted = []
    for spec in args.tile:
        name, _, pos = spec.partition("=")
        try:
            col, row = (int(v) for v in pos.split(","))
        except ValueError:
            print(f"--tile wants NAME=COL,ROW, got {spec!r}", file=sys.stderr)
            return 2
        if not (0 <= col < cols and 0 <= row < rows):
            print(f"--tile {spec}: cell outside the {cols}x{rows} grid", file=sys.stderr)
            return 2
        wanted.append((name, col, row))
    if args.name:
        wanted.append((args.name, 0, 0))
    if not wanted:
        print("nothing to emit: pass --name or --tile", file=sys.stderr)
        return 2

    sheet = np.asarray(Image.open(args.sheet).convert("RGB"))
    order = [p.strip() for p in args.layout.split(",")]
    unknown = set(order) - {"albedo", "height", "normal"}
    if unknown:
        print(f"unknown panel kind(s): {sorted(unknown)}", file=sys.stderr)
        return 2

    bounds = find_panels(sheet, len(order))
    panels = {kind: sheet[:, a:b] for kind, (a, b) in zip(order, bounds)}
    print(f"panels: {', '.join(f'{k} x[{a}:{b}]' for k, (a, b) in zip(order, bounds))}")

    # The cut is decided once, on the first panel, and applied to every panel —
    # the maps must stay registered to each other texel for texel.
    ref = panels[order[0]]
    if (cols, rows) == (1, 1):
        # One texture per panel: no dividers to find, so crop for tileability.
        size = min(ref.shape[:2])
        oy, seam, baseline = pick_crop(ref, size, args.crop)
        ox = 0
        verdict = "tiles cleanly" if seam < baseline * 0.45 else "SEAM WILL SHOW"
        print(f"crop: {size}x{size} at y={oy}  seam {seam:.1f} vs baseline {baseline:.1f} ({verdict})")
        cut = lambda p, c, r: p[oy : oy + size, :size]  # noqa: E731
    else:
        oy, ox = find_grid_origin(ref, cols, rows)
        ch, cw = ref.shape[0] // rows, ref.shape[1] // cols
        print(f"grid: {cols}x{rows} cells of ~{cw}x{ch} at origin ({ox},{oy})")
        cut = lambda p, c, r: cut_cell(p, c, r, cols, rows, oy, ox)  # noqa: E731

    flip = args.flip_green
    if "normal" in panels and "height" in panels and not args.no_flip and not args.flip_green:
        n, h = panels["normal"], panels["height"]
        side = min(n.shape[1], h.shape[1])
        conv, strength = detect_handedness(n[:, :side], h[:, :side])
        flip = conv == DIRECTX
        print(f"normal convention: {conv} (|corr| {strength:.2f} against height) -> flip_green={flip}")

    args.out.mkdir(parents=True, exist_ok=True)
    suffix = {"albedo": "", "height": "_h", "normal": "_n"}
    for name, col, row in wanted:
        for kind, panel in panels.items():
            cell = cut(panel, col, row)
            out = resample(cell, args.size, area=args.size < min(cell.shape[:2]))
            if kind == "normal":
                out = clean_normal(out, flip)
            elif kind == "height":
                out = np.repeat(out.mean(axis=2, keepdims=True).astype(np.uint8), 3, axis=2)
            path = args.out / f"{name}{suffix[kind]}.png"
            Image.fromarray(out).save(path)
        print(f"wrote {name}{{,_h,_n}}.png from cell ({col},{row}) at {args.size}x{args.size}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
