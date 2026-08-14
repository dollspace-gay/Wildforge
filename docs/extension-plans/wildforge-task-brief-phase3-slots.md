# Task Brief: Slot-Based Modular Frames

**Repo:** WildForge, branch `feat/multiblock-recognition`
**Primary files:** `src/world/multiblock.rs`, `src/world/machines.rs`,
`src/world/mod.rs` (`BlockEntity`/`MachineInstance` definitions, the edit
hook around L2075–2145)
**Related design doc:** `wildforge-engine-extensions-spec.md` — this task
implements Part 1.3 ("Slot-Based Modular Frames"). Capture/stamp tooling
(1.4), bounded local-structure entities (1.1), and everything in Parts
2.2–3 remain out of scope.

## Status Going In

Phases 1–2 are both in on this branch. Concretely, as of the current
`mod.rs`/`multiblock.rs`/`machines.rs`:

- `MultiblockShape`/`ShapeCell`/`BlockConstraint` (`multiblock.rs`) is the
  generic, data-driven shape matcher. `BlockConstraint` currently has five
  variants: `OneOf`, `Exact`, `Tag`, `Air`, `SolidOrGlass`.
- `MachineInstance` (`mod.rs` L207–240) is the generalized instance
  representation Phase 2 built: `kind: MachineKind`, `lit`, `progress`,
  `core`, item-stack slots (`charge`/`reagent`/`fuel`), machine-specific
  counters, and `stats: EffectiveStats` (folded Pattern A output). It
  also carries **`slots_placeholder: ()`** — reserved, explicitly
  documented as "so slot-based module state (spec Part 1.3) can be added
  without a second rewrite," and explicitly not designed yet. This task
  designs and fills that in.
- Stat aggregation (Pattern A) is real and working: `fold_stats` walks
  `MatchResult.matched` and sums `heat_retention` per block into
  `EffectiveStats`, consumed as `heat_multiplier()` by
  `tick_bloomeries`/`tick_forges`/`tick_kilns`.
- Revalidation is fully event-driven: block edits call
  `revalidate_multiblocks_around`, which finds every instance whose
  `edit_region` covers the edited position (`pos_within_extent`, O(1) per
  instance) and re-runs `kind.validate(...)`, re-folding stats and
  dousing a lit machine whose shell broke. No per-tick polling remains.
- **There is already a proto-slot pattern, worth generalizing rather than
  replacing.** `MachineKind::mouth()` returns `[Option<BlockId>; 2]` (the
  handed/lit block pair), and the anchor cell's constraint is
  `BlockConstraint::OneOf(mouth)`. In other words: *which specific block
  occupies one particular cell* already determines the structure's
  identity (Forge vs. Kiln vs. Separator, since all three share
  `stack_shape`). That's a slot — just a single hardcoded one, read only
  to pick a `MachineKind` enum variant, with no player-facing swap
  interaction (you build the right mouth block into the stack; there's
  no "swap the mouth module" action once it's there).

Slots (Part 1.3) are this same idea, generalized: any cell in a shape,
not just the anchor, holding one of a data-defined catalog of
interchangeable modules, swappable in place, and read for more than just
enum selection.

## Goal

Let any cell in a `MultiblockShape` be declared a **module slot** with a
named category (e.g. `"casing"`, `"output"`, `"catalyst"`), backed by a
data-defined catalog of which blocks qualify as modules for that
category and what each one contributes. Let the player swap a slot's
installed module in place, at that specific position, without disturbing
the rest of the structure or its `BlockEntity`. Persist per-slot module
identity across save/load and across revalidation.

## In Scope

### 3a — Extend `BlockConstraint` with a slot marker
Add a variant — e.g. `Module(&'static str)` (category id) — alongside
the existing five. A cell with this constraint matches if the world
block there belongs to the named category's catalog (mirrors how
`OneOf`/`Tag` already resolve against a set; a module category is
functionally "OneOf, but the set is named and swappable, and matching
also reports *which* module matched" — `MatchResult.matched` already
gives you the matched block per position for free, so no new return
type should be needed here).

### 3b — A module catalog
Data (not hardcoded per machine) mapping category → list of qualifying
block IDs, each carrying whatever it contributes. Two contribution
shapes worth keeping separate rather than conflating:
- **Numeric** — feeds the existing `fold_stats`/`EffectiveStats`
  pipeline (a module can be Pattern-A-eligible, same as firebrick tier
  already is).
- **Qualitative/behavioral** — a module changes what the structure *can
  do*, not just how well (an optional slot present or absent unlocks a
  capability; which module fills a slot picks among distinct behaviors,
  not points on a scale). `EffectiveStats` is the wrong home for this —
  decide the representation here (a capability flag/enum set is
  probably enough for a first pass; avoid over-designing beyond what the
  proof-of-concept module needs).

### 3c — Per-slot persisted state
Replace `slots_placeholder: ()` with real state. Open design question to
resolve during implementation, not assume: **is anything beyond "which
block currently occupies this relative offset" actually needed?** The
mouth-block precedent suggests slot identity might be fully
reconstructible from the world + shape match at revalidation time (the
same way `mouth` block selection already works today, with no separate
storage). If a genuine need for extra per-slot state emerges (a module
with its own durability/charge, say), keep it a
`HashMap<(i32,i32,i32) /* offset, not world pos */, SlotState>` so it
survives the instance moving conceptually (it won't move yet, but
storing by relative offset rather than world position keeps this
consistent with how `ShapeCell` offsets already work, and avoids
rework if it matters later).

### 3d — Slot swap interaction
Player-facing: interacting with a specific matched slot cell's world
position opens a way to choose a replacement from that category's
catalog (subject to having the item, however the existing swap-cost
model works elsewhere — check `light_machine_at`'s charge-consumption
pattern for precedent) and swaps the block there. Reuse
`swap_block_keep_entity_at`'s *pattern* (swap a block, keep the
`BlockEntity` at that anchor untouched) but note it currently swaps the
block at the **anchor position only** — a slot swap needs to target an
arbitrary matched cell position that isn't necessarily the anchor, so
this likely needs a small sibling function, not a direct reuse of the
existing one. After swapping, the instance at the *anchor* still needs
its stats/capabilities re-folded (call the same revalidation path used
today) since the slot's contribution changed.

## Suggested Types (illustrative — adjust to fit existing conventions)

```rust
// multiblock.rs — new BlockConstraint variant
enum BlockConstraint {
    OneOf(Vec<BlockId>),
    Exact(BlockId),
    Tag(&'static str),
    Air,
    SolidOrGlass,
    Module(&'static str), // category id, resolved against the catalog
}

// module catalog — lives near MachineKind's mouth() for now, or its own
// small registry if categories end up shared across several MachineKinds
struct ModuleDef {
    block: BlockId,
    // numeric contribution reuses whatever fold_stats already reads
    // (e.g. heat_retention on the block), no separate field needed
    capabilities: CapabilitySet, // whatever minimal repr 3b lands on
}

fn modules_in_category(reg: &Registry, category: &str) -> &[ModuleDef];

// mod.rs — replaces slots_placeholder
pub slot_modules: HashMap<(i32, i32, i32), BlockId>, // offset -> installed module, ONLY if 3c's open question concludes extra storage is needed
```

## Files Likely Touched

- `src/world/multiblock.rs` — new `BlockConstraint::Module` variant,
  module category resolution helper.
- `src/world/machines.rs` — at least one real shape updated to declare a
  slot cell, proving the pipeline end-to-end (a good candidate: turn one
  of the stack's existing ring cells, or a new cell, into a slot with 2+
  real interchangeable modules — doesn't need to be gameplay-balanced,
  just real).
- `src/world/mod.rs` — `MachineInstance`'s `slots_placeholder` becomes
  real state (or is removed, if 3c concludes no extra storage is
  needed); the swap-interaction entry point (wherever block interaction
  is currently dispatched — not present in the files reviewed for this
  brief, so locate the existing interact/right-click handling equivalent
  to chest or mouth-block interaction before starting 3d).

## Acceptance Criteria

- A shape with at least one `Module`-constrained cell validates
  correctly when any catalog member for that category occupies the cell,
  and fails when a non-catalog block is there — mirrors existing
  `OneOf`/`Tag` test coverage, just for the new variant.
- Swapping a slot's module in place (via 3d) does not invalidate the
  instance's `BlockEntity` (the `charge`/`fuel`/`progress` etc. survive
  the swap untouched) and does trigger a stats/capability refold
  afterward.
- If the swapped-in module changes a numeric stat (heat, say), that
  change is observable immediately — no requiring the player to
  break/rebuild, same guarantee Phase 2 already gives for ring-tier
  swaps.
- Save/load round-trips whichever slot representation 3c lands on
  without loss (extend `entities.rs`'s serialization if per-slot state
  beyond world-block-identity is added; no change needed there if 3c
  concludes block-identity-at-revalidation-time is sufficient).
- New unit tests: shape validation with a `Module` cell in both valid
  and invalid states; a swap-and-refold test proving stats change
  without a full revalidation cycle being required externally.

## Non-Goals (do not implement in this task)

- **Capture/stamp tooling** (spec Part 1.4) — no saving/stamping of slot
  loadouts as templates yet.
- **Bounded local-structure entities / trains** (spec 1.1, Part 2.2) —
  untouched.
- **Generalizing `MachineKind` itself** beyond what's needed to support
  slots — Bloomery/Forge/Kiln/Separator stay as today's fixed enum; this
  task adds slots *within* their existing shapes, it doesn't change how
  machine identity is selected at the top level.
- **New balanced game content** — one proof-of-concept slot with two or
  three test modules is sufficient; a fully content-designed module
  catalog (real casings, real tiers) is separate, later work.
- **Piece-based structures, NPCs, dialogue, quests, settlements, enemy
  archetypes, ire generalization** — untouched, per the spec's Part 3
  content-layer track.

## Suggested PR Description Framing

> Generalizes the mouth-cell pattern (a single hardcoded slot picking
> machine identity) into a reusable `BlockConstraint::Module` primitive:
> any shape cell can now be declared a named-category slot, backed by a
> data-defined module catalog, with an in-place swap interaction that
> preserves the instance's `BlockEntity` and re-folds its stats
> afterward. Fills in `MachineInstance::slots_placeholder`, reserved for
> this since the Phase 2 refactor. One proof-of-concept slot added to an
> existing shape; no new balanced content. Groundwork for the
> capture/stamp tool and block-built trains, per [design doc link].

## Roadmap After This

- **Phase 4 — Capture/stamp tooling** (spec 1.4): now meaningfully more
  valuable once a frame's saved state includes slot loadouts, not just
  raw blocks.
- **Phase 5 — Bounded local-structure entities** (spec 1.1): needed
  before block-built trains; independent of this phase but benefits from
  Phase 4's stamp tool for saving car designs once both exist.
- **Content-layer track** (spec Part 3): independent, can proceed on its
  own timeline regardless of the above.
