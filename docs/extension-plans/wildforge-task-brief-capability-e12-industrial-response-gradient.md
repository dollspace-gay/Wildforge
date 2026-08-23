# Task Brief: Industrial Response Gradient (capability E12)

**Repo:** WildForge, branch `feat/capability-ramp`
**Primary files:** `src/ruleset.rs` (`Ruleset.industrial_ire` + mode
override), `src/registry.rs` (`ModeDef.industrial_ire`, modes.toml
field), `src/world/calendar.rs` (`tick_industrial_ire`,
`INDUSTRIAL_IRE_PER_SEC`, `INDUSTRIAL_BUILDING_IRE`),
`src/world/mod.rs` (accumulator, `RegionCell::any_surface`,
the placement charge in `place_block_at`), `src/world/machine_tick.rs`
(tick call site), `src/world/ecology.rs` (bold grading at tier 2+),
`src/mobs.rs` (fear scaling), `mods/README.md`.
**Related design doc:** `belt-quest/docs/port-plan.md`, capability E12
(row 107), dep E1 (modes).

## Goal

Spec 3.7: factory output — buildings, machines, pollution-adjacent work —
feeds regional ire alongside extraction, and ire tiers drive a wildlife
aggression gradient.

## What shipped

- **Industrial feed**: every lit fire machine (bloomery, forge, kiln,
  separator) charges its region **0.01 ire/sec** on a one-second beat —
  one charge per distinct region cell, so a workshop row smokes as one
  chimney. A full bloomery firing (~300 s) costs roughly what mining its
  ore did; industry is now part of the moral meter, not just extraction.
- **Buildings**: placing any machine mouth costs a one-time
  **0.5 ire** in the valley that hosts it.
- **Aggression gradient**: at global tier ≥2 a hungry predator sizes up
  the player regardless of season or hour (hunger stays a hard
  requirement), and skittish wildlife startles from **75%** of their
  flight distance at tier 2 and **half** at tier 3. Wrathful country is
  visibly nervous country.
- **Mode gate (E1)**: `industrial_ire = true/false` on a mode repoints
  the whole gradient off; it defaults live wherever `ire` is live, so
  Survival behavior gains the feed and Creative never had one.
- **Tests**: machines/buildings feed ire (lit bloomery accrues over a
  minute of ticks; raising the mouth charges once); the mode override
  plumbing flips the feed off; wrathful country shortens deer flight
  distance while calm country does not. Suite green.

## Verification

- `cargo clippy --lib --tests -- -D warnings` clean.
- `cargo test --lib` green (1010 passing, 0 failed, 23 ignored).
- `wildforge --mod-qualification mods` PASS.
- Geode qualification hash refreshed (`src/world/mod.rs` changed); visual
  hash unchanged.

## Content notes

- No base content changed: the feed reads lit fire machines generically
  through the E7 handler contract, so modded fire machines smoke for
  free, and the mode field documents the toggle.
- Calibration: 0.01/s per machine keeps a full firing comparable to
  mining its inputs; regional decay (-2/day toward zero) still lets a
  tended valley absorb steady light industry.
- The gradient reads the GLOBAL tier (as hostile spawn budgets already
  do); regional texture arrives through the existing local-weighted
  spawn ring.

## Out of scope

- Pollution visuals/particles and machine-specific emission rates per
  def (a natural E7 follow-up: an optional `ire_per_sec` knob).
- Tier-driven wildlife abundance (repop pacing) — the row asked for
  aggression, which boldness + fear deliver.
