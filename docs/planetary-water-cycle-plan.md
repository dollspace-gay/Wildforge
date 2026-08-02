# Planetary water cycle — water leaves, travels, and returns

> **Status: implemented and qualified (2026-07-31).**
>
> This is goal 6 of the planetary-world sequence. It requires completed
> topology, atlas, geology, climate, and hydrology goals.

## Implementation record — 2026-07-31

**Goal 9 closure (2026-08-01).** WFW1 and the one-HU=`1/256`-voxel contract
remain current in generator 9. The evolved production save closes at
3,458,370,298,210 HU and 636,403,126,228,736 salt mass after 44 climate hours,
with zero unexplained delta. Live testing found and fixed saturated transport
spill and sliced-weather/detail-inbox interleaving defects. The configurable
headless long-run harness defaults to 200 144-day years, reports the required
reservoir/climate/habitat/memory metrics at years 1, 10, 100, and final, and
uses the production weather, surface, groundwater, basin, and audit paths on
a compact cube-sphere so the repository gate remains practical.

Goal 6 closes the loop between the conservative atmospheric weather added by
Goal 4, the finite basins and voxel water assigned by Goal 5, and every
shipping gameplay path that moves water. One hydro unit (HU) is exactly
`1/256` of a full voxel block, and every water owner carries integer HU plus
integer salt mass. Transfers debit before crediting, preserve indivisible
remainders deterministically, and are rejected when their source cannot fund
them. The ledger distinguishes atmosphere, soil, snow and ice, groundwater,
runoff, named coarse surface reservoirs, chunk-committed voxel water, pending
exchanges, three portable water classes, industrial storage, and precipitated
salt.

The authoritative server advances surface/weather fluxes hourly and
groundwater daily in deterministic slices. Rain and snow debit clouds;
evaporation and transpiration credit vapor; infiltration fills soil and then
aquifers; runoff follows the seam-crossing drainage graph; groundwater moves
down head gradients; aquifers feed eligible perennial rivers and finite
springs. Freeze/melt, salt rejection, brine concentration, terminal-lake salt
precipitation, breached basins, and river/ocean mixing all preserve exact mass.
An opened mine below the water table can receive groundwater seepage, and an
excavation beneath mapped ocean country materializes a finite parcel debited
from that ocean.

Detailed/coarse ownership is explicit. Fresh chunks debit the exact local
water and salt assigned by immutable hydrology and register a commitment to
the same named ocean, lake, or river; local salinity is not replaced by a
basin average. Saved chunks retain detailed authority. Inbox transfers bridge
unloaded country, while shoreline reconciliation follows basin level without
overwriting player-touched chunks. Waterfront excavation or masonry creates a
stable chunk-local dynamic-basin marker, so player waterworks have an audited
owner even before they contain water.

Buckets now preserve fresh, brackish, or salt-water identity. Pumps debit
visible named water or groundwater, boilers own their intake, and exhausted
steam returns fresh vapor while dissolved salt remains in the machine. Voxel
metadata and multiplayer block updates carry salinity, and host/guest chunk
snapshots agree on fluid level, salt metadata, and hydrological ownership.

### Formats and recovery

- immutable genesis is `WFA6`; atlas format and generation algorithm are `6`,
- mutable climate is `WFD3`, dynamic schema `3`,
- exact water state is the new independently bounded `WFW1` file,
- causal country/biome state is `biomes.wfb`, schema `1`,
- history, geology, hydrology, and water-cycle schemas remain `1`,
- the Goal-6 shipping point used world generator `7`, WFC8 chunk/network
  records, and protocol `23`; integrated qualification now uses generator 9,
  WFC8 on disk, network-only WFC9, and protocol 24.

The dynamic and water files are published as one logical save generation.
Load accepts only a checksum-consistent pair, recovers the last atomic backup
pair when possible, and otherwise fails closed instead of combining weather
from one hour with reservoirs from another. Older immutable, dynamic, water,
chunk, world-generator, and protocol versions are not silently reinterpreted.

### Production qualification

The release build generated production seed `1337`, all six `256 × 256` atlas
faces and the full diagnostic bundle in 28.78 seconds. Peak resident memory
was 513,520 KiB (501.5 MiB), with zero swap: inside the 512 MiB generation
budget by only 10,768 KiB, so the margin is valid but tight. Persisted files
were 127,402,016 immutable bytes, 12,582,952 dynamic bytes, 31,670,157 water
bytes, 134,448 geology bytes, 2,755,408 hydrology bytes, and 180 history bytes.
Estimated loaded atlas memory is 176,622,896 bytes.

The generated headless audit closed exactly at 3,542,941,318,509 HU and
657,333,643,306,016 units of salt mass, with zero unexplained water and salt,
zero pending exchanges, and named levels/salinity for every ocean, lake, and
river. Reloading and rendering that same production world left the persisted
audit unchanged. Diagnostics include water/salt reservoir totals, basin
levels, largest aquifer drawdowns and pending exchanges, plus groundwater
recharge, spring discharge, spring climate, and salinity maps. Operators can
repeat the proof with:

```sh
wildforge --water-audit <world>
```

The 19-test water-cycle suite covers individual transfer conservation,
two-century stability, save boundaries, serial/sliced equivalence, seams,
salinity mixing, freezing, terminal salt, rain, snow, aquifers, springs,
baseflow, drawdown, breaches, dynamic basins, ocean cave flooding, bucket and
boiler classes, detailed commitments, and unloaded exchange. Related machine,
world, hydrology, rendering, persistence, and multiplayer suites exercise the
shipping integration.

The final split all-target repository run passed 467 tests with zero failures
and 13 intentional development/measurement probes ignored: 463 non-agent
tests, followed by all four agent tests on one test thread. The default
parallel non-agent runner peaked at 3,926,324 KiB even though compilation used
`CARGO_BUILD_JOBS=1`; this confirms that constrained runners must also use
`--test-threads=1`. The serial agent lane peaked at 115,584 KiB.
Formatting, strict locked all-target/all-feature Clippy with warnings denied,
the locked release build, dependency advisories, and the whitespace audit all
pass.

### Honest limitations

The production atlas generator passes its memory budget, but the real release
client used 797,644–903,924 KiB while rendering these atlas-heavy water sites;
textures, meshes, renderer allocations, and the loaded planet all contribute.
That is outside the atlas-generation budget, but it is relevant to the earlier
out-of-memory report and leaves meaningful client-memory work for final
planetary qualification.

The real-client lake and estuary captures also exposed a presentation defect:
in bright conditions broad water surfaces can blend almost completely into
the sky, while the existing short-distance fog, exposed caves, and stark
terrain silhouettes make valid shores look fragmented. The world loaded,
settled, and rendered real atlas water, and its mass audit stayed exact; the
captures are technical evidence, not promotional screenshots. V1 dynamic
player basins are stable per-chunk ownership markers rather than a global
arbitrary-dam connectivity solver. These limits do not relax conservation,
but they should not be mistaken for finished water presentation.

## Purpose

Before Goal 6, visible voxel water was finite, but the old “cycle” removed
shallow water during evaporation and created water during rain. It had no
authoritative atmospheric reservoir, soil moisture, groundwater, or return
path from steam and snow. On an infinite world those tuned effects could look
seasonal. On a finite planet they would drift, drain, or mint water forever.

This goal closes the planetary water cycle while retaining detailed voxel
water where players interact.

## Governing invariant

In a sealed test planet:

```text
ocean water
+ visible surface/subsurface voxel water
+ lake coarse residuals
+ soil moisture
+ groundwater
+ atmospheric vapor/cloud
+ snow and ice
+ water in inventories, containers, and machines
= constant total water mass
```

Salt mass has an equivalent invariant. Ordinary processes only transfer mass.
Explicit test/dev commands may create or destroy it and must be labeled.

Floating-point conservation is not sufficient. Reservoir accounting uses
fixed-point integers.

## Units

Define:

```text
1 hydro unit (HU) = 1/256 of a full voxel water block
1 visible fluid unit = 32 HU
1 full voxel cell = 256 HU
```

Coarse reservoirs store `u64` HU. Signed temporary fluxes use checked wider
integers. Every transfer debits before it credits and is bounded by available
source and destination capacity.

Quantizing coarse water into a visible 1/8-block fluid level leaves the
remainder in the owning coarse reservoir.

Salt is stored as integer salt mass alongside water mass. Salinity is derived,
not transferred as an unweighted color.

## Reservoirs

### Atmosphere

Per atlas cell:

- vapor mass,
- cloud-liquid/ice mass,
- capacity derived from temperature,
- dynamic precipitation-ready mass.

Climate moves these masses through conservative advection and condensation.

### Soil

Per atlas cell:

- field capacity,
- current moisture,
- infiltration rate,
- evaporation access,
- vegetation-root access.

Soil capacity derives from texture, organic content, slope, and depth.

### Groundwater

Per atlas cell and relevant aquifer layer:

- storage capacity,
- current water volume,
- hydraulic head,
- permeability/transmissivity,
- connection to neighboring aquifers,
- connection to surface water,
- salinity.

The first implementation may cap vertical aquifer layers at three:

1. shallow unconfined,
2. perched where geology supports it,
3. deep/confined.

Do not represent pore water as millions of hidden voxel fluid blocks.

### Surface water

- Visible loaded/generated voxel cells remain exact.
- Unmaterialized baseline water remains in atlas basin/ocean accounting.
- Lake and ocean records store quantization residual and volume/elevation
  curves.
- Dynamic edits persist as normal chunk state and basin deltas.

### Snow and ice

- Coarse snow water equivalent stores sub-voxel accumulation.
- Visible snow layers and ice debit that reserve when materialized.
- Melt returns the exact stored water and salt relation.
- Sea ice rejects most salt back to underlying water.

### Portable and industrial water

Buckets, machine banks, boilers, and future tanks are reservoirs in the audit.
Save and network formats preserve their water and salinity class.

## Transfer processes

### Evaporation and transpiration

Evaporation:

- debits oceans, lakes, voxel surface water, or soil,
- credits atmospheric vapor in the same/coherent downwind cell,
- depends on temperature, wind, humidity deficit, exposure, and salinity,
- concentrates salt in the source,
- never special-cases deep water as immune.

Large/deep bodies persist because of volume, mixing, and climate balance—not
because a depth guard exempts them from physics.

Transpiration debits soil moisture and credits atmospheric vapor according to
vegetation and season. Dead-heart ecology may reduce vegetation and therefore
transpiration, but cannot delete water.

### Condensation and precipitation

- Cooling/saturation moves vapor to cloud water.
- Precipitation debits cloud water.
- Rain credits surface, soil, or interception stores.
- Snow credits snowpack and visible layers.
- Canopy interception may immediately re-evaporate a bounded share.
- Precipitation is fresh; dissolved salt does not evaporate from oceans.

Rain falls spatially according to the climate/weather goal. Deserts may receive
rare storms rather than a hard veto.

### Infiltration and recharge

Rain and surface water infiltrate according to:

- soil saturation and permeability,
- slope,
- vegetation,
- underlying rock permeability,
- frozen ground.

Water first fills soil storage, then percolates into an available aquifer.
Excess becomes runoff.

### Runoff and rivers

Coarse runoff follows the hydrology drainage graph until it:

- enters a materialized voxel channel,
- enters a lake/ocean reservoir,
- infiltrates,
- evaporates,
- waits as bounded surface storage.

Loaded voxel flows handle local dams, channels, breaches, and player terrain.
At the boundary of detailed simulation, exchange mass through explicit
flux records; never write into an unloaded chunk and hope.

### Groundwater flow

At a slower groundwater step, move water down hydraulic-head gradients,
bounded by transmissivity and available storage.

- Flow crosses cube-face seams.
- Confined layers may carry head above local surface.
- Ocean-connected aquifers can become saline near coasts.
- Heavy pumping creates a drawdown cone.
- Recharge raises head gradually.

### Springs and baseflow

A spring exists when:

- groundwater head exceeds the terrain outlet elevation,
- an aquifer or permeable layer is exposed,
- a fracture/fault provides a valid outlet,
- or a perched aquifer meets an impermeable boundary.

Spring discharge:

- debits groundwater,
- emits visible water when loaded,
- enters coarse runoff when unloaded,
- weakens or stops when head falls,
- recovers through recharge.

River baseflow uses the same aquifer exchange. Perennial rivers persist
through dry periods because groundwater feeds them; intermittent streams do
not.

### Freeze and melt

Freezing transfers water into ice/snow storage; thaw reverses it. It does not
swap a block while forgetting mass.

Lake/river ice formation depends on local temperature and flow. A full
one-block ice replacement is a presentation/materialization choice over a
tracked water equivalent.

### Plants, animals, and food

Biological water is below useful voxel scale. Treat short biological loops as
soil/atmosphere transfers within the coarse cell. Do not debit planetary
water for eating or drinking unless water becomes a portable item or machine
input.

### Machines

- Pumps move water; they do not create or destroy it.
- Pumping visible mine water may draw further seepage from an aquifer.
- Boilers debit stored water while running.
- Steam engines credit atmospheric vapor and/or a recoverable condensate
  output according to their design.
- Forges and fires do not silently consume water except explicit wetting or
  steam interactions.
- Future pipes/tanks register in the same audit interface.

## Voxel water and salinity

Retain water volume in the eight registered fluid levels. Use voxel metadata
for salinity concentration `0..255`.

Every fluid transfer moves both:

- water HU,
- proportional salt mass with deterministic remainder handling.

Mixing computes concentration from combined mass. Evaporation moves water
only. Freezing rejects most salt. Highly concentrated films may leave halite
or brine according to the hydrology/content rules.

Buckets require distinct fresh, brackish, and salt-water item identities
unless item-stack data lands first. Freshwater survival and farming may not
accept seawater interchangeably.

Lava remains outside the water/salt audit.

## Detailed/coarse exchange

One authority owns each mass at a time.

- A materialized voxel body's mass is marked committed to chunks/basin
  deltas.
- Coarse stores exclude committed mass.
- Loading a chunk materializes only its uncommitted baseline share.
- Unloading does not need to dissolve voxel detail back into the atlas;
  saved chunks retain authority.
- Coarse flux into an unloaded detailed region accumulates in an inbox.
- Loading applies inbox mass conservatively through channels, soil, or basin
  residual.
- Flux out of loaded chunks explicitly credits the destination coarse store.

Debug builds assert that no ownership interval overlaps.

## Dynamic lake and ocean levels

Every basin has a volume/elevation curve from the hydrology goal.

- Coarse inflow/outflow changes basin volume.
- Crossing a voxel elevation threshold wets or dries shoreline cells when
  they are loaded.
- Unloaded shores reconcile from basin state on load without touching
  player-built blocks.
- Player dams and excavations can create locally registered dynamic basins.
- Ocean level may change from truly planetary transfers, but a bucket has an
  immeasurably small effect retained as residual until thresholds accumulate.

Rainfall cannot refill only pre-existing partial water cells. It enters the
catchment, soil, aquifer, runoff, and basin whether the target chunk is loaded
or not.

## Time stepping and offline behavior

Suggested authoritative cadences:

- weather/advection: one in-game hour,
- surface runoff/basin exchange: one in-game hour,
- soil infiltration/evaporation: one in-game hour,
- groundwater flow: one in-game day,
- ecological moisture consumers: existing random/daily ticks.

Slice atlas work across server ticks with deterministic cursors. A complete
step uses the old state for all flux calculations and applies transfers in a
separate phase, preventing iteration-order bias.

The planet continues while chunks are unloaded and the server is running.
Server shutdown pauses simulation unless a later explicit real-time-server
policy says otherwise. Load reconciliation catches a chunk up to the current
planetary state; it does not simulate a second independent climate.

## Persistence and audit

Persist all dynamic reservoir fields atomically with world time. The save
contains:

- total initial water and salt,
- current total by reservoir class,
- any registered explicit source/sink from dev/admin operations,
- last completed climate/hydrology step,
- pending detailed/coarse flux inboxes.

Add a headless command/report:

```text
wildforge --water-audit <world>
```

It reports mass by reservoir, unexplained delta, basin levels, aquifer
drawdown, and largest pending exchanges.

## Existing gameplay changes

- Remove shallow-water random deletion and rain-fill creation.
- Replace hash-placed perpetual mountain seeps with atlas groundwater
  outlets.
- Flooded mines may recharge from aquifers.
- Rain barrels and irrigation become honest future consumers of the same
  transfers.
- Cellars may read soil/ground temperature but do not create water.
- Summer drought and autumn refill emerge from climate and storage.
- A heart's death may stop biological renewal; it cannot switch off rainfall,
  aquifer physics, or mass conservation.

## Required tests

### Exact conservation

- Sealed planet over hundreds of simulated years preserves total HU exactly.
- Every individual transfer preserves water and salt.
- Evaporation/precipitation round trips.
- Freeze/melt round trips.
- Loaded/unloaded exchange never duplicates or loses mass.
- Save/load at every transfer boundary preserves totals.

### Hydrological behavior

- A wet season raises soil moisture, aquifer head, and lake storage in order.
- Drought lowers shallow stores before deep groundwater.
- A spring weakens under drawdown and recovers after recharge.
- A perennial river receives baseflow; an intermittent one dries.
- Pumping creates a local drawdown cone.
- A breached lake moves exactly its lost volume downstream.
- An ocean-fed cave flood debits the ocean/basin accounting.

### Salinity

- Ocean evaporation increases source salinity.
- Rain is fresh.
- River/ocean mixing produces brackish water by mass.
- Freezing rejects salt.
- Terminal lakes concentrate and may precipitate salt.
- Bucket and machine persistence preserve water class.

### Stability and budgets

- Undisturbed climate-normal lakes stay within expected seasonal bounds for
  a century.
- No reservoir trends monotonically from numerical bias at equilibrium.
- Atlas updates remain within CPU/memory budgets.
- Parallel and serial flux calculation agree.
- Cube-face seams have no mass leak or preferred flow.

### Multiplayer and presentation

- Host and guest agree on voxel levels and salinity.
- Rain, shore changes, springs, pumps, and floods replicate.
- Visible water changes match basin/aquifer diagnostics.

## Completion criteria

This goal is complete when:

- all water and salt live in named, audited reservoirs,
- evaporation, rain, snow, infiltration, runoff, groundwater, springs,
  rivers, lakes, oceans, buckets, pumps, and steam transfer rather than mint
  or delete water,
- loaded and unloaded country share one authoritative cycle,
- springs and flooded mines respond to groundwater,
- basin levels reconcile to voxel shores without overwriting builds,
- the old random evaporation/rain-creation rules are removed,
- century-scale exact conservation, stability, persistence, seam, budget,
  multiplayer, and gameplay tests pass.

Do not mark this implemented from a stable lake screenshot. The audit must
prove where every hydro unit went.
