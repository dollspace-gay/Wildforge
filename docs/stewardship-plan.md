# Stewardship — Saplings, Offerings, Bedrolls & Breeding

Decided 2026-07-18. The homestead milestone: the give-back half of the
ire dialogue, plus renewable wood, renewable meat, and sleep. The
pacifist's answer to the bows-and-armor kit.

## 1. Saplings & tree regrowth (wood becomes renewable)

- **Sapling items/blocks** per wood: oak, birch, spruce, jungle, acacia.
  Cross-rendered, placeable on grass/dirt (the existing "cross needs
  solid ground" rule), pop when support vanishes.
- **Source**: breaking leaves drops that species' sapling at ~1 in 10
  (alongside existing apple/fruit rolls). No leaf decay in v1.
- **Growth**: random-tick driven like crops but slower — a sapling
  rolls ~2% per tick; on success it checks clearance (trunk height + 2,
  canopy radius) and **builds a real tree** via a runtime
  `grow_tree(world, pos, species, rng)` that mirrors the worldgen tree
  shapes. Blocked space → stays a sapling and retries later. Expect
  1–3 in-game days to mature.
- **Ire**: planting a sapling refunds −0.5 through the daily plant cap;
  a planted tree **maturing refunds −2.0 bypassing the cap** — it took
  days, it *is* the slow path. (Saplings never occur in worldgen, so
  every maturation is provably the player's.)

## 2. The offering stone (ritual reciprocity)

- **Block**: `base:offering_stone` — 5 cobblestone + 2 plant fiber in a
  bowl shape (`c.c / cfc / .c.`... final shape at impl). Faint
  wildlight: `light = 5`. Axe-class, hardness 3.
- **Block entity** (new variant): 3 offering slots. Right-click opens a
  small screen (furnace-style, no fuel/arrow widgets).
- **At dawn** (day rollover), the wild takes what was left: slots empty,
  ire refunds by a value table — foods `hunger × 0.25`, saplings 1.0,
  meats 1.0, the wild's own materials (heartwood, living wood, ember,
  frost shard) 2.0, anything else 0.25. Capped at **−10 per dawn**;
  items are consumed regardless (an offering is an offering). Toast:
  "The wild has accepted your offering."
- Persistence/spill/hot-reload identical to chests.

## 3. Bedroll (sleep + camp anywhere)

Not a bed block — a **bedroll item**: 3 hide + 2 plant fiber. Fits the
wilderness fiction, needs zero new block shapes, and makes camps real.

- **Use** (right-click, standing on ground): only at night; refused
  with a toast if any warden is within 24 blocks ("The wild is too
  close."). Sleeping:
  - skips to dawn (time_of_day → morning),
  - **sets your spawn point** to the campsite (death returns you here),
  - applies the skipped night's ire decay (time passes fairly),
  - saves the player (a natural checkpoint),
  - costs 1 bedroll durability (12 uses — camps are consumable;
    hides stay relevant forever).
- Surface wardens are already gone at dawn by the daylight-dissolve
  rule; no special handling needed.

## 4. Animal breeding (meat becomes renewable)

- **Feed to breed**: right-click a wildlife animal with its favorite
  food (consumes one): deer → berries, boar → potato, goat → wheat,
  grouse → seeds, rabbits/hares → carrot. Data:
  `breed_food = "base:berry"` on the species. Fed animals calm (no
  fleeing from you for 30 s) — feeding is also taming-lite.
- Two **fed adults** of the same species within 4 blocks → a **baby**
  spawns between them; both parents reset with a 5-minute cooldown.
  Baby: model scaled to 45%, grows to adult over ~20 minutes
  (`growth: 0..1` scales all model boxes). Babies can't be fed or bred
  and drop nothing if killed (you monster).
- **Ire**: a birth refunds −1.0 (life returned to the world, uncapped
  — births are rate-limited by the cooldown anyway).
- Persistence: `fed`/`growth`/`breed_cd` join animals.toml with serde
  defaults (old saves load fine). Wardens ignore all of this.

## Data summary

```toml
# animals.toml (wildlife)
breed_food = "base:berry"

# blocks.toml
[[block]]
id = "oak_sapling"
cross = true
sapling = { tree = "oak" }     # grows via grow_tree

[[block]]
id = "offering_stone"
interaction = "offering"
light = 5
```

## Tests

- Leaves drop the right species' sapling (forced rng); sapling on dirt
  grows into a tree with logs + leaves after enough ticks; blocked
  clearance stays a sapling; sapling pops without support.
- Ire: sapling plant refunds via the cap; maturation's −2 bypasses it;
  offering dawn refunds match the value table and cap at −10; birth
  refunds −1.
- Offering stone: items placed → gone at dawn, ire reduced, toast
  queued; breaking it spills; persistence round-trips.
- Bedroll: refused by day and with a warden near; success skips to
  morning, sets spawn, applies ire decay, wears durability; death
  respawns at camp.
- Breeding: feed two deer → baby at 45% scale between them; parents
  cool down; baby grows over time and can't breed; fed animals don't
  flee; hostiles can't be fed; drops suppressed for babies;
  fed/growth persist through save/load.
- Regression: wardens, hunting, and crop ire behavior unchanged.

## Implementation order

1. Runtime `grow_tree` extracted/mirrored from worldgen + sapling
   blocks/items/drops + growth ticks (headless).
2. Ire hooks: sapling/maturation/birth/offering refunds.
3. Offering stone: block entity + screen + dawn tick + value table.
4. Bedroll: item, sleep rules, time skip, spawn set, durability.
5. Breeding: feed interaction, fed/cooldown/growth state, baby
   spawn/scale/render, persistence.
6. Art (procedural + gemini: 5 saplings, offering stone, bedroll),
   balance, tests, README, push.
