// ---- capture-only visible-fragment evidence ----
//
// Rgba16Uint: R = atlas-family id (slot + 1; 0 is sky), G = the same
// geodesic-plus-radial depth used by fog normalized to its effective range,
// B = fragment class (1 solid/cutout, 2 water, 3 overlay), A = schema 1.
// These entry points only have pipelines when WILDFORGE_VISUAL_EVIDENCE=1.

fn diagnostic_slot(uv: vec2<f32>) -> u32 {
    let tile = vec2<u32>(clamp(floor(uv * ATLAS_TILES), vec2<f32>(0.0), vec2<f32>(ATLAS_TILES - 1.0)));
    return tile.y * u32(ATLAS_TILES) + tile.x;
}

fn diagnostic_fragment(uv: vec2<f32>, world: vec3<f32>, fragment_class: u32) -> vec4<u32> {
    let depth = u32(round(clamp(fog_distance(world) / max(u.cam.w, 1e-3), 0.0, 1.0) * 65535.0));
    return vec4<u32>(diagnostic_slot(uv) + 1u, depth, fragment_class, 1u);
}

@fragment
fn fs_diagnostic_chunk(in: VsOut) -> @location(0) vec4<u32> {
    let s = parallax_surface(in.uv, in.world, in.normal);
    let tex = textureSample(atlas_tex, atlas_smp, s.uv);
    if (s.layer_id == 0u && tex.a < 0.5) {
        discard;
    }
    if (s.layer_id != 0u) {
        let p0 = u.layer[(s.layer_id - 1u) * 2u];
        let p1 = u.layer[(s.layer_id - 1u) * 2u + 1u];
        let below = textureSampleLevel(atlas_tex, atlas_smp, s.interior_uv, 0.0);
        let src = select(tex.a, dot(tex.rgb, vec3<f32>(0.299, 0.587, 0.114)), p1.x > 0.5);
        let opacity = clamp(src, p0.y, p0.z);
        let coverage = opacity + below.a * (1.0 - opacity);
        if (coverage < p1.y) {
            discard;
        }
    }
    return diagnostic_fragment(s.uv, in.world, 1u);
}

@fragment
fn fs_diagnostic_chunk_overlay(in: VsOut) -> @location(0) vec4<u32> {
    let s = parallax_surface(in.uv, in.world, in.normal);
    let tex = textureSample(atlas_tex, atlas_smp, s.uv);
    if (s.layer_id == 0u && tex.a < 0.5) {
        discard;
    }
    if (s.layer_id != 0u) {
        let p0 = u.layer[(s.layer_id - 1u) * 2u];
        let p1 = u.layer[(s.layer_id - 1u) * 2u + 1u];
        let below = textureSampleLevel(atlas_tex, atlas_smp, s.interior_uv, 0.0);
        let src = select(tex.a, dot(tex.rgb, vec3<f32>(0.299, 0.587, 0.114)), p1.x > 0.5);
        let opacity = clamp(src, p0.y, p0.z);
        let coverage = opacity + below.a * (1.0 - opacity);
        if (coverage < p1.y) {
            discard;
        }
    }
    return vec4<u32>(65535u, 0u, 3u, 1u);
}

@fragment
fn fs_diagnostic_water(in: VsOut) -> @location(0) vec4<u32> {
    return diagnostic_fragment(in.uv, in.world, 2u);
}

