#!/usr/bin/env python3
"""The player's face tile — hand-authored, not model-generated.

Voxel faces read at the resolution they were designed for: eight
pixels across, one feature per pixel, scaled up hard. Drawing one
smoothly at 32x32 is what turned the last face into a grey snout.

The base stays near-greyscale and light: build_atlas multiplies this
tile by the skin palettes (see style.rs), so any real colour here
would fight the player's chosen tone. Contrast survives that
multiply, which is why the eyes are a bright sclera against a very
dark iris rather than "white" and "blue".
"""

from pathlib import Path

from PIL import Image

OUT = Path(__file__).resolve().parent.parent / "packs" / "gemini" / "tiles"
PX = 32
CELL = 4  # 8x8 logical face, four screen pixels per voxel

SKIN = (228, 220, 212)
SKIN_D = (214, 206, 198)  # jaw / nose shading
BROW = (112, 100, 92)
SCLERA = (250, 248, 246)
IRIS = (58, 60, 70)
MOUTH = (190, 160, 152)

#  . = skin   b = brow   w = sclera   i = iris
#  n = nose shade   m = mouth
FACE = [
    "........",
    "........",
    ".bb..bb.",
    ".wi..iw.",
    "...nn...",
    "........",
    "..mmmm..",
    "........",
]

PALETTE = {
    ".": SKIN,
    "b": BROW,
    "w": SCLERA,
    "i": IRIS,
    "n": SKIN_D,
    "m": MOUTH,
}


def main():
    img = Image.new("RGBA", (PX, PX), SKIN + (255,))
    for gy, row in enumerate(FACE):
        for gx, ch in enumerate(row):
            c = PALETTE[ch] + (255,)
            for y in range(CELL):
                for x in range(CELL):
                    img.putpixel((gx * CELL + x, gy * CELL + y), c)
    img.save(OUT / "player_face.png")
    print(f"wrote {OUT / 'player_face.png'}")


if __name__ == "__main__":
    main()
