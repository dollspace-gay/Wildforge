# Task Brief: Mod-Extensible UI Screens (capability E11)

**Repo:** WildForge, branch `feat/capability-ramp`
**Primary files:** `src/screens.rs` (new — the pure model: `ScreenDef`,
`ScreenWidget`, `screens.toml` parse/resolve + tests), `src/lib.rs`,
`src/registry.rs` (`Registry::screens`, parsing, build-loop resolve,
errors merged after `validate_material_graph`), `src/game/mod.rs`
(`Screen::Mod(usize)`), `src/game/ui.rs` (the def-driven draw arm),
`src/game/menus.rs` + `src/game/containers.rs` (click routing:
`mod_screen_click`, row geometry), `src/game/keymap.rs` +
`src/game/input.rs` + `src/game/tooltip.rs` (close lists, inventory
hints), `src/game/actions.rs` (`screen:` interaction arm,
`Cmd::OpenScreen` application), `src/script.rs` (`open_screen` fn +
`Cmd::OpenScreen`), `src/net/protocol.rs` (PROTOCOL 45 +
`C2S::ScreenClick`), `src/multiplayer/host.rs` + `src/dedicated.rs`
(validated host dispatch riding `HostFx`), `mods/gems/*` sample.
**Related design doc:** `belt-quest/docs/port-plan.md`, capability E11
(row 106). Closes the E5/E6/E7 "temporary hardcoded screen" notes.

## Goal

Open the closed `Screen` enum so mods add screens in data: skill trees,
equipment/loadout, dungeon HUDs, factory modes, quest boards — any panel
of text and buttons whose behavior lives in the mod's script.

## The design in one paragraph

A mod declares screens in **`screens.toml`**: a title plus rows of four
widget kinds — `label` (static text), `kv_label` (a live readout of one
per-player KV key), `toggle` (flips that key between "1"/"0"
client-side), and `button { label, action }`. Buttons dispatch ONE new
script hook, `on_screen_click(screen_id, action)`, where the mod reacts
through its existing command queue (`give`, `set_block`, `hud_message`,
`storage_set`, ...); state comes back on screen via a matching
`kv_label`. Screens open from block interactions shaped
`screen:<qualified id>` — pure presentation, identical solo and as a
guest — or from scripts via `open_screen("<qualified id>")`.

## What shipped

- **Schema** (`screens.toml`, schema_version 1): qualified ids, duplicate/
  empty-title/empty-action failures ride `material_errors`, so a bad file
  fails the MODS screen and `--mod-qualification`.
- **Runtime**: `Screen::Mod(usize)` indexes `Registry::screens` (the E7
  MachineKind pattern). One draw arm renders every def; one click arm
  routes toggles client-locally and buttons to the hook. Escape/E close,
  inventory grid, browser, and held-stack all work like native screens.
- **Authority**: on a guest, a button rides `C2S::ScreenClick`; the host
  validates BOTH ids against its own registry — a tampered client can
  only ever name buttons its content actually declares — then hands the
  click to the windowed host over `HostFx::ScreenClick`, because scripts
  live there. A dedicated server owns no script engine and drops clicks
  with that same guarantee.
- **Scripts**: registered `open_screen(name)` pushes `Cmd::OpenScreen`;
  unknown ids toast instead of silently failing.
- **Sample**: gems gains `gem_altar` (`interaction = "screen:gems:altar"`)
  and `gems:altar` — a label, an offering counter readout, a tracking
  toggle, and an offering button.
- **Tests**: 3 model tests (qualify/order, empty-action failure,
  duplicate failure), 2 lint tests (invalid fails with the reason; valid
  screen + bound block passes), and a bundled registry test asserting
  `gems:altar` resolves, has its button, and the altar block's
  interaction opens it. Suite green.

## Verification

- `cargo clippy --lib --tests -- -D warnings` clean.
- `cargo test --lib` green (1007 passing, 0 failed, 23 ignored).
- `wildforge --mod-qualification mods` PASS.
- Visual + geode qualification hashes refreshed (`src/lib.rs` gained a
  module).

## Content notes

- Base ships no screens; the capability is engine-only and demonstrated
  by the gems altar. Belt-quest authors real screens.
- Widget rows render grouped by kind (labels, kv labels, toggles,
  buttons) at one row per widget beneath the title; the player's
  inventory sits below as on every other container screen.
- Player-KV keys are the whole state model: scripts write them via
  `storage_get/storage_set` (mod namespace) or quest/dialogue paths, and
  screens read/write them through kv_labels and toggles.

## Out of scope

- Free-form layouts, images/icons per widget, scrolling lists, and text
  input on mod screens.
- Host-side screen sync: screens are client-local presentation by design;
  authority flows only through validated button clicks.
