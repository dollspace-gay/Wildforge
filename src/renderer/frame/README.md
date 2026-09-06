<!-- wildforge:guide -->

# GPU frame stages

The parent frame coordinator performs uniform/geometry uploads, cascade shadows,
point shadows, world and hand draws, bloom, late swapchain acquisition, composite
and UI, optional capture encoding, queue submission/presentation, then readback.
It shares one loaded-chunk list; each pass retains its original culling policy.

Preparation owns the frame uniform representation and upload order. Scene and
composite modules only encode their passes. Diagnostic replay uses the submitted
geometry and depth ordering, and capture owns typed GPU readback lifetimes and
evidence publication. It does not infer visibility from an unoccluded atlas.

PointShadows, in the parent renderer directory, owns its GPU resources and mutable
cube cache. The frame borrows those resources without exposing cache mutation to
scene or diagnostic passes. No pass receives an authoritative simulation object.

Run rendering tests, WGSL validation, the complete Rust gates, and the actual GPU
qualification campaign from the repository root. Record committed source, binary,
hardware, capture and timing evidence in the progress record.
