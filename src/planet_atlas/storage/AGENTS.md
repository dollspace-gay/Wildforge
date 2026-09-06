<!-- wildforge:guide -->

# Working in storage

Read [README.md](README.md) and [the atlas guidance](../AGENTS.md).
Preserve write/backup/recovery order, directory durability, file limits, and
separate immutable/mutable ownership. Failed loads must not mint replacement
water or regenerate genesis. Keep format details in codecs and compatibility
rules in the manifest module. Record persistence failures accurately.
Use explicit imports and cohesive modules around 400 lines; review above 500.
Run focused persistence/atlas/codec scenarios and applicable full gates before
acceptance, and distinguish implementation from verified evidence.
