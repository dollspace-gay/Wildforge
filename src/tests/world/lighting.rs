//! Lighting scenarios.

use super::*;

#[test]
fn removing_a_torch_leaves_no_residual_block_light() {
    let reg = base_reg();
    let mut w = test_world_with("torch_residual", reg.clone());
    let torch = b(&reg, "base:torch");
    // A chunk corner: the torch (light 14) glows into all four chunks that
    // meet here, so removing it must drain every one of them.
    let (tx, ty, tz) = (15, 100, 15);

    // Baseline block light over the region the torch can possibly touch.
    let region = |f: &mut dyn FnMut(i32, i32, i32)| {
        for x in (tx - 16)..=(tx + 16) {
            for y in (ty - 16)..=(ty + 16) {
                for z in (tz - 16)..=(tz + 16) {
                    f(x, y, z);
                }
            }
        }
    };
    let mut before: HashMap<(i32, i32, i32), [u8; 3]> = HashMap::new();
    region(&mut |x, y, z| {
        before.insert((x, y, z), w.light_rgb_at(x, y, z).0);
    });

    w.set_block(tx, ty, tz, torch);
    assert!(
        w.light_rgb_at(tx, ty, tz).0[0] > 0,
        "torch lights its own cell"
    );
    assert!(
        w.light_rgb_at(tx + 3, ty, tz + 3).0[0] > 0,
        "glow crosses the seam into the diagonal chunk"
    );

    w.set_block(tx, ty, tz, AIR);

    // Every cell must return to exactly its pre-torch block light.
    region(&mut |x, y, z| {
        let now = w.light_rgb_at(x, y, z).0;
        assert_eq!(
            now,
            before[&(x, y, z)],
            "residual block light at {x},{y},{z}: {now:?}"
        );
    });
}

#[test]
fn block_light_crosses_and_clears_at_a_real_planet_face_seam() {
    use crate::planet::BlockPos;

    let reg = base_reg();
    let mut world = World::new(43, tmp_dir("planet-light-all-seams"), reg.clone());
    let torch = b(&reg, "base:torch");
    let emission = reg.block(torch).light_rgb;

    for seam in directed_planet_seams() {
        let source =
            BlockPos::new(seam.source.face(), seam.source.u(), 100, seam.source.v()).unwrap();
        let across =
            BlockPos::new(seam.across.face(), seam.across.u(), 100, seam.across.v()).unwrap();
        let missing: Vec<_> = [source.chunk(), across.chunk()]
            .into_iter()
            .filter(|chunk| !world.has_chunk(*chunk))
            .collect();
        world.insert_empty_chunks_for_test(missing);

        world.set_block_at(source, torch);
        assert_eq!(world.light_rgb_at_pos(source).0, emission);
        assert_eq!(
            world.light_rgb_at_pos(across).0,
            emission.map(|channel| channel.saturating_sub(1)),
            "light did not cross {:?} {:?}",
            seam.face,
            seam.direction
        );

        world.set_block_at(source, AIR);
        assert_eq!(
            world.light_rgb_at_pos(across).0,
            [0; 3],
            "light did not drain across {:?} {:?}",
            seam.face,
            seam.direction
        );
    }
}

#[test]
fn torch_light_propagates_and_walls_block_it() {
    let reg = base_reg();
    let mut w = test_world("lighttorch");
    let stone = reg.block_id("base:stone").unwrap();
    let torch = reg.block_id("base:torch").unwrap();
    // Sealed 9x9x9 stone box, hollow interior, well above terrain.
    for x in 0..9 {
        for z in 0..9 {
            for y in 150..159 {
                let shell = x == 0 || x == 8 || z == 0 || z == 8 || y == 150 || y == 158;
                w.set_block(x, y, z, if shell { stone } else { AIR });
            }
        }
    }
    assert_eq!(w.light_at(4, 154, 4), (0, 0), "sealed box is pitch black");
    w.set_block(4, 151, 4, torch);
    assert_eq!(w.light_at(4, 151, 4).0, 14, "torch emits 14");
    assert_eq!(w.light_at(5, 151, 4).0, 13, "one step dims by one");
    assert_eq!(w.light_at(7, 151, 4).0, 11, "three steps");
    assert_eq!(w.light_at(4, 153, 4).0, 12, "propagates vertically too");
    assert_eq!(w.light_at(10, 151, 4).0, 0, "opaque wall stops it");
    w.set_block(4, 151, 4, AIR);
    assert_eq!(
        w.light_at(5, 151, 4).0,
        0,
        "removing the torch relights dark"
    );
}

#[test]
fn sky_light_surface_cave_and_roof_opening() {
    let reg = base_reg();
    let mut w = test_world("lightsky");
    let stone = reg.block_id("base:stone").unwrap();
    // Open surface reads full sky (a built block, sea-proof).
    w.set_block(2, 140, 2, stone);
    assert_eq!(w.light_at(2, 141, 2).1, 15, "surface is full daylight");
    // Sealed box: no sky inside; opening the roof floods it.
    for x in 20..29 {
        for z in 20..29 {
            for yy in 150..159 {
                let shell = x == 20 || x == 28 || z == 20 || z == 28 || yy == 150 || yy == 158;
                w.set_block(x, yy, z, if shell { stone } else { AIR });
            }
        }
    }
    assert_eq!(w.light_at(24, 154, 24).1, 0, "sealed roof blocks sky");
    w.set_block(24, 158, 24, AIR); // skylight hole
    assert_eq!(
        w.light_at(24, 154, 24).1,
        15,
        "column under the hole is lit"
    );
    assert_eq!(
        w.light_at(26, 154, 24).1,
        13,
        "and floods sideways, dimming"
    );
}

#[test]
fn light_crosses_chunk_borders() {
    let reg = base_reg();
    let mut w = test_world("lightseam");
    let torch = reg.block_id("base:torch").unwrap();
    // Torch on the last column of chunk (0,0); the neighbor chunk must see it.
    w.set_block(15, 200, 8, torch);
    assert_eq!(w.light_at(15, 200, 8).0, 14);
    assert_eq!(w.light_at(16, 200, 8).0, 13, "crosses the seam");
    assert_eq!(w.light_at(19, 200, 8).0, 10, "keeps dimming next door");
}

#[test]
fn water_dims_sky_and_mod_blocks_can_glow() {
    let reg = base_reg();
    let mut w = test_world("lightwater");
    let water = reg.water_ids[0];
    let stone = reg.block_id("base:stone").unwrap();
    // A water-filled shaft walled in stone: light only enters from above,
    // dimming one level per water block.
    for x in 2..7 {
        for z in 2..7 {
            for y in 179..183 {
                w.set_block(x, y, z, stone);
            }
        }
    }
    for y in 180..183 {
        w.set_block(4, y, 4, water);
    }
    assert_eq!(w.light_at(4, 182, 4).1, 14, "first water block dims to 14");
    assert_eq!(w.light_at(4, 180, 4).1, 12, "third dims to 12");

    // Mod block with light = 9.
    let root = tmp_dir("glowmod");
    let dir = root.join("glow");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"glow\"\nworld_api = 2\n").unwrap();
    std::fs::write(
        dir.join("blocks.toml"),
        "[[block]]\nid = \"lamp\"\ntexture = \"@stone\"\nlight = 9\n",
    )
    .unwrap();
    let reg2 = Arc::new(registry::load(&root));
    let lamp = reg2.block_id("glow:lamp").unwrap();
    assert_eq!(reg2.block(lamp).light_emit, 9);
    let mut w2 = test_world_with("lightmod", reg2);
    w2.set_block(4, 200, 4, lamp);
    assert_eq!(w2.light_at(4, 200, 4).0, 9, "emitter itself");
    assert_eq!(w2.light_at(4, 202, 4).0, 7, "two steps out");
}

#[test]
fn placing_a_roof_casts_shadow() {
    let reg = base_reg();
    let mut w = test_world("lightshadow");
    let stone = reg.block_id("base:stone").unwrap();
    let y = w.surface_height(8, 8);
    assert_eq!(w.light_at(8, y + 1, 8).1, 15);
    w.set_block(8, y + 3, 8, stone); // roof two above the ground cell
    let shaded = w.light_at(8, y + 1, 8).1;
    assert!(shaded < 15, "column shadowed, got {shaded}");
    assert!(shaded >= 12, "but side-lit by flood, got {shaded}");
}

#[test]
fn relight_perf_sane() {
    let mut w = test_world("lightperf");
    let t0 = std::time::Instant::now();
    for _ in 0..10 {
        w.relight_and_cascade(tchunk(0, 0));
    }
    let per = t0.elapsed().as_secs_f32() / 10.0;
    assert!(per < 0.05, "relight cascade averaged {per:.4}s");
}

#[test]
fn stained_glass_filters_torchlight_by_channel() {
    let reg = base_reg();
    let mut w = test_world_with("gw-stain", reg.clone());
    let b = |n: &str| reg.block_id(n).unwrap();
    let my = 120;
    // A sealed corridor: torch | red glass | probe cell.
    let stone = b("base:stone");
    for x in 8..15 {
        for y in my - 1..my + 3 {
            for z in 8..12 {
                w.set_block(x, y, z, stone);
            }
        }
    }
    for x in 9..14 {
        w.set_block(x, my, 10, AIR);
        w.set_block(x, my + 1, 10, AIR);
    }
    w.set_block(9, my, 10, b("base:torch"));
    w.set_block(11, my, 10, b("base:red_glass"));
    w.set_block(11, my + 1, 10, b("base:red_glass"));
    let (rgb, _) = w.light_rgb_at(13, my, 10);
    assert!(rgb[0] > 0, "red passes red glass: {rgb:?}");
    assert_eq!(rgb[1], 0, "green dies at red glass: {rgb:?}");
    assert_eq!(rgb[2], 0, "blue dies at red glass: {rgb:?}");
    // Clear glass passes everything.
    w.set_block(11, my, 10, b("base:glass"));
    w.set_block(11, my + 1, 10, b("base:glass"));
    let (rgb, _) = w.light_rgb_at(13, my, 10);
    // A torch burns warm: blue is already spent at this range, so the
    // proof is red and green surviving where red glass killed green.
    assert!(
        rgb[0] > 0 && rgb[1] > 0,
        "clear passes the torch's warmth: {rgb:?}"
    );
}
