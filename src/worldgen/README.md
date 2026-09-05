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
