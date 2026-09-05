<!-- wildforge:guide -->

# Working on scene queries

Read [README.md](README.md) and [world guidance](../AGENTS.md). Keep the source
private and expose only domain observations. Do not add mutable accessors or
implement BlockStore merely to satisfy a read consumer. Narrow the consumer.

Retain explicit unavailability of host-only data in the replica. Shared rules
belong to their domain helpers; this layer selects inputs and forwards queries.
Preserve gameplay/visible fallback behavior in structural moves. Keep cohesive
files near 400 lines and review above 500. Record final validation honestly.
