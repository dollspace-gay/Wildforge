<!-- wildforge:guide -->

# GPU renderer

Setup, resources, frame passes, and post-processing consume meshes and presentation snapshots through wgpu.

Start with `frame.rs`, `mod.rs`, `post.rs`, `resources.rs`, `setup.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Keep GPU state downstream of simulation. Validate affected WGSL and actual hardware captures; do not substitute CPU work or weaken rendering requirements.

## Focused checks

```sh
cargo test --locked tests::rendering::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
