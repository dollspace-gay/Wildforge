
struct PtFace {
    view_proj: mat4x4<f32>,
    light_pos: vec4<f32>,
};
@group(0) @binding(0) var<uniform> f: PtFace;

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
};

@vertex
fn vs_pt_shadow(@location(0) pos: vec3<f32>) -> VOut {
    var o: VOut;
    o.clip = f.view_proj * vec4<f32>(pos, 1.0);
    o.world = pos;
    return o;
}

@fragment
fn fs_pt_shadow(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(length(in.world - f.light_pos.xyz), 0.0, 0.0, 0.0);
}

// Transmission pass: glass surfaces multiply their filter color into the
// light's tint cube (order-independent), alpha Min-keeps the nearest
// glass distance so tint applies only between light and fragment.
@group(1) @binding(0) var atlas_tex: texture_2d<f32>;
@group(1) @binding(1) var atlas_smp: sampler;

struct TrOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_pt_tr(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>) -> TrOut {
    var o: TrOut;
    o.clip = f.view_proj * vec4<f32>(pos, 1.0);
    o.world = pos;
    o.uv = uv;
    return o;
}

@fragment
fn fs_pt_tr(in: TrOut) -> @location(0) vec4<f32> {
    let tex = textureSample(atlas_tex, atlas_smp, in.uv);
    // Panes are mostly-transparent texels, so raw alpha would wash the
    // tint to white. Take the tile's hue at full saturation and let
    // alpha set how strongly the pane stains the beam.
    let m = max(tex.r, max(tex.g, tex.b));
    let hue = tex.rgb / max(m, 1e-3);
    let strength = clamp(tex.a * 2.2, 0.0, 0.92);
    let tint = mix(vec3<f32>(1.0), hue, strength);
    return vec4<f32>(tint, length(in.world - f.light_pos.xyz));
}
