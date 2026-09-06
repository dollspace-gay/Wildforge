//! Strata scenarios.

use super::*;

#[test]
fn generated_strata_tiles_are_reproducible_and_seamless() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let generator = std::fs::read_to_string(root.join("tools/gen_base_tiles.py")).unwrap();
    assert!(
        generator.contains("hashlib.sha256"),
        "shipped pixels need a process-independent digest seed"
    );
    assert!(
        !generator.contains("random.Random(hash("),
        "Python's randomized hash must never seed shipped pixels"
    );
    let expected = [
        (
            "sandstone",
            "3bf20c8a284fd1d7501924f62f7474516e037195605f2d06f84625b1a968da60",
        ),
        (
            "limestone",
            "24de0cdcf40d35eca5336dbd7242ecdbe50cbb7a83e405d5f8d70487bfb49159",
        ),
        (
            "shale",
            "e902974fffc2a996b453cbaf0e6a674c51d2d4935c976f74ed39368012069442",
        ),
        (
            "granite",
            "687ef6708c00592c48a0d935242f9892771051d6f6dade5baeeb5bb6c56e74f3",
        ),
        (
            "marble",
            "14c15c46992a1c035b8964bfc13b5048f0343d493afb95638ce20df767395dd6",
        ),
        (
            "slate",
            "57e27f5d902c5123eaa223c98487b5979baefb3ee26db7e7930ade253bd804b5",
        ),
        (
            "quartzite",
            "a899a4175c80e7649ab705d89faa839c621536f1b188f6bd27a1aa6a71895348",
        ),
        (
            "basalt",
            "c25f65a5c4279e26061d4f47950b7c8b4ef05d02568779c32602f2a9debaa5aa",
        ),
    ];
    for (name, digest) in expected {
        let bytes = std::fs::read(root.join(format!("base/textures/{name}.png"))).unwrap();
        assert_eq!(crate::visual_capture::sha256_hex(&bytes), digest, "{name}");
        let (width, height, pixels) = read_strata_png(name);
        assert_eq!((width, height), (32, 32), "{name}");
        for y in 0..height as usize {
            assert_eq!(
                pixels[y * width as usize],
                pixels[y * width as usize + width as usize - 1],
                "{name} horizontal seam at row {y}"
            );
        }
        for x in 0..width as usize {
            assert_eq!(
                pixels[x],
                pixels[(height as usize - 1) * width as usize + x],
                "{name} vertical seam at column {x}"
            );
        }
    }
}

#[test]
fn pale_strata_keep_distinct_large_scale_signals() {
    let names = ["sandstone", "limestone", "marble", "quartzite"];
    let signatures: Vec<_> = names
        .iter()
        .map(|name| {
            let (width, _, pixels) = read_strata_png(name);
            let (signature, rms) = large_scale_signature(&pixels, width as usize);
            assert!(rms >= 0.020, "{name} large-scale linear-light RMS {rms:.4}");
            (name, signature)
        })
        .collect();
    for (index, (name, signature)) in signatures.iter().enumerate() {
        let distinct = signatures
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .filter(|(_, (_, candidate))| {
                signature
                    .iter()
                    .zip(candidate)
                    .map(|(left, right)| left * right)
                    .sum::<f32>()
                    < 0.80
            })
            .count();
        assert!(
            distinct >= 2,
            "{name} needs two mechanically distinct pale peers"
        );
    }
}

#[test]
fn strata_companion_maps_survive_pack_inheritance() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let reg = base_reg();
    let base = crate::atlas::build_atlas(&reg.tex_files, &[], &reg.tex_names);
    assert!(
        base.warnings.is_empty(),
        "base warnings: {:?}",
        base.warnings
    );
    for pack in ["gemini", "dusk", "hewn"] {
        let chain = crate::atlas::pack_chain_in(&root.join("packs"), pack);
        let atlas = crate::atlas::build_atlas(&reg.tex_files, &chain, &reg.tex_names);
        assert!(
            atlas.warnings.is_empty(),
            "{pack} warnings: {:?}",
            atlas.warnings
        );
        for name in STRATA_NAMES {
            let block = reg.block_id(&format!("base:{name}")).unwrap();
            let slot = reg.block(block).tiles[0];
            let base_color = atlas_slot_bytes(&base.color, base.px, slot);
            let color = atlas_slot_bytes(&atlas.color, atlas.px, slot);
            let material = atlas_slot_bytes(&atlas.material, atlas.px, slot);
            let normal = atlas_slot_bytes(&atlas.normal, atlas.px, slot);
            let hewn_override = pack == "hewn" && matches!(name, "sandstone" | "limestone");
            if hewn_override {
                assert_ne!(color, base_color, "Hewn {name} albedo must win");
                assert_ne!(
                    material,
                    atlas_slot_bytes(&base.material, base.px, slot),
                    "Hewn {name} height must win"
                );
                assert_ne!(
                    normal,
                    atlas_slot_bytes(&base.normal, base.px, slot),
                    "Hewn {name} normal must win"
                );
            } else {
                assert_eq!(color, base_color, "{pack} must inherit base {name} albedo");
                assert_eq!(
                    material,
                    atlas_slot_bytes(&base.material, base.px, slot),
                    "{pack} must inherit base {name} material"
                );
                assert_eq!(
                    normal,
                    atlas_slot_bytes(&base.normal, base.px, slot),
                    "{pack} must inherit base {name} normal"
                );
            }
            assert!(
                material.chunks_exact(4).all(|pixel| pixel[3] == 0),
                "{pack} {name} cannot acquire an interior-layer id"
            );
        }
    }
}
