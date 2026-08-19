# Task Brief: Modular Equipment (capability E6)

**Repo:** WildForge, branch `feat/capability-ramp`
**Primary files:** `src/equipment.rs` (new — pure model: `FrameDef`,
`FrameSlotDef`, `SlottedComponent`, `Loadout`, `PresetSlot`,
`LoadoutPreset` + tests), `src/game/equipment.rs` (new — Game glue:
mode gating, loadout stats fold, slot/unslot, repair, presets, loadout
screen geometry), `src/registry.rs` (RawMod `frame`/`component` item
fields + `FrameToml` parsing + validation, `ModeToml`/`ModeDef`
`equipment` toggle), `src/ruleset.rs` (new `equipment` mode toggle),
`src/game/mod.rs` (`SurvivalState.loadouts: [Loadout; 5]` +
`loadout_presets`, `Screen::Loadout`, `UiState` fields),
`src/game/session.rs` (loadout + preset persistence in the profile),
`src/game/survival.rs` (wear: frames disable at 0 durability instead of
destroying; death drops components intact), `src/game/containers.rs`
(`armor_click` returns components), `src/game/ui.rs` + `keymap.rs` +
`menus.rs` (loadout screen, `L` toggle),
`mods/gems/{items.toml,recipes.toml,modes.toml}` + `mods/README.md`
(sample frame/components + docs).
**Related design doc:** `belt-quest/docs/port-plan.md`, capability E6
(row 101), E4 dependency, and rows 53/153. **Explicitly overrides
WildForge's "disposable durability" design** where it applies to
frames: a worn-out frame is *disabled*, not destroyed, and repairable.
`Screen::Loadout` is a temporary hardcoded screen; E11 generalizes the
screens layer for mods.

## Goal

Frames (wearable items with typed slots) and components (items that slot
in and out intact) compose a loadout whose derived stats come from the
worn configuration: `armor.points` from the frame, E4 `StatModifier`s
from slotted components. Durability wears to 0 → the frame is disabled
but never destroyed and stays repairable. Loadout presets save/apply
whole configurations from the inventory. The capability is mode-gated
(E1), so built-in Survival/Creative are unchanged.

## What shipped

- **Content schema** (`items.toml`): a `frame = { slots = [ { type =
  "gem", max = 2 } ] }` table marks an item as a frame; `component =
  "gem"` marks an item as a component of that slot type. A `max = 0`
  slot, a duplicate slot type, an empty `[frame]`, an item that is both,
  or an empty component type is a load error surfaced on the mods
  screen. Frames and components are `one_only` (max stack 1). 5 unit
  tests in `src/equipment.rs`.
- **Engine model** (`src/equipment.rs`): `FrameDef::validate`,
  `FrameDef::max_for(slot_type)`, `Loadout` with `slot` (enforces frame
  capacity + slot types), `unslot` (returns the component *intact* —
  no stackdown/depletion), `used_for`, `stats(&Registry)` (folds
  component `StatModifier`s), and `LoadoutPreset` (per-slot frame id +
  component ids).
- **Mode gating** (E1): new `equipment` ruleset toggle, `false` in both
  built-in rulesets. `equipment_enabled(game)` = not creative + mode has
  `equipment = true` + the pack ships a frame item.
- **Game glue** (`src/game/equipment.rs`): `equipment_stats` (legacy
  armor stats always count; frame + component stats only while the frame
  is worn *and intact* when enabled), `worn_frame`, `return_loadout_components`,
  `slot_component`, `unslot_component`, `repair_frame`, `save_loadout_preset`,
  `apply_loadout_preset` (inventory lookups by slot index; previously
  worn frame returns to hand), plus the loadout-screen geometry.
- **Durability**: `armor_points` and the wear loop skip frames at 0
  durability (disabled-not-destroyed) when equipment is enabled; legacy
  armor items keep their `broken_into` behavior. Components ride the
  loadout: `armor_click` returns them for slots 0..=3, and death drops
  them intact alongside the armor.
- **Persistence**: the profile gains `[[loadout]]` per worn slot
  (`[[loadout.component]]`: item/count/durability/arcane_id) and
  `[[loadout_preset]]` (per-slot frame + components), all
  `#[serde(default)]` for backward compatibility.
- **UI**: `Screen::Loadout`, `L` toggles it in-world (mirror of `K`).
  The screen shows the four armor boxes with selected-frame highlight,
  durability + BROKEN status, per-slot typed component sub-slots (click
  inserts the held component / takes one back), a REPAIR button, four
  preset buttons (0 = SAVE, 1..=3 = APPLY), and the inventory grid.
- **Sample mod**: `mods/gems/items.toml` adds `gem_frame_chest` (frame,
  2 "gem" slots, armor chest 2, durability 60, move_speed ×1.05,
  materials `{ ruby = 1200 }`) and components `cut_ruby` (+2 health),
  `faceted_emerald` (stamina regen ×1.125), `void_pearl` (+32 carry),
  each materials `{ ruby = 400 }`; recipes craft them from rubies. The
  component materials being a strict subset of the frame's makes a worn
  component a valid repair part for the frame. `mods/README.md`
  documents the new fields, the `equipment` mode toggle, and the loadout
  screen. `equipment = true` on the `gems:cozy` mode is the in-game demo
  path.
- **Tests**: 5 `equipment` unit tests, `equipment_toggle_is_opt_in_and_off_by_default`
  (ruleset), 2 mod-lint tests (`an_invalid_frame_fails_qualification`,
  `a_component_with_a_recipe_passes_qualification`). Suite green.

## Verification

- `cargo clippy --lib --tests -- -D warnings` clean.
- `cargo test --lib` green.
- `wildforge --mod-qualification mods` PASS.
- Visual + geode qualification hashes refreshed (src/registry.rs,
  src/ruleset.rs, src/game/*.rs, src/lib.rs are qualification sources).

## Content notes

- Base ships no `frame`/`component` items and both built-in modes keep
  `equipment = false`, so stock gameplay is numerically identical.
- Loadout operations are local-only (no net sync yet; surviving clients
  reconcile via their own slot ops).
- A second frame per armor slot is possible (e.g. a helmet frame); the
  loadout array is index-aligned with `armor` (H/C/L/B + charm at 4).

## Out of scope (later capabilities)

- E11 mod-extensible screens: the hardcoded `Screen::Loadout` becomes a
  generic mod screen layer.
- E14 save/load completeness: loadout + preset persistence landed here;
  remaining belt/dungeon/equipment state is deferred.
- Net-synced loadout state / shared presets.
- C6 belt-quest content: gem/enchant frames and their recipes.
