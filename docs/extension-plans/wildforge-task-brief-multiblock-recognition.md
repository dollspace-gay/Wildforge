# Task Brief: Data-Driven Multiblock Recognition

**Repo:** WildForge · **Primary file:** `src/world/machines.rs`
**Related design doc:** `wildforge-engine-extensions-spec.md` — this task
implements Part 1.2 ("Multiblock Recognition") only. Everything else in
that spec is explicitly out of scope here (see Non-Goals).

## Goal

Replace the current per-shape bespoke matching functions in
`machines.rs` with one generic, data-driven multiblock shape matcher —
**with no change to observable in-game behavior** for any existing
machine. This is a refactor, not a feature addition: the acceptance bar
is "identical results, generalized implementation."

## Current State (what's being replaced)

- `check_stack` (≈L249): shared by `check_bloomery`, `check_forge`,
  `check_glassworks`, `check_separator`, `check_kiln`. Scans a hardcoded
  3-wide, 3-tall firebrick ring around a variable "mouth" block, trying
  all four cardinal directions. Ring size, ring material, and layout are
  baked into loop bounds.
- `check_stall` (≈L222): fully independent shape logic — two log posts
  (2 tall) flanking a counter on either axis, bridged by a 3-wide awning.
  Does not share code with `check_stack`.
- `has_chimney` (≈L164): a second, structurally similar but
  code-separate ring scan — three more courses of firebrick above a
  stack's core, flue open.
- `vice_near` (≈L525): pure radius-proximity scan (is block X anywhere
  within a 7×3×7 box), not a fixed relative shape at all.

## In Scope

1. **A generic shape description type** — a multiblock shape as a set of
   cells at relative offsets from an anchor/controller position, each
   cell carrying a constraint (exact block, block tag, "any solid",
   "must be air"), plus which rotations are valid to try.
2. **A generic matcher function** — given a candidate anchor position and
   a shape definition, return the matched core/anchor plus a map of every
   matched position to its actual block ID (collect this even though
   nothing consumes it yet — it's what Pattern A stat-folding will read
   later, and collecting it now avoids a second refactor).
3. **Reimplement the five `check_stack`-based checks** (bloomery, forge,
   glassworks, separator, kiln) as shape definitions distinguished only
   by which block occupies the mouth cell — same pattern as today, now
   expressed as data instead of a hand-rolled loop.
4. **Reimplement `check_stall`** as a shape definition using the same
   generic matcher.
5. **Reimplement `has_chimney`** as a shape definition (either a
   standalone shape checked in addition to the stack, or folded into the
   forge/glassworks shape definitions directly — implementer's choice,
   whichever is cleaner against the matcher's actual API).
6. Existing public functions (`check_bloomery`, `check_forge`, etc.) stay
   as thin wrappers around the new matcher, calling it with their
   specific shape definition, so every other call site in the codebase
   is unaffected.

## Suggested Types (illustrative — implementer should adjust to fit
WildForge's actual registry/tag types, not treat this as final)

```rust
enum BlockConstraint {
    Exact(BlockId),
    Tag(TagId),      // e.g. "base:logs"
    AnySolid,
    Air,
}

struct ShapeCell {
    offset: (i32, i32, i32),   // relative to anchor
    constraint: BlockConstraint,
}

struct MultiblockShape {
    name: &'static str,
    cells: Vec<ShapeCell>,
    rotations: &'static [Rotation],  // e.g. the 4 cardinal directions check_stack tries today
}

struct MatchResult {
    core: (i32, i32, i32),
    matched: HashMap<(i32, i32, i32), BlockId>,
}

fn match_shape(world: &World, anchor: (i32, i32, i32), shape: &MultiblockShape)
    -> Option<MatchResult>;
```

## Suggested File Layout

- New file `src/world/multiblock.rs`: the generic `MultiblockShape`,
  `ShapeCell`, `BlockConstraint`, `MatchResult` types and the
  `match_shape` function. Keep this file free of any specific machine's
  logic.
- `src/world/machines.rs`: keeps machine-specific shape *definitions*
  (bloomery ring, forge ring + chimney, stall posts-and-awning) and the
  thin wrapper functions, now calling into `multiblock.rs`.

## Acceptance Criteria

- For each of bloomery/forge/glassworks/separator/kiln/stall: identical
  pass/fail results to the current implementation across — a valid
  structure in all 4 rotations, a single missing block, a single wrong
  block substituted, and a structure breached at each distinct position
  type (ring wall, ring corner, mouth, chimney course where applicable).
- `has_chimney`'s behavior (forge/glassworks gating) is unchanged.
- No existing call site of `check_bloomery`/`check_forge`/`check_stack`/
  etc. needs to change its own code — signatures are preserved.
- New unit tests exercise `match_shape` directly against at least one
  synthetic shape, independent of any real machine, to confirm the
  matcher is genuinely generic and not accidentally coupled to the
  firebrick-ring case.

## Non-Goals (do not implement in this task)

- **Slot-based modular frames** (spec Part 1.3) — no slot state, no
  module swapping.
- **Stat aggregation / Pattern A folding** (spec Part 2.1) — the matcher
  returns matched blocks, but nothing yet *uses* that data to scale
  output. Wiring that up is separate follow-on work.
- **`BlockEntity` enum generalization** — `Forge`, `Kiln`, `Bloomery`,
  `Anvil`, `Clamp` stay as they are for now. Migrating to a generic
  multiblock-instance variant is real, separate work; doing it alongside
  the matcher refactor would make this PR much harder to review safely.
- **Event-driven recheck-on-edit** — `tick_kilns` keeps polling
  `check_kiln` every tick for now. Replacing that with invalidate-on-
  `block_place`/`block_break` is a good next PR, but is a distinct
  change (needs hooking into the edit-event pipeline) and shouldn't be
  bundled with a pure recognition refactor.
- **`vice_near`** — left untouched. It's a proximity scan, not a
  relative-shape match, and doesn't fit this primitive; converting it
  (if ever) is a separate, smaller task.
- Anything from spec Parts 2–3: trains, rail, dungeons/structures, NPCs,
  dialogue, quests, settlements, enemy archetypes. Not touched here.

## Suggested PR Description Framing

> Refactors the bloomery/forge/glassworks/separator/kiln/stall multiblock
> checks in `machines.rs` onto one generic, data-driven shape matcher
> (`src/world/multiblock.rs`), replacing four independent hand-rolled
> matching functions with shape *definitions*. No gameplay behavior
> changes — this is groundwork for slot-based modular machines, mass-
> aggregated stats, and eventually block-built trains, per
> [design doc link]. Deliberately scoped to recognition only; slots,
> stat folding, the `BlockEntity` generalization, and event-driven
> invalidation are follow-up PRs.
