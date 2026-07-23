# Minerals & geology — the ground earns its keep

Drafted 2026-07-23, **IMPLEMENTED** 2026-07-23, all eight stages.
Notes vs. this spec, where the implementation knew better:

- **Crucibles fire in the furnace**, not the kiln — a one-line smelt
  beside new kiln plumbing wasn't a contest; any hot chamber fires a
  cupel. The kiln's colorant slot, though, took over more than
  planned: **raw gold**, the **silver ingot**, and the **lead
  ingot** ride it directly (cranberry, yellow, crystal) — so yellow
  glass and crystal both genuinely require the cupellation chain.
- **Magma pockets are sealed chambers**, not open lava caves: where
  the deep cheese noise merely swells, the rock holds lava you mine
  into — better drama, and the open-cave threshold almost never
  fired below y 11 anyway.
- **Crystal clarity is the tile's alpha** (45 vs ~90 for common
  glass) plus the blended pass's existing sun glint — no shader
  work needed.
- Both rare-earth ores grind to one shared powder as planned;
  bastnasite drops its own lump (richer per grind than monazite).
- Volcanoes keep the existing snowcap rule at altitude — a
  snow-shouldered cone with a glowing crater reads exactly right.
- The base mod now ships PNG tiles through the standard mod-texture
  pipeline (`base/textures/`, `tools/gen_base_tiles.py`, 62 tiles),
  which also lifted a stale 256-slot mod-texture cap from the
  16-wide atlas era. `ModInfo.path` for base points at the repo's
  `base/` dir (the game runs from the repo root; README says so).
- Geode/pipe/volcano locators (`pipe_at`, `geode_at`,
  `volcano_near`) are pub so tests find structures without
  generating hundreds of chunks.
- The remaining aspirational screenshot (a cracked geode in situ)
  is deferred to reveal material; strata terraces, the volcano
  cone with its glowing crater, and the glowglass chamber shipped.

Drafted 2026-07-23. Decisions settled with dollspace: **full strata**
(real rock families replace uniform stone), **finite lava this pass**
(second fluid on the water engine's rails), **diamonds are craft
treasure, not a tool ladder** (the tech identity stays
bronze/iron/steel), **uranium and glowing uranium glass are in**,
**rare earths are in** (with honest hosts and a deliberately hard
refining story), **galena carries both lead and silver** — the
split is real cupellation through quartz crucibles — and **leaded
crystal glass is its own line**, not another color.

**This update is a precursor to the tech tree.** Glass is the
showcase, not the point: the materials landing here — lead, silver,
rare earths, sulfur, quartz, coal — are the feedstock a future
technology arc will consume (optics, electrics, chemistry). They
ship now with modest, honest uses so the world is already rich when
that arc arrives; nothing gets a placeholder machine it doesn't
need yet.

The state this plan fixes: six ores drunk-walk through featureless
stone, gated only by depth bands — nothing about *where* you dig
matters except how far down. The glass palette (teal, amber, blue,
red, violet from five quern powders) proved the colorant chain
works; this update makes it the showcase of a world whose minerals
live where geology would put them. The stated objective: get the
minerals out there, and make their distribution readable — ore in
sane host rocks, diamonds in kimberlite pipes, volcanoes on the map.

## Strata: rock families, not "stone"

"Geologically sane" needs host rocks to be sane about. The interior
of the world stops being uniform:

- **Sedimentary stack** (upper crust): horizontal bands warped by
  low-frequency noise and climate — **sandstone** (upper, thicker in
  dry country), **limestone** (middle), **shale** (lower, thicker in
  wet). Band boundaries drift so cliffs and cave walls show real
  bedding.
- **Basement**: familiar `base:stone` below the stack — old worlds
  still read as themselves at depth.
- **Granite intrusions**: 3D noise bodies (sampled on the existing
  worldgen lattice, so cost stays flat) that widen with depth and
  punch up through the sediments as plutons.
- **Contact metamorphism**: where sediment sits within a few blocks
  of granite (the near-threshold noise band, no extra sampling),
  limestone becomes **marble**, shale becomes **slate**, sandstone
  becomes **quartzite**.
- **Deep basalt**: a flood layer under everything, and the body rock
  of volcanic provinces.

Every rock is a real block with drops, hardness, and a brick recipe
— the strata rework doubles as the builder's-palette update
(granite, marble, slate, basalt, limestone, sandstone). Prospecting
is the gameplay: strata visible in cliffs and caves teach where to
dig; nothing needs a manual.

**Migration honesty**: already-generated chunks keep their saved
blocks; strata appear in newly generated terrain. Old/new seams at
the boundary are accepted (they read as odd cliffs, not corruption).

## Finite lava: the second fluid

Generalize the finite-water machinery rather than clone it: the
registry's fluid plumbing (`water = N` in blocks.toml auto-chains
flow variants) grows a **lava chain** with its own queue, a slower
cadence (~1 Hz vs water's 5), and a stiffer equalize hysteresis
(3 units — lava creeps, water rushes). Same conservation law: volume
moves, never multiplies.

- **Light**: lava emits (flood-fill `light_emit` ~13, warm
  `light_color`) and its surface renders full-bright through the
  blended pipeline — the point-light Director treats bright cells as
  ordinary emitter candidates, nearest-N as ever.
- **Water contact**: full lava touched by water hardens to
  **obsidian**; partial lava hardens to **basalt**. The touching
  water volume boils away — one documented exception to water
  conservation (the steam left).
- **Burn**: standing in lava damages fast, sets the damage vignette;
  dropped items in lava are destroyed.
- **Bucket**: the iron bucket scoops full lava cells exactly like
  water (`C2S::Scoop` already ships; `water_volume` generalizes to
  fluid volume). Pouring rides Place. No protocol bump anywhere in
  this update — blocks and items are data, and content-hash
  streaming updates guests automatically.
- **Worldgen**: settled lava pockets replace some deep cheese caves
  below y≈10 (magma chambers), and volcano craters pool it.

## Volcanoes

A dedicated low-frequency province noise marks volcanic country;
each province rolls a deterministic center on a coarse region grid
(cross-chunk consistent, same trick as climate). Within it:

- The terrain spline offset gains a **cone** shaped by distance to
  the center; body rock is basalt.
- The summit carves a **crater** with a finite **lava pool**, an
  **obsidian rim**, and **magma vents** (light-emitting block) in
  the walls.
- Flanks carry **sulfur** patches and ashy basalt gravel; copper
  runs richer in volcanic country (porphyry).

## Kimberlite pipes & geodes

- **Pipes**: a rare per-chunk deterministic roll (~1 in 400) plants
  a carrot-shaped **kimberlite** column — wide (~5 radius) near the
  surface, tapering to ~1 at depth — cutting through every stratum.
  **Diamonds exist only inside pipes**, sprinkled richer with depth.
  Most pipes are blind (topped below the surface); some breach and
  weather into a subtle **blue-ground** soil stain — the prospector's
  tell.
- **Geodes**: small hollow spheres seeded in limestone: a shell of
  rough quartz around a lining of **amethyst** and **quartz crystal**
  with a void at heart. Rare, luminous when you crack one open with
  a torch in hand.

## The mineral roster

New (host rock in parentheses): **coal** (flat seam lenses in shale
and limestone — found fuel, and the bloomery accepts it beside
charcoal), **gold** (native flecks inside visible white **quartz
veins** streaking basement and granite contact), **galena**
(limestone contact zones — lead sulfide carrying silver; see the
cupellation section), **chromite** (deep basalt, near kimberlite
country), **sulfur** (volcanic), **quartz** (veins and geodes),
**diamond** (kimberlite only), **pitchblende** (deep granite,
tier-6 gated, faintly glowing), and the rare earths: **monazite
sand** (placer seams in beach and riverbed sands — easy to find,
stubborn to use) and **bastnäsite** (hosted in **carbonatite**
dikes, the alkaline intrusion rock of volcanic country — richer,
deeper, geologically true).

Existing six get re-hosted without breaking progression — **the
bronze age must stay findable by a player who's never read a
geology book**: copper stays broad (richer in volcanic provinces),
tin concentrates at granite margins but keeps a generous range,
iron bands through the sedimentary stack, cobalt/cinnabar/manganese
keep their depths but prefer sane hosts. Feature defs grow a
`shape` field: `walk` (today's), `seam` (flat lens), `streak`
(near-vertical vein, used by quartz+gold).

**Diamond as craft treasure**: a diamond tips a steel pick into the
**diamond-tipped pick** — the single tier-6 gate (pitchblende needs
it) — and cuts glass: fine-glass recipes (lenses, prisms) open only
with a diamond in the grid. High offering value. No diamond armor,
no diamond sword, ever.

**Uranium**: pitchblende grinds to uranium powder at the quern;
handling is lightly cursed (raw pitchblende in your inventory
pauses natural regen — a whisper, not a mechanic). The kiln turns
it into **glowglass**: emissive green panes (`light_emit` ~7, green
`light_color`) that glow in the dark and tint every beam that
passes through them — the transmission-cube system already does the
work.

**Rare earths**: the refining story stays deliberately shallow this
pass — real separation chemistry is brutal, and that difficulty IS
the flavor. The quern grinds either ore into **rare-earth powder**
(mixed, unseparated — mischmetal in spirit); its one showcase use
today is **rose glass** in the kiln (neodymium-pink, faintly
shifting). True element separation is explicitly the future tech
tree's problem; the deposits and the powder exist now so that arc
lands in a world already holding them.

## Cupellation: the lead–silver split

Galena is lead ore that hides silver, and getting the silver out is
its own craft — as in life:

- A plain furnace smelt of galena yields a **lead ingot**; the
  silver stays locked inside (the flavor text says as much).
- **Quartz crucibles** are kiln-fired from quartz — a new kiln
  product beside glass batches.
- **Charging**: a grid recipe packs galena into a crucible (a
  **charged crucible** item). Smelting the charged crucible runs
  cupellation: the furnace's output slot yields the **silver
  ingot**, and the **lead pours out the mouth** — spat as item
  drops at the furnace face, via a small smelt-def extension
  (`spits`) that rides the existing pending-drops path (already
  multiplayer-clean). The crucible is consumed: cupels are
  single-fire ceramics, and single-use keeps the loop honest
  without durability plumbing.
- Net: galena → lead every time; galena + crucible → silver AND
  lead. No silver without quartz, which means no silver without
  reading the land for veins or geodes.

**Lead** is a first-class material, not a byproduct footnote:
leaded crystal (below), and it's seeded for the tech tree
(shielding for the uranium story, plumbing, solder — none built
now, all plausible later).

## Glass, the showcase

Six new kiln colors from real chemistry, joining the five shipped:
**emerald green** (chrome powder), **cranberry** (gold — colloidal
gold really does that), **bright yellow** (silver), **milk glass**
(tin powder, opaque white), **rose glass** (rare-earth powder,
neodymium-pink), **glowglass** (uranium, emissive). Eleven colors,
every one a mineral you dug from somewhere that made sense.

**Leaded crystal is its own line, not a color.** Lead ingots join
sand in the kiln to fire **crystal glass** — clearer, brighter,
with a glint the common batch doesn't have (higher transparency and
a stronger sun sparkle in the blended pass). Crystal is the medium
of **fine glasswork**: the lens and prism recipes take crystal
glass cut with a diamond, which braids the three threads — quartz
buys the crucible, the crucible buys the silver and frees the lead,
the lead buys the crystal, the diamond cuts it. That chain is the
tech tree's doorstep: optics starts here.

## Textures

~30 new tiles (rocks, ores, crystals, lava, ingots, powders, five
glasses). Base ships them as PNG tiles through the standard mod
texture pipeline (`base/textures/`), generated by a procedural PIL
script in `tools/` so the default look is complete without any API
key; regenerating gemini-pack versions is a follow-on flag (the
existing `tools/gen_texture_pack.py` flow, run on request).

## Touchpoints

- `src/worldgen.rs`: `rock_at` strata sampling on the existing
  lattice; province/cone/crater; **carbonatite dikes** in volcanic
  country; pipe + geode carves; magma pockets; feature shapes in
  `plant_ores` (note: monazite seams replace **sand**, not stone —
  the first non-stone-host feature, `replaces` already supports it).
- `src/registry.rs` + `base/blocks.toml`: rock/ore/crystal blocks,
  lava chain (generalized fluid registration), feature `shape`.
- `src/world/fluids.rs`: lava queue + cadence + hysteresis; the
  water-contact hardening; burn hooks in `physics.rs`/survival.
- `src/game/`: bucket paths generalize to fluid; pitchblende regen
  whisper; fine-glass recipes gate on diamond.
- `src/registry.rs` smelt defs: optional `spits` (byproduct item
  drops at the furnace mouth via pending_drops) for cupellation.
- `base/recipes.toml`: smelts (gold, galena→lead, charged
  crucible→silver+spit lead), quern powders (chrome, tin, uranium,
  rare-earth), kiln products (six colors, crystal glass, quartz
  crucibles), bricks, diamond-tipped pick, fine glasswork (lens,
  prism from crystal + diamond).
- `tools/`: procedural tile generator for the new art.

## Tests

- Strata: deterministic per seed; band coverage sane over a sample
  region (sedimentary above basement, marble only near granite).
- Host gating: sampled chunks show coal only in shale/limestone,
  diamonds only in kimberlite, gold only inside quartz streaks.
- Pipes: rarity within tolerance over many chunks; carrot profile.
- Lava: conservation in a sealed basin; slower cadence honored;
  water-contact matrix (full→obsidian, partial→basalt, water
  consumed); burn damage ticks; bucket scoop/pour round-trip;
  loopback lava edit sync.
- Volcano: province cone rises above baseline terrain; crater pool
  settles and stays (no drain-away); vents emit light.
- Cupellation: plain galena smelt yields lead only; the charged
  crucible yields silver in the slot AND spits lead at the mouth;
  the crucible is consumed; the content graph proves silver is
  unreachable without quartz.
- Rare earths: monazite seams appear only in sand; bastnäsite only
  in carbonatite; both grind to the same powder; rose glass fires.
- Glass: content-graph proves every new item obtainable in
  survival; glowglass emits (light_at) and tints (transmission);
  crystal glass renders clearer than common glass (alpha census);
  kiln maps all eleven colorants plus crystal and crucibles.
- Atlas: every new block/item resolves a tile (no warnings).
- Screenshots: a strata cliff at dawn; a volcano crater at night
  (lava glow + point shadows); a glowglass window tinting a beam;
  a cracked geode.

## Stages

1. **Strata worldgen** — rock_at bands + intrusions + contact zones
   + deep basalt; rock blocks, drops, bricks; cliff screenshots.
2. **Finite lava** — fluid generalization, light, water hardening,
   burn, bucket; magma pockets; conservation tests.
3. **Volcanoes** — province noise, cone, crater + pool, rim, vents,
   sulfur flanks.
4. **Pipes & geodes** — kimberlite carrots + diamonds + blue
   ground; limestone geodes + amethyst/quartz.
5. **Roster & redistribution** — new ores placed (incl. monazite
   sand seams + carbonatite bastnäsite), old six re-hosted, feature
   shapes; progression-safety balance pass.
6. **Cupellation & crucibles** — quartz crucibles at the kiln, the
   galena→lead smelt, charged-crucible cupellation with the `spits`
   extension, lead as a material.
7. **Processing & glass** — smelts, powders, eleven kiln colors,
   leaded crystal glass + its render clarity, glowglass emission,
   rose glass, coal fuel, diamond-tipped pick, fine glasswork
   (lens, prism), offering values, pitchblende whisper.
8. **Docs & screenshots** — README section, plan marked
   IMPLEMENTED, full gate green.
