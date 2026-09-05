<!-- wildforge:guide -->

# Working in biomes

Read [README.md](README.md) and [the atlas guidance](../AGENTS.md).
Preserve seed salts, traversal/tie-breaking order, stable biome and country IDs,
soil units, and float operation order. Habitat flags describe physical ground
and water conditions; generation must not invent a water source. Country hearts
must stay in their assigned terrestrial country and adjacency must be reciprocal.
Keep classification and queries independent of rendering or authoritative world
mutation. Use explicit imports and modules around 400 lines; review above 500.
Run focused biome/atlas/codec scenarios and applicable full gates before accepting
changes, and record unverified work separately from completed evidence.
