# Planetary qualification — prove one world can live

> **Status: implementation design complete, not implemented.**
>
> This is goal 9 and the final goal of the planetary-world sequence. Run it
> only after goals 1–8 are implemented and individually green.

## Purpose

The preceding documents divide work so it can be implemented safely. This
goal proves they form one planet rather than nine locally convincing systems.
It closes integration gaps, removes superseded infinite-world behavior,
updates player/mod/operator surfaces, runs long simulations, and records the
qualification evidence.

It is not a new feature wishlist. Every change in this goal must close a
cross-document requirement or qualification failure.

## Completion inventory

Audit the implemented repository against every numbered requirement,
invariant, test, diagnostic, format, and completion criterion in:

1. `docs/planet-topology-plan.md`
2. `docs/planet-atlas-plan.md`
3. `docs/planetary-geology-plan.md`
4. `docs/planetary-climate-plan.md`
5. `docs/planetary-hydrology-plan.md`
6. `docs/planetary-water-cycle-plan.md`
7. `docs/planetary-biomes-plan.md`
8. `docs/finite-materials-plan.md`

Create a checked qualification matrix in this document's implementation
record. A passing test name without inspected coverage is not enough.

## Remove the superseded world

Search the full tree for remaining infinite/planar assumptions:

- unbounded signed surface coordinates,
- `ChunkPos { x, z }` without a face,
- global downward gravity,
- planar Euclidean waystone/cairn distance,
- planar plate and continent noise,
- independent temperature/humidity noise deciding biomes,
- province-imposed climate,
- river/lake noise unrelated to drainage,
- global weather enum driving all precipitation,
- rainfall creating water,
- evaporation deleting water,
- hash-placed perpetual springs,
- new-chunk-only resource availability,
- tracked material deletion on wear/despawn,
- “generation horizon” ocean accounting,
- docs telling players to walk outward for new country.

Each occurrence is removed, ported, or documented as a strictly local
face-chart implementation beneath a topology-aware interface.

There is no shipping “temporary fallback” to the infinite generator.

## End-to-end planet creation

A player or headless host can:

1. choose Create World,
2. enter/roll a seed,
3. see planetary generation progress,
4. cancel without leaving a corrupt world,
5. enter a qualified spawn on success,
6. save, quit, reload, host, and join,
7. travel indefinitely and circumnavigate.

Creation validates the planet before it becomes selectable. Failed seeds
follow the deterministic bounded retry policy and report fatal exhaustion
clearly.

The world-selection UI shows:

- seed,
- generator/topology version,
- planet status,
- mod/content hash,
- corruption/incompatible-format errors.

## Long simulation harness

Add a headless planetary harness capable of advancing:

- one year,
- ten years,
- one hundred years,
- a configurable stress duration.

It runs coarse climate, weather, water, groundwater, ecology reconciliation,
and material audits without rendering. It may use accelerated simulation but
must execute production algorithms and transfer paths.

At defined intervals record:

- total water and salt by reservoir,
- unexplained mass delta,
- lake/ocean level distributions,
- aquifer head distribution,
- precipitation/evaporation/runoff totals,
- temperature and snow cover,
- biome/habitat changes,
- ecological population aggregates,
- tracked material totals,
- CPU time and peak memory.

An undisturbed century must remain bounded and seasonally stable. Climate
variability is allowed; numerical draining, filling, salinizing, heating, or
cooling trends without a modeled cause fail.

## Scenario qualification

Create deterministic end-to-end scenarios.

### Mountain rain shadow

- Moist marine air crosses a coastal range.
- Windward forest/rivers and lee aridity emerge.
- A river sourced in the range crosses the dry country.
- Voxel terrain matches atlas maps.

### Oasis and aquifer

- Rain/snow recharges an upland aquifer.
- A spring emerges in arid lowland and sustains an oasis.
- Pumping lowers the spring and oasis.
- Recharge restores both without creating water.

### Lake and dam

- A through-flow lake holds seasonal equilibrium.
- A player dam changes local basin storage.
- A breach sends the exact released mass downstream.
- Loaded and unloaded reaches agree.

### Flooded mine

- A mine intersects an aquifer.
- Seepage floods exposed workings.
- A powered pump lowers visible water and aquifer head.
- Stopping the pump permits physically bounded recharge.

### Continental journey

- A route crosses countries, watersheds, climate zones, and at least one
  cube-face seam.
- Bearings remain correct.
- Weather varies spatially.
- Multiplayer interest and entity interpolation remain continuous.

### Finite industry

- Mine, process, use, break, salvage, and remake one full metal tool chain.
- Deposit and material audits reconcile exactly.
- Steam returns its water to the cycle.

### Heart crisis

- Local exploitation sickens and kills a heart.
- Ecology/wardens respond.
- Climate and water conservation continue physically.
- Restoration works under the new habitat compatibility rules.
- A Long Winter applies/removes the defined supernatural global anomaly
  without corrupting orbital seasons.

## Chunk-order and seam matrix

For representative chunks on:

- every face interior,
- every directed face edge,
- every corner neighborhood,
- ocean/land/coast,
- river/lake,
- mountain/desert/forest,
- heart/structure/deposit,

generate in forward, reverse, random, parallel, host-streamed, and
save/reload orders. Immutable results and accounting must agree.

Exercise across seams:

- blocks and light,
- fluids and salt,
- fire and falling blocks,
- machines and shafts,
- mobs, boats, players, drops, projectiles,
- weather and groundwater flux,
- rivers and strata,
- countries and hearts,
- signs, stalls, waystones, and agent actions.

## Performance qualification

Measure on the documented review machine and record:

- atlas creation wall time and peak memory,
- first playable chunk latency,
- steady chunk generation/meshing throughput,
- server coarse-simulation cost,
- active detailed-water cost,
- host plus guest seam crossing,
- save size and save latency,
- century-harness throughput,
- atlas/material/water audit time.

Required guards:

- no per-frame work over the full planet,
- no server-tick work proportional to all voxel cells,
- atlas simulation is sliced and bounded,
- unloaded country does not require chunk materialization,
- save does not rewrite immutable genesis or every chunk,
- render distance does not bypass the planet via draw-through.

Any budget changes from the individual plans require measured rationale in the
implementation record.

## Player-facing documentation

Rewrite the README world sections around the planet:

- finite closed world and circumnavigation,
- planetary geology and prospecting,
- latitude/climate/local weather,
- watersheds, rivers, lakes, aquifers, and springs,
- exact finite water cycle,
- salinity and freshwater,
- finite materials, repair, salvage, and retrogen,
- local seasons and the Long Winter exception,
- save-format incompatibility.

Update:

- controls/UI descriptions,
- modding guide and executable example,
- server/operator commands,
- agent MCP coordinate/action documentation,
- all superseded design documents with a short historical note pointing to
  the planetary replacement where their old rule no longer holds.

Do not rewrite old implementation records as if they had never shipped.
Clearly distinguish historical behavior from current truth.

## Visual atlas and capture set

Produce a qualification directory outside tracked runtime assets unless the
README uses selected images:

- six-face and globe atlas maps for all required layers,
- continent/ocean globe views,
- mountain/rain-shadow transect,
- river source-to-mouth sequence,
- oasis dry/recharged comparison,
- seasonal lake range,
- sea-level and mountain horizons,
- seam-spanning building and river,
- local weather contrast,
- representative countries/biomes/hearts.

Every image includes seed, generator version, location, season/time, and
relevant settings in a sidecar or filename convention.

## Required tests and gates

Run and pass:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
cargo deny check advisories
```

Also run the new:

```text
production atlas generation suite
planet census seed suite
all directed seam/corner scenarios
host/guest planetary loopback suite
100-year water/climate stability suite
water and material audits
visual capture manifest validator
mod retrogen qualification suite
```

Commands and measured results are recorded in the implementation header.

## Final implementation record

When and only when qualification passes, prepend:

- implementation date,
- commits/PRs,
- final constants and format versions,
- deviations from every plan and why,
- measured budgets,
- full verification command/results,
- links/paths to diagnostic output,
- known limitations that do not contradict a completion criterion.

Then update each preceding planetary document from “not implemented” to its
accurate implementation status and add its own notes-vs-spec header.

## Completion criteria

The planetary sequence is complete only when:

- every requirement in goals 1–8 has authoritative evidence,
- no superseded infinite-world behavior remains on a shipping path,
- one planet can be created, saved, hosted, joined, circumnavigated, and
  simulated for a century,
- geology, climate, hydrology, water, biomes, countries, hearts, and material
  accounting agree,
- exact water/salt and tracked-material audits close,
- all seam/corner/chunk-order/multiplayer scenarios pass,
- performance budgets are measured and acceptable,
- README, modding, agent, and operator documentation describe current truth,
- complete diagnostics and visual qualification exist,
- every required repository gate passes.

Do not mark this document implemented because all earlier goals say
implemented. This goal exists to distrust that claim until the integrated
planet proves it.
