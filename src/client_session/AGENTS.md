<!-- wildforge:guide -->

# Working in the guest session

Read [README.md](README.md) and [the source guidance](../AGENTS.md).

Keep shared protocol state independent of rendering, audio, MCP, navigation,
and authoritative simulation. Bind mappings to the registry that resolved them;
unknown blocks use its placeholder and unknown items remain absent. Preserve
host-assigned instance IDs, stack quantities, and consumer-specific behavior.

Keep admission transitions explicit. Only decoded resident chunks satisfy the
manifest; graphics must supply its first-frame milestone before acknowledging.
Discard queued terrain and receivers when Welcome starts a new world.
Keep this reset inside `GuestSession::begin`; adapters must not reconstruct a
subset of session state independently. Retain host IDs in queued block updates
so registry reload cannot make deferred work refer to another local definition.
Keep chunk snapshots and edits ordered when a decode budget spans several
pumps. Do not apply an edit ahead of its chunk or replay it after a later
snapshot. Preserve batched lighting for contiguous terrain and edit groups.
Test malformed, partial, duplicate, out-of-order, and reconnect sequences for
both consumers. Keep new modules near 400 lines and document new subdirectories.
