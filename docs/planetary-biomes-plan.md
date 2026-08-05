# Planetary biomes — life follows climate, water, and ground

> **Status: implemented and qualified (2026-07-31).**
>
> This is goal 7 of the planetary-world sequence. It requires completed
> topology, atlas, geology, climate, hydrology, and water-cycle goals.

## Implementation and qualification record

**Goal 9 closure (2026-08-01).** Biome schema 1 remains current in generator
9. The final seed-1337 atlas has 500 countries and hearts, 849 country routes,
and all 393,216 cells represented in the validated visual/census bundle.
Freshwater ecology now falls back to atlas habitat when a relevant voxel chunk
is unloaded, so host interest streaming cannot turn a mapped river/lake into
ocean ecology. Older Goal-7 screenshots made before the cube-face winding fix
are explicitly excluded; the machine-checked final capture manifest lists
only corrected renderer evidence.

Goal 7 ships as atlas algorithm version 6, generator version 8, biome schema
1 (`biomes.wfb`), chunk format WFC8, heart format WFH4, and protocol 23.
The implementation keeps zonal biome, edaphic soil, hydrological habitat, and
wild/heart state as separate causal layers. Geographic countries are a
weighted partition over landmasses, watersheds, crests, and routes; they no
longer impose climate. Habitat predicates are data-driven for animals and
mods, while crop growth consumes real moisture, drainage, fertility,
temperature, and persisted salinity.

Production seed 1337 qualified the full 256-cell-side atlas:

- 393,216 cells, 35.16% land, 39 emerged landmasses;
- 500 geographic countries, 500 registered hearts, and 849 routes;
- 878 named rivers, 2,224 floodplain cells, 6,062 wetland cells, 557 delta
  cells, and 1,637 estuary cells;
- 131,727,392 immutable bytes plus 503,218 biome bytes;
- 28.52 seconds generation/export and 392,136 KiB measured generator peak RSS
  with one build job; the exported report records 401,547,264 peak resident
  bytes, below the 512 MiB generation envelope;
- exact zero unexplained water and salt in the conservation audit.

The automated suite covers causal zones, ecotones, seam-crossing habitat,
groundwater-caused oases, pumping/recharge, deltas and estuaries, salt-marsh
surface cover, country coverage/connectivity/barrier preference, every
country heart and edifice, physically constrained grafts, connected wildlife
migration/recovery, temperature/moisture/fertility/drainage/salinity crop
response, real-water irrigation, save/load, determinism, and heart isolation
from climate and conserved water. `qualification-sites.toml` records each
visual site, its exact spawn, evidence, and longitude-correct local noon;
headless client captures were run against the persisted production atlas at
view distance 4 after the world settled. Generator 8 also keeps a coherent
24-block density mantle and 12-block cave roof beneath atlas terrain; a
multi-face regression grid verifies at least eight contiguous solid blocks
under sampled highland surfaces while retaining caves and deliberate
overhangs. Those historical software-rendered captures exposed an
art/readability issue: pale strata blended into fog even where
production-column probes confirmed a continuous solid shell. The 2026-08-05
strata pass has now resolved and native-DX12-qualified that presentation
defect without changing biome placement, geology, terrain, or streaming; see
`docs/strata-fog-readability-plan.md` and the version-2 visual manifest.

## Purpose

The current province system samples a biome at one site and imposes that
temperature/humidity identity over roughly 900 blocks. It creates memorable
places, but reverses causality: political/spiritual regions decide climate,
while rivers and mountains are local exceptions.

On the planet, coherent geography itself prevents biome confetti. This goal
derives vegetation, soils, habitats, provinces, hearts, and ecology from:

- latitude and seasonal temperature,
- annual precipitation and aridity,
- elevation,
- soil and bedrock,
- drainage and groundwater,
- rivers, lakes, wetlands, and coasts,
- fire and disturbance,
- the wild's explicitly supernatural influence.

A desert country may contain an alpine source, green river corridor, oasis,
dunes, and salt basin while remaining recognizably one place.

## Classification layers

Do not assign one enum and ask it to explain everything. Use four layers.

### 1. Zonal climate biome

Derived from long-term temperature, precipitation, seasonality, and aridity:

- tropical wet → Jungle,
- tropical seasonal → Savanna,
- hot arid → Desert,
- warm semi-arid → Scrubland or Badlands by substrate/erosion,
- temperate moist → Forest,
- temperate seasonal/drier → Plains,
- cold forest-capable → Taiga,
- cold treeless → Tundra,
- permanent severe cold → Arctic,
- elevation-limited → Mountains.

Existing biome names remain content identities. Boundaries use continuous
climate response and broad ecotones, not nearest-centroid noise.

### 2. Edaphic/terrain modifiers

Soil, rock, slope, and disturbance alter local expression:

- shallow rocky soil limits trees,
- limestone supports different fertility/drainage,
- dunes and active sediment remain sparsely vegetated,
- fertile volcanic/alluvial soils support denser growth,
- steep/high terrain becomes alpine or bare,
- repeated natural fire favors grass/savanna,
- permafrost limits drainage and roots.

### 3. Hydrological habitat overlays

Independent local masks:

- riparian corridor,
- floodplain,
- swamp/wetland,
- oasis/spring,
- lakeshore,
- estuary/delta,
- salt marsh,
- beach/dune,
- aquatic fresh/brackish/salt,
- cave/aquifer outlet.

These overlays control blocks, vegetation, wildlife, and resources without
relabeling an entire region.

### 4. Wild/heart state

Heart and ire state modifies renewal, ambience, wardens, and exceptional
growth. It does not rewrite latitude, mountain elevation, rainfall, or water
mass.

After the planet is qualified, `docs/magic-sequence.md` adds magical richness
as another continuous habitat overlay. It composes with these four layers and
never reclassifies a desert, tundra, wetland, reef, or forest into a
climate-independent universal magic biome.

## Soils

Generate a soil profile from:

- parent rock/sediment,
- climate weathering,
- slope and erosion,
- drainage,
- organic input,
- flood/volcanic deposition,
- permafrost or aridity.

Atlas soil fields:

```text
depth
texture (sand/silt/clay fractions)
organic content
baseline fertility
drainage class
salinity
permafrost/seasonal freeze
erosion susceptibility
```

Voxel surface rules materialize existing grass, dirt, sand, clay, mud,
farmland, snow, rock, and salt materials from these properties.

Player fertility remains a local managed property layered over baseline soil.
Fallow recovery depends on living ecology, moisture, temperature, and organic
cycling.

## Vegetation

Vegetation density and species derive from water/energy limits:

- trees need sufficient growing-season warmth and available moisture,
- grass tolerates lower moisture and frequent disturbance,
- desert vegetation clusters around episodic water and groundwater,
- riparian woodland follows rivers through otherwise arid country,
- oases form around persistent fresh springs,
- treelines descend toward poles and rise/fall with climate,
- wetlands follow saturation rather than a swamp noise threshold,
- coastal salinity selects tolerant cover,
- jungle canopy belongs to year-round wet warmth.

Fine placement remains deterministic and chunk-local after sampling atlas
constraints. It cannot place water-demanding forest where the water budget
says none survives.

Natural succession/regrowth reads the same habitat model. A sapling planted
outside its tolerance may survive only with player care; it does not silently
change planetary climate.

## Oases and springs

An oasis requires a groundwater or surface-water cause:

- spring discharge in arid climate,
- shallow fresh water table,
- losing river with persistent wet pocket,
- human irrigation after settlement.

Oasis radius and vegetation respond to discharge and soil salinity. Pumping
the aquifer can shrink it; recharge can restore it. A dead heart may stop
supernatural renewal but cannot erase the water source.

## Provinces become geographic countries

Remove biome-imposing Voronoi provinces. Generate countries after geography.

Target:

- broadly 400–600 countries on the default planet,
- characteristic diameter near the old 900-block spacing,
- smaller on islands and fragmented mountains,
- larger in open plains/deserts where geography supports it.

Use weighted geodesic partitioning with boundary costs:

- mountain crests and major watershed divides strongly attract boundaries,
- open ocean separates countries except coherent island groups,
- major rivers may form an axis within a valley country or a boundary where
  geography supports either,
- narrow passes and basin rims matter,
- cube-face seams have zero special cost.

Every country stores:

- dominant zonal biome,
- habitat/soil composition,
- principal watershed or island,
- heart site,
- deterministic name seed,
- neighboring countries and traversable passes/routes.

This preserves named, legible places while allowing honest internal ecology.

## Hearts

Place a heart at a geographically meaningful site:

- wooded countries: old grove in a stable moist interior,
- dry countries: persistent spring/oasis or ancient stone near water,
- plains/savanna: barrow/tree near a river rise or central divide,
- mountain countries: prominent divide, source, or standing stone,
- swamp/wetland: deep stable wetland island,
- arctic/tundra: persistent ice/stone landmark.

Existing edifices remain findable landmarks and adapt their local material.

### Heart effects

Hearts govern the wild's relationship and regenerative capacity:

- wildlife repopulation,
- natural succession bonuses,
- offering response,
- wardens,
- ambience,
- exceptional fertility/regrowth near a living heart.

They do not:

- make or stop rain,
- create water,
- change latitude,
- move a mountain,
- alter marine salinity,
- violate the planetary conservation audit.

### Grafting and replacement

The existing foreign-heart terraforming promise is constrained by physical
climate:

- compatible grafts can shift species composition and soil ecology over
  seasons,
- marginal grafts require sustained irrigation/shelter and remain local,
- grossly incompatible grafts become stressed and cannot convert a desert
  climate into taiga or an arctic coast into jungle,
- reawakening the native heart is always physically supported.

The supernatural wild may accelerate succession, not manufacture a new sun
or water cycle.

The Long Winter remains the explicit planetary magical exception defined in
the climate plan.

## Ecology integration

Animal habitat suitability reads:

- temperature and season,
- vegetation/cover,
- water availability and salinity,
- prey/food,
- elevation and terrain,
- disturbance and heart state.

Update the existing roster:

- fish respect fresh/brackish/salt water and temperature,
- herons and frogs follow wetlands and freshwater margins,
- seals and seabirds follow marine coasts,
- grazers follow productive grasslands and water,
- predators follow prey habitat,
- polar species follow persistent cold rather than a province label.

Spawn-once/persistent wildlife remains where appropriate, but reseeding and
migration use connected habitat instead of arbitrary chunk odds.

Fungus, leaf litter, dung, predation, fertility, crop raids, and fire all read
local moisture and season.

## Agriculture

Crop success becomes legible from:

- temperature/growing season,
- soil fertility,
- soil moisture and drainage,
- salinity,
- light,
- crop family/rotation.

Do not add a numerical farming dashboard by default. Soil appearance, wilting,
snow, wetness, and plant growth communicate state. Tooltips may explain gross
failure such as saline soil, frozen ground, or drought.

Irrigation transfers real water through channels or future infrastructure.
Over-irrigating poorly drained arid soil may accumulate salt, giving drainage
and crop rotation a reason to matter.

## Worldgen and chunk order

Chunk generation samples:

- zonal biome,
- soil profile,
- habitat overlays,
- succession/vegetation potential,
- country/heart assignment.

Fine features use canonical position hashes. They remain deterministic,
seamless, and independent of load order. Existing data-driven species and
feature registries gain habitat predicates without hard-coding every mod to
base biome enums.

Mods may target climate/habitat tags such as:

```text
warm
cold
humid
arid
riparian
wetland
freshwater
marine
alpine
saline
volcanic_soil
```

Existing biome names remain supported as convenient compound tags.

## Diagnostics

Export:

- zonal biome map,
- soil depth/texture/fertility/drainage/salinity,
- habitat overlays,
- vegetation potential and tree line,
- animal suitability examples,
- geographic countries and adjacency,
- hearts and edifices,
- graft compatibility.

Reports include biome/habitat area by latitude, continent, elevation, and
water availability.

## Required tests

### Biome causality

- Wet tropical lowlands favor Jungle.
- Subtropical dry zones and rain shadows favor Desert/Scrubland/Badlands.
- Moist temperate regions favor Forest.
- Cold tree-capable regions favor Taiga; colder/shorter seasons favor
  Tundra/Arctic.
- Alpine override follows elevation and latitude-sensitive tree line.
- Biome maps have broad coherence without province-shaped climate borders.

### Local habitats

- Arid river valleys receive riparian vegetation.
- Oases require persistent fresh water.
- Wetlands follow saturation and low drainage.
- Deltas/estuaries follow hydrology and salinity.
- Pumping can reduce an oasis through the water-cycle interface.
- River corridors and habitats cross cube-face seams.

### Countries and hearts

- Country partition covers all land exactly once.
- Boundaries statistically prefer divides, coasts, and strong barriers.
- Islands and enclosed valleys form coherent countries.
- No cube-face seam attracts a boundary.
- Every country has one reachable, registered heart and edifice.
- Heart forms match the site's geography.
- Dead/live heart effects never modify conserved climate/water layers.

### Ecology and agriculture

- Species spawn only in suitable habitat, including salinity.
- Habitat networks allow recovery/migration under documented rules.
- Crops respond to temperature, moisture, fertility, drainage, and salinity.
- Irrigation debits real water.
- Foreign graft compatibility follows climate and player support.

### Visual qualification

Review transects and captures:

- wet mountain face to dry rain shadow,
- desert crossed by a green river,
- natural oasis and spring,
- floodplain into delta/estuary,
- temperate coast into continental interior,
- treeline from forest to alpine,
- one country containing several coherent habitats.

## Completion criteria

This goal is complete when:

- biomes derive from climate, water, elevation, and soil,
- rivers, springs, wetlands, oases, coasts, and alpine zones create local
  habitat overlays,
- provinces no longer impose climate and instead follow planetary geography,
- every country has a compatible heart and existing heart gameplay works,
- ecology, agriculture, regrowth, and species consume habitat state,
- heart magic never violates climate or water conservation except the
  explicitly defined Long Winter anomaly,
- mod habitat predicates and documentation ship,
- all causal, habitat, country, heart, ecology, agriculture, seam,
  determinism, and visual tests pass.

Do not mark this implemented because the final map contains every biome name.
Each biome and habitat must be explainable from the land beneath it and the
water and climate acting upon it.
