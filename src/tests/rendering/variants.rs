//! Variants scenarios.

use super::*;

#[test]
fn atlas_derives_tinted_player_variants() {
    use crate::atlas::{ATLAS_TILES, apply_player_variants, build_procedural, builtin_slots};
    use crate::style::{SKIN_TONES, Style, skin_tile};
    let tp = 8u32;
    let mut img = build_procedural(tp);
    let px = ATLAS_TILES * tp;
    apply_player_variants(&mut img, px);
    let base = *builtin_slots().get("player_skin").unwrap();
    let sample = |slot: u16, dx: u32, dy: u32| -> [u8; 4] {
        let (tx, ty) = (
            slot as u32 % ATLAS_TILES * tp + dx,
            slot as u32 / ATLAS_TILES * tp + dy,
        );
        let i = ((ty * px + tx) * 4) as usize;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    };
    for (i, c) in SKIN_TONES.iter().enumerate() {
        let st = Style {
            skin: i as u8,
            ..Default::default()
        };
        let b = sample(base, 3, 3);
        let v = sample(skin_tile(&st), 3, 3);
        for ch in 0..3 {
            let want = (b[ch] as f32 * c[ch]).min(255.0) as u8;
            assert!(
                (v[ch] as i32 - want as i32).abs() <= 1,
                "variant = base x palette (tone {i} ch {ch})"
            );
        }
        assert_eq!(v[3], b[3], "alpha preserved");
    }
}

#[test]
fn pack_numbered_variants_get_their_own_slots() {
    use crate::atlas::{ATLAS_TILES, build_atlas, builtin_slots};
    let pack = tmp_dir("packvariants");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/gravel.png"), 8, 8, [10, 10, 10, 255]);
    write_solid_png(&pack.join("tiles/gravel_2.png"), 8, 8, [20, 20, 20, 255]);
    write_solid_png(&pack.join("tiles/gravel_3.png"), 8, 8, [30, 30, 30, 255]);
    // A variant may carry its own companion maps.
    write_solid_png(&pack.join("tiles/gravel_2_h.png"), 8, 8, [90, 90, 90, 255]);
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &[]);
    assert!(atlas.warnings.is_empty(), "{:?}", atlas.warnings);
    let gravel = *builtin_slots().get("gravel").unwrap();
    let sig = atlas.variants.signature();
    let alts = &sig.iter().find(|(k, _)| *k == gravel).unwrap().1;
    assert_eq!(alts.len(), 2, "base tile plus two alternates");
    // Each alternate is a distinct slot carrying its own art.
    assert_eq!(
        tile_center(&atlas.color, atlas.px, gravel),
        [10, 10, 10, 255]
    );
    let mut seen: Vec<u8> = alts
        .iter()
        .map(|s| tile_center(&atlas.color, atlas.px, *s)[0])
        .collect();
    seen.sort_unstable();
    assert_eq!(seen, vec![20, 30], "variants painted with their own albedo");
    // gravel_2's authored height landed on gravel_2's slot, not on the base.
    let tp = atlas.px / ATLAS_TILES;
    let at = |slot: u16| {
        let i = (((slot as u32 / ATLAS_TILES * tp) * atlas.px + slot as u32 % ATLAS_TILES * tp) * 4)
            as usize;
        atlas.material[i]
    };
    let v2 = alts
        .iter()
        .find(|s| tile_center(&atlas.color, atlas.px, **s)[0] == 20)
        .unwrap();
    assert_eq!(at(*v2), 90, "variant height on the variant slot");
    assert_eq!(at(gravel), 255, "base tile height untouched");
}

#[test]
fn variant_pick_is_per_face_stable_and_spread() {
    use crate::atlas::{build_atlas, builtin_slots};
    let pack = tmp_dir("packvarpick");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/stone.png"), 8, 8, [10, 10, 10, 255]);
    for (n, v) in [
        ("stone_2.png", 20u8),
        ("stone_3.png", 30),
        ("stone_4.png", 40),
    ] {
        write_solid_png(&pack.join("tiles").join(n), 8, 8, [v, v, v, 255]);
    }
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &[]);
    let v = &atlas.variants;
    let stone = *builtin_slots().get("stone").unwrap();

    // Stable: same block, same face, same answer. This is what makes a
    // position hash safe to use instead of stored per-block state.
    for _ in 0..4 {
        assert_eq!(v.pick(stone, 7, 3, -2, 1), v.pick(stone, 7, 3, -2, 1));
    }
    // Per-face: one cube should not be the same variant on all six sides.
    let faces: std::collections::HashSet<u16> =
        (0..6).map(|f| v.pick(stone, 7, 3, -2, f)).collect();
    assert!(faces.len() > 1, "all six faces picked the same variant");

    // Spread: over a volume, every variant gets used, and none dominates.
    let mut hist = std::collections::HashMap::new();
    for x in 0..16 {
        for y in 0..16 {
            for z in 0..16 {
                *hist.entry(v.pick(stone, x, y, z, 2)).or_insert(0usize) += 1;
            }
        }
    }
    assert_eq!(hist.len(), 4, "all four looks used: {hist:?}");
    let total: usize = hist.values().sum();
    for (slot, n) in &hist {
        let frac = *n as f64 / total as f64;
        assert!(
            (0.15..0.35).contains(&frac),
            "slot {slot} took {frac:.2} of faces, expected ~0.25"
        );
    }
    // A tile with no variants is the identity.
    let dirt = *builtin_slots().get("dirt").unwrap();
    assert_eq!(v.pick(dirt, 7, 3, -2, 1), dirt);
}
