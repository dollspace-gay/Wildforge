<!-- wildforge:guide -->

# Guest scene state

ReplicaWorld in the parent file owns streamed terrain and received observations.
scene.rs supplies bounded scene queries, mesh bookkeeping, and entity presentation
updates. BlockRead supports shape inspection without physical machine mutation.
No generator, save writer, conservation ledger, or entity allocator belongs here.

Read [AGENTS.md](AGENTS.md). Final checks cover ordered snapshots, remapping,
weather/clock fallbacks, missing host-only data, machine recognition, and native
agent/graphical scene parity.
