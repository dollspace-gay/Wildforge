struct Uniforms {
    view_proj: mat4x4<f32>,
    // xyz = camera pos, w = fog distance
    cam: vec4<f32>,
    // rgb = sky/fog color, a unused
    sky: vec4<f32>,
    // x = underwater (0/1), y = daylight, zw = screen size in pixels
    misc: vec4<f32>,
    // xyz = normalized direction toward the sun (world space), w unused
    sun_dir: vec4<f32>,
    // rgb = warm direct-sun color, already scaled by daylight; a unused
    sun_col: vec4<f32>,
    // rgb = cool sky-ambient fill, already scaled by daylight; a unused
    amb_col: vec4<f32>,
    // world -> sun light-space clip, for shadow-map lookup
    light_vp: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var atlas_tex: texture_2d<f32>;
@group(1) @binding(1) var atlas_smp: sampler;
@group(2) @binding(0) var shadow_tex: texture_depth_2d;
@group(2) @binding(1) var shadow_smp: sampler_comparison;

const SHADOW_RES: f32 = 2048.0;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) light: f32,
    @location(4) sky: f32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: f32,
    @location(2) world: vec3<f32>,
    @location(3) sky: f32,
    @location(4) normal: vec3<f32>,
};

@vertex
fn vs_chunk(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = u.view_proj * vec4<f32>(in.pos, 1.0);
    out.uv = in.uv;
    out.light = in.light;
    out.sky = in.sky;
    out.world = in.pos;
    out.normal = in.normal;
    return out;
}

// Depth-only pass from the sun's point of view: positions into light-space
// clip. Reuses the chunk vertex buffer (only location 0 is read).
@vertex
fn vs_shadow(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    return u.light_vp * vec4<f32>(pos, 1.0);
}

// Fraction of the sun reaching a world point (1 = lit, 0 = fully shadowed),
// 3x3 PCF with a slope-scaled bias. `ndl` is the surface's sun incidence.
fn sample_shadow(world: vec3<f32>, ndl: f32) -> f32 {
    let lc = u.light_vp * vec4<f32>(world, 1.0);
    let p = lc.xyz / lc.w;
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    // Outside the shadow map (or behind the light) -> treat as lit.
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || p.z > 1.0 || p.z < 0.0) {
        return 1.0;
    }
    let bias = clamp(0.0016 * tan(acos(clamp(ndl, 0.0, 1.0))), 0.0004, 0.004);
    let ref_depth = p.z - bias;
    let texel = 1.0 / SHADOW_RES;
    var sum = 0.0;
    for (var dy = -1; dy <= 1; dy = dy + 1) {
        for (var dx = -1; dx <= 1; dx = dx + 1) {
            let off = vec2<f32>(f32(dx), f32(dy)) * texel;
            sum = sum + textureSampleCompare(shadow_tex, shadow_smp, uv + off, ref_depth);
        }
    }
    return sum / 9.0;
}

// Minecraft-style face brightness from a normal: top 1.0, bottom 0.5,
// Z-sides 0.8, X-sides 0.6. Gives torch-/ambient-lit faces their form
// without any real light direction.
fn face_shade(n: vec3<f32>) -> f32 {
    if (n.y > 0.5) { return 1.0; }
    if (n.y < -0.5) { return 0.5; }
    if (abs(n.z) > abs(n.x)) { return 0.8; }
    return 0.6;
}

// Full lit multiplier (per channel) for a world-space surface. A near-zero
// normal marks pre-shaded billboards/entities, which keep the old flat model.
fn world_light(normal: vec3<f32>, light: f32, sky: f32, world: vec3<f32>) -> vec3<f32> {
    if (dot(normal, normal) < 0.25) {
        return vec3<f32>(max(max(light, sky * u.misc.y), 0.03));
    }
    let n = normalize(normal);
    let fs = face_shade(n);
    // Warm sun: direct, gated by sky visibility, surface orientation, and the
    // shadow map (cast shadows). Ambient/torch are unaffected, so shadowed
    // ground fills with cool sky light instead of going black.
    let ndl = max(dot(n, u.sun_dir.xyz), 0.0);
    let shadow = sample_shadow(world, ndl);
    let sun = sky * ndl * shadow * u.sun_col.rgb;
    // Cool sky fill + steady (white) torch light, both with face shade.
    let amb = sky * fs * u.amb_col.rgb;
    let torch = vec3<f32>(light * fs);
    return max(sun + amb + torch, vec3<f32>(0.03));
}

fn apply_fog(color: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let dist = distance(world.xz, u.cam.xz);
    let fog = smoothstep(u.cam.w * 0.72, u.cam.w * 0.98, dist);
    return mix(color, u.sky.rgb, fog);
}

@fragment
fn fs_chunk(in: VsOut) -> @location(0) vec4<f32> {
    let tex = textureSample(atlas_tex, atlas_smp, in.uv);
    if (tex.a < 0.5) {
        discard; // alpha-tested item sprites share this pipeline
    }
    var rgb = tex.rgb * world_light(in.normal, in.light, in.sky, in.world);
    rgb = apply_fog(rgb, in.world);
    if (u.misc.x > 0.5) {
        rgb = mix(rgb, vec3<f32>(0.1, 0.2, 0.5), 0.55);
    }
    return vec4<f32>(rgb, 1.0);
}

@fragment
fn fs_water(in: VsOut) -> @location(0) vec4<f32> {
    let tex = textureSample(atlas_tex, atlas_smp, in.uv);
    var rgb = tex.rgb * world_light(in.normal, in.light, in.sky, in.world);
    rgb = apply_fog(rgb, in.world);
    if (u.misc.x > 0.5) {
        rgb = mix(rgb, vec3<f32>(0.1, 0.2, 0.5), 0.55);
    }
    return vec4<f32>(rgb, tex.a);
}

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
    out.clip = u.view_proj * vec4<f32>(in.pos, 1.0);
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
