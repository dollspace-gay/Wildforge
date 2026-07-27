"""The twelve countries and the shape their spirit wears.

One table drives everything: the tile palettes, the block definitions,
the seed items and the Rust lookup. Hand-maintaining 36 blocks and 12
seeds across four files is how they drift apart.

Archetypes are the three shapes a heart can take — a bole, a spring, a
standing stone. A biome picks one and supplies a palette; the failing
and dead variants are derived from it, so a new country needs two
colours rather than six.
"""

# (biome, archetype, (alive, sick, dead) names, key colour, accent colour)
#
# For a bole the key is bark and the accent is the veins running
# through it. For a spring the key is the water and the accent is the
# rim it wells up through. For a stone the key is the rock and the
# accent is the rune cut into it.
HEARTS = [
    ("Forest", "bole", ("Heartwood Bole", "Failing Heartwood", "Black Heartwood"),
     (96, 74, 46), (128, 236, 150)),
    ("Taiga", "bole", ("Frostpine Bole", "Failing Frostpine", "Black Frostpine"),
     (74, 82, 72), (150, 236, 214)),
    ("Jungle", "bole", ("Canopy Bole", "Failing Canopy", "Black Canopy"),
     (112, 66, 44), (156, 246, 96)),
    ("Swamp", "bole", ("Drowned Bole", "Failing Drowned Bole", "Black Drowned Bole"),
     (78, 84, 62), (126, 214, 160)),
    ("Desert", "spring", ("Wellspring", "Failing Wellspring", "Dry Wellspring"),
     (48, 140, 208), (212, 190, 140)),
    ("Badlands", "spring", ("Ochre Spring", "Failing Ochre Spring", "Dry Ochre Spring"),
     (118, 126, 134), (170, 92, 52)),
    ("Savanna", "spring", ("Waterhole", "Failing Waterhole", "Dry Waterhole"),
     (94, 126, 78), (190, 156, 78)),
    ("Scrubland", "spring", ("Thornwell", "Failing Thornwell", "Dry Thornwell"),
     (96, 176, 176), (120, 128, 92)),
    ("Plains", "stone", ("Standing Stone", "Cracked Standing Stone", "Broken Standing Stone"),
     (128, 124, 140), (198, 178, 255)),
    ("Tundra", "stone", ("Rimestone", "Cracked Rimestone", "Broken Rimestone"),
     (140, 148, 158), (186, 226, 255)),
    ("Arctic", "stone", ("Glacier Stone", "Cracked Glacier Stone", "Broken Glacier Stone"),
     (132, 166, 190), (232, 250, 255)),
    ("Mountains", "stone", ("Summit Stone", "Cracked Summit Stone", "Broken Summit Stone"),
     (104, 100, 104), (214, 200, 236)),
]

# How tall the site stands, by archetype. A bole is a landmark you can
# see across a valley; a spring is a thing you nearly walk past.
HEIGHT = {"bole": 5, "stone": 3, "spring": 1}

# Block properties per archetype: (hardness, tool, light rgb).
ARCHETYPE = {
    "bole": (4.0, "axe", [0.55, 1.0, 0.62]),
    "spring": (3.0, "pickaxe", [0.5, 0.85, 1.0]),
    "stone": (5.0, "pickaxe", [0.75, 0.7, 1.0]),
}

STAGES = ["", "_sick", "_dead"]


def biome_id(biome):
    return biome.lower()


def block_id(biome, stage):
    return f"heart_{biome_id(biome)}{stage}"


def seed_id(biome):
    return f"{biome_id(biome)}_seed"


def seed_name(biome):
    return f"{biome} Seed"


def mix(a, b, t):
    return tuple(int(round(x + (y - x) * t)) for x, y in zip(a, b))


def grey(c):
    v = int(round(0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]))
    return (v, v, v)


def desat(c, t):
    return mix(c, grey(c), t)


def darken(c, t):
    return mix(c, (0, 0, 0), t)


def lum(c):
    return 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]


def separate(key, accent, floor=46.0):
    """Keep the accent readable against the body it is drawn on.

    Draining a colour toward grey drains it toward the thing it sits
    on, so a dying stone's runes and a dying bole's veins simply
    vanished — the sick and dead variants all collapsed into "a rock
    with a crack in it". Push the accent away from the key until it can
    be seen again, darker on a pale body and paler on a dark one.
    """
    gap = lum(accent) - lum(key)
    if abs(gap) >= floor:
        return accent
    toward = (0, 0, 0) if lum(key) > 127 else (255, 255, 255)
    need = (floor - abs(gap)) / 255.0
    return mix(accent, toward, min(1.0, need * 1.6 + 0.18))


def stage_palette(key, accent, stage):
    """Alive, failing and dead, derived from the one authored pair.

    Failing is drained rather than dark — the colour going out of a
    thing reads as illness where darkness alone reads as night. Dead is
    both. The accent still has to be visible at every stage, or the
    dead hearts of four different countries become one grey rock.
    """
    if stage == "":
        return key, separate(key, accent)
    if stage == "_sick":
        k = desat(key, 0.55)
        return k, separate(k, desat(darken(accent, 0.25), 0.6))
    k = darken(desat(key, 0.85), 0.55)
    return k, separate(k, darken(desat(accent, 0.9), 0.45))
