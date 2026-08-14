# Task Brief: Player-Facing In-Structure Building

**Repo:** WildForge, branch `feat/multiblock-recognition`
**Primary files:** `src/world/raycast.rs` (new structure-aware
targeting), `src/world/local_structure.rs` (a narrow structure-scoped
break/place pair), `actions.rs` (routing — **not reviewed for this
brief**, see Open Questions; only line-number pointers were available).
**Related design doc:** `wildforge-engine-extensions-spec.md`. This is
the "player-facing in-structure building" item deferred by Phase 7b.

## Status Going In

- **Raycasting has zero concept of `LocalStructure` today.**
  `raycast_at`/`raycast_water_at` (`raycast.rs`) walk `World`'s chunk
  grid exclusively via `world.get_block_at`. A parked structure is
  currently invisible to targeting — a player couldn't select or
  interact with one at all, regardless of anything Phase 7b built.
- **The flat DDA already exists, just gated to tests.** `cast` (the
  `Vec3`-based Amanatides–Woo walk, currently `#[cfg(test)]`-only) has
  no planet-topology handling — no `Face`, no chart coordinates, no
  cube-edge rotation. That's not a limitation for this use case, it's a
  match: a structure's local space is exactly this shape. `cast_at`'s
  topology-awareness exists to handle planet-face seams a structure's
  interior will never have.
- **`World::break_block_at`/`place_block_at`/`place_item_block_at`
  (`mod.rs`) are deeply `World`-specific** — material ledger, alchemy
  apparatus state, arcane geography/ecology harvesting, dross scars,
  planetary weather, chunk dirty-tracking (`player_touched`), farmland
  soil initialization, and a placement-time marker-block-entity-seeding
  table (`wheel`/`sail`/`pump`/`generator`/`firebox`/`separator`/
  `discovery_lab`/`binding_frame` interaction strings → an initial
  `BlockEntity`). None of this is written against `BlockStore` — it's
  plain `World` methods reading `self.*` fields directly. **Reusing
  these functions wholesale against a `LocalStructure` isn't viable**;
  most of what they do (ecology, weather, farmland, chunk tracking)
  correctly doesn't apply inside a structure at all — consistent with
  7b already making `material_ledger`/`weather_at` return
  structure-appropriate no-ops via the `BlockStore` trait.
- **Machine instantiation-on-placement is a mixed model worth noting.**
  `place_block_at`'s marker table eagerly seeds a `BlockEntity` the
  moment a qualifying block is placed (e.g. `separator` gets a
  `MachineInstance` immediately, before any shape validation). Forge/
  Kiln/Bloomery aren't in that table — for those, instantiation
  presumably happens via the shape-match-on-edit path instead
  (`revalidate_machine_at`). Phase 7b's stated acceptance criteria
  ("build → validate → light → tick → complete" from scratch) implies
  the structure-side revalidation path already handles first-time
  instantiation, not just keeping existing instances in sync — **worth
  explicit confirmation during implementation**, not assumed here
  without `machines.rs` in hand this round.

## Goal

Let a player aim at, and break/place blocks inside, a parked
`LocalStructure` — with targeting correctly picking whichever is
closer (world geometry or a structure), and placement/breaking inside a
structure producing the same downstream effects (drops, machine
revalidation) a world-hosted equivalent would, without inheriting
`World`-only mechanics that don't apply.

## In Scope

### 8a — Structure-aware targeting
A new function, additive alongside the existing raycasters (do **not**
change `raycast_at`'s behavior or signature — some callers, e.g.
non-interactive AI line-of-sight, may not want structure-awareness;
changing shared behavior silently risks surprising them):

```rust
pub enum TargetHit {
    World(PlanetHit),
    Structure {
        id: LocalStructureId,
        block: (i32, i32, i32),
        adjacent: (i32, i32, i32),
    },
}

pub fn raycast_target_at(
    world: &World,
    origin: EntityPos,
    dir: Vec3,
    max_dist: f32,
) -> Option<TargetHit>;
```

Approach: run the existing `cast_at` for the world hit. Separately,
broad-phase reject against each nearby `LocalStructure` (an AABB
derived from its occupied local cells, resolved to world space via
`world_position`) — for structures within `max_dist`, transform the ray
into the structure's local space (inverse of `transform.anchor` +
`transform.rotation`) and DDA against it using the promoted `cast`
function (see below). Compare the world hit's and the nearest
structure hit's distances; the closer one wins.

**Promote `cast` out of `#[cfg(test)]`.** It's the correct raycaster for
this job (see Status above) — reuse it rather than writing a third DDA
implementation. Its existing signature (`Vec3` origin/dir) fits a
structure's local space directly once the ray is transformed into it.

Given structure counts are expected to be small (bounded, self-contained
objects, not world-scale content), brute-force checking every
`local_structures()` entry within range is acceptable for this phase —
flag, don't optimize preemptively, if this ever needs a spatial index
later.

### 8b — Structure-scoped break/place
A deliberately narrow pair, not a generalized `World` function:

```rust
impl LocalStructure {
    pub fn break_block(&mut self, offset: (i32, i32, i32), tool: Option<ItemId>) -> Option<ItemStack>;
    pub fn place_block(&mut self, offset: (i32, i32, i32), block: BlockId) -> bool;
}
```

Reuse only what's genuinely shared: `reg.block(block).hardness` for
break eligibility, `reg.drops_for(block, tool)` for drop calculation,
and the *subset* of `place_block_at`'s marker-seeding table that's
actually relevant to structure-hosted content (confirm which entries —
almost certainly not `binding_frame`/`discovery_lab`/farmland-adjacent
ones, but decide explicitly rather than copying the whole table
unexamined). Do **not** pull in material ledger, alchemy apparatus
checks, ecology harvesting, weather, or chunk dirty-tracking — these
correctly don't apply, matching 7b's precedent
(`material_ledger`/`weather_at` already no-op for structures via
`BlockStore`).

`set_block` (7b) already triggers `revalidate_machines_around` — reuse
it inside `place_block`/`break_block` rather than duplicating that call.

### 8c — Drop destination (explicit decision, not an assumption)
When a player breaks a block inside a structure, does the drop go to
the breaking player's inventory directly (natural for "I'm standing in
this car mining a wall"), or into the structure's `outbox` (consistent
with how structure-hosted machine outputs already collect there per
7b)? These are genuinely different UX, not just an implementation
detail — pick one explicitly and state it in the PR description. Player-
direct is probably the intuitive choice for *player-caused* breaks,
reserving `outbox` for *machine-caused* outputs as it's already used —
but this is a judgment call worth making deliberately, not inheriting
by default.

## Acceptance Criteria

- `raycast_target_at` returns `TargetHit::World` when only world
  geometry is in range, unchanged from `raycast_at`'s existing behavior
  for that case.
- Aiming at a parked structure from outside it returns
  `TargetHit::Structure` with the correct local offset and adjacent
  cell.
- When a structure is positioned such that both world geometry and the
  structure are along the same ray, the nearer of the two wins,
  verified at a few relative distances (structure closer, world closer,
  roughly tied).
- Breaking a block inside a structure correctly removes it, produces
  the expected drop (per whichever 8c decision was made), and triggers
  the same revalidation `set_block` already provides — confirmed by
  breaking one wall of an in-structure forge shell and observing it
  invalidate, mirroring 7b's world-side revalidation test.
- Placing a block inside a structure correctly adds it and, if it
  completes a valid machine shape, results in a functioning
  `MachineInstance` — confirms whichever answer 8's Status section
  flagged (first-time instantiation via revalidation) actually holds.
- New unit tests: `cast` (promoted) against a synthetic local-space
  scene; `raycast_target_at` distance-comparison cases above;
  structure `break_block`/`place_block` round-trip with drop and
  revalidation checks.

## Non-Goals (do not implement in this task)

- **Full `World`-mechanic parity inside structures** — no alchemy
  apparatus, ecology/arcane harvesting, farmland/soil, dross scars, or
  weather-driven behavior for structure-hosted blocks. These are
  correctly world-only concepts; a structure doesn't need them, per
  7b's existing `material_ledger`/`weather_at` no-ops.
- **Building/interacting with a structure while it's moving** — targeting
  and break/place apply to parked (`rail: None`, or at minimum
  momentarily stationary) structures only. Live-editing a
  moving structure is a harder problem (target resolution against a
  continuously-changing transform) and isn't required for this phase to
  be useful.
- **Networking/protocol changes beyond scoping the question** — see Open
  Questions. This brief identifies that `C2S::Break`/`C2S::Place`
  likely need a structure-aware variant; it doesn't design the wire
  format, since that depends on `actions.rs` internals not reviewed
  here.
- **UI/crosshair highlighting polish** — presentation layer, same
  posture as 7a's rendering non-goals.
- **A spatial index for structure broad-phase lookup** — brute-force
  over `local_structures()` is acceptable at this phase's expected
  scale.

## Open Questions to Resolve Before/During Implementation

- **`actions.rs` internals — genuinely not reviewed for this brief.**
  Before finalizing the routing piece, check: how does the player's
  mining-progress loop currently call `raycast_at` and use the result
  (swap in `raycast_target_at`, branch on `TargetHit`)? How does the
  `on_block_break` script hook (cancellable, per the pointer at
  actions.rs:1129–1138) receive its target — does it assume a `BlockPos`
  that a structure-hit can't naturally provide? Same question for the
  place path (actions.rs:2287–2292).
- **`C2S::Break`/`C2S::Place` wire format** — these almost certainly
  carry a `BlockPos` today. Routing a guest's in-structure break/place
  needs either a new enum variant (`C2S::BreakStructure { id, offset }`
  or similar) or some other resolution. This is a real protocol change
  with compatibility implications — worth its own deliberate look once
  `actions.rs` is in hand, possibly as a follow-up amendment rather than
  guessed at here.
- **Confirm structure-side first-time machine instantiation** — verify
  against current `machines.rs`/`revalidate_machine_at` whether building
  a new valid shape from scratch inside a structure actually creates a
  `MachineInstance` (implied by 7b's acceptance criteria) or only
  revalidates already-registered ones.
- **Which marker-seed table entries (if any) are relevant inside a
  structure** — decide explicitly per 8b rather than copying the whole
  table.

## Suggested PR Description Framing

> Adds structure-aware targeting (`raycast_target_at`, promoting the
> existing test-only flat DDA `cast` to production use for a
> structure's local space) and a narrow, structure-scoped break/place
> pair on `LocalStructure`, reusing only the genuinely shared pieces of
> `World`'s break/place logic (hardness, drops, relevant marker-seeding)
> rather than the full `World`-coupled functions. Machine revalidation
> reuses 7b's existing `set_block` hook. Explicitly does not add
> world-mechanic parity (alchemy/ecology/weather/farmland) inside
> structures, and does not yet touch networking — see PR for the
> `C2S::Break`/`Place` protocol question this surfaces. Per [design doc
> link].

## Roadmap After This

- **Networking/protocol support** for guest-initiated in-structure
  break/place, once `actions.rs`'s current shape is confirmed.
- **Building/editing a moving structure** — harder targeting problem,
  deferred.
- **Piece-based procedural structures** (spec Part 2.3) — independent,
  still pending.
- **Mass-driven power draw** — still pending, now closer: with
  in-structure machines functioning (7b) and player-buildable (this
  phase), an engine module wiring to `rail.speed` is the natural next
  gameplay payoff.
