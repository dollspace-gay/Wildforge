//! Dedicated hardware regression gate for zero-contribution point-light shadows.
//! Run: cargo test --locked renderer::lighting_tests::shadow_guard_hardware -- --ignored --exact --nocapture
//! Native scene and performance qualification remains separate from this shader probe.

use super::Uniforms;
use bytemuck::{Pod, Zeroable};
#[path = "lighting_test_fixture.rs"]
mod fixture;
use fixture::HardwareProbe;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Input {
    normal: [f32; 4],
    detail: [f32; 4],
    baked_ao: [f32; 4],
    world_sky: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Output {
    rgb: [f32; 4],
    cube: u32,
    dda: u32,
    padding: [u32; 2],
}

struct Case {
    name: String,
    input: Input,
    positive: bool,
    billboard: bool,
}

const PROBE: &str = r#"
struct ProbeInput { normal: vec4<f32>, detail: vec4<f32>, baked_ao: vec4<f32>, world_sky: vec4<f32> };
struct ProbeOutput { rgb: vec4<f32>, cube: atomic<u32>, dda: atomic<u32>, padding: vec2<u32> };
@group(3) @binding(0) var<storage, read> probe_inputs: array<ProbeInput>;
@group(3) @binding(1) var<storage, read_write> probe_outputs: array<ProbeOutput>;
var<private> probe_index: u32;
@compute @workgroup_size(1)
fn probe_main(@builtin(global_invocation_id) id: vec3<u32>) {
    probe_index = id.x;
    let p = probe_inputs[probe_index];
    let rgb = world_light(p.normal.xyz, p.detail.xyz, p.baked_ao.xyz,
                          p.world_sky.w, p.baked_ao.w, p.world_sky.xyz);
    probe_outputs[probe_index].rgb = vec4<f32>(rgb, 1.0);
}
"#;

fn shader(legacy: bool) -> String {
    let mut source = crate::shader::WORLD.to_owned();
    if legacy {
        // Removing just the added condition retains every production operation.
        // An unguarded production shader still reaches the GPU and fails counters.
        source = source.replace("ndl2 > 0.0 && ", "");
    }
    for (signature, counter) in [
        ("fn point_shadow(", "cube"),
        ("fn point_shadow_dda(", "dda"),
    ] {
        let start = source.find(signature).expect("production shadow function");
        let body = start + source[start..].find('{').expect("function body") + 1;
        source.insert_str(
            body,
            &format!("\n atomicAdd(&probe_outputs[probe_index].{counter}, 1u);\n"),
        );
    }
    source.push_str(PROBE);
    source
}

fn cases() -> Vec<Case> {
    let normals = [
        ("opposed", [0.0, -1.0, 0.0, 0.0], false, false),
        ("perpendicular", [1.0, 0.0, 0.0, 0.0], false, false),
        ("grazing-positive", [1.0, 0.000001, 0.0, 0.0], true, false),
        ("facing", [0.0, 1.0, 0.0, 0.0], true, false),
        ("relief", [0.7, 1.0, 0.3, 0.0], true, false),
        ("billboard", [0.0, 1.0, 0.0, 0.0], false, true),
    ];
    let mut out = Vec::new();
    for (baked, sky) in [
        ([0.0, 0.0, 0.0, 1.0], 0.0),
        ([0.4, 0.2, 0.8, 0.6], 0.4),
        ([0.9, 0.3, 0.1, 0.2], 1.0),
    ] {
        for (name, detail, positive, billboard) in normals {
            out.push(Case {
                name: format!("{name}/sky={sky}"),
                input: Input {
                    normal: if billboard {
                        [0.0; 4]
                    } else {
                        [0.0, 1.0, 0.0, 0.0]
                    },
                    detail,
                    baked_ao: baked,
                    world_sky: [4.25, 4.5, 4.5, sky],
                },
                positive,
                billboard,
            });
        }
    }
    out
}

fn uniforms(count: u32, dda: bool, debug: u32, radius: f32) -> Uniforms {
    let mut u = Uniforms::zeroed();
    u.cam = [4.25, 4.5, 8.5, 32.0];
    u.misc = [0.0, 0.8, 1280.0, 720.0];
    u.sun_dir = [0.0, 1.0, 0.0, 0.0];
    u.sun_col = [0.1, 0.12, 0.14, 0.1];
    u.amb_col = [0.0, 0.0, 0.0, 0.04];
    u.light_vp = [glam::Mat4::from_scale(glam::Vec3::splat(0.05)).to_cols_array_2d(); 3];
    u.sh[0] = [0.08, 0.1, 0.12, 0.0];
    u.room_sh[0] = [0.12, 0.08, 0.04, 0.3];
    u.pt_count = [count, 0, debug, u32::from(dda)];
    u.pt_pos[0] = [4.25, 8.5, 4.5, 16.0];
    u.pt_pos[1] = [4.25, 12.5, 4.5, 16.0];
    u.pt_col[0] = [1.6, 0.8, 0.3, 0.0];
    u.pt_col[1] = [0.3, 0.8, 1.6, 0.0];
    u.pt_misc[0] = [0.4, 14.0, 1.0, radius];
    u.pt_misc[1] = [0.3, 14.0, 1.0, radius];
    u
}

fn compare(probe: &HardwareProbe, cases: &[Case], u: &Uniforms, active: u32) {
    let guarded = probe.run(u, 0);
    let legacy = probe.run(u, 1);
    assert_eq!(guarded.len(), cases.len());
    assert_eq!(legacy.len(), cases.len());
    for ((case, guarded), legacy) in cases.iter().zip(guarded).zip(legacy) {
        let context = format!(
            "{} / flags={:?} radius={}",
            case.name, u.pt_count, u.pt_misc[0][3]
        );
        assert!(
            guarded.rgb.iter().chain(&legacy.rgb).all(|v| v.is_finite()),
            "nonfinite {context}"
        );
        assert_eq!(
            guarded.rgb.map(f32::to_bits),
            legacy.rgb.map(f32::to_bits),
            "lighting changed: {context}"
        );
        let guarded_calls = if case.positive { active } else { 0 };
        let legacy_calls = if case.billboard { 0 } else { active };
        let expected = |calls| {
            if u.pt_count[3] == 0 {
                (calls, 0)
            } else {
                (0, calls)
            }
        };
        assert_eq!(
            (guarded.cube, guarded.dda),
            expected(guarded_calls),
            "guarded work: {context}"
        );
        assert_eq!(
            (legacy.cube, legacy.dda),
            expected(legacy_calls),
            "legacy negative control: {context}"
        );
    }
}

#[test]
#[ignore = "requires a real Vulkan DiscreteGpu; run the explicit hardware gate"]
fn shadow_guard_hardware() {
    let cases = cases();
    let probe = HardwareProbe::new(&cases);
    for dda in [false, true] {
        for debug in 0..=4 {
            for radius in [0.0, 0.15, 0.75] {
                for count in [1, 2] {
                    compare(&probe, &cases, &uniforms(count, dda, debug, radius), count);
                }
            }
        }
        compare(&probe, &cases, &uniforms(0, dda, 0, 0.15), 0);
        let mut no_shadows = uniforms(1, dda, 0, 0.15);
        no_shadows.pt_misc[0][2] = 0.0;
        compare(&probe, &cases, &no_shadows, 0);
        let mut held = uniforms(1, dda, 0, 0.15);
        held.cam[..3].copy_from_slice(&held.pt_pos[0][..3]);
        compare(&probe, &cases, &held, 0);
    }
    eprintln!(
        "shadow guard: 1188 finite, bit-identical production/legacy pairs completed; exact cube/DDA call counts checked"
    );
}
