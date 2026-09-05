<!-- wildforge:guide -->

# Working in src/renderer

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

Setup, resources, frame passes, and post-processing consume meshes and presentation snapshots through wgpu.

Keep GPU state downstream of simulation. Validate affected WGSL and actual hardware captures; do not substitute CPU work or weaken rendering requirements.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo test --locked tests::rendering::` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
