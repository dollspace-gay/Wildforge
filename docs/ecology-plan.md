# The ecology arc — the cycle closes

Drafted 2026-07-25. Decisions settled with dollspace: a realistic
trophic layer and nutrient cycle that reaches all the way into
agriculture; ire as retribution AND regrowth (the titan levels the
valley, the jungle erupts behind it); aquatic life; native fauna
for every biome — full roster and then some, not all of it useful
to anyone; fungi creeping through dark damp places; grass healing
over bare dirt; dung and guano in the fertilizer bag. Wildlife
raids crops when hungry and the crops are the nearest richest food
— which they will tend to be. Predators threaten the player only
when hungry and desperate, like real predators — except the polar
bear, who loves murder and will maul you on principle.

Today the living world is a stage set with one actor: the player
taps every arc of the cycle from outside, but nothing in it feeds
anything else. Animals never eat, age, or die untouched; soil never
tires; leaves never fall; the water is empty; five biomes are
silent. This arc closes the loop so it runs WITHOUT the player:

    sun → plants → grazers → predators → carrion/dung → soil → plants
                                  ↑
              storms (the wild's own hand) feed the soil too

The arc's guards, stated up front:

- **The wild never touches what players BUILT.** Lightning strikes
  only natural ground. Raiding wildlife eats crops (a planted crop
  is food standing in a field, and a deer is a deer) but never
  breaks or climbs a placed block — any continuous one-block wall
  or fence turns them, deliberately: pens and fences are the
  answer, and permanence is rewarded.
- **Ire only hears the player's hand.** Predation, carrion rot,
  natural death, lightning — nature's own violence and gifts are
  ire-neutral. Killing ANY wildlife, fang or fluff, still charges
  +2.0 as it always has. The meter stays a moral instrument, not
  a physics readout.
- **Nomad valid-but-struggling, civilization encouraged.** The
  settler's fertilizer is the pen (steady dung); the nomad's is
  the cave (found guano) and the rod (fish anywhere there's
  water). Both live; one compounds.
- No structure decay, no recipe locks, solo always viable — as
  ever.

## Stage 1 — living soil (fertility, and the ground heals)

Every block already carries a meta byte (`substrate.rs` uses it
for sand octants; it rides the edit log to guests). Farmland's
meta byte becomes **fertility 0–255**. No new storage, no new
persistence, no protocol change.

- Tilling reads the ground it came from: grass-fed loam starts
  ~170, bare dirt ~110, sand-adjacent ~70.
- Crop growth chance scales `×(0.4 + 1.2 × fert/255)` — exhausted
  soil crawls at ~40%, rich loam runs half again over baseline.
- A crop reaching final stage DRAINS the block (~24). The same
  species again drains ×1.5; a different family after it drains
  ×0.75 — rotation emerges from arithmetic, no rules lecture.
- Fallow tilled land recovers slowly (+2/day), and faster under
  winter (+4/day): winter stops being dead time — it becomes the
  soil's turn. Snow-covered fallow counts double-blind as winter.
- **Legibility rule**: fertility tints the farmland face — dark
  loam through pale dust in four visible steps. No numbers, no
  screen; a farmer reads the field the way they read the sky.
- **Grass regrows on dirt**: a dirt block with sky light and an
  adjacent grass block rolls ~0.05 per random tick to heal over.
  Bare scars close; grazed ground (stage 2) recovers; the world
  stops accumulating permanent wounds nobody meant to leave.

## Stage 2 — the belly (grazing, dung, and the raid)

Every animal gets a **belly**: a hunger clock (~10–15 minutes
sated-to-hungry, scaled by season — winter bites harder). Hunger
drives everything that follows; behavior stays the cheap state
machine it is, with one new state (`Graze`/`Feed`) and a target.

- **Grazers** (deer, goat, and the new herds) seek grass within
  ~12 blocks when hungry, head down, eat: grass → dirt (which
  heals, stage 1). Browsers (deer again, boar) also take berry
  bushes back a stage and eat fallen produce.
- **Digestion**: a fed animal drops `dung` after a minute or two.
  Dung is an item; on farmland, right-click applies it (+48
  fertility). Penned animals ARE a fertilizer works — the
  civilization loop in one image: pen → dung → field → feed →
  pen.
- **The raid**: a hungry animal picks the nearest RICHEST food it
  can walk to — and a final-stage crop out-scores wild grass every
  time. Raiders eat the crop block back to its first stage (never
  break farmland, never touch placed blocks) and will not hop
  onto anything player-placed: a one-high wall or fence line
  turns them, full stop. An unfenced potato patch at the forest
  edge is an invitation, and that's the point.
- **Herds**: group-spawned species get centroid-biased wander —
  no flocking math, just a homeward lean — so bison read as a
  herd, not as strangers who co-spawned once.
- **Compost**: a `compost_heap` block (kiln-pattern stack check)
  takes plant matter — spoiled mush finds its purpose, leaf
  litter (stage 6), surplus seeds — and after a day yields
  `compost` (+32). The rot lane of the fertilizer bag.

## Stage 3 — fang and carrion (the trophic layer)

Predators hunt the existing `Hunt` state pointed at mobs instead
of players, gated on the belly. Sated predators are scenery.

- **The roster**: fox (plains/forest — rabbits, grouse, pheasant),
  wolf (taiga/tundra/forest — deer and hares, hunts in pairs),
  lynx (taiga/mountains — hares, marmots), jackal (savanna/
  scrubland — hares, guineafowl), eagle (mountains — marmots,
  from the sky), rattlesnake (badlands/desert — a defensive
  striker, not a hunter of players).
- **Desperation**: a predator long unfed goes desperate — and a
  desperate predator, in winter or at night, with no prey in
  range, will size up the player. It breaks off when wounded;
  it is hungry, not suicidal. Leashed and tamed animals are what
  it would honestly prefer — shepherding gets stakes.
- **The polar bear** (arctic): `fierce = true` — the one animal
  that is wildlife (persisted, natural, seeded like any other)
  and hunts the player on sight, fed or not, day or night. HP 40,
  hit 6, aggro 20. It also eats seals. It is not misunderstood.
  It is not desperate. It likes this.
- **Carcass, not loot**: a predator kill leaves a `carcass`
  entity, not player drops — no free hunting economy. Scavengers
  (vulture — badlands/savanna/desert) walk the sky to it and
  feed; whatever is left rots in ~3 minutes into the ground
  below (+fertility). Death feeds ground. Player kills work
  exactly as today.
- Predation is **ire-neutral** (nature's own violence); killing a
  predator yourself is +2.0 like any wildlife — the wild does not
  grade on cuteness.
- Predators never block sleep unless actively hunting you (the
  warden rule stays warden-only).

## Stage 4 — the water bears life

- **Fish** with real swim movement, confined to connected water:
  trout (cold — taiga/arctic/mountain water), carp (temperate
  lakes and rivers), catfish (swamp). Fish are ambience-plus-
  resource, seeded per water body near players under their own
  budget, culled far from players like hostiles — individually
  persisted fish are pomp; "this water has fish" is the truth
  worth keeping.
- **The rod**: sticks + plant fiber. Cast a bobber; a bite window
  5–20 s; strike to catch. If a real fish entity is near the
  bobber you catch THAT fish (it leaves the water); bare water
  falls back to a thin luck table. Fishing is honest about the
  ecology under it — empty water fishes poorly, and a heron
  working the shallows means you picked the right spot.
- **The waterline roster**: frog (swamp — croaks, hops, eaten by
  everything), heron (swamp/river — spears fish and frogs, the
  fisherman's rival), seal (arctic coasts — the polar bear's
  larder), crab (beaches — sidles, pinches for 1 if bothered),
  crocodile (swamp — waterline ambush; fish, frogs, herons, and
  any warm thing that lingers at the bank; desperate rules apply
  except it defines desperate loosely).
- **Water plants**: cattails/reeds at the margins (plant fiber
  standing in water — the swamp's flax), kelp in deep cold water,
  water lilies (walk-on pads? no — just beauty). Fish spawn odds
  lean toward planted water: life gathers where life is.

## Stage 5 — the full roster (every biome answers)

The five silent biomes get natives, and the loud ones get
company. New grazers/herds: bison (plains, the thundering
default), antelope (savanna, flighty), musk ox (tundra, stands
its ground in a ring), mouflon (scrubland/mountains), camel
(desert — tameable carrier, the nomad's truck). New birds:
pheasant (plains/forest), guineafowl (savanna), ptarmigan
(tundra/arctic, white in winter), duck (lake and river surface).
Underground: **bats** — roost in dark cave ceilings, flutter out
at dusk, and **guano** accumulates beneath a roost (+64 fertility,
the strongest fertilizer in the game, and it is found not made:
the nomad's lane, and a reason to go into the dark that isn't
ore).

- **Not all of it is for you.** Songbirds, butterflies, and
  dragonflies are client-side ambience — sprites and song, no
  entities, no cap pressure, no drops. Frogs croak, marmots
  whistle, owls own the forest night (audio bed). A world where
  everything drops something is a warehouse. Some of the world
  is just living here.
- MOB_CAP rises to ~320 with per-category budgets (wildlife /
  fish / hostile) so a full lake never starves the land spawns
  and wardens never crowd out deer.
- Breeding foods and taming set per species as shipped wildlife;
  wolf/fox companionship is explicitly a FOLLOW-UP arc (it
  deserves its own plan, not a rider).

## Stage 6 — rot and fruit (fungi and the falling leaf)

The decomposition arm — without it the nutrient cycle has no
return path and dung carries the whole load.

- **Leaf decay, at last**: leaves with no log within reach (BFS
  ≤4 through the canopy) decay on random tick, keeping the 10%
  sapling bonus, sometimes dropping **leaf litter** — a thin
  ground layer that feeds the soil below as it fades (+fertility)
  and feeds the compost heap if gathered. Felled forests ROT now;
  the floating-canopy era ends.
- **Fungi spread**: mushrooms creep by random tick where it is
  dark (light < 6) and damp (water within ~6, rain-wet, or
  underground), crowd-capped like the green tide. They favor
  litter and dead wood — the decomposers show up where the dying
  is.
- **Lantern fungus**: a cave species that glows (light ~6),
  spreading slowly on deep stone near water. The first native
  light of the underground; harvestable, placeable, pretty. Bats
  roost near it. The deep cave becomes a place instead of a
  hallway.

## Stage 7 — the storm gives back (the keystone)

Ire's missing half. Today wrath is only punishment; this stage
makes retribution and renewal the SAME event — the titan levels
the valley and the jungle erupts behind it.

- **Lightning**: during Storm weather (already ire-scaled: 10% at
  calm → 60% at wrathful), bolts strike every ~20–40 s near
  players — always the highest NATURAL column, never a placed
  block, never inside a player-touched footprint. Flash, crack,
  thunder rolling in late through the existing audio bed. The
  strike chars the ground: **charred soil**, fertility 255, a
  scorched scar that tills into the richest farmland in the game.
  No fire — fire is its own future arc, and the char sidesteps
  it cleanly.
- **The bloom ledger**: struck cells and warden death sites bank
  bloom-charge (persisted beside the regional-ire ledger). A
  charged cell blooms for ~3 game-days: green tide runs ×4 there
  (and ignores the resentment gate — this is the wild's OWN
  doing), grass heals ×4, bushes refruit ×2, flowers and fungi
  sprout on natural ground. A dryad's death site puts up
  saplings. The wild reclaims its own, extravagantly.
- The emergent story is the design goal: strip-mine a valley →
  the wild rages → a storm wrecks the night → and a week later
  that valley is the greenest, most fertile land on the map.
  Provoking the wild becomes a terrible, tempting farming
  strategy — the moral texture the meter always wanted. Wrathful
  nights were already a harvest (warden drops); now they are a
  planting too.
- **Flowers** ride in here: two or three meadow species seeded by
  blooms and by worldgen in plains/forest. No use at all. That is
  the use.

## Touchpoints

- world: farmland fertility in block meta; belly/dung fields on
  MobState; bloom ledger beside `rire`; grass-heal, fungus-creep,
  leaf-decay, litter-fade rules in random_tick (the K-oldest
  budget holds — new rules are samples, not sweeps); lightning in
  the weather tick; carcass entity; fish category in ecology
  spawn budgets; MOB_CAP → ~320 with per-category split.
- mobs: Graze state; hunt-mob targeting; desperation; fierce
  flag; swim movement (water-confined wander); sky movement for
  eagle/vulture/bat (float exists — extend with roost/circle);
  herd centroid lean.
- content: ~21 new species in animals.toml (textures via the
  existing pipeline), blocks/items — dung, guano, compost +
  compost_heap, carcass, charred_soil, leaf_litter, lantern
  fungus, cattail/kelp/lily, flowers, cloudberry bush (the
  arctic forage), fishing_rod. Tiles via gen_base_tiles.py argv
  mode ONLY (never regenerate shipped tiles).
- persistence: fertility rides chunk meta (already saved); bloom
  ledger beside rire; fish deliberately unpersisted; polar bear
  persists like all wildlife.
- protocol: none expected — species stream by registry index and
  content_hash keeps guests honest; meta already rides the edit
  log. Agents (PR #46) inherit the world for free; teaching them
  to fish is a follow-up.

## Tests

- Fertility drains on harvest, monoculture drains faster,
  rotation slower; fallow and winter recover; tint steps track
  quartiles; guests see the tint (meta sync).
- Grass heals dirt only under sky beside grass; grazed ground
  recovers end-to-end (graze → dirt → heal).
- A hungry deer beelines the unfenced final-stage crop over wild
  grass; a one-high fence line defeats the same deer; dung
  applies and the numbers add up.
- A hungry fox kills a rabbit; the rabbit leaves a carcass, not
  drops; the carcass rots into a fertility bump; a vulture beats
  the clock when present. Predation moves ire not at all.
- A desperate winter wolf sizes up the player and breaks off
  when wounded; a sated summer wolf ignores everyone. The polar
  bear charges a full-health player at high noon, unprovoked
  (the test is named the_polar_bear_needs_no_reason).
- Fish stay in water; the rod catches a real nearby fish first
  and the luck table only in empty water; heron eats fish.
- Leaves off a felled trunk decay; litter feeds the soil below;
  mushrooms spread only dark-and-damp and cap their crowd;
  lantern fungus lights its cave cell.
- Lightning strikes natural ground only — never a player-placed
  block, never a player-touched footprint; struck cells bloom;
  bloom ignores the resentment gate; charge decays; the ledger
  round-trips through save/load.
- The shared-ire guard holds: an agent or player killing any new
  species charges +2.0 (the design-guard test pattern from the
  agent arc, extended).

## Stages

1. Living soil — fertility in the meta byte, tints, rotation,
   fallow, grass heals dirt.
2. The belly — hunger, grazing, dung, herds, the raid, compost.
3. Fang and carrion — predators, desperation, the polar bear,
   carcass and scavengers.
4. The water bears life — fish, the rod, the waterline roster,
   water plants.
5. The full roster — every biome answers; bats and guano;
   ambience species; cap budgets.
6. Rot and fruit — leaf decay, litter, fungus creep, lantern
   fungus.
7. The storm gives back — lightning, charred soil, the bloom
   ledger. The keystone lands last so the whole cycle is
   standing when the wild starts giving back.

Stages 1–2 are the spine (soil + belly) and everything after
hangs off them; 4 and 5 can interleave; 6 before 7 so blooms
have flowers and fungi to sprout. Screenshot every new face —
the fertility gradient, a bison herd at graze, a wolf hunt in
snow, the heron and the rod at dusk, a lantern-fungus cave with
bats, and the money shot: the bloom after the storm.

The guard, restated once more: the wild never touches what
players BUILT. The deer eats your crops because a deer is a
deer — but the fence you build holds forever, and the land you
tend feeds you back.
