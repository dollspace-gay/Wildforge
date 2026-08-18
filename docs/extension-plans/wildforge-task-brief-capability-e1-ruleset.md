# Task Brief: Ruleset / Game-Mode System (capability E1)

**Repo:** WildForge, branch `feat/capability-ramp`
**Primary files:** `src/ruleset.rs`, `src/registry.rs` (mode parse +
resolution), `src/world/mod.rs` (world-side gates), `src/world/ecology.rs`
(ire/hostile/weather gates), `src/world/calendar.rs` (hearts gate),
`src/game/status.rs` (hunger/fall/drown/lava gates),
`mods/gems/modes.toml` (data-only sample), `mods/README.md`
**Related design doc:** `belt-quest/docs/port-plan.md`, capability E1.

## Goal

Let a mod declare a named **ruleset** a world's `mode` string can name, so
the mod can *mod out* WildForge systems that conflict with its design. This
is the belt-quest "no punishing survival" / "PvE-only" / "ire repointed"
mechanism, shipped generically.

## What shipped

- `src/ruleset.rs` — `Ruleset` (10 toggles: `creative`, `hunger`,
  `fall_damage`, `drowning`, `lava_burn`, `hostile_spawns`, `ire`, `hearts`,
  `weather_extremes`, `pvp`), `Ruleset::survival()` / `creative()`
  built-ins, and `apply_overrides(&ModeDef)`.
- `modes.toml` — a new optional mod file, `[[mode]] { id, base, <toggles> }`.
  A mode inherits every toggle from its `base` (`survival`, `creative`, or
  another declared mode) and overrides the fields it lists. Built-in ids are
  reserved; undeclared/cyclic bases are load errors that fall back to
  survival.
- `Registry::ruleset_for(mode)` — resolves a world's `mode` string through
  the declared mode chain. Unknown modes fall back to survival (never strip
  safety netting).
- `World::ruleset()` — the authoritative accessor.
- World-side gates: ire accrual (block break + mob kill), hostile warden
  spawning, hearts loop, storm spawn bonus, and PvE-only (player projectiles
  pass through players when `pvp` is off).
- Client-side gates in `game/status.rs`: hunger drain/starvation, fall
  damage, drowning, lava burn.
- Sample data-only mode shipped in `mods/gems/modes.toml` (`gems:cozy`).
- 7 unit/integration tests in `src/ruleset.rs`, including a world sim test
  proving a declared mode suppresses wardens on a wrathful night and accrues
  no ire for breaking ore.
- Both visual-polish qualification hashes refreshed (`src/lib.rs` is a
  `QUALIFICATION_SOURCES` file).

## Verification

- `cargo clippy --lib --tests -- -D warnings` clean.
- `cargo test --lib` green (suite now 926 passing).
- `wildforge --mod-qualification mods` PASS (gems declares a mode; a broken
  mode's error surfaces through `material_errors`).

## Out of scope (later capabilities)

- **E12 Industrial Response** repoints the ire *source* to factory output;
  E1 only toggles whether extraction/kills accrue ire at all.
- Third-person camera (E2) and the rest of the ramp.

## belt-quest integration

belt-quest ships `mods/belt_quest/modes.toml` declaring its ruleset (hunger
off, ire repointed by E12, hearts off, PvE-only), and its worlds are created
with `mode = "belt_quest:<mode>"`.
