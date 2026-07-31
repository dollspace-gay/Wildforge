# Planetary geology — continents have a history

> **Status: implementation design complete, not implemented.**
>
> This is goal 3 of the planetary-world sequence. It requires
> `docs/planet-topology-plan.md` and `docs/planet-atlas-plan.md`.

## Purpose

The current generator has a good vocabulary—oceanic and continental plates,
convergence, rifts, trenches, volcanoes, strata—but evaluates it as local
planar noise. On a finite planet, geology can be globally coherent:
continents have outlines, ranges continue across the map, ocean basins have
ages, volcanic arcs sit on the correct side of trenches, and mineral hosts
belong to geological events.

This goal generates the planet's immutable physical substrate:

- plates and their relative motion,
- continental cratons distinct from plate boundaries,
- oceanic crust and basin age,
- tectonic relief,
- preliminary continents and oceans,
- globally continuous rock provinces and strata,
- volcanoes, intrusions, faults, and mineral hosts,
- the first elevation field consumed by later climate and hydrology.

It does not route rivers, assign final biomes, or run the dynamic water cycle.

## Fixed geological scale

Default production planet:

- 18 major plates, deterministically jittered within a range of 16–22,
- 6–9 continental craton groups,
- 4–7 major emerged continents after sea-level solution,
- numerous islands and volcanic arcs,
- target ocean coverage 62–70%, centered near 66% for playable land,
- at least one ocean belt that permits global marine circulation,
- no continent required to touch a cube-face center or avoid a seam.

Counts may vary by seed within tested bands. Re-rolling until a map passes is
allowed only through a deterministic seed-derived attempt sequence with a
strict maximum and a recorded chosen attempt.

## Spherical plates

### Plate sites

Place plate seeds over the unit sphere using a deterministic approximately
even distribution followed by bounded seed jitter and Lloyd relaxation.
Assign each atlas cell to its nearest seed by angular distance, producing a
spherical Voronoi partition.

No algorithm may treat cube-face edges as boundaries or distance shortcuts.

### Plate motion

Give each plate an Euler pole and angular speed. The local plate velocity is
the tangent velocity induced by rotation around that pole. At a boundary,
project relative velocity onto:

- boundary normal: convergence or divergence,
- boundary tangent: transform motion.

Classify connected boundary runs, not isolated cells:

```text
convergent continental/continental
convergent oceanic/continental
convergent oceanic/oceanic
divergent continental rift
divergent ocean ridge
transform
passive/weak
```

Boundary classification is stable along a range and tapers near junctions.

### Continents are not plates

Generate ancient buoyant continental cratons as a separate spherical field.
A plate may carry continental and oceanic crust simultaneously.

Cratons:

- begin as 6–9 clustered ancient nuclei,
- expand through deterministic accretion fields,
- may span a later plate boundary,
- carry thicker, more buoyant crust,
- have old interiors and younger margins.

Oceanic crust fills the rest and receives an age field spreading away from
divergent ridges. Age increases density and ocean depth until subduction or
the maximum age.

## Relief from causes

Build preliminary elevation from:

1. crustal buoyancy and thickness,
2. oceanic crust age,
3. long-wavelength mantle/dynamic-topography noise,
4. tectonic boundary deformation,
5. volcanic construction,
6. bounded fine regional variation.

Effects:

- Continental interiors ride above ocean basins.
- Continent–continent convergence raises wide fold belts with parallel
  ridges, foothills, and foreland basins.
- Oceanic subduction cuts a trench on the descending side and raises a
  volcanic/coastal arc on the overriding side.
- Ocean–ocean subduction creates island arcs.
- Continental divergence creates rift valleys, escarpments, and linear lakes
  for the hydrology goal to fill.
- Oceanic divergence creates ridges and young shallow seafloor.
- Transform runs create fault-aligned valleys and offsets without inventing
  a mountain wall everywhere.
- Hotspots create age-progressing volcanic chains independent of boundaries.

No single-cell spikes or one-cell trenches survive smoothing.

## Sea level and continents

Choose sea level from the target water/land fraction after tectonic relief,
not from a hard-coded elevation percentile hidden in chunk generation.

Validation requires:

- 4–7 major continents above a minimum area,
- islands on at least half of seeds,
- no one continent owns more than 65% of all land,
- at least one connected global ocean,
- polar land and ocean both possible across the seed suite,
- coastlines cross atlas and cube-face boundaries continuously.

The hydrology goal may adjust lakes and eroded terrain, but it does not move
the marine datum casually.

## Geological provinces and strata

Each atlas cell receives:

- crust type and age,
- dominant bedrock family,
- stratigraphic stack identifier,
- metamorphic grade,
- fault/fracture intensity,
- permeability and porosity seeds for groundwater,
- volcanic/plutonic history.

Chunk generation converts those fields into the existing voxel families:

- sedimentary stacks in basins and passive margins,
- granite and related plutons in continental arcs/interiors,
- basalt and ultramafic rock in oceanic/rift/volcanic settings,
- shale, sandstone, limestone, and evaporites from depositional environment,
- marble, slate, and quartzite in contact/regional metamorphic zones.

Strata remain visible in cliffs and fold coherently across chunks. Folding
uses the boundary's local strike and compression, not global planar axes.

### Sediment basins

Mark marine shelves, foreland basins, rifts, and closed continental basins.
Later hydrology adds active river sediment; genesis uses basin history to
place:

- limestone on suitable former shelves,
- shale in deeper quiet basins,
- sandstone along clastic margins,
- coal-bearing strata only where past wet lowlands are plausible,
- evaporites in repeatedly isolated arid basins.

This is geological history expressed as host rock, not a second decorative
ore noise.

## Volcanoes and intrusive bodies

Place volcanoes from coherent tectonic runs:

- continental volcanic arcs sit inland of their trench,
- island arcs parallel oceanic trenches,
- rift volcanism follows extension,
- hotspot chains progress in age and erosion along plate motion.

Every volcano has:

- edifice and crater parameters,
- magma chamber,
- intrusive dikes,
- rock chemistry class,
- hydrothermal alteration halo,
- erosion/age state.

Intrusions and batholiths follow arc and craton history. Contact metamorphism
uses intrusion geometry. Carbonatites and rare-earth hosts stay exceptional
and globally countable.

## Minerals and finite deposits

This goal places geological hosts and immutable deposit sites. The material
lifecycle is completed later by `docs/finite-materials-plan.md`.

Preserve existing causal chains:

- copper with basaltic/hydrothermal systems,
- tin with evolved granite,
- gold in quartz/hydrothermal veins,
- galena in appropriate limestone/marble contacts,
- chromite in deep mafic/ultramafic rock,
- coal in sedimentary organic basins,
- diamonds in kimberlite pipes through old cratons,
- rare earths in carbonatite and monazite placer source regions,
- halite in evaporite basins,
- pitchblende in suitable granitic provinces.

Deposits are manifest entries with finite estimated grade and tonnage before
voxel materialization. Fine voxel placement is deterministic within the host
volume and cannot exceed the manifest budget.

Every world guarantees progression:

- commons occur on every major continent in useful quantities,
- each regional resource occurs in multiple separated provinces,
- every treasure class has at least two sites so one accident cannot erase
  the category,
- no resource is created merely because a player searches an untouched
  chunk,
- distances are measured geodesically and reported in the atlas census.

## Chunk materialization

The existing 3D density terrain remains the fine-detail layer, but its
large-scale offset, structure, rock family, and tectonic orientation come
from the atlas.

- Atlas elevation supplies the coarse surface.
- Fine noise adds bounded relief without changing continents or reversing
  major slopes.
- Density caves remain, later constrained by groundwater.
- Strata sample local geological frames across seams.
- Atlas deposit manifests gate ore feature placement.
- Volcanoes and intrusions are queried as global site objects.
- Nothing hashes chunk load order.

The old infinite planar plate, continentalness, and large-scale moisture
fields are removed rather than layered underneath the atlas.

## Interaction with existing systems

- Prospecting reads actual atlas geology and remaining deposit evidence.
- Survey cairns report named geological provinces and directional findings.
- Ruins may prefer stable valleys and resource corridors later; they do not
  generate during this goal based on incomplete climate.
- Hearts remain temporarily assigned by placeholder country data until the
  biome goal.
- Finite lava remains voxel fluid; magma chambers are finite manifest
  volumes.
- The pump receives groundwater integration in the water-cycle goal.

## Headless maps and reports

This goal adds:

- plate and Euler-pole map,
- boundary-type map,
- continental crust fraction/age map,
- oceanic age map,
- preliminary elevation and bathymetry,
- tectonic contribution layers,
- geological province and bedrock map,
- volcano/intrusion/deposit map,
- land/ocean and resource census.

The report lists rejection attempts and exact failed constraints if a seed
needed a deterministic retry.

## Required tests

### Tectonic coherence

- Plate partition covers every atlas cell exactly once.
- All boundary edges are reciprocal across faces.
- Relative velocity classification is rotationally invariant.
- Connected boundary runs do not flicker class cell by cell.
- Continental collision raises both sides into a broad range.
- Oceanic subduction places trench and arc on the correct sides.
- Divergent ocean boundaries produce young shallow crust.
- Hotspot chains age in the plate-motion direction.

### Continents and terrain

- Seed suite satisfies continent count, land share, and global-ocean bands.
- Continents and ranges cross cube-face seams without discontinuity.
- No face or seam has statistically privileged elevation.
- Elevation contains continental shelves, abyssal basins, lowlands, and
  mountains in pinned proportions.
- Fine chunk terrain stays within the atlas relief envelope.

### Geology

- Strata continue across chunk and face boundaries.
- Fold orientation follows boundary strike.
- Contact metamorphism surrounds intrusions and not arbitrary coordinates.
- Every mineral occurs only in permitted host geology.
- Deposit materialization never exceeds manifest tonnage.
- Progression resource guarantees hold over a broad deterministic seed suite.

### Determinism and budgets

- Same inputs yield byte-identical geological atlas layers and manifests.
- Serial and parallel generation agree.
- Production atlas completes within the atlas plan's memory budget.
- Chunk generation order cannot alter geology or deposit totals.

### Visual qualification

Review atlas maps and in-world captures of:

- a continent–continent range and foreland,
- a subduction trench, coastal range, and volcanic arc,
- a rift valley,
- an island arc,
- an eroded hotspot chain,
- folded strata and a contact aureole.

## Completion criteria

This goal is complete when:

- the planet has globally coherent plates, cratons, ocean basins, continents,
  relief, strata, volcanoes, intrusions, and deposit manifests,
- chunk terrain and host rock derive from those atlas layers,
- the old planar large-scale geology no longer decides generation,
- finite deposit budgets and progression guarantees are recorded,
- prospecting reads the new geological truth,
- all topology, causality, determinism, census, host-rock, budget, and visual
  tests pass.

Do not mark this implemented because a plate map looks attractive. Ranges,
trenches, rocks, volcanoes, and deposits must all tell the same history.
