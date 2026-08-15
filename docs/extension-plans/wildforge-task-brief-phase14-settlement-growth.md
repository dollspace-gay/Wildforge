# Task Brief: Settlement Reputation & Tiered Growth (spec 3.4)

**Repo:** WildForge, new branch `feat/settlement-growth` (off
`feat/flag-gated-markers`)
**Primary files:** `src/registry.rs` (`SettlementDef` + load),
`src/world/pieces.rs` (tier-tagged piece placement + hidden recording),
`src/world/mod.rs` (`hidden` registry + accessors, `break_block_at` hidden
refusal, surface queries), `src/world/storage.rs` (hidden persistence +
load reconciliation), `src/world/chunks.rs` (assembly→settlement wiring),
`src/physics.rs` + `src/raycast.rs` + `src/mesher.rs` (hidden cells behave
as air), `src/game/actions.rs` (interact skip + reveal trigger),
`src/game/dialogue.rs` (`add_reputation` quest reward), `base/pieces.toml`
(authored settlement), `base/quests.toml` (rep reward),
`mods/README.md` + spec note.
**Related design doc:** `wildforge-engine-extensions-spec.md` Part 3.4.

## Status Going In

- Assemblies place pieces via a deterministic connector walk (`place_assembly`
  in `pieces.rs`, rolled per chunk in `chunks.rs:922`); every stamped cell goes
  through `set_block_at`, and structure chunks are reserved so retrogen never
  re-rolls them. Pieces already carry optional `markers` consumed by later
  phases.
- Flag-gated features (spec 2.5, Phase 13) established the exact pattern this
  phase generalizes: a per-world position→index registry (`World.gated`),
  persisted to a dedicated save file (`gated`, magic `WFG1`), consulted by
  `break_block_at`, physics, and the interact path, with the per-player KV
  flag read live from `dialogue.rs::read_player_kv`. Unlocking is a one-way
  world mutation (block swap + `ungate_at`).
- Quests (spec 3.3) already write arbitrary `(flag, value)` pairs via the
  `SetFlag` reward (`dialogue.rs:224`); adding reputation is a new reward
  variant on the same KV.
- Rendering is chunk-snapshot based: `mesher::ChunkMeshInput::capture(world,
  pos)` snapshots the center chunk and neighbor border columns; the mesh job
  runs off that snapshot, so any per-position presentation override must be
  applied *at capture time*, not in the mesh kernel.

## Goal

Implement spec 3.4: a **settlement** is an authored piece assembly whose
pieces are tagged with growth tiers. All tiers are placed at worldgen
(literal blocks in the chunk grid, retrogen-protected, saved with the world).
Tier-1 pieces are visible and solid immediately; tier-2+ pieces are placed
but **hidden** — non-collidable, not raycast-targetable, not rendered, not
breakable — until the player's **reputation** for that settlement crosses the
tier's threshold, at which point the world **reveals** the tier (positions
drop out of the hidden registry, the affected chunks re-mesh, and the
building becomes solid/visible/usable). No runtime procedural placement:
reveal only flips the visibility/collision of geometry that was already
stamped at worldgen.

The Phase 13 `gated` registry is the direct precedent and structural hook;
this phase adds a parallel **hidden** registry with a *different presentation*
(sealed-but-visible vs. present-but-inert), and adds reputation as a numeric
per-player KV value quests accumulate.

## In Scope

### 14a — `SettlementDef` + tier authoring format

- `pieces.toml` gains a `[[settlement]]` table:
  ```toml
  [[settlement]]
  id = "elder_haven"
  # KV key holding the numeric reputation (default `rep_<id>`).
  rep_key = "elder_haven_rep"
  tiers = [
    { tier = 2, threshold = 2 },   # revealed when rep >= 2
    { tier = 3, threshold = 5 },
  ]
  ```
- `[[assembly]]` gains optional `settlement = "id"` — the assembly generates
  that settlement (its entry + pools walk places every tier).
- `[[piece]]` gains optional `settlement_tier: u32` (default `1`). Tier 1 is
  the always-visible baseline; `settlement_tier >= 2` means the piece is a
  hidden growth tier. A piece with `settlement_tier > 1` is only meaningful
  under a settlement assembly; validate that.
- Registry: `SettlementDef { id, rep_key, tiers: Vec<SettlementTier{tier,
  threshold}> }`, `reg.settlements: Vec<SettlementDef>`,
  `settlement_id(name) -> Option<usize>`, and a `settlement_of_assembly`
  map for the chunkgen wiring. Load validation mirrors the gate/NPC
  loaders:
  - duplicate settlement ids fail;
  - `tiers` must be tier-ascending with increasing thresholds;
  - a piece with `settlement_tier > 1` referenced only from non-settlement
    assemblies (or with no settlement def for its assembly) fails with an
    error naming the piece;
  - an assembly `settlement = "id"` that names no `[[settlement]]` fails.

### 14b — worldgen placement + `World.hidden` registry

- `place_piece` already knows the stamped `(BlockPos, BlockId)` cells; when
  the assembly is a settlement and the piece has `settlement_tier > 1`, each
  stamped cell is *also* recorded in a new
  `World.hidden: HashMap<BlockPos, RevealKey>` where
  `RevealKey { settlement: usize, tier: u32 }`. The block is still placed via
  the ordinary `set_block_at` path (worldgen grid, retrogen guard, chunk
  save all work unchanged).
- Accessors: `is_hidden(pos)`, `hidden_positions()`, `hidden_count()` and a
  test-only `is_hidden_for_test`. `place_assembly` returns the settlement
  context alongside markers so `chunks.rs` can pass it down (or the assembly
  def carries it directly into `place_piece`).
- Retrogen: the existing `structure_chunks` reservation already guarantees a
  regenerated chunk never re-rolls the settlement, so hidden cells can never
  be re-placed or dropped; add an assertion/test that re-running chunkgen on
  a settlement chunk leaves the hidden registry unchanged.

### 14c — hidden cells behave as air everywhere

All presentation queries must treat a hidden position as if it were air.
Every site below is a single `&& !world.is_hidden(pos)` / blank-to-AIR check:

- **Physics** (`physics.rs::collides`): a hidden block never collides (walk
  straight through the un-built building).
- **Raycast** (`raycast.rs::cast_at`, plus the `#[cfg(test)]` `raycast`
  helper): hidden cells are skipped — the player cannot aim at, interact
  with, or mine them.
- **Rendering** (`mesher.rs::ChunkMeshInput::capture`): blank hidden cells to
  AIR in both the center snapshot and the border-column capture so neither
  the hidden block nor its faces render.
- **Breaking** (`world/mod.rs::break_block_at`): return `None` for hidden
  positions before any tool logic — a world-level backstop exactly like the
  Phase 13 gate seal, so scripts/commands/remote hosts cannot dismantle an
  un-revealed building.
- **Interact** (`game/actions.rs`): the interact path skips hidden targets
  (nothing to right-click yet).
- **Surface queries** (`surface_height_at`, `air_column_at`, `standable_at`):
  hidden cells are not ground — NPC spawn clamping and player placement never
  stand on an un-built floor.

### 14d — reputation + reveal trigger

- New quest reward `QuestReward::Reputation(String settlement, u32 amount)`,
  authored as `{ add_reputation = "elder_haven", rep_amount = 2 }`
  (`QuestRewardToml` gains `add_reputation` + `rep_amount`). On completion,
  `dialogue.rs` reads `rep_key` KV, adds `amount`, writes it back, and then
  triggers the reveal check for that settlement.
- `World::reveal_settlement(&mut self, settlement: usize, reputation: u32)`:
  walk the `hidden` registry, drop every `(pos, key)` whose key.settlement ==
  settlement and key.tier threshold `<= reputation`, mark affected chunks
  modified, and re-mesh them. Returns the revealed tier count for a toast
  (`"Elder Haven grows to tier 2."`). Called with the local player's rep.
- One-way and permanent: once a tier is revealed its cells leave the hidden
  registry; the blocks were already in the chunk grid so they simply become
  solid/visible. Mirrors the Phase 13 `ungate_at` semantics.
- Load reconciliation: on session start (the same place quest state is
  available), read `rep_key` for every settlement and reveal any tiers the
  player has already earned — so a save where reputation predates this
  feature still shows the settlement grown.

### 14e — persistence + hot reload

- Persist the `hidden` registry in a dedicated save file (e.g. `settlements`,
  magic `WFST1`, records of `BlockPos` + settlement index u16 + tier u8),
  exactly parallel to `gated` (`storage.rs`). Load restores it; revealed
  cells are simply absent (their blocks live in the chunk data).
- Hot-reload `remap_from`: re-resolve hidden records against the new
  registry; a record whose settlement def was removed or whose tier is no
  longer in `tiers` is **dropped** (treated as revealed) so a removed tier
  never leaves invisible permanent geometry.

### 14f — base content + docs

- Author one settlement in `base/pieces.toml` reusing existing blocks: an
  assembly `elder_haven` around a tier-1 plaza (visible at once) with one or
  two tier-2 pieces (e.g. `haven_hall`) and optionally a tier-3 piece. Use
  existing blocks (cobblestone, cracked_masonry, planks, wool, torches,
  chests) — no new textures.
- `base/quests.toml`: a small quest granting reputation
  (`add_reputation = "elder_haven", rep_amount = 2`), chained after the
  existing `elder_cerium` quest so the settlement visibly grows as the player
  helps the elder's people.
- `mods/README.md`: a settlement-features section (tier tags, rep rewards,
  hidden-until-threshold behavior). Note the precedent in the spec doc.

### 14g — tests

- Registry: settlement def parses; tier thresholds validated (ascending);
  duplicate id fails; tier>1 piece under a non-settlement assembly fails;
  assembly→settlement wiring resolves.
- Worldgen: a settlement assembly places tier-1 visible + tier-2 cells
  recorded as hidden; hidden cells are non-colliding (physics), skipped by
  raycast, blanked to AIR in the mesh capture, and refused by
  `break_block_at`.
- Reveal: writing a rep value crosses a threshold → the tier's positions drop
  out of `hidden`, the cells become collidable/breakable, and lower tiers
  stay hidden; thresholds not yet met stay hidden.
- Persistence: save/reload keeps the `hidden` set; hot-reload drop of a
  removed settlement reveals (drops) its records.
- Quest: `add_reputation` reward increments the KV and triggers a reveal.
- All tests follow the existing single-threaded `tests/*.rs` style.

## Out of Scope

- **Per-player visibility.** The settlement is world geometry; reveal is a
  one-way, persisted world mutation triggered by the *local* player's
  reputation (single-player semantics, same as the gate unlock and the quest
  journal). Two players with different rep see the same, already-revealed
  world.
- **Reputation decay / sources beyond quest rewards.** Reputation only
  increases via `add_reputation` quest rewards in this phase; decay, passive
  gain, and faction systems are future content.
- **Transition animation.** Reveal is an instant visibility/collision flip +
  re-mesh, not a scaffolding/fade sequence.
- **New blocks or textures.** Reuse the existing palette.
- **3.5 blueprint-gated recipes** and **3.6/3.7** — later phases.

## Verification

- `cargo check` / `cargo clippy --all-targets -- -D warnings` clean.
- `cargo test --lib` green including the new settlement tests; the five
  pre-existing `visual_capture` failures remain known-unchanged.
- Manual smoke: find a settlement in a fresh world; the tier-2 hall is
  invisible and walk-through; completing the rep quest reveals it (toast +
  the building appears and collides); save/reload keeps the revealed state;
  a pre-reveal save with rep already earned reconciles on load.

## Roadmap After This

- **3.5 blueprint-gated recipes** plugs the same numeric KV read into recipe
  gating.
- **3.6 / 3.7** remain as authored later.
- Reputation as a first-class currency (more reward types, decay, NPC
  relationship integration) layers onto `rep_key` once quests grow.
