# Finite materials — a planet cannot hide deletion behind the frontier

> **Status: implemented and qualified (2026-08-01).**
>
> This is goal 8 of the planetary-world sequence. It requires completed
> planetary geology and should run after planetary biomes so every material
> source and renewable cycle is known.

## Implementation and qualification record — 2026-08-01

The integrated generator-9 ledger records immutable deposit mass,
materialization reservations, inventories/entities/placed blocks, secondary
stock, explicit sinks, and external/operator adjustments in `materials.wfm`
plus its bounded replay log and backups. The production seed-20260801 audit is
balanced for every tracked material and passes progression qualification:
three bronze-capable regions, 609 pessimistic complete technology arcs, all
seven required carbonate-flux regions, and redundant diamond, pitchblende,
and rare-earth sites. Retrogen policy validation, idempotence, removed-mod
placeholders, crash replay, recovery, salvage, machine, drop/despawn, and
guest/server transfers are covered by the final repository suite. The
aggregate audit completes in under 0.01 seconds on the review host and exposes
no secret coordinates.

## Purpose

On an infinite world, tool wear and lost items silently consume newly
generated ore. On a finite planet, every nonrenewable has an actual upper
bound. That is valuable: mines become places, salvage matters, old workshops
are inherited, and scarcity can support trade. But careless deletion can also
make a mature world impossible for a new player to inhabit.

This goal makes finite material honest and survivable:

- geological manifests bound virgin resources,
- extraction debits known deposits,
- durable goods retain recoverable material,
- wear and dismantling produce scrap rather than deleting all metal,
- sinks are explicit and measured,
- progression has planetary redundancy,
- new worldgen mods have a safe finite-world retrogen path,
- diagnostics can state what remains without revealing secret coordinates to
  ordinary players.

This plan does not make every atom individually serializable. It preserves the
economically meaningful mass classes.

## Material classes

Classify every base material and mod material:

### Renewable

Examples: wood, fiber, food, hides, dung, compost.

- Has an ecological or agricultural reproduction path.
- Renewal rate and habitat are finite.
- A dead heart may suppress natural renewal but player-managed cycles remain.

### Geologically finite

Examples: copper, tin, iron, lead, silver, gold, chromium, rare earths,
diamonds, uranium-bearing minerals, salt deposits.

- Virgin source exists only in the planet manifest.
- Processing and products retain recoverable mass where physically sensible.
- Dispersion and inefficient recovery create real but explicit loss.

### Transformative finite

Examples: stone, sand, clay, glass, ceramics.

- Planetary quantity is enormous but still bounded.
- Building/mining transfers blocks rather than needing an expensive global
  atom ledger.
- Destructive conversions are documented.

### Consumptive

Examples: coal, charcoal, fuels, fluxes, food.

- Consumption is the intended sink.
- Byproducts enter atmosphere, ash, soil, slag, or another named output where
  gameplay uses them.
- No critical progression element may exist only as an unrecoverable fuel.

### Exceptional/magical

Examples: heartwood, living wood, embers, frost shards.

- Source and renewal rules remain tied to the Wild or to the exact arcane
  ledger introduced later by `docs/magic-sequence.md`.
- They do not masquerade as ordinary geology.
- Their lifecycle is documented separately from conservation reports.

## Planetary resource manifest

The geology goal creates deposit sites. This goal completes their accounting.

Each deposit records:

```text
resource family
host geology
grade bands
original recoverable mass
materialized mass
extracted mass
remaining unmaterialized mass
tailings/secondary recovery potential
discovery state
```

Materializing a chunk reserves the exact ore mass represented by its blocks.
Breaking ore transfers that mass into items/tailings and increments extracted
mass. Generating, unloading, or remeshing cannot alter it.

The server audit can sum deposits exactly. Ordinary prospecting reveals
geological evidence and coarse richness, never a debug coordinate or exact
remaining number.

## Material identity

Items made from a single tracked base material carry an internal material-mass
value derived from content definitions:

```text
iron_ingot = canonical iron unit
iron_plate = one iron unit
iron_gear = recipe-defined iron units
steel parts = iron mass + alloy/process identity
```

The system may track integer canonical units rather than grams. Recipes
declare material transformations, yields, recoverable fraction, and intended
loss. Content-graph validation checks balance.

Mixed objects list their recoverable constituents. Wood handles and leather
straps need not be recovered when salvaging a ruined metal tool unless the
recipe explicitly supports it.

## Wear, repair, and salvage

### Tools and armor

- Durability wear does not erase metal continuously.
- A broken metal tool yields a damaged-object or scrap result representing a
  substantial fraction of its metal.
- Repair consumes smaller material/parts and preserves the object.
- Reforging scrap requires the appropriate workshop and fuel.
- Primitive recovery is possible at poor yield; proper forge/machinery
  improves yield.

The exact default recovery target is:

- 75% of principal metal through primitive salvage,
- 90% through an appropriate forge,
- 95% for clean dismantling of an unbroken machine/component.

Remainders become slag/scale or explicit process loss. Balance may tune these
numbers only with census evidence.

### Machines and buildings

- Breaking a machine returns its recoverable components or a dismantling
  bundle.
- Bare-handed destructive breaking may reduce recovery.
- A proper tool and nearby/used work station recover the maximum.
- Chests and inventories still spill contents.
- No machine controller silently deletes rare internal parts.

### Melt loss and slag

Metalworking may create slag, scale, or tailings:

- early processes lose more,
- advanced processes can rework some secondary material,
- no chain exists solely to create component clutter,
- secondary recovery is valuable late-world infrastructure.

## Absolute-loss paths

Audit and decide every deletion path:

- item despawn,
- death drops,
- lava/fire destruction,
- void loss (removed by planet topology),
- worn item breaking,
- crafting remainders,
- mod removal/placeholders,
- admin deletion,
- entity/cargo loss,
- world corruption recovery.

Policy:

- Ordinary dropped items may despawn for performance, but tracked finite
  material transfers to a regional buried/salvage ledger rather than ceasing
  to exist. A later salvage action can recover a fraction.
- Lava may disperse or oxidize metals into a difficult secondary deposit, not
  annihilate them.
- Admin/dev deletion is logged as an explicit sink in the audit.
- Corrupt data is never “balanced” by inventing replacement mass; recovery
  uses backups and reports the gap.

The salvage ledger is coarse and bounded. It must not retain one record per
lost nail forever.

## Bootstrap and planetary redundancy

Every valid new planet guarantees:

- wood/stone/basic food access near viable spawn,
- copper, iron, coal, and basic flux on every major continent,
- at least three independent bronze bootstrap regions globally,
- multiple deposits for every progression-critical regional material,
- at least two treasure sites per treasure category,
- enough total material for a documented number of complete technology arcs
  even under primitive recovery loss.

Spawn selection considers early survival and bronze viability without placing
every regional resource nearby.

The qualification report computes pessimistic progression capacity after
loss. It fails a planet that can be soft-locked by exhausting one tiny site.

## Renewable geological services

Geological deposits do not respawn on human timescales. Renewal comes from:

- recycling,
- salvage,
- trade,
- reopening tailings with better technology,
- finding previously unknown finite deposits.

Meteorites, magical ore regrowth, and tectonic replenishment are out of scope
for the base game. Mods may add explicit events whose material source is
classified and audited.

The follow-on magic sequence may grow exceptional charge-bearing crystal from
persistent seeds by transferring finite ambient Current. That is not ore
respawn: it does not replenish an ordinary geological deposit or create an
ordinary mineral, and its complete charge lifecycle remains in the arcane
ledger.

## Mod worldgen on a finite planet

Infinite-world advice—“walk into new chunks”—does not survive a mapped planet.

At world creation:

- the loaded mod/content set participates in atlas genesis,
- mod deposits receive manifest entries and guarantees,
- the manifest stores the content hash.

When a worldgen mod is added later, it must declare one retrogen policy:

```text
untouched_host_only
secondary_recovery
world_event
no_retrogen
```

### Untouched-host retrogen

- Select deterministic eligible host cells from the immutable atlas.
- Never overwrite player-touched blocks, block entities, structures, or
  existing deposits.
- Unmaterialized chunks reserve normally.
- Materialized untouched host rock may be replaced through an atomic,
  recorded retrogen pass.
- The new manifest records exact added mass and algorithm version.

### Secondary recovery

A mod may add processing for existing tailings/slag without changing terrain.

### World event

A mod may introduce material through an explicit meteor, wild event, trade,
or other authored source. The audit records it as an external addition.

### No retrogen

The content is available only in newly created worlds. UI and logs say so
plainly.

Removing a mod preserves placeholders and its manifest mass. Reinstalling it
restores identity where save-safe.

## Economy and information

The server knows exact reserves; players should not.

- Prospecting exposes host, direction, and qualitative richness.
- Survey cairns preserve public geological knowledge.
- Depleted mines remain physical and historically meaningful.
- Market prices may respond to perceived scarcity.
- No HUD shows “38% of world iron remains.”
- Server operators receive aggregate audit tools for qualification and
  recovery, with permissions protecting deposit coordinates.

## Persistence and audit

Add:

```text
wildforge --material-audit <world>
```

Report by material:

- original virgin manifest mass,
- unmaterialized reserve,
- materialized underground,
- inventories/containers/entities,
- placed blocks/machines,
- scrap/slag/tailings/salvage ledger,
- explicit consumption/loss,
- unexplained delta.

The unexplained delta must be zero for tracked materials in tests.

Manifest and ledger updates are crash-safe and idempotent relative to block
edits. A crash cannot grant both ore block and mined item or delete both.

## Content and registry changes

Content definitions gain optional material accounting:

```toml
material_class = "geologically_finite"
materials = { iron = 1200 }
salvage = { station = "forge", recovery = 0.90 }

loss = { iron = 60 }
byproducts = [{ item = "base:iron_scale", count = 1 }]
```

One ordinary ingot is 1,200 integer canonical units. Recipes, smelts, and
station work accept `loss`; recipes accept item/count `byproducts`, smelts
accept a single item/count `spit`, and kilns consuming a finite powder must
declare `consumes = true`. The remaining schema rules are:

- mods use the same system,
- recipes validate input/output accounting,
- intentional loss is declared,
- tags cannot duplicate material by substituting members with unequal mass,
- hot reload cannot change material mass in live stacks without a migration
  decision.

## Required tests

### Conservation

- Every tracked base recipe balances declared input, output, and loss.
- Mining transfers exact mass from manifest/block to item/tailings.
- Repair, breakage, salvage, smelting, and dismantling balance.
- Item despawn transfers to bounded salvage accounting.
- Save/load and crash-replay do not duplicate or lose mass.
- Mod removal/reinstall preserves accounted mass.

### Planet viability

- Seed suite satisfies bootstrap and redundant-deposit guarantees.
- Pessimistic primitive-loss simulation still permits the required number of
  full technology arcs.
- No progression-critical resource has a single point of planetary failure.
- Geodesic scarcity bands remain compatible with the economy plan.

### Retrogen

- Untouched host gains deterministic finite deposits.
- Player-touched terrain and structures remain byte-identical.
- Re-running retrogen is idempotent.
- Content-hash and manifest versions persist.
- Every policy reports clearly to host and client.

### Performance and bounds

- Salvage ledger cardinality is bounded by region/material class.
- Audits complete without loading every voxel chunk.
- Ordinary mining/crafting adds constant bounded accounting work.

## Completion criteria

This goal is complete when:

- geological resources have finite manifests and exact extraction accounting,
- metal tools, armor, machines, and components have repair/salvage paths,
- all absolute-loss paths are explicit,
- planetary bootstrap and redundancy are proven,
- post-creation mods have safe declared retrogen behavior,
- operator audits reconcile every tracked material with zero unexplained
  delta,
- registry/mod documentation ships,
- conservation, viability, retrogen, persistence, crash, and budget tests
  pass.

Do not mark this implemented because ore locations are finite. Finitude is
playable only when the material that leaves a mine continues to have an
accountable life.

## Implementation and verification record

Verified 2026-08-01 against the complete repository state for this goal:

- `src/materials.rs` owns the finite deposit manifest, exact material
  compartments, bounded regional salvage, checksummed delta log, recovery,
  retrogen records, qualification proof, and operator audit.
- `src/registry.rs` loads material classes and vectors, enforces balanced
  recipes/tags/processes, generates the 75/90/95-percent recovery paths, and
  preserves the saved material identity of removed or changed mod content.
- `src/world/mod.rs`, `src/world/chunks.rs`, and the machine, persistence,
  entity, calendar, game, and multiplayer paths journal extraction,
  placement, processing, wear, repair, dismantling, food consumption,
  spoilage, death/cargo delivery, dropped-item retirement, lava loss, and
  explicit admin/development sources and sinks.
- Production seed qualification proves bootstrap redundancy, flux coverage,
  treasure sites, and pessimistic complete-technology capacity. Retrogen
  tests prove host-only deterministic reservation, touched/structure safety,
  idempotence, persistence, and player/operator notices.
- The shipped player/operator workflow is documented in `README.md`; the
  content and mod-author schema, policies, and recovery rules are documented
  in `mods/README.md`.
- `cargo fmt --all -- --check`, strict all-target Clippy, the locked serial
  all-target test suite (506 passed, 13 intentionally ignored, 0 failed),
  `cargo deny check advisories`, and the locked release build all pass.
- A release build was run through the real game on a freshly generated
  production planet with the steelworks capture fixture. Its offline
  `--material-audit` reported zero unexplained deltas, named every development
  source, and passed planetary qualification with 609 pessimistic technology
  arcs against a requirement of 16.
