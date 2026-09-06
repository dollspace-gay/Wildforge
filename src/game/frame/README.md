<!-- wildforge:guide -->

# Graphical frame preparation

The parent frame module retains update ordering. Feedback, authoritative session
advancement, content refresh, and player motion have separate coordinators.
Rendering first prepares lighting and selection values, then emits resident
entities/avatars, station detail, precipitation, overlays, and hands. UI effects
and light selection follow, then low-light adaptation, renderer submission, and
capture/title bookkeeping. Keep this order explicit in `render.rs`.

LightingFrame and SelectionFrame contain values sampled for this frame. Geometry
owns each batch's vertices and indices; emission helpers borrow only the batch
they extend. Renderer FrameInput remains the submission contract. Shared WorldView
queries supply scene observations without giving renderer passes authority.

The viewmodel is one cohesive geometry routine with several authored silhouettes;
its file remains within the 500-line review boundary. Camera transforms, sky/SH
values, draw order, RNG consumption, light selection, and capture eligibility are
compatibility requirements for structural work.

Read [AGENTS.md](AGENTS.md). Final validation includes Rust checks, movement and
framing regressions, native gameplay proofs, assembled WGSL validation, and the
full fresh capture/motion/timing campaign on the actual GPU.
