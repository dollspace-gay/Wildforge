# Task Brief: Rendering & Interpolation for Local Structures

**Repo:** WildForge, branch `feat/multiblock-recognition`
**Primary files:** unknown — this brief has no visibility into the
client/render architecture; see Open Questions before starting.
**Related design doc:** `wildforge-engine-extensions-spec.md`, and the
"Rendering & interpolation" item on Phase 6's roadmap. This brief
assumes Phase 6 (rail geometry + path-following) lands roughly as
specced — `RailState { current_cell, next_cell, progress, speed }` on
`LocalStructure`. If the actual implementation differs, adjust field
names accordingly; the interpolation approach below doesn't depend on
the exact shape, only on "a 0.0–1.0 progress value between two known
cells" existing somewhere.

## Goal

Given a `LocalStructure`'s current state (static `transform`, or
in-motion `rail` progress), compute a smooth, continuous world-space
position and facing suitable for rendering — without touching or
requiring changes to how `transform`/`rail` are stored or stepped.

## In Scope

### 7a-1 — A pure interpolation function
```rust
pub struct RenderPose {
    pub position: /* whatever float world-space type rendering already uses */,
    pub facing: Rotation,
}

pub fn interpolated_pose(structure: &LocalStructure, world: &World) -> RenderPose;
```
- If `structure.rail` is `None`: trivial case, resolve `transform.anchor`
  to a world-space position with zero interpolation and
  `facing = transform.rotation`.
- If `structure.rail` is `Some(rail)`: resolve **both** `rail.current_cell`
  and `rail.next_cell` to world-space positions (via whatever conversion
  already exists — see Open Questions, do not write a second one), then
  linearly interpolate between those two *already-converted* float
  positions using `rail.progress`. Facing snaps to the new
  `transform.rotation` at cell boundaries (matches Phase 6's rotation
  model — see Non-Goals on why this stays a snap, not a smooth turn).

### 7a-2 — Time sampling
However the renderer already handles tick-to-frame interpolation for
other moving things (players, mobs — the design doc references existing
snapshot/interpolation machinery for these), reuse that same sampling
pattern here rather than inventing a second one. If no such pattern
exists or isn't visible from this brief's vantage point, flag that
explicitly in the PR rather than guessing at a new scheme.

## Acceptance Criteria

- A static (`rail: None`) structure's `interpolated_pose` matches
  directly resolving `transform.anchor`/`transform.rotation` — no
  observable behavior change from Phase 5 for anything not on rails.
- A structure at `rail.progress == 0.0` resolves to exactly
  `current_cell`'s position; at `1.0`, exactly `next_cell`'s position;
  values in between lie on the straight line (in converted world-space
  floats) between the two.
- Facing changes exactly at cell-boundary crossings, matching
  `transform.rotation` as Phase 6 sets it — no interpolated/partial
  rotation.
- New unit tests: static pass-through case; interpolation midpoint and
  endpoints for a rail-following structure; confirms no reliance on raw
  `BlockPos` integer lerping (see the face-seam risk below).

## Non-Goals (do not implement in this task)

- **Continuous rotation/turning animation.** Facing only ever takes one
  of `Rotation`'s four cardinal values, snapping at cell boundaries —
  matches Phase 6's model exactly. Introducing smooth angular
  interpolation would mean introducing a whole new continuous-orientation
  representation alongside the existing four-way one; that's a bigger,
  separate decision, not something to fold in here.
- **Cross-face-boundary transit.** See the flagged risk below — a rail
  segment whose `current_cell` and `next_cell` sit on different planet
  faces is explicitly out of scope for this task. If Phase 6 already
  allows building rail across a face seam, this phase should either
  detect that case and fall back to a snap (no interpolation) rather
  than producing a wrong lerp, or the acceptance criteria above should
  be understood as same-face-only until a follow-up addresses it — pick
  one and state it plainly in the PR.
- **Actual GPU/scene-graph integration** — this task produces the pose
  data; wiring it into whatever draws a `LocalStructure` is separate,
  unreviewed-here work.
- **Camera-relative smoothing, easing curves, or any interpolation
  beyond linear** — plain lerp is sufficient for this phase.

## Flagged Risk: Planet-Face Coordinate Seams

`BlockPos` carries a `Face` plus `u`/`y`/`v` coordinates — consistent
with a cube-sphere-style planet, not a flat infinite grid. **Do not
linearly interpolate raw `u`/`y`/`v` integers directly.** Two adjacent
rail cells on the *same* face should convert cleanly to nearby
world-space floats and lerp correctly. Two cells that happen to straddle
a face boundary are a much harder case — `u`/`v` axes can remap
discontinuously across a seam, so a naive lerp there could produce a
position that visually jumps or cuts through the planet rather than
tracking the rail. This is exactly why cross-face transit is a stated
Non-Goal above rather than something this brief tries to handle
correctly on a guess.

## Open Questions to Resolve Before Starting

- **Does a `BlockPos → world-space float` conversion function already
  exist** (for rendering static blocks, presumably it must)? Find and
  reuse it for both `current_cell` and `next_cell` — do not write a
  second conversion.
- **How does the renderer currently interpolate other moving entities**
  (players, mobs) between ticks? Reuse that sampling/timing pattern.
- **What float type/precision does existing render code use for world
  positions** — match it rather than introducing `f64` (or `f32`) by
  guess.

## Suggested PR Description Framing

> Adds `interpolated_pose`, a pure function producing a smooth
> world-space position + facing for a `LocalStructure`, consuming
> Phase 6's rail progress. Reuses the existing `BlockPos`→world-space
> conversion and tick-interpolation sampling pattern rather than
> introducing new ones. Explicitly scoped to same-planet-face transit;
> cross-face rail segments fall back to a snap rather than an incorrect
> lerp (see PR for which). No rotation interpolation — facing still
> snaps at cell boundaries, matching Phase 6.
