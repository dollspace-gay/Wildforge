# Chests, Torches & Lighting — Design Plan

Decided 2026-07-18. Three pieces that set up the hostile-mob milestone
(which will need "darkness" to be a real, queryable value — hostiles are
deliberately NOT in this doc; their design comes next).

## 1. Lighting engine (the core)

Two light channels per block, 0..15, recomputed per chunk (never saved):

- **Block light**: emitted by blocks with a `light = N` def field
  (torch 14). BFS flood-fill from emitters, −1 per step, stopped by
  opaque blocks. Fully data-driven: any block (or mod block) can glow.
- **Sky light**: column scan from the top — 15 until the first opaque
  block, then 0 below; after the scan, horizontal BFS (−1 per step)
  floods daylight sideways into overhangs and cave mouths.
- Water attenuates sky light an extra −1 per block (depth dims).

**Storage**: two `u8` arrays per chunk (16×256×16 each). Light is
derived data — recomputed on chunk load/gen, never persisted.

**Updates**: correctness-first per-chunk relight. `set_block` marks the
chunk + adjacent chunks dirty; a relight pass recomputes their columns +
BFS (seeded from neighbor-chunk borders so light crosses seams), then
queues remesh. A full relight is a few hundred µs per chunk in release —
fine at interactive edit rates, and immune to the classic
incremental-unlighting bugs.

**Rendering**:

- `Vertex` gains a second light attribute: `(block_light, sky_light)`
  per vertex, sampled from the cell the face looks into (same cell the
  face-culling check reads).
- Shader combines: `light = max(block, sky * daylight)` where
  `daylight` is a frame uniform driven by the existing time_of_day
  (night ≈ 0.12 floor so moonlit surfaces stay navigable). Torches thus
  hold their brightness through the night while the sky dims — the
  whole point of the split.
- The existing FACE_SHADE directional term multiplies on top.
- Deep-cave ambient floor ~0.03: unlit caves are properly black.
- Entities (item drops, mobs) sample the light at their position and
  scale their vertex light — a deer in a cave is dark, torchlight
  reveals it.
- v1 is flat per-face light (alpha-Minecraft look). Smooth per-corner
  light/AO is an explicit non-goal this milestone.

## 2. Torches

- `base:torch`: cross-rendered, non-solid, `light = 14`, instant break,
  drops itself. Placeable on solid ground only (floor torches v1 — wall
  mounting needs per-block orientation state; punt). Pops off (drops)
  when the block under it is removed, like crops.
- Recipe: charcoal over stick → 4 torches (closes the charcoal loop);
  coal isn't a thing yet, charcoal is the fuel age we have.
- Art: procedural tile + gemini-pack sprite (plant category — bottom
  aligned); warm glow comes from the light system, not the texture.

## 3. Chests

- `base:chest`: crafted from 8 planks in a ring (`#base:planks` tag —
  any wood mix). Pickaxe-free, axe-class, hardness ~4.
- **Block entity**: `BlockEntity::Chest { slots: [Option<ItemStack>; 27] }`
  on the existing block-entity infrastructure (the furnace was built as
  the prototype for exactly this). `interaction = "chest"` on the def
  routes right-click to `Screen::Chest(pos)`.
- **UI**: 3×9 chest grid above the player inventory, standard
  click_stack rules (left = stack, right = single), item browser stays
  docked. No shift-quick-move v1.
- Breaking spills contents as item entities (existing pending_drops
  path). Persistence in entities.toml by item name (mod-change safe),
  same as furnace slots. Hot reload remaps by name.
- Textures: chest_front (latch), chest_side, chest_top — procedural +
  gemini additions.
- Mods get containers for free via `interaction = "chest"`.

## Data formats

```toml
# blocks.toml
[[block]]
id = "torch"
light = 14          # any block may declare light 1..15
solid = false
cross = true

[[block]]
id = "chest"
interaction = "chest"
```

## Tests

- Block light: torch in a sealed dark room — 14 at source, −1 per
  step, 0 behind an opaque wall; removing the torch relights to dark.
- Sky light: 15 on the surface, 0 in a sealed cave; opening the roof
  floods it; light crosses chunk borders (torch at a seam lights the
  neighbor); water column dims with depth.
- Data-driven glow: a mod block with `light = 9` propagates 9.
- Placing an opaque block over a torch casts shadow after relight.
- Mesher: vertices carry (block, sky) sampled from the faced cell.
- Torch: recipe from charcoal, floor-only placement, pops + drops when
  support breaks, instant break.
- Chest: place → entity exists; click_stack moves stacks in/out
  headlessly; break spills all slots; save/load round-trips contents by
  name; unknown items (removed mod) skip; furnace still works.
- Perf sanity: relighting a full 256-high chunk stays under a few ms.

## Implementation order

1. Light storage + per-chunk relight (sky scan + emitter BFS + border
   seeding) — headless, fully testable before any rendering.
2. Vertex/shader change: dual light channels + daylight uniform +
   ambient floors; entity light sampling.
3. set_block dirty-marking + relight-then-remesh queue.
4. Torch content: block, recipe, placement/support rules, art.
5. Chest: entity + screen + persistence + spill + recipe + art.
6. Tuning (night floor, cave black), screenshots, README, push.
