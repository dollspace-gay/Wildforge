# Task Brief: Rail Geometry & Path-Following Motion

**Repo:** WildForge, branch `feat/multiblock-recognition`
**Primary files:** likely a new `src/world/rail.rs`; touches
`src/world/local_structure.rs` (adds optional motion state to
`LocalStructure`) and `src/world/mod.rs` (tick-dispatch wiring — see Open
Questions).
**Related design doc:** `wildforge-engine-extensions-spec.md` — this task
is a scoped-down slice of Part 2.2 ("Block-Built Trains & Rail"): rail
geometry and path-following motion only. Block-built (player-editable,
slot-bearing) car interiors, mass-driven power draw, cargo, player
throttle control, rendering, and collision are all explicitly deferred
— see Non-Goals.

## Status Going In

Phase 5 (amended) provides `LocalStructure`: a bounded, non-chunk-grid
block store with a `LocalTransform { anchor: BlockPos, rotation:
Rotation }`. Storage is canonical (never rotated in place);
`world_position(local_offset)` is the one correct way to resolve a cell
to a world position, applying `transform.rotation` exactly once. `rotate`
composes orientation in O(1). None of this moves yet — `transform` is
static, set once at `spawn_structure` and never updated afterward.

**Key scoping clarification for this phase:** a train *car* is a
`LocalStructure` (per the above). The *track* it rides on is not — rail
pieces are ordinary blocks placed in the main world's chunk grid, the
same as any other block. This means rail connectivity and switch state
live in `World`'s existing block/`BlockEntity` machinery, using the same
patterns already established for machines (Phases 1–3) — this phase
does **not** need to resolve the block-behavior-inside-a-moving-
structure question Phase 5 flagged and deferred. That question stays
deferred; it isn't a prerequisite for this phase.

## Goal

1. A small catalog of rail piece types (straight, curve, incline,
   switch) placeable as ordinary world blocks, with a connectivity query:
   given a rail cell and the direction you entered it from, which
   direction(s) can you continue in.
2. A path-following step: a `LocalStructure` marked as "on rails"
   advances along connected rail cells over time, updating its
   `LocalTransform` each step — anchor snapping to the current cell,
   rotation matching travel direction, with sub-cell progress tracked
   separately for later interpolated rendering (rendering itself is out
   of scope here).

## In Scope

### 6a — Rail piece catalog & connectivity
- Straight, curve (two variants: which pair of perpendicular directions
  it connects — reuse `Rotation`'s four cardinal values to distinguish
  orientation rather than inventing a second orientation system), incline
  (rise one block over the run of the piece), and switch (two possible
  exits, one currently selected).
- A connectivity function: `fn rail_exit(reg: &Registry, world: &World,
  pos: BlockPos, entered_from: Direction) -> Option<Direction>` (or
  equivalent) — given the piece at `pos` and which direction you arrived
  from, returns which direction to continue in, or `None` if the piece
  doesn't connect that way (dead end / wrong orientation).
- **Switch state** is small, persisted, mutable per-switch-block state —
  which exit is currently selected. This is an ordinary `BlockEntity`
  variant, following the exact pattern `Forge`/`Kiln`/etc. already
  established (Phases 1–3), not a new coupling concern. A minimal
  toggle interaction (switch the selected exit) is in scope; a full UI
  for it is not.

### 6b — Motion state on `LocalStructure`
Add optional rail-following state — a structure not on rails has `None`
here and behaves exactly as Phase 5 left it (fully static):

```rust
pub struct RailState {
    /// The rail cell the structure is currently departing.
    pub current_cell: BlockPos,
    /// The rail cell it's traveling toward.
    pub next_cell: BlockPos,
    /// 0.0 at current_cell, 1.0 at next_cell.
    pub progress: f32,
    /// Cells per tick (or per second — pick one unit and use it
    /// consistently; document the choice, since Part 2.2's later
    /// mass-driven power-draw formula will need to agree with it).
    pub speed: f32,
}

pub struct LocalStructure {
    // ...existing fields unchanged...
    pub rail: Option<RailState>,
}
```

### 6c — The step function
Each tick (see Open Questions for where this hooks into the existing
tick loop), for every `LocalStructure` with `rail: Some(_)`:
- Advance `progress` by `speed × dt`.
- If `progress >= 1.0`: the structure has arrived at `next_cell`. Query
  `rail_exit` from `next_cell`, using the direction of travel just
  completed, to find the new `next_cell`. If none exists (dead end,
  unbuilt track), stop the structure (`speed = 0` or clear `rail` —
  implementer's call, document whichever) rather than panicking or
  silently teleporting.
- On arrival, update `transform.anchor = current_cell` (the new
  departure point) and `transform.rotation` to match the new travel
  direction, using the same `Rotation` values everything else in this
  codebase already uses — no new orientation representation.
- Carry over any progress past `1.0` into the new segment rather than
  resetting to exactly `0.0`, so speed stays consistent across
  segment boundaries regardless of tick rate.

## Suggested Types (illustrative — adjust to fit existing conventions)

```rust
// rail.rs
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RailPiece {
    Straight,
    Curve,   // orientation carried by the placed block's own rotation
    Incline,
    Switch { selected: Direction },
}

pub fn rail_exit(
    world: &World,
    pos: BlockPos,
    entered_from: Direction,
) -> Option<Direction>;
```

Reuse `crate::world::multiblock::Rotation` for orientation wherever
possible rather than introducing a second directional enum — the
codebase already has one, and Phase 5's `world_position` shows the
established pattern for combining a rotation with a position.

## Acceptance Criteria

- A `LocalStructure` with `rail: None` is completely unaffected by this
  phase's tick step — confirms Phase 5's static structures keep working
  unchanged.
- Placing a straight run of rail pieces and stepping a structure along
  it advances `progress` correctly and transitions `current_cell`/
  `next_cell` at each boundary without skipping or double-counting a
  segment.
- A curve piece correctly changes travel direction on arrival — the
  structure's `transform.rotation` after the curve matches the new
  direction, not the old one.
- A switch's currently-selected exit determines which way a structure
  goes at that junction; toggling the switch changes the outcome for
  the *next* structure to arrive (does not need to redirect anything
  already mid-transit through it, unless that's trivial to also get
  right — not required either way, but state the behavior explicitly
  in the PR description).
- A dead end (no valid `rail_exit`) stops the structure cleanly — no
  panic, no silent teleport to an arbitrary position.
- New unit tests: straight-line stepping arithmetic (including
  progress carryover across a tick that completes a segment); curve
  reorientation; switch-directs-correctly; dead-end stop behavior.

## Non-Goals (do not implement in this task)

- **Rendering / visual interpolation** — `progress` is tracked as data
  for a later renderer to consume; nothing here draws anything.
- **Collision physics** — untouched, as in Phase 5.
- **Mass-driven power draw** (the belt/rail quantized-load-tier formula
  from the original design discussion) — this phase produces motion,
  not the cost of that motion. Separate, later task.
- **Player throttle control** — `speed` is a plain field a test or
  future system can set; no player-facing "accelerate/brake" input is
  wired up here.
- **Block-built (player-editable, slot-bearing) car interiors** — a
  `LocalStructure`'s own blocks remain exactly as Phase 5 left them:
  inert data, no ticking, no player interaction. This phase only makes
  the *structure as a whole* move; what's inside it is still frozen.
- **Cargo / inventory riding along with a structure.**
- **New rail-piece game content beyond what's needed to prove the
  pipeline** (a handful of test pieces is sufficient).

## Open Questions to Resolve Before/During Implementation

- **Where does the per-tick update loop for `LocalStructure`s hook in?**
  Not visible in the files reviewed for this brief. `World` clearly has
  an existing tick-dispatch pattern (`tick_kilns` etc. from Phases 1–3);
  find its registration point and add a sibling call for rail-following
  structures, rather than inventing a second tick-scheduling mechanism.
- **Does the block registry already support a "facing/orientation"
  property on placed blocks** (needed to distinguish which two
  directions a curve piece connects, or an incline's rise direction)?
  Not visible in the files reviewed for this brief — check before
  deciding whether curve/incline orientation is encoded as distinct
  block ids (e.g. `rail_curve_ne`, `rail_curve_nw`, ...) or via a shared
  orientation field, whichever the existing block system already
  supports.
- **Units for `speed`** — cells/tick or cells/second. Either is fine;
  document the choice, since the deferred mass-driven power-draw phase
  will need to agree with whatever unit this phase settles on.

## Suggested PR Description Framing

> Adds a rail piece catalog (straight/curve/incline/switch) as ordinary
> world blocks with a connectivity query, plus optional path-following
> motion state on `LocalStructure` (Phase 5). A per-tick step advances
> structures along connected rail cells, updating their `LocalTransform`
> via the existing `world_position`/`Rotation` primitives — no new
> orientation system introduced. Switch state uses the same
> `BlockEntity` pattern as existing machines; no change to the
> deferred block-behavior-inside-a-structure question from Phase 5,
> since rails live in the main world, not inside a structure. No
> rendering, collision, throttle control, or power-draw cost — motion
> only. Per [design doc link].

## Roadmap After This

- **Rendering & interpolation**: consumes this phase's `progress` field
  to draw smooth motion between discrete cells.
- **Mass-driven power draw**: the quantized load-tier formula from the
  original design discussion, applied to rail-following structures.
- **Block-built car interiors**: resolves the tick-coupling question
  Phase 5 deferred, letting a car's own blocks (an onboard machine, say)
  actually function while riding.
- **Piece-based procedural structures** (spec Part 2.3): independent of
  this phase, still pending, also builds on `Template`.
