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

import heart_table

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

    # The belly's leavings: dung and the compost chain.
    dung = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
    d = ImageDraw.Draw(dung)
    for cx, cy, r in [(13, 20, 6), (19, 21, 5), (16, 17, 5)]:
        d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=(96, 72, 40, 255),
                  outline=(66, 48, 26, 255))
    d.ellipse([12, 15, 16, 18], fill=(112, 86, 50, 255))
    save_tile(dung, "dung")

    comp = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
    d = ImageDraw.Draw(comp)
    d.polygon([(4, 24), (16, 10), (28, 24), (26, 27), (6, 27)],
              fill=(58, 46, 30, 255), outline=(38, 30, 18, 255))
    rng = rng_for("compost")
    for _ in range(26):
        x, y = rng.randint(6, 26), rng.randint(14, 25)
        c = rng.choice([(84, 66, 40), (48, 56, 30), (70, 54, 34)])
        d.point((x, y), fill=c + (255,))
    save_tile(comp, "compost")

    # Heap faces: slatted side, rotting top, ripe crumb top.
    side = rock("compost_heap_side", base=(96, 74, 46), dark=(64, 48, 28))
    d = ImageDraw.Draw(side)
    for yy in [3, 11, 19, 27]:
        d.line([(0, yy), (31, yy)], fill=(52, 38, 22, 255), width=2)
    for xx in [2, 29]:
        d.line([(xx, 0), (xx, 31)], fill=(58, 44, 26, 255), width=2)
    save_tile(side, "compost_heap_side")
    top = rock("compost_heap_top", base=(88, 84, 44), dark=(56, 52, 26))
    save_tile(top, "compost_heap_top")
    ready = rock("compost_ready_top", base=(52, 40, 26), dark=(30, 22, 12))
    save_tile(ready, "compost_ready_top")

    # Fang and carrion: predator pelts, faces, and the bear's meat.
    def face(name, base_c, dark_c, snout=None):
        img = rock(name, base=base_c, dark=dark_c)
        d = ImageDraw.Draw(img)
        for ex in (8, 20):
            d.rectangle([ex, 11, ex + 3, 14], fill=(26, 22, 16, 255))
            d.point((ex + 1, 12), fill=(240, 235, 220, 255))
        if snout:
            d.rectangle([13, 19, 18, 26], fill=snout + (255,))
            d.rectangle([14, 23, 17, 26], fill=(32, 26, 20, 255))
        return img

    pelts = {
        "fox": ((196, 108, 44), (140, 66, 24), (232, 222, 206)),
        "wolf": ((136, 136, 142), (88, 88, 96), (108, 106, 110)),
        "lynx": ((188, 158, 110), (130, 104, 66), (214, 196, 162)),
        "jackal": ((186, 158, 104), (128, 102, 60), (208, 188, 142)),
        "eagle": ((96, 72, 48), (62, 44, 28), None),
        "vulture": ((70, 62, 58), (44, 38, 34), (196, 130, 110)),
        "polar_bear": ((236, 234, 226), (198, 196, 186), (222, 218, 206)),
    }
    for name, (base_c, dark_c, snout) in pelts.items():
        body = rock(name, base=base_c, dark=dark_c)
        if name == "lynx":
            d = ImageDraw.Draw(body)
            rl = rng_for("lynx_spots")
            for _ in range(22):
                x, y = rl.randint(1, 29), rl.randint(1, 29)
                d.rectangle([x, y, x + 1, y + 1], fill=(96, 74, 44, 255))
        save_tile(body, name)
        # The eagle's white head rides its face tile.
        if name == "eagle":
            f = rock("eagle_face", base=(226, 222, 210), dark=(188, 184, 170))
            d = ImageDraw.Draw(f)
            for ex in (7, 21):
                d.rectangle([ex, 11, ex + 3, 14], fill=(40, 30, 12, 255))
                d.point((ex + 1, 12), fill=(250, 220, 120, 255))
            d.polygon([(13, 19), (18, 19), (15, 27)], fill=(212, 160, 44, 255))
            save_tile(f, "eagle_face")
        else:
            save_tile(face(f"{name}_face", base_c, dark_c, snout), f"{name}_face")
    # The rattlesnake wears its diamonds.
    snake = rock("rattlesnake", base=(190, 168, 120), dark=(140, 118, 76))
    d = ImageDraw.Draw(snake)
    for cy in range(2, 32, 7):
        d.polygon(
            [(16, cy), (21, cy + 3), (16, cy + 6), (11, cy + 3)],
            outline=(96, 74, 40, 255),
            fill=(150, 120, 70, 255),
        )
    save_tile(snake, "rattlesnake")
    sf = rock("rattlesnake_face", base=(190, 168, 120), dark=(140, 118, 76))
    d = ImageDraw.Draw(sf)
    for ex in (8, 20):
        d.rectangle([ex, 12, ex + 3, 15], fill=(180, 140, 30, 255))
        d.line([(ex + 1, 12), (ex + 1, 15)], fill=(20, 16, 10, 255))
    save_tile(sf, "rattlesnake_face")
    # The carcass: hide gone still, opened dark red.
    car = rock("carcass", base=(122, 92, 58), dark=(84, 60, 36))
    d = ImageDraw.Draw(car)
    d.ellipse([8, 10, 26, 24], fill=(128, 42, 34, 255), outline=(84, 26, 20, 255))
    d.ellipse([13, 13, 21, 20], fill=(96, 30, 24, 255))
    save_tile(car, "carcass")
    # Bear meat, raw and cooked.
    for nm, fill_c, edge_c in [
        ("raw_bear", (196, 74, 66), (140, 44, 40)),
        ("cooked_bear", (140, 88, 52), (96, 56, 30)),
    ]:
        img = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
        d = ImageDraw.Draw(img)
        d.rounded_rectangle([6, 9, 26, 24], radius=6, fill=fill_c + (255,), outline=edge_c + (255,))
        d.rounded_rectangle([10, 13, 18, 18], radius=3, fill=edge_c + (255,))
        save_tile(img, nm)

    # The water bears life: fish scales, waterline hides, and the
    # rod-and-catch items.
    fish = {
        "trout": ((150, 130, 110), (104, 88, 70)),
        "carp": ((170, 150, 96), (120, 104, 60)),
        "catfish": ((96, 100, 92), (60, 64, 58)),
        # Salt water: cod goes olive-and-pale, mackerel steel with
        # the barred back that names it.
        "cod": ((132, 138, 108), (88, 94, 70)),
        "mackerel": ((122, 140, 156), (76, 92, 108)),
    }
    for name, (base_c, dark_c) in fish.items():
        img = rock(name, base=base_c, dark=dark_c)
        d = ImageDraw.Draw(img)
        rf = rng_for(name + "_scales")
        for _ in range(30):
            x, y = rf.randint(1, 29), rf.randint(1, 29)
            d.arc([x, y, x + 3, y + 3], 200, 340, fill=(
                min(base_c[0] + 40, 255), min(base_c[1] + 40, 255), min(base_c[2] + 40, 255), 255))
        if name == "trout":
            for _ in range(10):
                x, y = rf.randint(2, 28), rf.randint(2, 28)
                d.point((x, y), fill=(196, 90, 80, 255))
        if name == "mackerel":
            # The barred back, wavering the way the real ones do.
            for by in range(1, 32, 5):
                d.line([(0, by), (10, by + 2), (21, by), (31, by + 2)],
                       fill=(38, 52, 66, 255))
        save_tile(img, name)
    # The gull: white over the sea, grey across the mantle, and a
    # yellow bill with the red gonys spot on its face tile.
    gull = rock("gull", base=(238, 238, 234), dark=(206, 208, 208))
    d = ImageDraw.Draw(gull)
    d.rectangle([0, 6, 31, 17], fill=(150, 158, 166, 255))
    save_tile(gull, "gull")
    gf = rock("gull_face", base=(242, 242, 238), dark=(212, 214, 214))
    d = ImageDraw.Draw(gf)
    for ex in (8, 20):
        d.rectangle([ex, 11, ex + 3, 14], fill=(24, 22, 18, 255))
        d.point((ex + 1, 12), fill=(236, 232, 220, 255))
    d.polygon([(13, 18), (18, 18), (15, 27)], fill=(228, 182, 52, 255))
    d.point((15, 25), fill=(206, 62, 44, 255))
    save_tile(gf, "gull_face")
    save_tile(rock("frog", base=(96, 138, 70), dark=(62, 96, 44)), "frog")
    save_tile(face("frog_face", (96, 138, 70), (62, 96, 44), None), "frog_face")
    save_tile(rock("heron", base=(176, 184, 190), dark=(128, 136, 144)), "heron")
    hf = rock("heron_face", base=(176, 184, 190), dark=(128, 136, 144))
    d = ImageDraw.Draw(hf)
    for ex in (8, 20):
        d.rectangle([ex, 10, ex + 3, 13], fill=(30, 26, 20, 255))
        d.point((ex + 1, 11), fill=(240, 210, 90, 255))
    d.polygon([(13, 17), (18, 17), (15, 29)], fill=(216, 170, 60, 255))
    save_tile(hf, "heron_face")
    save_tile(rock("seal", base=(140, 138, 146), dark=(100, 98, 108)), "seal")
    save_tile(face("seal_face", (140, 138, 146), (100, 98, 108), (110, 106, 114)), "seal_face")
    crabimg = rock("crab", base=(196, 92, 60), dark=(140, 58, 36))
    save_tile(crabimg, "crab")
    croc = rock("crocodile", base=(88, 110, 62), dark=(56, 74, 40))
    d = ImageDraw.Draw(croc)
    for yy in range(2, 32, 6):
        for xx in range(2, 32, 6):
            d.rectangle([xx, yy, xx + 2, yy + 2], fill=(64, 84, 46, 255))
    save_tile(croc, "crocodile")
    cf = rock("crocodile_face", base=(88, 110, 62), dark=(56, 74, 40))
    d = ImageDraw.Draw(cf)
    for ex in (7, 21):
        d.rectangle([ex, 8, ex + 3, 11], fill=(210, 190, 60, 255))
        d.line([(ex + 1, 8), (ex + 1, 11)], fill=(20, 18, 12, 255))
    for tx in range(9, 24, 4):
        d.polygon([(tx, 24), (tx + 2, 24), (tx + 1, 27)], fill=(230, 228, 214, 255))
    save_tile(cf, "crocodile_face")
    # Items: the catch, cooked, the rod, and gathered kelp.
    for nm, fill_c, edge_c in [
        ("raw_fish", (150, 164, 176), (104, 118, 130)),
        ("cooked_fish", (168, 128, 78), (118, 86, 48)),
    ]:
        img = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
        d = ImageDraw.Draw(img)
        d.ellipse([4, 11, 22, 21], fill=fill_c + (255,), outline=edge_c + (255,))
        d.polygon([(21, 16), (28, 11), (28, 21)], fill=edge_c + (255,))
        d.point((9, 14), fill=(20, 20, 24, 255))
        save_tile(img, nm)
    rod = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
    d = ImageDraw.Draw(rod)
    d.line([(4, 28), (24, 4)], fill=(122, 86, 48, 255), width=2)
    d.line([(24, 4), (27, 14)], fill=(228, 226, 218, 255))
    d.ellipse([25, 14, 28, 17], fill=(196, 60, 50, 255))
    save_tile(rod, "fishing_rod")
    kelp_i = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
    d = ImageDraw.Draw(kelp_i)
    for x0 in (10, 16, 21):
        d.line([(x0, 26), (x0 + 2, 8)], fill=(52, 96, 60, 255), width=3)
    save_tile(kelp_i, "kelp_item")
    # Plants: cattails at the margin, kelp in the deep, the lily pad.
    cat = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
    d = ImageDraw.Draw(cat)
    for x0, h in [(8, 6), (15, 3), (23, 8)]:
        d.line([(x0, 31), (x0, h + 6)], fill=(92, 128, 60, 255), width=2)
        d.rectangle([x0 - 1, h, x0 + 1, h + 7], fill=(110, 74, 40, 255))
    d.line([(12, 31), (11, 12)], fill=(80, 116, 52, 255))
    d.line([(19, 31), (20, 10)], fill=(80, 116, 52, 255))
    save_tile(cat, "cattail")
    kp = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
    d = ImageDraw.Draw(kp)
    rk = rng_for("kelp")
    for x0 in (7, 13, 19, 25):
        pts = [(x0 + rk.randint(-2, 2), y) for y in range(0, 33, 4)]
        d.line(pts, fill=(44, 88, 54, 255), width=3)
        for px_, py_ in pts[::2]:
            d.ellipse([px_ - 2, py_ - 1, px_ + 3, py_ + 2], fill=(56, 104, 62, 255))
    save_tile(kp, "kelp")
    lily = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
    d = ImageDraw.Draw(lily)
    d.ellipse([3, 4, 28, 27], fill=(62, 118, 58, 255), outline=(44, 88, 44, 255))
    d.polygon([(16, 15), (28, 8), (28, 20)], fill=(0, 0, 0, 0))
    d.pieslice([3, 4, 28, 27], -25, 25, fill=(0, 0, 0, 0))
    d.ellipse([12, 12, 17, 17], fill=(170, 200, 150, 255))
    save_tile(lily, "lily_pad")


    # The full roster: every biome answers.
    herd_pelts = {
        "bison": ((110, 78, 52), (74, 50, 32), (88, 62, 40)),
        "antelope": ((196, 160, 104), (140, 110, 66), (222, 206, 178)),
        "musk_ox": ((92, 74, 56), (58, 46, 34), (76, 60, 44)),
        "mouflon": ((160, 130, 96), (112, 88, 60), (188, 168, 140)),
        "camel": ((206, 172, 116), (152, 122, 76), (188, 156, 104)),
        "marmot": ((164, 130, 88), (116, 88, 56), (188, 160, 120)),
        "bat": ((70, 60, 66), (44, 36, 42), (96, 80, 88)),
    }
    for name, (base_c, dark_c, snout) in herd_pelts.items():
        save_tile(rock(name, base=base_c, dark=dark_c), name)
        save_tile(face(f"{name}_face", base_c, dark_c, snout), f"{name}_face")
    birds = {
        "pheasant": ((150, 96, 60), (104, 60, 36), (216, 170, 60)),
        "guineafowl": ((92, 92, 100), (60, 60, 68), (196, 130, 110)),
        "ptarmigan": ((228, 228, 224), (190, 190, 186), (60, 50, 40)),
        "duck": ((136, 116, 82), (94, 78, 52), (222, 170, 60)),
    }
    for name, (base_c, dark_c, beak) in birds.items():
        body = rock(name, base=base_c, dark=dark_c)
        if name == "guineafowl":
            d = ImageDraw.Draw(body)
            rg = rng_for("guinea_dots")
            for _ in range(60):
                d.point((rg.randint(1, 30), rg.randint(1, 30)),
                        fill=(216, 216, 220, 255))
        save_tile(body, name)
        f = rock(f"{name}_face", base=base_c, dark=dark_c)
        d = ImageDraw.Draw(f)
        for ex in (8, 20):
            d.rectangle([ex, 11, ex + 3, 14], fill=(26, 22, 16, 255))
            d.point((ex + 1, 12), fill=(240, 235, 220, 255))
        d.polygon([(13, 19), (18, 19), (15, 26)], fill=beak + (255,))
        save_tile(f, f"{name}_face")
    # Guano: the cave's pale gift.
    gu = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
    d = ImageDraw.Draw(gu)
    for cx, cy, r in [(12, 20, 6), (20, 21, 5), (16, 16, 4)]:
        d.ellipse([cx - r, cy - r, cx + r, cy + r],
                  fill=(216, 210, 188, 255), outline=(170, 162, 138, 255))
    save_tile(gu, "guano")


    # Rot and fruit: the litter underfoot and the cave's own light.
    lit = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
    d = ImageDraw.Draw(lit)
    rl = rng_for("leaf_litter")
    for _ in range(46):
        x, y = rl.randint(1, 28), rl.randint(1, 28)
        c = rl.choice([(122, 88, 40), (100, 70, 34), (140, 104, 48), (86, 74, 30)])
        d.ellipse([x, y, x + 3, y + 2], fill=c + (255,))
        d.point((x + 1, y + 1), fill=(60, 44, 20, 255))
    save_tile(lit, "leaf_litter")
    lf = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
    d = ImageDraw.Draw(lf)
    for x0, cap_y, r in [(9, 14, 5), (20, 10, 6), (15, 19, 4)]:
        d.rectangle([x0 - 1, cap_y, x0 + 1, 30], fill=(174, 196, 178, 255))
        d.ellipse([x0 - r, cap_y - r, x0 + r, cap_y + r // 2 + 2],
                  fill=(96, 232, 176, 255), outline=(56, 160, 118, 255))
        d.point((x0 - 1, cap_y - r // 2), fill=(210, 255, 232, 255))
    save_tile(lf, "lantern_fungus")


    # The storm gives back: scorched ground and what blooms after.
    ch = rock("charred_soil", base=(52, 44, 40), dark=(28, 24, 22))
    d = ImageDraw.Draw(ch)
    rc = rng_for("char_embers")
    for _ in range(9):
        x, y = rc.randint(2, 29), rc.randint(2, 29)
        d.point((x, y), fill=(216, 110, 40, 255))
        if rc.random() < 0.4:
            d.point((x + 1, y), fill=(150, 60, 24, 255))
    save_tile(ch, "charred_soil")
    for name, petal, heart in [
        ("meadow_bloom", (236, 232, 244), (232, 196, 70)),
        ("ember_poppy", (216, 84, 60), (40, 32, 28)),
    ]:
        fl = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
        d = ImageDraw.Draw(fl)
        rf = rng_for(name)
        for x0, top in [(10, 12), (21, 9), (15, 16)]:
            d.line([(x0, 31), (x0, top + 3)], fill=(84, 124, 58, 255), width=1)
            for ang in range(0, 360, 72):
                ox = int(3 * math.cos(math.radians(ang + rf.randint(-10, 10))))
                oy = int(3 * math.sin(math.radians(ang)))
                d.ellipse([x0 + ox - 1, top + oy - 1, x0 + ox + 1, top + oy + 1],
                          fill=petal + (255,))
            d.point((x0, top), fill=heart + (255,))
        save_tile(fl, name)


    # The hearts of the land: a bole, a spring, a standing stone.
    # Twelve countries, three shapes, and the failing and dead variants
    # derived from each country's own two colours (tools/heart_table.py)
    # rather than authored six times over.
    def heart_bole(name, bark, dark, veins):
        img = rock(name, base=bark, dark=dark)
        d = ImageDraw.Draw(img)
        rb = rng_for(name + "_veins")
        for _ in range(5):
            x = rb.randint(3, 28)
            pts = [(x + rb.randint(-2, 2), y) for y in range(0, 33, 4)]
            d.line(pts, fill=veins + (255,), width=2)
        return img

    def heart_top(name, ring_a, ring_b, core):
        img = rock(name, base=ring_a, dark=ring_b)
        d = ImageDraw.Draw(img)
        for r in range(15, 2, -3):
            d.ellipse([16 - r, 16 - r, 16 + r, 16 + r], outline=ring_b + (255,))
        d.ellipse([12, 12, 20, 20], fill=core + (255,))
        return img

    def heart_spring(name, water, rim, glow, dry):
        img = rock(name, base=rim, dark=tuple(max(0, v - 40) for v in rim))
        d = ImageDraw.Draw(img)
        d.ellipse([4, 4, 27, 27], fill=water + (255,),
                  outline=tuple(max(0, v - 50) for v in rim) + (255,))
        if dry:
            # A cracked pan where the water was.
            rr = rng_for(name + "_dry")
            for _ in range(7):
                x, y = rr.randint(8, 24), rr.randint(8, 24)
                d.line([(x, y), (x + rr.randint(-5, 5), y + rr.randint(-5, 5))],
                       fill=tuple(max(0, v - 34) for v in water) + (255,))
        else:
            for r in (9, 6, 3):
                d.ellipse([16 - r, 16 - r, 16 + r, 16 + r], outline=glow + (255,))
        return img

    def heart_stone(name, base_c, dark_c, rune, cracked):
        img = rock(name, base=base_c, dark=dark_c)
        d = ImageDraw.Draw(img)
        for y0 in (7, 14, 21):
            d.line([(9, y0), (22, y0)], fill=rune + (255,), width=2)
            d.line([(9, y0), (12, y0 - 3)], fill=rune + (255,))
            d.line([(22, y0), (19, y0 + 3)], fill=rune + (255,))
        if cracked:
            d.line([(6, 2), (14, 16), (9, 30)], fill=(28, 26, 30, 255), width=2)
            d.line([(24, 4), (19, 15)], fill=(28, 26, 30, 255))
        return img

    for biome, arch, _names, key, accent in heart_table.HEARTS:
        bid = heart_table.biome_id(biome)
        for stage in heart_table.STAGES:
            k, a = heart_table.stage_palette(key, accent, stage)
            nm = f"heart_{bid}{stage}"
            if arch == "bole":
                save_tile(heart_bole(nm, k, heart_table.darken(k, 0.35), a), nm)
                save_tile(
                    heart_top(f"{nm}_top", k, heart_table.darken(k, 0.35), a),
                    f"{nm}_top",
                )
            elif arch == "spring":
                # The spring's key IS the water; its accent is the rim.
                save_tile(
                    heart_spring(nm, k, a, heart_table.mix(k, (255, 255, 255), 0.6),
                                 stage == "_dead"),
                    nm,
                )
            else:
                save_tile(heart_stone(nm, k, heart_table.darken(k, 0.35), a,
                                      stage != ""), nm)

    # The quickened seed: a living thing you carry. One per country,
    # wearing the colour of the heart it was cut from, so a pack of
    # them reads at a glance.
    def heart_seed(name, husk, core, halo):
        img = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
        d = ImageDraw.Draw(img)
        d.ellipse([9, 8, 23, 26], fill=husk + (255,),
                  outline=heart_table.darken(husk, 0.38) + (255,))
        d.ellipse([12, 12, 20, 21], fill=core + (255,))
        d.ellipse([14, 14, 18, 18], fill=halo + (255,))
        d.line([(16, 8), (16, 3)], fill=(96, 150, 84, 255), width=2)
        d.ellipse([16, 2, 22, 6], fill=(110, 176, 96, 255))
        return img

    for biome, arch, _names, key, accent in heart_table.HEARTS:
        # The husk is the heart's own body, the core what runs in it.
        husk = key if arch != "spring" else accent
        core = accent if arch != "spring" else key
        save_tile(
            heart_seed(f"{heart_table.biome_id(biome)}_seed", husk, core,
                       heart_table.mix(core, (255, 255, 255), 0.55)),
            f"{heart_table.biome_id(biome)}_seed",
        )
    sd = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
    d = ImageDraw.Draw(sd)
    save_tile(sd, "heart_seed")
    for nm, core, halo in [
        ("spring_seed", (120, 208, 240), (210, 240, 255)),
        ("stone_seed", (188, 172, 244), (226, 216, 255)),
    ]:
        s2 = Image.new("RGBA", (PX, PX), (0, 0, 0, 0))
        d = ImageDraw.Draw(s2)
        d.ellipse([9, 8, 23, 26], fill=(104, 96, 88, 255), outline=(64, 58, 52, 255))
        d.ellipse([12, 12, 20, 21], fill=core + (255,))
        d.ellipse([14, 14, 18, 18], fill=halo + (255,))
        d.line([(16, 8), (16, 3)], fill=(120, 116, 110, 255), width=2)
        save_tile(s2, nm)

    print(f"wrote {len(list(OUT.glob('*.png')))} tiles to {OUT}")


if __name__ == "__main__":
    main()
