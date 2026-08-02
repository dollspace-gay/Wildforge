# Planet atlas — global knowledge before local chunks

> **Status: implemented and qualified (2026-07-31).**
>
> This is goal 2 of the planetary-world sequence. It requires the completed
> coordinate and topology work in `docs/planet-topology-plan.md`.

## Implementation record — 2026-07-31

**Goal 9 closure (2026-08-01).** The architecture below remains current, but
its intermediate format measurements are historical. The integrated build is
generator 9 / protocol 24, keeps WFA6/WFD3/WFW1 and atlas algorithm 6, writes
WFC8 chunks to disk, and emits WFC9 chunks only on the network. A fresh
production seed-1337 export generated 393,216 cells in 10.98 seconds at
352,188 KiB generation peak RSS and emitted 132 validated map/legend pairs;
the complete export peaked at 383,916 KiB. See the final qualification record
and `screenshots/planetary-atlas-seed-1337-v9/`.

The implementation follows the fixed production dimensions in this document:
six `256 × 256` faces, `393,216` cells, and 32 logical surface columns per
atlas cell. Immutable genesis, mutable planetary state, and sparse history are
separate typed owners and separate persisted files. At the original Goal 2
qualification, the on-disk formats were `WFA1` for genesis and `WFD1` for
dynamic state; atlas format, generation algorithm, dynamic-state, and history
schema versions were all `1`. The world generator version was `2`, making the
intended pre-atlas format break explicit.

Measured production qualification for seed `1337`:

- headless creation and the complete diagnostic export took 2.49 seconds,
- peak resident memory was 220,584 KiB (215.4 MiB), with zero swaps,
- immutable genesis was 42,860,576 bytes (40.9 MiB),
- dynamic state was 20,447,264 bytes (19.5 MiB),
- estimated loaded atlas memory was 66,060,288 bytes (63.0 MiB),
- all 393,216 cells, six faces, 42 registered maps, 42 legends, a globe
  preview, TOML/CSV census, manifest, and validation report were emitted.

Qualification passed formatting, strict Clippy, a release build, the advisory
audit, and the complete repository test suite: 398 passed, zero failed, and 12
explicit development/benchmark probes ignored. The atlas-specific suite covers
serial/parallel determinism, every seam and corner, persistence and bounded
decode failures, immutable corruption refusal, dynamic recovery, migration,
atomic cancellation, export completeness, and chunk-order independence.

Goal 3 subsequently expanded the immutable geological record. Its
qualification values were `WFA2`, atlas format and algorithm versions `2`,
geology schema version `1`, and world generator version `3`; immutable genesis
was 60,948,512 bytes at production resolution.

Goal 4 subsequently expanded the immutable seasonal-climate record and the
mutable local-weather record. Its qualification values were `WFA3`/`WFD2`,
atlas format and algorithm versions `3`, world generator version `4`, and
multiplayer protocol `20`.

Goal 5 subsequently supplied routed hydrology, Goal 6 closed the dynamic
water cycle, and Goal 7 added causal soil/biome/country state. Current shipping
values are `WFA6` for genesis, `WFD3` for climate state, `WFW1` for exact water
state, and `biomes.wfb` schema `1`; atlas format and algorithm versions are `6`,
world generator version was `7`, chunk/save/network records were `WFC8`, and
multiplayer protocol was `23`. Generator 9 / protocol 24 now retain WFC8 for
disk but use WFC9 for host-streamed chunks. The measurements below and the former
127,402,016-byte `WFA5` result remain historical qualification records, not
the current save format.

At the original Goal 2 qualification, scientific values were deliberately
deterministic placeholders, as this goal permits. Goal 3 replaced plate
mechanics, continents, strata, and geological manifests; Goal 4 replaced
climate normals and global weather with causal seasonal fields and conserved
local weather; Goal 5 replaced placeholder drainage with finite oceans,
routed rivers, lake basins, salinity, and voxel volume accounting; Goal 6
connected those baselines to exact atmosphere, soil, snow, aquifer, runoff,
surface, portable, and industrial reservoirs. Ecological biomes and exact
material lifecycles remain owned by Goals 7–8. The base atlas architecture
provides independent persistence, recovery, and bounded sliced iteration, but
the Goal 6 implementation record is the proof that rain now recharges
aquifers and the dynamic water/salt ledger closes.

## Purpose

An infinite chunk generator must answer every question from local coordinates.
A finite planet does not. It can know its continents, watersheds, climate,
aquifers, hearts, and mineral provinces before any voxel chunk is requested.

The planet atlas is a persistent, coarse, whole-world model from which later
geology, climate, hydrology, water-cycle, and biome goals derive their local
answers. It is also the headless test surface for deciding whether a planet is
credible without flying through billions of block cells.

This goal builds the atlas infrastructure, persistence, sampling, diagnostics,
and versioning. It initially fills fields with deterministic placeholders.
Later goals own the scientific algorithms.

## Fixed resolution

```text
ATLAS_CELL_BLOCKS = 32
ATLAS_FACE_SIDE   = FACE_BLOCKS / 32 = 256
ATLAS_CELL_COUNT  = 6 × 256 × 256 = 393,216
```

One atlas cell covers `32 × 32` logical surface columns. This is fine enough
to route regional rivers and rain shadows while cheap enough to update as one
planet.

Atlas coordinates use the same six-face topology as chunks. Edge and corner
neighbors come from the shared topology module at atlas resolution.

Cube-sphere cells have slightly different embedded surface areas. Precompute
and store each cell's spherical area/Jacobian. Climate, water, resource
density, and global census totals weight by that physical area rather than
giving cube corners extra planetary influence because they own more logical
cells per square kilometer.

## Immutable and dynamic layers

Do not put every field into one anonymous struct. Ownership and persistence
frequency differ.

### Immutable genesis

Generated once at world creation:

```text
unit direction, latitude, and physical surface area
plate id and boundary classification
continental-crust fraction and crust age
bedrock family / geological province
base elevation and eroded elevation
ocean-basin id
drainage receiver and watershed id
lake basin and spill elevation
mean temperature and seasonality
mean precipitation and aridity
prevailing wind
soil parent material
aquifer capacity and permeability
baseline groundwater head
baseline biome and local habitat flags
province and heart assignment
finite deposit/site references
```

Later documents define these fields precisely. This goal establishes typed
layer containers and safe absence/default behavior while the fields are
introduced.

### Dynamic planetary state

Persisted independently and updated during play:

```text
atmospheric vapor and cloud water
soil moisture
snowpack
groundwater volume and head anomaly
surface runoff
lake and ocean storage anomaly
local weather anomaly
fire/vegetation moisture anomaly where required
```

Dynamic fields must not force immutable atlas rewrites on every save.

### Player-history overlays

Existing regional ledgers remain sparse:

```text
ire and tending
heart state
player-touched cells
bloom and exhaustion
named places and waystones
resource extraction
mod retrogen stamps
```

They reference atlas cells or planetary positions but are not baked into
genesis.

## Storage and versioning

World creation writes:

```text
saves/<world>/planet/genesis.wfa
saves/<world>/planet/dynamic.wfd
saves/<world>/planet/history.wfh
saves/<world>/planet/geology.wfg
saves/<world>/planet/hydrology.wfy
saves/<world>/planet/manifest.toml
```

Names may change during implementation, but immutable and dynamic data remain
separate.

The manifest records:

- world seed,
- topology version,
- atlas resolution,
- each layer's schema version,
- generator algorithm versions,
- planet parameters,
- content/mod-set hash used at genesis,
- checksums for atlas payloads.

Atlas files use fixed-width, explicitly endian versioned data or a
self-describing format with bounded decoding. They are written atomically.
Loading validates dimensions and checksums before allocating.

Immutable genesis is persisted, not silently regenerated from newer code.
This prevents an update from moving a mountain underneath saved chunks.

## Layer API

Provide typed grids:

```text
AtlasGrid<T>
AtlasCell
AtlasPos { face, u, v }
```

Required operations:

- exact cell lookup,
- four- and eight-neighbor iteration across seams,
- bilinear scalar sampling across seams,
- normalized vector sampling in local tangent frames,
- bounded stencil walks,
- global deterministic iteration,
- connected-component labeling,
- geodesic radius queries,
- upstream/downstream graph traversal,
- immutable layer fingerprints.

Interpolation across a face edge transforms tangent vectors into the
destination frame before blending.

Chunk generation queries through a `PlanetAtlas` interface. It does not know
file layout and does not reach into unrelated layer vectors.

## Generation pipeline contract

The complete sequence will fill the atlas in this order:

```text
topology constants
→ tectonics and crust
→ preliminary elevation
→ climate normals and winds
→ erosion, basins, and drainage
→ hydrological equilibrium
→ soils, groundwater, and habitats
→ biomes and provinces
→ finite geological/resource sites
→ validation and immutable write
```

Some goals iterate—hydrology erodes elevation, then recomputes drainage—but
the committed result has one explicit dependency order and no chunk-load
dependency.

Every stage:

- reads only completed predecessor layers,
- writes layers it owns,
- is deterministic for seed and version,
- has a stage checksum,
- can be run from the headless atlas tool,
- validates before the next stage begins.

## World creation

Creating a world becomes an explicit job with progress:

```text
SHAPING PLANET
RAISING CONTINENTS
MOVING AIR
FINDING THE WATERS
LAYING THE GROUND
WAKING THE COUNTRIES
```

The exact presentation may land later, but world creation may take seconds
rather than hiding global generation behind the first chunk.

- It is cancellable before the world becomes selectable.
- A partial atlas is never treated as a valid world.
- Creation uses a temporary directory and atomic rename on success.
- Headless server creation prints stage progress.
- Tests may request reduced-resolution fixture planets through a test-only
  constructor. Production constants never shrink under environment flags.

## Lazy voxel materialization

The atlas describes ungenerated country. Voxel chunks still materialize on
demand.

- A new chunk samples immutable atlas fields plus deterministic fine detail.
- Atlas fields agree across chunk and cube-face boundaries.
- Saved chunks override regenerated material exactly as today.
- Global ocean, lake, groundwater, and resource budgets already include
  unmaterialized chunks.
- Materializing a chunk transfers no mass and consumes no deposit; it merely
  reveals baseline state.
- Player edits are deltas from genesis and persist normally.

## Diagnostics

Add a headless command:

```text
wildforge --generate-atlas <seed> --output <directory>
```

It produces:

- manifest and validation report,
- one image per scalar/categorical layer using a documented six-face layout,
- optional CSV summaries,
- a low-resolution globe preview,
- stage timings and memory use,
- water and material budgets once those layers exist.

Use a simple built-in image format or existing dependency; diagnostics must
not require a network service or proprietary tool. Colors and legends are
stable so atlas changes are reviewable in version control when desired.

Required maps by the end of the full sequence:

- plates, crust, boundary types,
- elevation and ocean depth,
- rock provinces and deposit sites,
- seasonal temperature,
- prevailing wind,
- annual/seasonal precipitation,
- aridity,
- drainage, watersheds, rivers, and lakes,
- soil moisture, aquifer head, springs,
- salinity,
- biomes, habitats, provinces, and hearts.

## Query and census tools

The atlas exposes deterministic reports:

- land/ocean fraction,
- continent and island counts,
- elevation histogram,
- plate/boundary counts,
- climate-zone areas,
- biome areas by latitude and continent,
- river length/discharge distributions,
- lake and watershed counts,
- freshwater and total-water budgets,
- aquifer storage,
- resource quantities and nearest-distance bands,
- province/heart counts.

These reports are qualification evidence, not debug prose. Tests pin ranges,
relationships, and invariants rather than one artistically arbitrary exact
map.

## Performance budgets

Production atlas target:

- generation peak memory below 512 MiB,
- persisted immutable atlas below 128 MiB,
- dynamic planetary state below 64 MiB,
- ordinary scalar lookup allocation-free,
- full one-step dynamic scan capable of completing comfortably within one
  in-game hour when sliced across server ticks,
- no atlas work proportional to voxel count.

The exact final sizes are recorded in an implementation header when shipped.
Exceeding them requires a design note, measurement, and explicit decision.

## Failure handling

- Corrupt immutable genesis makes the world unavailable with a precise error;
  it does not regenerate beneath saved chunks.
- Corrupt dynamic state restores from its last atomic backup if available.
- Unknown newer layer versions are refused.
- Missing optional later-goal layers report the unmet planetary goal during
  development; production worlds require the final qualification schema.
- No atlas decode trusts file-provided lengths without checking the fixed
  planet dimensions.

## Required tests

### Grid and topology

- Every atlas edge sample agrees from both faces.
- Scalar interpolation is continuous across seams.
- Tangent vectors rotate correctly across seams.
- Connected components cross seams and corners.
- Radius queries near an edge equal equivalent interior queries.

### Determinism and order

- Same seed/version/content hash yields byte-identical genesis.
- Parallel and serial stage execution yield identical results.
- Chunk generation order does not alter atlas or chunk output.
- Saved genesis survives generator-code changes in a fixture migration test.

### Persistence and safety

- All layers and manifests round-trip.
- Truncated/corrupt files fail boundedly.
- Atomic creation never exposes a partial selectable world.
- Unknown versions are refused clearly.

### Budgets and tools

- Production-size blank/placeholder atlas satisfies memory budgets.
- The headless command exports every registered layer with a legend.
- Reports cover all six faces and totals equal the known cell count.
- Test-size worlds exercise the same algorithms, not alternate mock logic.

## Completion criteria

This goal is complete when:

- a production-resolution finite atlas is created and persisted for every new
  world,
- typed immutable, dynamic, and history layers have separate ownership,
- all sampling and graph operations work across cube-sphere seams,
- chunk generation consumes atlas queries without depending on chunk order,
- the headless exporter and census report work,
- generation progress and atomic failure handling work in windowed and
  headless modes,
- version, corruption, determinism, seam, memory, and export tests pass.

Placeholder scientific fields are allowed at this stage. Missing atlas
infrastructure, persistence, or diagnostics are not.
