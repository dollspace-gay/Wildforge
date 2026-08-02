# The Current — master magic design and `/goal` execution sequence

> **Status: goals 1–4 implemented and production-qualified; goals 5–9 remain.**
>
> This document is the index, dependency contract, vocabulary, and final
> definition of done for player magic in Wildforge. Run it only after
> `docs/planetary-world-sequence.md` is implemented and qualified. Do not
> implement the arc as one undifferentiated change. Run the nine goal
> documents below, in order.

## North star

Magic is already true in Wildforge. Charms survive in ruins, hearts embody
countries, the Wild remembers conduct, and wardens are temporary supernatural
beings. The missing fact is that players cannot yet learn what the old makers
knew.

The arc makes that knowledge playable without allowing magic to escape the
world's physical and social stakes:

> **Magic is the dangerous art of renegotiating processes that already
> exist.**

Magic may reveal, bind, move, accelerate, preserve, stabilize, or redirect.
It does not create ordinary matter, turn one element into another, teleport,
replace logistics, or provide a creative inventory disguised as lore.

Like land, water, ore, and trust, magical power is a commons. A group may
survey it, cultivate it, monopolize it, pollute it, restore it, legislate it,
hide what it did to it, or inherit the consequences of someone else's work.
That is how magic serves Wildforge's larger promise:

> **We came to mine blocks. We ended up founding a country.**

## Settled vocabulary

The design deliberately uses its own system and visual language. Generic
development terms remain useful in code and discussion, but the preferred
player-facing vocabulary is:

| Meaning | Wildforge term |
|---|---|
| Ambient magical medium / mana | **the Current** |
| Current stored in an item, organism, entity, or effect | **charge** |
| Broad affinity of Current | **resonance** |
| Disordered magical waste | **dross** |
| Local magical pollution made physically manifest | **scarring** |
| A shaped magical effect / spell | **working** |
| A high-capacity or intersecting magical place | **confluence** |
| A naturally weak or insulating magical place | **still** |
| A player instrument for reading the Current | **tuning lens** |
| A potion or other stored consumable effect | **preparation** |

These names are a design baseline, not a substitute for originality review.
Content must also have original silhouettes, palettes, ecological roles,
crafting logic, progression, prose, audio, and failure behavior. Do not ship
recognizable copies of another game's rainbow crystals, signature trees,
scanners, research puzzles, corruption palette, creatures, or interfaces
under replacement names.

## The three systems that must remain distinct

### Ire is relational

Ire records conduct toward the Wild. It remembers taking and tending. Hearts
and wardens respond with agency. Intent, attribution, and reciprocity matter.

### The Current is environmental

The Current is a conserved planetary state. It has location, capacity,
conductivity, resonance, and reservoirs. It moves and changes form whether or
not anyone deserves the outcome.

### Dross is waste

Dross is the disordered state produced by inefficient, unstable, or
overdrawn magic. It follows deterministic physical rules. A careful tyrant
can keep a clean laboratory; a generous novice can poison a watershed.

All four combinations of calm/angry Wild and clean/polluted Current must be
representable, testable, and perceptibly different. Dross may harm living
country and thereby provoke Ire, but the meters are never aliases and one is
never directly derived from the other.

## Constitutional laws

Every goal preserves these laws.

### No ordinary matter from magic

Every magical operation must answer:

1. What existing physical state, matter, organism, or relationship changes?
2. From which named reservoir does its Current come?
3. Where do its charge and dross go when it ends?

Forbidden base-game outcomes include:

- creating blocks or items without an accounted material source,
- converting coal to diamond, lead to gold, or one ordinary material identity
  into another,
- duplicating inventories or fluids,
- infinite water, fuel, food, repair, healing, or crop growth,
- teleportation,
- permanent free flight,
- bypassing tool tiers, workshops, transport, or finite-resource manifests
  through a magical synonym.

Magical crystal growth is not an exception hidden in wording. A crystal is an
exceptional reservoir: a compatible seed and host gather a precisely
accounted quantity of the Current into charge. Harvesting transfers that
charge; using, losing, dissolving, or contaminating it transfers the same
ledger quantity elsewhere.

### One arcane ledger

For every world:

```text
total Current =
    deep reserve
  + ambient Current
  + bound charge
  + active workings
  + dross
  + manifested scarring
```

All quantities use fixed-point units. Transfers reconcile exactly. Hearts,
plants, weather, and time may move or reorder Current and dross; they never
silently create or delete either.

### One physical world

- Growth still consumes water, nutrients, substrate, and time.
- Healing still consumes the body's food reserve and appropriate reagents.
- Repair still consumes matching material or parts.
- Fire still needs something combustible after magical ignition.
- Weather workings redistribute existing heat, air, and water.
- Liquid preparations debit the planetary water ledger.
- Durable physical changes remain after a working ends only when their
  ordinary material and energy obligations were paid.

### One living world

- Magical richness overlays climate, geology, soil, water, and ecology; it
  does not replace them.
- A confluence in a desert, reef, tundra, mountain, or rainforest has a
  climate-correct supernatural ecology.
- Hearts accelerate reordering and recovery but do not manufacture Current.
- The Wild never rewrites player-built blocks.
- Scarring may occupy empty space, attach removable growth, foul magical
  apparatus, and threaten inhabitants; it never silently replace or delete
  construction.

### One authority

- The host owns Current, dross, research, working validation, world edits,
  effects, and attribution.
- Solo, listen server, dedicated host, remote guest, and headless agent use
  the same simulation operations.
- Clients may predict presentation but never award charge, ingredients,
  knowledge, or outcomes.
- Mods use the same conservation and effect transaction APIs as base content.

### Legible, attributable, recoverable

- Dangerous practice gives warnings before regional catastrophe.
- Instruments report uncertainty instead of fake precision.
- Every damaging threshold has a prevention and recovery path.
- Dross generation records installation and actor contributions for
  diagnostics and social evidence.
- Recovery is work, not a one-click universal cleanser.
- No normal accident permanently removes access to the core magic progression
  from a finite planet.

## Dependency graph

```text
qualified finite planet
        │
1. magical foundations and conservation
        │
2. planetary Current and places
        │
3. magical ecology and resources
        │
4. discovery and shared knowledge
        │
5. charms, wands, and apparatus
        │
6. workings and rituals
        │
7. preparations and alchemy
        │
8. dross, scarring, and remediation
        │
9. integrated magic qualification
```

The order is causal:

- geography cannot distribute a field whose units and ledger do not exist,
- magical organisms need finalized climate, habitat, and Current layers,
- research needs phenomena worth observing,
- implements need discovered materials and knowledge,
- workings need implements and atomic accounting,
- preparations need effects, ingredients, and bound-charge containers,
- dangerous pollution needs all real production paths before it can be tuned,
- qualification needs the complete social and technical loop.

## How to run the sequence

Complete and qualify `docs/planetary-world-sequence.md` first. Then run one
command/prompt at a time. The intended interaction is exactly:

```text
/goal complete all features in the design document docs/magic-foundations-plan.md
```

After that goal is genuinely complete and verified, run the next line.

### Goal 1

```text
/goal complete all features in the design document docs/magic-foundations-plan.md
```

Produces:

- exact Current/charge/dross units and reservoirs,
- atomic arcane transfers and audits,
- server, persistence, wire, agent, and mod foundations,
- explicit separation from Ire and ordinary material accounting,
- deterministic headless simulation with no player magic yet.

### Goal 2

```text
/goal complete all features in the design document docs/magic-geography-plan.md
```

Produces:

- planet-wide capacity, conductivity, resonance, and deep-reserve layers,
- conservative Current and dross transport across every face seam,
- confluences, stills, wakes, echoes, heartshadows, and scars as places,
- climate- and geology-caused magical geography,
- atlas exports, surveys, and long-run equilibrium tests.

### Goal 3

```text
/goal complete all features in the design document docs/magic-ecology-plan.md
```

Produces:

- magical plants and minerals with actual cycle roles,
- climate-correct confluence ecologies instead of one universal magic biome,
- growing charge-bearing crystals with exact accounting,
- renewable habitat cycles and finite geological magical resources,
- warden, heart, water, soil, fire, and biome integration.

### Goal 4 — implemented and production-qualified 2026-08-02

```text
/goal complete all features in the design document docs/magic-discovery-plan.md
```

Produces:

- an archaeological entry into player magic,
- the tuning lens and field observation,
- experiment-based discovery rather than scan grinding,
- copyable physical records and settlement libraries,
- shared multiplayer knowledge with no permanent class locks.

### Goal 5

```text
/goal complete all features in the design document docs/magic-implements-plan.md
```

Produces:

- craftable versions of the existing ruin charms,
- component-built wands with different material behavior,
- charge vessels, foci, safe containment, and embodied workshop apparatus,
- one charm slot and no linear best-item ladder,
- complete durability, salvage, wire, and mod behavior.

### Goal 6

```text
/goal complete all features in the design document docs/magic-workings-plan.md
```

Produces:

- useful sensing, binding, moving, accelerating, preserving, and stabilizing
  workings,
- efficient constructed rituals and flexible wand use,
- atomic Current/material/water/ecology effect transactions,
- wards with continuous costs rather than absolute safety,
- hard guards against conjuration, transmutation, teleportation, and
  logistics bypass.

### Goal 7

```text
/goal complete all features in the design document docs/magic-alchemy-plan.md
```

Produces:

- physical brewing, extraction, distillation, and infusion,
- reusable vessels and finite solvents,
- preparations carrying ingredients and bound charge,
- healing, perception, cultivation, resistance, and remediation preparations
  with bodily and ecological costs,
- residues, spoilage where useful, dosage, and server-authoritative effects.

### Goal 8

```text
/goal complete all features in the design document docs/magic-dross-plan.md
```

Produces:

- complete dross generation from every magical practice,
- staged, visible, deterministic scarring,
- magical weather, organisms, hazards, and breaches without block
  transmutation,
- transport through air and water without violating either ledger,
- containment, treatment, recovery, attribution, and social evidence.

### Goal 9

```text
/goal complete all features in the design document docs/magic-qualification-plan.md
```

Produces:

- requirement-by-requirement audit of goals 1–8,
- exact arcane, material, and water reconciliation,
- long simulation and abuse scenarios,
- multiplayer, mod, save, crash, agent, and performance qualification,
- final visual, audio, naming, documentation, and progression review,
- removal of every unaccounted or placeholder shipping path.

## Goal-run contract

Every implementation goal must:

1. Read this master document, its target document, and every prerequisite.
2. Inspect current code and implementation records instead of assuming an
   earlier goal matched its draft exactly.
3. Derive a checklist from every explicit requirement, artifact, invariant,
   command, test, content entry, and completion criterion in the target.
4. Implement the complete target across persistence, multiplayer, headless,
   agents, mods, UI, diagnostics, visuals, audio, and documentation.
5. Use fixed-point authoritative accounting for conserved planetary state.
6. Keep worldgen and coarse simulation deterministic and independent of
   chunk-load order.
7. Preserve established Wildforge behavior except where this sequence
   explicitly supersedes it.
8. Run tests proportionate to intermediate risk and all required gates before
   completion.
9. Prepend an implementation record to the target document containing:
   - implementation date,
   - notes versus specification,
   - format/protocol/content versions,
   - measured budgets,
   - verification evidence,
   - honest remaining limitations.
10. Mark the target implemented only after every completion criterion has
    authoritative evidence, then stop before the next goal.

Compilation, one attractive screenshot, a debug-only prototype, or a narrow
green test is not completion.

## Requirement ownership matrix

| Requirement | Owning goal | Final proof |
|---|---|---|
| Conserved finite planetary Current | Foundations | Exact world audit and transfer tests |
| Ire and dross are independent | Foundations | Four-state scenario matrix |
| Geography controls magical distribution | Geography | Global maps and relational tests |
| Closed seam-safe transport | Geography | Face-edge/corner and equilibrium suite |
| Climate-correct magical places | Geography + ecology | Cross-climate confluence census |
| Magical plants have cycle functions | Ecology | Growth/harvest/recovery ledgers |
| Magical crystals debit ambient Current | Ecology | Exact crystal lifecycle tests |
| Magical minerals respect finite geology | Ecology | Resource manifest audit |
| Archaeology unlocks reproducibly | Discovery | Seed-suite progression proof |
| Knowledge is observable and shareable | Discovery | Multiplayer library scenarios |
| Existing charms become craftable | Implements | Full recipe/charge/effect lifecycle |
| Wands differ by components, not tiers | Implements | Component behavior matrix |
| Workings cannot create/transmute matter | Workings | Forbidden-effect and conservation tests |
| Growth/water/repair pay physical costs | Workings | Cross-ledger transaction tests |
| Preparations use finite ingredients/solvent | Alchemy | Batch mass/water/charge reconciliation |
| Pollution warns before catastrophe | Dross | Threshold and perception scenarios |
| Scarring does not rewrite construction | Dross | Touched/build preservation tests |
| Pollution is treatable but not trivial | Dross | Prevention and remediation campaigns |
| Server authority and mod parity | All; qualification | Hostile-client and fixture-mod suite |
| Entire arc is coherent and original | Qualification | Full audit, capture set, content review |

No requirement is considered shared without one goal owning its implementation
and goal 9 proving the integration.

## Existing systems and supersession

| Existing document/system | Magic arc treatment |
|---|---|
| `hostiles-plan.md` | Keeps Ire and warden purpose; accounts for charge bound in manifestations and drops |
| `hearts-plan.md` | Keeps hearts physical and Long Winter; adds rate-limited dross reordering without matter creation |
| `roadmap-plan.md` ruins/charms | Keeps salvage entry and one charm slot; adds makers, discovery, and craftable descendants |
| `planet-atlas-plan.md` | Adds versioned arcane genesis/dynamic/history layers after planetary qualification |
| `planetary-biomes-plan.md` | Adds magical richness as an overlay, never a replacement climate biome |
| `planetary-water-cycle-plan.md` | Carries dissolved/suspended dross without creating or deleting water |
| `finite-materials-plan.md` | Gives exceptional magical matter its own exact ledger and connects finite magical minerals to manifests |
| `modding-plan.md` | Extends registries and scripts through safe declarative magical APIs; scripts cannot bypass accounting |
| `multiplayer-plan.md` | Adds authoritative knowledge, item state, effects, apparatus, and regional field deltas |
| `agent-mcp-plan.md` | Adds ordinary perceptible readings and actions, never omniscient debug access |

## Scope guards

This sequence does not include:

- teleportation or portals,
- dimensions, planes, or off-planet realms,
- player classes, levels, skill points, or permanent magic-user locks,
- hundreds of combinatorial aspects,
- procedural incantation text treated as content,
- secret recipes that one player can permanently deny the server,
- formal law, land-claim, faction, or government mechanics,
- magical automation that obsoletes transport or mechanization,
- a spell for every physical verb,
- bosses required for basic magic,
- irreversible whole-planet contamination as a normal accident.

Those may be reconsidered only in a later design that preserves the
constitutional laws.

## Repository gates

Every goal keeps targeted tests green. Goals with format, protocol, or broad
simulation changes run:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
cargo deny check advisories
```

The final qualification goal also runs every arcane audit, seed census,
seam/corner, long-run, multiplayer, mod, crash-replay, scenario, performance,
and visual validator introduced by goals 1–8.

## Sequence completion

The arc is complete only when goal 9 proves:

- players can progress from a found old charm to observing and practicing
  magic without a debug command or lucky unique drop,
- magical geography and ecology arise from the same finite planet as ordinary
  climate, water, soil, and geology,
- every crystal, wand, charm, working, preparation, warden manifestation,
  dross plume, and scar reconciles in the arcane ledger,
- ordinary matter, tracked material, water, and ecological inputs also
  reconcile,
- careless magic produces legible, attributable, serious consequences,
- careful prevention and costly recovery both work,
- construction remains permanent,
- multiplayer, dedicated hosting, agents, and mods use one authoritative path,
- magic enriches industry, travel, ecology, archaeology, and politics without
  replacing any of them.

Until then, individual goals may be implemented, but player magic as a whole
does not ship.
