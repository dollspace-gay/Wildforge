<!-- wildforge:guide -->

# Working on guest scene state

Read [README.md](README.md) and [world guidance](../AGENTS.md). Preserve host IDs,
received quantities, and bounded observation rules. Cosmetic interpolation and
received cargo updates must not become simulation or grant authority to a guest.

Keep the replica free of generation/persistence APIs and conservation ledgers.
Do not emulate physical drops while applying host blocks. Keep cohesive source
near 400 lines, review above 500, and run the final protocol/native/GPU gates
before accepting the migration.
