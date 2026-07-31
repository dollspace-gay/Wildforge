# Planetary water cycle — water leaves, travels, and returns

> **Status: implementation design complete, not implemented.**
>
> This is goal 6 of the planetary-world sequence. It requires completed
> topology, atlas, geology, climate, and hydrology goals.

## Purpose

Visible voxel water is already finite, but the current “cycle” removes shallow
water during evaporation and creates water during rain. There is no
atmospheric reservoir, soil moisture, groundwater, or return path from steam
and snow. On an infinite world those tuned effects can look seasonal. On a
finite planet they would drift, drain, or mint water forever.

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
