# Task Brief: Spawn & Flag-Gated Markers (spec 2.4 / 2.5)

**Repo:** WildForge, new branch `feat/flag-gated-markers` (off
`feat/npc-runtime`)
**Primary files:** `src/registry.rs` (`GateDef` + load), `src/world/pieces.rs`
(`AssemblyMarker` doc/kind surface), `src/world/chunks.rs` (marker consumer:
`feature:<id>` arm), `src/world/mod.rs` (gated-position registry + accessors,
`break_block_at` gate), `src/game/actions.rs` (locked interaction toast),
`src/game/dialogue.rs` (flag read for gates), `base/features.toml`
(`type = "gate"` defs), `base/pieces.toml` (example sealed-door marker),
`src/tests/worldgen.rs` (+ marker tests), `src/game/content.rs` (hot-reload
rule), `docs/extension-plans/wildforge-engine-extensions-spec.md` note.
**Related design doc:** `wildforge-engine-extensions-spec.md` Part 2.4/2.5
and `wildforge-task-brief-phase10-piece-based-structures.md` §10g.

## Status Going In

The Phase 10 marker seam is resolved-but-optionally-consumed, and Phase 12a
consumed the first marker kind:

- `AssemblyMarker { kind: String, at: BlockPos }` is produced by
  `place_assembly` for every piece marker (rotation applied) and returned to
  the chunkgen caller in `chunks.rs:930`.
- `chunks.rs:935` consumes `spawn:npc:<id>` markers — the NPC is placed at
  the resolved position (clamped to surface), unknown ids are skipped.
- The piece marker format is `MarkerToml { du, dy, dv, kind }`; pieces.toml
  ships one `spawn:npc:base:elder` marker as the first consumer.
- Quest/dialogue state lives in the per-mod KV (player-namespaced in
  `dialogue.rs`: `read_player_kv` / `write_player_kv`). `SetFlag` quest
  rewards already write arbitrary `(flag, value)` pairs there.
- Registry skips unknown feature `type`s today (`registry.rs:4454` filters
  `r#type != "ore"`), so adding a new type is a pure additive change.

Everything below is untouched (verified): no `feature:` marker consumption,
no `GateDef`, no gated-position registry, no break/interaction gate.

## Goal

Complete the 2.4/2.5 marker seam: NPC spawn markers (2.4) are already
consumed by Phase 12a — this phase **hardens and documents** that arm — and
adds **flag-gated features** (2.5): authored pieces may declare
`feature:<gate_id>` markers that place a sealed block which stays locked
(unbreakable, and interaction/breaking refused with a toast) until a
per-player KV flag crosses a threshold, then unlocks (block becomes
breakable, or is replaced by an `unlocked_block`).

1. **2.4** — spawn markers stay as-is; add regression tests + a retrogen
   guard assertion so a regenerated chunk never double-spawns a marker NPC.
2. **2.5** — `GateDef { id, block, flag, value, unlocked_block?, message? }`
   loaded from `features.toml` entries with `type = "gate"`. The marker
   consumer places the gate's `block` at the resolved position and records
   the position in a world gated registry. `break_block_at` and block
   interaction consult the player's KV flag; locked gates refuse
   (unbreakable) with the def's message toast, unlocked gates behave
   normally (or swap to `unlocked_block`).
3. **Base content** — one authored example: the elder's sealed study door,
   gated on the `elder_told_tales` flag the cerium quest's `SetFlag` reward
   already writes, opening (swap to air / passage) once the quest completes.

## In Scope

### 13a — `GateDef` format + registry load

- `FeatureToml` gains optional gate fields: `id: Option<String>`,
  `flag: Option<String>`, `value: Option<String>`,
  `unlocked_block: Option<String>`, `message: Option<String>`.
  A `type = "gate"` entry resolves to `GateDef`:
  - `id: String` — what `feature:<id>` markers reference (`mod:gate`).
  - `block: BlockId` — the sealed block placed at the marker.
  - `flag: String` — KV key read per player (e.g. `elder_told_tales`).
  - `value: String` — required value to unlock (default `"true"`).
  - `unlocked_block: Option<BlockId>` — if set, the gate is *replaced* by
    this block once unlocked (e.g. air to "open" a door; the sealed block
    itself is removed). If `None`, the sealed block just becomes breakable.
  - `message: Option<String>` — locked-interaction toast, default
    `"It's locked tight."`
  - `unbreakable_when_locked: bool` (default true) — whether the sealed
    block can still be mined while locked (allows "hard rock" gates that
    just check a flag on completion without being truly unbreakable).
- Registry: `reg.gates: Vec<GateDef>` + `gate_id(name) -> Option<usize>` +
  a `block -> gate` reverse map (`gate_for_block: HashMap<BlockId, usize>`)
  built at load so the runtime can find a gate by the placed block even
  without marker provenance. Reject a gate whose `block` is unknown with a
  load error naming the gate id.
- Validate at load: duplicate gate ids fail; a gate with neither
  `unlocked_block` nor breakability-after-unlock still loads (it degrades to
  "sealed marker" only).

### 13b — `feature:<id>` marker consumption + gated registry

- `AssemblyMarker` doc: extend the kind-convention comment to cover
  `feature:<gate_id>` ("places `gate.block`, sealed until the player's KV
  flag reads `value`; unknown id is silently skipped, the sealed block is
  placed regardless of whether a gate def resolves" — no: **skip the
  placement entirely** if the gate id is unknown, same as spawn markers, so
  a missing def cannot leave a permanent unbreakable wall).
- `chunks.rs:935`: in the existing marker loop, add a `feature:` arm. It
  resolves the gate def, places `gate.block` at `marker.at` (clamped to a
  walkable surface like the spawn arm when the offset is below grade — a
  door sits on the floor), and inserts the position into a new
  `World.gated: HashMap<BlockPos, usize>` (position → gate index).
- The sealed block is stamped through the ordinary `set_block` path so it
  lives in the chunk grid, is protected by the existing `structure_chunks`
  retrogen gate, and saves with the world. The gated registry itself is
  **re-derived on load**: iterate loaded marker provenance? No — instead
  persist `gated` positions with the world save (a small `Vec<(BlockPos,
  gate_id)>` alongside structure/marker state) so a save doesn't depend on
  regeneration to know what is sealed. Decision: persist `gated` as part of
  the world's marker/piece state block.
- Retrogen guard: `ensure_chunk`'s `structure_chunks` short-circuit already
  prevents marker re-consumption on regeneration; add an assertion/test that
  re-running chunkgen for a structure chunk does not re-place the gate nor
  spawn a second marker NPC.

### 13c — runtime gate (break + interaction)

- `World.break_block_at` (world/mod.rs:2498): before proceeding, if the
  target position is in `gated` and `unbreakable_when_locked`, refuse
  (`return None`) — the block cannot be broken regardless of tool or mode.
  The world-level check keeps scripts/commands from bypassing it.
- Player-facing behavior in `actions.rs`:
  - When a mining swing completes on a gated position, the above `None`
    already suppresses the break; surface the reason as a toast using the
    gate's `message` (the swing can additionally `clink` sfx on the sealed
    block).
  - Interact (right-click) on a gated position: if locked, toast the
    message and consume the interaction (no open/use of the block behind
    it). If unlocked and `unlocked_block` is set, replace the block and
    toast `"The gate opens."`; otherwise pass through to normal behavior.
  - Unlock check reads the player's KV via `read_player_kv` (the same
    namespace quest `SetFlag` rewards write) — no new flag plumbing.
- Gated positions are evaluated live from the persisted registry (flag is
  per-player, the position set is per-world) — unlocking is cheap and
  needs no global event.

### 13d — base content + docs

- `base/features.toml`: one gate def:
  ```
  [[feature]]
  type = "gate"
  id = "sealed_elder_door"
  block = "base:cracked_masonry"   # reuse existing sealed-looking block
  flag = "elder_told_tales"
  value = "true"
  unlocked_block = "base:air"
  message = "The elder's door is sealed shut."
  ```
  (Choose a block that reads as a door/wall; `cracked_masonry` exists and
  is already used in pieces. If a dedicated look is wanted, defer the new
  block + texture to a later pass — not required for the mechanic.)
- `base/pieces.toml`: the watch_platform piece (or a second small piece)
  gains a `feature:base:sealed_elder_door` marker near the elder's
  `spawn:npc:base:elder` marker, demonstrating the seam.
- `mods/README.md` + `AssemblyMarker` doc comment: document the
  `feature:<gate_id>` kind, the KV flag it reads, and the
  `unlocked_block`/`message` behavior.
- `registry.rs` load error text for gates mirrors the NPC/dialogue/quest
  loaders.

### 13e — tests

- Registry: gate def parses; unknown `block` fails load with the gate id in
  the message; duplicate id fails; a `type = "gate"` entry is *not* treated
  as ore.
- Marker consumption: a piece with a `feature:` marker places the sealed
  block at the resolved position; unknown gate id skips placement (no wall).
- Runtime gate: a locked gate position refuses `break_block_at` (returns
  `None`) regardless of tool; after writing the player KV flag, breaking
  succeeds (or the block is replaced by `unlocked_block`); `unlocked_block`
  swap happens exactly once.
- Retrogen: re-running chunkgen on a structure chunk does not re-place
  gates or double-spawn marker NPCs (guards the 2.4 arm too).
- Persistence: save/reload keeps the gated registry and the sealed block's
  locked/unlocked state.
- All gate tests follow the existing single-threaded `tests/*.rs` style.

## Out of Scope

- **3.4 settlement tiered growth** — its "hidden/non-collidable until a
  threshold" flavor is *not* this phase; here locked means "unbreakable +
  interaction refused", the visible-seal presentation is the sealed block
  sitting in the world. (The `gated` registry is the hook 3.4 will extend.)
- **Per-NPC relationship flags** and anything that gates on per-NPC state —
  gates read only the per-player KV flag they name.
- **Scriptable gate conditions** — a gate checks `read_player_kv(flag) ==
  value`; arbitrary `condition` hooks for gates are deferred (dialogue
  conditions already cover that authoring need).
- **New block types / textures** for a bespoke door — reuse existing blocks.
- **Doors as entities / animation** — unlock is an instant block swap or a
  bit of mining hardness, not a moving door.

## Verification

- `cargo check` / `cargo clippy --all-targets -- -D warnings` clean.
- `cargo test --lib` green including the new gate/marker tests; the five
  pre-existing `visual_capture` failures remain known-unchanged.
- Manual smoke: a worldgen piece with the sealed-door marker starts locked
  (toast + clink on swing/interact); completing the cerium quest (or setting
  the KV flag) opens it; save/reload preserves the state.

## Roadmap After This

- **3.4 settlement reputation & tiered growth** reuses `gated` (and the
  marker seam) with a "hidden / non-collidable until threshold" unlock
  behavior.
- **3.5 blueprint-gated recipes** plugs the same KV-flag read into recipe
  gating (a `quest_done`-style flag gates a recipe's `unlock`).
- **2.4/2.5 general markers** — if later content needs `spawn:mob:<id>` or
  `spawn:guard:<id>` arms, they land in the same marker loop.
