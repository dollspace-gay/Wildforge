struct Surface {
    uv: vec2<f32>,
    // World-space normal, tilted by the height gradient so relief catches light.
    normal: vec3<f32>,
    // uv for the deeper interior stratum (parallaxed further than the surface,
    // wrapped within the tile so the periodic crack layer scrolls seamlessly).
    interior_uv: vec2<f32>,
    // 0 = no authored interior layer; else its atlas slot, already resolved.
    layer_slot: u32,
    // 1-based index into u.layer for this tile's settings; 0 = none.
    layer_id: u32,
};

// Per-layer settings, or defaults when the pack declared none.
fn layer_depth(id: u32) -> f32 {
    if (id == 0u || id > 32u) { return LAYER_DEPTH_DEFAULT; }
    return u.layer[(id - 1u) * 2u].x;
}

// Keep a uv inside one atlas cell by wrapping within it. A seamless tile is
// periodic, so the sample past an edge is the sample at the opposite edge —
// which is exactly what `fract` gives, with no clamp to smear against.
fn wrap_tile(p: vec2<f32>, tile_min: vec2<f32>, ts: f32) -> vec2<f32> {
    return tile_min + fract((p - tile_min) / ts) * ts;
}

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
    out.layer_slot = 0u;
    out.layer_id = 0u;

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
    if (h0 > 0.995 && g0 < 0.01 && nrm_amt < 0.004 && mat0.a < 0.002) {
        return out;
    }
    // Is the uv basis usable? The test has to be scale-free. `det` has units of
    // (uv per pixel) squared, so it shrinks with the square of how much screen a
    // tile covers — walk up to a wall and a perfectly healthy basis reaches 1e-10,
    // which an absolute threshold reads as degenerate. That flattened the relief
    // on everything nearer than ~0.4 blocks at 720p (and ~1.0 at 1440p, since the
    // cutoff scales with resolution), with the boundary tracing a constant-depth
    // line across the surface: a diagonal seam that slid with the camera.
    // What actually matters is whether the two derivative vectors are
    // near-PARALLEL (a zero-area mapping), and 1e-6 of their magnitudes is also
    // about where f32 cancellation leaves `det` meaningless anyway.
    let det = dux.x * duy.y - duy.x * dux.y;
    if (abs(det) <= 1e-6 * length(dux) * length(duy)) {
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
    // Per-block uv shift direction; clamp grazing z so it can't blow up. The
    // march has to stay inside this tile's atlas cell so it never bleeds into a
    // neighbour — but it WRAPS within the cell rather than clamping to its edge.
    //
    // Clamping was a visible artifact, not a safety net. At grazing incidence vz
    // pins to 0.25, so the ray sweeps 0.32 of a tile; every fragment starting
    // within that of the edge froze against the clamp and smeared its last texel
    // along the view direction. On a flat expanse seen at a shallow angle that is
    // most of the screen, and it read as seams at the block boundaries.
    // Wrapping is also what the tile actually is: pack tiles are seamless, so the
    // texel past the edge IS the texel at the far side. The interior stratum below
    // already worked this way; the surface just never got the same treatment.
    let vz = max(abs(vt.z), 0.25);
    let dir = vt.xy / vz * ts;
    let tile_min = floor(uv / ts) * ts;

    // An authored interior layer names its slot in the material alpha. The
    // surface above it is a flat sheet by design — parallaxing both would fight,
    // and the depth cue we want is the LOWER layer sliding under a still one.
    let layer_id = u32(round(mat0.a * 255.0));
    let has_layer = layer_id > 0u;
    if (has_layer) {
        out.layer_slot = u32(u.occ_origin.w) + layer_id - 1u;
        out.layer_id = layer_id;
    }

    var cur_uv = uv;
    // Surface relief: only when the surface height itself has structure (R < 1),
    // and never under an interior layer (the sheet stays flat).
    if (h0 < 0.995 && !has_layer) {
        let p = dir * PARALLAX_DEPTH;
        let layer = 1.0 / f32(PARALLAX_STEPS);
        let duv = p * layer;
        var ray_depth = 0.0;
        var surf_depth = 1.0 - h0;
        for (var i = 0; i < PARALLAX_STEPS; i = i + 1) {
            if (ray_depth >= surf_depth) {
                break;
            }
            cur_uv = wrap_tile(cur_uv - duv, tile_min, ts);
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
        let enc = textureSampleLevel(normal_tex, atlas_smp, wrap_tile(cur_uv, tile_min, ts), 0.0).xyz;
        let dec = enc * 2.0 - 1.0;
        // Green is negated: OpenGL maps measure y up the image, while our
        // bitangent runs down it (tile row 0 is v = 0). That one sign is the
        // whole difference between the OpenGL and DirectX conventions.
        // Otherwise the map is taken as authored — a malformed one should look
        // wrong, not be silently repaired here every frame; sanitizing belongs
        // at import. The only guard is against a degenerate (zero-length)
        // texel, which would normalize to NaN.
        let v = vec3<f32>(dec.x, -dec.y, dec.z);
        let len2 = dot(v, v);
        let un = select(vec3<f32>(0.0, 0.0, 1.0), v * inverseSqrt(max(len2, 1e-12)), len2 > 1e-6);
        n_ts = normalize(vec3<f32>(un.xy * nrm_amt, un.z));
    } else if (h0 < 0.995) {
        // Height-gradient normal at the displaced point (central differences).
        let texel = 1.0 / vec2<f32>(textureDimensions(material_tex, 0));
        let hl = textureSampleLevel(material_tex, atlas_smp, wrap_tile(cur_uv - vec2<f32>(texel.x, 0.0), tile_min, ts), 0.0).r;
        let hr = textureSampleLevel(material_tex, atlas_smp, wrap_tile(cur_uv + vec2<f32>(texel.x, 0.0), tile_min, ts), 0.0).r;
        let hd = textureSampleLevel(material_tex, atlas_smp, wrap_tile(cur_uv - vec2<f32>(0.0, texel.y), tile_min, ts), 0.0).r;
        let hu = textureSampleLevel(material_tex, atlas_smp, wrap_tile(cur_uv + vec2<f32>(0.0, texel.y), tile_min, ts), 0.0).r;
        n_ts = normalize(vec3<f32>((hl - hr) * NORMAL_STRENGTH, (hd - hu) * NORMAL_STRENGTH, 1.0));
    }
    out.normal = normalize(t * n_ts.x + b * n_ts.y + n * n_ts.z);
    // The internal stratum sits INTERIOR_DEPTH deeper than the (possibly smooth)
    // surface, so it shifts further along the view ray. The crack pattern is
    // periodic, so we WRAP the sample within this tile's cell (nearest-filtered,
    // no bleed) instead of clamping: the pattern scrolls seamlessly and reads as
    // one continuous layer at depth across every block — no edge clamp, no pop.
    var layer_min = tile_min;
    if (has_layer) {
        layer_min = vec2<f32>(
            f32(out.layer_slot % u32(ATLAS_TILES)),
            f32(out.layer_slot / u32(ATLAS_TILES)),
        ) * ts;
    }
    if (has_layer) {
        // PROBE: parameterise the stratum by WORLD position on the face plane,
        // not by per-face uv. Per-face uv re-anchors the layer in every block,
        // so it is N independently-wrapped patches rather than one sheet; a
        // thing genuinely below the surface should be continuous across blocks
        // by construction. Axis-aligned faces make the projection exact.
        // Must reproduce the mesher's per-face uv convention (mesher.rs): side
        // faces flip v, top/bottom do not. `fract(-y) == 1 - fract(y)`, so
        // negating y is exactly that flip in a world-continuous form.
        let an = abs(normalize(geo_n));
        var w2: vec2<f32>;
        if (an.y >= an.x && an.y >= an.z) {
            w2 = vec2<f32>(world.x, world.z);          // +-Y: (x, z), no flip
        } else if (an.x >= an.z) {
            w2 = vec2<f32>(world.z, -world.y);         // +-X: (z, 1-y)
        } else {
            w2 = vec2<f32>(world.x, -world.y);         // +-Z: (x, 1-y)
        }
        // `dir` is in atlas-uv units; /ts puts the shift back into blocks, which
        // is what w2 is measured in.
        let shift = vt.xy / vz * layer_depth(layer_id);
        // Anchor to the block, not the origin: fract() on a raw world coordinate
        // loses its fractional bits far from spawn, and this layer is sampled at
        // 1/32 of a block. Subtracting the block corner keeps the operand small.
        out.interior_uv = layer_min + fract(w2 - floor(w2) - shift) * ts;
    } else {
        out.interior_uv = wrap_tile(cur_uv - dir * INTERIOR_DEPTH - tile_min + layer_min, layer_min, ts);
    }
    return out;
}

