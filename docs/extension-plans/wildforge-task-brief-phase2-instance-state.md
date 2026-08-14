# Task Brief: Multiblock Instance State, Stat Aggregation & Event-Driven Revalidation

**Repo:** WildForge · **Primary files:** `src/world/machines.rs`,
`src/world/multiblock.rs` (added in Phase 1)
**Related design doc:** `wildforge-engine-extensions-spec.md` — this task
covers the three items Phase 1 explicitly deferred: the `BlockEntity`
generalization (Part 0), Pattern A stat aggregation (Part 2.1), and
event-driven recheck (Part 1.2's scaling note). Slots (Part 1.3), the
capture tool (Part 1.4), and everything in Parts 2.2–3 remain out of
scope.

## Status Going In

Phase 1 shipped: a generic `MultiblockShape`/`ShapeCell`/`match_shape`
system in `multiblock.rs`, with `check_bloomery`, `check_forge`,
`check_glassworks`, `check_separator`, `check_kiln`, and `check_stall`
reimplemented as shape definitions against it. `MatchResult` already
carries a `matched: HashMap<(i32,i32,i32), BlockId>` — collected but
unused. `BlockEntity` is still the pre-existing hand-written enum
(`Forge`, `Kiln`, `Bloomery`, `Anvil`, `Clamp`). `tick_kilns` still polls
`check_kiln` every tick per lit kiln. No behavior changed from
pre-Phase-1 in any of this.

## Goal

Three related changes, landed together because they touch the same
representation:

1. Replace the per-kind growth of `BlockEntity` with one generic
   multiblock-instance representation carrying a shape ID and arbitrary
   per-instance state (fuel/charge/output as today, plus room for slot
   state later — though slot state itself is NOT implemented in this
   task).
2. Use `MatchResult.matched` (already returned by `match_shape` since
   Phase 1) to fold constituent-block properties into a machine
   instance's effective stats — the actual Pattern A behavior the data
   collection was built for but never consumed.
3. Replace `tick_kilns`' per-tick `check_kiln` polling with invalidation
   triggered by `block_place`/`block_break` events near a registered
   controller position.

## In Scope

### 2a — Generic instance state
- Introduce a block-property registry (or extend whatever block
  metadata already exists) so a block ID can carry stat contributions —
  e.g. a `firebrick_advanced` block contributing a higher heat-retention
  value than base `firebrick`, without needing a distinct mouth-block
  special-case the way machine *identity* currently works.
- Replace the `BlockEntity` enum's five hand-written variants with one
  generic variant: shape ID (which `MultiblockShape` this instance
  matches) + a small state struct (fuel/charge/output — same fields as
  today, just no longer duplicated per enum arm) + reserved space or a
  placeholder field for future slot state (do not design the slot
  schema itself here — that's Part 1.3's job; just don't paint this
  representation into a corner that makes adding it later a second
  rewrite).
- Migrate existing forge/kiln/bloomery/anvil/clamp instances to the new
  representation. Existing save data compatibility: decide and document
  whether this requires a save-migration step or whether it's acceptable
  to treat as a breaking change pre-1.0-equivalent — flag this decision
  explicitly in the PR description rather than deciding silently.

### 2b — Stat aggregation (Pattern A)
- Add a fold step: given a `MatchResult`, walk `matched` and combine
  each cell's block's stat contribution into an effective stat bundle
  for that instance (sum, max, or per-stat-defined combination rule —
  implementer's call, but the combination rule per stat should be data,
  not hardcoded per machine).
- Wire at least one real stat through end-to-end as proof: heat
  retention or smelt-speed multiplier from firebrick tier is the natural
  candidate given the existing ring-of-firebrick shapes. Store the
  folded result on the instance (recompute on revalidation, not every
  tick — see 2c) rather than recomputing per-use.
- This task does NOT need to introduce new block variants (e.g. an
  actual "advanced firebrick") as game content — a single test/dev block
  with a different stat value is sufficient to prove the pipeline works.
  Content additions are a separate, later task.

### 2c — Event-driven revalidation
- Hook `block_place`/`block_break` (or whatever the current edit-event
  entry points are named) to check whether the edited position is
  within the bounding box of any registered multiblock instance's shape,
  and if so, re-run `match_shape` for that instance only.
- On failure, transition the instance to an invalid/unlit state (same
  end state `tick_kilns` currently reaches by polling) and re-fold stats
  (2b) on success in case the swap changed them.
- Remove the per-tick `check_kiln` call from `tick_kilns` once the
  event-driven path covers the same cases. Keep a slow-interval safety
  poll (e.g. once every N seconds, not every tick) only if you're not
  fully confident every edit path is covered by the hook — call this out
  explicitly rather than silently leaving both in place.

## Acceptance Criteria

- All Phase 1 test cases still pass unchanged (no behavior regression
  for the six existing shapes).
- Breaking a block within a valid multiblock invalidates it immediately
  via the edit hook, with no dependency on that machine being lit/ticked
  for the invalidation to be noticed.
- Swapping a matched block for a different-but-still-valid block (e.g.
  firebrick → advanced firebrick in a ring position, assuming both
  satisfy the shape's constraint) changes the instance's folded stat
  output without requiring the player to break and rebuild the
  structure.
- `BlockEntity`'s match arms for machine-specific logic (recipe
  processing, fuel consumption, etc.) are updated to read from the new
  generic representation but keep their existing observable behavior —
  a forge still forges the same way, just backed by the new struct.
- New unit tests: one confirming stat folding produces the expected
  combined value for a synthetic multi-block instance; one confirming
  an edit adjacent to (but not part of) a registered instance does *not*
  trigger unnecessary revalidation (bounding-box check is actually
  scoped, not global).

## Non-Goals (do not implement in this task)

- **Slot-based modular frames** (spec Part 1.3) — no slot UI, no module
  swap-in-place interaction. This task only makes sure the underlying
  instance representation won't need a second rewrite when slots land.
- **Capture/stamp tool** (spec Part 1.4).
- **Bounded local-structure entities** (spec Part 1.1) — trains and any
  moving block-containers are untouched.
- **New game content** — no new block types, machine types, or recipes
  beyond whatever minimal test block is needed to prove stat folding
  works.
- **`vice_near` conversion** — still out of scope, as in Phase 1.
- Anything from spec Parts 2.2–3: rail, piece-based structures, NPCs,
  dialogue, quests, settlements, enemy archetypes.

## Suggested PR Description Framing

> Follow-up to the multiblock recognition refactor. Generalizes
> `BlockEntity`'s per-kind enum into one shape-ID-plus-state
> representation, wires the shape matcher's already-collected matched-
> block data into actual stat aggregation (Pattern A), and replaces
> `tick_kilns`' per-tick polling with revalidation triggered by nearby
> block edits. No new game content; forges/kilns/etc. behave identically
> to before except that invalidation is now immediate rather than
> tick-delayed, and block-tier substitution now measurably changes
> output. Groundwork for slot-based modular machines (next PR) and
> block-built trains, per [design doc link].

## Roadmap After This

For context on why this scope, not to be treated as committed work yet:

- **Phase 3 — Slot-based modular frames** (spec 1.3): named slots on a
  frame, module swap-in-place, per-slot UI. Depends directly on this
  task's generic instance representation.
- **Phase 4 — Capture/stamp tool** (spec 1.4): independent of slots;
  could run in parallel with Phase 3 if capacity allows.
- **Phase 5 — Bounded local-structure entities** (spec 1.1): needed
  before block-built trains; independent of Phases 3–4 but benefits
  from Phase 4's stamp tool for saving car designs.
- **Content-layer track** (spec Part 3 — NPCs, dialogue, quests,
  settlements, enemy archetypes, ire generalization): independent of
  all of the above, can proceed on its own timeline.
