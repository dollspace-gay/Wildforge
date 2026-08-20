# Task Brief: Belt Geometry & Persistence (capability E8)

**Repo:** WildForge, branch `feat/capability-ramp`
**Primary files:** `src/world/belt.rs` (geometry model `BeltKind`, the
entry-direction-aware `BeltState`, the generalized `tick_belts`, and
machine-feed mouth handling), `src/world/machines.rs` (`machine_feed_def`/
`machine_mouth_full`/`machine_insert_at`), `src/world/entities.rs`
(persisted `[[belt]]` records, entities.toml version 10),
`src/world/rail.rs` (`direction_offset` promoted to `pub(crate)`),
`base/blocks.toml` (eleven new belt pieces), `src/net/protocol.rs`
(PROTOCOL 43 + `C2S::ToggleSwitch`/`S2C::SwitchState`),
`src/multiplayer/host.rs`, `src/game/remote.rs`, `src/agent/mod.rs`
(toggle broadcast/apply), `src/game/actions.rs` (the shared
`rail_switch|belt_switch` interaction with a remote branch).
**Related design doc:** `belt-quest/docs/port-plan.md`, capability E8
(row 103), deps E7 (machine feed) / E10 (dungeon-lair feeds).

## Goal

Give belts the rail line's geometry — curves, inclines, switches,
splitters/mergers, and belts orientable in all four directions — and make
belt cargo durable instead of transient. A belt's mouth can also feed a
machine directly (the E7 data-driven machine contract) instead of only
dropping loose items. Stock behavior is unchanged: `base:belt` is the
North straight, empty belts coast at nominal speed, back-pressure stalls
and draws nothing.

## What shipped

- **Geometry model** (`src/world/belt.rs`): `BeltKind` grows from a lone
  `Straight` to `Straight(Direction4)`, `Curve(NE/NW/SE/SW)`, `Incline`,
  `Switch`, `Splitter`, `Merger`, classified from block ids exactly like
  `RailKind` (`BlockDef` has no per-block orientation). `belt_exit(pos,
  kind, entered)` mirrors `rail_exit` (curves/splitters/mergers need to
  know which way cargo entered), and `belt_next(pos, exit)` mirrors
  `rail_next` including the incline's climb link and the descent rejoin.
- **Cell state**: `BeltState` carries `entry_dir` (set on each handoff)
  and `split_phase` (the splitter's alternating bit). Splitter cells are
  retained even empty so a drained line resumes alternating.
- **Tick rewrite**: `tick_belts` advances the front item along
  `belt_exit`/`belt_next`; splitters prefer their phase exit and fall back
  to the other when one side is blocked (a two-full-outputs case stalls);
  mergers take South/West and merge North; switches read `switch_selected`;
  incline cells pay the 1.5× climb multiplier on their draw. Back-pressure
  and mouth checks follow the exit direction. Cargo in an **unloaded
  chunk** parks rather than being read as air and spilled; it resumes when
  the chunk loads.
- **Content** (`base/blocks.toml`): eleven new world-only pieces
  (`belt_s/e/w`, four curves, `belt_incline_n`, `belt_switch` with
  `interaction = "belt_switch"`, `belt_splitter`, `belt_merger`) sharing
  the rail blocks' shape. No items, recipes, or drops — crafting belts is
  the belts-quest mod's business.
- **Machine feed (E7 dep)** (`src/world/machines.rs`): `machine_feed_def`
  resolves a mouth block's interaction to a fire-handler machine with
  charge slots whose shell actually validates (the separator's counters
  and the workbench station are deliberately not belt-fed). `machine_mouth_full`
  back-pressures a belt when the charge is full; `machine_insert_at`
  merges into a matching charge slot then the first empty one, creating the
  machine instance on first feed (the belt as the machine's first touch),
  and returns the leftover for a loose drop.
- **Persistence** (`src/world/entities.rs`): belt cargo joins
  `entities.toml` as `[[belt]]` records (pos, `entry_dir`, `progress`,
  `split_phase`, cargo `[[belt.slot]]` stacks by item name/durability/
  arcane_id). Header bumped to version 10; the loader accepts `(9..=10)`
  so pre-E8 saves still load (they simply have no belt records) and parses
  `belt` only from version 10 onward. Belt stacks ride the existing
  arcane durable-item audit automatically (entities.toml is already a
  durable path). The belt module doc and the `World::belt_state` comment
  no longer call the cargo transient.
- **Multiplayer**: `PROTOCOL` bumped to 43. `C2S::ToggleSwitch { pos }`
  asks the host to flip a switch; the host validates the block's
  interaction, toggles the authoritative `SwitchState`, and broadcasts
  `S2C::SwitchState { pos, selected }` to every guest (incl. the actor).
  Guests and the headless agent mirror it into their `block_entities`
  (the `SignText` pattern). The `actions.rs` interaction arm now covers
  `rail_switch | belt_switch` with a remote branch — **this also fixes
  rail switches in multiplayer**, which were previously toggled locally
  on the guest only (a no-op for the host's simulation).
- **Tests**: ten new `src/tests/belt.rs` cases (all 17 pass): orientable
  straights, both curve turns, the incline climb, switch default/branch,
  splitter alternation, merger, back-pressure through a curve, machine
  feed + full-machine back-pressure, cargo save/reload round-trip (cargo
  + progress + resumed motion), and splitter-phase save/reload
  (alternation resumes). Existing rail, machines, multiplayer, registry,
  and mod-lint suites are unchanged and green.

## Verification

- `cargo clippy --lib --tests -- -D warnings` clean.
- `cargo test --lib` green (983 passing, 0 failed, 23 ignored).
- `wildforge --mod-qualification mods` PASS.
- Geode qualification hash refreshed (`src/world/mod.rs` is a
  qualification source; the visual hash is unchanged).

## Content notes

- `base:belt` is unchanged (the North straight), so existing belts and
  saves behave exactly as before.
- Belt pieces are world-only blocks (no item/recipe/drop), matching rails;
  survival crafting is a belts-quest content decision, not base's.
- Belt cargo is host-authoritative in multiplayer; guests see mouth drops
  via the loose-item stream but not on-belt cargo. Switch state is now
  host-authoritative via the toggle message.
- Orientation stays encoded as distinct block ids (rail's convention);
  no per-block orientation field was added to `BlockDef`.

## Out of scope (later capabilities)

- E10 dungeon/lair feeds (belt deliveries into dungeon/lair drop points).
- Belt rendering/interpolation (`BeltState.progress` remains the future
  renderer's hook, exactly as with rails).
- Survival-craftable belt/rail items and recipes (belts-quest mod).
- Additional belt handlers (powered fast belts, item filtering).
