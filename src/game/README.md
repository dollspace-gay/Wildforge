<!-- wildforge:guide -->

# Graphical application

The windowed app coordinates input, UI, sessions, streaming, actions, and presentation. Existing broad Game access is being replaced with cohesive owners.

`mesh_jobs.rs` owns CPU snapshot processing and per-chunk deduplication through
the shared background owner. `streaming.rs` supplies immutable inputs, validates
finished meshes, and uploads them through the existing renderer. Startup/runtime
worker failures reach the entry screen or pause notification; native GPU proof
is required in addition to worker buffer/lifecycle tests.

Terrain streaming compares the live immutable preparation context before using
its worker owner. Mesh completion retains the input registry and variant signature;
the pool discards mismatched results and releases their slots before the adapter
checks the live chunk's dirty state and performs the existing GPU upload.

`world_loading.rs` owns one creation or entry operation until its worker joins.
`world_loading_work.rs` prepares a private world; `world_loading_ui.rs` maps
requests and terminal outcomes to screens and session adoption. UI state holds
only presentation status. Requested registry identity guards entry across content
reload, separately from placeholder definitions restored into the loaded world.

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
