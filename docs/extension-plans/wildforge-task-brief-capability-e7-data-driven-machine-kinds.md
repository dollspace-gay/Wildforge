# Task Brief: Data-Driven Machine Kinds (capability E7)

**Repo:** WildForge, branch `feat/capability-ramp`
**Primary files:** `src/machines.rs` (new — pure model: `MachineHandler`,
`MachineDef`, `RawMachineToml`, `parse`/`resolve` + tests),
`src/world/multiblock.rs` (the closed `MachineKind` enum becomes an index
newtype with registry lookups), `src/world/machines.rs` (`mouth`/
`validate`/`edit_region` per handler, `light_machine_at`,
`revalidate_machine_at` read the def), `src/world/machine_tick.rs`
(tick fns dispatch by handler), `src/registry.rs` (`machines.toml`
parsing + `reg.machines` + interaction→machine resolution),
`src/game/actions.rs` (interaction dispatch by handler; workbench arm),
`src/game/containers.rs` (click validation by handler; workbench
crafting), `src/game/ui.rs` + `menus.rs` + `keymap.rs` +
`input.rs`/`tooltip.rs` (the `Screen::Workbench` recipe-list screen),
`src/net/protocol.rs` (new `S2C::MachineContainer`, `PROTOCOL` bumped),
`src/multiplayer/host.rs` + `src/game/remote.rs` (machine open/snapshot
by id), `src/world/{mod.rs,template.rs,entities.rs,local_structure.rs}` +
`src/game/frame.rs` (placement, stamping, persistence, smoke),
`base/machines.toml` (the four built-ins declared as data),
`mods/gems/{machines.toml,blocks.toml,items.toml,recipes.toml}` +
`mods/README.md` (the `jewel_bench` sample).
**Related design doc:** `belt-quest/docs/port-plan.md`, capability E7
(row 102), deps E0/E11. `Screen::Workbench` is a temporary hardcoded
screen; E11 generalizes the screens layer for mods.

## Goal

Open the closed `MachineKind` enum and the `interaction` match so a mod
can declare a new machine — with its shell, recipes, and screen — in
data, without touching the engine. Kinds become registry entries
(`machines.toml`) bound to a closed native handler; the engine dispatches
on the handler. Built-in Survival/Creative behavior is numerically
identical because the four base machines are the same rows, just declared
as data.

## What shipped

- **Content schema** (`machines.toml`, schema_version 1): `[[machine]]`
  entries `{ id, label, handler, mouth, mouth_lit?, fire_secs,
  charge_slots, fuel_slots, reagent_slots, items_per_fuel, min_charge,
  min_fuel }`. Ids qualify to the declaring mod. Unknown handlers,
  empty labels/mouths, a lit face on a non-fire handler, or duplicate ids
  fail the pack on the MODS screen and fail `--mod-qualification`. 5
  unit tests in `src/machines.rs`.
- **Engine model** (`src/machines.rs`): the **closed handler set**
  `Bloomery | Forge | Kiln | Separator | Workbench` with the predicate
  surface the engine branches on (`has_fire`, `hand_fed`, `is_station`,
  `reads_chimney`, `requires_anvil`). `MachineDef` carries every knob the
  handlers read.
- **Enum → data** (`src/world/multiblock.rs`): `MachineKind` is now
  `struct MachineKind(u16)` — the index into `Registry::machines` — with
  `name(reg)`/`from_name(reg, _)`/`handler(reg)`. Kind 0 is the default
  and base declares its machines first so it is always real. The ~90
  closed-match sites across shells, ticks, clicks, persistence, templates,
  frames, and multiplayer were generalized to handler/dispatch lookups.
- **Machine behaviors** (`src/world/machines.rs` + `machine_tick.rs`):
  `mouth`, `validate` (stack shell + forge's chimney/anvil), `edit_region`
  (kiln chimney, forge union), `light_machine_at` (def-driven charge
  minimums + lit face), `revalidate_machine_at` (def-driven unlit face),
  and the four tick fns (filter + fire_secs + separator lit-face all read
  the def). Block placement, break-spill, save/load, structure hosting,
  template stamping, and the smoke emitter all key off the def/handler.
- **Interaction match opened** (`src/game/actions.rs`): machine
  interactions resolve through `reg.machine_by_interaction` and dispatch
  by handler (hand-fed separator pattern, fire machines' screens,
  workbench stations). Click validation in `containers.rs` and the solo
  light path dispatch by handler too.
- **Workbench machines**: `handler = "workbench"` kinds open
  `Screen::Workbench`, a recipe-list station. The screen lists every
  `[[recipe]]` whose `station` is the machine's id and crafts it from the
  inventory (distinct-slot ingredient consumption, `recipe_gates_met`
  gating, output + `craft` XP). Station recipes stay off the free grid,
  so the machine is their only reach path.
- **Persistence + templates**: save/load uses the machine id string
  (`MachineKind::from_name(reg, …)`, unknown kinds drop safely); structure
  hosting mirrors it; template stamping resolves kinds by interaction.
- **Multiplayer**: `PROTOCOL` bumped to 42; machine containers ride the
  new `S2C::MachineContainer { machine: String, slots, aux }`, remapped
  host→guest by id like the item palette. Host open/snapshot and guest
  reconstruction dispatch by handler (kiln 9 slots, bloomery/forge 8,
  workbench none).
- **Sample mod**: `mods/gems/machines.toml` declares `gems:jewel_bench`
  (workbench) with its mouth block `jewel_bench` (`interaction =
  "gems:jewel_bench"`, crafted from 4 firebrick); `gem_circlet` is a
  head-armor item reachable only through the bench's station recipe.
  `mods/README.md` documents the schema, the closed handler set, the
  workbench screen, and the multiplayer rule.
- **Tests**: 5 `machines` model tests, 2 mod-lint tests
  (`an_invalid_machine_fails_qualification`,
  `a_station_machine_and_recipes_pass_qualification`), and a bundled-mods
  registry test asserting `gems:jewel_bench` resolves and its station
  recipe binds. The existing ~120 machine/materials/multiplayer tests
  were updated for the newtype and all pass unchanged. Suite green.

## Verification

- `cargo clippy --lib --tests -- -D warnings` clean.
- `cargo test --lib` green (973 passing, 0 failed, 23 ignored).
- `wildforge --mod-qualification mods` PASS.
- Visual + geode qualification hashes refreshed (src/lib.rs, src/game/
  frame.rs, src/world/mod.rs are qualification sources).

## Content notes

- Base ships no modded machines and the four built-ins behave exactly as
  before — stock gameplay is unchanged.
- The workbench handler is the first non-furnace machine kind; more
  handlers (armory, tannery, sensor tower, turret, assembler) are the C6
  content path once their engine behaviors are specified.
- `MachineKind`'s registry index is load-order stable within a pack; the
  save format stores the qualified id, so index drift across packs is
  harmless.

## Out of scope (later capabilities)

- E11 mod-extensible screens: the hardcoded `Screen::Workbench` becomes a
  generic mod screen layer.
- C6 belt-quest machine content (armory/tannery/turret/etc.) and their
  recipes.
- Additional closed handlers with brand-new engine behaviors (powered
  fabrication, sensor logic) — deferred until belt-quest specifies them.
