<!-- wildforge:guide -->

# Working in world storage

Read [README.md](README.md), [world guidance](../AGENTS.md), and
[root instructions](../../../AGENTS.md). Keep codecs and immutable readers
independent of application, rendering, network-session, and simulation state.

Preserve byte formats and palette conversions during extraction. A failed read
must not become permission to replace persisted data. Test missing, malformed,
and unreadable inputs alongside old-format round trips. Keep new files near
400 lines and add both guides for any new maintained subdirectory.

Route live region reads and writes through RegionStore. Preserve per-region
parallelism, invalidate watched chunk revisions before writes, and never give
workers authoritative write access. Revision tokens and weak region/watch maps
must stay bounded by active work. Test edits saved and unloaded before adoption.

Prepared terrain is tied to its decoding registry and palette as well as its
saved bytes. Replace palette snapshots instead of mutating shared tables. A
loader predating the first palette publication must never replace a saved edit
with obsolete prepared work. Fresh mappings must already describe runtime IDs.

Stored palette bindings are append-only. Preserve removed names and vacant IDs;
never rewrite a global palette in current runtime order while cold chunks still
use previous IDs. Route every authoritative chunk write through palette
publication, including unload, retrogen, and material transactions. Disk encoding
maps to stored IDs; network encoding keeps runtime IDs. Test both paths and
preserve retryable save failures without publishing undecodable chunk bytes.
