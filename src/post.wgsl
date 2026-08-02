// Post-processing: HDR scene -> bloom -> composite to the swapchain.
//
// The world is drawn to a linear Rgba16Float target so emitters (whose
// self-lit tiles are pushed past 1.0 in the chunk shader) and stacked
// point lights keep their overbright energy instead of clipping at the
// sRGB store. Bloom keys off exactly that headroom: only fragments above
// 1.0 bleed, so the glow reads as firelight without muddying lit-but-not-
// glowing surfaces. The core stays crisp; the halo is all that spreads.

// group 0: the primary input (scene for bright/composite, prior blur for blur)
@group(0) @binding(0) var tex0: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
// group 1: the second input, composite only (the blurred bloom)
@group(1) @binding(0) var tex1: texture_2d<f32>;
// group 2: composite params — x = bloom intensity (0 disables the add)
struct PostParams { p: vec4<f32> };
@group(2) @binding(0) var<uniform> post: PostParams;

// Only fragments this bright bloom; keeps the crisp mid-range untouched.
const THRESHOLD: f32 = 1.0;


struct FsQuad {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// A single oversized triangle covers the screen — no vertex buffer needed.
@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> FsQuad {
    var p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: FsQuad;
    out.clip = vec4<f32>(p[vi], 0.0, 1.0);
    // Texture v grows downward; clip y grows upward.
    out.uv = vec2<f32>(p[vi].x * 0.5 + 0.5, 0.5 - p[vi].y * 0.5);
    return out;
}

// Bright pass: keep only the energy above THRESHOLD, with a soft knee so the
// bloom mask has no hard edge. Runs at the bloom target's (half) resolution.
@fragment
fn fs_bright(in: FsQuad) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(tex0, samp, in.uv, 0.0).rgb;
    // Threshold on how bright this will actually be once exposed, not on the
    // raw scene value. The threshold means "brighter than white on screen",
    // and that only stays true if it is measured after exposure — otherwise
    // raising the sun makes every white surface in daylight cross it, and
    // ordinary things like a pale flower pick up a halo they have not earned.
    // The output stays in scene units so the composite exposes it exactly once.
    let l = max(max(c.r, c.g), c.b) * post.p.z;
    let knee = max(l - THRESHOLD, 0.0) / max(l, 1e-4);
    return vec4<f32>(c * knee, 1.0);
}

// 9-tap Gaussian, weights symmetric about the center.
fn gauss(dir: vec2<f32>, uv: vec2<f32>) -> vec3<f32> {
    let ts = dir / vec2<f32>(textureDimensions(tex0, 0));
    let w = array<f32, 5>(0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);
    var sum = textureSampleLevel(tex0, samp, uv, 0.0).rgb * w[0];
    for (var i = 1; i < 5; i = i + 1) {
        let o = ts * f32(i);
        sum = sum + textureSampleLevel(tex0, samp, uv + o, 0.0).rgb * w[i];
        sum = sum + textureSampleLevel(tex0, samp, uv - o, 0.0).rgb * w[i];
    }
    return sum;
}

@fragment
fn fs_blur_h(in: FsQuad) -> @location(0) vec4<f32> {
    return vec4<f32>(gauss(vec2<f32>(1.0, 0.0), in.uv), 1.0);
}

@fragment
fn fs_blur_v(in: FsQuad) -> @location(0) vec4<f32> {
    return vec4<f32>(gauss(vec2<f32>(0.0, 1.0), in.uv), 1.0);
}

// Filmic tone curve (the ACES approximation). Maps an unbounded linear scene
// onto the display by rolling the highlights off instead of cutting them, which
// is what makes a value above 1 mean anything at all. Clamping — what this pass
// used to do — throws that range away: a surface twice as bright as white and
// one ten times as bright both came out the same flat white, so there was no
// point in the sun being brighter than the sky. The shoulder is also where the
// contrast comes from; a clamped image is flat right up to the point it dies.
fn tonemap(x: vec3<f32>) -> vec3<f32> {
    // Extended Reinhard, not the filmic ACES curve. ACES has a toe that crushes
    // the very dark end hard — around a hundredth of white it multiplies by
    // about a third — and a room lit by ambient alone lives entirely down
    // there, so interiors went to mud while the daylight looked fine. This
    // keeps the shadows where the lighting put them and still rolls the
    // highlights off, which was the point: WHITE is the scene value that is
    // allowed to reach display white, and anything past it compresses instead
    // of clipping.
    let wp = max(post.p.w, 0.25);
    let w = wp * wp;
    return clamp(x * (1.0 + x / w) / (1.0 + x), vec3<f32>(0.0), vec3<f32>(1.0));
}

// Composite: expose, add bloom, tone-map, grade. The swapchain is sRGB, so the
// linear value written here is encoded on store.
@fragment
fn fs_composite(in: FsQuad) -> @location(0) vec4<f32> {
    let scene = textureSampleLevel(tex0, samp, in.uv, 0.0).rgb;
    let bloom = textureSampleLevel(tex1, samp, in.uv, 0.0).rgb;
    // Exposure first, so the tone curve sees the scene at the brightness we
    // mean it to have; then the curve, which is the only thing that decides
    // what "too bright" looks like.
    var c = tonemap((scene + bloom * post.p.x) * post.p.z);
    // Night color-grade (post.p.y = night factor, 0 by day .. 1 deep night):
    // slightly desaturate and cool the image toward blue so a moonlit scene
    // reads unmistakably cold even where surface albedo (green grass) can't take
    // a blue tint from light alone. Weighted toward the darker pixels, so warm
    // firelight/torches keep their hue and stay cozy.
    let night = post.p.y;
    if (night > 0.001) {
        let lum = dot(c, vec3<f32>(0.299, 0.587, 0.114));
        let cold = mix(vec3<f32>(lum), c, 0.75) * vec3<f32>(0.82, 0.94, 1.18);
        let dark = 1.0 - smoothstep(0.15, 0.75, lum);
        c = mix(c, cold, night * dark * 0.7);
    }
    return vec4<f32>(c, 1.0);
}
