#!/usr/bin/env python3
"""Procedural tile art for the minerals & geology update.

Writes 32x32 PNGs into base/textures/ so the default look is complete
without any image-model API. Deterministic per tile name. The gemini
pack can override any of these later through the normal pack flow.
"""

import math
import random
from pathlib import Path

from PIL import Image, ImageDraw

PX = 32
OUT = Path(__file__).resolve().parent.parent / "base" / "textures"


def rng_for(name: str) -> random.Random:
    return random.Random(hash(name) & 0xFFFFFFFF)


def noise(rng, scale, octaves=3):
    """Multi-octave value noise as a PX x PX float grid in 0..1."""
    total = [[0.0] * PX for _ in range(PX)]
    amp, norm = 1.0, 0.0
    for o in range(octaves):
        side = max(2, scale * (2**o))
        grid = Image.new("F", (side, side))
        grid.putdata([rng.random() for _ in range(side * side)])
        layer = grid.resize((PX, PX), Image.BILINEAR)
        for y in range(PX):
            for x in range(PX):
                total[y][x] += layer.getpixel((x, y)) * amp
        norm += amp
        amp *= 0.5
    return [[v / norm for v in row] for row in total]


def mix(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def clamp(c):
    return tuple(max(0, min(255, v)) for v in c)


def rock(name, base, dark, *, bands=0.0, veins=None, speckle=None,
         speckle_n=0, glassy=False):
    """A stone tile: noise-shaded base, optional bedding bands,
    contrasting veins, and crystal speckles."""
    rng = rng_for(name)
    n = noise(rng, 4)
    img = Image.new("RGB", (PX, PX))
    for y in range(PX):
        for x in range(PX):
            t = n[y][x]
            if bands > 0.0:
                # Bedding: shift the shade by a slow vertical wave.
                t = t * (1.0 - bands) + bands * (
                    0.5 + 0.5 * math.sin(y * 0.9 + n[y][x] * 2.0)
                )
            c = mix(dark, base, t)
            if glassy:
                # Conchoidal glint: sparse sharp highlights.
                if rng.random() < 0.02:
                    c = mix(c, (200, 210, 230), 0.7)
            img.putpixel((x, y), clamp(c))
    d = ImageDraw.Draw(img)
    if veins is not None:
        for _ in range(3):
            x, y = rng.uniform(0, PX), rng.uniform(0, PX)
            ang = rng.uniform(0, math.tau)
            for _ in range(40):
                ang += rng.uniform(-0.5, 0.5)
                x = (x + math.cos(ang)) % PX
                y = (y + math.sin(ang)) % PX
                d.point((x, y), fill=veins)
    if speckle is not None:
        for _ in range(speckle_n):
            x, y = rng.randrange(PX), rng.randrange(PX)
            d.point((x, y), fill=speckle)
            if rng.random() < 0.5:
                d.point(((x + 1) % PX, y), fill=speckle)
    return img


def bricks(name, rock_img, mortar):
    """Dressed stone: the rock tile behind a running-bond mortar grid."""
    img = rock_img.copy()
    d = ImageDraw.Draw(img)
    course = 8
    for row in range(PX // course):
        y = row * course
        d.line([(0, y), (PX - 1, y)], fill=mortar)
        offset = (row % 2) * 8
        for bx in range(0, PX, 16):
            x = (bx + offset) % PX
            d.line([(x, y), (x, y + course - 1)], fill=mortar)
    return img


def ore(name, host_img, mineral, glint=None, blobs=5):
    """An ore tile: mineral blobs pressed into the host rock."""
    rng = rng_for(name)
    img = host_img.copy()
    d = ImageDraw.Draw(img)
    for _ in range(blobs):
        cx, cy = rng.randrange(3, PX - 3), rng.randrange(3, PX - 3)
        r = rng.randrange(1, 3)
        d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=mineral)
        if glint:
            d.point((cx, cy - 1), fill=glint)
    return img


def item(name, body, edge, shape="lump"):
    """A simple item sprite on transparency."""
    rng = rng_for(name)
    img = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    if shape == "lump":
        d.ellipse([8, 10, 24, 24], fill=body + (255,), outline=edge + (255,))
        d.ellipse([12, 8, 22, 16], fill=body + (255,))
        for _ in range(6):
            x, y = rng.randrange(10, 22), rng.randrange(11, 22)
            d.point((x, y), fill=edge + (255,))
    elif shape == "ingot":
        d.polygon([(6, 20), (12, 12), (26, 12), (20, 20)], fill=body + (255,),
                  outline=edge + (255,))
        d.polygon([(6, 20), (20, 20), (20, 24), (6, 24)], fill=edge + (255,))
        d.polygon([(20, 20), (26, 12), (26, 16), (20, 24)],
                  fill=mix(body, edge, 0.5) + (255,))
    elif shape == "powder":
        d.polygon([(8, 24), (16, 12), (24, 24)], fill=body + (255,))
        for _ in range(10):
            x = rng.randrange(9, 23)
            lo = min(23, max(13, 24 - abs(x - 16) * 2))
            y = rng.randrange(lo, 24)
            d.point((x, y), fill=edge + (255,))
    elif shape == "gem":
        d.polygon([(16, 6), (25, 14), (16, 26), (7, 14)], fill=body + (255,),
                  outline=edge + (255,))
        d.line([(16, 6), (16, 26)], fill=mix(body, (255, 255, 255), 0.5) + (255,))
        d.line([(7, 14), (25, 14)], fill=mix(body, (255, 255, 255), 0.35) + (255,))
    elif shape == "crucible":
        d.polygon([(9, 12), (23, 12), (20, 24), (12, 24)], fill=body + (255,),
                  outline=edge + (255,))
        d.ellipse([9, 9, 23, 14], fill=mix(body, (255, 255, 255), 0.25) + (255,),
                  outline=edge + (255,))
    elif shape == "lens":
        d.ellipse([9, 9, 23, 23], fill=body + (110,), outline=edge + (255,))
        d.arc([11, 11, 21, 21], 200, 320, fill=(255, 255, 255, 220))
    elif shape == "pick":
        d.line([(8, 24), (20, 12)], fill=(122, 84, 48, 255), width=2)
        d.arc([8, 4, 28, 22], 210, 340, fill=body + (255,), width=3)
        d.point((10, 9), fill=edge + (255,))
        d.point((26, 13), fill=edge + (255,))
    elif shape == "prism":
        d.polygon([(16, 7), (25, 24), (7, 24)], fill=body + (120,),
                  outline=edge + (255,))
        d.line([(16, 7), (13, 24)], fill=(255, 255, 255, 160))
    return img


def glass(name, tint, alpha=90, glow=False):
    """A stained pane matching the shipped glass alpha conventions."""
    rng = rng_for(name)
    img = Image.new("RGBA", (PX, PX), tint + (alpha,))
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, PX - 1, PX - 1], outline=clamp(mix(tint, (0, 0, 0), 0.35)) + (200,))
    for _ in range(4):
        x = rng.randrange(4, PX - 6)
        y = rng.randrange(4, PX - 6)
        d.line([(x, y), (x + 3, y + 3)],
               fill=clamp(mix(tint, (255, 255, 255), 0.6)) + (min(255, alpha + 60),))
    if glow:
        d.rectangle([2, 2, PX - 3, PX - 3],
                    outline=clamp(mix(tint, (255, 255, 255), 0.4)) + (150,))
    return img


ROCKS = {
    "sandstone": dict(base=(216, 196, 150), dark=(178, 156, 110), bands=0.45),
    "limestone": dict(base=(205, 200, 184), dark=(166, 160, 142), bands=0.2),
    "shale": dict(base=(120, 118, 122), dark=(84, 82, 90), bands=0.55),
    "granite": dict(base=(188, 172, 160), dark=(140, 124, 116),
                    speckle=(224, 216, 208), speckle_n=40),
    "marble": dict(base=(232, 230, 226), dark=(204, 202, 200),
                   veins=(160, 158, 166)),
    "slate": dict(base=(96, 102, 112), dark=(64, 70, 82), bands=0.6),
    "quartzite": dict(base=(222, 214, 204), dark=(188, 178, 168),
                      speckle=(244, 240, 234), speckle_n=24),
    "basalt": dict(base=(88, 86, 90), dark=(56, 54, 60),
                   speckle=(110, 108, 112), speckle_n=16),
    "obsidian": dict(base=(38, 32, 48), dark=(16, 12, 24), glassy=True),
    "kimberlite": dict(base=(110, 124, 138), dark=(76, 88, 102),
                       speckle=(150, 164, 176), speckle_n=20),
    "carbonatite": dict(base=(196, 178, 162), dark=(158, 140, 126),
                        veins=(216, 206, 190)),
}

BRICK_MORTAR = {
    "sandstone": (150, 132, 96),
    "limestone": (140, 134, 118),
    "granite": (110, 98, 92),
    "marble": (176, 174, 172),
    "slate": (48, 52, 62),
    "basalt": (40, 38, 44),
}

# (name, host, mineral color, glint, blobs)
ORES = [
    ("coal_ore", "shale", (30, 30, 32), None, 7),
    ("quartz_vein", "stone", (236, 232, 226), (255, 255, 255), 8),
    ("gold_quartz", "stone", (236, 232, 226), None, 0),  # special-cased below
    ("galena_ore", "limestone", (110, 116, 130), (196, 204, 220), 5),
    ("chromite_ore", "basalt", (52, 56, 50), (120, 130, 116), 5),
    ("sulfur_ore", "basalt", (220, 200, 60), (250, 240, 130), 6),
    ("diamond_ore", "kimberlite", (190, 235, 240), (255, 255, 255), 4),
    ("pitchblende_ore", "granite", (40, 46, 40), (120, 220, 130), 5),
    ("monazite_sand", "sand_base", (168, 132, 92), (210, 170, 110), 8),
    ("bastnasite_ore", "carbonatite", (188, 122, 84), (232, 168, 120), 5),
]

ITEMS = [
    ("coal", (36, 36, 40), (10, 10, 12), "lump"),
    ("raw_gold", (232, 190, 70), (150, 110, 30), "lump"),
    ("gold_ingot", (240, 198, 78), (160, 120, 34), "ingot"),
    ("raw_galena", (120, 126, 140), (70, 74, 86), "lump"),
    ("lead_ingot", (128, 134, 148), (76, 80, 94), "ingot"),
    ("silver_ingot", (222, 228, 236), (150, 156, 170), "ingot"),
    ("raw_chromite", (60, 66, 58), (30, 34, 30), "lump"),
    ("chrome_powder", (110, 190, 120), (60, 120, 70), "powder"),
    ("sulfur", (228, 208, 70), (170, 150, 40), "lump"),
    ("quartz_shard", (238, 236, 232), (180, 178, 176), "gem"),
    ("amethyst_shard", (186, 132, 222), (120, 76, 160), "gem"),
    ("diamond", (200, 240, 245), (110, 180, 200), "gem"),
    ("raw_pitchblende", (44, 52, 44), (16, 20, 16), "lump"),
    ("uranium_powder", (150, 230, 120), (80, 150, 60), "powder"),
    ("rare_earth_powder", (216, 150, 170), (150, 90, 115), "powder"),
    ("tin_powder", (210, 214, 222), (140, 145, 155), "powder"),
    ("monazite_grit", (180, 144, 100), (120, 92, 60), "powder"),
    ("bastnasite", (196, 130, 90), (130, 82, 52), "lump"),
    ("quartz_crucible", (228, 224, 216), (160, 156, 150), "crucible"),
    ("charged_crucible", (176, 168, 172), (100, 96, 104), "crucible"),
    ("crystal_lens", (210, 230, 240), (140, 170, 190), "lens"),
    ("diamond_tipped_pick", (200, 240, 245), (120, 190, 210), "pick"),
    ("crystal_prism", (215, 232, 242), (145, 172, 192), "prism"),
    ("prospect_pick", (198, 166, 92), (128, 100, 52), "pick"),
]

GLASSES = [
    ("green_glass", (60, 180, 90), 90, False),
    ("cranberry_glass", (200, 40, 90), 95, False),
    ("yellow_glass", (230, 210, 60), 90, False),
    ("milk_glass", (240, 240, 235), 200, False),
    ("rose_glass", (230, 130, 170), 95, False),
    ("glow_glass", (120, 240, 130), 110, True),
    ("crystal_glass", (225, 240, 248), 45, False),
]

BLOCKS_EXTRA = {
    # Crystal blocks for geodes; lava gets its own hot look.
    "amethyst_block": dict(base=(170, 120, 210), dark=(110, 70, 150),
                           speckle=(226, 190, 245), speckle_n=30),
    "quartz_block": dict(base=(230, 226, 220), dark=(196, 192, 186),
                         speckle=(250, 248, 244), speckle_n=30),
    "magma_vent": dict(base=(70, 50, 46), dark=(40, 26, 24),
                       speckle=(255, 140, 40), speckle_n=26),
    "mud": dict(base=(96, 76, 58), dark=(62, 48, 36), bands=0.15),
}


def lava_tile():
    rng = rng_for("lava")
    n = noise(rng, 3)
    img = Image.new("RGB", (PX, PX))
    for y in range(PX):
        for x in range(PX):
            t = n[y][x]
            if t > 0.62:
                c = mix((255, 214, 80), (255, 244, 180), (t - 0.62) / 0.38)
            else:
                c = mix((120, 24, 8), (240, 96, 20), t / 0.62)
            img.putpixel((x, y), clamp(c))
    return img


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    made = {}
    for name, kw in ROCKS.items():
        made[name] = rock(name, **kw)
        made[name].save(OUT / f"{name}.png")
    for name, kw in BLOCKS_EXTRA.items():
        rock(name, **kw).save(OUT / f"{name}.png")
    for name, mortar in BRICK_MORTAR.items():
        bricks(name, made[name], mortar).save(OUT / f"{name}_bricks.png")

    # Hosts that aren't rocks we drew above.
    stone = rock("stone_host", base=(150, 150, 152), dark=(112, 112, 116))
    sand = rock("sand_host", base=(226, 210, 160), dark=(196, 178, 128))
    hosts = dict(made)
    hosts["stone"] = stone
    hosts["sand_base"] = sand

    for name, host, mineral, glint, blobs in ORES:
        if name == "gold_quartz":
            img = ore("gq_base", hosts["stone"], (236, 232, 226),
                      (255, 255, 255), 8)
            img = ore(name, img, (232, 190, 70), (255, 230, 120), 3)
        else:
            img = ore(name, hosts[host], mineral, glint, blobs)
        img.save(OUT / f"{name}.png")

    for name, body, edge, shape in ITEMS:
        item(name, body, edge, shape).save(OUT / f"{name}.png")
    for name, tint, alpha, glow in GLASSES:
        glass(name, tint, alpha, glow).save(OUT / f"{name}.png")
    lava_tile().save(OUT / "lava.png")
    # The lava bucket: the tin bucket silhouette, molten fill.
    bl = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
    d = ImageDraw.Draw(bl)
    d.polygon([(9, 12), (23, 12), (20, 24), (12, 24)], fill=(150, 150, 158, 255),
              outline=(90, 90, 96, 255))
    d.ellipse([9, 9, 23, 14], fill=(255, 120, 30, 255), outline=(90, 90, 96, 255))
    d.arc([6, 2, 26, 14], 200, 340, fill=(110, 108, 104, 255))
    bl.save(OUT / "bucket_lava.png")
    print(f"wrote {len(list(OUT.glob('*.png')))} tiles to {OUT}")


if __name__ == "__main__":
    main()
