#!/usr/bin/env python3
"""Generate a Wildforge texture pack with Gemini image generation.

Reads the API key from ~/.gemini_key (never stored in the repo).
Writes packs/<pack>/tiles/<name>.png at 32x32, ready for the in-game
TEXTURE PACKS screen. Tiles you don't generate fall through to the
procedural defaults, so partial packs are fine.

Usage:
  tools/gen_texture_pack.py                # everything missing
  tools/gen_texture_pack.py grass_top sand # just these (implies --force)
  tools/gen_texture_pack.py --force        # regenerate everything
"""

import base64
import io
import json
import os
import sys
import time
import urllib.error
import urllib.request

from PIL import Image

MODEL = "gemini-3.1-flash-image"
PACK = "packs/gemini"
OUT_PX = 32

STYLE = (
    "Retro voxel-game texture in crisp 32x32 pixel art style, chunky pixels, "
    "limited earthy palette, flat even lighting, no text, no watermark, no borders. "
)
TILE_STYLE = STYLE + "A single seamless tileable square ground/block texture, uniform density, {}"
SPRITE_STYLE = (
    STYLE
    + "A single small game item sprite, centered, isolated on a SOLID PURE MAGENTA "
    + "(#FF00FF) background. The magenta must fill the entire image edge to edge — "
    + "no frames, no borders, no white bars, no other background colors, no shadows. {}"
)
FACE_STYLE = STYLE + "A full-frame flat texture (no background visible), {}"
HEAD_STYLE = (
    STYLE
    + "A flat texture for the FRONT face of a cube-shaped animal head in a retro "
    + "voxel game (like a Minecraft mob face). Fur fills the entire frame edge to "
    + "edge. Facial features are drawn FLAT and simple: two eyes in the upper "
    + "half, {} No ears, no horns, no head outline, no portrait framing, no "
    + "background, no fisheye — just fur with flat features."
)

# name -> (category, prompt fragment). Categories:
#   tile   = opaque + edge-wrapped for tiling, crop center quarter
#   face   = opaque full-frame (animal fur/faces), crop center quarter
#   sprite = magenta chroma-keyed to transparency, whole frame
TILES = {
    "grass_top": ("tile", "lush green meadow grass seen from directly above"),
    "grass_side": ("tile", "cross-section: brown soil with a strip of green grass along the very top edge only"),
    "dirt": ("tile", "plain brown soil with tiny pebbles"),
    "stone": ("tile", "grey rough stone rock face"),
    "cobblestone": ("tile", "grey cobblestone made of a few large rounded stones with dark mortar"),
    "sand": ("tile", "pale yellow desert sand, fine grain"),
    "gravel": ("tile", "loose grey-brown gravel pebbles"),
    "log_side": ("tile", "oak tree bark, vertical brown wood grain with knots"),
    "log_top": ("tile", "cut tree stump end grain with concentric growth rings"),
    "leaves": ("tile", "dense green oak leaf foliage"),
    "planks": ("tile", "warm brown wooden planks, horizontal boards with nail dots"),
    "bedrock": ("tile", "very dark jagged unbreakable rock"),
    "table_top": ("tile", "crafting workbench top: wooden surface with a tool grid pattern"),
    "table_side": ("tile", "crafting workbench side: wood panel with a saw and hammer motif"),
    "snow": ("tile", "smooth white snow with faint blue shading"),
    "ice": ("tile", "pale translucent blue ice with cracks"),
    "cactus_side": ("tile", "green cactus skin with vertical ribs and small spines"),
    "cactus_top": ("tile", "green cactus viewed from above with radial ribs"),
    "birch_log": ("tile", "white birch bark with black horizontal streaks"),
    "birch_log_top": ("tile", "cut birch stump end grain, pale rings"),
    "birch_leaves": ("tile", "light airy green-yellow birch foliage"),
    "spruce_log": ("tile", "dark brown spruce bark, rough vertical grain"),
    "spruce_log_top": ("tile", "cut spruce stump end grain, dark rings"),
    "spruce_leaves": ("tile", "deep dark green dense spruce needles"),
    "jungle_log": ("tile", "reddish tropical bark with moss patches"),
    "jungle_log_top": ("tile", "cut tropical hardwood end grain, reddish rings"),
    "jungle_leaves": ("tile", "vivid bright green tropical jungle foliage"),
    "acacia_log": ("tile", "grey-brown acacia bark, rough texture"),
    "acacia_log_top": ("tile", "cut acacia end grain, orange-tinged rings"),
    "acacia_leaves": ("tile", "olive green flat-crown acacia foliage"),
    "birch_planks": ("tile", "pale cream birch wooden planks, horizontal boards"),
    "spruce_planks": ("tile", "dark brown spruce wooden planks, horizontal boards"),
    "jungle_planks": ("tile", "warm reddish tropical wooden planks, horizontal boards"),
    "acacia_planks": ("tile", "orange-brown acacia wooden planks, horizontal boards"),
    "copper_ore": ("tile", "grey stone with embedded orange copper nuggets"),
    "tin_ore": ("tile", "grey stone with embedded silvery tin flecks"),
    "copper_block": ("tile", "polished orange copper metal block panel"),
    "bronze_block": ("tile", "polished golden-brown bronze metal block panel"),
    "furnace": ("tile", "stone furnace front: dark arched opening with orange fire glow"),
    "farmland": ("tile", "dark tilled farm soil with moist furrow rows"),
    # animal fur + faces (box-model textures, full frame)
    "deer": ("face", "tan brown deer fur"),
    "deer_face": ("head", "a small dark nose at the bottom center. Tan brown deer fur."),
    "boar": ("face", "coarse dark grey-brown bristly boar hide"),
    "boar_face": ("head", "a flat pink snout disc with two nostrils at the bottom center and two tiny white tusk dots at the sides. Dark grey-brown bristly boar fur."),
    "goat": ("face", "shaggy off-white mountain goat wool"),
    "goat_face": ("head", "eyes with horizontal bar pupils, and a grey muzzle patch at the bottom. Shaggy off-white goat wool."),
    "grouse": ("face", "mottled brown and buff feather plumage"),
    "grouse_face": ("head", "a small flat orange beak triangle at the bottom center. Mottled brown and buff feathers."),
    "rabbit": ("face", "soft light brown rabbit fur"),
    "rabbit_face": ("head", "a tiny pink nose at the center bottom with small whisker dots. Soft light brown rabbit fur."),
    "desert_hare": ("face", "sandy pale tan hare fur"),
    "snow_hare": ("face", "pure white winter hare fur with faint grey shading"),
    # crops & plants (sprites on magenta, drawn full-height)
    "wheat_young": ("sprite", "a few short young green wheat sprouts growing from the bottom edge"),
    "wheat_ripe": ("sprite", "tall ripe golden wheat stalks with heavy heads, growing from the bottom edge"),
    "carrot_plant": ("sprite", "leafy green carrot tops with orange carrot crowns peeking at the soil line"),
    "potato_plant": ("sprite", "low bushy green potato plant with tiny white blossoms"),
    "bush_fruited": ("sprite", "small leafy berry bush dotted with ripe red berries"),
    "bush_bare": ("sprite", "small leafy green bush with no berries"),
    "mushroom": ("sprite", "single brown forest mushroom with pale stem"),
    # items
    "stick": ("sprite", "a simple wooden stick, diagonal"),
    "wood_pickaxe": ("sprite", "pickaxe whose head is carved from light brown WOOD (not stone, not metal), stick handle, diagonal"),
    "stone_pickaxe": ("sprite", "stone-headed pickaxe with stick handle, diagonal"),
    "copper_pickaxe": ("sprite", "orange copper-headed pickaxe with stick handle, diagonal"),
    "bronze_pickaxe": ("sprite", "golden-brown bronze-headed pickaxe with stick handle, diagonal"),
    "wood_axe": ("sprite", "wooden hatchet axe with stick handle, diagonal"),
    "stone_axe": ("sprite", "stone-headed hatchet axe with stick handle, diagonal"),
    "copper_axe": ("sprite", "orange copper-headed hatchet axe, diagonal"),
    "bronze_axe": ("sprite", "golden-brown bronze-headed hatchet axe, diagonal"),
    "wood_shovel": ("sprite", "wooden shovel with stick handle, diagonal"),
    "stone_shovel": ("sprite", "stone-headed shovel with stick handle, diagonal"),
    "copper_shovel": ("sprite", "orange copper-headed shovel, diagonal"),
    "bronze_shovel": ("sprite", "golden-brown bronze-headed shovel, diagonal"),
    "wood_hoe": ("sprite", "wooden farming hoe with stick handle, diagonal"),
    "stone_hoe": ("sprite", "stone-headed farming hoe, diagonal"),
    "copper_hoe": ("sprite", "orange copper farming hoe, diagonal"),
    "bronze_hoe": ("sprite", "golden-brown bronze farming hoe, diagonal"),
    "wood_sword": ("sprite", "wooden shortsword with crossguard, diagonal, blade up-right"),
    "stone_sword": ("sprite", "stone-bladed shortsword with crossguard, diagonal, blade up-right"),
    "copper_sword": ("sprite", "orange copper shortsword with crossguard, diagonal, blade up-right"),
    "bronze_sword": ("sprite", "golden-brown bronze shortsword with crossguard, diagonal, blade up-right"),
    "raw_copper": ("sprite", "rough lump of raw orange copper ore"),
    "raw_tin": ("sprite", "rough lump of raw silvery tin ore"),
    "copper_ingot": ("sprite", "cast orange copper metal ingot bar"),
    "tin_ingot": ("sprite", "cast silvery tin metal ingot bar"),
    "bronze_ingot": ("sprite", "cast golden-brown bronze metal ingot bar"),
    "bronze_blend": ("sprite", "small pile of mixed orange and silver metal dust"),
    "charcoal": ("sprite", "black lump of charcoal"),
    "bread": ("sprite", "golden crusty bread loaf"),
    "berry": ("sprite", "cluster of three ripe red berries"),
    "carrot": ("sprite", "orange carrot with green top"),
    "potato": ("sprite", "brown raw potato"),
    "baked_potato": ("sprite", "golden baked potato, steaming"),
    "roasted_mushroom": ("sprite", "a grilled mushroom with a wide browned cap and pale stem, char marks on the cap"),
    "cactus_fruit": ("sprite", "pink prickly pear cactus fruit"),
    "jungle_fruit": ("sprite", "yellow tropical fruit"),
    "stew": ("sprite", "wooden bowl of chunky vegetable stew"),
    "hearty_stew": ("sprite", "wooden bowl of rich dark meat stew"),
    "seeds": ("sprite", "small scattered handful of dark green seeds"),
    "raw_venison": ("sprite", "raw dark red venison steak with fat marbling"),
    "cooked_venison": ("sprite", "grilled browned venison steak"),
    "raw_boar": ("sprite", "raw pink boar meat chop with fat marbling"),
    "cooked_boar": ("sprite", "roasted browned boar meat chop"),
    "raw_chevon": ("sprite", "raw red goat meat cut"),
    "cooked_chevon": ("sprite", "roasted browned goat meat cut"),
    "raw_fowl": ("sprite", "raw pale pink whole small fowl"),
    "cooked_fowl": ("sprite", "golden roasted whole small fowl"),
    "raw_rabbit": ("sprite", "raw small pale rabbit meat cut"),
    "cooked_rabbit": ("sprite", "roasted browned small rabbit meat cut"),
    "hide": ("sprite", "flat brown animal hide pelt"),
    "leather": ("sprite", "folded piece of tanned tan leather"),
    "feather": ("sprite", "single white-grey bird feather, diagonal"),
}


def api_key():
    return open(os.path.expanduser("~/.gemini_key")).read().strip()


def generate(prompt, retries=4):
    url = (
        "https://generativelanguage.googleapis.com/v1beta/models/"
        f"{MODEL}:generateContent?key={api_key()}"
    )
    body = json.dumps({
        "contents": [{"parts": [{"text": prompt}]}],
        "generationConfig": {"responseModalities": ["IMAGE"]},
    }).encode()
    for attempt in range(retries):
        try:
            req = urllib.request.Request(url, body, {"Content-Type": "application/json"})
            r = json.load(urllib.request.urlopen(req, timeout=120))
            for p in r["candidates"][0]["content"]["parts"]:
                if "inlineData" in p:
                    return Image.open(io.BytesIO(base64.b64decode(p["inlineData"]["data"])))
            raise RuntimeError("no image part in response")
        except (urllib.error.HTTPError, urllib.error.URLError, RuntimeError, KeyError) as e:
            code = getattr(e, "code", None)
            if attempt + 1 == retries:
                raise
            time.sleep(3 * (attempt + 1) if code in (429, 500, 503) else 2)


def quantize(img, colors=28):
    alpha = img.getchannel("A")
    q = img.convert("RGB").quantize(colors, method=Image.MEDIANCUT).convert("RGB")
    out = q.convert("RGBA")
    out.putalpha(alpha)
    return out


def make_tileable(img, band=3):
    """Crossfade opposite edges so the tile wraps without seams."""
    w, h = img.size
    px = img.load()
    for y in range(h):
        for i in range(band):
            t = (i + 1) / (band + 1)
            a = px[i, y]
            b = px[w - 1 - i, y]
            px[i, y] = tuple(int(a[c] * (0.5 + t / 2) + b[c] * (0.5 - t / 2)) for c in range(4))
            px[w - 1 - i, y] = tuple(int(b[c] * (0.5 + t / 2) + a[c] * (0.5 - t / 2)) for c in range(4))
    for x in range(w):
        for i in range(band):
            t = (i + 1) / (band + 1)
            a = px[x, i]
            b = px[x, h - 1 - i]
            px[x, i] = tuple(int(a[c] * (0.5 + t / 2) + b[c] * (0.5 - t / 2)) for c in range(4))
            px[x, h - 1 - i] = tuple(int(b[c] * (0.5 + t / 2) + a[c] * (0.5 - t / 2)) for c in range(4))
    return img


def chroma_key(img, thresh=110):
    """Magenta background -> transparent, with halo cleanup."""
    px = img.load()
    w, h = img.size
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            # Distance to pure magenta.
            d = ((r - 255) ** 2 + g ** 2 + (b - 255) ** 2) ** 0.5
            if d < thresh:
                px[x, y] = (0, 0, 0, 0)
            elif r > g and b > g and min(r, b) - g > 60:
                # Magenta-tinted halo: pull toward neutral.
                m = (r + b) // 2
                px[x, y] = (m, g, m, a)
    return img


def resize_sprite(img, px_out):
    """Alpha-aware downscale: premultiply, BOX-resize, unpremultiply, then
    binarize alpha for crisp pixel edges (prevents background fringing)."""
    px = img.load()
    w, h = img.size
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            if a < 255:
                f = a / 255.0
                px[x, y] = (int(r * f), int(g * f), int(b * f), a)
    img = img.resize((px_out, px_out), Image.BOX)
    q = img.load()
    for y in range(px_out):
        for x in range(px_out):
            r, g, b, a = q[x, y]
            if a < 100:
                q[x, y] = (0, 0, 0, 0)
            else:
                f = 255.0 / max(a, 1)
                q[x, y] = (min(255, int(r * f)), min(255, int(g * f)), min(255, int(b * f)), 255)
    return img


def process(img, cat):
    img = img.convert("RGBA")
    w, h = img.size
    side = min(w, h)
    if cat == "tile":
        # Crop the center quarter: the model paints ~128 logical pixels,
        # a quarter keeps detail chunky at 32x32.
        c = side // 4
        img = img.crop(((w - 2 * c) // 2, (h - 2 * c) // 2, (w + 2 * c) // 2, (h + 2 * c) // 2))
    else:
        # Faces/sprites are single subjects filling the frame: keep it all.
        img = img.crop(((w - side) // 2, (h - side) // 2, (w + side) // 2, (h + side) // 2))
    if cat == "sprite":
        # Key at full resolution, then alpha-aware downscale — keying after
        # the resize smears magenta into every edge pixel.
        img = chroma_key(img)
        img = resize_sprite(img, OUT_PX)
    else:
        img = img.resize((OUT_PX, OUT_PX), Image.BOX)
    img = quantize(img)
    if cat == "tile":
        img = make_tileable(img)
    return img


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    force = "--force" in sys.argv or bool(args)
    names = args or list(TILES)
    outdir = os.path.join(PACK, "tiles")
    os.makedirs(outdir, exist_ok=True)
    toml = os.path.join(PACK, "pack.toml")
    if not os.path.exists(toml):
        with open(toml, "w") as f:
            f.write('name = "Gemini"\ndescription = "AI-generated tiles (nano banana 2)"\n')
    done = skipped = failed = 0
    for name in names:
        if name not in TILES:
            print(f"?? unknown tile {name}")
            continue
        out = os.path.join(outdir, f"{name}.png")
        if os.path.exists(out) and not force:
            skipped += 1
            continue
        cat, frag = TILES[name]
        styles = {"tile": TILE_STYLE, "face": FACE_STYLE, "head": HEAD_STYLE, "sprite": SPRITE_STYLE}
        prompt = styles[cat].format(frag)
        try:
            img = process(generate(prompt), cat)
            img.save(out)
            done += 1
            print(f"ok {name}")
        except Exception as e:
            failed += 1
            print(f"FAIL {name}: {e}")
        time.sleep(1.0)
    print(f"done={done} skipped={skipped} failed={failed}")


if __name__ == "__main__":
    main()
