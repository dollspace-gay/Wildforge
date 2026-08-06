#!/usr/bin/env python3
"""Audit pack and base tiles for geometry and consistency defects.

Mechanical triage for the art the eye keeps catching: asymmetric
pickaxe heads, tools that drift in scale across a tier, sprites
knocked off their 45-degree handle line, terrain tiles that seam.
Scores every PNG it knows how to judge, prints a ranked report, and
writes annotated contact sheets per family for the taste pass.

Usage:
  tools/audit_tiles.py                 # audit packs/gemini + base/textures
  tools/audit_tiles.py --sheets DIR    # also write contact sheets to DIR
"""

import math
import os
import sys

from PIL import Image, ImageDraw

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gen_texture_pack import TILES  # noqa: E402  (category source of truth)

PACK_DIR = "packs/gemini/tiles"
BASE_DIR = "base/textures"
ROCK_FAMILY = (
    "sandstone",
    "limestone",
    "shale",
    "granite",
    "marble",
    "slate",
    "quartzite",
    "basalt",
)

# Families whose members should agree on scale and anchor. The key is
# the name suffix; members are prefixed by material.
TOOL_FAMILIES = ("pickaxe", "axe", "shovel", "hoe", "sword")
# Tools that are bilaterally symmetric across the handle axis (the
# anti-diagonal), with per-family thresholds: a spade blade's curve
# reads fine at scores a pick head's lobes must not sink to. Axes
# and hoes hook one way on purpose.
SYMMETRIC = {"pickaxe": 0.62, "shovel": 0.40}
# Sprites promised "diagonal" in their prompts want their long axis
# near 45 degrees.
DIAGONALS = ("pickaxe", "axe", "shovel", "hoe", "sword")


def alpha_mask(img):
    return [
        [1 if img.getpixel((x, y))[3] > 8 else 0 for x in range(img.width)]
        for y in range(img.height)
    ]


def mask_iou(a, b):
    inter = union = 0
    for ra, rb in zip(a, b):
        for va, vb in zip(ra, rb):
            inter += va & vb
            union += va | vb
    return inter / union if union else 1.0


def symmetry_score(img):
    """1.0 = perfectly symmetric across the anti-diagonal (the handle
    axis of a diagonal tool sprite); lower is worse."""
    m = alpha_mask(img)
    t = alpha_mask(img.transpose(Image.TRANSVERSE))
    return mask_iou(m, t)


def bbox_stats(img):
    """Occupancy fraction and centroid offset (pixels from center)."""
    bbox = img.getbbox()
    if not bbox:
        return 0.0, 0.0, (0, 0, 0, 0)
    w, h = bbox[2] - bbox[0], bbox[3] - bbox[1]
    occ = max(w, h) / img.width
    cx = (bbox[0] + bbox[2]) / 2 - img.width / 2
    cy = (bbox[1] + bbox[3]) / 2 - img.height / 2
    return occ, math.hypot(cx, cy), bbox


def principal_angle(img):
    """Principal-axis angle of the opaque pixels, degrees in 0..90
    (45 = on the diagonal)."""
    xs, ys, n = [], [], 0
    for y in range(img.height):
        for x in range(img.width):
            if img.getpixel((x, y))[3] > 8:
                xs.append(x)
                ys.append(y)
                n += 1
    if n < 8:
        return None
    mx = sum(xs) / n
    my = sum(ys) / n
    sxx = sum((x - mx) ** 2 for x in xs) / n
    syy = sum((y - my) ** 2 for y in ys) / n
    sxy = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / n
    ang = 0.5 * math.degrees(math.atan2(2 * sxy, sxx - syy))
    return abs(ang) % 90


def seam_score(img):
    """Wrap-seam severity for tileables: edge-pair difference relative
    to the tile's own interior gradient. > ~2 reads as a visible seam."""
    w, h = img.size
    px = img.convert("RGB").load()

    def col_diff(x0, x1):
        return sum(
            abs(px[x0, y][c] - px[x1, y][c]) for y in range(h) for c in range(3)
        ) / (h * 3)

    def row_diff(y0, y1):
        return sum(
            abs(px[x, y0][c] - px[x, y1][c]) for x in range(w) for c in range(3)
        ) / (w * 3)

    interior = (col_diff(w // 2, w // 2 + 1) + row_diff(h // 2, h // 2 + 1)) / 2
    seam = (col_diff(0, w - 1) + row_diff(0, h - 1)) / 2
    return seam / max(interior, 1.0)


def linear_luminance(rgb):
    def linear(channel):
        value = channel / 255.0
        return value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4

    return 0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])


def pixels(img):
    flattened = getattr(img, "get_flattened_data", None)
    return flattened() if flattened is not None else img.getdata()


def rock_signature(img, side=8):
    """Mean-free large-scale luminance used only for family similarity."""
    reduced = img.convert("RGB").resize((side, side), Image.Resampling.BOX)
    values = [linear_luminance(pixel) for pixel in pixels(reduced)]
    mean = sum(values) / len(values)
    centered = [value - mean for value in values]
    norm = math.sqrt(sum(value * value for value in centered))
    return [value / max(norm, 1e-9) for value in centered]


def opaque_rock_audit():
    findings = []
    rocks = {}
    signatures = {}
    for name in ROCK_FAMILY:
        path = os.path.join(BASE_DIR, f"{name}.png")
        if not os.path.exists(path):
            findings.append((10.0, name, "missing", path))
            continue
        source = Image.open(path)
        if source.size != (32, 32) or source.format != "PNG" or source.mode != "RGB":
            findings.append(
                (
                    10.0,
                    name,
                    "format",
                    f"{source.format} {source.mode} {source.width}x{source.height}",
                )
            )
        img = source.convert("RGB")
        rocks[name] = img
        px = img.load()
        horizontal = max(
            abs(px[0, y][channel] - px[img.width - 1, y][channel])
            for y in range(img.height)
            for channel in range(3)
        )
        vertical = max(
            abs(px[x, 0][channel] - px[x, img.height - 1][channel])
            for x in range(img.width)
            for channel in range(3)
        )
        if horizontal or vertical:
            findings.append(
                (
                    10.0,
                    name,
                    "seam",
                    f"opposite-edge delta h={horizontal} v={vertical}",
                )
            )
        luma = sorted(linear_luminance(pixel) for pixel in pixels(img))
        p10 = luma[len(luma) // 10]
        p90 = luma[len(luma) * 9 // 10]
        minimum_span = 0.010 if name == "basalt" else 0.018
        if p90 - p10 < minimum_span:
            findings.append(
                (2.0, name, "flat-rock", f"linear p10-p90 span {p90 - p10:.4f}")
            )
        clipped = sum(
            1
            for pixel in pixels(img)
            if min(pixel) <= 3 or max(pixel) >= 252
        )
        if clipped:
            findings.append(
                (2.0, name, "clipping", f"{clipped}/{img.width * img.height} pixels")
            )
        signatures[name] = rock_signature(img)

    names = sorted(signatures)
    for i, left in enumerate(names):
        for right in names[i + 1 :]:
            correlation = sum(
                a * b for a, b in zip(signatures[left], signatures[right])
            )
            if correlation > 0.92:
                findings.append(
                    (
                        correlation / 0.92,
                        f"{left}/{right}",
                        "same-rock",
                        f"8px structure correlation {correlation:.3f}",
                    )
                )
    return findings, rocks


def tool_family(name):
    for fam in TOOL_FAMILIES:
        if name.endswith("_" + fam):
            return fam
    return None


def audit():
    findings = []  # (severity, name, check, detail)
    sprites = {}
    # ---- pack sprites (category known from the generator) ----
    for name, (cat, _) in sorted(TILES.items()):
        path = os.path.join(PACK_DIR, f"{name}.png")
        if not os.path.exists(path):
            continue
        img = Image.open(path).convert("RGBA")
        if cat in ("sprite", "plant"):
            sprites[name] = img
        if cat == "tile":
            s = seam_score(img)
            if s > 2.5:
                findings.append((s / 2.5, name, "seam", f"seam ratio {s:.1f}"))
        if cat == "sprite":
            occ, drift, _ = bbox_stats(img)
            if occ < 0.28:
                findings.append((0.28 / max(occ, 0.01), name, "ghost", f"occupies {occ:.0%}"))
            fam = tool_family(name)
            if fam in SYMMETRIC:
                sym = symmetry_score(img)
                if sym < SYMMETRIC[fam]:
                    findings.append((1.0 - sym, name, "asymmetric", f"mirror IoU {sym:.2f}"))
            if fam in DIAGONALS:
                ang = principal_angle(img)
                if ang is not None and abs(ang - 45) > 18:
                    findings.append(
                        ((abs(ang - 45) - 18) / 20, name, "off-axis", f"axis {ang:.0f} deg")
                    )
    # ---- tier-family scale coherence ----
    for fam in TOOL_FAMILIES:
        members = {n: img for n, img in sprites.items() if tool_family(n) == fam}
        if len(members) < 3:
            continue
        occs = {n: bbox_stats(i)[0] for n, i in members.items()}
        med = sorted(occs.values())[len(occs) // 2]
        for n, occ in occs.items():
            if med and abs(occ - med) / med > 0.28:
                findings.append(
                    (abs(occ - med) / med, n, "scale-drift", f"{occ:.0%} vs family {med:.0%}")
                )
        drifts = {n: bbox_stats(i)[1] for n, i in members.items()}
        for n, d in drifts.items():
            if d > 4.5:
                findings.append((d / 4.5, n, "anchor-drift", f"centroid {d:.1f}px off"))
    # ---- base (procedural) sprites: alpha images only ----
    for f in sorted(os.listdir(BASE_DIR)):
        if not f.endswith(".png"):
            continue
        img = Image.open(os.path.join(BASE_DIR, f)).convert("RGBA")
        if img.getchannel("A").getextrema()[0] == 255:
            continue  # opaque block tile: procedural, judged elsewhere
        name = "base/" + f[:-4]
        occ, _, _ = bbox_stats(img)
        if 0.0 < occ < 0.28:
            findings.append((0.28 / max(occ, 0.01), name, "ghost", f"occupies {occ:.0%}"))
        fam = tool_family(f[:-4])
        if fam in SYMMETRIC:
            sym = symmetry_score(img)
            if sym < SYMMETRIC[fam]:
                findings.append((1.0 - sym, name, "asymmetric", f"mirror IoU {sym:.2f}"))
    return findings, sprites


def sheets(sprites, outdir):
    os.makedirs(outdir, exist_ok=True)
    order = ["wood", "stone", "copper", "bronze", "iron", "steel"]
    for fam in TOOL_FAMILIES:
        row = [(n, i) for n, i in sprites.items() if tool_family(n) == fam]
        row.sort(key=lambda p: next((k for k, m in enumerate(order) if p[0].startswith(m)), 99))
        if not row:
            continue
        s = 128
        sheet = Image.new("RGBA", (s * len(row), s + 18), (40, 40, 48, 255))
        d = ImageDraw.Draw(sheet)
        for i, (n, img) in enumerate(row):
            sheet.paste(img.resize((s, s), Image.NEAREST), (i * s, 0))
            sym = symmetry_score(img) if fam in SYMMETRIC else None
            occ, drift, _ = bbox_stats(img)
            label = f"{n.split('_')[0]} occ{occ:.0%}"
            if sym is not None:
                label += f" sym{sym:.2f}"
            d.text((i * s + 4, s + 3), label, fill=(230, 230, 235, 255))
        sheet.save(os.path.join(outdir, f"{fam}s.png"))


def rock_sheets(rocks, outdir):
    if not rocks:
        return
    os.makedirs(outdir, exist_ok=True)
    scale = 5
    tile = 32 * scale
    label_height = 20
    for filename, greyscale in (("strata-color.png", False), ("strata-greyscale.png", True)):
        sheet = Image.new("RGB", (tile * len(ROCK_FAMILY), tile + label_height), (36, 38, 44))
        draw = ImageDraw.Draw(sheet)
        for index, name in enumerate(ROCK_FAMILY):
            if name not in rocks:
                continue
            image = rocks[name].convert("L").convert("RGB") if greyscale else rocks[name]
            sheet.paste(image.resize((tile, tile), Image.Resampling.NEAREST), (index * tile, 0))
            draw.text((index * tile + 4, tile + 4), name, fill=(232, 232, 236))
        sheet.save(os.path.join(outdir, filename))


def main():
    findings, sprites = audit()
    rock_findings, rocks = opaque_rock_audit()
    findings.extend(rock_findings)
    findings.sort(key=lambda f: -f[0])
    for sev, name, check, detail in findings:
        print(f"{sev:5.2f}  {check:12} {name:28} {detail}")
    print(f"-- {len(findings)} findings")
    if "--sheets" in sys.argv:
        outdir = sys.argv[sys.argv.index("--sheets") + 1]
        sheets(sprites, outdir)
        rock_sheets(rocks, outdir)
        print(f"sheets -> {outdir}")


if __name__ == "__main__":
    main()
