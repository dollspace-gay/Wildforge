# Magical foundations — the Current has a ledger

> **Status: implementation design complete, not implemented.**
>
> This is goal 1 of `docs/magic-sequence.md`. It requires a completed and
> qualified `docs/planetary-world-sequence.md`.

## Purpose

Before Wildforge adds a wand, it needs one authoritative answer to where magic
comes from and where it goes. This goal creates the units, reservoirs,
transfer operations, persistence, diagnostics, and extension contracts for
the Current, charge, and dross. It adds no usable player working.

The foundation prevents later content from inventing power in a recipe,
deleting it on item despawn, duplicating it across a crash, or hiding an
ordinary material conjuration behind a particle effect.

## Fixed conceptual model

The Current is a finite supernatural constituent of the planet. It can exist
in six reservoirs:

```text
Deep       inaccessible planetary reserve; exchanges slowly with the surface
Ambient    locally available Current in an atlas cell
Bound      charge held by an item, block, organism, heart, or entity
Active     charge temporarily held by a running working or ritual
Dross      disordered Current in air, water, soil, apparatus, or storage
Scar       dross embodied in a manifested growth, entity, or anomaly
```

The world manifest records the genesis total. Every operation is a transfer
between reservoirs. Natural recovery changes Dross into Ambient Current;
magic use usually changes Ambient or Bound Current into Active and then
returns some Ambient while producing Dross. No tick path changes the total.

This is a gameplay ledger, not a claim to simulate real thermodynamics.

## Units and arithmetic

Use a newtype around a fixed-width integer, provisionally:

```rust
struct CurrentUnits(u64);
```

Implementation may choose a signed delta type and a wider accumulator, but:

- persisted reservoir values are nonnegative integers,
- all multiplication by rates uses documented fixed-point rounding,
- the remainder is retained in a per-process accumulator rather than lost,
- global sums use checked arithmetic wide enough for the whole planet,
- overflow, underflow, NaN conversion, or negative results reject the
  transaction and report a diagnostic,
- changing the unit scale is a format migration.

Floating point may drive visuals and display estimates. It may not own the
ledger.

## Resonance

Current has one conserved scalar quantity and a normalized, low-dimensional
resonance mixture. The initial six resonances are:

| Resonance | Affinity |
|---|---|
| Root | living growth, repair, organic cycles |
| Tide | water, dissolution, circulation |
| Ember | heat, ignition, rapid change |
| Stone | structure, stability, pressure |
| Gale | motion, air, weather |
| Echo | perception, memory, traces |

These are affinities, not inventories of aspect points assigned to every
block. Most systems care about scalar Current plus one or two dominant
resonances. Mixing and transformation preserve scalar units and use
deterministic integer weights.

Base content starts with these six. Mods may add resonances only through a
versioned registry whose saved string identities survive removal. The base
game does not grow a combinatorial tree of compound aspects during this arc.

## Ownership and identities

Every non-ambient reservoir has a stable owner:

```text
planet/cell id
block position + block-entity generation
item instance id
mob/warden stable id
player profile id
active-working id
scar instance id
regional bounded-loss bucket
```

Charge-bearing items therefore require stable instance state rather than a
definition-only stack. Rules:

- stacks combine only when charge, dross, resonance, provenance class, and
  other merge-relevant state are identical,
- splitting a stack divides integer quantities exactly and retains remainder
  deterministically,
- crafting consumes instance state through the transaction API,
- unknown/mod-removed items retain opaque charge state in their placeholder,
- no client-supplied instance id is trusted,
- ids are unique across save/load and crash replay.

## Atomic transfer API

Provide one server-side operation for all arcane state changes:

```text
ArcaneTransaction
  read-set with expected versions
  debit entries
  credit entries
  resonance transforms
  attribution
  reason/content id
  optional linked material/water/ecology transaction
```

Commit is all-or-nothing. It:

1. validates owners, versions, permissions, bounds, and sufficient balances,
2. validates equal scalar debit and credit totals,
3. validates any allowed resonance transformation,
4. commits linked world/material/water changes atomically or not at all,
5. appends a bounded audit event,
6. emits authoritative deltas.

Retries are idempotent by transaction id. A crash cannot award a charged item
while leaving its source undebited.

No ordinary script API exposes raw `add_current`, `spawn_charged_item`, or
unaccounted setters. Debug mutation exists only behind explicit dev/operator
commands and records an external adjustment visible in the audit.

## Planet and regional storage

Extend the qualified planet manifest with:

```text
arcane schema and algorithm versions
Current unit scale
genesis total
reservoir totals at last clean save
content/resonance registry hash
checksums for immutable and dynamic arcane layers
```

Arcane atlas state is stored separately from immutable geography and from
high-frequency local chunks. Sparse item/block/entity state remains with its
own authoritative save owner and participates in the audit.

Use atomic replacement, checksums, and a journal or equivalent commit protocol
for changes that cross atlas and chunk/player files. Recovery either replays a
complete transaction once or rolls it back; it never guesses replacement
Current.

## Bounded loss and unloaded objects

Current cannot vanish through:

- item despawn,
- death drops,
- fire or lava,
- entity despawn,
- chunk unload,
- container destruction,
- mod removal,
- malformed network packets,
- server restart,
- admin deletion.

Where a physical item is legitimately destroyed, its charge transfers to a
regional bounded reservoir as Ambient Current or Dross according to the
content rule. The ledger stores aggregates by atlas region, resonance class,
and disposition, not one immortal record per lost shard.

Warden daylight dissolution and other temporary supernatural entities return
their bound charge through this same path. Killed wardens transfer a declared
fraction into drops and return or disorder the rest.

## Ire separation

The Current API must not read or write Ire as an accounting shortcut.

- Ire can influence what a heart or warden chooses to do.
- The resulting manifestation still requires an arcane transaction.
- Dross can damage wild habitat; that independently invokes existing Ire
  rules for the damaging act.
- Cleaning dross can count as tending through an explicit capped stewardship
  action, never because dross numerically became Ire.

Add a headless four-state fixture:

```text
calm + clean
calm + polluted
angry + clean
angry + polluted
```

Each state must have distinct readings and behavior. Mutating one axis must
not mutate the stored value of the other.

## Heart and warden accounting contract

Hearts receive bound regional reserves during planet genesis. They may:

- lend charge to wardens and recover it on dissolution,
- transfer charge into exceptional natural growth,
- reorder Dross into Ambient Current at a rate defined in goal 8,
- accept charged offerings,
- move Current among their country's reservoirs.

They may not create Current. Heart death freezes or releases its remaining
reserve according to an explicit transaction and never deletes it.

Existing warden drops become charged exceptional items. Their material item
identity remains, but their supernatural potency comes from stored charge.
Normal repeated encounters must recycle most manifestation charge so ordinary
play does not accidentally drain a country. Extracting large amounts through
deliberate hunting is allowed, visible, finite, and included in later balance
qualification.

## Content and mod contracts

Add declarative definitions sufficient for later goals:

```toml
[arcane]
bound_capacity = 120
conductivity = 0.40
stability = 0.85
resonance = { root = 3, stone = 1 }
on_destroy = "ambient"
```

Exact schema is implementation-owned. Validation requires:

- nonnegative bounded values,
- known string resonance ids,
- an explicit destruction disposition for charged content,
- no recipe or script event that credits more Current than it debits,
- deterministic merge/split behavior,
- stable fallback for removed mods.

Rhai receives read-only local estimates and transaction-building operations
with per-event budgets. It cannot mutate the global atlas or ledger directly.
Later effect APIs must build on this gate.

## Multiplayer and agents

The host sends only the local information a player can legitimately perceive:

- coarse environmental cues available without instruments,
- tuning-lens readings after goal 4,
- charge state for items the player can inspect,
- nearby working and manifestation events.

The wire never sends a global Current map, exact heart reserves, or operator
audit. Remote actions name an intended instrument/target; the host validates
range, knowledge, inventory, balance, rate, and transaction.

Agent APIs eventually expose the same observations and verbs as a player.
This goal adds typed placeholders/versioning but no omniscient query.

## Operator diagnostics

Add:

```text
wildforge --arcane-audit <world>
```

The report includes:

- genesis total,
- total by reservoir,
- total by resonance,
- ambient/deep atlas checksums,
- bound totals by owner class,
- dross/scar totals,
- external admin/dev adjustments,
- unexplained delta.

Tests require unexplained delta zero. Ordinary players never see this report.

Provide debug-only commands or fixtures to transfer Current among named
reservoirs, force save boundaries, and corrupt a disposable test copy. Every
debug adjustment is visibly marked and cannot be invoked through multiplayer
gameplay.

## Performance budgets

- An ordinary no-magic server tick performs no scan over all charged objects.
- Per-transaction work is bounded by its declared read/write set.
- Coarse atlas processing belongs to goal 2 and does not materialize chunks.
- Audit sums atlas files plus bounded owner indexes; it does not load every
  voxel chunk.
- Wire state is interest-managed and delta-compressed.
- Audit/event history has a bounded aggregation strategy.

Implementation records measured memory, save size, audit time, transaction
latency, and idle-tick cost on the default planet.

## Required tests

### Conservation

- Every reservoir-to-reservoir transfer reconciles exactly.
- A large randomized sequence of bind, split, merge, activate, disorder,
  cleanse, destroy, lose, recover, and save operations retains the genesis
  total.
- Rounding remainders are retained across ticks and save/load.
- Overflow and insufficient-balance attempts fail without partial mutation.
- Resonance transformations conserve scalar units.

### Persistence and crash safety

- Every charge-bearing owner round-trips.
- Retrying one transaction id cannot duplicate its effect.
- Simulated crashes at each multi-file commit boundary recover exactly once.
- Unknown content preserves opaque charge.
- Item despawn and entity dissolution transfer rather than delete.
- Corruption reports a gap and refuses unsafe invention.

### Authority and extension

- A hostile client cannot forge charge, ids, readings, or transactions.
- Windowed solo, host, dedicated server, and loopback guest produce identical
  authoritative results.
- Fixture mods can declare valid charge behavior.
- Unbalanced, unknown-resonance, raw-mutation, and overflow fixture mods fail
  with actionable errors.
- Agent placeholders reveal no global state.

### Separation

- The four Ire/dross states are independently constructible and persisted.
- Dross-changing transactions do not directly change Ire.
- Ire changes do not alter the arcane total.
- Heart/warden spawn, dissolution, death, drop, and heart-death paths balance.

## Completion criteria

This goal is complete when:

- every unit of planetary Current has a typed reservoir and stable owner,
- the fixed-point global ledger reconciles with zero unexplained delta,
- all arcane mutation goes through an atomic authoritative transaction,
- save, crash, loss, mod-removal, entity, and warden paths conserve Current,
- Ire and dross are provably independent,
- registry, script, multiplayer, agent, and operator foundations exist,
- required tests and repository gates pass,
- the implementation record contains measured budgets and format versions.

Do not add a player-cast working to demonstrate the foundation. Goal 1 proves
that later magic cannot cheat; goal 6 makes it useful.
