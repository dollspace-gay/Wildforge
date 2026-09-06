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

The world shader is the compile-time aggregate `crate::shader::WORLD` from
`src/shader/`; production and validation share its source. Shader units own
bindings, geometry, shadows/light, sky/fog, materials, and pass entry points.

`device.rs` owns surface-compatible hardware selection, device limits, and
initial surface configuration. It preserves the adapter ranking and refusal of
software CPU adapters before handing the configured context to renderer setup.

`post.rs` and `post/setup.rs` own the complete post-processing lifetime: pipelines,
parameter bindings, HDR/bloom targets, resize, and encoding. Frame orchestration
passes scalar exposure inputs and explicitly orders bloom, swapchain acquisition,
composite, and UI; capture replay composites through the same owner.

`frame.rs` is the ordered submission coordinator; `frame/` contains uniform and
geometry upload, sun/world/hand/composite encoding, diagnostic replay, and capture
readback. `point_shadows.rs` owns cube-cache state with its GPU resources.
`setup/` returns typed construction results and shares raster descriptor rules
between ordinary and capture pipelines while preserving their explicit recipes.
