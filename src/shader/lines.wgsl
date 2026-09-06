// ---- solid-color lines (block outline in world space, crosshair in clip space) ----

struct LineIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec3<f32>,
};

struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_line_world(in: LineIn) -> LineOut {
    var out: LineOut;
    out.clip = u.view_proj * vec4<f32>(in.pos - u.origin.xyz, 1.0);
    out.color = in.color;
    return out;
}

@vertex
fn vs_line_screen(in: LineIn) -> LineOut {
    var out: LineOut;
    out.clip = vec4<f32>(in.pos.xy, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_line(in: LineOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}

@fragment
fn fs_diagnostic_line(_in: LineOut) -> @location(0) vec4<u32> {
    return vec4<u32>(65535u, 0u, 3u, 1u);
}

