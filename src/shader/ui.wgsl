// ---- 2D UI: colored or atlas-textured quads in pixel coordinates ----

struct UiIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct UiOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_ui(in: UiIn) -> UiOut {
    var out: UiOut;
    let ndc = vec2<f32>(
        in.pos.x / u.misc.z * 2.0 - 1.0,
        1.0 - in.pos.y / u.misc.w * 2.0,
    );
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

@fragment
fn fs_ui(in: UiOut) -> @location(0) vec4<f32> {
    let tex = textureSample(atlas_tex, atlas_smp, max(in.uv, vec2<f32>(0.0)));
    var c = in.color;
    if (in.uv.x >= 0.0) {
        c = tex * in.color;
    }
    return c;
}

@fragment
fn fs_diagnostic_ui(in: UiOut) -> @location(0) vec4<u32> {
    let tex = textureSample(atlas_tex, atlas_smp, max(in.uv, vec2<f32>(0.0)));
    var alpha = in.color.a;
    if (in.uv.x >= 0.0) {
        alpha = alpha * tex.a;
    }
    if (alpha <= 0.01) {
        discard;
    }
    return vec4<u32>(65535u, 0u, 3u, 1u);
}
