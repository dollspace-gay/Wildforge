struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) light: vec3<f32>,
    @location(4) sky: f32,
    @location(5) ao: f32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: vec3<f32>,
    @location(2) world: vec3<f32>,
    @location(3) sky: f32,
    @location(4) normal: vec3<f32>,
    @location(5) ao: f32,
};

@vertex
fn vs_chunk(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = u.view_proj * vec4<f32>(in.pos - u.origin.xyz, 1.0);
    out.uv = in.uv;
    out.light = in.light;
    out.sky = in.sky;
    out.world = in.pos;
    out.normal = in.normal;
    out.ao = in.ao;
    return out;
}

