# Planetary climate — latitude, air, ocean, and rain

> **Status: implemented and qualified (2026-07-31).**
>
> This is goal 4 of the planetary-world sequence. It requires the completed
> topology, atlas, and geology goals.

## Implementation record — 2026-07-31

**Goal 9 closure (2026-08-01).** The integrated build retains the causal
seasonal solver and WFD3 dynamic state under generator 9 / protocol 24. The
production seed-1337 solver converged in `[93, 93, 90, 90]` iterations with
maximum residual `0.019581556` and moisture-budget error
`0.000007949769496917725`. A live production save advanced 44 accepted
weather hours and one groundwater day with exact zero water/salt drift; a
separate 12-hour whole-atlas probe and a ten-hour cold-streaming interleaving
probe also close exactly. The final matrix records the rollback/capacity bugs
found and fixed by those live runs.

Goal 4 replaces independent temperature/humidity noise and the single global
weather machine with one spherical causal climate and one authoritative
mutable weather atlas. Runtime and genesis share a 23.5-degree axial tilt,
144-day orbital year, manifest rotation axis and prime meridian, local solar
geometry, hemisphere-aware seasons, polar day/night, and latitude/longitude
derivation. The immutable solver stores four seasonal tables for temperature,
wind, moisture, precipitation, potential evapotranspiration, aridity, snow
persistence, continentality, ocean currents, and ocean heat anomaly.

The climate-normal solver builds three circulation cells per hemisphere,
conservative multi-neighbor moisture advection, convergence and subtropical
descent, land fetch, maritime moderation, elevation cooling, coherent
orographic lift/rain shadows, and broad current-driven coastal heat. It must
converge within residual `0.02` or fail after 256 iterations. Dynamic vapor,
cloud, pressure, heat, wind, storms, precipitation, soil moisture, snow,
runoff, groundwater, and storage anomalies advance as deterministic sliced
whole-planet passes. Integer transfer preserves every water unit across
advection, precipitation, cube seams, save/reload, and the explicit
coarse-to-voxel handoff. Ire may convert moist instability into stronger local
storms but cannot add water.

All former weather/season consumers now sample a planetary position: crops,
foliage, wildlife, wardens, fire, snow/ice, windmills and other machines,
offerings, UI, agents, sky, particles, lightning, audio, solo, dedicated host,
and multiplayer guests. The global `Weather` enum and its save field are gone.
Clients receive authoritative nearby weather cells across face seams. The
Long Winter is a global supernatural −18 C thermal anomaly and growth winter;
the real orbital sun and ordinary seasons continue underneath it.

### Format and compatibility

- immutable genesis is `WFA3`, atlas format and algorithm versions are `3`,
- dynamic state is `WFD2`, dynamic schema version is `2`, and persists the
  last completely accepted climate hour,
- geology and history schemas remain `1`,
- world generator version is `4`, and multiplayer protocol is `20`,
- corrupt immutable climate is refused; corrupt dynamic state restores a
  checksum-validated atomic backup or rebuilds its deterministic baseline.

The break is intentional. Older atlas/world/protocol versions are not silently
reinterpreted because doing so could move climatology, weather, or terrain
under already materialized chunks.

### Production qualification

The final release binary generated deterministic seed `1337`, all 393,216
cells, and the full diagnostic bundle in 29.74 seconds, using 417,924 KiB peak
resident memory and no swap. Its immutable and dynamic files were byte-equal
to the earlier qualified run, with SHA-256 hashes `76010b57d0ecfc5dcfdca5f5d09d52ac5ab44304d8aee7568872a45ff521f8b0`
and `d2e475cfef76f3c43cfeb904f76ad4e0f8182eaf3304e36ac9c915d667eae422`.
The immutable atlas is 98,697,248 bytes, dynamic state 25,165,864 bytes,
geology 133,826 bytes, and estimated loaded atlas memory 127,535,810 bytes;
all remain inside the atlas budgets. Diagnostic export itself took 18.61
seconds and emitted 91 maps plus census, manifest, geology, weather tracks,
mountain transects, globe preview, validation report, and exact qualification
coordinates.

The production seasonal iterations were `[93, 93, 90, 90]`, maximum residual
`0.019581556`, and maximum floating moisture-budget error
`0.000007949769496917725`. Area-weighted mean temperature is 14.25 C (−19.89
to 32.30 C) and mean precipitation is 1,301.87 mm/year. The wettest land site
receives 4,700.1 mm/year; the driest warm site receives 90.7 mm/year at 18.9 C
and aridity 8.0. The strongest exported mountain transect loses 764.0 mm from
windward to leeward. Dry, polar, temperate, and tropical climate areas all
occupy substantial non-striped country.

A 24-hour dynamic diagnostic kept unexplained water drift at exactly zero in
every hour. Hour 1 contained 367,682 clear, 14,695 overcast, and 10,839
precipitating cells; by hour 24 the moving field contained 193,831 clear,
37,654 overcast, 160,321 precipitating, and 1,410 storm cells. A three-year
test also preserved the exact starting water total.

### Live and automated evidence

The real client loaded the production atlas twice from the same hour-zero
dynamic checksum. After one accepted weather hour, the NegX qualification site
reported `PRECIPITATION`, +25 C, early autumn, while the distant PosX site
simultaneously reported `CLEAR`, +16 C, early spring. The retained captures are
`/tmp/wildforge-climate-rain-status-hour1-final.png` and
`/tmp/wildforge-climate-clear-status-hour1-final.png`. A 10-second dedicated
host smoke run loaded and advanced the same atlas at 357,812 KiB peak resident
memory with no swap.

The climate-specific suite has 17 relational, conservation, seam,
persistence, locality, multiplayer, astronomy, Long Winter, and long-run
tests. It proves warm/cold current signals reach the corresponding coasts,
weather is simultaneously different in distant regions, and sliced updates
are byte-deterministic. The real-client captures also exposed a still-stark
tangent horizon on the NegX face at short view distance; that rendering
limitation is recorded rather than misreported as a climate defect.

The final repository-wide serial run passed 431 tests with zero failures and
12 intentional developer/benchmark ignores in 10 minutes 13 seconds. It used
287,740 KiB peak resident memory and no swap. Serial execution and
`CARGO_BUILD_JOBS=1` are deliberate on the qualification machine: earlier
parallel compilation exhausted its memory, which was a runner choice rather
than evidence of a climate leak.

Final repository gates also pass: formatting, all-target locked compilation,
strict all-feature Clippy with warnings denied, the release build, dependency
advisories, and the whitespace/error-marker audit.

Notes versus the draft: v1 uses four seasonal samples rather than twelve and
resolves mixed precipitation to rain or snow as permitted. This goal supplies
conservative atmospheric weather and exact transfer amounts to voxel water;
Goal 5 consumed those normals to provide routed rivers, lakes, oceans,
baseline salinity, and finite voxel water. Goal 6 has since connected them to
the complete reservoir cycle, springs, pumping, and multi-century audit; this
climate goal does not claim that later work as its own. Goal 7's current
shipping format is `WFA6`/`WFD3` plus `WFW1` and `biomes.wfb`, atlas algorithm
`6`, world generator `7`, `WFC8` chunk/network records, and multiplayer
protocol `23`.

## Purpose

Before Goal 4, Wildforge sampled temperature and humidity from independent
noise and advanced one global weather state. That could color an infinite
world, but it could not explain why one face of a mountain is forest and the
other desert, why continental interiors dry out, why rain arrives from an
ocean, or why the two hemispheres have opposite seasons.

This goal builds a simplified but causal planetary climate:

- latitude and axial tilt determine sunlight and seasonality,
- elevation cools the air,
- oceans moderate nearby land,
- broad circulation determines prevailing winds,
- ocean circulation creates warm and cold coastal anomalies,
- atmospheric moisture originates in water and vegetation,
- wind and terrain create orographic rain and rain shadows,
- local weather moves through climate rather than toggling worldwide,
- precipitation is a mass transfer consumed by the planetary water cycle.

The target is process realism and stable, legible geography—not computational
fluid dynamics.

## Fixed astronomical model

- Rotation axis is a fixed unit vector stored in the planet manifest.
- Axial tilt is **23.5 degrees**.
- One year remains 144 in-game days: four 36-day seasons.
- Northern vernal equinox begins the calendar.
- Northern and southern seasons are opposite.
- Solar declination follows a sinusoidal orbit approximation.
- Day length varies by latitude and season.
- Polar day and night occur where geometry requires them.
- Orbital eccentricity is omitted in v1.

The renderer and climate use the same sun direction. Sunrise, sunset, local
solar noon, skylight, crop season, snow, and temperature must not disagree
about which side of the planet faces the sun.

## Latitude and longitude

Latitude is derived from the surface unit direction and rotation axis.
Longitude is derived around that axis with one manifest-fixed prime meridian.

Latitude directly influences insolation. Longitude has no arbitrary hot or
cold band; it matters through the positions of continents, oceans, currents,
mountains, and moving weather.

All diagnostic maps label latitude and longitude, but simulation remains in
canonical atlas coordinates.

## Climate normals

The immutable climate atlas stores annual and seasonal normals:

```text
mean near-surface temperature
temperature seasonal amplitude
ocean temperature anomaly
continentality
prevailing tangent wind
mean atmospheric moisture
annual precipitation
precipitation seasonality
potential evapotranspiration
aridity index
snow persistence
```

Compute four or twelve seasonal samples during genesis, then retain compact
harmonic coefficients or seasonal tables. Dynamic weather adds anomalies to
these normals.

### Temperature

Temperature comes from:

1. latitude-dependent solar input,
2. season and day length,
3. elevation lapse rate,
4. land/ocean heat capacity,
5. distance from ocean,
6. broad ocean-current anomaly,
7. bounded low-amplitude planetary variation.

Noise may perturb regional temperature modestly. It may not override latitude
or place a tropical annual climate at a pole.

Elevation cooling is continuous. Alpine climate can appear at any latitude
without reclassifying an entire geological province.

### Continentality

Calculate geodesic distance and upwind fetch from major ocean cells. Coastal
cells have smaller seasonal temperature swings and more available moisture.
Large continental interiors have larger swings and may dry even outside the
subtropical belt.

Island climates remain maritime.

## Atmospheric circulation

Build a tangent wind field from three broad cells per hemisphere:

- tropical trade winds,
- midlatitude westerlies,
- polar easterlies.

Add:

- convergence and rising air near the equatorial rain belt,
- descending dry air in the subtropics,
- subpolar convergence,
- seasonal north/south migration,
- terrain steering,
- land–ocean thermal perturbations,
- bounded rotational weather variation.

Wind vectors are transported correctly across cube-face seams. No face-local
axis may become a preferred east or north.

The climate-normal solver iterates wind and moisture until its annual fields
converge within a documented tolerance or reaches a hard failure bound.

## Ocean circulation

The full ocean-fluid problem is out of scope, but currents must provide the
first-order coastal effects.

Create surface currents from:

- prevailing wind stress,
- planetary rotation sign by hemisphere,
- coastline deflection,
- connected ocean basins,
- a broad equator-to-pole heat transport.

Solve a bounded coarse current field over ocean atlas cells. Use it to apply
warm western-boundary and cool eastern-boundary style anomalies where basin
geometry supports them, without naming Earth-specific currents.

Currents:

- transport heat and atmospheric moisture potential,
- do not move visible voxel ocean cells,
- are immutable climate normals in this goal,
- cross cube-face seams continuously.

## Moisture and precipitation

### Moisture sources

At genesis equilibrium, atmospheric moisture enters from:

- ocean evaporation,
- lake and wet-soil evaporation,
- vegetation transpiration estimated from provisional cover.

The completed dynamic water-cycle goal debits real reservoirs using the
equilibrium rates and transfer coefficients computed here.

### Advection

Move moisture downwind over the atlas using a conservative, bounded transport
step. Each iteration accounts for:

- source evaporation,
- downwind advection,
- convergence/divergence,
- cooling and saturation,
- precipitation,
- residual atmospheric export to neighbors.

Transport cannot lose moisture at cube-face seams or corners.

### Orographic precipitation

When wind crosses rising terrain:

- lift and cool the air,
- increase condensation and precipitation on the windward slope,
- reduce remaining moisture,
- warm/dry descending air on the lee slope.

This must produce coherent rain shadows across whole ranges. It is not a
per-cell “mountain nearby” bonus.

### Other precipitation regimes

- Persistent equatorial convergence favors wet tropics.
- Subtropical descent favors deserts near roughly 20–35 degrees latitude.
- Continental interiors dry with long land fetch.
- Midlatitude storm tracks produce seasonal precipitation.
- Monsoon-like seasonal reversal may occur on sufficiently large tropical
  continents through land–ocean heating contrast.
- Cold air holds less moisture; polar regions may be cold deserts.

## Dynamic local weather

Replace the global `Weather` finite-state machine with coarse weather fields:

```text
temperature anomaly
pressure anomaly
vapor
cloud water
storm energy
precipitation rate/type
wind anomaly
```

The atlas owns authoritative weather. It advances at a coarse climate step
(target: once per in-game hour), sliced across server ticks.

Weather behavior:

- anomalies advect with local winds,
- ocean and wet land recharge vapor,
- convergence and terrain form cloud water,
- cloud water precipitates locally,
- fronts have coherent spatial extent and direction,
- storms emerge from instability and available moisture,
- ordinary weather relaxes toward seasonal normals,
- the world never rains everywhere merely because one enum changed.

Clients receive weather cells or sampled local weather sufficient to render
nearby precipitation, wind, sky, lightning, and ambience. The host remains
authoritative.

### Ire and weather

The wild may still lean on weather, but it cannot create water or rewrite
climatology.

- Ire can increase storm-energy conversion or lightning likelihood locally.
- It cannot make a dry subtropical air mass rain without moisture.
- Rain continues to soothe ire.
- Wild-caused lightning remains attributed to the wild.

## Precipitation form

Precipitation form derives from the vertical temperature profile
approximation:

- rain when the near-surface column remains above freezing,
- snow when it remains below,
- mixed/freezing conditions may resolve to rain or snow in v1 rather than
  adding sleet content.

Snowfall credits snowpack and visible snow through the water-cycle interface.
Snowmelt debits that same reserve.

Deserts do not veto precipitation categorically. They receive little because
the circulation supplies little; rare rain is therefore possible and can
refill wadis, oases, and aquifers.

## Calendar and the Long Winter

Ordinary seasons are hemispheric. Existing gameplay that asks for a season
uses the local astronomical season at the position.

The Long Winter is explicitly supernatural:

- It applies a persistent negative insolation/temperature anomaly to the
  entire planet.
- It suppresses growing-season behavior in both hemispheres.
- It does not pretend the orbital position stopped with one hemisphere
  permanently facing a northern winter.
- Relighting enough hearts removes the anomaly and ordinary orbital seasons
  resume continuously.

This preserves the campaign while keeping normal climate internally honest.

## Integration with existing systems

- Foliage tint reads local season and biome.
- Crops read local temperature, light, soil moisture, and season rather than
  one global season index.
- Wildlife breeding and migration hooks read local season.
- Windmills read local wind, not a global weather rate.
- Fire spread reads precipitation, wind, and fuel moisture.
- Snow and ice read local energy balance.
- Audio and sky render local weather.
- World titles/debug UI show local biome and weather.
- Dedicated hosts simulate the whole coarse weather atlas, including
  unvisited country, within budget.

## Diagnostics

Export:

- annual and four-season temperature,
- seasonal amplitude and continentality,
- prevailing winds,
- ocean currents and temperature anomaly,
- atmospheric moisture paths,
- annual and seasonal precipitation,
- aridity,
- snow persistence,
- example dynamic weather frames and tracks.

Add transect reports across selected ranges: elevation, wind, moisture,
precipitation, temperature, and aridity by distance.

## Required tests

### Astronomical

- Equator, midlatitude, and polar day lengths follow season correctly.
- Hemispheres have opposite seasons.
- Sun rendering, skylight, and climate share the same local solar geometry.
- No cube face is climatically privileged.

### Climate causality

- Annual temperature declines from equator to poles statistically.
- Temperature declines with elevation at matched latitude.
- Coastal seasonal range is lower than comparable continental interior.
- Warm/cold current anomalies affect the correct downstream coasts.
- Wet equatorial and dry subtropical bands emerge over a seed suite without
  becoming perfect stripes.

### Moisture

- Moisture transport is conservative during genesis iterations.
- A mountain transect is wetter windward and drier leeward.
- Long land fetch dries continental interiors.
- Islands receive maritime moisture.
- Rare precipitation remains possible in deserts.
- All seam-crossing advection agrees from both faces.

### Weather

- Fronts move continuously across face seams.
- Rain occurs only where cloud water is debited.
- Storm bias from ire cannot create moisture.
- Local weather persists and round-trips.
- Server update work remains bounded and deterministic.
- Guests see the host's local weather across a seam.

### Long-run and visual

- Climate normals converge within the documented iteration bound.
- A multi-year dynamic run shows no unexplained atmospheric water drift.
- Atlas maps display coherent circulation, rain shadows, wet coasts, and dry
  interiors.
- In-world captures show simultaneous different weather in distant regions.

## Completion criteria

This goal is complete when:

- solar geometry, latitude, elevation, oceans, winds, currents, and terrain
  causally determine climate,
- rain shadows and continental aridity are measurable,
- dynamic weather is local and spatially moving,
- precipitation exposes conservative transfer amounts to the water cycle,
- all existing weather/season consumers use local planetary state,
- the global weather enum no longer drives simulation,
- astronomical, climate, moisture, seam, persistence, multiplayer, budget,
  and visual tests pass.

Do not mark this implemented because latitude colors a temperature map.
Mountains, oceans, wind, moisture, and moving weather must all participate.
