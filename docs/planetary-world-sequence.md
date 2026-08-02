# The finite planet — master design and `/goal` execution sequence

> **Status: all nine goals and world-entry remediation implemented and
> qualified (2026-08-01).**
>
> This document is the index, dependency contract, and final definition of
> done for Wildforge's transition from an infinite plane to one finite living
> planet. Do not implement the whole sequence in one undifferentiated change.
> Run the nine goal documents below, in order.

## North star

Wildforge is a familiar voxel survival game that quietly turns a group of
friends into a society:

> **We came to mine blocks. We ended up founding a country.**

An infinite frontier works against that promise. Consequences can be
abandoned, land can always be replaced, maps never finish, and mineral
scarcity is only a probability over more walking.

The planet makes one fact true:

> **There is one world. Everyone lives with what happens to it.**

The planet is finite but not small, closed but not bordered, realistic in its
causes without trying to simulate every molecule at voxel resolution.

## Settled architecture

- One cube-sphere planet; no shipping infinite-flat mode.
- Six `8192 × 8192` surface faces.
- `16 × 16 × 256` column chunks, generated lazily.
- 32-block atlas cells: 393,216 coarse cells over the whole planet.
- A complete global atlas is generated before the first voxel chunk.
- Large-scale geology, climate, drainage, groundwater, and biomes come from
  causal global fields.
- Visible water remains finite voxel volume.
- Atmosphere, soil, groundwater, snow, lakes, and oceans are conservative
  coarse reservoirs.
- Geological deposits and economically meaningful material have finite
  manifests and accountable lifecycles.
- Existing flat saves and old world/mod coordinate APIs receive an explicit
  format break; they are never silently reinterpreted.

Constants may change only through measured implementation evidence recorded
in the relevant document's notes-vs-spec header. A later goal does not casually
reopen an earlier goal's fixed decisions.

## Dependency graph

```text
1. planet topology
        │
2. planet atlas
        │
3. planetary geology
        │
4. planetary climate
        │
5. planetary hydrology
        │
6. planetary water cycle
        │
7. planetary biomes / countries / hearts
        │
8. finite materials and retrogen
        │
qualification prerequisite: world entry and cold streaming
        │
9. integrated planetary qualification
```

The order is causal:

- the atlas cannot have correct neighbors before topology,
- climate needs continents and elevation,
- drainage needs precipitation and terrain,
- the dynamic cycle needs basins and weather,
- biomes need climate, soil, and water,
- finite-material guarantees need final geography and habitat,
- qualification needs all of them.

## How to run the sequence

Run one command/prompt at a time. The intended interaction is exactly:

```text
/goal complete all features in the design document docs/planet-topology-plan.md
```

After that goal is genuinely complete and verified, run the next line.

### Goal 1

**Implementation status:** complete and qualified (2026-07-31).

```text
/goal complete all features in the design document docs/planet-topology-plan.md
```

Produces:

- finite cube-sphere coordinates and address space,
- closed seam/corner topology,
- curved rendering and local gravity,
- planetary physics, raycasts, chunks, persistence, networking, mods,
- circumnavigation on temporary terrain.

### Goal 2

**Implementation status:** complete and qualified (2026-07-31).

```text
/goal complete all features in the design document docs/planet-atlas-plan.md
```

Produces:

- persistent whole-planet coarse atlas,
- immutable/dynamic/history layer ownership,
- deterministic staged creation,
- lazy chunk sampling contract,
- atlas export and census tools.

### Goal 3

**Implementation status:** complete and qualified (2026-07-31).

```text
/goal complete all features in the design document docs/planetary-geology-plan.md
```

Produces:

- spherical plates and motion,
- cratons distinct from plates,
- continents, oceans, ranges, trenches, rifts, arcs,
- coherent strata and geological provinces,
- globally finite deposit manifests and geology diagnostics.

### Goal 4

**Implementation status:** complete and qualified (2026-07-31).

```text
/goal complete all features in the design document docs/planetary-climate-plan.md
```

Produces:

- axial tilt, latitude, local seasons, and day length,
- elevation/continental/ocean temperature,
- prevailing wind and simplified ocean currents,
- conservative moisture climatology,
- orographic rain, rain shadows, and moving local weather.

### Goal 5

**Implementation status:** complete and qualified (2026-07-31).

```text
/goal complete all features in the design document docs/planetary-hydrology-plan.md
```

Produces:

- global ocean and basin solution,
- terminating drainage graph and watersheds,
- discharge-derived rivers,
- lakes, terminal basins, wetlands, deltas, and baseline salinity,
- seamless voxel rivers/lakes/oceans with known baseline volume.

### Goal 6

**Implementation status:** complete and qualified (2026-07-31).

```text
/goal complete all features in the design document docs/planetary-water-cycle-plan.md
```

Produces:

- exact fixed-point planetary water/salt accounting,
- atmosphere, soil, groundwater, snow, lake, ocean, and portable reservoirs,
- conservative evaporation, rain, infiltration, runoff, freeze/melt,
- aquifer-fed springs and flooded mines,
- pumps, boilers, and steam integrated into the cycle,
- century-scale stability and water-audit tooling.

### Goal 7

**Implementation status:** complete and qualified (2026-07-31).

```text
/goal complete all features in the design document docs/planetary-biomes-plan.md
```

Produces:

- climate/water/soil-derived zonal biomes,
- riparian, wetland, oasis, alpine, coastal, and aquatic habitats,
- geographic countries replacing biome-imposing Voronoi provinces,
- geographically placed hearts and constrained grafting,
- ecology and agriculture consuming planetary habitat.

### Goal 8

**Implementation status:** complete and qualified (2026-08-01).

```text
/goal complete all features in the design document docs/finite-materials-plan.md
```

Produces:

- exact finite-resource extraction manifests,
- repair, salvage, scrap, tailings, and dismantling,
- explicit material sinks and operator audits,
- planetary bootstrap/redundancy guarantees,
- finite-world mod retrogen policies.

### Goal 9

**Implementation status:** complete and qualified (2026-08-01).

Before running final qualification, complete the live-test remediation:

```text
/goal complete all features in the design document docs/world-entry-streaming-plan.md
```

This prepares and persists one qualified common spawn, removes synchronous
cold generation from the host pump, gives clients an explicit preparation and
entry-ready state, and closes the agent swimming/coordinate contract defects.
It was discovered by testing the integrated production planet and therefore
sits between feature implementation and final qualification without
renumbering the original nine planetary goals.

Then run:

```text
/goal complete all features in the design document docs/planetary-qualification-plan.md
```

Produces:

- requirement-by-requirement completion audit,
- removal of all shipping infinite-world fallbacks,
- long simulation and end-to-end scenarios,
- full seam/chunk-order/multiplayer matrix,
- measured performance qualification,
- current README/mod/agent/operator documentation,
- final diagnostic atlas and visual record.

## Goal-run contract

Every implementation goal must:

1. Read this master document, its target document, and all prerequisites.
2. Inspect current code and prior implementation records rather than assuming
   the previous goal matched its draft exactly.
3. Derive a checklist from every explicit requirement, named artifact,
   invariant, test, command, and completion criterion in the target.
4. Implement the complete target, including persistence, multiplayer,
   headless, mod/API, documentation, diagnostics, and visual requirements.
5. Preserve existing behavior except where the planetary design explicitly
   supersedes it.
6. Keep generation deterministic and independent of chunk-load order.
7. Run tests proportionate to intermediate risk throughout, then all required
   target gates.
8. Prepend an implementation record to the target document:
   - implementation date,
   - notes vs. specification,
   - format/protocol versions,
   - measured budgets,
   - verification evidence,
   - honest remaining limitations.
9. Change the target status to implemented only after every completion
   criterion has authoritative evidence.
10. Stop before the next goal. Do not blur two goals into one unreviewable
    implementation unless a prerequisite primitive is strictly necessary.

An implementation is not complete because it compiles, renders one demo, or
has a green narrow unit test. The target document's completion criteria are
the definition of done.

## Shared invariants

Every goal preserves these.

### One topology

- All authoritative surface positions are finite and canonical.
- Every neighbor operation closes across seams.
- Cube-face seams have no physical, climatic, ecological, or economic
  privilege.
- No system smuggles an infinite planar coordinate back into shipping code.

### One genesis

- The immutable atlas is deterministic from seed, version, and creation-time
  content.
- Chunk order cannot change the planet.
- Saved chunks override genesis; genesis never overwrites player work.
- Unmaterialized terrain already belongs to global water/resource budgets.

### One water ledger

- Rain transfers water; it never creates it.
- Evaporation transfers water; it never deletes it.
- Springs debit aquifers.
- Rivers reach sinks.
- Loaded and unloaded country share one cycle.
- Water and salt audits close exactly.

### One material history

- Virgin geological resources are finite.
- Mining transfers known material out of deposits.
- Economically important durable material has repair/salvage paths.
- Loss is explicit, measured, and never hidden behind new chunks.
- Mods declare how new worldgen enters an already finite world.

### One living world

- Climate and hydrology follow physical causes.
- Biomes and habitats follow climate, water, ground, and elevation.
- The wild may accelerate, withdraw, warn, and impose the explicitly magical
  Long Winter.
- Heart magic does not accidentally violate planetary mass or ordinary
  climate causality.
- The wild never rewrites player-built blocks.

### One authority

- The server owns planetary simulation.
- Guests receive authoritative local state and never generate their own
  planet truth.
- Windowed solo, windowed host, and dedicated host use the same simulation
  path.

## Requirement ownership matrix

| Requirement | Owning goal | Final proof |
|---|---|---|
| Finite number of addressable blocks | Topology | Bounded coordinate tests and manifest constants |
| Closed circumnavigable surface | Topology | Four-face walk and live circumnavigation |
| Visible curvature/local horizon | Topology | Geometry tests and captures |
| Whole-world precomputation | Atlas | Persisted production atlas and exporter |
| Realistic continents/oceans | Geology | Global maps, census, causal terrain tests |
| Tectonic ranges/rifts/trenches/arcs | Geology | Boundary-to-landform tests |
| Finite mineral sites | Geology + materials | Deposit manifest and material audit |
| Latitude/elevation temperature | Climate | Seasonal global maps and relational tests |
| Prevailing winds/ocean influence | Climate | Current/wind maps and coastal tests |
| Rain shadows/desert placement | Climate | Mountain transects and biome tests |
| Rivers follow valleys to sinks | Hydrology | Drainage graph and longitudinal profiles |
| Real lakes/oceans/terminal basins | Hydrology | Basin volume/spill and salinity tests |
| Rain refills lakes through catchments | Water cycle | Conservative wet-season scenario |
| Aquifers/water table | Water cycle | Head, flow, pumping, and recharge tests |
| Natural finite springs/oases | Water cycle + biomes | Spring mass and oasis habitat scenarios |
| Closed atmospheric water cycle | Water cycle | Exact century-scale water audit |
| Correct regional/local biomes | Biomes | Climate/soil/water causality suite |
| Forested river valleys/wetlands | Biomes | Habitat overlay transects |
| Geographic countries and hearts | Biomes | Partition/heart reachability tests |
| Finite-world repair/recycling | Materials | Recipe/lifecycle conservation |
| Safe mod resources after creation | Materials | Retrogen qualification |
| All systems agree end to end | Qualification | Full audit and scenario matrix |
| Qualified responsive world entry | World entry remediation + qualification | Production cold solo/host/guest/agent matrix |

No requirement is “shared” without one document owning its implementation and
the final qualification goal proving the integration.

## Existing plans and supersession

Existing plans remain historical records. Where they conflict, the planetary
documents are current:

| Existing document | Planetary change |
|---|---|
| `terrain-v2-plan.md` | Retains fine 3D density/caves; replaces infinite large-scale splines and planar coordinates |
| `minerals-geology-plan.md` | Retains rock/mineral/process vocabulary; replaces planar tectonics and probabilistic infinity |
| `weather-seasons-plan.md` | Replaces global weather and global seasons with local planetary climate |
| `water-and-ticks-plan.md` | Retains visible finite fluid volume; replaces rain creation, evaporation deletion, and generation-horizon oceans |
| `hearts-plan.md` | Replaces biome-imposing Voronoi provinces and unconstrained climate terraforming |
| `economy-plan.md` | Keeps regional scarcity; makes total reserves finite and material recoverable |
| `trade-travel-plan.md` | Bearings and routes become geodesic; circumnavigation becomes possible |
| `modding-plan.md` | World coordinate API breaks; worldgen mods gain finite retrogen contracts |
| `multiplayer-plan.md` | Wire positions, interest, and interpolation become planetary |
| `agent-mcp-plan.md` | Perception/action coordinates and bearings become planetary |
| `scaling-plan.md` | Adds bounded atlas simulation and planet-creation budgets |

Each implementing goal updates the affected current docs/code paths. The final
qualification goal adds concise historical supersession notes where needed.

## Scope guards

The sequence includes everything required to make the finite living planet
coherent. It does not include:

- spaceflight or viewing the entire planet from orbit,
- simulated mantle motion during play,
- plate movement on human timescales,
- full atmospheric or ocean Navier–Stokes simulation,
- arbitrary planet sizes before the default is qualified,
- multiple shipping topology modes,
- an MMO-scale server promise,
- formal governments, factions, elections, or claims,
- new biome/item/animal quantity for its own sake.

Those can be designed after one qualified planet exists.

## Repository gates

Every goal keeps its targeted tests green. Goals that alter broad formats run
the full repository gates before completion:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
cargo deny check advisories
```

The final qualification goal runs all gates plus every new atlas, seam,
long-run, conservation, census, retrogen, multiplayer, and visual validator.

## Sequence completion

This sequence is complete only after goal 9 proves:

- the game has one finite, curved, circumnavigable planet,
- continents, oceans, geology, climate, rivers, lakes, aquifers, springs,
  biomes, and countries share one causal atlas,
- water, salt, and tracked finite material reconcile,
- the world remains stable across a century-scale simulation,
- player construction and multiplayer work across all seams,
- old infinite behavior no longer ships,
- all player, modder, agent, and operator documentation tells the new truth.

The 2026-08-01 qualification run satisfied this definition: the full suite,
release build, static/security gates, production save entry validator,
century-scale conservation harness, atlas diagnostics, multiplayer streaming
and corrected graphical evidence all passed.

## Follow-on sequence

Player magic, magical plants/minerals/places, conserved ambient power, and
magical pollution are intentionally not smuggled into the planet conversion.
After this nine-goal sequence is implemented and qualified, continue with:

```text
docs/magic-sequence.md
```

That sequence extends the finished atlas, biome, water, material, heart, Ire,
multiplayer, mod, and operator systems. It does not reopen the topology or use
magic to evade planetary conservation.
