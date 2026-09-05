@fragment
fn fs_chunk(in: VsOut) -> @location(0) vec4<f32> {
    let s = parallax_surface(in.uv, in.world, in.normal);
    let tex = textureSample(atlas_tex, atlas_smp, s.uv);
    // A layered tile reinterprets its own alpha as coverage over the stratum
    // below, so the plain alpha test must not eat it; the cutoff below is that
    // tile's version of the same decision.
    if (s.layer_id == 0u && tex.a < 0.5) {
        discard; // alpha-tested item sprites share this pipeline
    }
    if (u.pt_count.z != 0u) {
        // Shadow-debug viz: bypass albedo/fog so the diagnostic color is pure.
        return vec4<f32>(world_light(in.normal, s.normal, in.light, in.sky, in.ao, in.world), 1.0);
    }
    let lit = world_light(in.normal, s.normal, in.light, in.sky, in.ao, in.world);
    let surface_lit = tex.rgb * lit;
    var rgb = surface_lit;
    if (s.layer_id != 0u) {
        // A real second albedo below the surface. How opaque the surface is over
        // it is the material's business, not the renderer's: mode 0 reads the
        // surface's authored ALPHA (foliage — real holes), mode 1 derives it from
        // luminance (ice — dark means thin). Only the lower layer parallaxes, so
        // it slides beneath a still sheet as the eye moves.
        let p0 = u.layer[(s.layer_id - 1u) * 2u];
        let p1 = u.layer[(s.layer_id - 1u) * 2u + 1u];
        let below = textureSampleLevel(atlas_tex, atlas_smp, s.interior_uv, 0.0);
        let src = select(tex.a, dot(tex.rgb, vec3<f32>(0.299, 0.587, 0.114)), p1.x > 0.5);
        let opacity = clamp(src, p0.y, p0.z);
        // Standard over-operator, and the coverage it leaves is what says whether
        // anything is here at all: gaps in BOTH strata are a hole you see through,
        // which is what gives a canopy its ragged edge against the sky.
        // The stratum is a portal into an infinite plane, not geometry, so a
        // hole in it has nothing behind it and shows sky. Whether that is wanted
        // is the material's business: author the layer opaque for a backstop
        // (foliage, where the canopy should read solid), or with real gaps and a
        // cutoff for a surface you can see past.
        let coverage = opacity + below.a * (1.0 - opacity);
        if (coverage < p1.y) {
            discard;
        }
        rgb = mix(below.rgb * lit * p0.w, surface_lit, opacity / max(coverage, 1e-4));
    } else {
        // Legacy single-channel interior (the procedural ice): a greyscale mask
        // that modulates the surface's own colour rather than a layer of its own.
        let structure = textureSampleLevel(material_tex, atlas_smp, s.interior_uv, 0.0).g;
        if (structure > 0.02) {
            let interior = surface_lit * mix(INTERIOR_LO, INTERIOR_HI, structure);
            rgb = mix(interior, surface_lit, SURFACE_VEIL);
        }
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
    var rgb = tex.rgb * world_light(in.normal, in.normal, in.light, in.sky, in.ao, in.world);
    // Sun specular glint: a sharp Blinn-Phong highlight where the sun reflects
    // into the eye, gated by sky visibility and cast shadows.
    if (dot(in.normal, in.normal) > 0.25) {
        let n = normalize(in.normal);
        let v = normalize(u.cam.xyz - in.world);
        let h = normalize(u.sun_dir.xyz + v);
        let spec = pow(max(dot(n, h), 0.0), 64.0);
        let sun_lit = in.sky * sample_shadow(in.world, max(dot(n, u.sun_dir.xyz), 0.0)).x;
        rgb = rgb + spec * sun_lit * u.sun_col.rgb * 1.6;
    }
    rgb = apply_fog(rgb, in.world);
    if (u.misc.x > 0.5) {
        rgb = mix(rgb, vec3<f32>(0.1, 0.2, 0.5), 0.55);
    }
    return vec4<f32>(rgb, tex.a);
}

