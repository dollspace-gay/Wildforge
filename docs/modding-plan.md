# Wildforge Native Mod System — Design Plan

## Goals

- Mods are first-class: vanilla content registers through the same API mods use
  (the "base" mod is built into the binary but goes through the front door).
- Hot-loadable during development: edit a mod file, see it in the running game
  within a second, without losing world or mod state.
- Safe for end users: mods cannot touch the filesystem/network; a broken mod
  degrades gracefully instead of crashing the game or corrupting saves.
- Save-compatible: worlds survive mods being added, removed, or updated.

## Non-goals (v1)

- Native (dylib) mods — Rust has no stable ABI and unloading is UB-prone.
  May become an explicit opt-in "dev tier" later.
- Client/server mod sync (no multiplayer yet).
- Scripted worldgen terrain shaping (data-driven features only in v1;
  deterministic scripted gen hooks are a later phase).

## Architecture: three tiers

1. **Data mods** (TOML + PNG) — new blocks, items, recipes, worldgen features.
   No code. Covers the majority of real-world content mods.
2. **Script mods** (embedded scripting runtime behind a `ModBackend` trait) —
   event-driven behavior: interactions, custom drops, timers, HUD, etc.
3. **(Later) WASM tier** — full-power sandboxed mods in any language via
   wasmtime, using the same host API surface as tier 2.

## Phase 1 — Dynamic registries & save format v2 (the enabler)

Replace the hardcoded `Block`/`Item` enums with registries:

```rust
struct BlockId(u16);                 // dense index into the registry
struct BlockDef {
    id: String,                      // "base:stone", "copper:ore"
    name: String,
    tiles: [TileRef; 6],             // atlas slots, resolved at load
    hardness: Option<f32>,           // None = unbreakable
    tool: Option<ToolKind>,
    requires_tool: bool,
    drops: DropRule,                 // SelfDrop | Item(String, u32) | None
    solid: bool, opaque: bool, translucent: bool,
    interactive: bool,               // right-click opens something
    fluid: Option<FluidDef>,         // water-like behavior parameters
}
```

- Property lookups become `registry.block(id)` array indexing — same speed
  class as the current enum matches.
- Chunk storage widens `u8 -> u16`. Save format v2: magic + version header,
  RLE over (count: u16, id: u16).
- **Per-world palette**: `saves/<world>/palette` maps numeric ids to string
  ids. On load, stored ids remap through the current registry by string;
  unknown ids become a visible `base:unknown` placeholder block. This is what
  makes saves robust across mod changes.
- v1 worlds (no header) load through the legacy fixed palette and re-save
  as v2 — existing worlds survive.
- Items and recipes get the same treatment (`ItemId(u16)`, string-keyed
  recipe registration resolved at load).
- Vanilla becomes `mods/base` semantically: registration data embedded in the
  binary via `include_str!`, run through the identical loading path.

## Phase 2 — Mod packages & data mods

```
mods/copper/
  mod.toml          # id, name, version, depends = ["base"]
  blocks.toml
  items.toml
  recipes.toml
  features.toml     # worldgen: ores, plants
  textures/*.png    # 16/32/64 px tiles, packed into the atlas at load
  main.rhai         # optional behavior script (phase 3)
```

Example content, zero code required:

```toml
[[block]]
id = "copper:ore"
name = "Copper Ore"
texture = "copper_ore.png"        # or { top = "...", side = "...", bottom = "..." }
hardness = 6.5
tool = "pickaxe"
requires_tool = true
drops = { item = "copper:raw" }

[[feature]]
type = "ore"
block = "copper:ore"
replaces = "base:stone"
vein_size = 6
per_chunk = 8
y_range = [8, 48]

[[recipe]]
pattern = ["ccc", ".s.", ".s."]
c = "copper:ingot"
s = "base:stick"
output = { item = "copper:pickaxe", count = 1 }
```

- Load order: topological sort by `depends`; cycles are a load error.
- Atlas packer assigns tile slots at load; mod textures may be any supported
  tile resolution (scaled to the atlas resolution); missing texture -> magenta
  checkerboard placeholder, not a crash.
- Worldgen features run in deterministic per-chunk order with a seeded RNG —
  no cross-chunk writes, same constraint the native tree pass already obeys.

## Phase 3 — Script runtime & event API

- `trait ModBackend` so the runtime is pluggable (script now, WASM later).
- Events dispatched by function-name convention in the mod's script:

```
on_load()
on_world_start(world_info)
on_block_break(player, pos, block_id) -> bool   // false cancels
on_block_place(player, pos, block_id) -> bool
on_interact(player, pos, block_id)              // custom interactive blocks
on_craft(player, recipe_id)
on_player_respawn(player)
on_tick(dt)                                     // budgeted; slow mods throttled
```

- Host API exposed to scripts: `get_block/set_block`, `spawn_item`,
  `give(player, item, n)`, player pos/health, `play_sound`, `hud_message`,
  `log`, and a **per-mod persistent KV store** (`storage_get/set`) owned by
  the engine and saved with the world.
- Script state model: code is disposable, state lives in the engine KV.
  This is what makes hot reload and save/load coherent (Factorio's model).
- Sandboxing: no filesystem/network access; instruction/fuel limits per event
  dispatch so a runaway script can't hang the frame.

## Phase 4 — Hot reload

- `notify` file watcher on `mods/` (debounced ~200 ms) + F5 manual reload.
- Data change: rebuild registry (numeric ids stay stable within the session;
  new ids append; removed defs -> placeholder), repack + re-upload atlas,
  mark all loaded chunks dirty (budgeted remesh). Works mid-game.
- Script change: recompile that mod's AST only. On error: **keep the old
  version running**, surface the error as a HUD toast and on the MODS screen.
  A typo must never kill a dev session.
- Title screen gains a MODS entry: list, version, load state, errors.

## Later phases

- WASM tier (wasmtime + WIT-defined host API mirroring the script API).
- Scripted worldgen hooks with enforced determinism.
- Per-world mod selection; mod config screens; dependency version ranges.
- Networked mod sync when multiplayer lands.

## Testing

- Registry: id palette round-trip, unknown-block placeholder, v1->v2 save
  migration.
- Data loading: full parse of a fixture mod; bad-input errors are load
  errors, not panics.
- Events: headless world + scripted mod fixture exercising each event.
- Hot reload: registry rebuild keeps ids stable; script error keeps old AST.

## Decisions (2026-07-17)

1. **Scripting runtime: Rhai** — pure Rust (no C deps in the windows-gnu
   cross build), sandboxed by default, per-event fuel limits, painless hot
   reload. The `ModBackend` trait keeps Lua/WASM backends possible later
   without redesign.
2. **Status: plan approved, implementation not started.** Phase 1 (registry
   refactor + save format v2 + palette) is the agreed starting point when
   implementation kicks off.
