# Task Brief: Block-Built Interiors (Foundation)

**Repo:** WildForge, branch `feat/multiblock-recognition`
**Primary files:** `src/world/local_structure.rs`,
`src/world/multiblock.rs`, `src/world/machines.rs`, `src/world/mod.rs`.
**⚠ This brief is grounded in an earlier view of `mod.rs`/
`machine_tick.rs`** (from before Phases 4–6 landed). Re-verify the
specifics below against current code before starting — the shape of the
problem is very likely still accurate, but exact function/field names
may have drifted.

**Related design doc:** `wildforge-engine-extensions-spec.md`, Phase
5's explicitly-deferred tick-coupling question, restated here as the
thing this task actually resolves. This is a **foundation** task:
it makes block behavior *possible* inside a `LocalStructure` and proves
it once, end to end, with one example machine. It does not deliver
player-facing building-inside-a-car as a finished feature — see
Non-Goals.

## Why This Is the Big One

Every prior phase reused an existing primitive: Phase 3 generalized the
mouth-slot pattern, Phase 4 built on `Template`, Phase 5 built on
`Template` again, Phase 6 reused `Rotation`/`world_position`. This phase
doesn't have that luxury — it's the one place the codebase's existing
assumption (block behavior lives in `World`, addressed by `BlockPos`,
full stop) genuinely doesn't fit the new requirement (block behavior
that can *also* live inside a small, separate, non-chunk-grid store).
Budget accordingly; this is architecture work, not a data-shape reuse.

## Status Going In (as last reviewed — confirm before starting)

- `LocalStructure` (Phase 5) stores **only** `blocks: HashMap<(i32,i32,i32),
  BlockId>`. It has **no `block_entities` field at all.** A forge's full
  shape could be built inside a structure's block space today and it
  would remain permanently inert raw blocks — there is currently no
  mechanism by which it could become a `MachineInstance`.
- `match_shape` (`multiblock.rs`) is signed against `world: &World,
  anchor: BlockPos` specifically — not generic over any storage
  abstraction.
- `World`'s per-machine tick functions (`tick_bloomeries`, `tick_forges`,
  `tick_kilns`, etc., in what was `machine_tick.rs`) are methods on
  `World`, reading/writing `self.block_entities`, `self.get_block_at`,
  and (for furnaces specifically) `self.ire` directly — tightly coupled
  to `World` being the one and only block store in the game.
- `Template` (Phase 4) deliberately captures **raw block layout only —
  no `BlockEntity` contents**, as an explicit anti-duplication scope
  decision. This means a `LocalStructure` spawned from a template that
  happens to include a full forge shape starts with zero `MachineInstance`
  state even if the blocks are all present — instantiation has to happen
  some *other* way, the same way it presumably happens in the main world
  today (first successful match after placement, or an explicit
  player-triggered "light" action — confirm which before assuming).

## Goal

Generalize enough of the storage/recognition/tick pipeline that a
`LocalStructure` can host a real, functioning machine instance —
recognized via the same shape system, ticking via the same per-kind
logic — entirely self-contained within the structure, with **zero**
behavior change for existing `World`-based machines.

## In Scope

### 7b-1 — Give `LocalStructure` its own block-entity storage
```rust
pub struct LocalStructure {
    // ...existing fields...
    pub block_entities: HashMap<(i32, i32, i32), BlockEntity>,
}
```
Mirrors `World`'s `block_entities` map, keyed by local offset instead of
`BlockPos`. Extend `local_structures.toml`'s save/load (`SavedStructure`)
to persist this too — Phase 5's format currently only round-trips
`blocks`; a machine's `charge`/`fuel`/`progress` state would silently
vanish on reload without this.

### 7b-2 — A minimal storage abstraction
Introduce the smallest trait that lets `match_shape` and the tick
functions operate against either `World` or `LocalStructure` without
duplicating their logic:
```rust
pub trait BlockStore {
    type Pos: Copy;
    fn get_block(&self, pos: Self::Pos) -> BlockId;
    fn offset(&self, pos: Self::Pos, d: (i32, i32, i32)) -> Option<Self::Pos>;
    fn block_entities(&self) -> &HashMap<Self::Pos, BlockEntity>;
    fn block_entities_mut(&mut self) -> &mut HashMap<Self::Pos, BlockEntity>;
}
```
(`Self::Pos = BlockPos` for `World`, using its existing `offset`
semantics; `Self::Pos = (i32,i32,i32)` for `LocalStructure`, where
`offset` is plain tuple addition with no failure case, unlike
`BlockPos::offset`'s `Option` return for out-of-world positions.)
Exact shape is the implementer's call — the goal is "one trait, two
implementors, zero duplicated matching/ticking logic," not this precise
signature.

### 7b-3 — Generalize `match_shape`
Make it generic over `BlockStore` rather than hardcoded to `&World`/
`BlockPos`. **Non-negotiable acceptance bar: every existing Phase 1–3
test for `World`-based machines passes unchanged after this
generalization** — this is a refactor of working code, same discipline
Phase 1 held itself to when it replaced the original bespoke
`check_stack`/`check_stall` functions.

### 7b-4 — Generalize (or cleanly parallel) machine ticking
Whatever the current `tick_kilns`-style dispatch loop looks like, extend
it (or add a sibling) so a `LocalStructure`'s own matched machine
instances get ticked each world tick, using the *same* per-kind tick
logic (fuel consumption, progress, recipe completion) as `World`-hosted
machines — not a forked copy. If `self.ire` (or anything else
`World`-specific) is read by existing tick logic, decide explicitly
whether a `LocalStructure`'s machines participate in the main world's
`ire` at all, or are exempt — state the decision, don't leave it
implicit.

### 7b-5 — Prove it with one example
Build (via a test/debug helper — this does not need a player-facing
placement UI, matching prior phases' "minimal proof, not full content"
posture) a working shell of one existing machine kind — Forge is the
best candidate, being the smallest/simplest already-implemented shape —
inside a spawned `LocalStructure`'s block space. Confirm: it validates
via the generalized `match_shape`; lighting it and ticking it produces
the same fuel/progress/output behavior a world-hosted forge would;
breaking a shell block invalidates it via the same event-driven
revalidation model (Phase 2) — scoped to the structure's own edits, not
triggering on unrelated main-world edits.

## Acceptance Criteria

- All existing `World`-based multiblock/machine tests (Phases 1–3) pass
  unchanged.
- A forge shell built inside a `LocalStructure`'s block space
  successfully validates via the generalized matcher.
- Lighting and ticking that in-structure forge produces observably
  correct behavior (fuel consumption, recipe progress, completion) —
  same outcome a world-hosted forge would produce for the same inputs.
- Breaking a shell block inside the structure invalidates the in-
  structure instance without touching or revalidating anything in the
  main world.
- `LocalStructure`'s `block_entities` round-trips through
  `local_structures.toml` (save, reload, confirm charge/fuel/progress
  survives).
- New unit tests: the generalized-matcher regression suite (7b-3's bar,
  explicit test coverage, not just "it compiles"); an in-structure
  forge lifecycle test (build → validate → light → tick → complete);
  a persistence round-trip test for structure-hosted machine state.

## Non-Goals (do not implement in this task)

- **Wiring an in-structure machine to rail motion/speed/power** — e.g.
  an "engine module" that actually drives a train's `speed`. That's the
  still-pending mass-driven power-draw phase; this task proves machines
  can *exist and function* inside a structure, not that they affect
  movement.
- **Player-facing building/interaction inside a placed structure** — how
  a player actually places/breaks blocks inside a parked or moving
  `LocalStructure` (aiming, targeting, whatever the interaction model
  needs to become) is separate, substantial UX/interaction work, not
  covered by proving the underlying data model works.
- **Slot-module swapping inside a structure** — should fall out "for
  free" once 7b-2/7b-3 generalize correctly (slots are just shape cells,
  same as everywhere else), but this task doesn't specifically build or
  test a full swap-in-place flow for the in-structure case — one
  additional test if it's cheap, not a requirement.
- **Multiple simultaneous distinct machines in one structure** — the
  one-forge proof is sufficient; nothing here should *prevent* more, but
  it's not a tested requirement.
- **Any observable behavior change for existing `World`-hosted
  machines** — this is a hard regression bar, restated because it's the
  single most important property of this task succeeding cleanly.
- **Piece-based procedural structures** (spec Part 2.3) — unrelated,
  still pending.

## Open Questions to Resolve Before/During Implementation

- **Re-confirm current `mod.rs`/`machine_tick.rs` shape** — this brief
  is grounded in an earlier view; verify function names, whether
  `tick_kilns`-style dispatch has changed shape since Phase 2's
  event-driven revalidation work, and whether anything else has shifted
  in the intervening phases.
- **How does a `MachineInstance` get created in the main world today** —
  automatically on first successful shape match after a block-place
  event, or via an explicit player action (e.g. "light")? Whichever it
  is, the in-structure path (7b-5) should mirror it, not invent a
  different trigger.
- **Does `self.ire` (or any other genuinely `World`-only concept) need a
  local-structure equivalent, or should structure-hosted machines simply
  not interact with it?** Pick one, document it.
- **Where does `World`'s tick loop actually schedule `tick_kilns`
  et al.** — same open question Phase 6 already flagged; resolving it
  once benefits both phases.

## Suggested PR Description Framing

> Generalizes multiblock recognition and machine ticking behind a
> `BlockStore` trait so both `World` and `LocalStructure` can host
> functioning machine instances, closing the tick-coupling gap Phase 5
> flagged and deferred. Adds `LocalStructure::block_entities`
> (previously absent — structures could hold blocks but not behavior)
> with save/load support. Proves the pipeline with one in-structure
> forge: builds, validates, lights, ticks, and persists correctly, with
> zero observable change to existing world-hosted machines (full
> Phase 1–3 regression suite passes unchanged). Does not yet wire
> in-structure machines to movement/power or add player-facing
> in-structure building UX — foundation only. Per [design doc link].

## Roadmap After This

- **Player-facing in-structure building** — the interaction/UX layer
  this task deliberately doesn't touch.
- **Mass-driven power draw** — can finally wire an in-structure engine
  module to actual rail speed, now that in-structure machines function.
- **Piece-based procedural structures** (spec Part 2.3) — independent,
  still pending.
