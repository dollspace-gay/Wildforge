# The hearts want edifices

A country's heart is currently a 1–5 block stub at the province centre.
In play that is unfindable: Doll went looking for one and had to grid-
search by hand. This plan makes each one a landmark you can see, and
fixes the reason you couldn't.

## The numbers, first

They decide the whole shape of this.

| | |
|---|---|
| Province spacing | **900 blocks** |
| Default view distance | **7 chunks = 112 blocks** |
| Maximum view distance | **12 chunks = 192 blocks** |
| Heart site today | 1 block (springs), 3 (stones), 5 (boles) |

**No edifice can be seen from the next province.** A pyramid 60 blocks
tall is still invisible at 900 blocks, because the chunks it stands in
are not loaded. So the edifice cannot be the whole answer — it solves
the last ~150 blocks, which is exactly the "I know it's in this valley,
where the hell is it" part Doll actually hit. The first 750 blocks is a
navigation problem and needs its own answer.

Survey cairns already give a bearing, but they are **player-placed** —
raised with a prospector's pick. You have to know they exist and choose
to build one. That is knowledge the game never offers.

## The blocker

`adopt_chunk` registers a heart by taking `surface_height(sx, sz)` and
checking whether the block *there* is a heart. Build anything over the
site and the country registers **no heart at all** — not a dead one, no
heart. Wardens keep spawning, offerings keep being accepted, and the
whole arc quietly does not happen there.

This must be fixed before any edifice exists, and the fix is worth
having regardless: a player who builds a roof over a heart currently
breaks their own country the same way.

**Fix:** search the site column for a heart block over a vertical band
rather than requiring it to be the surface. Cheap, and it makes the
ledger robust to anything standing above.

## The edifices

Twelve countries, and the same discipline as the hearts themselves: one
table drives the parameters, a handful of generator shapes build from
it. Hand-authoring twelve large schematics in TOML is how they rot.

Four families, each parameterised (size, material, palette):

**Bole** — a trunk far thicker and taller than any natural tree, with a
root flare and a canopy above the treeline. The heart block sits at the
foot, in a hollow you can walk into.
- *Forest* — the world tree. The reference case.
- *Taiga* — a frost-rimed spruce, black bark, pale needled crown.
- *Jungle* — a strangler cathedral: a hollow shell of aerial roots.
- *Swamp* — a half-drowned trunk, boardwalk roots over black water.

**Stepped** — a mass of worked stone in courses, the spring or stone in
a chamber at its heart, an entrance on one face.
- *Desert* — the pyramid.
- *Badlands* — a collapsed ziggurat. Always dead, so build it broken:
  the receipt made monumental.
- *Savanna* — a low earthen mound over the waterhole.
- *Scrubland* — a squat thorn-tower of stacked slabs.

**Ring** — pillars around the standing stone, open to the sky.
- *Plains* — a henge.
- *Tundra* — a dolmen field, rimed.

**Cap** — something that crowns terrain rather than sitting on it.
- *Arctic* — a glacier arch of ice over the stone.
- *Mountains* — terraces cut into the summit.

### Scale

Big enough to clear what surrounds it. A world tree that does not top
the forest canopy is not a landmark. Working figure: **crown or crest
30–45 blocks above local ground**, footprint 15–25 blocks. That is
visible across most of a loaded view radius, from most angles.

## What must stay true

- **The axe still works.** The heart remains reachable and cuttable —
  that trap is the centre of the arc. An edifice is a wrapper, never a
  lock. Every one has a way in that needs no tools.
- **The site is still the province centre**, so cairns, `country_biome`,
  restoration and the rooting radius are unaffected.
- **Badlands stay born-dead**, and their edifice should read as ruined
  from the first glance.
- **No new blocking cost at chunk generation.** These are large, and
  worldgen already owns the frame budget. They generate once, at the
  chunk that holds the centre, like the heart does now.

## The other 750 blocks

The edifice does nothing for a player who does not know which way to
walk. Options, in rough order of how much I like them:

1. **A gradient in the world.** Something grows denser the closer you
   are — a flower, a fungus, birdlife. Readable while walking, no UI, no
   map. Fits a game whose whole idiom is "the land tells you".
2. **The takers' roads.** Ruins already bias to barren country; old
   waymarkers along a road that once ran between hearts would make
   archaeology into navigation.
3. **Cairns you find rather than build.** Weathered ones already
   standing, which read when clicked.
4. **A sky signal** — a plume or light column drawn beyond block
   distance. Most effective, least in keeping.

## Decided

- **Edifices plus a gradient.** Something in the world thickens as you
  near a heart, so the walk between provinces is navigable without a
  map or a marker. The land tells you.
- **The stepped ones are hollow**, with a mouth on one face, a passage,
  and the heart in a chamber. Walking in to reach a spirit is a better
  moment than digging through one. Tablets and offerings gain somewhere
  to belong.
  - **The ambush is intended** (settled 2026-07-27). A sheltered dark
    interior is exactly where the hostile spawner wants to work, and
    that is the point: walking into a living country's heart to take a
    cutting should cost you something. Do not light these chambers and
    do not veto spawns in them.
  - **A dead chamber is empty, and that is correct** (settled
    2026-07-27). Wardens gate on `heart_alive_at`, so they never come
    in dead country — which makes the badlands ziggurat the safest
    room in the game while the living countries' chambers bite. That
    reads backwards until you remember what the silence means: the
    wardens falling quiet IS the death knell, and a ruin with nothing
    guarding it is the loudest way to say nobody is home. Do not add
    danger to dead chambers by any means. The living ones are where
    the teeth belong.

## The other constraint: edifices cross chunks

Heart placement today writes into the single chunk buffer being
generated, which is fine for a 5-block stub inside one chunk. A 20-block
footprint spans up to four, and a chunk cannot write into its
neighbours — anything written into an unloaded chunk is lost, and
anything written twice is a seam.

So an edifice **cannot be a stamping routine**. It has to be a pure
function of `(world position, site, params) -> Option<BlockId>`, which
every chunk evaluates for its own columns. Each chunk asks "is any
province centre within reach of me?" and fills only what falls inside
itself. Deterministic, seamless, and no ordering between chunks.

This suits the four families: a pyramid, a ring of pillars, a trunk
with a canopy and a terraced cap are all naturally expressed as
"what block belongs at this offset from the centre".
