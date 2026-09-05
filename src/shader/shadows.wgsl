// Depth-only pass from the sun's point of view: positions into light-space
// Sun visibility at a world point. Returns (shadow, coverage): shadow is the
// lit fraction 1..0 (3x3 PCF, half-texel spread, slope-scaled bias); coverage
// is 1 if the point fell inside a cascade (the shadow map is authoritative) or
// 0 if it's beyond the farthest cascade (caller falls back to the sky mask).
// Cascades are tested tightest-first, so near geometry uses the densest map.
fn sample_shadow(world: vec3<f32>, ndl: f32) -> vec2<f32> {
    let bias = clamp(0.0016 * tan(acos(clamp(ndl, 0.0, 1.0))), 0.0004, 0.004);
    let texel = 1.0 / SHADOW_RES;
    for (var c = 0u; c < SHADOW_CASCADES; c = c + 1u) {
        let lc = u.light_vp[c] * vec4<f32>(world, 1.0);
        let p = lc.xyz / lc.w;
        let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || p.z > 1.0 || p.z < 0.0) {
            continue; // not in this cascade; try the next (wider) one
        }
        // Wider cascades spread each texel over more world, so the
        // same slope needs proportionally more bias — the fixed cap
        // was the triangle acne on steep faces (ngutten's #24).
        let cscale = f32(1u << (2u * c)) * 0.5 + 0.5; // 1, 2.5, 8.5
        let ref_depth = p.z - bias * cscale;
        var sum = 0.0;
        for (var dy = -1; dy <= 1; dy = dy + 1) {
            for (var dx = -1; dx <= 1; dx = dx + 1) {
                let off = vec2<f32>(f32(dx), f32(dy)) * texel * 0.5;
                sum = sum + textureSampleCompareLevel(shadow_tex, shadow_smp, uv + off, i32(c), ref_depth);
            }
        }
        return vec2<f32>(sum / 9.0, 1.0);
    }
    return vec2<f32>(1.0, 0.0); // beyond all cascades
}

// Minecraft-style face brightness from a normal: top 1.0, bottom 0.5,
// Z-sides 0.8, X-sides 0.6. Gives torch-/ambient-lit faces their form
// without any real light direction.
fn face_shade(n: vec3<f32>, world: vec3<f32>) -> f32 {
    let vertical = dot(normalize(n), normalize(world));
    if (vertical > 0.5) { return 1.0; }
    if (vertical < -0.5) { return 0.5; }
    // A common side value avoids a cube-chart lighting seam. Directional
    // sunlight still gives each wall its actual form.
    return 0.72;
}

// Sky ambient irradiance in the direction `n` (a diffuse light multiplier),
// reconstructed from the 9 SH coefficients. The basis order/constants match the
// CPU projection in sky.rs. Clamped non-negative (SH can ring below zero).
fn sh_irradiance(n: vec3<f32>) -> vec3<f32> {
    var c = u.sh[0].rgb * 0.282095;
    c += u.sh[1].rgb * (0.488603 * n.y);
    c += u.sh[2].rgb * (0.488603 * n.z);
    c += u.sh[3].rgb * (0.488603 * n.x);
    c += u.sh[4].rgb * (1.092548 * n.x * n.y);
    c += u.sh[5].rgb * (1.092548 * n.y * n.z);
    c += u.sh[6].rgb * (0.315392 * (3.0 * n.z * n.z - 1.0));
    c += u.sh[7].rgb * (1.092548 * n.x * n.z);
    c += u.sh[8].rgb * (0.546274 * (n.x * n.x - n.y * n.y));
    return max(c, vec3<f32>(0.0));
}

// The room's bounced light arriving on a surface facing `n`. Only four
// coefficients, because bounced light is smooth enough that the L1 lobe is all
// there is to say about it: which way the colour is coming from, and how much.
fn room_irradiance(n: vec3<f32>) -> vec3<f32> {
    var c = u.room_sh[0].rgb * 0.282095;
    c += u.room_sh[1].rgb * (0.488603 * n.y);
    c += u.room_sh[2].rgb * (0.488603 * n.z);
    c += u.room_sh[3].rgb * (0.488603 * n.x);
    return max(c, vec3<f32>(0.0));
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

// Which cube face a direction samples (+X 0, -X 1, +Y 2, -Y 3, +Z 4, -Z 5).
// Debug only: lets the shadow viz show where cube-face seams fall.
fn cube_face(dir: vec3<f32>) -> i32 {
    let a = abs(dir);
    if (a.x >= a.y && a.x >= a.z) { return select(1, 0, dir.x > 0.0); }
    if (a.y >= a.z) { return select(3, 2, dir.y > 0.0); }
    return select(5, 4, dir.z > 0.0);
}

// Six distinct colors for the cube faces (debug viz mode 3).
fn face_color(f: i32) -> vec3<f32> {
    switch (f) {
        case 0: { return vec3<f32>(1.0, 0.2, 0.2); } // +X red
        case 1: { return vec3<f32>(0.4, 0.0, 0.0); } // -X dark red
        case 2: { return vec3<f32>(0.2, 1.0, 0.2); } // +Y green
        case 3: { return vec3<f32>(0.0, 0.4, 0.0); } // -Y dark green
        case 4: { return vec3<f32>(0.3, 0.4, 1.0); } // +Z blue
        default: { return vec3<f32>(0.0, 0.1, 0.4); } // -Z dark blue
    }
}

// Occupancy sample for a world cell: RGBA8 where a = class (255 opaque full
// block, 128 glass, 0 air) and rgb = the glass's per-channel light filter
// (stained-glass tint). Cells outside the grid read as open air.
fn occ_at(cell: vec3<i32>) -> vec4<u32> {
    let local = cell - u.occ_origin.xyz;
    if (any(local < vec3<i32>(0)) || any(local >= vec3<i32>(OCC_GRID))) {
        return vec4<u32>(0u);
    }
    return textureLoad(occ_tex, local, 0);
}

// Per-channel transmittance from a receiver surface to a point `target`, marched
// cell-by-cell through the voxel grid (Amanatides-Woo). Opaque cells drop it to
// zero; glass cells multiply in their filter tint (real per-pane Beer-Lambert),
// so a shadow cast through red glass comes out red. No depth compare, no bias:
// the march starts a hair off the surface along its geometric normal, so the
// fragment's own cell is never entered and a lit face can't self-shadow. The
// target's own cell is excluded too. vec3(1) = fully lit, vec3(0) = blocked.
fn dda_transmit(world: vec3<f32>, normal: vec3<f32>, tgt: vec3<f32>) -> vec3<f32> {
    let start = world + normal * 0.03;
    let to_t = tgt - start;
    let dist = length(to_t);
    if (dist < 1e-3) { return vec3<f32>(1.0); }
    let dir = to_t / dist;
    let stepf = sign(dir);
    let stepi = vec3<i32>(stepf);
    let ad = abs(dir);
    let tDelta = 1.0 / max(ad, vec3<f32>(1e-8));
    // Ray distance to the first cell boundary on each axis (upper if stepping
    // positive, lower if negative). Axes with ~zero dir never cross: push far.
    let cell0 = floor(start);
    let next = cell0 + max(stepf, vec3<f32>(0.0));
    var tMax = (next - start) / dir;
    tMax = select(tMax, vec3<f32>(1e30), ad < vec3<f32>(1e-8));
    var cell = vec3<i32>(cell0);
    let tgt_cell = vec3<i32>(floor(tgt));
    var trans = vec3<f32>(1.0);
    for (var s = 0; s < 96; s = s + 1) {
        let tEnter = min(tMax.x, min(tMax.y, tMax.z));
        if (tEnter >= dist) { return trans; } // next boundary is past the target
        if (tMax.x <= tMax.y && tMax.x <= tMax.z) {
            cell.x += stepi.x; tMax.x += tDelta.x;
        } else if (tMax.y <= tMax.z) {
            cell.y += stepi.y; tMax.y += tDelta.y;
        } else {
            cell.z += stepi.z; tMax.z += tDelta.z;
        }
        if (all(cell == tgt_cell)) { return trans; } // reached the target's cell
        let s4 = occ_at(cell);
        if (s4.a == 255u) { return vec3<f32>(0.0); } // opaque
        if (s4.a == 128u) {                          // glass: multiply its tint
            trans = trans * (vec3<f32>(s4.rgb) / 255.0);
            if (trans.r + trans.g + trans.b < 0.01) { return vec3<f32>(0.0); }
        }
    }
    return trans;
}

// Soft point-light transmittance: treats the light as a disk of the given world
// radius facing the receiver and averages grid-march rays to points spread
// across it, so shadow edges get a physically-widening penumbra (broad far from
// the caster, tight at contact) — and tinted panes soften the same way. Fully
// lit and fully shadowed regions early out to a single ray; only the penumbra
// pays for the full fan. radius 0 is a hard point light — one exact ray.
fn point_shadow_dda(world: vec3<f32>, normal: vec3<f32>, lp: vec3<f32>, radius: f32) -> vec3<f32> {
    let center = dda_transmit(world, normal, lp);
    if (radius < 0.01) { return center; }
    // Disk basis perpendicular to the light direction. It rotates smoothly with
    // the light direction across neighbouring fragments, so the sample pattern
    // stays coherent — no per-fragment random rotation (that made the penumbra a
    // noisy Monte-Carlo estimate that crawled and flickered as the radius moved).
    let ld = normalize(lp - world);
    let up0 = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(ld.y) > 0.99);
    let tang = normalize(cross(up0, ld));
    let bitang = cross(ld, tang);
    // Early out: probe the four cardinal edges of the disk. If all agree with
    // the centre ray, this fragment is solidly in umbra or full light — skip the
    // fan. Four directions (not one axis) so a penumbra crossing either axis is
    // never missed and hardened.
    let et = tang * radius;
    let eb = bitang * radius;
    if (all(dda_transmit(world, normal, lp + et) == center)
        && all(dda_transmit(world, normal, lp - et) == center)
        && all(dda_transmit(world, normal, lp + eb) == center)
        && all(dda_transmit(world, normal, lp - eb) == center)) {
        return center;
    }
    // Penumbra: average a fixed Vogel-spiral fan across the light disk. Fixed
    // (unrotated) so the soft edge is a smooth, stable gradient; the source is
    // small (a flame), so the band is thin and never wraps around a block.
    let N = 16;
    var sum = vec3<f32>(0.0);
    for (var s = 0; s < N; s = s + 1) {
        let rr = sqrt((f32(s) + 0.5) / f32(N));
        let th = f32(s) * 2.399963;
        let off = (tang * cos(th) + bitang * sin(th)) * (rr * radius);
        sum = sum + dda_transmit(world, normal, lp + off);
    }
    return sum / f32(N);
}

