<!-- wildforge:guide -->

# Working in world storage

Read [README.md](README.md), [world guidance](../AGENTS.md), and
[root instructions](../../../AGENTS.md). Keep codecs and immutable readers
independent of application, rendering, network-session, and simulation state.

Preserve byte formats and palette conversions during extraction. A failed read
must not become permission to replace persisted data. Test missing, malformed,
and unreadable inputs alongside old-format round trips. Keep new files near
400 lines and add both guides for any new maintained subdirectory.
