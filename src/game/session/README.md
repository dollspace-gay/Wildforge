<!-- wildforge:guide -->

# Graphical session lifecycle

Entry adopts a fully prepared World/Server and resets player/presentation state
in the existing order. Configuration/world browsing and departure remain separate
application operations. Player save/load and the historical client loose-item
sidecar codec retain their paths, schemas and identity/custody behavior.

GuestSession and ReplicaWorld own remote play. Saving/quit paths check runtime
kind, including while the Remote handle is temporarily taken by its pump. World
loading and worker cancellation/joining remain owned by world_loading, not these
persistence adapters. Never publish an incomplete entry or lose a failed save.

Run applicable UI/session/inventory characterization and parity tests, then the
root Rust gates and final native gameplay/GPU proofs from the repository root.
