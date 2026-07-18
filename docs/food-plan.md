# Food, Hunger & Farming — Design Plan

Decisions (agreed 2026-07-18):

- **Nutrition tracks reward MAX HEALTH** (base 7 hearts → up to 13 with a
  balanced diet). No meal-to-meal food fatigue — the tracks alone carry
  the diversity incentive.
- **Player persistence ships in this milestone** (position, health,
  hunger, nutrition, inventory — saved with the world).
- Protein category is designed in but dormant until animals exist.
- Deferred: spoilage, soil nutrients/rotation, seasons, animals.

## 1. Hunger

- `hunger: f32` 0..20, drawn as 10 drumstick-style icons mirroring hearts.
- Drain: 0.01/s idle; +0.02/s while sprinting; +0.005 per jump; +0.008
  per block broken. (~15–25 min from full to empty in active play.)
- Effects: hunger ≥ 17 → health regen (1 half-heart / 3 s, consumes 0.5
  hunger per heal) — **replaces** the current free idle regen;
  hunger < 6 → sprint disabled; hunger = 0 → 1 half-heart damage / 4 s
  (starvation stops at 1 heart — starvation weakens, doesn't kill).
- Eating: right-click with food selected; `eat_time` (default 1.5 s) hold
  with progress on the crosshair area, crunch sounds, then hunger +
  nutrition apply. Cannot eat at full hunger unless the food grants
  nutrition below cap (topping up nutrients is allowed).

## 2. Nutrition tracks → max health

Five categories: **Grain, Vegetable, Fruit, Fungi, Protein** (dormant).

- Each is 0..100, depleting ~0.6/min of active play (full → empty over
  ~3 in-game days). Eating adds that food's nutrition values, capped.
- **Max health** = 14 (7 hearts) + 2 per category ≥ 40 (up to 22 = 11
  hearts with four active categories; 26 = 13 hearts once Protein wakes).
  Recomputed continuously; losing a category's threshold shrinks max
  health (current health clamps down too).
- HUD: hunger bar always visible; nutrition panel in the inventory
  screen — five labeled colored bars + "MAX HEALTH +N" readout.
- Data on items:

```toml
[[item]]
id = "berry"
food = { hunger = 3, eat_time = 1.0, nutrition = { fruit = 14 } }
```

Multi-category foods allowed: `nutrition = { grain = 8, vegetable = 6 }`.

## 3. Farming

**Engine additions:**

- **Random ticks**: every 0.5 s, for each loaded chunk, sample 8 random
  positions; if the block has a `tick` behavior (crop growth), roll it.
  Not required to be deterministic (unlike worldgen).
- **Cross-shape rendering**: `shape = "cross"` on BlockDef — mesher emits
  two diagonal quads (like item sprites) instead of a cube; non-solid,
  alpha-tested, breaks instantly, drops always.
- **Hoe** tool class (wood/stone/copper/bronze tiers): right-click on
  grass/dirt → `base:farmland` (dark tilled texture). Farmland reverts to
  dirt if a solid block sits on it. (No hydration in v1 — water proximity
  is a future refinement.)
- **Crop stage blocks**: `crop = { stages = 4, next_chance = 0.2 }`
  auto-registers `<id>/stage1..N` like water flow levels; random tick
  advances a stage with `next_chance`; final stage drops the harvest.
  Crops require farmland below and break (dropping seeds) otherwise.

**Base crops & wild sources (biome-tied variety):**

| Food | Category | Source |
|---|---|---|
| Wheat → bread | Grain | wild wheat patches in Plains; farmable |
| Carrot | Vegetable | wild in Forest; farmable |
| Potato → baked potato | Vegetable | wild in Taiga; farmable |
| Berries | Fruit | regrowing berry bushes in Forest/Taiga (right-click harvest, bush persists and re-fruits via random tick) |
| Cactus fruit | Fruit | harvested from desert cacti tops |
| Jungle fruit | Fruit | jungle leaves drop occasionally |
| Apple | Fruit | oak leaves drop occasionally (~1/60) |
| Mushroom → roasted | Fungi | caves (below y50) + Taiga floor |

- Wild plants generate in worldgen (same feature pass as trees); breaking
  wild crops yields food + seeds to start farms.
- Cooking: furnace smelts potato → baked potato, mushroom → roasted
  mushroom (higher hunger + nutrition than raw). Crafting: 3 wheat →
  bread; **stew** (bowl? no — keep v1 simple: "forest stew" = mushroom +
  carrot + berry in any grid shape (shapeless-ish via 1x3 row) → high
  hunger, three categories at once. Cooked/combined food is strictly
  better than raw — cooking is the optimization path.

## 4. Player persistence

`saves/<world>/player.toml`: position, yaw/pitch, health, hunger,
nutrition per category, hotbar selection, inventory + craft-grid stacks
(by item name/count/durability, mod-change safe like entities.toml).
Saved with `save_modified`, loaded in `start_world` (falls back to fresh
spawn if absent). Death still clears inventory (drops persist as world
items only if saved mid-session — acceptable v1).

## 5. UI

- Hunger icons right-aligned above the hotbar (mirrors hearts, same 7x7
  pixel-art style — drumstick/bread glyph).
- Eat progress: small filling ring/bar at the crosshair while holding.
- Inventory screen: nutrition panel on the left of the storage grid —
  5 bars (grain gold, vegetable green, fruit red, fungi brown, protein
  grey/dormant label "SOON"), plus current max-health bonus line.
- Hearts row renders max-health growth (up to 11 hearts of icons).

## Tests

- Hunger drains with activity; regen consumes hunger and only above 17;
  starvation floors at 1 heart; sprint gate below 6.
- Eating applies hunger + nutrition, respects caps and eat gating.
- Nutrition decay over simulated time; max health rises at threshold and
  clamps current health when it falls.
- Random ticks advance crop stages only on farmland; final stage drops
  harvest + seeds; cross-shape blocks mesh as two quads, non-solid.
- Berry bush harvest cycles (fruited → empty → refruits on tick).
- Player persistence round-trip: save, reload world, same position /
  health / hunger / nutrition / inventory; missing-item entries skip.
- Wild food generates per biome (plains wheat, taiga potato/mushroom).
- Furnace cooking + stew recipe resolve; mod-added food with custom
  nutrition parses and applies.

## Implementation order

1. Player persistence (independent, unblocks everything).
2. Hunger core: drain/effects/regen rework + HUD bar + eating action
   with a couple of hardcoded-simple foods (apple, berry from leaves
   drops) to make it playable immediately.
3. Nutrition tracks + max-health scaling + inventory panel.
4. Engine: random ticks + cross rendering + farmland/hoe + crop stages.
5. Content: crops, wild generation, bushes, cactus fruit, cooking,
   stew; food data on items via `food = {}` TOML.
6. Tuning + screenshots + README.
