# The hearts arc — what the takers really did

Drafted 2026-07-26. Decisions settled with dollspace: the wild's
resistance gets a body. Every region has a spirit living in a
physical heart; the wardens are that spirit's immune response; and
the reason the takers' cities stand intact in dead country is that
they solved their monster problem permanently — they cut the hearts
out. A dead region is not a curse and not a soft-lock: it is a
place you can walk to and heal, at real cost, the way Regrowth and
Blightfall make reclamation the verb instead of the reward. The
Long Winter is survivable but brutal. And none of this works until
biomes are big enough to be places, so the arc starts in worldgen.

The problem this fixes, stated plainly: **ire is currently a yield
multiplier.** A walled settlement at WRATHFUL loses nothing it is
still using — wardens are masonry's whole purpose, the green tide
is irrelevant to a tree farm, and since the ecology arc the storms
now hand out charred max-fertility soil and blooms. The optimal
play is to provoke the wild and never apologize. That inversion
exists because every consequence the wild has is **martial**, and
martial is exactly what walls answer. The wild needs a sanction
that a wall cannot solve, and the honest one is withdrawal: it
stops giving. Nothing you built is ever touched. Everything you
depended on quietly stops.

The arc's guards:

- **The wild still never touches what players BUILT.** Not one
  block. The heart's death takes the *land's* gifts away, never
  the player's work.
- **Always legible, always reversible.** A dying heart is visible
  for seasons before it is final, named by survey cairns, and the
  road back is always open — long and expensive, never closed.
- **The player may make the takers' mistake.** Cutting a heart is
  possible, effective, and never warned against beyond the wild's
  own whispers. It really does stop the raids. That is the trap,
  and walking into it knowingly is the best story the game has.
- No teleportation, no structure decay, solo always viable — as
  ever. The long walk is the point.

## Stage 1 — provinces (biomes become places)

`climate()` samples temperature and humidity at `x * 0.0026` —
about 385-block features — and `biome_from()` classifies every
column independently by nearest centroid in 4D. Near any decision
boundary the label flips column to column, which is why a forest,
a desert and a taiga can share a walk. Three changes, smallest
first:

- **Slower climate.** Drop t/h to ~`x * 0.0008` (≈1250-block
  features). Continentalness (900) and erosion (700) already run
  slower; this brings the fast fields in line.
- **Provinces.** A jittered-grid Voronoi partition at ~900 blocks:
  each province samples climate ONCE at its site and classifies
  once. That label is the province's biome, full stop — no
  per-column flipping. Site jitter keeps borders organic, not
  square.
- **Transitional edges.** Within ~60 blocks of a province border,
  feature density lerps between the two labels (tree density,
  forage, ground cover) so a forest thins into plains instead of
  ending at a line. The block palette switches at the border; the
  *life* on it fades across.

Mountains, rivers, lakes and coasts stay local overrides on top —
`plate_relief() > 30.0` forcing Mountains is good work and keeps
working. Provinces decide the biome; terrain decides the shape.

A province is then the unit everything else in this arc speaks in:
one heart, one territory, one name. The regional-ire ledger keeps
its 256-block cells and its save format — a province's standing is
the mean of the cells it covers, so nothing migrates.

Cost, honestly: crossing biomes gets much longer, so biome-locked
resources (jungle wood, desert glass sand) become regional
specialities. That is good for the trade arc and hard on the
early nomad; the forage/hunt density is unchanged, so it is a
travel cost, not a survival one.

## Stage 2 — the heart

Every province has a heart: a findable site, not a number.

- **Forms by biome.** Forest/Taiga: a great tree, living wood in
  its trunk, canopy far above the others. Desert/Badlands: a
  spring that should not be there. Mountains: a standing stone.
  Swamp: something half-sunk and best not looked at directly.
  Plains/Savanna: a lone ancient tree over a barrow. Arctic: a
  pillar of blue ice that never melts.
- **Visibly alive.** A faint glow, permanent fertility in a radius,
  the green tide radiating outward from it, ambience thickest
  here. You can tell a healthy heart from a sick one at a glance,
  across a valley.
- **Named and findable.** The survey cairn (and the prospector's
  pick) report the heart's direction, distance, and condition:
  "The heart of this country lies ~300 blocks northwest, and it is
  failing." A player must never lose a valley to something they
  never knew existed.
- The heart holds the province's standing, which is what the
  regional ledger already tracks. Tending near it counts double —
  the wild notices what is done in its sight.

## Stage 3 — the sickening and the silence

Escalation gets a third rung. Violence you can wall out; withdrawal
you cannot.

- **Sickening is slow and loud.** Held at high standing over
  seasons, the heart greys: leaves fall and do not return, the
  spring thins, the glow dims through four visible steps (the
  fertility-tint trick, applied to a landmark). The whispers change
  from grievance to something more tired.
- **The wardens fall silent.** They are the spirit's immune
  response — never persisted, dissolving in daylight, spawned by
  ire. When the heart dies they simply stop coming. The player's
  whole mental model says high ire means brace for a bad night;
  then one night nothing comes, and nothing ever comes again. The
  scariest signal in the game becomes an absence, and it costs no
  new content to deliver.
- **The offering is unreceived.** In a dead province the stone
  stops accepting. Leave a diamond at dusk and it is still there
  at dawn — not refused. Unreceived. Nobody is home.
- **The axe.** A player may kill a heart deliberately, and it
  WORKS: the raids stop that night, forever. No warning beyond the
  whispers, no confirmation dialog. In multiplayer it is a
  region-scale act by one hand (see the open question below).

## Stage 4 — the dead land

What a province without a spirit is:

- **Nothing given.** Fallow soil stops recovering, wildlife stops
  repopulating, the green tide stops, bushes stop fruiting, fish
  leave, the blessed reseed never fires. Crops still grow on soil
  you feed yourself — the ecology arc's whole fertilizer chain is
  what keeps you alive here. Farms work. Wilderness does not.
- **Masterless wardens.** The ones that were mid-existence when the
  heart died were never recalled. No ire gate, no daylight
  dissolution, no purpose — still walking. One flag on the
  existing warden roster, and much eerier than a new monster.
  These are what the dead country has instead of wildlife.
- **The scars stay.** Charred cells never bloom back; the ground
  keeps the record. This is also the fix for the ire-farming
  exploit: **the wild's capacity to heal is finite if you never
  reciprocate.** A cell that blooms repeatedly and is never
  tended blooms weaker each time, then not at all.

## Stage 5 — the long walk back

Restoration is an expedition, and every tool for it already ships.

- **Prepare the ground.** A heart will not root in dead dirt. The
  province's soil must be raised by hand across a wide radius —
  dung, compost, guano, leaf litter, fallow seasons. The entire
  nutrient cycle becomes the endgame verb; your pen and compost
  heap are siege equipment.
- **Carry the seed.** You need something from a LIVING heart: a
  cutting, an ember, a jar of the spring. With no teleportation
  that is a real overland haul from healthy country — and the seed
  is fragile: it needs water, it suffers in winter, it dies if you
  are careless. A journey with something delicate in your pack is
  a shape this game does not have yet and wants.
- **Defend the rooting.** While it takes (a season), the dead land
  pushes back with what is left — the masterless. Not a boss: a
  siege you survive by having built well.

## Stage 6 — reawaken or replace

Two paths out, with different prices and different meanings:

- **Reawaken** the original: slower, dearer, needs the old heart's
  remains intact. The land remembers who did it — permanent
  standing, exceptional fertility. The valley forgives you
  specifically.
- **Replace** with a foreign heart: faster, but what grows back is
  a stranger. It starts neutral, knows nothing of you, and brings
  its own nature — plant a taiga heart in a temperate province and
  the province begins to BECOME taiga over seasons.

Replacement is therefore terraforming, which gives a player a
reason to kill a healthy heart on purpose. That loop is left
deliberately lying around.

## Stage 7 — the Long Winter, and the confession

The global tier, and the answer to what happened.

- **Enough dead hearts and the calendar stops.** Held past a
  threshold of dead provinces worldwide, the year stops advancing
  past winter. Survivable but brutal, and every mechanic already
  exists: winter is crop growth 0.0, a breeding lockout, halved
  repopulation, frozen water. The greenhouse rule (glass roof,
  0.75x) and the whole preservation chain — salt, smoke, pickles,
  the crock, the smoker — stop being conveniences and become the
  thing that decides whether a settlement sees spring.
- **The way out is the walk.** Relight enough hearts and the world
  turns again. That is the endgame campaign.
- **The archaeology confesses.** Ruins bias toward dead and barren
  country, because the barrenness is the receipt — you find the
  lost cities intact in ground that stopped feeding them. The
  etched tablets (the lost takers already speak in this game) tell
  the sequence in their own words: the walls held, the wardens
  came and were farmed, then the axes went to the hearts, then the
  fields were rich for a while, and then spring did not come.

## Touchpoints

- worldgen: climate frequency; province partition + per-province
  classification; transitional edge blending; heart site placement;
  ruin placement biased to dead/barren provinces.
- world: province lookup + standing aggregation over the existing
  256-cell ledger (no save migration); heart state (alive /
  sickening steps / dead) persisted like the bloom ledger; bloom
  exhaustion counter per cell; Long Winter flag on the calendar.
- mobs: warden spawn gated on a LIVING heart; masterless flag
  (no ire gate, no daylight dissolution).
- content: heart blocks per biome form; the seed/cutting/ember
  item (perishable, tended); dead-heart husk variants. Tiles via
  gen_base_tiles.py argv mode only.
- ui/audio: cairn and pick reports name the heart and its
  condition; whispers gain the tired register; the silence needs
  no new audio, only the absence of the presence cue.
- protocol: heart state is host-side; guests see its effects and
  its blocks. No protocol change expected.

## Tests

- Provinces are coherent: a 2000-block walk crosses few biomes,
  and no province is smaller than its floor; borders are irregular
  (not grid-aligned); mountains and rivers still override.
- A heart's standing tracks the mean of its cells; tending near it
  counts double; the cairn names direction, distance, condition.
- Sickening is monotone and visible: four steps, seasons apart,
  reversible at every step by tending.
- On death: wardens stop spawning in that province, the offering
  stone accepts nothing, fallow recovery and repopulation and the
  green tide all stop — and not one player-placed block changes.
  (The guard, as a test.)
- Masterless wardens survive daylight and ignore the ire gate;
  they never spawn in a living province.
- Bloom exhaustion: a cell bloomed repeatedly without tending
  yields less each time and finally nothing; tending resets it.
- Restoration end-to-end: dead province + raised soil + a carried
  seed + a defended season = a living heart, and the wild comes
  back (repop, tide, offerings accepted).
- A replaced heart shifts the province's biome over seasons; a
  reawakened one restores standing permanently.
- The Long Winter arrives on the threshold, a greenhouse still
  grows at 0.75x through it, and relighting hearts ends it.
- Ruins generate biased to dead country; tablets are readable and
  say what happened.

## Stages

1. Provinces — coherent biome regions (the enabler).
2. The heart — sites, forms, legibility, cairn naming.
3. The sickening and the silence — escalation, the axe.
4. The dead land — masterless wardens, sterility, bloom
   exhaustion (this alone closes the ire-farming exploit).
5. The long walk back — ground, seed, rooting.
6. Reawaken or replace — the terraforming fork.
7. The Long Winter and the confession — global tier, archaeology.

Stage 1 must land first and is worth shipping on its own — bigger
biomes are an improvement with or without the rest. Stage 4 is the
balance fix and should not wait long behind it. Screenshot every
new face: a living heart at dusk, the same heart grey, a masterless
warden in daylight, the seed on the road in winter, and the money
shot — a province coming back.

The arc's guard, restated once: the wild never breaks your walls.
It stops feeding you, and the door back is always open — long,
expensive, and open. The takers had that door too. They kept
walking the other way.
