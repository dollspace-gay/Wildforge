# Planetary qualification — prove one world can live

> **Status: implemented and qualified (2026-08-01).**
>
> This is goal 9 and the final goal of the planetary-world sequence. Run it
> only after goals 1–8 and the discovered prerequisite
> `docs/world-entry-streaming-plan.md` are implemented and individually green.

## Implementation record — 2026-08-01

This record covers the integrated working tree on branch `agent/planet-atlas`
based on commit `2bd7a95`. The final implementation is intentionally one
reviewable change set containing goals 1–9 plus the live-discovered entry
remediation; commit/PR identifiers are to be filled by the publication step.

Final format constants are:

| Contract | Current value |
|---|---|
| World topology | `cube_sphere_v1`, six 8192-block faces, 256-block shell |
| Generator / multiplayer | generator 9 / protocol 24 |
| Atlas | WFA6, format/algorithm 6; geology/hydrology/biome schema 1 |
| Mutable planet | WFD3 dynamic schema 3; WFW1 exact water schema 1 |
| Chunks | WFC8 on disk; network-only WFC9 with exact host-derived light |
| Hearts / materials | WFH4; material manifest/log schema 1 |
| Entry | spawn manifest 1; verification contract 1 |

### Checked completion matrix

This matrix was built by reading the owners and assertions, not by matching
test names alone. The named tests are representative executable anchors; the
full suite exercises the surrounding integrations.

| Goal | Inspected production ownership | Authoritative evidence | Result |
|---|---|---|---|
| 1. Topology | `planet.rs` canonical face/u/v coordinates, tangent rotation, geodesics and radial embedding; topology-aware world neighbors, physics, raycasts, renderer, save and wire DTOs | Every directed edge/corner is reciprocal; player, block, fire, lava, falling block, shaft, mesh, fluid, bearing and multiplayer seam scenarios; WFC/world-format refusal tests | [x] |
| 2. Atlas | `PlanetAtlas` owns immutable genesis, mutable weather/water, sparse history and versioned sidecars; creation publishes by one final directory rename | Serial/parallel and chunk-order identity, bounded decode/corruption/recovery/cancellation, sliced scans, complete 132-layer export, census and manifest validation | [x] |
| 3. Geology | Spherical Voronoi plates, Euler-pole motion, cratons, erosion, strata, volcanism, intrusions and finite deposits feed voxel generation | Multi-seed acceptance bands, rotational boundary classification, seam strata, host-rock and tonnage bounds, prospecting/persistence, production census and material audit | [x] |
| 4. Climate | Latitude/longitude, 23.5-degree tilt, local sun/seasons, circulation, ocean heat, orography and conservative spatial weather are server-owned | Opposite seasons/polar day, causal normals, 764-mm rain-shadow pair, bounded deterministic slices, simultaneous regional weather, seam front and host/guest agreement | [x] |
| 5. Hydrology | Priority-flood drainage, watersheds, named basins/rivers, hypsometry, salinity, erosion/deposition and voxel entitlements replace local river/lake noise | Acyclic terminating drainage, discharge geometry, seams/order/save, terminal salt, deltas/estuaries/placers, exact residuals, production 24-ocean/10-lake/878-river census | [x] |
| 6. Water cycle | Integer HU/salt ledger spans atmosphere, cloud, soil, snow, aquifers, runoff, named surface, detailed chunks, portable and industrial owners | Transfer/save/slice equality, spring drawdown, lake breach, flooded mine/pump, unloaded rain, steam/bucket salt, 200-year bounded harness, production 12-hour and live interleaving probes | [x] |
| 7. Biomes | Zonal climate, edaphic, habitat and heart/country layers remain separate; ecology/crops query real climate, soil, salinity and water | Causal zones/seams, country coverage/barriers/routes, all 500 hearts, oasis drawdown/recovery, compound animal habitat, crop moisture/drainage/salinity, heart/climate isolation | [x] |
| 8. Materials | Deposit reservation and the material ledger cover chunks, items, inventories, entities, machines, products, salvage, secondary pools, sinks and retrogen | Conservation/crash replay/save/remove-mod/retrogen scenarios; full bronze-to-machine chains; production audit has zero unexplained mass and 609 pessimistic tech arcs | [x] |
| Entry prerequisite | One persisted atlas/voxel-qualified common spawn; 5x5 common and exact per-player 3x3 sets; bounded generation/encoding/adoption; explicit protocol gate | Atomic/cancel/restart/ledger tests, pending-player inertness, reconnect, view grants, WFC9 light equality, production dedicated + agent + graphical captures | [x] |

### Superseded-world audit

The source and current player/mod/operator documentation were searched for the
listed planar/infinite assumptions. Shipping worlds always load a committed
planet atlas and construct `Generator::with_atlas`; incompatible flat or
unversioned `world.toml` is refused. `ChunkPos` is `{face,u,v}`, global
rendering uses radial embedding, and simulation's negative local `y` is
rotated only at topology crossings. Waystones, cairns, agents and countries
use topology-aware distance/bearing and face-bearing keys.

`Generator::new` still contains the pre-atlas noise terrain used by compact
legacy unit fixtures and by inert remote mirrors before host chunks arrive.
It is not reachable as an authoritative production world: production
`load_or_create` requires `PlanetAtlas::load`, server/solo workers carry the
atlas, and remote `World::ensure_chunk` refuses generation. Atlas-backed
hydrology returns before the legacy `rivernoise`/`lakenoise` branch. This is a
test/client scaffold beneath the same finite coordinates, not a shipping
infinite-world fallback.

### End-to-end and scenario matrix

| Scenario | Integrated proof |
|---|---|
| Create/open/reload | Explicit seed-or-roll UI; named progress and cancellation; atomic hidden-directory publication; browser generator/atlas/content/status; qualified spawn; save/reload and `--validate-entry` |
| Mountain rain shadow | Causal climate test plus hydrology runoff/perennial test; exported rain-shadow pair and transect; atlas/voxel terrain envelope checks |
| Oasis/aquifer | Atlas oasis visibility follows pumping drawdown and recharge; spring weakens/recovers without mass creation; fresh corrected live oasis capture |
| Lake/dam/breach | Through-flow/terminal basin validation, communicating voxel pools, player basin registration and exact downstream breached-lake transfer |
| Flooded mine | Ocean/aquifer cave materialization, seepage, powered pump drawdown and bounded groundwater refill |
| Continental journey | Geodesic bearing and directed seam tests; host/guest seam crossing with both faces streamed; spatial host/guest weather agreement |
| Finite industry | Registered bronze/iron/steel/machine chains, exact material transformations, break/salvage/remake, boiler vapor return, zero-delta production audit |
| Heart crisis | Local ire sickens/kills only its country; ecology/warden and restoration/graft tests; living/dead hearts leave physical climate/water identical; Long Winter preserves orbit |

Chunk ordering is partitioned by owner instead of running an impractical
Cartesian product: atlas/chunks compare serial, parallel, forward, reverse and
random order; hydrology compares channel/salinity/residual order; host-streamed
WFC9 bytes/light compare to host state; every directed edge covers geometry,
light, fluids, fire, falling blocks and shafts; corner tests cover canonical
topology/scalars/components. Player/mob/projectile/drop, country/heart,
structure/sign/stall/waystone and agent actions use the same canonical
position and neighbor primitives exercised by those matrices.

### Long-run harness

`tests::water_cycle::sealed_planet_conserves_exactly_for_two_centuries` is a
headless configurable harness. `WILDFORGE_STRESS_YEARS` selects any positive
duration and `WILDFORGE_STRESS_ATLAS_SIDE` selects a finite cube-sphere fixture
resolution; defaults are 200 144-day years and side 2. It calls the production
whole-hour weather, atmospheric transport, precipitation/evaporation, soil,
surface routing, basin reconciliation, daily groundwater and audit paths.
At years 1, 10, 100 and final it records water/salt and unexplained deltas,
reservoir classes and levels, aquifer heads, precipitation/evaporation,
temperature anomaly, snow, runoff, vegetation-moisture/habitat/country
aggregates, elapsed CPU wall time and Linux peak RSS. It asserts exact mass,
surviving surface/aquifer stores, seasonal rise/fall, under-0.1% bounded
multi-century redistribution and no accelerating numerical trend.

The century gate uses a compact atlas because 393,216 cells x 1,244,160 hours
would turn a repository test into days of redundant computation; algorithms
and transfer paths are unchanged. Production resolution is separately proven
by atlas generation/export, 44 evolved live hours, a 12-hour whole-atlas
probe, a ten-hour server/chunk interleaving probe, and exact persisted audits.
Tracked materials are static in an undisturbed climate run and are therefore
qualified by their separate production ledger audit rather than copied into
the climate fixture.

### Measured budgets

Review host: Linux/WSLg development machine, serial Cargo compilation to stay
inside memory, production runtime concurrency enabled. `/usr/bin/time -v` RSS
includes the complete process named below.

| Operation | Result |
|---|---|
| Production atlas seed 1337 | 10.98 s generation; 352,188 KiB generation peak; 383,916 KiB through export |
| Diagnostic bundle | 132 map/legend pairs, 290 files, 192 MiB; exporter 17.70 s; validation `passed` |
| Evolved production water audit | 1.06 s; 483,268 KiB; 3,458,370,298,210 HU / 636,403,126,228,736 salt; both deltas zero |
| Material audit | under 0.01 s; 8,288 KiB; balanced and progression-qualified |
| Production 12-hour weather | 8.62 s test body; 485,876 KiB; exact conservation |
| Ten-hour server/cold-stream interleave | 15.30 s test body; exact conservation |
| Dedicated host / agent | 478,024 KiB / 159,188 KiB peak RSS |
| Graphical remote full view 12 | 30.10 s to settled capture; 914,340 KiB; 518 resident / 498 GPU; settled sim 1.12 ms, draw 115.71 ms |
| Resident seam view 7 | 9.5 s; about 968 MiB; 149 resident / 135 GPU; 16 FPS capture |
| Cold natural-oasis view 12 | 72.92 s while responsive; 1,123,680 KiB; 504 resident / 485 GPU |
| Cold ocean horizon view 14 | 159.69 s; 1,341,484 KiB; 712 resident / 654 GPU |
| Persisted evolved save | 218 MiB total: 214 MiB planet state plus 4.1 MiB materialized chunks |
| Full serial repository test gate | 518 passed / 0 failed / 17 ignored in 658.34 s; 300,944 KiB peak RSS |
| Release build | 33.44 s; 1,985,780 KiB peak RSS during optimized linking |
| Release entry validator | 2.46 s; 814,244 KiB peak RSS; 25/25 homeland chunks ready, 0 dirty writes, exact water/salt, materials qualified |

The slow view-14 number is a deliberately cold 712-chunk horizon, not entry:
the exact 3x3 admission set becomes playable first. The performance work did
not reduce view distance or remove lighting/terrain. It eliminates per-chunk
remote relighting via WFC9, batches chunk adoption, paces the received queue,
meshes two chunks concurrently, bounds frontier neighbor waits to the granted
view, and prevents outer-ring dirty/remesh churn.

### Diagnostics and visual evidence

- `screenshots/planetary-atlas-seed-1337-v9/` contains the six-face maps,
  globe preview, census, manifests, transects, river profiles, weather tracks,
  qualification sites and exact water audit.
- `screenshots/planetary-qualification.toml` is the sidecar for corrected
  solo, remote, seam-building, seam-water, sea-horizon and natural-oasis
  captures. `tests::rendering::planetary_visual_capture_manifest_is_complete`
  checks metadata, PNG signatures, exporter status and all 132 map/legend
  pairs.
- Older `screenshot-goal7-*` images visibly contain the pre-fix culling bug
  and are not qualification evidence. They remain only as a historical defect
  record; the validated manifest excludes them.

### Deviations and limitations

- Full-view streaming remains CPU/memory-heavy on completely cold terrain,
  especially view 14. Admission is bounded and responsive, so this is an
  optimization target rather than a correctness or feature reduction.
- The renderer's 20 dirty chunks in the final remote telemetry are prepared
  residents outside the host-granted circular view, not visible holes or
  pending work. The settled-work predicate excludes them correctly.
- Visual before/after behavior such as pumped/recharged oasis and breached
  lake is asserted from exact state in deterministic scenarios; the final
  capture set does not duplicate every state assertion as a screenshot pair.
- The test-only atlas-free generator remains for compact legacy test fixtures
  and inert client mirrors as documented above. No selectable/server world can
  enter it.

### Verification commands

The final 2026-08-01 repository gate passed. The serial full suite reported
518 passed, zero failed and 17 deliberately ignored production probes in
658.34 seconds with 300,944 KiB peak RSS. The ignored probes are represented
by the separately executed production atlas/export, weather, streaming and
graphical evidence above. Clippy passed with warnings denied, formatting and
the advisory audit passed, and the release build completed in 33.44 seconds.
The resulting release binary then ran `--validate-entry saves/entry-v9`
against the production qualification save: all 25 common-spawn chunks loaded,
zero dirty chunks needed writing, water and salt deltas were exactly zero, and
the material audit remained balanced and progression-qualified.

```sh
CARGO_BUILD_JOBS=1 cargo fmt --all -- --check
CARGO_BUILD_JOBS=1 cargo clippy --locked --all-targets -- -D warnings
CARGO_BUILD_JOBS=1 cargo test --locked --all-targets -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo build --locked --release
cargo deny check advisories
```

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

Also audit every requirement and completion criterion in
`docs/world-entry-streaming-plan.md`. Planet creation is not qualified if its
atlas succeeds but solo, dedicated, graphical, or agent entry can still block
on synchronous cold chunk generation.

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
