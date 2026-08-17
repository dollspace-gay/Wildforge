# WildForge Engine Extensions — Unified Specification

Proposed additions to WildForge's engine/mod API to support a questing
roleplay factory builder. Organized by dependency: **primitives** first
(the few building blocks everything else composes from), then **systems**
built on them, then **content-layer** additions that are mostly independent.

---

## Part 0 — Current State (`src/world/machines.rs`)

Grounding the primitives below against what's actually implemented today,
so the ask is legible as "generalize this" rather than "add this."

**Multiblock checking exists, but as one function per shape family, not a
general recognizer.** `check_stack` is shared by the bloomery, forge,
glassworks, separator, and kiln checks — it scans a hardcoded 3-wide,
3-tall firebrick ring around a variable "mouth" block, trying all four
cardinal directions to find where the mouth sits. That's real reuse of
*one* topology across several machine identities (the mouth block
selects which machine/recipe-table applies), but the ring's size, ring
material, and layout are baked into the loop bounds — not data. A new
machine with a differently-shaped stack needs a new function, not a new
shape description.

Three more machine families each have their own independent,
non-shared matching logic: `check_stall` (log posts + awning), the
chimney check inside `has_chimney` (a second, structurally-similar but
code-separate ring scan), and `vice_near` (pure radius proximity, no
shape at all). Four families, four different pieces of matching code.
This is the concrete shape of the gap: no `MultiblockShape` type
anything can be defined *against* — only more Rust functions per shape.

**Recognition is boolean-only; nothing aggregates stats from matched
blocks.** In `check_stack`, the mouth block selects *which machine this
is* (and therefore which recipe table applies) — it is never read as a
stat contributor. There is no path anywhere in this file from "which
specific block occupies this position" to "output scales by X." Pattern
A (rank aggregation) isn't a feature to layer on top of the existing
recognition — the recognition functions themselves would need to change
return type, from pass/fail to "walk matched positions, fold their
properties," since the data to fold currently isn't collected at all.

**Validity is re-checked by polling, not by edit events.** `tick_kilns`
calls `check_kiln` fresh, every tick, for every currently-lit kiln, to
catch a breached stack. Fine at today's scale (a handful of active
machines); it's O(active machines × scan cost) every tick forever, and
won't hold up once player-defined multiblocks number in the hundreds.
The event-driven "recheck only on block place/break near a registered
controller" model (Part 1.2 below) is a scaling requirement, not a
style preference.

**A deeper constraint sits below recognition entirely: per-machine-kind
state.** `block_entities` is a `HashMap<(i32,i32,i32), BlockEntity>` with
a hand-written enum — `Forge`, `Kiln`, `Bloomery`, `Anvil`, `Clamp`. Every
new machine *kind* needs a new enum variant and matching arms throughout
this file, exactly as every new *shape* needs a new function. A generic
slot-frame system (Part 1.3) wants this to become one generic
multiblock-instance variant carrying a shape ID plus a slot-state map,
rather than growing the enum per machine forever. This is arguably the
real refactor underneath the recognition question — worth scoping as its
own line item, not assumed to fall out of adding a shape matcher.

**Worth keeping unmodified in any redesign:** `swap_block_keep_entity` —
swap a block's type while preserving whatever `BlockEntity` lives there
— is exactly the primitive that slot-swapping and lit/unlit machine-state
transitions will keep needing. It's already correctly factored out and
general; no argument for touching it.

| Current (machines.rs) | Proposed (Part 1) |
|---|---|
| `check_stack`, `check_stall`, `has_chimney`, `vice_near` — one bespoke Rust function per shape family | 1.2 Multiblock Recognition — one data-driven shape matcher, shapes described not coded |
| Mouth block picks machine identity only, never contributes to output stats | 1.2/2.1 recognition returns matched blocks for stat folding (Pattern A) |
| `tick_kilns` polls `check_kiln` every tick per lit kiln | Event-driven recheck on `block_place`/`block_break` near a registered controller |
| `BlockEntity` enum grows one variant per machine kind | Generic multiblock-instance variant: shape ID + slot-state map (1.3) |
| No slot concept — a machine's "loadout" is just its charge/fuel/output item slots, not swappable functional modules | 1.3 Slot-Based Modular Frames |
| No mod-facing way to define a new machine shape without writing and compiling Rust | Shape/frame definitions as data (TOML-equivalent), consumed by the generic matcher |

---

## Part 1 — Foundational Primitives

These four ideas recur across nearly every feature below. Building them
generically once, rather than per-feature, is the core efficiency argument
for this whole spec.

### 1.1 Bounded Local-Structure Entity
**What it is:** A block-container independent of the main chunk grid — its
own small block array, its own transform (position + rotation), its own
lightweight collision mesh rebuilt on block change, and its own tick scope.
Static by default (a placed building), but the same primitive supports a
*moving* variant whose transform is driven parametrically along a path
(e.g. progress along a rail spline) rather than free rigid-body physics.

**Explicitly not needed:** a second dimension / sublevel system in the
Minecraft-Create-Sable sense. That approach solves a narrower problem
(letting *unmodified* block/tile-entity code assume it lives in a fixed
global grid) that doesn't apply to an engine built from scratch.

**Enables:** multiblock machines (1.2), block-built train cars (Part 2.2),
piece-based structures (Part 2.3).

### 1.2 Multiblock Recognition
**What it is:** Shape-matching against a set of placed blocks, anchored on
a controller block, rotation/mirror-aware, re-evaluated only on
`on_block_place`/`on_block_break` for the affected structure (not polled).
Partial damage (removing one block) invalidates the match.

**Enables:** multiblock factory machines, both aggregation- and slot-based
(Part 2.1).

### 1.3 Slot-Based Modular Frames
**What it is:** A fixed, authored shape (a "frame") with explicitly named
slots, each with a size class and optional category filter. Each slot
independently holds one **module** chosen from a catalog of things that
fit that slot. Per-instance state (which module is in which slot) persists
with the structure, the same way a chest's contents persist.

Distinct from 1.2: recognition asks "is this a valid X," slots ask "what's
currently installed in this specific position of this X."

**Enables:** modular factory buildings, block-built train car loadouts
(Part 2.1, 2.2).

### 1.4 Capture & Stamp Tooling
**What it is:** Select a built region in-world → save as a reusable
template (blocks + slot loadout, if any). Placing a saved template can be
instant (if materials are available) or a **ghost overlay** the player
fills in by hand — recommended default, since it preserves the logistics
challenge of *getting materials to the site* while removing only the
tedious *arrangement* labor.

**Enables:** fast re-placement of proven factory designs, authoring tool
for piece-based structures (Part 2.3), saving/sharing train car designs.

---

## Part 2 — Composite Systems

### 2.1 Multiblock Factory Machines
Combines 1.2 (recognition) with either:
- **Pattern A — rank aggregation:** unconstrained-shape structure; stats
  are summed/looked-up from whichever specific blocks fill it (e.g. basic
  vs. advanced casings change throughput).
- **Pattern B — slot frame:** fixed frame (1.3) with named functional
  slots; each slot's *module* can itself be rank-tiered.
- **Hybrid (recommended):** Pattern B at the whole-building level for
  legibility ("here's what this building does, here's where I upgrade
  it"), Pattern A within individual slots for incremental tiering.

**Enables:** the core factory-building loop — machines whose function and
output scale with what's actually built, not a fixed recipe-per-entity.

### 2.2 Block-Built Trains & Rail
Combines 1.1 (moving local-structure), 1.3 (slot frame — engine slot,
cargo slots, etc.), 1.2/Pattern A within slots, and 1.4 (save/stamp a car
design). Mass for the power formula below is computed by summing the
car's actual placed blocks, the same mechanism as machine stat
aggregation — not a hardcoded number.

**Rail geometry:** a fixed piece catalog (straight, gentle curve, tight
curve, incline, switch), grid-snapped like belts. The train's own
in-world transform interpolates smoothly between the discrete pieces
(precedent: vanilla-Minecraft-style minecarts), so blocky track doesn't
imply a blocky ride.

**Power draw:** trains and belts share one solution to the same
tension (mass-driven load vs. WildForge's boolean-rate shaft power):
quantized load tiers (Empty/Light/Medium/Heavy/Overloaded) computed each
tick from summed mass, each mapped to a fixed draw rate —
`draw = incline_multiplier(segment/track type) × load_tier_rate(mass)`.
No continuous wattage accounting needed; both lookups are cheap even over
long networks. Optionally, Overloaded can be a real failure state (jam/
stall/visible strain) rather than just a cost tier.

**Scope recommendation:** ship prefab (non-editable) train cars first,
using only 1.1's static-transform-on-rail behavior. Add block-built cars
(full 1.1+1.3+1.4) as a later pass. Within that, restrict slot editing to
stationary/at-station cars initially — editing modules on a moving,
colliding structure is meaningfully harder and not needed for the feature
to be worth having.

**Enables:** the mass-and-incline-driven belt/rail logistics layer this
whole project is seeded from.

### 2.3 Piece-Based Procedural Structures
Uses 1.4 (capture tool) for authoring individual **pieces** (rooms,
corridors, town buildings), each tagged with typed connector points.
**Pools** are weighted lists of interchangeable pieces per connector type.
A **generation walk** assembles a coherent structure from an entry piece
outward, respecting max depth/piece count. **Terrain adaptation** modes
(none / bury / encapsulate) control how a piece meets uneven voxel terrain.

Pieces need spawn markers for NPCs (2.4) and quest-flag-gated features
(locked doors, etc., via 2.5's storage-backed flags) — this system is the
assembly layer the content-layer systems below plug into, not a fourth
unrelated system.

**Enables:** explorable dungeons, NPC towns, any hand-authored-but-
randomly-assembled structure, replacing WildForge's current single-fixed-
template `structures.toml` model for anything beyond simple ruins.

---

## Part 3 — Content-Layer Additions

Mostly independent of Part 1/2; can be built in parallel.

### 3.1 Friendly NPC Primitive
Distinct from `animals.toml`: fixed position or patrol path, an interact
hook, and per-NPC-per-player state (quest stage, relationship, etc.),
rather than the hostile/fauna-oriented fields wildlife currently has.

**Enables:** quest givers, town populations, any non-hostile scripted
character.

### 3.2 Dialogue System
Data-driven branching format (author-supplied — not exposed here in
detail), exposing just evaluation hooks through the existing script API:
condition checks against quest flags, choice-selected callbacks. Kept
data-vs-code-split the same way `recipes.toml` + `on_craft` already are.

**Enables:** quest dialogue, NPC personality/flavor lines, branching
player choices (e.g. faction-relationship decision points).

### 3.3 Quest Tracking
Objective/completion state stored via the existing per-mod
`storage_get`/`storage_set` KV store (already hot-reload- and save-safe);
the new work is authoring format (quest definitions, objectives,
rewards) and a journal/log UI surfacing it to the player.

**Enables:** the full quest reward loop — reputation, blueprints, unique
recipes, map data, NPC recruits.

### 3.4 Settlement Reputation & Tiered Growth
Pre-placed reveal pattern: all growth tiers of a settlement are placed at
worldgen; higher tiers stay hidden/non-collidable until reputation crosses
a threshold. Avoids runtime procedural placement entirely.

**Enables:** settlements that visibly grow with player investment,
without regenerating or streaming new geometry at runtime.

### 3.5 Blueprint-Gated Recipes
Extend `recipes.toml`-equivalent with optional tech-gate and/or
blueprint-item-gate fields (recipe needs tech, a consumable blueprint
item, both, or neither).

**Enables:** dungeon-exclusive recipe unlocks as a reward type, distinct
from the standard tech-tree unlock path.

### 3.6 Enemy Behavior Archetype Library
Extend `animals.toml`-equivalent so an enemy entry references a named
**behavior archetype** implemented in code (state machine), rather than
only the current hostile/attack/movement fields. Needed for: multiple
attacks per enemy with distinct damage/cooldown, resistances/
vulnerabilities by damage type, weak-point-sequence encounters,
hackable-vs-destroy branching, and a "builder" archetype that constructs
things in real time.

**Enables:** tiered enemy variety (wildlife → corrupted → machines →
constructs → bosses) without one-off code per creature.

### 3.7 Generalized Industrial Response
Before building a parallel system: check whether WildForge's existing
**ire** meter (rises with extraction, decays over time, gates escalating
night threat) can be generalized/reused rather than reimplemented — the
desired "bigger factory → more aggressive local wildlife" gradient is
mechanically the same shape.

**Enables:** factory-scale-driven difficulty pressure, reusing proven,
already-playtested logic instead of a duplicate system.

---

## Suggested Build Order

1. Bounded local-structure primitive (1.1) + multiblock recognition (1.2)
   — unlocks static multiblock machines immediately.
2. Slot-based frames (1.3) — layer onto machines as Pattern B/hybrid.
3. Capture/stamp tool (1.4) — pays for itself across machines, dungeons,
   and trains simultaneously once it exists.
4. Friendly NPC primitive (3.1) + dialogue (3.2) + quest tracking (3.3) —
   independent track, can proceed in parallel with 1–3.
5. Piece-based structures (2.3) — depends on 1.4 being done.
6. Prefab (non-block-built) trains + rail (2.2, scoped down) — depends on
   1.1 only.
7. Blueprint-gated recipes (3.5) — per-player recipe gates, the
   `learn_recipe` quest reward, and one base example; layers on the
   Phase 14 settlement/quest infra.
8. Enemy behavior archetype library (3.6) — branches off 3.5's content
   layer; damage types, multi-attack defs, and behavior archetypes.
9. Everything else (3.7, block-built trains) — lower urgency, layer on once
   the above are proven.
