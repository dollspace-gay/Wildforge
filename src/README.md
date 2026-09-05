<!-- wildforge:guide -->

# Engine source

lib.rs assembles the single package; main.rs dispatches into it. Server simulation, world state, content, clients, transport, and rendering have separate responsibilities.

`content_files.rs` owns the historical mod file inventory and content hash for
both genesis compatibility and network transfer, without depending on transport.
`planet.rs` owns the serialized topology identity used by world and atlas saves.

Start with `alchemy.rs`, `arcane.rs`, `arcane_ecology.rs`, `arcane_geography.rs`, `audio.rs`, `bounce.rs`, `camera.rs`, `chunk.rs`. See the [repository overview](../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Keep simulation and generation independent of window/audio/GPU state. Use domain owners and explicit imports as the migration progresses.

## Focused checks

```sh
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
