# Item Browser & Creative Mode — Design Plan

Decisions (2026-07-18): **docked JEI-style browser panel**; game mode
**switchable in the pause menu** (and chosen at world creation).

## 1. Item browser (native NEI/JEI)

Docked panel on the right side of the Inventory, Crafting (3x3), and
Furnace screens. Powered entirely by the registry — mods appear
automatically.

- **Item grid**: filtered list of `reg.items`, hiding internal variants
  (names containing `/`). 6x8 icons per page, page arrows, page N/M.
- **Search**: text field above the grid; matches substring of label and
  id (`"base:"` prefixes searchable). Engine addition: text input via
  winit `KeyEvent.text` routed to the focused field; our pixel font
  renders it. Esc clears search before closing the screen.
- **Recipe view** (click an item): overlay panel showing, tabbed/paged:
  - Crafting recipes producing it: pattern drawn with slot widgets +
    output; **tag ingredients cycle** members every 0.8 s.
  - Smelting recipes producing it (input + flame + output; fuel examples).
  - **Uses tab**: recipes/smelts where the item is an ingredient or fuel.
  - Right-click an ingredient inside the view → jump to *its* recipes
    (JEI-style navigation, with a small back stack).
- **Creative give**: in creative mode, left-click an item in the grid →
  stack of `max_stack` onto the cursor (right-click: single). Survival:
  browser is reference-only.
- Stretch (post-v1): click-to-fill craft grid from inventory.

## 2. Creative mode

- **World metadata**: `world.toml` (seed + mode) replaces the bare
  `seed` file; loader falls back to reading legacy `seed` (survival).
  Title screen NEW WORLD gains a Survival/Creative toggle; world rows
  show the mode.
- **Pause menu**: "MODE: SURVIVAL/CREATIVE" button toggles and persists.
- Creative rules (all gated on `world.mode`):
  - Invulnerable: damage()/hunger/air/starvation no-op; hearts, hunger
    pips, and nutrition panel hidden.
  - Instant break (ignore hardness/tools), **no drops**, no tool wear.
  - Placement doesn't consume items; eating not needed (food still
    placeable as crops).
  - **Flight**: double-tap space toggles; while flying — no gravity,
    space ascends, ctrl descends, 2x move speed, collisions still apply.
    Landing/toggling off restores normal physics.
  - Simulation unchanged (water, furnaces, crops, scripts, day/night).
- Script event `on_mode_change(mode)` (cheap, keeps mods informed).

## Engine work summary

1. Text input plumbing + search field widget.
2. Browser panel UI + recipe/uses renderer + tag cycling + navigation.
3. world.toml (serde) + legacy seed fallback + creation toggle + pause
   toggle + mode plumbed into Game.
4. Creative gates in interact/damage/food/HUD + flight physics mode.

## Tests

- Search filter matches label/id; variant items hidden.
- Recipe lookup: recipes-for(item) and uses-of(item) correct for bread,
  planks (tag uses), charcoal (fuel uses).
- world.toml round-trip + legacy seed fallback; mode toggle persists.
- Creative: break yields no drops and ignores min_tier; placement
  doesn't consume; damage/hunger no-op; flight velocity has no gravity.
- Survival unaffected when mode = survival (regression pass).

## Implementation order

1. world.toml + mode plumbing + pause/creation toggles.
2. Creative gameplay gates + flight.
3. Text input + browser grid + search.
4. Recipe/uses view + tag cycling + creative give.
5. Polish (navigation stack, fuel display), tests, README, push.
