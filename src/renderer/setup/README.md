<!-- wildforge:guide -->

# GPU setup stages

The parent setup coordinator selects hardware and constructs resources in their
original order. Typed construction results expose the bindings, layouts, targets,
and pipelines needed for assembly, without passing a partially built Renderer.

RasterFactory shares the one-vertex-buffer descriptor rules between ordinary and
diagnostic pipelines. Each call retains an explicit recipe for vertex/fragment
entry points, topology, culling, depth, blending, and target format. Sky and shadow
pipelines keep their distinct buffer, attachment, and depth contracts. Diagnostic
pipeline creation remains conditional on evidence capture.

Shadow targets and per-face uniforms are assembled into the point-shadow owner;
the owner keeps cache state private. Bound wgpu resources retain their lifetime
through the existing bind groups and retained render-target views.

Run rendering tests, WGSL validation, the complete Rust gates, and the actual GPU
qualification campaign from the repository root. Record committed source, binary,
hardware, capture and timing evidence in the progress record.
