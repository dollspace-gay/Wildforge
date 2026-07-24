# The wild arc — the land becomes an actor

Drafted 2026-07-24. Decisions settled with dollspace: the wild's
side of the give-and-take deepens from a tax into a relationship —
regional memory, legible escalation, richer offerings — plus the
housekeeping sweep folded into this arc (preservation's second
chain and ceramics, the mod system's cross-reference, and two known
irritants).

Ire today is one global number: honest, but it reads as a tax rate.
The compelling version is a wild that remembers WHERE. The forest
you clear-cut goes quiet and hostile; the valley you replant
forgives; and a settlement's standing with the land around it is
part of what a place IS — which feeds the trade arc too (a town
with an angry forest needs to import wood, and caravan guards).

## Stage 1 — regional ire (the land remembers where)

- The global ire field stays (the backdrop mood). Added: a coarse
  regional ledger, `HashMap<(i32, i32), f32>` keyed on ~256-block
  cells, persisted with the world, capped modestly (±20).
- Taking (chopping, hunting, mining surface soil) charges the CELL
  it happens in as well as the world; planting and offerings credit
  the cell they happen in. Decay drifts every cell toward zero over
  days — grudges and gratitude both fade.
- Effects are local: warden spawn odds, aggression radius, and the
  hunt tier read (global + regional clamped). A hostile forest is
  ~2 tiers angrier inside its cells; a tended valley ~2 gentler.
  The numbers stay small enough that a nomad passing through an
  angry region is menaced, not executed.
- Legibility rule: every regional state must be READABLE in the
  world (see stage 2) — invisible modifiers are just spooky RNG.

## Stage 2 — legible escalation (the wild warns before it strikes)

Everything the wild is about to do should be visible one beat
early:

- **The watcher**: at rising local ire, a warden spawns passive at
  the treeline, watching the player — no attack, one in-world
  warning. Fixing relations (planting, an offering) while watched
  de-escalates; ignoring it graduates to the existing hunt.
- **Gathering signs**: wisps drift toward the region's angriest
  cell at dusk; birdsong (ambience layer) thins in hostile cells
  and thickens in tended ones. Cheap ambience reads, no new AI.
- **The whisper line**: crossing into a markedly hostile or blessed
  region toasts one line, tablet-voiced ("The trees here remember
  the axe." / "This ground knows you."). Rate-limited, once per
  region per session.

## Stage 3 — offerings with seasons (the wild wants things)

The offering stone learns appetite. Each season the wild wants a
category (hash of world seed + year + season, readable by
interacting with the stone): winter wants food, spring wants
seeds and saplings, summer wants water in vessels, autumn wants
the harvest's first fruits. Wanted offerings credit double
(regional and global); unwanted still count singly — never a
punishment, only a bonus for listening. The stone's whisper states
the want plainly; no wiki required.

## Stage 4 — the green tide (regrowth without decay)

The wild acts on the WORLD only where nature already owns it — the
no-structure-decay guard holds absolutely:

- Natural saplings: mature trees in tended-or-neutral cells
  occasionally seed a sapling on adjacent natural grass (random
  tick, forest density capped so forests thicken but never march
  over builds — a sapling never lands within ~8 blocks of any
  player-placed block... approximated honestly: never on a cell
  the chunk records as player-modified).
- Berry bushes in blessed cells refruit at double chance —
  tending the land literally feeds you back (the forage patches
  stay patches; this sweetens them where you've earned it).
- Hunted-out cells (the spawn-once wildlife rule) get ONE
  exception: a blessed cell that stays blessed for a full season
  reseeds its wildlife roll once. The wild forgives — slowly, and
  only the land you actually tend.

## Stage 5 — the housekeeping sweep (folded in, as agreed)

- **Clay and the crock** (preservation's second chain + first
  ceramic): clay beds in river/lake shallows (worldgen: clay
  blocks under fills where hydrology placed water over sediment).
  Clay → kiln → `crock`. Crock + brine (salt + water bucket) +
  vegetables = **pickles** (veg keep ~10 days); crock + smoked
  goods pairs with the **smoker** — a campfire-pattern mini-stack
  (logs + green leaves) that turns raw meat to `smoked_meat`
  (~6 days, no salt needed — the woodland answer to salt country,
  so preservation has two regional identities).
- **The mod system** (cross-reference): docs/modding-plan.md is
  approved and unstarted. It is the enabler for community content
  across all three arcs and should be scheduled as its own work —
  recorded here so the sweep list is complete, not re-planned here.
- **The flaky loopback test**: loopback_join_stream_and_edit fails
  under parallel test load (QUIC timing). Give it a retry-or-longer
  handshake timeout so the suite is honest again.
- **Cascade shadow acne** (ngutten's #24): the known triangle
  artifacts on steep slopes want a slope-scaled depth bias in the
  cascade sampler — flagged for/with ngutten, listed so it stops
  living in commit messages.

## Touchpoints

- world: regional ledger + persistence (world.toml sidecar or the
  stamps pattern); ire read paths take a position; ecology spawn
  odds consult the cell; random_tick gains sapling-seed and
  bush-bonus rules; chunk modified-flag exposed to the seeding
  rule.
- mobs: watcher behavior (spawn passive, despawn on de-escalation).
- audio: ambience density hooks per-cell.
- content: clay, crock, pickles, smoked_meat, smoker; tiles via
  gen_base_tiles.py.
- No protocol changes (regional ire is host-side; guests see its
  effects, not its numbers).

## Tests

- Regional charge/credit/decay round-trips through save/load;
  effects clamp; a neutral traveler in an angry region faces tier
  +2, not +10.
- The watcher spawns before the hunt and stands down on planting.
- Seasonal want doubles wanted offerings only; the stone reports
  the want deterministically.
- Saplings seed only on natural ground in non-hostile cells; never
  on player-modified cells; forest density caps.
- Reseed fires once per blessed season, never for neutral cells.
- Pickles and smoked meat hold their perish clocks; crock requires
  the kiln; smoker validates and fears nothing (it IS smoke).
- Loopback test passes 20 consecutive parallel runs.

## Stages

1. Regional ire ledger + effects.
2. Legible escalation (watcher, signs, whispers).
3. Seasonal offerings.
4. The green tide (regrowth within guards).
5. Housekeeping sweep (clay/crock/pickles/smoker; loopback fix;
   shadow-acne bias with ngutten; mod system scheduled separately).

Stage 5 can land first or interleave — it shares no state with 1-4.
The arc's guard, restated once: the wild never touches what players
BUILT. It acts only on what was always its own.
