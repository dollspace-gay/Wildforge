# Glassworks — sand falls, glass rises, minerals give it color

Drafted 2026-07-19. Sand finally earns its keep: it falls like it
should, cooks into glass, and — through a kiln, a quern, and minerals
you mine and grind — becomes colored glass with no dye system
anywhere in sight. Every color is a rock first. The steelworks
template (multiblock shell, batch firing, station-work tables) gets
its first reuse, which is exactly what that machinery was built for.

This plan also carries **Stage 0: the atlas grows 16×16 → 32×32**
(256 → 1024 tiles), because glassworks would otherwise be the last
feature that fits. It lands as its own commit before any glass
exists, so a rendering regression bisects cleanly.

## Why minerals, not dyes (design stance)

Dyes are a crafting-menu abstraction; minerals are *places*. A blue
window means someone mined cobalt below y 24; a violet one means
someone carried a steel pick into the deep. Color becomes geology and
effort, which is the same materialist philosophy as steel-as-process
— and two of the five colors come from ores the game already has, so
the system starts half-connected to the existing world.

## Stage 1 — sand and gravel fall

- **Gravity blocks**: `sand` and `gravel` gain a data flag
  `falls = true`. When any block change leaves a gravity block
  unsupported (the same edit cascade that pops torches), it detaches
  and falls.
- **Host/singleplayer**: a `FallingBlock` sim entity (pos, vel,
  block) — ItemEntity-style physics, rendered as a full-size cube via
  the item-entity cube path. On landing it re-plants as a block;
  landing on a non-solid (crop, torch, snow layer) pops that first,
  exactly like the support-pop rule.
- **Multiplayer**: the host simulates; guests get a
  `S2C::Falling(Vec<FallSnap { pos, block }>)` datagram alongside
  Mobs/Bolts (protocol bump), so they see the same tumble; the
  landing arrives as the usual authoritative `BlockSet`.
- Chains settle naturally: each landing/removal re-triggers the cell
  above. Worldgen deserts are pre-supported and never wake until
  disturbed.
- Suffocation damage, anvils-falling-on-heads, and duping guards
  beyond "detach removes the block atomically" are out of scope.

## Stage 2 — clear glass (and honest greenhouses)

- **`base:glass`**: smelt sand in any furnace (6 s). Transparent
  solid — `opaque = false` renders exactly like leaves (alpha-tested
  cutout), sky light passes through. Drops itself when broken (no
  silk-touch tier exists to gate it behind, and colored glass is
  already process-expensive).
- **The greenhouse rule gets its real ending**: today's winter
  exception wants a *dark* roof + torchlight. Glass closes the loop:
  a crop that is sky-lit in winter also grows (at 0.75×) if a scan of
  the 16 blocks above it hits **glass before sky** — a glass roof is
  a greenhouse, no torches required. (The scan runs only in the
  winter branch of `random_tick`; cost is nil.)
- Glass panes need thin-shape meshing we don't have (the `height`
  field only makes slabs); full blocks only, v1.

## Stage 3 — minerals and the quern

Five colors; three new ores, two from metals the game already mines:

| mineral | source | tier | powder | glass |
|---|---|---|---|---|
| verdigris | grind existing `raw_copper` | 2 (bronze) | `verdigris_powder` | **teal** |
| ochre | grind existing `raw_iron` | 3 (bronze) | `ochre_powder` | **amber** |
| cobalt | **`cobalt_ore`**, y < 24 | 4 (iron) | `cobalt_powder` | **blue** |
| cinnabar | **`cinnabar_ore`**, y 16–40 | 4 (iron) | `cinnabar_powder` | **red** |
| manganese | **`manganese_ore`**, y < 16 | 5 (**steel**) | `manganese_powder` | **violet** |

- Manganese is deliberately steel-gated: the first *mining* reason to
  finish the steelworks, and violet glass quietly signals "this
  player has a bloomery."
- New ores ride `features.toml` like iron did; each drops a raw chunk.
- **The quern** (`base:quern`, `interaction = "quern"`): a stone
  hand-mill, 3 stone + 2 stick, `height = 0.6`. It works exactly like
  the anvil — rest a grindable on it, channel with your bare hands
  (1.5 s per turn, 2 turns), and the powder pops out. One mineral
  chunk → 2 powder.
- **Engine**: the anvil's `[[worked]]` table generalizes rather than
  duplicating: entries gain `station = "anvil" | "quern"` (default
  anvil) and `tool = "hammer" | "none"` (default hammer), plus
  `count` on the output. `AnvilState` becomes the shared
  station-work entity. One system, two stations, and mods get both.

## Stage 4 — the glass kiln

**The stack is the stack; the mouth decides the craft.** The kiln
reuses the bloomery's exact 23-firebrick shell — players who built a
smithy already know the shape — with a different mouth block:

- **`base:kiln`** mouth: crafted `["ff", "tt"]` — 2 firebrick over
  2 tin ingots (tin finally does something beyond bronze).
  `interaction = "kiln"`, with a lit variant glowing white-gold
  (`light = 13`, `light_color = [1.0, 0.85, 0.55]`).
- `check_bloomery` parameterizes into `check_stack(mouth_kinds)` —
  same scan, either mouth. Breach rules, rain-halving, storm-dousing,
  roof detection: all inherited from the bloomery tick, verbatim.
- **The batch**: 4 charge slots (sand only), **1 powder slot**, 4
  fuel slots (charcoal). Fire for **0.25 day (150 s)** — glass runs
  hotter but faster than steel. Yield: every 2 sand + 2 charcoal
  makes 2 glass; the single powder colors the *entire batch* (a full
  8-sand firing = 8 colored glass per 1 powder). No powder = clear
  glass in bulk — the kiln beats the furnace on throughput even
  uncolored.
- Data: `[[kiln]] { powder, glass }` entries map each powder to its
  glass; sand/fuel/clear-output are fields on a `[kiln_base]` table.
  Mods add colors by adding an ore, a `[[worked]]` grind, and a
  `[[kiln]]` line — no engine work.
- Container kind 4 for guests (9 slots + the same `aux` the bloomery
  streams); the light/vent/breach messages reuse `LightBloomery`
  (renamed use, same shape — the host checks which mouth it hit).

## Stage 5 (stretch, cuttable) — stained light

The colored-light engine makes stained glass more than decoration:
colored glass **filters block light per channel** — a torch behind
red glass throws only its red component; blue glass beside it casts
blue. Implementation: a `light_filter = [r, g, b]` (0/1) block field;
the per-channel flood-fill treats a 0 channel as opaque for that
color. Sky light is a scalar and passes untinted (documented
honestly). If the flood-fill interaction gets hairy, this stage ships
separately — the rest of the plan doesn't depend on it.

## Stage 0 — the atlas grows to 32×32 (1024 tiles)

The 256-slot atlas has ~40 slots left; glassworks wants ~17. Grow it
first, as a standalone commit:

- **`ATLAS_TILES` 16 → 32**, default tile stays 32 px → a 1024×1024
  atlas (trivial for any GPU this engine runs on). Slot numbers are
  stable identifiers everywhere (saves store names, packs and mods
  address by name, `builtin_slots()` keeps its exact numbers) — only
  the slot→pixel layout changes, so **nothing outside the renderer's
  math can notice**.
- **Kill every hardcoded 16.** The refactor is mechanical but must be
  total: ~37 `% 16` / `/ 16` in `atlas.rs`, and every UI call site
  doing `(icon % 16, icon / 16)` by hand. Two API changes make the
  bug class unrepresentable afterwards:
  1. The procedural painters (`tile`, `tf`, `lump`, `plant`,
     `armor_art`, `bow_art`, `sapling_art`, ...) take **slot
     numbers**, not (tx, ty) pairs; each literal call site converts
     via `slot = ty * 16 + tx` once, scripted, and coords are derived
     inside with `ATLAS_TILES`.
  2. `UiBatch::tile` takes a **slot u16** and derives coords itself —
     deleting the `(icon % 16, icon / 16)` tuple math at every call
     site in `main.rs`.
- Pack tiles, mod textures, the gemini pack, exports, and the season
  tint all address by name or already use the constant — verified
  untouched by tests, not by hope:
  - the existing `export_tiles_round_trip_reproduces_atlas` must pass
    unchanged (it round-trips by name);
  - `pack_tile_override_applied_at_slot` and the embedded-gemini test
    must pass unchanged;
  - new assertions: atlas side is `ATLAS_TILES * tile_px`; grass_top
    still renders at slot 0's derived coords; the magenta
    missing-texture checkerboard still lives at `UNKNOWN_SLOT`; and a
    screenshot smoke confirms the world looks identical before/after
    (pixel-compare a known region).
- `FIRST_FREE_SLOT` stays 216 for this commit; after glassworks lands
  at ~233, mods have ~790 slots — effectively unbounded for years of
  content. `mods/README.md`'s budget paragraph updates to match (the
  executable-doc test keeps the rest honest).

## Glassworks tile budget (post-upgrade)

New tiles: 3 ores + 5 powders + 6 glass (clear + 5 colors, colored
generated by tinting the clear art) + kiln mouth ×2 + quern ≈ 17,
landing at slots 216+. Gemini prompts for every new tile as usual.

## Tests

- Gravity: unsupported sand falls and lands; chains settle a column;
  landing pops a crop and drops it; falling state round-trips the
  wire (guest sees Falling then BlockSet).
- Glass: sand smelts; glass is transparent to sky light in the BFS;
  a glass-roofed winter crop grows at 0.75× while its sky-open twin
  sleeps; the dark-roof+torch greenhouse still works.
- Quern/worked: station+tool generalization — hammer-on-anvil still
  makes bars, hands-on-quern grinds 1 chunk → 2 powder, hammer on
  quern and blooms on quern both refuse.
- Ores: three new bands generate within range, manganese refuses an
  iron pick (tier 5 gate).
- Kiln: shell validates with a kiln mouth (and still with a bloomery
  mouth); 8 sand + 1 cobalt powder + 8 charcoal → 8 blue glass;
  no-powder batch → clear; rain/storm/roof inherited behavior holds;
  loopback: a guest charges, lights, and empties a kiln (kind 4).
- Content-graph: the fixpoint learns grind + kiln edges; every glass
  and powder is obtainable; alias-free (all new content).
- Stained light (if shipped): red glass passes only red from a torch,
  asserted on the per-channel light grid.

## Sequencing

Stage 0 (atlas) is its own commit, first — pure refactor, zero
content, screenshot-verified. Stages 1–2 are a small commit (falling
sand + clear glass + greenhouse). Stages 3–4 are the big one. Stage
5 rides alone at the end, cuttable without a trace.
