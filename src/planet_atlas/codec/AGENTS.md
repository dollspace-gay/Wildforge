<!-- wildforge:guide -->

# Working in codec

Read [README.md](README.md) and [the atlas guidance](../AGENTS.md).
Preserve magic, versions, field widths/order, checksums, byte limits, and error
behavior. Keep encoding independent of generation, rendering, and live mutation.
Do not regenerate missing or corrupt authoritative data. Helpers are atlas-scoped.
Use explicit imports and cohesive modules around 400 lines; review above 500.
Run focused persistence/atlas/codec scenarios and applicable full gates before
acceptance, and distinguish implementation from verified evidence.
