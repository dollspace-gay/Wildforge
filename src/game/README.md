<!-- wildforge:guide -->

# Graphical application

The windowed app coordinates input, UI, sessions, streaming, actions, and presentation. Existing broad Game access is being replaced with cohesive owners.

`mesh_jobs.rs` owns CPU snapshot processing and per-chunk deduplication through
the shared background owner. `streaming.rs` supplies immutable inputs, validates
finished meshes, and uploads them through the existing renderer. Startup/runtime
worker failures reach the entry screen or pause notification; native GPU proof
is required in addition to worker buffer/lifecycle tests.

Start with `actions.rs`, `app.rs`, `browser.rs`, `capture.rs`, `combat.rs`, `containers.rs`, `content.rs`, `demos.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Keep simulation order explicit. Guests apply host state, while UI and rendering consume it. Preserve real interaction paths when extracting helpers.

## Focused checks

```sh
python3 tools/run_gameplay_proofs.py
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
