#!/usr/bin/env python3
"""Procedural tile art for the minerals & geology update.

Writes 32x32 PNGs into base/textures/ so the default look is complete
without any image-model API. Deterministic per tile name. The gemini
pack can override any of these later through the normal pack flow.
"""

import math
import random
import sys
from pathlib import Path

from PIL import Image, ImageDraw

PX = 32
OUT = Path(__file__).resolve().parent.parent / "base" / "textures"

# With names on the command line, only those tiles are (re)written —
# adding new art never has to churn every shipped byte.
ONLY = set(sys.argv[1:])


def save_tile(img, name):
    if not ONLY or name in ONLY:
        img.save(OUT / f"{name}.png")


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
        # Head arc on a circle centered on the handle axis (the
        # anti-diagonal), angles symmetric about it: both points of
        # the pick come out the same length.
        d.line([(8, 23), (19, 12)], fill=(122, 84, 48, 255), width=2)
        d.arc([8, 3, 28, 23], 240, 30, fill=body + (255,), width=3)
        d.point((12, 7), fill=edge + (255,))
        d.point((24, 19), fill=edge + (255,))
    elif shape == "strip":
        d.line([(7, 24), (25, 8)], fill=body + (255,), width=3)
        d.line([(7, 24), (25, 8)], fill=edge + (255,), width=1)
    elif shape == "lead":
        d.arc([6, 6, 26, 26], 300, 200, fill=body + (255,), width=2)
        d.ellipse([20, 18, 26, 24], outline=edge + (255,), width=2)
    elif shape == "bags":
        d.rectangle([7, 12, 14, 22], fill=body + (255,), outline=edge + (255,))
        d.rectangle([18, 12, 25, 22], fill=body + (255,), outline=edge + (255,))
        d.line([(14, 14), (18, 14)], fill=edge + (255,), width=2)
        d.line([(8, 15), (13, 15)], fill=edge + (255,))
        d.line([(19, 15), (24, 15)], fill=edge + (255,))
    elif shape == "boat":
        d.polygon([(4, 14), (28, 14), (24, 22), (8, 22)], fill=body + (255,),
                  outline=edge + (255,))
        d.line([(4, 14), (28, 14)], fill=edge + (255,), width=2)
        d.rectangle([14, 16, 18, 18], fill=edge + (255,))
    elif shape == "sign":
        d.rectangle([6, 8, 26, 18], fill=body + (255,), outline=edge + (255,))
        d.rectangle([15, 18, 17, 26], fill=edge + (255,))
        for yy in (11, 14):
            d.line([(9, yy), (23, yy)], fill=edge + (255,))
    elif shape == "cairn":
        for (x0, y0, x1, y1) in [(9, 20, 23, 26), (11, 14, 21, 20), (13, 9, 19, 14)]:
            d.rectangle([x0, y0, x1, y1], fill=body + (255,), outline=edge + (255,))
        d.point((12, 22), fill=edge + (255,))
        d.point((18, 17), fill=edge + (255,))
    elif shape == "screw":
        # Slotted head, threaded shank.
        d.ellipse([11, 5, 21, 15], fill=body + (255,), outline=edge + (255,))
        d.line([(13, 8), (19, 12)], fill=edge + (255,), width=2)
        d.polygon([(14, 14), (18, 14), (17, 26), (15, 26)], fill=body + (255,))
        for yy in range(15, 26, 3):
            d.line([(13, yy + 1), (19, yy - 1)], fill=edge + (255,))
    elif shape == "leadscrew":
        # A long true thread, corner to corner.
        d.line([(6, 26), (26, 6)], fill=body + (255,), width=4)
        for t in range(7):
            x0 = 7 + t * 3
            d.line([(x0 - 1, 27 - t * 3), (x0 + 3, 23 - t * 3)],
                   fill=edge + (255,))
        d.rectangle([4, 24, 9, 28], fill=edge + (255,))
    elif shape == "ring":
        # Races and balls: the bearing.
        d.ellipse([7, 7, 25, 25], fill=body + (255,), outline=edge + (255,))
        d.ellipse([12, 12, 20, 20], fill=(0, 0, 0, 0), outline=edge + (255,))
        for ang in range(0, 360, 45):
            import math as _m
            bx = 16 + int(6.5 * _m.cos(_m.radians(ang)))
            by = 16 + int(6.5 * _m.sin(_m.radians(ang)))
            d.point((bx, by), fill=(240, 242, 248, 255))
    elif shape == "gearwheel":
        # A cut gear: disc, teeth, keyed bore.
        d.ellipse([8, 8, 24, 24], fill=body + (255,), outline=edge + (255,))
        for ang in range(0, 360, 45):
            import math as _m
            bx = 16 + int(9 * _m.cos(_m.radians(ang)))
            by = 16 + int(9 * _m.sin(_m.radians(ang)))
            d.rectangle([bx - 1, by - 1, bx + 1, by + 1], fill=body + (255,))
        d.rectangle([14, 14, 18, 18], fill=(0, 0, 0, 0))
        d.rectangle([14, 14, 18, 18], outline=edge + (255,))
    elif shape == "plate":
        # Hammered sheet: a slab with peen marks.
        d.polygon([(5, 19), (13, 9), (27, 9), (19, 19)], fill=body + (255,),
                  outline=edge + (255,))
        d.polygon([(5, 19), (19, 19), (19, 23), (5, 23)],
                  fill=mix(body, edge, 0.4) + (255,))
        for (px, py) in [(11, 13), (17, 12), (14, 16), (21, 14)]:
            d.point((px, py), fill=edge + (255,))
    elif shape == "cylinder":
        # An open bored tube, seen at a lean.
        d.ellipse([15, 6, 27, 16], fill=mix(body, (0, 0, 0), 0.45) + (255,),
                  outline=edge + (255,))
        d.polygon([(5, 16), (17, 6), (25, 15), (13, 26)], fill=body + (255,))
        d.ellipse([4, 16, 15, 26], fill=body + (255,), outline=edge + (255,))
        d.ellipse([7, 19, 12, 23], fill=(20, 20, 24, 255))
    elif shape == "magnet":
        # A horseshoe with bright poles.
        d.arc([7, 6, 25, 24], 150, 390, fill=body + (255,), width=5)
        d.rectangle([8, 18, 12, 24], fill=(220, 224, 232, 255))
        d.rectangle([20, 18, 24, 24], fill=(220, 224, 232, 255))
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
    "halite": dict(base=(236, 232, 226), dark=(198, 192, 186), bands=0.35,
                   veins=(250, 248, 244)),
    "waystone": dict(base=(126, 130, 140), dark=(88, 92, 104),
                     veins=(196, 214, 230)),
    "stall_counter": dict(base=(158, 116, 68), dark=(112, 78, 42), bands=0.5),
    "boat_hull": dict(base=(150, 106, 58), dark=(104, 70, 36), bands=0.6),
    "clay_block": dict(base=(174, 170, 178), dark=(130, 126, 136), bands=0.25),
    "smoking_rack": dict(base=(120, 84, 50), dark=(78, 52, 30), bands=0.7),
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
    ("salt_crystal", (240, 240, 236), (190, 190, 184), "lump"),
    ("salted_meat", (170, 96, 88), (216, 214, 208), "lump"),
    ("spoiled_mush", (110, 116, 72), (74, 78, 48), "lump"),
    ("survey_cairn", (168, 168, 172), (110, 110, 116), "cairn"),
    ("leather_strip", (146, 96, 54), (100, 62, 32), "strip"),
    ("lead", (160, 108, 62), (110, 70, 36), "lead"),
    ("saddlebags", (122, 80, 44), (82, 52, 26), "bags"),
    ("sign", (146, 108, 62), (96, 68, 36), "sign"),
    ("boat", (150, 106, 58), (100, 66, 32), "boat"),
    ("clay_ball", (172, 168, 176), (124, 120, 130), "lump"),
    ("crock", (150, 96, 70), (100, 60, 42), "crucible"),
    ("pickles", (146, 176, 92), (96, 124, 56), "crucible"),
    ("smoked_meat", (124, 70, 52), (80, 44, 30), "lump"),
    ("beam", (158, 112, 62), (104, 72, 40), "strip"),
    ("screw", (196, 138, 88), (128, 84, 46), "screw"),
    ("leadscrew", (188, 150, 96), (120, 92, 52), "leadscrew"),
    ("iron_shaft", (192, 196, 204), (120, 124, 134), "strip"),
    ("bearing", (206, 210, 218), (128, 132, 142), "ring"),
    ("iron_gear", (184, 188, 196), (112, 116, 126), "gearwheel"),
    ("plate", (198, 202, 210), (126, 130, 140), "plate"),
    ("cylinder", (176, 180, 190), (108, 112, 122), "cylinder"),
    ("neodymium", (202, 206, 216), (130, 134, 146), "lump"),
    ("cerium", (214, 208, 186), (146, 140, 118), "powder"),
    ("magnet", (150, 60, 64), (96, 34, 38), "magnet"),
]

GLASSES = [
    ("polished_glass", (235, 244, 250), 35, False),
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
    # Millwork and the machine-tool age: wood that reads as worked
    # timber, stone that reads as dressed, iron that reads as oiled.
    "water_wheel": dict(base=(140, 100, 58), dark=(96, 64, 34), bands=0.65),
    "water_wheel_run": dict(base=(146, 106, 62), dark=(96, 64, 34), bands=0.65,
                            speckle=(214, 234, 246), speckle_n=24),
    "windmill_sail": dict(base=(228, 222, 204), dark=(192, 184, 164), bands=0.3),
    "windmill_sail_run": dict(base=(234, 228, 210), dark=(196, 188, 168),
                              bands=0.3, speckle=(252, 252, 246), speckle_n=18),
    "shaft": dict(base=(168, 128, 76), dark=(120, 86, 48), bands=0.7),
    "gear": dict(base=(150, 106, 58), dark=(104, 70, 36), bands=0.55,
                 veins=(134, 138, 146)),
    "millstone": dict(base=(178, 170, 160), dark=(134, 126, 118),
                      speckle=(210, 202, 192), speckle_n=28),
    "sawmill": dict(base=(158, 116, 68), dark=(110, 76, 42), bands=0.5,
                    veins=(184, 190, 198)),
    "helve_hammer": dict(base=(124, 92, 56), dark=(82, 58, 34), bands=0.6,
                         speckle=(152, 152, 158), speckle_n=10),
    "lathe": dict(base=(150, 110, 64), dark=(100, 72, 40), bands=0.55,
                  veins=(150, 154, 162)),
    "iron_lathe": dict(base=(126, 130, 140), dark=(84, 88, 98), bands=0.35,
                       veins=(196, 170, 96)),
    "vice": dict(base=(140, 144, 154), dark=(92, 96, 106),
                 speckle=(190, 194, 202), speckle_n=14),
    "fitted_shaft": dict(base=(168, 128, 76), dark=(120, 86, 48), bands=0.7,
                         veins=(200, 204, 212)),
    "boring_mill": dict(base=(110, 114, 124), dark=(70, 74, 84),
                        veins=(184, 154, 94)),
    "firebox": dict(base=(120, 74, 60), dark=(74, 44, 36), bands=0.3,
                    speckle=(50, 46, 48), speckle_n=16),
    "firebox_lit": dict(base=(150, 84, 56), dark=(96, 50, 36), bands=0.3,
                        speckle=(255, 170, 70), speckle_n=22),
    "boiler": dict(base=(134, 128, 120), dark=(90, 84, 78),
                   speckle=(196, 188, 178), speckle_n=30),
    "steam_engine": dict(base=(104, 108, 118), dark=(64, 68, 78),
                         veins=(196, 168, 92)),
    "steam_engine_run": dict(base=(112, 116, 126), dark=(70, 74, 84),
                             veins=(210, 180, 100),
                             speckle=(230, 234, 240), speckle_n=14),
    "separator": dict(base=(172, 156, 142), dark=(122, 108, 96), bands=0.3,
                      veins=(196, 130, 90)),
    "separator_lit": dict(base=(192, 162, 138), dark=(136, 112, 90), bands=0.3,
                          speckle=(255, 190, 110), speckle_n=20),
    "generator": dict(base=(112, 116, 126), dark=(72, 76, 86),
                      veins=(196, 128, 84)),
    "generator_run": dict(base=(120, 124, 134), dark=(78, 82, 92),
                          veins=(214, 140, 92),
                          speckle=(240, 244, 250), speckle_n=10),
    "arc_lamp": dict(base=(88, 90, 98), dark=(52, 54, 62),
                     speckle=(180, 176, 160), speckle_n=8),
    "arc_lamp_lit": dict(base=(244, 238, 214), dark=(210, 198, 160),
                         speckle=(255, 255, 240), speckle_n=20),
    "blue_arc_lamp": dict(base=(70, 80, 106), dark=(42, 48, 68),
                          speckle=(140, 160, 210), speckle_n=8),
    "blue_arc_lamp_lit": dict(base=(170, 196, 250), dark=(120, 150, 220),
                              speckle=(230, 240, 255), speckle_n=20),
    "red_arc_lamp": dict(base=(104, 66, 66), dark=(64, 38, 38),
                         speckle=(190, 130, 130), speckle_n=8),
    "red_arc_lamp_lit": dict(base=(248, 170, 160), dark=(220, 120, 110),
                             speckle=(255, 230, 225), speckle_n=20),
    "pump": dict(base=(118, 130, 126), dark=(76, 86, 82),
                 speckle=(168, 178, 174), speckle_n=12),
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
        save_tile(made[name], name)
    for name, kw in BLOCKS_EXTRA.items():
        save_tile(rock(name, **kw), name)
    for name, mortar in BRICK_MORTAR.items():
        save_tile(bricks(name, made[name], mortar), f"{name}_bricks")

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
        save_tile(img, name)

    for name, body, edge, shape in ITEMS:
        save_tile(item(name, body, edge, shape), name)
    for name, tint, alpha, glow in GLASSES:
        save_tile(glass(name, tint, alpha, glow), name)
    save_tile(lava_tile(), "lava")
    # The lava bucket: the tin bucket silhouette, molten fill.
    bl = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
    d = ImageDraw.Draw(bl)
    d.polygon([(9, 12), (23, 12), (20, 24), (12, 24)], fill=(150, 150, 158, 255),
              outline=(90, 90, 96, 255))
    d.ellipse([9, 9, 23, 14], fill=(255, 120, 30, 255), outline=(90, 90, 96, 255))
    d.arc([6, 2, 26, 14], 200, 340, fill=(110, 108, 104, 255))
    save_tile(bl, "bucket_lava")

    # Farmland fertility variants: graded from the shipped farmland
    # tile so the four steps read as the same soil — dust, poor,
    # (the original), rich. The mesher picks by the meta quartile.
    from PIL import ImageEnhance

    gem = OUT.parent.parent / "packs" / "gemini" / "tiles" / "farmland.png"
    if gem.exists():
        soil = Image.open(gem).convert("RGBA")
        # Exhausted soil dries toward warm khaki; rich soil deepens.
        dry = Image.new("RGBA", soil.size, (198, 172, 122, 255))
        for name, toward, blend, sat in [
            ("farmland_dust", dry, 0.52, 0.72),
            ("farmland_poor", dry, 0.26, 0.88),
            ("farmland_rich", None, 0.0, 1.20),
        ]:
            img = soil
            if toward is not None:
                img = Image.blend(soil, toward, blend)
            img = ImageEnhance.Color(img).enhance(sat)
            if name == "farmland_rich":
                img = ImageEnhance.Brightness(img).enhance(0.72)
            save_tile(img, name)

    print(f"wrote {len(list(OUT.glob('*.png')))} tiles to {OUT}")


if __name__ == "__main__":
    main()
