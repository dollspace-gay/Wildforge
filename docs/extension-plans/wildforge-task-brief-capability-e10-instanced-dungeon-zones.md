# Task Brief: Instanced Dungeon Zones (capability E10)

**Repo:** WildForge, branch `feat/capability-ramp`
**Primary files:** `src/planet.rs` (`Face::Deep`, the seventh face; basis/
edge/canonicalization guards), `src/world/dungeon.rs` (new — the run state
machine), `src/world/chunks.rs` (bare Deep chunk adoption), `src/worldgen.rs`
(void generation + grassland reads), `src/planet_atlas.rs` (face-guarded
atlas lookups), `src/world/hearts.rs` + `src/server.rs` (geography defaults,
the overworld-freeze consensus gate, run clocks),
`src/world/pieces.rs` (Deep runs float at a fixed floor line),
`src/registry.rs` (`DungeonDef` on assemblies), `src/world/storage.rs`
(Deep chunks never persist), `src/game/actions.rs` + `src/game/survival.rs`
(entry/exit/checkpoint interactions, dungeon death rules),
`src/multiplayer/host.rs` + `src/net/protocol.rs` (`C2S::DungeonUse`,
PROTOCOL 44).
**Related design doc:** `belt-quest/docs/port-plan.md`, capability E10
(row 105), deps E8 (feeds) / E11 (HUD).

## Goal

Instanced dungeon zones: separate-zone load/unload with the overworld
freezing on entry, room-graph generation from templates (the assembly
walk), checkpoints + dungeon respawn, loot tables, and world-timer reset
on exit.

## The design in one paragraph

The world gains a **seventh face — the Deep** — appended to the stable
`Face` enum (id 6) so every existing save and wire id is untouched. It is
pure addition: the six surface faces keep every block, and Deep chunks
live in the same chunk store, stream through the same pipeline, collide,
mesh, and render like any other chunks. Worldgen emits void there and
chunk adoption skips bedrock/water/ruins/hearts/ecology entirely;
planetary geography has no cells below, so atlas reads miss benignly.
Because **a player is "in a dungeon" exactly when their position's face is
the Deep**, no participant bookkeeping exists to drift or sync.

## What shipped

- **Runs**: an assembly whose def carries `[assembly.dungeon]`
  (`reset` secs) never generates at worldgen. `World::enter_dungeon`
  stamps it on demand into one of 1024 reserved 16×16-chunk slots via the
  ordinary connector-typed assembly walk (rooms float at a fixed floor
  line, `DEEP_RUN_Y = 48`); void chunks generate instantly so stamping is
  synchronous — nobody falls through an ungenerated zone. Exit routes each
  participant back to their own recorded door.
- **Reset on exit**: `tick_dungeon_runs` dissolves a run after its def's
  delay once empty, dropping its chunks unsaved. Deep chunks are excluded
  from persistence at the single `save_chunk` choke point, so every entry
  regenerates fresh — the world-timer reset is structural.
- **Overworld freeze**: when *every* connected player stands below, the
  sun, planetary weather/Current/dross/ecology, ire decay, dawn, fluids,
  and crop ticks hold their breath (the sleep-vote consensus rule; solo
  play always qualifies). Industry, machines, belts, and dungeon creatures
  keep ticking. Hostile ring spawns already pause below (no country heart)
  and wildlife restock skips deep players.
- **Checkpoints + respawn**: a block interacting as `dungeon_checkpoint`
  sets the party-shared checkpoint; death inside a run keeps all
  belongings (no scatter) and respawns there — solo directly, guests via
  the host's respawn path. Ordinary deaths elsewhere are untouched.
- **Entry/exit**: blocks interacting as `dungeon_entry:<assembly>` /
  `dungeon_exit`; guests send `C2S::DungeonUse { pos, kind }` and the host
  validates the block, runs the machine, and teleports via a full
  `PlayerState` snap. PROTOCOL bumped to 44.
- **Loot**: flows through the existing piece-chest machinery unchanged —
  mods bind loot tables to their dungeon pieces; the engine adds nothing
  and removes nothing.
- **Tests** (`src/tests/dungeon.rs`): face id stability + wire round-trip;
  void generation + grassland habitat reads + no heart below; entry stamps
  rooms into the slot and exit returns the participant's own position; an
  emptied run resets past its delay and drops its chunks unsaved, then
  re-entry takes a fresh slot. The obsolete "no face 6" planet invariant
  was updated to pin Deep's id instead.

## Verification

- `cargo clippy --lib --tests -- -D warnings` clean.
- `cargo test --lib` green (1001 passing, 0 failed, 23 ignored).
- `wildforge --mod-qualification mods` PASS.
- Geode qualification hash refreshed; visual hash unchanged.

## Content notes

- Engine-only again: base ships no dungeons. A mod declares pieces +
  pools as usual, marks its assembly with `[assembly.dungeon]`, and places
  entry/checkpoint/exit blocks (any blocks with the right `interaction`
  strings). Dungeon piece floors belong at the anchor level; the entry
  piece's origin cell should be walkable — participants arrive standing on
  it.
- Slots bound a run to 256×256 blocks; `max_pieces` budgets keep walks
  inside. Two concurrent runs never share chunks.
- Mob cap remains global (320): dungeon populations share it with the
  surface. Nest/controller caps from E9 bound dungeon populations naturally.

## Out of scope (later capabilities)

- E11 dungeon HUD (timers, party status) on the mod-screen layer.
- Per-run difficulty scaling and boss mechanics (E9 archetypes are the kit).
- Cross-run state (shared cooldowns, best-time records) if belt-quest wants them.
