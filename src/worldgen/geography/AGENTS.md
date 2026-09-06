<!-- wildforge:guide -->

# Working on geography queries

Read [README.md](README.md) and [generation guidance](../AGENTS.md). This owner
is immutable except for its derived province cache. Keep cache keying and tie
order deterministic; do not expose the map or add simulation/persistence here.

Preserve seed salts, spline points, coordinate packing, floating-point operation
order, atlas overrides, and biome/province classification during extraction.
Queries may be shared with replicas without giving them a chunk generator.
Keep files near 400 physical lines, review above 500, and record final behavioral
and deterministic checks in the migration record.
