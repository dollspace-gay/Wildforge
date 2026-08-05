# Task Brief: Bounded Local-Structure Entities

**Repo:** WildForge, branch `feat/multiblock-recognition`
**Primary files:** likely a new `src/world/local_structure.rs`; reuses
`src/world/template.rs` (`Template`, `TemplateCell`) as its authoring
format; touches `src/world/mod.rs` for a new persisted collection and
command surface, mirroring `templates`/`template_command`.
**Related design doc:** `wildforge-engine-extensions-spec.md` — this task
implements Part 1.1 ("Bounded Local-Structure Entity"), scoped down
deliberately (see below). Rail geometry, path-following movement, and
block-built trains proper (spec Part 2.2) are **not** implemented here —
this phase is the static representation only.

## Status Going In

`template.rs` (Phase 4) gives a working capture/stamp pipeline: `Template`
is a named `Vec<TemplateCell>` (offset + block name, resolved to
`BlockId` at use time), captured from a world region and either
instant-stamped or ghost-filled into the main chunk grid. Crucially,
`Template` is already **coordinate-space-agnostic** — its cells are
relative offsets, resolved against a world anchor only at
`rotated_cells()`. That's the exact shape a local structure's own block
store needs, just resolved against a *moving* transform instead of a
fixed world anchor. This phase's central move is: stop always resolving
a `Template` into the main chunk grid, and instead let it become a
free-standing, non-chunk-grid object.

## Goal

A `LocalStructure`: a small, self-contained block store (built from a
`Template`, reusing that type directly rather than re-deriving a second
capture format) that exists independently of the main world's chunk
grid, has its own position (a "transform" — for this phase, **static**;
path-following motion is explicitly deferred, see Non-Goals), and can be
queried/set block-by-block in its own local coordinate space.

## In Scope

### 5a — The block store itself
A `LocalStructure` holds its cells as `HashMap<(i32,i32,i32), BlockId>`
(local offsets, not `BlockPos` — a `LocalStructure` is bounded and small
by design, so it does not need chunking, planet faces, or any of the
main world's addressing machinery). Build one directly from an existing
`Template`:

```rust
pub struct LocalStructure {
    pub id: LocalStructureId, // new id space, e.g. a u64 counter like templates could use
    pub name: String,         // which Template it was spawned from, for save/debug
    pub blocks: HashMap<(i32, i32, i32), BlockId>,
    pub transform: LocalTransform, // position + rotation; static in this phase
}

pub struct LocalTransform {
    pub anchor: BlockPos,           // where in the main world it currently renders/sits
    pub rotation: crate::world::multiblock::Rotation, // reuse directly, do not redefine
}

pub fn from_template(template: &Template, reg: &Registry) -> LocalStructure;
```

`from_template` should reuse `Template::cells` and the same
name-to-`BlockId` resolution `template.rs::rotated_cells` already does
— do not write a second name→id lookup.

### 5b — World-level collection, spawn/persist, minimal command surface
Add `World.local_structures: Vec<LocalStructure>` (or keyed map),
persisted the same way `templates` is (`local_structures.toml`,
mirroring `save_templates`/`load_templates`'s exact pattern — versioned
file struct, `atomic_write`, silent no-op on missing/mismatched-version
file). Extend `template_command`'s command surface (or add a sibling
command set) with something like `spawn <template_name> <face> <u> <y>
<v> [rot]` that creates a `LocalStructure` from a saved template **without**
touching the main chunk grid at all — this is the key behavioral
difference from `stamp_instant`/`stamp_ghost`, both of which write into
`World`'s real block storage.

### 5c — Read/write within a `LocalStructure`
Simple accessors (`get_block`, `set_block` by local offset) — enough to
prove the store is genuinely independent and mutable, not enough to wire
up full player interaction with a placed instance yet (see Non-Goals).

## Acceptance Criteria

- Spawning a `LocalStructure` from a saved template does **not** modify
  any block in the main world's chunk storage — verify by checking the
  target region's blocks remain unchanged (still `AIR` or whatever they
  were) after spawn, in contrast to `stamp_instant`/`stamp_ghost` which
  deliberately do write there.
- The spawned structure's own block store exactly matches the source
  template's cells (same round-trip guarantee `capture_region`/
  `stamp_instant` already have, just resolved into `LocalStructure`
  storage instead of world storage).
- Rotating a spawned structure's transform correctly reorients its
  blocks in local space, reusing `Rotation::apply` — same rotation math
  already tested for shapes and templates, no new rotation logic
  introduced.
- Persistence round-trips: save, reload the world, confirm spawned
  structures and their transforms survive.
- New unit tests: `from_template` round-trip; spawn-does-not-touch-
  main-world-blocks; save/load round-trip for `local_structures.toml`.

## Non-Goals (do not implement in this task — deliberately deferred)

- **Movement / path-following transform.** `LocalTransform` is static
  this phase. Driving `anchor` parametrically along a rail spline is
  rail/train-specific work (spec Part 2.2) and depends on rail piece
  geometry that doesn't exist yet — bundling it here would blow this
  phase's scope for no testable gain, since there's nothing to follow a
  path along yet.
- **Rendering and collision.** How a `LocalStructure` actually draws in
  the game world and what it physically collides with is real,
  separate, likely larger work (probably touching client/render code
  this brief has no visibility into). This phase produces the
  authoritative world-state representation only.
- **Block ticking / behavior inside a `LocalStructure`.** This is the
  single biggest open question this phase surfaces rather than resolves
  — see below. Blocks placed in a `LocalStructure` are inert data in
  this phase; no furnace fire, no growth, no `BlockEntity` behavior of
  any kind runs on them yet.
- **Player interaction with a placed `LocalStructure`** (breaking/
  placing blocks in it post-spawn, opening a chest inside it, etc.).
- **Multiblock recognition running *inside* a `LocalStructure`** (e.g. an
  engine slot as its own validated shell within a car). `match_shape`
  is currently signed against `&World`/`BlockPos` specifically;
  generalizing it to run against either storage is real work, better
  scoped once there's an actual feature needing it.
- **Any rail, train-car-specific, or new game content.**

## Open Question This Phase Surfaces (flag, don't resolve here)

`World`'s existing per-kind tick functions (`tick_kilns`, etc., and by
extension anything driving `BlockEntity` behavior) are methods on
`World`, operating over `self.block_entities` — tightly coupled to the
main world's own storage, not written against any block-store
abstraction. Making a `LocalStructure`'s blocks genuinely *do* things
later (a lit forge riding in a train car) will require either
duplicating tick logic for local structures or generalizing the
existing tick functions behind a trait both `World` and `LocalStructure`
implement. That's a real, separate design decision — flagging it now so
whoever scopes the *next* phase (block behavior inside local structures)
goes in with eyes open, rather than discovering the coupling mid-task.
Do not attempt this generalization in this task.

## Suggested PR Description Framing

> Introduces `LocalStructure`: a small, self-contained block store built
> directly from Phase 4's `Template` type, holding its own blocks in
> local-offset space independent of the main chunk grid, with a static
> world transform. No movement, rendering, collision, or block behavior
> yet — this is the representation primitive block-built trains (spec
> Part 2.2) will eventually attach path-following motion and rail
> geometry to. Surfaces, but does not resolve, the tick-coupling
> question that block behavior inside a moving structure will need to
> answer. Per [design doc link].

## Roadmap After This

- **Rail geometry & path-following** (spec Part 2.2, scoped down):
  attaches a moving transform to `LocalTransform`, driven by progress
  along track pieces. Depends on this phase's representation existing
  first.
- **Block behavior inside local structures**: resolves the tick-coupling
  open question above. Needed before a block-built train car can
  meaningfully contain a working machine.
- **Piece-based procedural structures** (spec Part 2.3): independent of
  this phase, also builds on `Template`.
