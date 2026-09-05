struct Uniforms {
    view_proj: mat4x4<f32>,
    // xyz = camera pos, w = fog distance
    cam: vec4<f32>,
    // Absolute embedded camera position, subtracted before projection.
    origin: vec4<f32>,
    // Radial up at the camera. The visible sky is local to the curved surface.
    local_up: vec4<f32>,
    // rgb = flat sky/overcast color (fog far target under cloud), a = weather
    // gloom 0..1 (blends the gradient toward the flat overcast color)
    sky: vec4<f32>,
    // x = underwater (0/1), y = daylight, zw = screen size in pixels
    misc: vec4<f32>,
    // xyz = normalized direction toward the sun (world space), w unused
    sun_dir: vec4<f32>,
    // rgb = warm direct-sun color, already scaled by daylight;
    // a = how much of the ambient floor survives in a fully occluded corner
    sun_col: vec4<f32>,
    // rgb = cool sky-ambient fill, already scaled by daylight;
    // a = the ambient floor (the stark<->soft darkness knob)
    amb_col: vec4<f32>,
    // world -> sun light-space clip, one per shadow cascade (tightest/densest
    // first). A fragment samples the first cascade whose box contains it.
    light_vp: array<mat4x4<f32>, 3>,
    // x = active point-light count, z = shadow-debug viz mode (dev), w = 1 when
    // point-light shadows use the voxel-grid DDA instead of the distance cube.
    pt_count: vec4<u32>,
    // per light: xyz = world position, w = range
    pt_pos: array<vec4<f32>, 8>,
    // per light: rgb = color × intensity, w unused
    pt_col: array<vec4<f32>, 8>,
    // per light: x = flood-suppression scale, y = its range,
    // z = shadows enabled, w unused
    pt_misc: array<vec4<f32>, 8>,
    // Inverse view-projection: unprojects fullscreen NDC to world rays for
    // the procedural sky pass.
    inv_view_proj: mat4x4<f32>,
    // xyz = true (unclamped) direction toward the sun, dipping below the
    // horizon at night; drives the sky gradient. The lighting sun_dir above
    // stays clamped just over the horizon so shadows never degenerate. w unused.
    sun_dir_true: vec4<f32>,
    // Sky irradiance as 9 RGB spherical-harmonic coefficients (cosine-convolved
    // to a diffuse light multiplier). Evaluated per-normal for the ambient fill.
    sh: array<vec4<f32>, 9>,
    // xyz = world cell of occupancy texel (0,0,0); w = first atlas slot of the
    // interior-layer run, so a tile's material alpha can name one in a byte.
    occ_origin: vec4<i32>,
    // Two entries per interior layer, indexed by (material alpha - 1) * 2:
    //   [0] = (depth, opacity_min, opacity_max, dim)
    //   [1] = (mode, cutoff, _, _)  mode 0 = surface alpha, 1 = surface luminance
    layer: array<vec4<f32>, 64>,
    // The room's own light as SH-L1 (L0 then the three L1 terms), in the same
    // diffuse-multiplier convention as `sh` above. One probe's worth, from
    // where the player is standing. [0].w carries how much sun is landing
    // nearby, 0..1, measured without reference to what colour it comes back as.
    room_sh: array<vec4<f32>, 4>,
};

const MAX_PT_LIGHTS: u32 = 8u;
const OCC_GRID: i32 = 128;

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var atlas_tex: texture_2d<f32>;
@group(1) @binding(1) var atlas_smp: sampler;
// Material atlas (linear): R = parallax height (1 = surface, 0 = deepest),
// G = interior mask (legacy, procedural ice), B = authored-normal strength
// (0 = none), A = 1 + index into the interior-layer slot run, or 0 for none.
// A flat tile (R = 1, G = 0, B = 0) is a no-op, so all of it is opt-in per texture.
@group(1) @binding(2) var material_tex: texture_2d<f32>;
// Normal atlas (linear): tangent-space normals in the standard OpenGL / +Y
// encoding, so a stock or model-generated map drops in unmodified. Flat (128,
// 128, 255) wherever nothing is authored; material.b says where that is, so the
// plain-tile early-out never has to read this texture.
@group(1) @binding(3) var normal_tex: texture_2d<f32>;
@group(2) @binding(0) var shadow_tex: texture_depth_2d_array;
@group(2) @binding(1) var shadow_smp: sampler_comparison;
@group(2) @binding(2) var pt_cube: texture_cube_array<f32>;
@group(2) @binding(3) var pt_smp: sampler;
@group(2) @binding(4) var pt_tr_cube: texture_cube_array<f32>;
// Voxel occupancy grid (R8Uint, 1 = opaque full block) covering a cube of the
// world around the camera, for exact point-light shadow marching. u.occ_origin
// is the world-cell coordinate of texel (0,0,0).
@group(2) @binding(5) var occ_tex: texture_3d<u32>;

// What the flat fill falls to where no sun is landing at all, as a fraction of
// itself. Not zero: a cave with a torch still has air in it, and the sky term
// carries the night on its own.
const AMB_MIN: f32 = 0.3;
// The landing fraction at which the fill is considered fully paid for.
const AMB_FULL: f32 = 0.35;

const SHADOW_RES: f32 = 2048.0;
const SHADOW_CASCADES: u32 = 3u;
const ATLAS_TILES: f32 = 64.0; // keep in sync with atlas::ATLAS_TILES
// Apparent displacement depth, in blocks (a face is one tile wide), so the uv
// offset is scaled into a single tile's span and can't drag across tiles.
const PARALLAX_DEPTH: f32 = 0.08;
const PARALLAX_STEPS: i32 = 24;
// How hard the height gradient tilts the surface normal (relief lighting).
const NORMAL_STRENGTH: f32 = 4.0;
// Multilayer: the internal crack stratum sits this many blocks below the smooth
// surface, so it parallaxes further and slides beneath it (depth, not overlay).
// The interior wraps within its (periodic) tile, so depth is unconstrained by
// the tile size now — this sets how far the internal layer slides under the surface.
const INTERIOR_DEPTH: f32 = 0.30;
// How opaque the surface veil is over the interior: 1 = surface only, 0 = interior
// only. The interior is always partly visible through it (real translucency).
const SURFACE_VEIL: f32 = 0.45;
// Layer appearance is per-tile now (u.layer, authored in pack.toml), because
// what the surface's opacity MEANS is a property of the material: ice is a
// translucent sheet whose brightness says how thin it is, foliage is a cutout
// with real holes. Deriving opacity from luminance for everything was an
// ice-shaped policy compiled in as if it were the mechanism.
// Fallback depth for a layer with no pack.toml entry.
const LAYER_DEPTH_DEFAULT: f32 = 0.13;
// The interior is the block's own lit colour, modulated by its internal
// structure (G): dimmer/clearer in the gaps, brighter/frosted where dense.
const INTERIOR_LO: f32 = 0.35;
const INTERIOR_HI: f32 = 1.9;

