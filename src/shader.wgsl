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
    // rgb = cool sky-ambient fill, already scaled by daylight;
    // a = the ambient floor (the stark<->soft darkness knob)
    amb_col: vec4<f32>,
    // world -> sun light-space clip, for shadow-map lookup
    light_vp: mat4x4<f32>,
    // x = active point-light count
    pt_count: vec4<u32>,
    // per light: xyz = world position, w = range
    pt_pos: array<vec4<f32>, 8>,
    // per light: rgb = color × intensity, w unused
    pt_col: array<vec4<f32>, 8>,
    // per light: x = flood-suppression scale, y = its range,
    // z = shadows enabled, w unused
    pt_misc: array<vec4<f32>, 8>,
};

const MAX_PT_LIGHTS: u32 = 8u;

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var atlas_tex: texture_2d<f32>;
@group(1) @binding(1) var atlas_smp: sampler;
// Material atlas (linear): R = parallax height (1 = surface, 0 = deepest),
// G = interior mask, B = authored-normal strength (0 = none).
// A flat tile (R = 1, G = 0, B = 0) is a no-op, so all of it is opt-in per texture.
@group(1) @binding(2) var material_tex: texture_2d<f32>;
// Normal atlas (linear): tangent-space normals in the standard OpenGL / +Y
// encoding, so a stock or model-generated map drops in unmodified. Flat (128,
// 128, 255) wherever nothing is authored; material.b says where that is, so the
// plain-tile early-out never has to read this texture.
@group(1) @binding(3) var normal_tex: texture_2d<f32>;
@group(2) @binding(0) var shadow_tex: texture_depth_2d;
@group(2) @binding(1) var shadow_smp: sampler_comparison;
@group(2) @binding(2) var pt_cube: texture_cube_array<f32>;
@group(2) @binding(3) var pt_smp: sampler;
@group(2) @binding(4) var pt_tr_cube: texture_cube_array<f32>;

const SHADOW_RES: f32 = 2048.0;
const ATLAS_TILES: f32 = 32.0;
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
// The interior is the block's own lit colour, modulated by its internal
// structure (G): dimmer/clearer in the gaps, brighter/frosted where dense.
const INTERIOR_LO: f32 = 0.35;
const INTERIOR_HI: f32 = 1.9;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) light: vec3<f32>,
    @location(4) sky: f32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: vec3<f32>,
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

// Per-fragment pseudo-random value in [0,1) from a world position (stable
// under camera motion, so the dithered penumbra doesn't crawl).
fn hash12(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// Fraction of point light `i` reaching `world` (1 lit, 0 occluded), from its
// distance cube map. pt_misc[i].w is the source radius: 0 is a hard point;
// larger softens the penumbra by PCF-sampling a Vogel disk (scaled by radius,
// dither-rotated per fragment) — an approximate area source.
fn point_shadow(i: u32, world: vec3<f32>, to_light: vec3<f32>, d: f32, range: f32) -> f32 {
    let bias = 0.08 + 0.15 * d / range;
    let dir = -to_light;
    let radius = u.pt_misc[i].w;
    if (radius < 0.001) {
        let nearest = textureSampleLevel(pt_cube, pt_smp, dir, i32(i), 0.0).r;
        return select(0.0, 1.0, d <= nearest + bias);
    }
    let nd = normalize(dir);
    let up0 = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(nd.y) > 0.99);
    let tang = normalize(cross(up0, nd));
    let bitang = cross(nd, tang);
    let rot = hash12(world.xz + world.yx) * 6.2831853;
    let N = 16u;
    var occ = 0.0;
    for (var s = 0u; s < N; s = s + 1u) {
        let rr = sqrt((f32(s) + 0.5) / f32(N));
        let th = f32(s) * 2.399963 + rot;
        let off = rr * vec2<f32>(cos(th), sin(th)) * radius;
        let sdir = dir + tang * off.x + bitang * off.y;
        let nearest = textureSampleLevel(pt_cube, pt_smp, sdir, i32(i), 0.0).r;
        occ = occ + select(0.0, 1.0, d <= nearest + bias);
    }
    return occ / f32(N);
}

// Full lit multiplier (per channel) for a world-space surface. A near-zero
// normal marks pre-shaded billboards/entities, which keep the old flat model.
// `normal` is the flat geometric face normal (drives the stylized per-face
// shade and ambient); `detail_n` is the relief-perturbed normal (drives the
// directional sun and point-light N·L, so grooves self-shade). They're equal
// for flat surfaces and non-relief geometry.
fn world_light(normal: vec3<f32>, detail_n: vec3<f32>, light: vec3<f32>, sky: f32, world: vec3<f32>) -> vec3<f32> {
    if (dot(normal, normal) < 0.25) {
        // Pre-shaded billboards/entities: colored block light or grayscale sky,
        // whichever is brighter per channel, over a small floor.
        return max(max(light, vec3<f32>(sky * u.misc.y)), vec3<f32>(u.amb_col.a));
    }
    let n = normalize(normal);
    let dn = normalize(detail_n);
    let fs = face_shade(n);
    // Warm sun: direct, gated by sky visibility, surface orientation, and the
    // shadow map (cast shadows). Ambient/torch are unaffected, so shadowed
    // ground fills with cool sky light instead of going black.
    let ndl = max(dot(dn, u.sun_dir.xyz), 0.0);
    let shadow = sample_shadow(world, ndl);
    let sun = sky * ndl * shadow * u.sun_col.rgb;
    // Cool sky fill.
    let amb = sky * fs * u.amb_col.rgb;
    // Hard-edged colored point lights: range-attenuated N·L, summed, gated
    // by the distance cube maps. Each promoted light also cancels its own
    // soft flood-fill wrap (suppression) so the hard shadow reads — the
    // sim's flood values are untouched; this is render-side only.
    var direct = vec3<f32>(0.0);
    var suppress = vec3<f32>(0.0);
    let count = min(u.pt_count.x, MAX_PT_LIGHTS);
    for (var i = 0u; i < count; i = i + 1u) {
        let lp = u.pt_pos[i].xyz;
        let range = u.pt_pos[i].w;
        let to_light = lp - world;
        let d = length(to_light);
        let sup = u.pt_misc[i].x * max(1.0 - d / max(u.pt_misc[i].y, 0.001), 0.0);
        suppress = suppress + u.pt_col[i].rgb * sup;
        if (d < range) {
            let ldir = to_light / max(d, 1e-3);
            let ndl2 = max(dot(dn, ldir), 0.0);
            let a = clamp(1.0 - d / range, 0.0, 1.0);
            let atten = a * a;
            var shadow_pt = 1.0;
            var tint = vec3<f32>(1.0);
            if (u.pt_misc[i].z > 0.5) {
                // Cube distance map, hard or soft (per-light radius).
                shadow_pt = point_shadow(i, world, to_light, d, range);
                // Stained transmission: panes between light and fragment
                // multiply in their color (a small margin keeps a pane
                // from tinting its own surface).
                let tr = textureSampleLevel(pt_tr_cube, pt_smp, -to_light, i32(i), 0.0);
                if (tr.a <= d - 0.1) {
                    tint = tr.rgb;
                }
            }
            direct = direct + u.pt_col[i].rgb * (atten * ndl2 * shadow_pt * tint);
        }
    }
    // Steady (colored) torch light, minus each promoted light's estimate.
    let torch = max(light - suppress, vec3<f32>(0.0)) * fs;
    return max(sun + amb + torch + direct, vec3<f32>(u.amb_col.a));
}

fn apply_fog(color: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let dist = distance(world.xz, u.cam.xz);
    let fog = smoothstep(u.cam.w * 0.72, u.cam.w * 0.98, dist);
    return mix(color, u.sky.rgb, fog);
}

struct Surface {
    uv: vec2<f32>,
    // World-space normal, tilted by the height gradient so relief catches light.
    normal: vec3<f32>,
    // uv for the deeper interior stratum (parallaxed further than the surface,
    // wrapped within the tile so the periodic crack layer scrolls seamlessly).
    interior_uv: vec2<f32>,
};

// Parallax occlusion mapping + a height-derived surface normal. Steps the
// tangent-space view ray through the material atlas's height channel to find
// the displaced uv, then reads the local height gradient there to perturb the
// normal — so recessed detail (ice cracks) both shifts with the eye AND catches
// directional light on its walls. The tangent frame comes from world/uv screen
// derivatives (no per-vertex tangents). Flat tiles early-out for near-zero cost.
fn parallax_surface(uv: vec2<f32>, world: vec3<f32>, geo_n: vec3<f32>) -> Surface {
    var out: Surface;
    out.uv = uv;
    out.normal = geo_n;
    out.interior_uv = uv;

    // Derivatives must be evaluated in uniform control flow (before any branch).
    let dpx = dpdx(world);
    let dpy = dpdy(world);
    let dux = dpdx(uv);
    let duy = dpdy(uv);

    let mat0 = textureSampleLevel(material_tex, atlas_smp, uv, 0.0);
    let h0 = mat0.r;
    let g0 = mat0.g;
    // Authored-normal strength. Constant across a tile in practice (the atlas
    // flags whole slots), so reading it at the undisplaced uv is safe and keeps
    // the plain-tile test to this one texture fetch.
    let nrm_amt = mat0.b;
    // Truly flat: smooth surface (R~1), no interior layer (G~0), no authored
    // normal (B~0) — nothing to do.
    if (h0 > 0.995 && g0 < 0.01 && nrm_amt < 0.004) {
        return out;
    }
    let det = dux.x * duy.y - duy.x * dux.y;
    if (abs(det) < 1e-9) {
        return out;
    }
    let r = 1.0 / det;
    let n = normalize(geo_n);
    let t = normalize((dpx * duy.y - dpy * dux.y) * r);
    let b = normalize((dpy * dux.x - dpx * duy.x) * r);
    // View direction (fragment -> eye) in tangent space.
    let v = normalize(u.cam.xyz - world);
    let vt = vec3<f32>(dot(v, t), dot(v, b), dot(v, n));
    let ts = 1.0 / ATLAS_TILES;
    // Per-block uv shift direction; clamp grazing z so it can't blow up. Keep the
    // march inside this tile's atlas cell so it never bleeds into a neighbour.
    let vz = max(abs(vt.z), 0.25);
    let dir = vt.xy / vz * ts;
    let tmin = floor(uv / ts) * ts + ts * 0.02;
    let tmax = tmin + ts - ts * 0.04;

    var cur_uv = uv;
    // Surface relief: only when the surface height itself has structure (R < 1).
    if (h0 < 0.995) {
        let p = dir * PARALLAX_DEPTH;
        let layer = 1.0 / f32(PARALLAX_STEPS);
        let duv = p * layer;
        var ray_depth = 0.0;
        var surf_depth = 1.0 - h0;
        for (var i = 0; i < PARALLAX_STEPS; i = i + 1) {
            if (ray_depth >= surf_depth) {
                break;
            }
            cur_uv = clamp(cur_uv - duv, tmin, tmax);
            surf_depth = 1.0 - textureSampleLevel(material_tex, atlas_smp, cur_uv, 0.0).r;
            ray_depth = ray_depth + layer;
        }
        out.uv = cur_uv;
    }

    // The detail normal, in tangent space, read at the parallax-displaced point.
    // An authored map wins over the height gradient: it carries detail the height
    // field never had (a chisel bevel inside one flat-toned face) and it is what
    // the texture's author actually meant. Height-derived stays the free default.
    var n_ts = vec3<f32>(0.0, 0.0, 1.0);
    if (nrm_amt > 0.004) {
        let enc = textureSampleLevel(normal_tex, atlas_smp, clamp(cur_uv, tmin, tmax), 0.0).xyz;
        let dec = enc * 2.0 - 1.0;
        // Green is negated: OpenGL maps measure y up the image, while our
        // bitangent runs down it (tile row 0 is v = 0). That one sign is the
        // whole difference between the OpenGL and DirectX conventions.
        // z is held off zero so a hallucinated back-facing texel can't invert
        // the face, and xy is scaled by the authored strength.
        let un = normalize(vec3<f32>(dec.x, -dec.y, max(dec.z, 0.05)));
        n_ts = normalize(vec3<f32>(un.xy * nrm_amt, un.z));
    } else if (h0 < 0.995) {
        // Height-gradient normal at the displaced point (central differences).
        let texel = 1.0 / vec2<f32>(textureDimensions(material_tex, 0));
        let hl = textureSampleLevel(material_tex, atlas_smp, clamp(cur_uv - vec2<f32>(texel.x, 0.0), tmin, tmax), 0.0).r;
        let hr = textureSampleLevel(material_tex, atlas_smp, clamp(cur_uv + vec2<f32>(texel.x, 0.0), tmin, tmax), 0.0).r;
        let hd = textureSampleLevel(material_tex, atlas_smp, clamp(cur_uv - vec2<f32>(0.0, texel.y), tmin, tmax), 0.0).r;
        let hu = textureSampleLevel(material_tex, atlas_smp, clamp(cur_uv + vec2<f32>(0.0, texel.y), tmin, tmax), 0.0).r;
        n_ts = normalize(vec3<f32>((hl - hr) * NORMAL_STRENGTH, (hd - hu) * NORMAL_STRENGTH, 1.0));
    }
    out.normal = normalize(t * n_ts.x + b * n_ts.y + n * n_ts.z);
    // The internal stratum sits INTERIOR_DEPTH deeper than the (possibly smooth)
    // surface, so it shifts further along the view ray. The crack pattern is
    // periodic, so we WRAP the sample within this tile's cell (nearest-filtered,
    // no bleed) instead of clamping: the pattern scrolls seamlessly and reads as
    // one continuous layer at depth across every block — no edge clamp, no pop.
    let tile_origin = floor(uv / ts) * ts;
    out.interior_uv = tile_origin + fract((cur_uv - dir * INTERIOR_DEPTH - tile_origin) / ts) * ts;
    return out;
}

@fragment
fn fs_chunk(in: VsOut) -> @location(0) vec4<f32> {
    let s = parallax_surface(in.uv, in.world, in.normal);
    let tex = textureSample(atlas_tex, atlas_smp, s.uv);
    if (tex.a < 0.5) {
        discard; // alpha-tested item sprites share this pipeline
    }
    let surface_lit = tex.rgb * world_light(in.normal, s.normal, in.light, in.sky, in.world);
    var rgb = surface_lit;
    // Multilayer as a real translucent composite: the surface is a partial veil,
    // and the interior — present everywhere the material declares it (G floored
    // > 0), the block's own colour modulated by its structure — sits deeper and
    // parallaxes beneath. So you see THROUGH the surface to the structure sliding
    // under it, not a stencil painted on top. Untouched where there's no interior.
    let structure = textureSampleLevel(material_tex, atlas_smp, s.interior_uv, 0.0).g;
    if (structure > 0.02) {
        let interior = surface_lit * mix(INTERIOR_LO, INTERIOR_HI, structure);
        rgb = mix(interior, surface_lit, SURFACE_VEIL);
    }
    rgb = apply_fog(rgb, in.world);
    if (u.misc.x > 0.5) {
        rgb = mix(rgb, vec3<f32>(0.1, 0.2, 0.5), 0.55);
    }
    return vec4<f32>(rgb, 1.0);
}

@fragment
fn fs_water(in: VsOut) -> @location(0) vec4<f32> {
    let tex = textureSample(atlas_tex, atlas_smp, in.uv);
    var rgb = tex.rgb * world_light(in.normal, in.normal, in.light, in.sky, in.world);
    // Sun specular glint: a sharp Blinn-Phong highlight where the sun reflects
    // into the eye, gated by sky visibility and cast shadows.
    if (dot(in.normal, in.normal) > 0.25) {
        let n = normalize(in.normal);
        let v = normalize(u.cam.xyz - in.world);
        let h = normalize(u.sun_dir.xyz + v);
        let spec = pow(max(dot(n, h), 0.0), 64.0);
        let sun_lit = in.sky * sample_shadow(in.world, max(dot(n, u.sun_dir.xyz), 0.0));
        rgb = rgb + spec * sun_lit * u.sun_col.rgb * 1.6;
    }
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
