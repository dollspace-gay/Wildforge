# Task Brief: Player Stat Surface (capability E4)

**Repo:** WildForge, branch `feat/capability-ramp`
**Primary files:** `src/stats.rs` (new — StatKind/StatModifier/StatBlock
modifier surface), `src/game/stats.rs` (new — Game surface plumbing:
equipment + preparation aggregation, derived `max_health`/`stamina_max`/
`stamina_regen`/`carry_capacity`/`reach`/`scan_range`/`move_speed`),
`src/registry.rs` (`ItemDef.carry_weight` + `ItemDef.stats`, `[[item.stats]]`
content, 6 constructor sites), `src/alchemy.rs` (`PreparationModifiers`
gains 6 multiplier channels), `src/inventory.rs` (`Inventory::total_weight`),
`src/game/status.rs` (carry-gated walk-over pickup), `src/physics.rs`
(`Input.speed_mult`), `src/game/actions.rs`/`app.rs`/`frame.rs` (derived
reach call sites, observation radius), `src/game/ui.rs` (stamina uses
derived max; carry burden meter), `src/arcane_ecology.rs` +
`src/world/mod.rs` (observation radius parameter).
**Related design doc:** `belt-quest/docs/port-plan.md`, capability E4.

## Goal

Generalize the stat model to a modifier surface: health, stamina,
stamina regen, carry capacity, build range, scan range, and move speed
each derive from `(base + flat) * permille`, with modifiers coming from
worn equipment (`[[item.stats]]`) and active preparations
(`PreparationModifiers`). Skills (E5) and loadouts (E6) plug into the
same surface later.

## What shipped

- **Modifier surface** (`src/stats.rs`): `StatKind` (health, stamina,
  stamina_regen, carry, build_range, scan_range, move_speed),
  `StatModifier { kind, flat, mult_permille }`, and `StatBlock` —
  flat sums add, permilles multiply (u64 intermediate so chained
  permilles cannot overflow). `effective = (base + flat) * mult / 1000`,
  floored at zero. 5 unit tests cover identity, flat+permille, chained
  permilles, merge, and content-name parsing.
- **Equipment source**: `ItemDef.stats: Vec<StatModifier>` +
  `ItemDef.carry_weight: u32` (default 1) declared in `[[item]]` content
  as `[[item.stats]]` (`kind = "carry"`, `flat = 128`,
  `mult_permille = 1200`). The 5 worn armor slots aggregate into the
  surface; a `stats`/`carry_weight` integration test loads a scratch mod.
- **Consumable source**: `PreparationModifiers` gains
  `health_permille`, `stamina_regen_permille`, `carry_permille`,
  `reach_permille`, `scan_permille`, `move_speed_permille` (serde
  defaulted to 1000) alongside the existing stamina/recovery/perception
  channels; all map into the same `StatBlock` and multiply rather than
  stack with equipment.
- **Routed stats**:
  - Health → `max_health()` (base 14 + nutrition bonus, then surface).
  - Stamina/StaminaRegen → `CombatState::tick` clamps/regen to derived
    max/rate; creative and respawn/world-entry refill to derived max;
    the HUD bar uses derived max.
  - Carry → new mechanic: `Inventory::total_weight` (count × carry
    weight), 512-unit base capacity, survival pickups that would exceed
    capacity stay on the ground (creative ignores it), and a carry
    burden meter under the stamina bar (green→amber→red).
  - BuildRange → derived `reach()` feeds every interaction raycast
    (mine/place/aim/bucket/pick-block/outline).
  - ScanRange → derived observation radius threaded through
    `arcane_ecology_observation_at`/`perceived_arcane_ecology_at`
    (was a hardcoded 72.0; host/agent/guest paths keep 72.0, the local
    player uses `scan_range()`).
  - MoveSpeed → `physics::Input.speed_mult` multiplies walk/sprint/swim
    speed (agents and test drivers pass 1.0).
- **Tests**: 5 `stats` tests + combat suite updated for the new `tick`
  signature + registry stat parse test + `Inventory::total_weight`
  assertion in the existing inventory test. Full suite 943 passing.

## Verification

- `cargo clippy --lib --tests -- -D warnings` clean.
- `cargo test --lib` green (suite now 943 passing, 23 ignored).
- `wildforge --mod-qualification mods` PASS.
- Visual + geode qualification hashes refreshed (src/game/mod.rs,
  src/game/frame.rs, src/lib.rs, src/world/mod.rs are qualification
  sources).

## Content notes

- No base item declares `stats` or a non-default `carry_weight` yet, so
  stock gameplay is numerically identical; the surface is ready for
  equipment (E6) and skill (E5) content to modulate it.

## Out of scope (later capabilities)

- E5 skill trees: nodes grant `StatModifier`s into the same surface.
- E6 modular equipment: frame/component loadouts compute the equipment
  `StatBlock`; durability-to-zero but never destroyed.
- Carry capacity persistence (a derived value, not stored; E14 may
  revisit).
