<!-- wildforge:guide -->

# Working on resident terrain

Read [README.md](README.md) and [world guidance](../AGENTS.md). Keep this owner
independent of generation, authoritative simulation, persistence policy, and
network transport. A wire codec is a byte adapter, not an authenticated session.

Preserve voxel planes, wire versions, fallback light behavior, neighbor order,
and the cascade visit cap. Dirty mesh state is derived presentation work; do
not turn it into the persisted modified flag. Do not expose a mutable map or
DerefMut outside the domain coordinator. Existing raw-map access is test-only.

Keep cohesive files near 400 lines and review above 500. Run existing codec,
lighting, and rendering scenarios plus the full gates during final validation;
record incomplete checks honestly in the migration record.
