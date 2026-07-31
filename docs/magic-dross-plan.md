# Dross and scarring — power leaves a mess

> **Status: implementation design complete, not implemented.**
>
> This is goal 8 of `docs/magic-sequence.md`. It requires goals 1–7 and the
> qualified atmosphere, water, soil, ecology, heart, Ire, fire, persistence,
> multiplayer, and moderation systems.

## Purpose

Dross turns careless magic from a private efficiency loss into a shared
environmental consequence. It is predictable waste, not a morality meter and
not random purple corruption.

This goal connects every real magical production path to transport, warning,
manifestation, harm, attribution, containment, treatment, and recovery.
Ordinary use can remain clean enough for a homestead. A badly run laboratory
can scar a valley. Planetary catastrophe requires sustained planetary abuse,
not one novice mistake.

## Dross is not Ire

The separation from goal 1 remains visible:

- Dross responds to throughput, stability, containment, transport, and time.
- Ire responds to what players take, damage, tend, and return.
- Producing dross inside a sealed vessel does not itself raise Ire.
- Releasing it into habitat may damage life; that harmful act raises Ire
  through normal provenance-aware ecology rules.
- Cleaning an old anonymous scar does not erase someone's Ire, though the work
  may count as capped tending.
- A calm country can be polluted and an angry country can be clean.

Hearts may choose responses based on Ire and may participate in dross recovery,
but neither number is calculated from the other.

## Dross compartments

The one conserved Dross reservoir has location/carrier states:

```text
airborne
dissolved/suspended in water
bound to soil/sediment
held in organisms
contained in items/vessels/filters
embedded in apparatus
manifested as scar growth/entity/anomaly
```

Transfers between these states debit and credit equal Current units. The
carrier matters:

- airborne dross diffuses and advects with weather,
- waterborne dross follows exact water movement and watershed routing,
- evaporation leaves nonvolatile dross behind unless a content process
  explicitly aerosolizes it,
- freezing and thawing partition it by declared coefficients without gain,
- soil/sediment sorbs it and erosion can move it,
- organisms sequester rather than erase it,
- contained dross leaks only through capacity/damage rules,
- manifestations remain ledger owners until removed or transformed.

No chunk-local contamination number duplicates atlas dross. Fine state is a
materialization of owned coarse/local reservoirs with reconciliation.

## Sources

Goal 8 audits and connects every source:

- wand and ritual inefficiency,
- resonance mismatch,
- Ambient Current overdraw,
- damaged, dirty, or overfilled apparatus,
- charm leakage and forced activation,
- crystal overcharge/destructive harvest,
- charged organism/mineral processing,
- alchemy heat, vapor, wastewater, and spent filters,
- warden death and unrecovered manifestation charge,
- interrupted/crashed workings,
- scar/breach activity,
- deliberate discharge and transport.

Every source has:

```text
amount and resonance
carrier/location
actor id if known
installation/source id
transaction/provenance class
warning threshold
prevention path
```

There is no generic percentage secretly discarded after the ledger closes.
Dross is the credited destination.

## Burden and thresholds

Define local burden as a documented function of mobile/retained dross,
physical cell area, retention, stability, water/soil carrier, and current
capacity. The initial threshold bands are:

| Relative burden | State |
|---:|---|
| `< 0.10` | clear |
| `0.10–0.25` | trace |
| `0.25–0.50` | strained |
| `0.50–0.75` | seep |
| `0.75–1.00` | scar |
| `>= 1.00` | breach risk |

Implementation may tune numeric bands only with recorded scenario evidence.
Transitions use hysteresis so a cell does not flicker at boundaries.

Thresholds control eligible deterministic processes, not a one-time random
catastrophe roll. Manifestation work is budgeted and selects stable canonical
sites.

## Warning ladder

### Trace

- Tuning lens and sensitive indicator species detect it.
- Charged glass shows internal haze or asymmetry.
- No direct hazard.

### Strained

- Charms hum or pulse under use.
- Conductor/vessel sound becomes discordant.
- Magical plants close, lean away, or change growth.
- Recharge and workings produce more dross through the real stability
  calculation.
- A player can identify the problem before damage.

### Seep

- Removable filament, soot-like branching, mineral bloom, oily refraction, or
  other climate/material-specific signs occupy empty space and natural
  surfaces.
- Water and soil samples read fouled.
- Long exposure applies bounded symptoms.
- Sensitive plants stall or sequester burden.

### Scar

- Persistent scar growths claim empty cells and attach removable overlays to
  surfaces.
- Magical organisms may become malformed, dormant, or dangerous.
- Scar entities and local arcane weather become possible.
- Cultivated plants may wilt/stall; structural construction remains.
- Apparatus leakage/failure pressure rises.

### Breach

- A local field discontinuity releases stored pressure as a storm, shear,
  animated castoff, or rapid redistribution.
- It is not a portal and does not connect another dimension.
- It transfers existing Dross/Current among explicit owners.
- Forecast cues persist long enough for evacuation or emergency containment.
- One breach is regional, not an automatic self-replicating planet death.

Color is never the only warning, and no stage uses the same presentation as
Ire.

## Manifestation rules

Scarring may:

- occupy air/replace an earlier scar manifestation,
- attach a removable nonstructural overlay to surfaces,
- colonize natural soil, sediment, vegetation, ice, and water margins through
  declared ecological state,
- wilt or stall living cultivated blocks without deleting them,
- foul magical blocks/apparatus through explicit burden state,
- obstruct paths and sight,
- emit hazards, entities, and local magical weather,
- change the Current's local conductivity/stability.

Scarring may not:

- replace, delete, or transmute player-built structural blocks,
- turn ordinary ore into magical ore or vice versa,
- duplicate/delete water or material,
- alter container inventories,
- open portals or teleport,
- silently cross a sealed boundary without a modeled carrier/path,
- generate in an unloaded region from chunk-order RNG.

Player-caused magical fire and ordinary hazards retain their existing separate
rules.

## Scar visual ecology

Scarring reflects place and carrier instead of applying one purple palette:

- wet scars form films, reeds, slick crusts, and fog structure,
- dry scars form branching salts, glassy needles, and dust wakes,
- forest scars bleach rings, interrupt leaf motion, and grow threadlike mats,
- alpine/polar scars sing, craze ice, and bend auroral light,
- cave scars form echoes, false shadows, and resonant mineral crust,
- industrial scars accrete around seams, drains, conductors, and vessels.

Common visual grammar—phase mismatch, branching, repeated broken symmetry,
polarized edges—makes them recognizable across climates.

Procedural fallback art, texture-pack art, particles, audio, lighting, and
accessibility cues all receive final review in goal 9.

## Harm

Dross exposure is environmental, not an instant curse:

- short trace/strained exposure is harmless,
- seep exposure may impair working stability, perception, stamina, or health
  recovery,
- scar exposure causes escalating bounded status effects,
- contaminated water/preparations carry their measured burden,
- physical armor is not magical immunity,
- preparations mitigate body effects but do not clean the environment,
- leaving the area and ordinary recovery remain possible.

No permanent player-stat loss exists in this arc. Death and item behavior use
ordinary rules.

## Prevention

The cheapest dross is the dross not produced:

- choose matched resonance/materials,
- remain below safe throughput,
- measure source stability and capacity,
- use a ritual instead of a wand for bulk work,
- maintain and clean apparatus,
- use settling structures and capture vessels,
- schedule recovery between high-load batches,
- locate risky work away from conductive water and dense settlement,
- keep emergency empty containment.

Good practice is observable and learnable. There is no invisible optimal
formula available only from a wiki.

## Containment and transport

Containment moves dross into a finite owner:

- vessels,
- still-salt beds,
- Ashlace tissue/filter,
- soil/sediment traps,
- sealed cargo ampoules,
- purpose-built scar fragments.

Every container has capacity, stability, damage, inspection, and failure
behavior. Moving pollution away is not remediation; it creates a cargo and a
destination responsibility.

Long-distance transport uses ordinary inventory, vehicles, roads, ships, and
handling risk. No pipe or ritual sends dross off-world.

## Treatment and recovery

There is no universal cleansing block or instant potion. Recovery combines:

### Stop and isolate

Shut down sources, break conductive paths safely, divert clean water around
the site, and establish containment.

### Remove manifestations

Excavate/cut scar growth with appropriate tools. Drops hold their exact dross
and any ordinary material. Burning them merely changes carrier and may spread
airborne dross.

### Sequester

Use filters, still salt, Ashlace, settling ponds, or sediment beds. This makes
the region safer while concentrating an accountable hazardous material.

### Reorder

Healthy hearts and specific mature ecosystems slowly convert Dross into
Ambient Current:

- conversion debits dross and credits equal Current,
- rate depends on living habitat, water/soil state, heart condition, burden,
  and capacity,
- it has a saturation ceiling and cannot accept an industrial dump instantly,
- dead hearts withdraw the bonus but player-operated treatment still works,
- offerings of contained charge/dross may route through this process rather
  than disappear at dawn.

### Dilute/disperse

Controlled release may keep burden below manifestation thresholds but never
changes the planetary total. It can spread liability into another watershed
or country and is deliberately a political choice.

### Restore habitat

Clean water, repaired soil, seed reintroduction, and time recover ecological
functions after the arcane burden falls. Magic does not instantly repaint the
biome.

## Attribution and social evidence

Exact transaction history is too large and too omniscient for ordinary play.
Use bounded provenance mixtures:

```text
top contributing installation ids
top contributing stable player ids where known
known/unknown fraction
resonance/process signature
first/last contribution interval
transport confidence
```

Mixing, transport, and time reduce confidence. A tuning lens can compare a
sample to an apparatus signature and report likely/consistent/inconclusive,
not expose an administrative guilt list.

Echo slate reference plates and laboratory records can provide stronger
voluntary evidence. Operator audits retain aggregate exact contribution
events under existing permission/privacy rules.

Nothing mechanically prevents:

- dumping waste downstream,
- hiding a vessel near a rival,
- falsifying a player-authored label,
- operating an unsafe town laboratory,
- monopolizing the clean confluence.

The game supplies evidence, consequences, recovery, and moderation primitives;
players supply law and politics.

## Unloaded world and catch-up

- Coarse dross transport and threshold history advance globally.
- Scar manifestation sites are selected by canonical ids and persisted.
- Loading materializes the already-authoritative state once.
- Unloading summarizes fine mobile state without deleting owners.
- Catch-up is bounded, deterministic, and equivalent to continuous stepping.
- Scarring never uses first-load surprise to bypass its warning time.
- Player construction/touched data is consulted before manifestation
  placement even when the chunk was unloaded.

## Mods and scripts

Mods may define:

- dross coefficients for approved processes,
- carrier partition behavior within bounded schemas,
- scar growths/entities/statuses using approved handlers,
- filters/containers/treatment ecology,
- visual/audio variants,
- server policy thresholds.

They may not:

- delete or create dross,
- alias dross to Ire,
- replace arbitrary blocks,
- bypass warning stages,
- make an unbounded self-replicator,
- expose player attribution beyond earned/server-authorized evidence,
- ship a zero-cost infinite sink.

Content validation simulates declared lifecycles and rejects unbalanced or
unbounded fixtures.

## Operator tools

Extend:

```text
wildforge --arcane-audit <world>
wildforge --arcane-atlas <world> --layer dross
```

Reports include:

- totals by carrier/resonance/country/watershed,
- generation and reordering rates by process,
- top installation/source classes,
- burden band area and population exposure,
- scar site count/state,
- contained inventory and unknown fraction,
- unexplained delta,
- projected recovery under current conditions.

Operator commands may quarantine a corrupt server state or perform a clearly
logged external adjustment. They are not survival cleanup tools.

## Required scenarios

### Careful homestead

One player uses charms, occasional wands, small ritual horticulture, and an
apothecary for years. Good maintenance keeps the area below seep without
requiring daily cleanup chores.

### Careless laboratory

Forced overdraw, poor resonance, open wastewater, and overfilled vessels
advance through every warning stage into a local scar. The cause is
reproducible and warnings precede damage.

### Watershed dispute

An upstream laboratory discharges waterborne dross. It follows the qualified
river into a downstream wetland without changing water mass. Samples retain a
degrading but useful source signature.

### Dead-heart recovery

A scar in dead country can still be contained and treated by players, but
reordering is slower. Reawakening the heart accelerates recovery without
creating Current or water.

### Planetary abuse

Sustained multiple industrial sources can create connected regional problems.
One ordinary accident cannot trigger an unstoppable global cascade. Stopping
all sources allows eventual bounded recovery.

## Required tests

### Transport and conservation

- Every carrier transfer closes the arcane ledger.
- Water movement carries proportional dross and exact remainder.
- Evaporation/freezing/soil binding/erosion obey declared partition rules.
- Air and water cross every planet seam without privilege or loss.
- Loaded/unloaded/catch-up/worker order produce identical checkpoints.

### Thresholds and manifestation

- Hysteresis prevents band flicker.
- Each warning appears before its damaging stage under bounded rate limits.
- Canonical manifestation sites do not duplicate across load/save.
- Structural player blocks remain byte-identical through every scar stage.
- Cultivated-life effects stall/wilt without silent deletion.
- Breaches redistribute accounted state and never become portals.

### Prevention and recovery

- Matched stable practice measurably produces less dross.
- Every containment medium fills and fails at a finite capacity.
- Excavation transfers scar dross into drops/residue.
- Ashlace/filter treatment sequesters rather than deletes.
- Heart/ecology reordering debits and credits exactly and respects rate caps.
- Full scenario scars are recoverable without a unique item.

### Ire, authority, and evidence

- All four Ire/dross combinations retain distinct behavior.
- Habitat damage, not dross arithmetic, invokes Ire.
- Attribution mixtures degrade correctly through transport.
- Clients cannot forge, erase, or over-read provenance.
- Concurrent dumping/cleanup commits once and identifies the right
  installation/actor.
- Agent observation/action parity and mod restrictions hold.

### Budgets

- Worst legal source populations, scar sites, provenance aggregation, atlas
  transport, and client deltas meet measured CPU/memory/save/wire budgets.
- Long recovery simulation retains exact totals and bounded cardinality.

## Completion criteria

This goal is complete when:

- every magical practice has a real dross path,
- dross moves through air, water, soil, organisms, containers, apparatus, and
  scars without loss or duplication,
- warning, seep, scar, and breach stages are deterministic and legible,
- construction permanence and ordinary material/water truth remain intact,
- prevention is cheaper than cleanup and every catastrophe is recoverable,
- Ire remains independent while real ecological harm still matters,
- attribution supports evidence and politics without omniscience,
- loaded/unloaded, persistence, multiplayer, agents, mods, diagnostics,
  visuals, audio, and accessibility work,
- all scenarios, tests, budgets, and repository gates pass.

Do not mark this complete because a contamination meter rises. Dross matters
only when players can understand what caused it, live with what it does, argue
about responsibility, and undertake the work of recovery.
