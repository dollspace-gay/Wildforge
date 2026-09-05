<!-- wildforge:guide -->

# Working on content files

Read [README.md](README.md) and [the source instructions](../AGENTS.md).
Preserve historical hashing and transfer order. Keep filesystem ownership
independent of transport, renderer, and game state. Only an exclusively created
snapshot may be cleaned by its owner; do not reclaim collisions or stale paths.
Copy validated relative paths before publishing a registry and retain the owner
for every reader. Do not link snapshot files to mutable cache files.

Keep source near 400 physical lines. Include failure and reader-lifetime cases
in the final validation phase, using disposable directories.
