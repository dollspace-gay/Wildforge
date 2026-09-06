<!-- wildforge:guide -->

# World shader units

`mod.rs` concatenates authored WGSL units at Rust compile time. It preserves
file bytes and one explicit order; no generated aggregate is checked in and no
shader source is read from disk at runtime. Renderer creation and Naga tests
consume the same `WORLD` source.

- `bindings.wgsl`: uniform/binding ABI and shared constants.
- `geometry.wgsl`: chunk vertex inputs and world-space projection.
- `shadows.wgsl`: sun/point visibility, irradiance helpers, and voxel transport.
- `lighting.wgsl`: combined surface lighting and debug visualization.
- `sky.wgsl`: sky radiance, geodesic fog, and background pass.
- `materials.wgsl`: layered materials, parallax, and normal mapping.
- `terrain.wgsl`: terrain and water fragment entry points.
- `diagnostics.wgsl`: capture-only visible-fragment evidence.
- `lines.wgsl` and `ui.wgsl`: line and UI passes with diagnostic counterparts.

Keep the binding layout aligned with `renderer::Uniforms` and resource setup.
Keep sky/fog formulas aligned with their documented CPU reference in `sky.rs`.
Final validation compares aggregate bytes, runs Naga and rendering contracts,
and captures/times the native GPU paths. Read [AGENTS.md](AGENTS.md).

`point_shadow.wgsl` and `cascade_shadow.wgsl` remain separate shader modules with
their own uniform ABIs. Their constants sit beside the world aggregate so GPU
setup does not embed authored WGSL inside Rust constructors.
