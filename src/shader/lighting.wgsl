// Full lit multiplier (per channel) for a world-space surface. A near-zero
// normal marks pre-shaded billboards/entities, which keep the old flat model.
// `normal` is the flat geometric face normal (drives the stylized per-face
// shade and ambient); `detail_n` is the relief-perturbed normal (drives the
// directional sun and point-light N·L, so grooves self-shade). They're equal
// for flat surfaces and non-relief geometry.
fn world_light(normal: vec3<f32>, detail_n: vec3<f32>, light: vec3<f32>, sky: f32, ao: f32, world: vec3<f32>) -> vec3<f32> {
    if (dot(normal, normal) < 0.25) {
        // Pre-shaded billboards/entities: colored block light or grayscale sky,
        // whichever is brighter per channel, over a small floor.
        return max(max(light, vec3<f32>(sky * u.misc.y)), vec3<f32>(u.amb_col.a));
    }
    let n = normalize(normal);
    let dn = normalize(detail_n);
    let fs = face_shade(n, world);
    // Warm sun: direct, gated by sky visibility, surface orientation, and the
    // shadow map (cast shadows). Ambient/torch are unaffected, so shadowed
    // ground fills with cool sky light instead of going black.
    let ndl = max(dot(dn, u.sun_dir.xyz), 0.0);
    let sh = sample_shadow(world, ndl);
    // Where a cascade covers this fragment (sh.y = 1) the shadow map is
    // authoritative, so the sun is gated by the map alone — a sunbeam through a
    // window lights an interior floor at full strength. Beyond the cascades
    // (sh.y = 0) fall back to the baked skylight mask so distant unshadowed
    // caves don't catch false sun.
    let sun_gate = mix(sky, 1.0, sh.y);
    let sun = sun_gate * ndl * sh.x * u.sun_col.rgb;
    // Sky fill: the actual sky color from the direction this (relief-perturbed)
    // surface faces. So a face pointing at the sunset warms, one at the zenith
    // cools, and relief bumps pick up different sky directions. Gated by a
    // concave power of the voxel skylight: the scalar mask over-reports how much
    // sky a partially-enclosed surface sees (it can't tell a wide-open hemisphere
    // from a sliver through a door), so squaring-and-then-some makes interiors
    // fall dark while open sky (mask ~1) stays full.
    let amb = pow(sky, 2.5) * sh_irradiance(dn);
    // The room's own light, which is what a sunbeam landing on something
    // coloured actually does to the space around it. Faded in as the skylight
    // mask closes: out under open sky the SH ambient above already carries
    // this, and doubling it there would only wash the daylight out.
    let bounce = room_irradiance(dn) * (1.0 - pow(sky, 2.5));
    // Hard-edged colored point lights: range-attenuated N·L, summed, gated by
    // the shadow term (voxel-grid DDA or the distance cube map). Each promoted
    // light also cancels its own baked flood-fill wrap (suppression) so its hard
    // shadow reads; the sim's flood values are untouched (render-side only).
    // Shadow-debug viz (WILDFORGE_SHADOW_DEBUG): 0 off, else capture the
    // most-occluded contributing torch's shadow decision for the override below.
    let dbg = u.pt_count.z;
    var dbg_hit = false;
    var dbg_shadow = 1.0;
    var dbg_margin = 1e9;
    var dbg_face = -1;
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
            var shadow_rgb = vec3<f32>(1.0);
            // A light sitting on the camera — your own held torch — skips its
            // shadow test. It can't cast a shadow onto any surface you can see:
            // anything occluded from a light at the eye is occluded from the eye
            // too. Its shadow map only ever produced self-shadow acne, a false
            // dark bar that grazed across the floor and slid with the player
            // (the held light hovers ~1.5 above the floor, so the floor is
            // edge-on to it and a distance-only cube-map compare self-occludes).
            // Remote players' torches are at their position, not yours, so they
            // still cast real shadows.
            if (u.pt_misc[i].z > 0.5 && distance(lp, u.cam.xyz) > 0.5) {
                if (u.pt_count.w != 0u) {
                    // Exact voxel-grid march — no cube map, no acne. Returns a
                    // per-channel transmittance, so glass panes tint the shadow.
                    shadow_rgb = point_shadow_dda(world, n, lp, u.pt_misc[i].w);
                } else {
                    // Cube distance map, hard or soft (per-light radius).
                    let sp = point_shadow(i, world, to_light, d, range);
                    // Stained transmission: panes between light and fragment
                    // multiply in their color (a small margin keeps a pane
                    // from tinting its own surface).
                    var tint = vec3<f32>(1.0);
                    let tr = textureSampleLevel(pt_tr_cube, pt_smp, -to_light, i32(i), 0.0);
                    if (tr.a <= d - 0.1) {
                        tint = tr.rgb;
                    }
                    shadow_rgb = vec3<f32>(sp) * tint;
                }
                // Debug: for a light that faces this fragment, remember the
                // strongest occlusion (mean channel) for the viz override.
                if (dbg != 0u && ndl2 > 0.001) {
                    let sc = dot(shadow_rgb, vec3<f32>(1.0)) * (1.0 / 3.0);
                    if (sc < dbg_shadow) {
                        let nearest = textureSampleLevel(pt_cube, pt_smp, -to_light, i32(i), 0.0).r;
                        dbg_hit = true;
                        dbg_shadow = sc;
                        dbg_margin = nearest - d; // cube-only; meaningless under DDA
                        dbg_face = cube_face(-to_light);
                    }
                }
            }
            direct = direct + u.pt_col[i].rgb * (atten * ndl2) * shadow_rgb;
        }
    }
    // Shadow-debug override: replace the shading with a diagnostic color.
    if (dbg != 0u) {
        if (!dbg_hit) { return vec3<f32>(0.02); } // no torch faces here
        if (dbg == 1u) {
            // Mode 1: red where the cube shadow occludes, dim grey where lit.
            return vec3<f32>(1.0 - dbg_shadow, dbg_shadow * 0.25, dbg_shadow * 0.25);
        }
        if (dbg == 2u) {
            // Mode 2: raw shadow factor, grayscale (1 lit .. 0 occluded).
            return vec3<f32>(dbg_shadow);
        }
        if (dbg == 3u) {
            // Mode 3: which cube face this fragment sampled.
            return face_color(dbg_face);
        }
        // Mode 4: depth-compare margin (nearest - d). Blue = fragment sits in
        // front of the stored occluder (lit, with slack). Green = razor edge
        // (|margin| tiny → a bias tweak would flip it). Red = fragment sits
        // BEHIND the stored occluder by a real distance → a gross/structural
        // mismatch no bias can close. Brightness scales with the gap.
        let m = dbg_margin;
        if (m < -0.03) { return vec3<f32>(clamp(-m, 0.05, 1.0), 0.0, 0.0); }
        if (m > 0.03) { return vec3<f32>(0.0, 0.0, clamp(m * 0.25, 0.05, 1.0)); }
        return vec3<f32>(0.0, 1.0, 0.0);
    }
    // Steady (colored) torch light from the baked voxel flood, minus each
    // promoted light's estimate.
    let torch = max(light - suppress, vec3<f32>(0.0)) * fs;
    // Corner openness against the flat floor, but not all the way to nothing:
    // a fully wedged corner keeps sun_col.a of it. Letting it reach true black
    // read as holes punched in the room rather than as shape.
    let occ = mix(u.sun_col.a, 1.0, ao);
    // The room's own light is occluded like any other ambient. Left flat it
    // lays an even wash over every surface and undoes the corner darkening
    // below it — the walls lose their shape exactly as the tint gets strong
    // enough to notice.
    // The flat fill follows how much light is actually about. It used to be a
    // constant, so a room kept the same floor of brightness whether a broad
    // noon beam or a thin evening sliver was coming through the window — only
    // its colour changed. Tying it to the measured amount means a narrowing
    // beam dims the room as well as tinting it, while the colour of the floor
    // it happens to fall on moves the hue and nothing else.
    let fill = u.amb_col.a * occ * mix(AMB_MIN, 1.0, min(u.room_sh[0].w / AMB_FULL, 1.0));
    return max(sun + amb + bounce * occ + torch + direct, vec3<f32>(fill));
}

