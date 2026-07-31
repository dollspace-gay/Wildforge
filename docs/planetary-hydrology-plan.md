# Planetary hydrology — every river goes somewhere

> **Status: implementation design complete, not implemented.**
>
> This is goal 5 of the planetary-world sequence. It requires completed
> topology, atlas, geology, and climate goals.

## Purpose

The present rivers are locally carved noise bands and lakes are local noise
basins. They can look river-like, but they do not begin in catchments,
accumulate tributaries, descend through a watershed, cross continents, and
reach an ocean or terminal lake.

This goal solves the finite planet's static hydrological geography:

- connected oceans and marine datum,
- terrain-conditioned drainage,
- watersheds and tributaries,
- erosion-shaped river valleys,
- lakes with catchments and spill points,
- endorheic basins, salt lakes, deltas, wetlands, and floodplains,
- baseline surface-water volumes and salinity,
- seamless voxel materialization.

Dynamic rain, evaporation, groundwater recharge, spring discharge, and
player-driven level changes land in the following water-cycle goal.

## Hydrological elevation

Begin from planetary geology's preliminary elevation. Hydrology owns a
bounded erosion loop:

1. resolve drainage receivers,
2. accumulate climate-normal runoff,
3. erode high-discharge/high-slope paths,
4. deposit in low-energy basins and coasts,
5. recompute slopes and drainage,
6. stop on convergence or a hard iteration limit.

Fine voxel noise may decorate a valley but cannot dam or reverse a major
atlas river unless the atlas declares a dam, lake sill, or waterfall.

## Ocean solution

Marine sea level and baseline water volume are global.

- Identify all below-datum connected components.
- Require one dominant connected world ocean.
- Permit enclosed seas where topography supports them.
- Record every ocean basin, connection sill, volume-elevation curve, and
  salinity baseline.
- Fill baseline ocean water conceptually across the whole finite planet,
  including unmaterialized chunks.

The dynamic water cycle stores deviations from genesis. Generating an ocean
chunk reveals its assigned water; it does not create new mass.

## Drainage graph

Use the atlas's seam-aware eight-neighbor graph.

For every land cell, determine one of:

- downstream receiver,
- lake interior,
- ocean outlet,
- terminal endorheic sink.

Use a deterministic priority-flood/depression analysis to distinguish:

- numerical pits that should drain through a spill,
- real tectonic or arid basins that should retain a lake or playa,
- volcanic craters,
- karst-loss candidates deferred to groundwater.

Flats receive deterministic drainage directions that do not produce loops.

Required invariants:

- every downstream chain terminates,
- no directed cycle exists outside an explicitly modeled lake body,
- open-basin paths reach an ocean,
- terminal basins have a finite storage/spill relation,
- all edges work across cube-face seams.

## Runoff and discharge

Climate normals supply precipitation, snowmelt timing, and potential
evapotranspiration. Geology/soil placeholders supply infiltration. Compute
mean and seasonal runoff for each cell.

Flow accumulation gives channel discharge. Use discharge and slope to derive:

- channel existence,
- bankfull width,
- mean depth,
- stream order,
- sediment energy,
- perennial/intermittent classification.

The smallest mapped channels may become damp gullies rather than continuous
voxel water. Major rivers are wide persistent bodies. Dry-country rivers may
be episodic and lose water into aquifers.

Tributaries join; rivers do not independently cross as noise lines.

## Rivers and valleys

Carve atlas valleys from discharge, substrate resistance, and slope:

- narrow steep headwaters,
- waterfalls or rapids at resistant steps,
- widening downstream valleys,
- meanders on low-gradient floodplains,
- alluvial terraces,
- deltas or estuaries at low-energy coasts,
- canyons where uplift and resistant rock support incision.

Meander detail may be procedural below atlas scale, but endpoints and
downstream order remain fixed.

Channel bed elevation descends monotonically after lake surfaces and explicit
falls are accounted for.

## Lakes and closed basins

Every lake has:

- catchment,
- basin hypsometry (volume by surface elevation),
- baseline inflow,
- evaporation rate supplied by climate,
- groundwater exchange coefficient reserved for the next goal,
- outlet and spill elevation if exorheic,
- salinity baseline,
- seasonal level range.

Lake classes:

- through-flow freshwater lake,
- terminal freshwater lake with high inflow,
- saline terminal lake,
- seasonal playa,
- rift lake,
- volcanic crater lake,
- glacial/alpine lake.

Lakes are not forced to a four-block terrace. Their voxel surface is level in
embedded gravitational potential; quantization appears only at block
resolution.

## Wetlands, floodplains, and deltas

Mark habitat overlays where:

- groundwater or lake level is near the surface,
- river slope is low and flooding recurs,
- deltaic sediment accumulates,
- coastal terrain is shallow and protected.

These masks feed the biome goal. They do not assign a whole province as swamp.

Floodplains reserve lateral space for dynamic high water. Ordinary seasonal
flooding may alter natural ground and vegetation but retains Wildforge's
absolute rule that the wild does not rewrite player-built blocks. Water may
still physically enter an unprotected structure through open space.

## Salinity genesis

Water has a salinity concentration:

```text
0     = fresh
1..63 = slightly mineral/brackish
64..191 = saline
192..255 = ocean/brine
```

Baseline:

- precipitation and snow are fresh,
- rivers begin fresh and acquire dissolved load,
- connected oceans are saline,
- terminal lakes concentrate according to inflow/evaporation,
- evaporite basins may become brine or dry salt flats,
- estuaries are brackish.

The next goal conserves salt mass during mixing, flow, freezing, and
evaporation.

## Voxel materialization

Atlas channels become exact voxel terrain:

- coarse valley and water-surface constraints are sampled first,
- fine channel centerlines connect across chunks,
- bed and bank blocks follow discharge, geology, and sediment,
- water level and salinity initialize from the atlas,
- lakes and oceans remain sealed by generated terrain,
- chunk boundaries and cube-face seams share channel geometry,
- caves are not automatically filled merely because they lie below sea level;
  groundwater handles subsurface saturation later.

Materialized volume matches the atlas baseline accounting. Where atlas
precision and voxel quantization differ, a recorded residual stays in the
basin's coarse storage rather than creating or deleting water.

## Player changes and genesis

The immutable atlas is the original land, not a command to restore it.

- Digging a canal may join basins dynamically.
- Breaking a natural dam may drain a lake.
- Building a dam may impound a river.
- Voxel edits override genesis and persist.
- Atlas drainage remains a broad routing prior; dynamic surface routing near
  player changes belongs to the water-cycle goal.
- No load or reconcile pass rebuilds an atlas river through a player
  structure.

## Existing-system integration

- Boats follow connected water and gain meaningful continental routes.
- Fish and aquatic habitats read discharge, temperature, depth, and salinity.
- Clay and placer minerals prefer depositional environments.
- Monazite placers trace source geology through drainage.
- Waystones and atlas diagnostics can name rivers, lakes, seas, and
  watersheds deterministically.
- Hearts/provinces use watersheds in the biome goal.
- Finite water blocks retain their 1–8 visible volume levels.

## Diagnostics

Export:

- depression and basin map,
- drainage receivers,
- watersheds,
- flow accumulation and stream order,
- river centerlines and discharge,
- lakes by class and spill point,
- seasonal water range,
- ocean basins and connections,
- floodplain/wetland/delta masks,
- salinity.

For selected rivers, export longitudinal profiles: source elevation, bed
elevation, discharge, width, tributaries, lakes, and mouth.

## Required tests

### Graph correctness

- Every drainage path terminates in ocean, lake, or declared terminal basin.
- No undeclared cycle exists.
- Open-basin river bed elevations descend to the sea.
- Flat resolution is deterministic.
- Drainage crosses all face seams and corners.

### Geographic causality

- Tributary discharge increases downstream.
- River width/depth statistically increase with discharge.
- Wet climates generate denser perennial networks than arid climates.
- Rain-shadow basins produce sparse or intermittent networks.
- Large closed arid basins form playas or saline lakes.
- Resistant uplifted terrain forms narrower/steeper valleys than soft
  lowlands.

### Oceans and lakes

- Global ocean connectivity and land-share constraints remain satisfied.
- Every lake has valid volume/elevation and catchment data.
- Exorheic lakes have one deterministic outlet at or above their surface.
- Baseline lake input/output closes within tolerance.
- Salinity class follows basin closure and evaporation.

### Voxel realization

- A river remains connected across chunks and cube faces.
- Voxel beds never climb contrary to the atlas profile.
- Generated banks hold baseline water at rest.
- Materialized baseline volume plus residual equals atlas volume.
- Chunk order does not change channels, lakes, deltas, or salinity.

### Visual qualification

Review:

- a mountain source with tributaries,
- a river crossing several geological regions,
- a wet windward network and dry lee basin,
- a through-flow lake and its outlet,
- a terminal salt lake/playa,
- a delta or estuary,
- a river crossing a cube-face seam.

## Completion criteria

This goal is complete when:

- the complete planet has a valid drainage graph,
- rivers arise from accumulated runoff and reach real sinks,
- lakes have basins, levels, outlets, and salinity,
- oceans are globally known finite reservoirs,
- erosion and deposition shape valleys and coasts coherently,
- voxel rivers/lakes/oceans materialize seamlessly without creating mass,
- all graph, geography, lake, ocean, salinity, determinism, volume, seam, and
  visual tests pass.

Do not mark this implemented because blue lines connect on an atlas map.
Their elevations, catchments, discharge, voxel channels, and water budgets
must agree.
