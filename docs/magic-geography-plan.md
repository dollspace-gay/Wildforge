# Magical geography — the Current belongs to this planet

> **Status: implemented and production-qualified (2026-08-02).**
>
> This is goal 2 of `docs/magic-sequence.md`. It requires goal 1 and the
> complete qualified planetary atlas.

## Implementation record — 2026-08-02

Goal 2 is live in world creation, the authoritative simulation, persistence,
dedicated hosting, multiplayer observation, agents, scripts, mods, operator
tools, and the graphical client. It adds no player-cast working and no
harvestable magical ecology. The implementation includes:

- one compact control and dynamic record for every one of the qualified
  planet's 393,216 atlas cells, with physical-area capacity, deep capacity,
  four directed conductances in the local tangent frame, deep exchange,
  dross mobility/retention, stability, recovery potential, baseline six-band
  resonance, site reference, exact Current, exact dross, fixed-point
  remainders, extrema, wakes, scars, and bounded observations;
- deterministic four-stage genesis after ordinary planetary qualification,
  with capacity-weighted deep/ambient allocation, exact six-resonance
  allocation, a bounded equilibrium, stable causal sites, distribution
  validation, and atomic publication;
- one-minute coarse planetary transport over the entire closed sphere. Every
  undirected edge is evaluated once; debit and credit share one signed delta;
  deep exchange and dross use the same conservative pattern; work is sliced
  without mutating the checkpoint until a complete pass commits;
- stable named Confluences, Wells, Stills, Echoes, Heartshadows, Wakes, and
  Scars. Confluence networks derive from actual conductive connectivity;
  Wells debit finite deep storage; Heartshadows condition signs and recovery
  without overwriting capacity; ruin Echoes use the same deterministic
  surface roll as lazy structure generation before any chunk is visited;
- player-safe qualitative surveys and sensory signs. Ordinary players and
  agents receive no raw cell, balance, exact drift, hidden site id, or global
  map. The tuning-lens precision and observation/map UI remain owned by goal
  4, while this goal persists their bounded records and confidence contract;
- declarative creation-time modifiers and explicit host-only retrogen.
  Retrogen skips touched cells, applies bounded capacity/resonance changes,
  preserves unknown removed-provider sites, and redistributes existing
  Current without changing genesis;
- `--arcane-geography-audit`, `--arcane-geography-export`, and
  `--arcane-geography-retrogen` operator paths, plus the normal arcane audit's
  geography-custody reconciliation.

### Formats, identity, and recovery

| Surface | Qualified version |
|---|---:|
| Geography schema / algorithm / dynamic journal | 1 / 2 / 1 |
| Immutable / dynamic framing | `WAG1` / `WAD1` |
| Planet atlas format / algorithm / dynamic | 6 / 7 / 3 |
| World generator | 10 |
| Multiplayer protocol | 27 |
| Authoritative geography step | 60 seconds |

The production manifest binds seed, content hash, genesis parameter, six
resonance totals, all three layer checksums and byte lengths, last completed
authoritative time, versions, retrogen history, and per-stage timings.
Immutable controls are never regenerated when a saved planet is missing or
corrupt: loading fails closed. Dynamic state and the site catalog use atomic
primary/backup bundles plus a recoverable pending checkpoint; manifested
corruption restores the last complete backup or refuses the world rather than
minting a second genesis.

For qualification seed `20260802`, immutable controls were 10,620,232 bytes,
dynamic state 11,529,296 bytes, and the catalog 2,439,433 bytes. The measured
loaded geography footprint was 31,457,280 bytes (30 MiB). Genesis geography
stages themselves took 152.216 ms listening to stone, 20.017 ms finding the
Current, 26.070 ms settling the deep, and 71.733 ms marking sites.

### Production census

The current release created and reopened a full 256-by-256-per-face planet
from seed `20260802`. Its exact census was:

| Measure | Qualified value |
|---|---:|
| Atlas cells / physical area | 393,216 / 341,782,637.711 block² |
| Total cell capacity | 1,385,687,079 |
| Geography genesis Current | 1,409,286,144 subunits |
| Deep / ambient / dross | 1,006,140,738 / 403,145,406 / 0 |
| Each of Root, Tide, Ember, Stone, Gale, Echo | 234,881,024 |
| Confluences / independent networks | 117 / 20 |
| Wells / Stills | 142 / 156 |
| Echoes / Heartshadows | 5,556 / 500 |
| Total stable sites | 6,471 |

The census emitted 20,848 data rows across continent, climate, biome,
country, watershed, and site dimensions: 50 continent keys, six climate
regimes, 12 biomes, 501 country keys including unclaimed/ocean country zero,
13,808 watersheds, and 6,471 sites. Every generated country had nonzero usable
ambient Current. The capacity distribution ranged from 1,995 to 5,585 per
cell, with median 3,690 and 99th percentile 4,670, preserving an exceptional
rich tail without inert ordinary country. Four deterministic statistical
seeds (`3`, `17`, `41`, `89`) additionally passed causal distribution,
face-density, face-center, edge, corner, and interior comparisons.

The parent arcane audit remained exact after geography took custody:
1,610,612,736 total subunits, zero unexplained delta, six equal 268,435,456
global resonance totals, and a geography account exactly matching the
geography checksum and mixture.

### Maps and operator evidence

One export writes 34 PNG maps: a six-face atlas and Lambert cylindrical
equal-area projection for capacity, conductivity, stability, deep Current,
ambient Current, dross, all six resonances, potential, drift, place id, place
type, and recovery potential. Extensive quantities are shown as density on
the equal-area view so legitimate cubed-sphere cell-area differences do not
masquerade as magic seams. Sparse projection bins are deterministically
filled across the date line rather than appearing as black voids.

The production visual review checked capacity, conductivity, stability,
potential, Tide, and place-type maps in both representations. Causal
continental, oceanic, fault, water, soil, and heart/ruin structure remained
legible; place facts stayed sparse; no empty projection pixels or privileged
face center/edge/corner pattern remained. The same export produced:

- a 20,849-line physical-area census including its header;
- 231 representative mountain, fault, river, coast, Well, Still, and
  Confluence cross-section rows;
- a 117-row Confluence connectivity graph plus header;
- 126 physical-area-weighted percentile rows plus header;
- a complete named site catalog, exact geography audit, and machine-readable
  validation report.

Reproduce the complete evidence set without committing generated binaries:

```sh
wildforge --arcane-geography-export <world> --output <directory>
wildforge --arcane-geography-audit <world>
wildforge --arcane-audit <world>
```

### Budgets and live qualification

Fresh atomic creation of the production planet completed in 25.73 seconds at
696,272 KiB peak RSS with no swap or OOM. Prepared-world entry and full
water/salt/material reconciliation completed in 7.65 seconds at 830,948 KiB.
The 34-map export completed in 13.08 seconds at 487,768 KiB.

The explicit optimized transport probe measured 44.703 ms for a complete
393,216-cell planet pass and 3.794 ms for the worst authoritative 4,096-cell
server slice, with no chunks loaded by the transport itself. Exact totals and
all six resonance identities were unchanged after the pass.

A live WSLg release client entered the production save at view distance 4,
moved under ordinary player physics, settled 63 resident and meshed chunks
with zero dirty chunks, and captured a 1280-by-720 frame at 23 FPS, 2.23 ms
simulation, and 38.90 ms drawing. Peak RSS was 958,856 KiB with no swap. The
settled capture visibly qualified the ordinary-player sign—faint static,
stable rhythm, and Stone resonance—and the generic toast wrapper kept the
long message inside the viewport.

A separate release dedicated server then admitted a real MCP agent through
the normal protocol. The agent received a bounded 21-by-21 local map and
coarse local Current/dross bands plus one categorical Stone sign, then
pathfound from `neg_x 5631,67,6395` to `neg_x 5633,67,6395` while weather
advanced. The categorical sign uses the same sensory vocabulary as solo play;
no global geography, exact balance, resonance mixture, site identity, or
drift crossed the wire. The live run also exposed and fixed a guest-side
performance bug: a replicated weather/ecology block burst relit the connected
view once per message. Both graphical guests and agents now apply a poll as
one shared lighting batch while retaining exact block metadata and salt. The
same two-block agent route that had consumed 344 CPU-seconds without returning
then completed normally; agent peak RSS fell from 343,704 to 163,556 KiB in
the qualified rerun. A forced shutdown left two transient warden accounts;
normal reopen recovered them into Deep, and the final audit returned to zero
dormant owners with both ledgers exact.

### Verification and deliberate boundaries

Targeted tests cover byte-identical genesis, causal relations, shared ruin
rolls, all directed seams/corners, reciprocal physical edge factors, pulse
crossing, fixed-point equilibrium, dross potential, 1,000 randomized steps,
slice and catch-up identity, atomic backup recovery, missing-layer refusal,
sites, wakes, scars, surveys, qualitative information, retrogen, exports,
projection coverage, remote/solo resonance-sign parity, replicated block-burst
settling, and the multi-seed distribution suite. Repository gates at
qualification were formatting clean, all-target Clippy clean with warnings
denied, optimized release build clean, advisory audit clean, and 579
nonignored tests passed with zero failures; 21 explicit operator/measurement
probes remained ignored by the ordinary suite, and the geography production
probe was run separately and passed.

Goal 8 still owns dross sources, air/water advection, manifestations, hazards,
and remediation; goal 4 owns the tuning lens and record/map interface; goal 3
owns magical organisms and harvestable resources. This goal supplies their
finite storage, conservative transport, causal controls, stable place
identity, qualitative signs, persistence, and audit hooks without pretending
those later gameplay systems already exist.

## Purpose

The Current must be somewhere before anything can gather or use it. This goal
adds its immutable geographic controls, initial reservoirs, conservative
transport, named place types, persistence, survey data, and global
diagnostics.

It does not place magical plants, harvestable crystals, implements, or player
workings. It creates a planet on which those later features have causes.

## Geographic causality

Magical distribution is neither uniform nor independent noise. It derives
from qualified planetary fields:

- mineral family, crust type and age,
- faults, plate boundaries, volcanic history, and pressure,
- groundwater, surface water, salinity, and permeability,
- soil and organic activity,
- prevailing wind and storm climatology,
- latitude and the planet's magnetic/auroral geometry,
- hearts, ruins, and recorded historical disturbance.

Longitude has no arbitrary magical banding. Longitudinal patterns arise from
continents, ocean basins, faults, currents, climate, and history. Latitude
matters where sunlight, seasonality, circulation, ice, and auroral activity
provide a cause.

Cube-face centers, edges, and corners have no magical privilege.

## Atlas layers

### Immutable genesis controls

Add area-weighted atlas layers:

```text
Current capacity
deep-reserve capacity
surface/deep exchange coefficient
horizontal conductivity
dross mobility/retention
baseline six-resonance mixture
stability
natural recovery potential
magical-geography region/site references
```

Capacity is how much Current a place can hold at ordinary potential, not a
current balance. Conductivity governs movement. Stability governs how well
the place tolerates rapid use. Recovery potential is only a rate coefficient;
actual recovery in goal 8 still transfers existing Dross.

Vector or directional conductivity, where justified by strata, groundwater,
or prevailing circulation, is stored in the local tangent frame and rotates
correctly across face edges.

### Dynamic state

Persist separately:

```text
deep Current
ambient Current
ambient resonance mixture
air/soil dross aggregate
transport remainders
temporary wake/anomaly state
historical extrema used for ecology and warnings
```

Bound items, organisms, chunks, hearts, wardens, workings, and scars remain in
their owning stores and appear only as aggregate totals or references here.

## Genesis allocation

World creation gains explicit stages after ordinary geography is final:

```text
LISTENING TO THE STONE
FINDING THE CURRENT
SETTLING THE DEEP
MARKING THE CONFLUENCES
```

The exact presentation may change, but generation follows this order:

1. Derive capacity, conductivity, stability, and initial resonance from the
   immutable planet.
2. Allocate a fixed genesis total between deep and ambient reservoirs,
   weighted by physical cell area and capacity.
3. Solve a deterministic bounded equilibrium over the closed sphere.
4. Identify stable place types from geographic controls and the equilibrium.
5. Validate distribution, connectivity, progression redundancy, and seams.
6. Write immutable and dynamic layers atomically with checksums.

The genesis total is a versioned planet parameter. It may be balanced only
with measured census evidence. Generation never retries until a desired
pretty map appears; invalid seeds fail their explicit constraints.

## Conservative transport

Ambient Current moves toward lower local potential:

```text
potential = ambient / effective capacity
```

Neighbor transfer is bounded by:

- potential difference,
- source balance,
- shared physical edge length,
- cell area,
- conductivity in the crossing direction,
- a stable fixed simulation step.

The implementation may use a more numerically robust equivalent, but each
edge transfer is computed once and applied as equal debit/credit. The order of
cell iteration cannot change the result.

Deep exchange moves Current between the cell's deep and ambient reservoirs
using the same debit/credit rule. A "well" is a high-exchange location, not an
infinite source.

Dross has separate mobility and ordinarily moves more slowly. Goal 8 adds air
and water advection; this goal supplies conservative neighbor/deep storage and
the extension points.

### Time and loading

- Coarse transport advances for the entire planet on authoritative fixed
  intervals.
- Catch-up is derived from persisted time and bounded into deterministic
  steps.
- Loaded chunks neither gain simulation priority nor become privileged sinks.
- Opening, closing, or visiting a region cannot change planetary results.
- The idle equilibrium does not oscillate or leak units through rounding.

## Resonance geography

The six-resonance mixture is shaped, not generated, by geography:

- Root strengthens through persistent living soils and productive habitat.
- Tide strengthens through groundwater, wetlands, rivers, coasts, and ocean
  circulation.
- Ember strengthens around volcanic provinces, fire history, and strong
  thermal gradients.
- Stone strengthens in old coherent crust, pressure zones, and massive rock.
- Gale strengthens in storm tracks, exposed heights, and auroral belts.
- Echo strengthens around long-lived hearts, stable caves, ruins, and places
  with repeated supernatural history.

These relations are biases, not one-to-one biome labels. A wet volcanic island
can hold Tide and Ember; an old desert craton may be rich in Stone but poor in
ambient quantity.

Natural and later player processes may transform the mixture while preserving
scalar Current. Genesis stores the baseline toward which slow environmental
mixing tends.

## Place types

Place types are atlas-backed geographic facts, not necessarily structures.
They receive stable ids, extents, confidence/signature data, and generated
name seeds.

### Confluence

Several conductive paths or resonance regimes meet. It has high usable
throughput, not free generation. Workings are easier to stabilize; abuse can
affect a wider connected region.

### Well

Deep/surface exchange is unusually strong. Its apparent replenishment debits a
finite deep reserve. A depleted well recovers slowly as the surrounding
planet re-equilibrates.

### Still

Low capacity or conductivity makes magic weak and hard to sustain. A still is
not a disposal void: dross placed there remains in its ledger and can make the
site worse.

### Echo

A place retains a readable resonance trace of repeated or exceptional events.
Echoes reveal patterns and history, not an exact replay camera. Some are
genesis facts around ruins; others become sparse history overlays.

### Heartshadow

A country's heart persistently conditions resonance, recovery, and natural
signs in a geographically bounded area. Heartshadows follow country topology,
but they do not overwrite capacity, climate, or water.

### Wake

A temporary moving anomaly left by a warden, storm, large ritual, or other
event. Wakes decay by transferring their charge/dross into ordinary
reservoirs.

### Scar

A location whose dross has crossed manifestation conditions. Goal 8 owns
hazards and remediation. This goal reserves persistent site identity so
unloaded or currently invisible scars still exist.

## Geographic naming and maps

The ordinary player map may record discovered approximate boundaries and
named observations. It never exposes raw atlas cells or exact balances.

Before the tuning lens exists, cues are sensory:

- unusual quiet or harmonic ambient sound,
- recurrent lights or polarized shimmer,
- plant posture and animal behavior,
- mineral ringing, fog structure, or persistent static,
- a heart's visible condition.

After goal 4, surveys can record qualitative bands:

```text
still / faint / steady / strong / saturated
stable / strained / fouled
dominant resonance(s)
direction of local drift with uncertainty
```

Exact values and global maps remain operator diagnostics.

## Progression and distribution guarantees

Every qualified default planet must provide:

- usable Current and at least one stable observation site on every viable
  major continent,
- at least three independent confluence networks globally,
- multiple wells and stills in different climates,
- all six resonances in more than one geographic region,
- no progression-critical resonance restricted to one island, one heart, one
  deposit, or one cube face,
- a viable early discovery region reachable from every accepted spawn without
  crossing an ocean,
- rare exceptionally rich places without making ordinary country magically
  inert.

These are planet-census guarantees, not player-facing coordinate reveals.

## Persistence and versioning

The planet manifest records:

- geography algorithm and layer versions,
- genesis Current parameter,
- layer checksums,
- site catalog checksum,
- last authoritative simulation time,
- dynamic journal/checkpoint version.

Immutable arcane geography is never silently regenerated under a saved world.
Algorithm updates apply only to new planets unless an explicit migration
preserves site identity and ledger totals.

Dynamic saves are atomic and reconcile against goal 1. A damaged arcane layer
does not cause the game to reinitialize a second genesis total.

## Mod integration

Mods may declaratively add bounded modifiers or site predicates at world
creation:

```toml
[[arcane_site]]
id = "example:singing_fault"
requires = ["fault", "carbonate_rock"]
capacity_factor = 1.15
resonance = { echo = 2, stone = 1 }
rarity = 0.02
```

Rules:

- modifiers combine through bounded documented operations,
- no mod obtains direct per-cell arbitrary mutation,
- creation-time content participates in the genesis hash,
- post-creation additions use an explicit finite-world retrogen policy,
- retrogen never overwrites construction or changes the genesis total,
- new capacity/sites redistribute or reclassify existing Current rather than
  add it,
- removed mods preserve unknown site ids and dynamic balances safely.

## Operator tools

Extend the atlas exporter and arcane audit with:

```text
capacity
conductivity
stability
deep reserve
ambient Current
dross
each resonance
potential and drift
place ids/types
recovery potential
```

Exports include all six faces and an equal-area or globe projection suitable
for checking seam privilege. Also produce:

- totals/census by continent, climate, biome, country, watershed, and site,
- cross-sections through representative mountains, faults, rivers, coasts,
  wells, stills, and confluences,
- a connectivity graph of conductive regions,
- min/max/percentile and physical-area-weighted histograms.

## Required tests

### Genesis and causality

- Same seed, versions, and content produce byte-identical arcane genesis.
- Chunk generation/load order has no effect.
- Capacity and resonance relationships follow their declared geography in a
  seed suite.
- No face center, edge, or corner has a statistical privilege after physical
  area weighting.
- Every distribution guarantee passes for the qualification seeds.

### Topology and transport

- Equal fields remain equal across every directed face edge and corner.
- A pulse crosses seams without gain, loss, reflection, or rotation error.
- Each neighbor transfer debits and credits the exact same units.
- Global total remains exact across long randomized transport and deep
  exchange.
- Iteration order and worker count do not change the checkpoint hash.
- Catch-up and continuous stepping reach identical results.

### Stability and performance

- An undisturbed planet approaches and retains a bounded equilibrium.
- No cell oscillates because of fixed-point rounding.
- The full default planet advances within the measured server budget without
  loading chunks.
- Persistence remains bounded and atomic.

### Places and information

- Place ids/extents are stable across save/load.
- Wakes decay by accounted transfer.
- Wells debit deep reserves and stills retain dumped dross.
- Player surveys reveal only earned qualitative information.
- Operator exports reconcile with the arcane audit.

## Completion criteria

This goal is complete when:

- the qualified planet carries causal immutable and dynamic arcane layers,
- all Current and dross transport is conservative, stable, seam-safe, and
  chunk-order independent,
- place types exist as persistent geographic facts,
- distribution and progression guarantees pass a planet seed suite,
- ordinary maps and operator exports expose the correct different levels of
  information,
- mod genesis/retrogen behavior is finite and deterministic,
- long-run, topology, persistence, performance, and repository gates pass,
- the implementation record contains maps, censuses, budgets, and versions.

Do not call a high noise value a confluence. It must arise from the same
planetary causes that make the surrounding place legible.
