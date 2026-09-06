<!-- wildforge:guide -->

# Deterministic chunk generation

`Generator::generate` in the parent file coordinates shape, cave carving,
surface materials, mineral deposits, vegetation, province structures, finite
water/salt finalization, and bedrock in that order. Stages borrow the immutable
Generator context. ShapeColumns and SurfaceColumns name the intermediate maps;
only the prepared Chunk leaves generation for authoritative adoption.

Stages do not schedule work, read saves, publish world effects, or use GPU/UI
state. Keep seed salts, coordinate canonicalization, loop order, lattice apron,
and atlas allocation stable. The Deep remains empty until dungeon stamping.
Read [AGENTS.md](AGENTS.md). Final checks compare pinned chunk/atlas outputs
across worker counts and request order, then run the applicable runtime gates.

Supporting modules separate climate classification, province queries, relief,
hydrology, density lattices, stratigraphy, and mineral prospecting. `context.rs`
constructs the seeded fields and resolved content bindings. `landmarks.rs` owns
pure heart forms/heights shared with living-heart simulation; generation does
not import the authoritative world owner. The parent retains public value types,
immutable generator storage, and the explicit stage coordinator.

The immutable temperature field and seam-safe surface noise live in the source
climate module so replica weather fallback shares the same seed and sampling
formula without constructing a Generator. Atlas-backed climate stays here.

`Geography` now owns climate fields, relief splines, and the derived province
cache with their query algorithms. It has no registry/material bindings or chunk
generation method. Generator forwards its established query API through
`queries.rs`; generation stages borrow the geography-owned detail fields.
Replicas use Geography for the same atlas-free biome and climate observations.
