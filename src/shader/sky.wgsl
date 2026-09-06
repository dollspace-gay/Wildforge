// Procedural sky radiance along a world-space view direction `rd`. An analytic
// gradient (horizon -> zenith) whose day/night mix, warm twilight band, and sun
// halo are all driven by the true sun elevation, so one function feeds both the
// visible sky and the fog far-color. Sun/moon discs and stars land in later
// slices; this is the dome and its color.
fn sky_radiance(rd_in: vec3<f32>) -> vec3<f32> {
    let rd = normalize(rd_in);
    let sd = normalize(u.sun_dir_true.xyz);
    let radial_up = normalize(u.local_up.xyz);
    let up = clamp(dot(rd, radial_up), 0.0, 1.0);
    let se = dot(sd, radial_up); // local sun elevation, -1..1

    // Day palette: deep blue zenith -> pale horizon.
    let day_zenith = vec3<f32>(0.18, 0.40, 0.78);
    let day_horizon = vec3<f32>(0.66, 0.79, 0.94);
    let day_sky = mix(day_horizon, day_zenith, pow(up, 0.55));

    // Night palette: near-black, a hair of cold navy at the horizon. Kept this
    // dark so a moonlit *surface* out-reads the sky instead of the sky glowing
    // brighter than the ground it lights.
    let night_zenith = vec3<f32>(0.001, 0.0016, 0.005);
    let night_horizon = vec3<f32>(0.0025, 0.0045, 0.012);
    let night_sky = mix(night_horizon, night_zenith, pow(up, 0.7));

    // Sun elevation drives the day/night crossfade.
    let day = smoothstep(-0.12, 0.18, se);
    var col = mix(night_sky, day_sky, day);

    // Twilight: how strongly dusk/dawn is in play (peaks with the sun near the
    // horizon).
    let twilight = smoothstep(0.35, 0.0, se) * smoothstep(-0.32, 0.03, se);
    // Azimuthal proximity to the sun (horizontal only) and height above horizon.
    let hb = 1.0 - up; // 0 at zenith, 1 at horizon
    let rd_h = normalize(rd - radial_up * dot(rd, radial_up));
    let sd_h = normalize(sd - radial_up * dot(sd, radial_up));
    let az = dot(rd_h, sd_h);
    let sun_side = max(az, 0.0);

    // First dim and cool-drain the whole dome at dusk, so the horizon fire has
    // something dark to read against instead of washing into bright blue.
    col *= 1.0 - 0.5 * twilight;
    // A molten orange band *replaces* the horizon color (mix, so it wins instead
    // of washing to white), spread along the horizon and biased toward the sun.
    let band_w = pow(hb, 3.0) * (0.30 + 0.70 * sun_side) * twilight;
    col = mix(col, vec3<f32>(1.0, 0.32, 0.06), clamp(band_w, 0.0, 0.90));
    // A tight amber core right at the sun, pushed past 1.0 so bloom glares it.
    let core = pow(hb, 6.0) * pow(sun_side, 3.0) * twilight;
    col += vec3<f32>(1.0, 0.55, 0.20) * (core * 1.4);

    // A thin cold limn on the horizon where the sun set / will rise. It fades
    // with the sun's depth below the horizon — strong just after sunset / before
    // sunrise, essentially gone by deep midnight — and hugs the horizon line
    // tightly, so a dead-of-night sky stays near-black.
    let limn_amt = smoothstep(-0.55, -0.03, se);
    let limn = pow(hb, 12.0) * sun_side * limn_amt;
    col += vec3<f32>(0.03, 0.07, 0.16) * limn;

    // Soft warm halo around the sun (no hard disc yet).
    let mu = max(dot(rd, sd), 0.0);
    col += u.sun_col.rgb * (pow(mu, 8.0) * 0.55 * day);

    // Weather gloom flattens the dome toward the precomputed overcast gray.
    col = mix(col, u.sky.rgb, smoothstep(0.0, 0.85, u.sky.a));
    return col;
}

fn fog_distance(world: vec3<f32>) -> f32 {
    let camera_radius = length(u.cam.xyz);
    let world_radius = length(world);
    let angle = acos(clamp(dot(
        u.cam.xyz / max(camera_radius, 1e-3),
        world / max(world_radius, 1e-3)
    ), -1.0, 1.0));
    let surface_dist = angle * 5215.189;
    return length(vec2<f32>(surface_dist, world_radius - camera_radius));
}

fn apply_fog(color: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let dist = fog_distance(world);
    // Only the last tenth dissolves. This band used to start at 0.72,
    // which turned the far QUARTER of the view into sky — the thing you
    // were straining to see was always in it. Fog cannot go entirely:
    // without it the loaded world ends at a visible wall.
    let fog = smoothstep(u.cam.w * 0.90, u.cam.w * 1.0, dist);
    // Underwater keeps the flat watery fog color; above water, distant terrain
    // dissolves into the sky gradient along its own view ray.
    let rd = normalize(world - u.cam.xyz);
    let far = select(sky_radiance(rd), u.sky.rgb, u.misc.x > 0.5);
    return mix(color, far, fog);
}

// Fullscreen background sky. A single oversized triangle; the view ray is the
// unprojected NDC. Drawn first in the main pass with depth-write off, so terrain
// paints over it.
struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ray: vec3<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) vi: u32) -> SkyOut {
    var p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let ndc = p[vi];
    var o: SkyOut;
    o.clip = vec4<f32>(ndc, 1.0, 1.0);
    let near = u.inv_view_proj * vec4<f32>(ndc, 0.0, 1.0);
    let far = u.inv_view_proj * vec4<f32>(ndc, 1.0, 1.0);
    o.ray = far.xyz / far.w - near.xyz / near.w;
    return o;
}

@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    if (u.misc.x > 0.5) {
        // Underwater: keep the flat watery background.
        return vec4<f32>(u.sky.rgb, 1.0);
    }
    return vec4<f32>(sky_radiance(in.ray), 1.0);
}

