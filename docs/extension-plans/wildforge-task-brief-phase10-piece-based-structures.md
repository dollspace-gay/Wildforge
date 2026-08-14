# Task Brief: Piece-Based Procedural Structures

**Repo:** WildForge, branch `feat/multiblock-recognition`
**Primary files:** likely a new `src/world/pieces.rs` (or extend
`src/world/chunks.rs`); touches `src/world/template.rs` (reuse only —
the piece cell format *is* `TemplateCell`), `src/world/multiblock.rs`
(`Rotation`, reuse only), `src/registry.rs` (new piece/pool/assembly
defs + parsing), `base/pieces.toml` (new file; `structures.toml` stays
untouched).
**Related design doc:** `wildforge-engine-extensions-spec.md` — this
task implements Part 2.3 ("Piece-Based Procedural Structures").

## Status Going In

Phases 1–9 are in and working on this branch:

- Generic shape matching + `Rotation` (`multiblock.rs`), used by all
  machine kinds and by template stamping.
- Capture & stamp tooling (Phase 4): `Template` = `name` + `cells`
  where each cell is `TemplateCell { du, dy, dv, block }` — a
  coordinate-space-agnostic offset + block-name list, persisted to
  `templates.toml`, rotated at use time via `Rotation::apply`.
- Bounded local-structure entities (Phase 5) built *from* `Template`
  (same cells resolved against a `LocalTransform` instead of a fixed
  anchor).
- `structures.toml` worldgen model: single-fixed-template ruins
  (`StructureDef`), one per chunk at most, stamped via
  `place_structure_at` in `chunks.rs`. Every structure marks its origin
  chunk in `structure_chunks`, which gates retrogen and regeneration.
- Mass-driven power draw (Phase 9) closed out spec Part 2.2's first
  real slice.

**This phase builds the assembly layer spec §2.3 describes:** a
generation walk over *pieces* (typed-connector templates) chosen from
weighted *pools*, assembled from an entry piece outward with terrain
adaptation. This is the replacement for the single-fixed-template
`structures.toml` model "for anything beyond simple ruins" — the
existing ruins stay, and the piece system slots in beside them.

## Goal

Given an authored entry piece and a set of connector-typed pools, walk
outward from the entry, connecting interchangeable pieces at matching
typed connector points, respecting a max depth / max piece count, and
adapt the whole assembly to the voxel terrain. The assembly is stamped
into the normal chunk grid (like today's ruins — pieces are *ordinary
world blocks*, not `LocalStructure` entities; players break/place/
loot them normally), deterministic from a seed, and safe against
regeneration and retrogen.

## In Scope

### 10a — Piece model & authoring format

A piece is a template plus connector points. Concretely:

- `PieceDef { name, cells: Vec<TemplateCell>, connectors: Vec<Connector> }`
  where `Connector { du, dy, dv, kind: String, facing: ConnectorFacing }`.
- `cells` reuse the exact `TemplateCell` serde shape from Phase 4, so a
  piece *is* loadable as a `Template` and vice-versa. **Authoring
  format is the capture tool's own format** (offset + block name) — a
  captured region is already a valid piece body; authoring a piece is
  "capture it, then tag its connectors."
- `kind` is an author-chosen string tag (e.g. `"door"`, `"corridor"`,
  `"window"`); connectors only pair with connectors of the *same* kind.
  `facing` is the world direction the connector points *out of* the
  piece (one of the four cardinal directions via `Direction4`), used to
  orient the child piece.
- Connector cells conventionally sit at the piece's outer edge but the
  engine does not require it — it aligns the *connector points*, and the
  child's `facing` must oppose the parent's when rotated into place.

**Decision (resolved): pieces live in a new `base/pieces.toml` /
mod-side `pieces.toml`**, *not* in `structures.toml`. Rationale: the
spec's §2.3 framing is that piece assemblies *replace* the
single-fixed-template `structures.toml` model for anything beyond
simple ruins — the two models are siblings, and keeping them in
separate files lets a mod ship assemblies without editing the base
ruins file. Loading adds a `pieces.toml` read to `parse_mod_dir`
mirroring the existing `structures.toml` read, plus a
`BASE_PIECES = include_str!("../base/pieces.toml")` const and a
`PiecesFile { piece, pool, assembly }` deserialize shim — one new file
in the fixed list, nothing else in the loader changes.

### 10b — Pools & entry selection

- `PoolDef { id, entries: Vec<PoolEntry { piece: String, weight: u32 }> }`
  — a weighted list of interchangeable pieces. Pools are keyed by
  connector *kind*: when the walk needs to satisfy a `"door"` connector,
  it rolls from the `"door"` pool.
- `AssemblyDef` (per biome/rarity, mirroring `StructureDef`'s
  `biomes`/`rarity` gate) names: the entry `piece`, the pools by kind,
  a `max_depth`, a `max_pieces`, and a `terrain` adaptation mode
  (see 10d).
- Entry selection feeds the existing deterministic per-chunk roll
  already used by `seed_structures` (`mob_hash_at` / `is_multiple_of`)
  — assemblies and single-template ruins share the biome-gated,
  at-most-one-per-chunk placement opportunity. Decide and document how
  a chunk that rolls "structure" resolves between the two models
  (recommended: each `AssemblyDef` rolls like a structure with its own
  rarity; a chunk can host a ruin or an assembly, first roll wins).

### 10c — Generation walk

BFS/DFS over placed pieces, seeded deterministically:

- Place the entry piece at the chosen origin (anchor Y from terrain
  adaptation, see 10d), enqueue all its connectors.
- Repeatedly pop a connector. Roll a piece from that connector `kind`'s
  pool (weighted, deterministic RNG — reuse the existing `roll_loot`-
  style integer LCG pattern, see `chunks.rs::roll_loot`). Orient the
  candidate piece so its chosen connector opposes the parent's `facing`
  (compute the `Rotation` that maps the child's connector facing onto
  the parent's; `Rotation::CARDINAL` + `Rotation::apply` already cover
  the four turns). Place its cells at world positions derived from the
  parent connector's world cell + facing.
- Budgets: never exceed `max_pieces` placed, never place deeper than
  `max_depth` (steps from entry). A connector whose pool is empty or
  whose roll fails terrain checks is left unconnected (dead end) — that
  is a valid and expected outcome.
- **Collision:** a candidate piece is rejected if any of its non-air
  cells would overwrite an already-placed assembly cell, a
  `structure_chunks` cell, or the origin of another structure. The walk
  tracks its own placed `BlockPos` set; overwriting pre-existing
  terrain *outside* the assembly is allowed (that's what ruins already
  do — `'~'`/cell carving is the stamp's normal behavior).
- Determinism: all rolls derive from the same seeded hash as
  `seed_structures`, so re-generating a chunk reproduces the identical
  assembly. Pieces stamp once at first generation (the `fresh` gate in
  `adopt_chunk`) and the resulting chunk saves, exactly like ruins.

### 10d — Terrain adaptation

Three modes, per assembly:

- **`none`** — place the entry at the surface anchor as authored; the
  piece's own floor cells are simply stamped. Best for small perches.
- **`bury`** — sink the entry piece so its *floor* meets the computed
  surface at the origin (like today's `buried` ruins), optionally by a
  fixed depth. Connector-relative child placement stays relative to the
  sunk parent, so the whole assembly sits underground together.
- **`encapsulate`** — place the entry inside solid terrain (into a
  hillside or below surface) and carve only the assembly's air cells;
  terrain stays as the outer shell. This is the dungeon mode.

Anchor Y is derived per assembly from `surface_height_at` sampled at
the entry origin (the same call `seed_structures` already uses), then
each subsequent piece computes its Y from the parent connector's world
Y + the child's connector `dy` — the assembly follows its own floor
continuity rather than re-sampling terrain per piece.

### 10e — Multi-chunk stamping & safety

Current ruins are ≤14×14 within one chunk and stamp only the origin
chunk. Piece assemblies **will span chunks** — the plan must handle:

- `set_block_state_at` silently no-ops when the target chunk isn't
  loaded (`chunks.get_mut` returns `None`). Before stamping any cell,
  the walk must `ensure_chunk` every touched chunk. Verify `ensure_chunk`
  is safe to call *during* a chunk's own `adopt_chunk` (it calls
  `adopt_chunk` → `seed_structures` recursively today; confirm no
  re-entrancy hazard for the *origin* chunk being mid-adoption).
- **Mark every touched chunk in `structure_chunks`** — not just the
  origin — so retrogen (`apply_loaded_material_retrogen`) and any
  regeneration guard leave the whole assembly alone. This is the
  existing precedent; extend its use from "origin chunk" to "every
  chunk the assembly wrote to."
- **Decision (resolved): the origin chunk's roll wins; the neighbor's
  seeded roll is suppressed via `structure_chunks` reservation.**
  Add an early-out at the top of `seed_structures`: if
  `self.structure_chunks.contains(&pos)`, return — a chunk already
  claimed by a structure never rolls its own. Then have the walk
  insert *every* chunk it will touch into `structure_chunks` **before**
  calling `ensure_chunk` on it. Sequencing this ordering means: during
  `adopt_chunk(origin)` → `seed_structures(origin)`, the entry piece is
  placed first (origin lands in `structure_chunks` via the existing
  `place_structure_at` insert); when the walk needs a neighbor chunk it
  reserves it, then `ensure_chunk` → `adopt_chunk(neighbor)` →
  `seed_structures(neighbor)` early-returns because the neighbor is
  already reserved. This uses the exact same `structure_chunks`
  semantics (worldgen wrote here, don't touch) that retrogen already
  keys on, and it must be **documented on `seed_structures`**: the
  guard is what makes reservation-then-ensure safe, and it also
  protects the origin chunk from a later re-generation of the region
  trying to seed a second structure into claimed territory.

### 10f — Loot, chests, and heritage

Reuse the existing ruin machinery instead of writing a second path:

- Chest cells (`'C'` in today's model) become a per-cell "chest with
  loot table" flag on `PieceCell` (or the piece declares chests by a
  marker connector). Roll loot with the existing `roll_loot`, build
  `ChestState { wild_owned: true, .. }`, bind arcane/discovery stacks
  exactly like `place_structure_at` does today.
- Record placed materials + stacks through `material_ledger`
  (`record_external_world_content`), same heritage label ("pre-genesis
  ruin inheritance" or a piece-specific label — pick one and use it).

### 10g — Spawn & feature markers (format + resolve, don't consume)

Pieces may declare typed marker points (e.g. `spawn:guard`,
`feature:locked_door`) as connector-like offsets. The walk resolves
them to world positions (applying the same rotation/placement math as
connectors) and records them on the assembly's output. **No NPC or
quest logic this phase** — this is the forward-looking seam for
spec Parts 2.4/2.5 to consume. Keep it to: a marker list per piece
name → world-pos resolution → a returned/recorded list. Decide and
document where resolved markers are stored (recommended: a
`Vec<(String, BlockPos)>` returned with the assembly result; later
phases persist/consume them).

## Out of Scope

- **Any NPC behavior, dialogue, or quest logic** — the markers are
  carried but nothing consumes them yet.
- **`LocalStructure`-as-piece-container.** Pieces stamp into the
  ordinary chunk grid as normal blocks. `LocalStructure` remains the
  moving/bounded primitive (trains, in-structure working machines); it
  is deliberately *not* the vehicle for worldgen assemblies.
- **Player-authored pieces from the capture tool end-to-end.** The
  *format* is the capture format, but the capture UI→piece registration
  loop (capturing in-world and registering the result as a live piece)
  is deferred; authoring here means shipping `pieces.toml` cell lists.
- **Rotations beyond the four cardinal turns** (no 180 pitch/roll
  flips). Connector alignment only needs `Rotation::CARDINAL`.
- **Vertical (up/down) connectors** — all connectors are cardinal
  horizontal; multi-floor assemblies are achieved by authoring a piece
  that *contains* the stairs as its cells, not by a distinct connector
  axis.
- **Collision with other structures beyond the `structure_chunks`
  guard** (e.g. avoiding another nearby assembly's origin, or measuring
  distance between assemblies) — see Open Questions.

## Verification

- **Determinism:** re-running the walk with the same seed (via the
  existing seeded-hash path, or a direct unit test of the walk)
  produces byte-identical block layouts.
- **Budgets:** a test assembly with `max_pieces` / `max_depth` bounds
  never exceeds them, and dead-ends leave connectors unconnected
  without panicking or corrupting the world.
- **Connector correctness:** for each placed connector pair, the child's
  connector cell lands exactly adjacent to the parent's, on the correct
  world cell, with opposing facings.
- **Multi-chunk:** an assembly that crosses a chunk border places all
  cells (none silently dropped), marks *all* touched chunks in
  `structure_chunks`, and survives a save→load→regenerate cycle
  unchanged.
- **Terrain modes:** `none` sits on the surface; `bury` is fully
  underground with its floor at the target depth; `encapsulate` keeps
  solid terrain around its shell.
- **Loot/heritage:** chest pieces get rolled, wild-owned, arcane-/
  discovery-bound stacks; ledger heritage records the same content as
  today's ruins.
- Existing test suite stays green (run the same test command the last
  phase used; see the phase 9 brief / repo docs for the invocation).

## Suggested PR Description Framing

> Adds piece-based procedural structures (spec Part 2.3): a `PieceDef`
> model reusing Phase 4's `TemplateCell` format with typed connector
> points, weighted per-kind pools, a deterministic generation walk
> (entry piece outward, `max_pieces`/`max_depth` budgets, dead-end
> tolerance), and three terrain-adaptation modes (none/bury/
> encapsulate). Assemblies stamp into the ordinary chunk grid with
> multi-chunk support — every touched chunk is `ensure_chunk`ed and
> marked in `structure_chunks` for retrogen/regeneration safety — and
> reuse the existing loot/chest/arcane/ledger machinery rather than a
> second stamp path. Spawn/feature markers are resolved to world
> positions but not consumed (seam for spec 2.4/2.5). Existing
> single-template ruins in `structures.toml` are unchanged.

## Roadmap After This

- **Friendly NPC primitive (spec 3.1)** consumes the resolved spawn
  markers; **quest-flag-gated features (2.5)** consume feature markers.
- **Settlement tiered growth (3.4)** can place its tiers as piece
  assemblies at worldgen, matching this phase's pre-placed approach.
- **Player-authored piece registration** (capture in-world → live
  piece) once the authoring loop UI is worth building.
