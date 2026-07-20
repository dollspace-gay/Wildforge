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
    + "no frames, no borders, no white bars, no other background colors, no shadows. "
    + "The ENTIRE object is fully visible and small in the frame with generous "
    + "magenta margin on every side — never zoomed in, never touching or cut off "
    + "by the image edges. {}"
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
    "antler": ("face", "pale cream deer antler bone surface with faint darker ridges"),
    "torch": ("plant", "a wooden torch: brown stick with a bright orange-yellow flame at the top"),
    "oak_sapling": ("plant", "a tiny young oak tree sapling with a few green leaves"),
    "birch_sapling": ("plant", "a tiny young birch sapling, pale trunk, light yellow-green leaves"),
    "spruce_sapling": ("plant", "a tiny young spruce sapling, dark green needle tufts"),
    "jungle_sapling": ("plant", "a tiny young tropical sapling with vivid green fronds"),
    "acacia_sapling": ("plant", "a tiny young acacia sapling with a flat olive-green crown"),
    "offering_stone": ("tile", "mossy grey stone altar surface with a shallow bowl glowing faint pale green in the center"),
    "bedroll": ("sprite", "a rolled-up brown leather hide bedroll tied with green fiber cords"),
    # wardens
    "thornling": ("face", "dark bristly shrub hide, dense green-black twigs with pale thorn tips"),
    "dryad": ("face", "mossy ancient bark with green lichen veins"),
    "dryad_face": ("head", "two amber glowing eyes and a thin dark slit mouth. Mossy ancient bark."),
    "emberkin": ("face", "charred black crust cracked open with glowing orange-yellow fire lines"),
    "rimewisp": ("face", "pale ice-blue swirling frost mist texture"),
    "gravelurk": ("face", "cracked grey granite stone hide with deep dark fissures"),
    "wrathwood": ("face", "gnarled ancient dark oak bark, deeply furrowed"),
    "wrathwood_face": ("head", "two small burning orange eyes and a huge jagged dark maw lined with wooden teeth. Gnarled dark bark."),
    "hunting_bow": ("sprite", "a simple wooden hunting bow with a taut string, drawn curve facing left"),
    "warbow": ("sprite", "a recurve war bow carved from green-veined living wood with a taut string"),
    "arrow": ("sprite", "a single arrow: wooden shaft, grey stone tip, white feather fletching, diagonal"),
    "leather_helmet": ("sprite", "a simple brown leather cap helmet"),
    "leather_chestplate": ("sprite", "a brown leather tunic chestplate with stitching"),
    "leather_leggings": ("sprite", "brown leather trouser leggings"),
    "leather_boots": ("sprite", "a pair of brown leather boots"),
    "bronze_helmet": ("sprite", "a polished golden-bronze metal helmet"),
    "bronze_chestplate": ("sprite", "a polished golden-bronze metal chestplate with rivets"),
    "bronze_leggings": ("sprite", "golden-bronze metal plate leggings"),
    "bronze_boots": ("sprite", "a pair of golden-bronze metal boots"),
    # Player tiles are tint bases: near-greyscale, multiplied by each
    # player's chosen palette at atlas build (style.rs). The script
    # desaturates them after generation (GREY_TILES below); paint form
    # and shading, not color. The face must read gender-neutral.
    "player_shirt": ("face", "plain woven fabric with soft fold shading, light grey, flat clothing texture"),
    "player_face": ("head", "a neutral friendly androgynous face, epicene, simple dark eyes, tiny subtle mouth, NO facial hair, NO hair on the head, plain pale grey-toned skin"),
    "player_skin": ("face", "plain smooth pale grey skin texture, very subtle tonal variation, flat"),
    # (player_hair / player_hair_top stay procedural: they need an
    # alpha-cut fringe the model can't paint reliably.)
    "player_trousers": ("face", "plain woven trouser fabric, light grey, a single vertical seam line, flat clothing texture"),
    "player_boot": ("face", "worn brown leather boot texture with a darker sole strip along the bottom edge, flat"),
    "mossy_cobblestone": ("tile", "grey cobblestone heavily overgrown with green moss patches"),
    "cracked_masonry": ("tile", "old dressed stone brickwork with deep jagged cracks"),
    "packed_earth": ("tile", "dark trodden packed earth floor with small stones"),
    "old_coin": ("sprite", "a single worn ancient gold coin with a faded face"),
    "etched_tablet": ("sprite", "a small grey stone tablet carved with rows of unreadable runes"),
    "charm_quiet": ("sprite", "a small woven ring talisman of green vines and feathers on a cord"),
    "charm_bark": ("sprite", "a small carved bark disc talisman on a fiber cord"),
    "charm_hunger": ("sprite", "a small amber bead talisman on a fiber cord"),
    "iron_ore": ("tile", "grey stone with embedded dull silver-grey iron nuggets"),
    "iron_block": ("tile", "polished silver-grey iron metal block panel"),
    "steel_block": ("tile", "polished bright blue-silver steel metal block panel"),
    "raw_iron": ("sprite", "rough lump of raw grey-brown iron ore"),
    "iron_ingot": ("sprite", "cast silver-grey iron metal ingot bar"),
    "steel_ingot": ("sprite", "cast bright blue-silver steel metal ingot bar"),
    "iron_pickaxe": ("sprite", "silver-grey iron pickaxe with stick handle, diagonal"),
    "iron_axe": ("sprite", "silver-grey iron hatchet axe, diagonal"),
    "iron_shovel": ("sprite", "silver-grey iron shovel, diagonal"),
    "iron_hoe": ("sprite", "silver-grey iron farming hoe, diagonal"),
    "iron_sword": ("sprite", "silver-grey iron shortsword with crossguard, blade up-right"),
    "steel_pickaxe": ("sprite", "bright steel pickaxe with stick handle, diagonal"),
    "steel_axe": ("sprite", "bright steel hatchet axe, diagonal"),
    "steel_shovel": ("sprite", "bright steel shovel, diagonal"),
    "steel_hoe": ("sprite", "bright steel farming hoe, diagonal"),
    "steel_sword": ("sprite", "bright steel longsword with crossguard, blade up-right"),
    "iron_helmet": ("sprite", "a silver-grey iron metal helmet"),
    "iron_chestplate": ("sprite", "a silver-grey iron metal chestplate"),
    "iron_leggings": ("sprite", "silver-grey iron plate leggings"),
    "iron_boots": ("sprite", "a pair of silver-grey iron boots"),
    "steel_helmet": ("sprite", "a bright polished steel helmet"),
    "steel_chestplate": ("sprite", "a bright polished steel chestplate"),
    "steel_leggings": ("sprite", "bright polished steel plate leggings"),
    "steel_boots": ("sprite", "a pair of bright polished steel boots"),
    "shears": ("sprite", "a pair of iron shears with two crossed blades"),
    "excavation_brush": ("sprite", "an archaeologist's brush: wooden handle with soft pale bristles, diagonal"),
    "thorn_bolt": ("sprite", "a sharp green thorn spike, diagonal"),
    "ember_bolt": ("sprite", "a small blazing fireball with an orange-yellow core"),
    "frost_bolt": ("sprite", "a sharp blue ice shard, vertical"),
    "plant_fiber": ("sprite", "a coiled loop of tough green plant fiber cord"),
    "living_wood": ("sprite", "a piece of wood with glowing green sap veins"),
    "ember": ("sprite", "a lump of charcoal with glowing orange cracks, radiating heat"),
    "snowball": ("sprite", "a single round packed snowball, white with soft blue shading, centered with margin on all sides"),
    "glass": ("sprite", "a clear glass window pane: thin pale frame around the edge, two faint diagonal white glints, everything else pure magenta background"),
    "cobalt_ore": ("tile", "grey stone with clusters of deep blue cobalt crystal nuggets"),
    "cinnabar_ore": ("tile", "grey stone with clusters of vivid vermilion red cinnabar crystal"),
    "manganese_ore": ("tile", "grey stone with clusters of dark violet manganese crystal"),
    "verdigris_powder": ("sprite", "a small heap of blue-green verdigris pigment powder, centered with margin"),
    "ochre_powder": ("sprite", "a small heap of warm yellow-brown ochre pigment powder, centered with margin"),
    "cobalt_powder": ("sprite", "a small heap of rich blue cobalt pigment powder, centered with margin"),
    "cinnabar_powder": ("sprite", "a small heap of bright red cinnabar pigment powder, centered with margin"),
    "manganese_powder": ("sprite", "a small heap of violet manganese pigment powder, centered with margin"),
    "teal_glass": ("sprite", "a teal stained-glass pane: thin darker teal frame, two faint diagonal glints, interior a flat translucent-looking teal, on the background color"),
    "amber_glass": ("sprite", "an amber stained-glass pane: thin darker amber frame, two faint diagonal glints, interior flat warm amber, on the background color"),
    "blue_glass": ("sprite", "a blue stained-glass pane: thin darker blue frame, two faint diagonal glints, interior flat deep blue, on the background color"),
    "red_glass": ("sprite", "a red stained-glass pane: thin darker red frame, two faint diagonal glints, interior flat rich red, on the background color"),
    "violet_glass": ("sprite", "a violet stained-glass pane: thin darker violet frame, two faint diagonal glints, interior flat violet, on the background color"),
    "kiln": ("tile", "red-brown firebrick wall with a wide dark rectangular kiln slot in the center, cold"),
    "kiln_lit": ("tile", "red-brown firebrick wall with a wide kiln slot glowing white-gold hot"),
    "snow_trod": ("tile", "bright white snow surface with two pressed bootprints sunken in, blue-grey shadowed prints"),
    "quern": ("tile", "a round grey millstone seen from above: center eye hole and curved sweep grooves"),
    "firebrick": ("tile", "deep red-brown refractory bricks with dark mortar seams, kiln-worn"),
    "bloomery": ("tile", "red-brown firebrick wall with a dark arched furnace mouth in the center, cold and sooty"),
    "bloomery_lit": ("tile", "red-brown firebrick wall with an arched furnace mouth glowing bright orange from within, embers"),
    "charcoal_block": ("tile", "a wall of packed black charcoal chunks, matte with faint grey ash"),
    "stone_anvil": ("tile", "worn grey stone anvil top seen from above, a darker hammered band across the middle"),
    "steel_bloom": ("sprite", "a spongy grey-silver lump of raw bloomery steel streaked with dark slag, centered with margin"),
    "smith_hammer": ("sprite", "a stout smithing hammer with grey iron head and short wooden handle, diagonal, centered with margin"),
    "rain_streak": ("sprite", "three thin vertical pale blue-grey rain streaks, faint and translucent-looking, on the background color"),
    "snow_flake": ("sprite", "five small soft white snowflake dots scattered sparsely, on the background color"),
    "frost_shard": ("sprite", "a jagged pale blue ice crystal shard"),
    "heartwood": ("sprite", "a dense dark red-brown wooden core piece, ancient and heavy"),
    "chest_side": ("tile", "wooden chest side panel: warm planks with a dark frame border and a grey metal latch at the top center"),
    "chest_top": ("tile", "wooden chest lid seen from above: warm planks with a dark frame border"),
    # crops & plants: sprite-keyed, then bottom-aligned so they sit on soil
    "wheat_young": ("plant", "a few short young green wheat sprouts growing from the bottom edge"),
    "wheat_ripe": ("plant", "tall ripe golden wheat stalks with heavy heads, growing from the bottom edge"),
    "carrot_plant": ("plant", "leafy green carrot tops with orange carrot crowns peeking at the soil line"),
    "potato_plant": ("plant", "low bushy green potato plant with tiny white blossoms"),
    "bush_fruited": ("plant", "small leafy berry bush dotted with ripe red berries"),
    "bush_bare": ("plant", "small leafy green bush with no berries"),
    "mushroom": ("plant", "single brown forest mushroom with pale stem"),
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
            if d < thresh or (min(r, b) > max(g, 1) * 1.6 and min(r, b) > 60 and abs(r - b) < 80):
                # Pure key color, or strongly magenta-dominant (models
                # sometimes drift the background hue).
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


# Player tint bases are desaturated after generation (style palettes
# multiply over them at atlas build).
GREY_TILES = {"player_shirt", "player_face", "player_skin", "player_trousers"}


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
    if cat in ("sprite", "plant"):
        # Key at full resolution, then alpha-aware downscale — keying after
        # the resize smears magenta into every edge pixel.
        img = chroma_key(img)
        img = resize_sprite(img, OUT_PX)
        if cat == "sprite":
            # Frame-filling art reads as a cropped zoom in inventory slots;
            # cap sprites at ~75% of the tile and center them.
            bbox = img.getbbox()
            if bbox:
                w, h = bbox[2] - bbox[0], bbox[3] - bbox[1]
                if max(w, h) > 26:
                    crop = img.crop(bbox)
                    scale = 24 / max(w, h)
                    nw = max(1, round(w * scale))
                    nh = max(1, round(h * scale))
                    crop = crop.resize((nw, nh), Image.NEAREST)
                    img = Image.new("RGBA", (OUT_PX, OUT_PX), (0, 0, 0, 0))
                    img.paste(crop, ((OUT_PX - nw) // 2, (OUT_PX - nh) // 2))
    else:
        img = img.resize((OUT_PX, OUT_PX), Image.BOX)
    img = quantize(img)
    if cat == "tile":
        img = make_tileable(img)
    if cat == "plant":
        # Cross-rendered blocks grow from the block floor: shift the art
        # down so its lowest opaque row touches the bottom edge.
        px = img.load()
        lowest = 0
        for y in range(OUT_PX - 1, -1, -1):
            if any(px[x, y][3] > 0 for x in range(OUT_PX)):
                lowest = y
                break
        shift = (OUT_PX - 1) - lowest
        if shift:
            moved = Image.new("RGBA", (OUT_PX, OUT_PX), (0, 0, 0, 0))
            moved.paste(img, (0, shift))
            img = moved
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
        styles = {
            "tile": TILE_STYLE,
            "face": FACE_STYLE,
            "head": HEAD_STYLE,
            "sprite": SPRITE_STYLE,
            "plant": SPRITE_STYLE,
        }
        prompt = styles[cat].format(frag)
        try:
            img = process(generate(prompt), cat)
            if name in GREY_TILES:
                # Tint bases: strip the model's color drift, keep form.
                a = img.getchannel("A")
                img = img.convert("L").convert("RGBA")
                img.putalpha(a)
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
