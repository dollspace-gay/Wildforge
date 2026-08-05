# Task Brief: Capture & Stamp Tooling

**Repo:** WildForge, branch `feat/multiblock-recognition`
**Primary files:** likely a new `src/world/template.rs`; touches
`src/world/machines.rs`, `src/world/multiblock.rs` (reuse only, see
below); possibly `src/world/storage.rs` / `src/world/region.rs` — **not
reviewed for this brief, check them first** (see Open Questions).
**Related design doc:** `wildforge-engine-extensions-spec.md` — this task
implements Part 1.4 ("Capture & Stamp Tooling"). Piece-based procedural
structures (2.3) and bounded local-structure entities / trains (1.1,
2.2) build on this later but are not implemented here.

## Status Going In

Phases 1–3 are all in and working on this branch:

- Generic shape matching (`multiblock.rs`: `MultiblockShape`,
  `ShapeCell`, `BlockConstraint`, `match_shape`, `Rotation`).
- Generalized instance state (`MachineInstance` in `mod.rs`, one
  `BlockEntity::Multiblock` variant, no per-kind enum growth).
- Event-driven revalidation on block edit (`revalidate_multiblocks_around`
  in `mod.rs`).
- Pattern A stat folding (`fold_stats`/`EffectiveStats`) and Pattern B
  slot modules (`BlockConstraint::Module`, `module_catalog`,
  `fold_capabilities`/`Capabilities`, `swap_slot_module_at`). The
  `"casing"` category is live on one ring cell of the shared stack shape.

**Since Phase 3 resolved slot state as fully re-derivable from a live
world scan (no separate per-slot storage), this matters for Phase 4:**
capturing the raw blocks of a region automatically captures whatever
module is installed in any slot cell, for free — a captured forge
template with an advanced-firebrick slot just *is* that block at that
position in the template, same as any other cell. No special-casing
needed for slots in the capture format itself.

**Also relevant, from the merged main branch:** `BindingFrameState` and
`ChargeVesselState` are new `BlockEntity` variants holding item-stack
*contents* (mounts, a vessel, a reservoir) rather than structural block
identity. These are a different kind of "loadout" than machine slots —
they live in inventory-like fields on the block entity, not in world
blocks at distinct positions — and are the reason the scope decision
below (raw structure only, no `BlockEntity` contents) needs to be
explicit rather than assumed.

## Goal

Let a player select a built region, save it as a reusable template
(block layout + rotation, not contents), and place a copy of it
elsewhere — either instantly (materials permitting) or as a
fill-in-yourself ghost overlay.

## In Scope

### 4a — Capture
Given two corner positions (however region selection is normally done in
this codebase — check the existing block-selection/area-tool pattern
before adding a new one), record every non-air block's relative offset
from a chosen origin and its block id. **Deliberately excludes
`BlockEntity` state** — a captured chest is captured as an empty chest
block, a captured forge as an empty, unlit forge shell (whatever module
sits in its slot cells is still captured, since that's block identity,
not `BlockEntity` state). This is a scope decision, not an oversight:
including inventory-like contents (chest goods, binding-frame mounts,
machine charge/fuel) in a copyable template is a duplication-exploit
surface, and keeping capture to "structure only" sidesteps it entirely
without needing an allowlist/denylist of which `BlockEntity` fields are
"safe" to copy.

### 4b — Template storage
Persist captured templates (name, origin/core offset, cell list) to disk
— check whether `storage.rs` or `region.rs` already provide a
suitable persistence primitive before building a new one from scratch;
`entities.rs`'s TOML-writing pattern is a reasonable fallback precedent
if nothing generic already fits. Decide and document: are templates
personal (per-player) or shared (world-wide, any player can stamp any
saved template)? Either is defensible; pick one explicitly rather than
leaving it ambiguous.

### 4c — Placement: rotation
Support placing a template rotated. `multiblock.rs::Rotation` (already
public, already used by shapes) is directly reusable here — a template
is conceptually just cell offsets, exactly like `ShapeCell::offset`, so
`Rotation::apply` should need no modification to serve this too.

### 4d — Placement: instant stamp
Sum the template's cells into an aggregate block-id → count table,
check it against the player's inventory (reuse whatever existing
multi-item consume/has-enough helper the crafting/recipe system already
uses — do not write a second one), and place every block at once on
success. Fail cleanly, naming what's short, on insufficient materials.

### 4e — Placement: ghost overlay (recommended default per the original
design discussion — preserves the logistics constraint of getting
materials to the site, removes only arrangement tedium)
Recommended approach: **do not** require a rendered "ghost" variant of
every block in the game (that's a large, likely out-of-scope asset/
rendering ask). Instead, track pending-fill state server-side per
placed-template instance: the set of (position, required block id)
pairs not yet satisfied. Hook the normal block-place path so that
placing the correct block at a pending position clears that entry (and,
naturally, placing an incorrect block there is just treated as an
ordinary place — the player can always brute-force-fill by eye without
the tool's help if they don't trust the overlay). Once every pending
cell is cleared, the structure is just an ordinarily-built structure —
**no special activation step**, since Phases 1–3's revalidation-on-edit
already fires normally as the last block lands. This reuse is the
central efficiency argument for doing 1.4 after 1.2/1.3 rather than
before.

Client-side rendering of the ghost overlay itself (translucent preview
blocks) is real work but is presentation, not world-state logic — keep
it decoupled from the pending-fill tracking described above so the
server-side correctness doesn't depend on any particular rendering
approach.

## Suggested Types (illustrative)

```rust
// new module, e.g. src/world/template.rs
pub struct TemplateCell {
    pub offset: (i32, i32, i32), // relative to template origin
    pub block: BlockId,
}

pub struct Template {
    pub name: String,
    pub cells: Vec<TemplateCell>,
}

pub fn capture_region(world: &World, min: BlockPos, max: BlockPos, origin: BlockPos) -> Template;

pub enum StampMode {
    Instant,
    Ghost,
}

pub fn stamp_template(
    world: &mut World,
    template: &Template,
    anchor: BlockPos,
    rotation: crate::world::multiblock::Rotation,
    mode: StampMode,
) -> Result<(), &'static str>;

// pending-fill tracking for ghost mode — one instance per active overlay
pub struct PendingFill {
    pub anchor: BlockPos,
    pub remaining: std::collections::HashMap<BlockPos, BlockId>,
}
```

## Acceptance Criteria

- Capturing a region and immediately instant-stamping it elsewhere
  (rotation 0) reproduces the exact block layout, verified cell-by-cell.
- Stamping at each of the four `Rotation` values places blocks at the
  geometrically correct rotated positions (reuse the same rotation math
  `multiblock.rs` already tests against, or add equivalent coverage
  here).
- Instant stamp fails without placing anything when the player's
  inventory is short any required block, and the failure names what's
  missing.
- A ghost-mode template, once every pending cell is filled with its
  correct block via ordinary placement, results in a structure that
  passes the same `MachineKind::validate` (or whatever shape check
  applies) as if it had been hand-built — no template-specific
  activation logic runs.
- A captured machine template does not restore any `BlockEntity`
  contents on stamp — placing a captured forge yields an empty, unlit
  forge shell, confirmed by checking `MachineInstance` state after
  stamping.
- New unit tests: round-trip capture → stamp equality; rotated stamp
  correctness; instant-stamp material-shortfall failure path; ghost-mode
  fill-completion triggering ordinary revalidation with no special code
  path.

## Non-Goals (do not implement in this task)

- **Piece-based procedural structures** (spec Part 2.3 — pools,
  connector points, generation walks, terrain adaptation modes). This
  task is the authoring primitive that system will eventually use; it
  does not build the assembly/pooling logic itself.
- **Bounded local-structure entities / block-built trains** (spec 1.1,
  Part 2.2) — untouched.
- **Copying `BlockEntity` contents of any kind** — chests, furnace
  fuel/input, machine charge/fuel, binding-frame mounts, charge vessels.
  Structural block identity only, per 4a.
- **Ghost-overlay client rendering polish** — functional pending-fill
  tracking is in scope; a translucent-preview visual system is a
  separate, presentation-layer task.
- **New game content, UI polish beyond a minimal capture/stamp
  interaction** — same posture as prior phases.

## Open Questions to Resolve Before/During Implementation

- Does `storage.rs` or `region.rs` already provide a generic
  keyed-persistence primitive templates should use, rather than a new
  bespoke TOML writer? Check before designing 4b's format.
- Is there an existing region-selection tool (two-corner or similar)
  already used elsewhere (e.g. for area-based commands) that capture
  should reuse rather than duplicate?
- Personal vs. shared templates (4b) — pick one and document the choice
  in the PR description; don't leave it implicit.
- Does WildForge have any land-claim/permission concept that capturing
  a region should respect (e.g. can't capture inside another player's
  claimed area)? Not visible in the files reviewed for this brief.

## Suggested PR Description Framing

> Adds capture (region → reusable structural template) and stamp
> (instant or fill-in-yourself ghost placement) tooling. Templates
> record block layout only — no `BlockEntity` contents — closing off a
> duplication-exploit surface rather than requiring a per-field
> allowlist. Ghost-mode placement requires no new activation logic:
> once every pending cell is filled via ordinary block placement, the
> existing multiblock revalidation (Phases 1–3) takes it from there.
> Groundwork for piece-based dungeon/town assembly and block-built
> trains, per [design doc link].

## Roadmap After This

- **Phase 5 — Bounded local-structure entities** (spec 1.1): needed for
  block-built trains; benefits from this phase's stamp tool for saving
  car designs once both exist.
- **Phase 6 — Piece-based procedural structures** (spec 2.3): consumes
  this phase's `Template` type as its per-piece authoring format.
- **Content-layer track** (spec Part 3): independent, unaffected by this
  phase.
