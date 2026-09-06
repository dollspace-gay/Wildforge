//! Pack layers scenarios.

use super::*;

#[test]
fn pack_inherits_layers_maps_over_a_parent_albedo() {
    use crate::atlas::{ATLAS_TILES, build_atlas, builtin_slots, pack_chain_in};
    // The point of inheritance: a child pack that ships only companion maps
    // rides on its parent's albedo instead of replacing the whole pack.
    let root = tmp_dir("packinherit");
    let parent = root.join("base_look");
    let child = root.join("maps_only");
    std::fs::create_dir_all(parent.join("tiles")).unwrap();
    std::fs::create_dir_all(child.join("tiles")).unwrap();
    std::fs::write(parent.join("pack.toml"), "name = \"Base Look\"\n").unwrap();
    std::fs::write(
        child.join("pack.toml"),
        "name = \"Maps Only\"\ninherits = \"base_look\"\n",
    )
    .unwrap();
    write_solid_png(&parent.join("tiles/stone.png"), 8, 8, [255, 0, 255, 255]);
    write_solid_png(&child.join("tiles/stone_h.png"), 8, 8, [64, 64, 64, 255]);

    let chain = pack_chain_in(&root, "maps_only");
    assert_eq!(chain.len(), 2, "ancestor + child");
    let atlas = build_atlas(&[], &chain, &[]);
    let stone = *builtin_slots().get("stone").unwrap();
    assert_eq!(
        tile_center(&atlas.color, atlas.px, stone),
        [255, 0, 255, 255],
        "parent's albedo survives - the child shipped no stone.png"
    );
    let tp = atlas.px / ATLAS_TILES;
    let i = (((stone as u32 / ATLAS_TILES * tp) * atlas.px + stone as u32 % ATLAS_TILES * tp) * 4)
        as usize;
    assert_eq!(atlas.material[i], 64, "child's authored height applied");
}

#[test]
fn pack_inherits_cycle_terminates() {
    use crate::atlas::pack_chain_in;
    let root = tmp_dir("packcycle");
    for (id, parent) in [("a", "b"), ("b", "a")] {
        std::fs::create_dir_all(root.join(id).join("tiles")).unwrap();
        std::fs::write(
            root.join(id).join("pack.toml"),
            format!("inherits = \"{parent}\"\n"),
        )
        .unwrap();
    }
    // Must not hang, and must not visit a pack twice.
    assert_eq!(pack_chain_in(&root, "a").len(), 2);
    // A pack naming itself is ignored outright.
    std::fs::create_dir_all(root.join("solo").join("tiles")).unwrap();
    std::fs::write(root.join("solo/pack.toml"), "inherits = \"solo\"\n").unwrap();
    assert_eq!(pack_chain_in(&root, "solo").len(), 1);
    // A parent that resolves to nothing degrades to the child alone.
    std::fs::create_dir_all(root.join("orphan").join("tiles")).unwrap();
    std::fs::write(root.join("orphan/pack.toml"), "inherits = \"nope\"\n").unwrap();
    assert_eq!(pack_chain_in(&root, "orphan").len(), 1);
}

#[test]
fn pack_can_author_the_interior_layer() {
    use crate::atlas::{ATLAS_TILES, build_atlas, builtin_slots};
    // An interior layer is a full second ALBEDO at a slot of its own, not a
    // greyscale mask: the shader finds it via `interior_base + (alpha - 1)`.
    let pack = tmp_dir("packinterior");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/leaves.png"), 8, 8, [40, 90, 30, 255]);
    write_solid_png(&pack.join("tiles/leaves_i.png"), 8, 8, [10, 20, 200, 255]);
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &[]);
    assert!(atlas.warnings.is_empty(), "{:?}", atlas.warnings);
    let leaves = *builtin_slots().get("leaves").unwrap();
    let tp = atlas.px / ATLAS_TILES;
    let at = |slot: u16| {
        (((slot as u32 / ATLAS_TILES * tp) * atlas.px + slot as u32 % ATLAS_TILES * tp) * 4)
            as usize
    };
    let id = atlas.material[at(leaves) + 3];
    assert_ne!(
        id, 0,
        "surface tile names its interior layer in material alpha"
    );
    let layer = atlas.interior_base + (id as u16 - 1);
    assert_eq!(
        tile_center(&atlas.color, atlas.px, layer),
        [10, 20, 200, 255],
        "interior layer keeps its own colour at its own slot"
    );
    assert_eq!(
        tile_center(&atlas.color, atlas.px, leaves),
        [40, 90, 30, 255],
        "surface albedo untouched"
    );
    assert_eq!(
        atlas.material[at(layer) + 3],
        0,
        "the layer itself has no layer"
    );
}

#[test]
fn luminance_height_fallback_keeps_an_authored_interior() {
    use crate::atlas::{ATLAS_TILES, build_atlas, builtin_slots};
    // stone/cobblestone get a free luminance height when none is authored. That
    // fallback writes R, and must not disturb the interior-layer id in A.
    let pack = tmp_dir("packinteriorstone");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/stone.png"), 8, 8, [120, 120, 120, 255]);
    write_solid_png(&pack.join("tiles/stone_i.png"), 8, 8, [180, 180, 180, 255]);
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &[]);
    let stone = *builtin_slots().get("stone").unwrap();
    let tp = atlas.px / ATLAS_TILES;
    let i = (((stone as u32 / ATLAS_TILES * tp) * atlas.px + stone as u32 % ATLAS_TILES * tp) * 4)
        as usize;
    assert_ne!(
        atlas.material[i + 3],
        0,
        "interior layer id survived the fallback"
    );
    assert!(
        atlas.material[i] < 255,
        "and the free luminance height still applied"
    );
}

#[test]
fn variant_shipping_only_maps_inherits_the_base_look() {
    use crate::atlas::{build_atlas, builtin_slots};
    // `stone_2_n.png` with no `stone_2.png`: the variant still needs something
    // to draw, so it takes the base albedo rather than rendering blank.
    let pack = tmp_dir("packvarmaps");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/stone.png"), 8, 8, [77, 88, 99, 255]);
    write_solid_png(
        &pack.join("tiles/stone_2_n.png"),
        8,
        8,
        [128, 128, 255, 255],
    );
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &[]);
    let stone = *builtin_slots().get("stone").unwrap();
    let sig = atlas.variants.signature();
    let alt = sig.iter().find(|(k, _)| *k == stone).unwrap().1[0];
    assert_eq!(
        tile_center(&atlas.color, atlas.px, alt),
        [77, 88, 99, 255],
        "map-only variant inherited the base albedo"
    );
}

#[test]
fn layer_settings_come_from_pack_toml() {
    use crate::atlas::{build_atlas, builtin_slots};
    // What an interior layer means is the material's business. Opacity defaults
    // to the surface's own ALPHA — the general mechanism — and "luminance" is a
    // named special case for sheets like ice, not a rule compiled into the shader.
    let pack = tmp_dir("packlayers");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    for t in ["ice", "leaves"] {
        write_solid_png(
            &pack.join(format!("tiles/{t}.png")),
            8,
            8,
            [80, 90, 100, 255],
        );
        write_solid_png(
            &pack.join(format!("tiles/{t}_i.png")),
            8,
            8,
            [10, 20, 30, 255],
        );
    }
    std::fs::write(
        pack.join("pack.toml"),
        "[layers.ice]\n\
         depth = 0.13\n\
         opacity = \"luminance\"\n\
         opacity_min = 0.3\n\
         opacity_max = 0.75\n\
         dim = 0.82\n\
         cutoff = 0.0\n\
         [layers.leaves]\n\
         depth = 0.30\n\
         cutoff = 0.35\n",
    )
    .unwrap();
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &[]);
    assert!(atlas.warnings.is_empty(), "{:?}", atlas.warnings);

    let id_of = |name: &str| -> usize {
        let slot = *builtin_slots().get(name).unwrap();
        let tp = atlas.px / crate::atlas::ATLAS_TILES;
        let i = (((slot as u32 / crate::atlas::ATLAS_TILES * tp) * atlas.px
            + slot as u32 % crate::atlas::ATLAS_TILES * tp)
            * 4) as usize;
        atlas.material[i + 3] as usize
    };
    let ice = atlas.layer_params[id_of("ice") - 1];
    let leaves = atlas.layer_params[id_of("leaves") - 1];

    assert_eq!(ice.mode, 1, "ice asked for luminance");
    assert_eq!(
        leaves.mode, 0,
        "leaves said nothing, so the alpha default holds"
    );
    assert!((ice.depth - 0.13).abs() < 1e-6);
    assert!((leaves.depth - 0.30).abs() < 1e-6);
    assert!((ice.opacity_min - 0.3).abs() < 1e-6);
    assert!((leaves.cutoff - 0.35).abs() < 1e-6);
    // Unstated fields fall back rather than zeroing out.
    let d = crate::atlas::LayerParams::default();
    assert!(
        (leaves.dim - d.dim).abs() < 1e-6,
        "unset dim keeps the default"
    );
    assert!((leaves.opacity_max - d.opacity_max).abs() < 1e-6);
}

#[test]
fn a_tile_without_pack_toml_layer_settings_gets_defaults() {
    use crate::atlas::{LayerParams, build_atlas};
    let pack = tmp_dir("packlayerdefault");
    std::fs::create_dir_all(pack.join("tiles")).unwrap();
    write_solid_png(&pack.join("tiles/ice.png"), 8, 8, [80, 90, 100, 255]);
    write_solid_png(&pack.join("tiles/ice_i.png"), 8, 8, [10, 20, 30, 255]);
    let atlas = build_atlas(&[], &[crate::atlas::PackSource::Dir(pack)], &[]);
    assert_eq!(atlas.layer_params.len(), 1);
    assert_eq!(atlas.layer_params[0], LayerParams::default());
    assert_eq!(atlas.layer_params[0].mode, 0, "alpha is the default source");
}
