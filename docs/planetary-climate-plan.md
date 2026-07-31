# Planetary climate — latitude, air, ocean, and rain

> **Status: implementation design complete, not implemented.**
>
> This is goal 4 of the planetary-world sequence. It requires the completed
> topology, atlas, and geology goals.

## Purpose

Wildforge currently samples temperature and humidity from independent noise
and advances one global weather state. That can color an infinite world, but
it cannot explain why one face of a mountain is forest and the other desert,
why continental interiors dry out, why rain arrives from an ocean, or why the
two hemispheres have opposite seasons.

This goal builds a simplified but causal planetary climate:

- latitude and axial tilt determine sunlight and seasonality,
- elevation cools the air,
- oceans moderate nearby land,
- broad circulation determines prevailing winds,
- ocean circulation creates warm and cold coastal anomalies,
- atmospheric moisture originates in water and vegetation,
- wind and terrain create orographic rain and rain shadows,
- local weather moves through climate rather than toggling worldwide,
- precipitation is a mass transfer consumed by the later water-cycle goal.

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

The dynamic water-cycle goal later debits real reservoirs. This goal computes
the equilibrium rates and transfer coefficients.

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
