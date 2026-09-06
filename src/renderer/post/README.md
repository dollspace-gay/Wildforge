<!-- wildforge:guide -->

# Post-processing construction

The parent `post.rs` owns HDR/bloom targets, post pipelines, exposure parameters,
resize, and pass encoding. `setup.rs` builds size-independent bindings/pipelines
and the initial targets. Private fields keep layouts, samplers, and their target
bindings in the same lifetime.

The frame coordinator calls bloom, acquires the swapchain, then calls composite
before UI. Capture replay composites the same scene to its capture target.
Bloom-off still clears the sampled bloom target. Read [AGENTS.md](AGENTS.md).
Final validation covers resize, bloom off/on, exposure, UI/capture order, shader
contracts, and fresh hardware GPU captures/timings.
