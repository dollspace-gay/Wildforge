<!-- wildforge:guide -->

# Working in climate

Read [README.md](README.md) and [the atlas guidance](../AGENTS.md).
Keep immutable normals separate from dynamic weather. Preserve arithmetic order,
seed salts, convergence tolerance, topology, iteration order, and transfer units.
Only the completed-hour coordinator publishes scratch state. Keep failure latching
and checkpoint restoration intact. External ecology/industry transfers must
respect whether the affected cell has already been processed this hour.
Use explicit imports and keep helpers within the climate boundary. Target 400
lines per cohesive source module; review above 500. Run focused atlas/climate/
water scenarios and the applicable Rust/runtime gates before acceptance.
