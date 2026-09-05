<!-- wildforge:guide -->

# Working in the guest session

Read [README.md](README.md) and [the source guidance](../AGENTS.md).

Keep shared protocol state independent of rendering, audio, MCP, navigation,
and authoritative simulation. Bind mappings to the registry that resolved them;
unknown blocks use its placeholder and unknown items remain absent. Preserve
host-assigned instance IDs, stack quantities, and consumer-specific behavior.

Use explicit session transitions as admission and snapshot ownership migrate.
Test malformed, partial, duplicate, out-of-order, and reconnect sequences for
both consumers. Keep new modules near 400 lines and document new subdirectories.
