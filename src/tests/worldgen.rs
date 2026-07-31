//! Terrain, biome, ore, vegetation, and structure generation.

use super::*;

#[test]
fn generation_is_deterministic() {
    let mut a = test_world("det-a");
    let mut b2 = test_world("det-b");
    a.ensure_chunk(tchunk(5, -3));
    b2.ensure_chunk(tchunk(5, -3));
    assert_eq!(
        a.chunks()[&tchunk(5, -3)].raw(),
        b2.chunks()[&tchunk(5, -3)].raw()
    );
}

#[test]
fn planetary_generator_is_continuous_across_every_face_edge() {
    use crate::planet::{Direction4, FACE_BLOCKS, Face, SurfacePos, step4};

    let reg = base_reg();
    let generator = Generator::new(42, &reg);
    let mut chunks = std::collections::HashMap::new();
    let top = |chunk: &crate::chunk::Chunk, x: usize, z: usize| {
        (1..CHUNK_Y)
            .rev()
            .find(|&y| reg.is_solid(chunk.get(x, y, z)))
            .unwrap_or(0) as i32
    };

    for face in Face::ALL {
        for direction in [
            Direction4::East,
            Direction4::North,
            Direction4::West,
            Direction4::South,
        ] {
            let (u, v) = match direction {
                Direction4::East => (FACE_BLOCKS - 1, FACE_BLOCKS / 2),
                Direction4::North => (FACE_BLOCKS / 2, FACE_BLOCKS - 1),
                Direction4::West => (0, FACE_BLOCKS / 2),
                Direction4::South => (FACE_BLOCKS / 2, 0),
            };
            let here = SurfacePos::new(face, u, v).unwrap();
            let across = step4(here, direction).pos;
            assert_ne!(here.face(), across.face());

            let a = generator.climate_at(here);
            let b = generator.climate_at(across);
            assert!(
                (a.t - b.t).abs() < 0.03,
                "temperature tore at {face:?} {direction:?}"
            );
            assert!(
                (a.h - b.h).abs() < 0.03,
                "moisture tore at {face:?} {direction:?}"
            );
            assert!(
                (a.c - b.c).abs() < 0.03,
                "continent tore at {face:?} {direction:?}"
            );
            assert!(
                (a.e - b.e).abs() < 0.03,
                "erosion tore at {face:?} {direction:?}"
            );

            for surface in [here, across] {
                let chunk_pos = crate::planet::ChunkPos::from_surface(surface);
                chunks
                    .entry(chunk_pos)
                    .or_insert_with(|| generator.generate(chunk_pos, &reg));
            }
            let here_chunk = crate::planet::ChunkPos::from_surface(here);
            let across_chunk = crate::planet::ChunkPos::from_surface(across);
            let ha = top(
                &chunks[&here_chunk],
                usize::from(here.u()) % crate::chunk::CHUNK_X,
                usize::from(here.v()) % crate::chunk::CHUNK_Z,
            );
            let hb = top(
                &chunks[&across_chunk],
                usize::from(across.u()) % crate::chunk::CHUNK_X,
                usize::from(across.v()) % crate::chunk::CHUNK_Z,
            );
            assert!(
                (ha - hb).abs() <= 12,
                "terrain tore by {} blocks at {face:?} {direction:?}: {ha} vs {hb}",
                (ha - hb).abs()
            );
        }
    }
}

#[test]
fn terrain_has_bedrock_and_surface() {
    let reg = base_reg();
    let w = test_world_with("terrain", reg.clone());
    assert_eq!(w.get_block(0, 0, 0), b(&reg, "base:bedrock"));
    let h = w.surface_height(0, 0);
    assert!(h > 4 && h < CHUNK_Y as i32 - 1);
    assert!(reg.is_solid(w.get_block(0, h, 0)));
    assert!(!reg.is_solid(w.get_block(0, h + 2, 0)));
}

#[test]
fn mod_ore_generates_in_terrain() {
    let root = tmp_dir("oremod");
    write_demo_mod(&root);
    let reg = Arc::new(registry::load(&root));
    let ore = reg.block_id("testium:ore").unwrap();
    let mut w = World::new(42, tmp_dir("oreworld"), reg.clone());
    let mut found = 0;
    for cx in -2..=2 {
        for cz in -2..=2 {
            w.ensure_chunk(tchunk(cx, cz));
        }
    }
    for x in -32..32 {
        for z in -32..32 {
            for y in 4..60 {
                if w.get_block(x, y, z) == ore {
                    found += 1;
                }
            }
        }
    }
    assert!(found > 0, "ore feature should generate veins");
}

#[test]
fn all_seven_biomes_exist_and_are_deterministic() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let g2 = Generator::new(42, &reg);
    for biome in [
        Biome::Forest,
        Biome::Plains,
        Biome::Desert,
        Biome::Jungle,
        Biome::Scrubland,
        Biome::Taiga,
        Biome::Arctic,
        Biome::Swamp,
        Biome::Savanna,
        Biome::Tundra,
        Biome::Badlands,
    ] {
        let pos = find_biome(&g, biome)
            .unwrap_or_else(|| panic!("{biome:?} not found within search radius"));
        assert_eq!(g.biome_at(pos), g2.biome_at(pos), "same seed, same biome");
    }
    // Different seeds shuffle the layout.
    let g3 = Generator::new(1337, &reg);
    let mut diff = 0;
    for face in crate::planet::Face::ALL {
        for i in 0..8u16 {
            let pos =
                crate::planet::SurfacePos::new(face, 512 + i * 877, 768 + ((i * 1297) % 6500))
                    .unwrap();
            if g.biome_at(pos) != g3.biome_at(pos) {
                diff += 1;
            }
        }
    }
    assert!(
        diff > 5,
        "different seeds should give different biome maps ({diff})"
    );
}

#[test]
fn desert_has_sand_surface_and_cacti() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let anchor = find_biome(&g, Biome::Desert).expect("dry desert column");
    let (mut world, _) = gen_at_surface(&reg, "desert", anchor);
    let sand = b(&reg, "base:sand");
    let cactus = b(&reg, "base:cactus");
    let mut sandy = 0;
    for du in -16..=16 {
        for dv in -16..=16 {
            let pos = surface_offset(anchor, du, dv);
            let h = world.surface_height_at(pos);
            sandy += u32::from(block_at(&world, pos, h) == sand);
        }
    }
    assert!(
        sandy > 300,
        "desert country is substantially sanded ({sandy})"
    );
    let center = ChunkPos::from_surface(anchor);
    for du in -4..=4 {
        for dv in -4..=4 {
            world.ensure_chunk(center.offset(du, dv));
        }
    }
    let cactus_cells: usize = world
        .chunks()
        .values()
        .map(|chunk| {
            chunk
                .raw()
                .iter()
                .filter(|&&block| block == cactus.0)
                .count()
        })
        .sum();
    assert!(cactus_cells > 0, "cacti should generate in deserts");
}

#[test]
fn arctic_has_snow_and_frozen_ocean() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let anchor = find_biome(&g, Biome::Arctic).expect("an arctic country");
    let mut land = None;
    let mut ocean = None;
    for du in -128..=128 {
        for dv in -128..=128 {
            let pos = surface_offset(anchor, du, dv);
            if g.biome_at(pos) != Biome::Arctic {
                continue;
            }
            let h = g.surface_estimate_at(pos);
            if h > crate::chunk::SEA_LEVEL + 1 {
                land.get_or_insert(pos);
            }
            if h < crate::chunk::SEA_LEVEL - 2 {
                ocean.get_or_insert(pos);
            }
        }
    }
    if let Some(pos) = land {
        let (world, h) = gen_at_surface(&reg, "arctic-land", pos);
        assert_eq!(block_at(&world, pos, h), b(&reg, "base:snow"));
    }
    if let Some(pos) = ocean {
        let (world, _) = gen_at_surface(&reg, "arctic-sea", pos);
        assert_eq!(
            block_at(&world, pos, crate::chunk::SEA_LEVEL),
            b(&reg, "base:ice")
        );
        assert!(reg.is_water(block_at(&world, pos, crate::chunk::SEA_LEVEL - 1)));
    }
    assert!(land.is_some() || ocean.is_some());
}

#[test]
fn jungle_denser_than_plains() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let count_logs = |name: &str, biome: Biome, log_name: &str| -> (u32, u32) {
        let anchor = find_biome(&g, biome).unwrap();
        let cp = ChunkPos::from_surface(anchor);
        let mut world = World::new(42, tmp_dir(name), reg.clone());
        let log = b(&reg, log_name);
        let mut logs = 0;
        let mut cols = 0;
        for du in -3..=3 {
            for dv in -3..=3 {
                let chunk = cp.offset(du, dv);
                world.ensure_chunk(chunk);
                for lx in 0..16u16 {
                    for lz in 0..16u16 {
                        let pos = crate::planet::SurfacePos::new(
                            chunk.face(),
                            chunk.u() * 16 + lx,
                            chunk.v() * 16 + lz,
                        )
                        .unwrap();
                        if world.generator.biome_at(pos) == biome {
                            cols += 1;
                            if (60..200).any(|y| block_at(&world, pos, y) == log) {
                                logs += 1;
                            }
                        }
                    }
                }
            }
        }
        (logs, cols.max(1))
    };
    let (jl, jc) = count_logs("jungle", Biome::Jungle, "base:jungle_log");
    let (pl, pc) = count_logs("plains", Biome::Plains, "base:log");
    let jd = jl as f32 / jc as f32;
    let pd = pl as f32 / pc as f32;
    assert!(
        jd > pd * 3.0,
        "jungle tree density ({jd:.4}) should dwarf plains ({pd:.4})"
    );
}

#[test]
fn spline_eval_clamps_and_interpolates() {
    use crate::worldgen::Spline;
    let s = Spline::new(&[(-1.0, 10.0), (0.0, 20.0), (1.0, 100.0)]);
    assert_eq!(s.at(-2.0), 10.0);
    assert_eq!(s.at(2.0), 100.0);
    assert_eq!(s.at(-1.0), 10.0);
    assert!((s.at(-0.5) - 15.0).abs() < 1e-4);
    assert!((s.at(0.5) - 60.0).abs() < 1e-4);
}

#[test]
fn terrain_has_overhangs() {
    // 3D density terrain must produce air-under-solid somewhere.
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("overhang"), reg.clone());
    let mut found = false;
    'outer: for cx in -6..6 {
        for cz in -6..6 {
            w.ensure_chunk(tchunk(cx, cz));
            for lx in 0..16 {
                for lz in 0..16 {
                    let (x, z) = (cx * 16 + lx, cz * 16 + lz);
                    // solid above air above solid, all above sea level
                    for y in 70..200 {
                        if w.get_block(x, y, z) == AIR
                            && reg.is_solid(w.get_block(x, y + 1, z))
                            && (66..y).any(|yy| reg.is_solid(w.get_block(x, yy, z)))
                        {
                            found = true;
                            break 'outer;
                        }
                    }
                }
            }
        }
    }
    assert!(found, "3D terrain should create overhangs");
}

#[test]
fn mountains_rise_above_plains() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let plains = find_biome(&g, Biome::Plains).expect("plains country");
    let mountain = find_biome_where(&g, Biome::Mountains, |_| true).expect("mountain country");
    let mut p = 0;
    for du in (-96..=96).step_by(8) {
        for dv in (-96..=96).step_by(8) {
            p = p.max(g.surface_estimate_at(surface_offset(plains, du, dv)));
        }
    }
    let mut m = 0;
    for du in (-128..=128).step_by(4) {
        for dv in (-128..=128).step_by(4) {
            m = m.max(g.surface_estimate_at(surface_offset(mountain, du, dv)));
        }
    }
    // Goal 1's broad fields are temporary, but they must still produce a
    // visibly distinct highland tier before the scientific relief pass.
    assert!(m > 90, "temporary fold ranges should reach high ({m})");
    assert!(m > p + 15, "ranges ({m}) rise above plains ({p})");
}

#[test]
fn oceans_exist_and_fill_with_water() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let pos = find_biome_where(&g, Biome::Ocean, |pos| g.surface_estimate_at(pos) < 46)
        .expect("a deep ocean should exist");
    let (world, floor) = gen_at_surface(&reg, "ocean", pos);
    assert!(
        reg.is_water(block_at(&world, pos, crate::chunk::SEA_LEVEL))
            || block_at(&world, pos, crate::chunk::SEA_LEVEL) == b(&reg, "base:ice")
    );
    assert!(floor < 62, "ocean floor below sea level ({floor})");
    let fb = block_at(&world, pos, floor);
    assert!(
        fb == b(&reg, "base:sand") || fb == b(&reg, "base:gravel"),
        "ocean floor surfaced with sand/gravel, got {}",
        reg.block(fb).name
    );
}

#[test]
fn caves_exist_underground() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("caves2"), reg.clone());
    let mut pockets = 0;
    for cx in -3..3 {
        for cz in -3..3 {
            w.ensure_chunk(tchunk(cx, cz));
            for lx in 0..16 {
                for lz in 0..16 {
                    let (x, z) = (cx * 16 + lx, cz * 16 + lz);
                    let top = w.surface_height(x, z);
                    for y in 6..(top - 10).min(50) {
                        if w.get_block(x, y, z) == AIR {
                            pockets += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(
        pockets > 200,
        "underground cave air should be plentiful ({pockets})"
    );
}

#[test]
fn steep_faces_and_peaks_surface_correctly() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let anchor = find_biome_where(&g, Biome::Mountains, |_| true).expect("mountain country");
    let peak = (-128..=128)
        .step_by(4)
        .flat_map(|du| {
            (-128..=128)
                .step_by(4)
                .map(move |dv| surface_offset(anchor, du, dv))
        })
        .max_by_key(|&pos| g.surface_estimate_at(pos))
        .unwrap();
    let (world, _) = gen_at_surface(&reg, "peaks", peak);
    let cp = ChunkPos::from_surface(peak);
    let snow = b(&reg, "base:snow");
    let stone = b(&reg, "base:stone");
    let (mut snowy, mut stony, mut grassy_high) = (0, 0, 0);
    for lx in 0..16u16 {
        for lz in 0..16u16 {
            let pos = crate::planet::SurfacePos::new(cp.face(), cp.u() * 16 + lx, cp.v() * 16 + lz)
                .unwrap();
            let top = world.surface_height_at(pos);
            let tb = block_at(&world, pos, top);
            if top >= 170 && tb == snow {
                snowy += 1;
            }
            if tb == stone {
                stony += 1;
            }
            if top >= 170 && tb == b(&reg, "base:grass") {
                grassy_high += 1;
            }
        }
    }
    assert_eq!(grassy_high, 0, "no grass on extreme peaks");
    assert!(snowy + stony > 0, "mountain tops are stone/snow");
}

#[test]
fn biomes_grow_their_own_wood() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let count_wood = |name: &str, biome: Biome, log: &str| -> (u32, u32) {
        let anchor = find_biome(&g, biome).unwrap();
        let cp = ChunkPos::from_surface(anchor);
        let mut world = World::new(42, tmp_dir(name), reg.clone());
        let want = b(&reg, log);
        let oak = b(&reg, "base:log");
        let (mut hits, mut oaks) = (0, 0);
        for du in -4..=4 {
            for dv in -4..=4 {
                let chunk = cp.offset(du, dv);
                world.ensure_chunk(chunk);
                for lx in 0..16u16 {
                    for lz in 0..16u16 {
                        let pos = crate::planet::SurfacePos::new(
                            chunk.face(),
                            chunk.u() * 16 + lx,
                            chunk.v() * 16 + lz,
                        )
                        .unwrap();
                        if world.generator.biome_at(pos) != biome {
                            continue;
                        }
                        for y in 64..200 {
                            let blk = block_at(&world, pos, y);
                            if blk == want {
                                hits += 1;
                            } else if blk == oak {
                                oaks += 1;
                            }
                        }
                    }
                }
            }
        }
        (hits, oaks)
    };
    let (spruce, _) = count_wood("wood-taiga", Biome::Taiga, "base:spruce_log");
    assert!(spruce > 0, "taiga should grow spruce");
    let (jungle, _) = count_wood("wood-jungle", Biome::Jungle, "base:jungle_log");
    assert!(jungle > 0, "jungle should grow jungle wood");
    let (acacia, _) = count_wood("wood-scrub", Biome::Scrubland, "base:acacia_log");
    assert!(acacia > 0, "scrubland shrubs should be acacia");
    // Forests mix oak and birch.
    let (birch, oaks) = count_wood("wood-forest", Biome::Forest, "base:birch_log");
    assert!(
        birch > 0 && oaks > 0,
        "forest should mix birch ({birch}) and oak ({oaks})"
    );
}

#[test]
fn base_metal_ores_generate() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("metals"), reg.clone());
    let (cu, sn) = (b(&reg, "base:copper_ore"), b(&reg, "base:tin_ore"));
    let (mut found_cu, mut found_sn) = (0, 0);
    for cx in -3..3 {
        for cz in -3..3 {
            w.ensure_chunk(tchunk(cx, cz));
            for lx in 0..16 {
                for lz in 0..16 {
                    for y in 4..73 {
                        let blk = w.get_block(cx * 16 + lx, y, cz * 16 + lz);
                        if blk == cu {
                            found_cu += 1;
                        } else if blk == sn {
                            found_sn += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(found_cu > 0, "copper generates");
    assert!(found_sn > 0, "tin generates");
    assert!(found_cu > found_sn, "copper more common than tin");
}

#[test]
fn wild_food_generates_per_biome() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let has = |biome: Biome, blocks: &[&str], name: &str| -> bool {
        // Countries are large now; sample a few of them before
        // concluding a biome grows nothing.
        let anchors = find_biomes(&g, biome, 3);
        let ids: Vec<_> = blocks.iter().filter_map(|n| reg.block_id(n)).collect();
        for (ai, anchor) in anchors.into_iter().enumerate() {
            let cp = ChunkPos::from_surface(anchor);
            let mut world = World::new(42, tmp_dir(&format!("{name}{ai}")), reg.clone());
            for du in -4..=4 {
                for dv in -4..=4 {
                    let chunk = cp.offset(du, dv);
                    world.ensure_chunk(chunk);
                    for lx in 0..16u16 {
                        for lz in 0..16u16 {
                            let pos = crate::planet::SurfacePos::new(
                                chunk.face(),
                                chunk.u() * 16 + lx,
                                chunk.v() * 16 + lz,
                            )
                            .unwrap();
                            for y in 64..140 {
                                if ids.contains(&block_at(&world, pos, y)) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    };
    assert!(
        has(Biome::Plains, &["base:wheat_seeds/stage2"], "ww"),
        "plains wheat"
    );
    assert!(
        has(
            Biome::Forest,
            &["base:carrot_crop/stage1", "base:berry_bush/stage1"],
            "wf"
        ),
        "forest carrots/berries"
    );
    assert!(
        has(
            Biome::Taiga,
            &["base:potato_crop/stage1", "base:wild_mushroom"],
            "wt"
        ),
        "taiga potato/mushroom"
    );
}

#[test]
#[ignore]
fn print_biome_locations() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    for biome in [
        Biome::Forest,
        Biome::Plains,
        Biome::Desert,
        Biome::Jungle,
        Biome::Scrubland,
        Biome::Taiga,
        Biome::Arctic,
        Biome::Mountains,
    ] {
        let pos = find_biome(&g, biome).unwrap();
        println!("{biome:?}: {},{},{}", pos.face().name(), pos.u(), pos.v());
    }
}

#[test]
fn iron_ore_generates_in_band() {
    let reg = base_reg();
    let mut w = test_world("ironband");
    let ore = reg.block_id("base:iron_ore").unwrap();
    let mut found = 0;
    let mut out_of_band = 0;
    for x in -32..32 {
        for z in -32..32 {
            w.ensure_chunk(ChunkPos::of_world(x * 4, z * 4));
        }
    }
    for (_, c) in w.chunks().iter() {
        for (i, b) in c.raw().iter().enumerate() {
            if *b == ore.0 {
                found += 1;
                let y = i % 256;
                // Basement iron (4..48) plus the banded seams in shale
                // (40..62); either kind of vein drifts a little.
                if !(1..=70).contains(&y) {
                    out_of_band += 1;
                }
            }
        }
    }
    assert!(found > 0, "iron generates");
    assert_eq!(out_of_band, 0, "iron stays in its band");
}

#[test]
fn structures_parse_and_place() {
    let reg = base_reg();
    assert_eq!(reg.structures.len(), 6, "six base ruins");
    assert!(reg.loots.contains_key("base:ruin_artifacts"));
    assert!(reg.loots.contains_key("base:ruin_chest"));
    let cellar = reg
        .structures
        .iter()
        .position(|s| s.name == "base:buried_cellar")
        .expect("cellar");
    let mut w = test_world("ruinplace");
    w.place_structure(cellar, 2, 120, 2, 12345);
    let cob = reg.block_id("base:cobblestone").unwrap();
    assert_eq!(w.get_block(2, 121, 2), cob, "cellar wall");
    assert_eq!(w.get_block(4, 121, 4), AIR, "carved interior");
    // The chest exists, is loot-filled, and belongs to the wild.
    let chest = w
        .block_entities()
        .find_map(|(_, e)| match e {
            crate::world::BlockEntity::Chest(c) => Some(c),
            _ => None,
        })
        .expect("ruin chest placed");
    assert!(chest.wild_owned, "the wild keeps its trophies");
    assert!(chest.slots.iter().flatten().count() >= 3, "loot inside");
    // Worn tools from loot arrive worn.
    let mut rng = 99u32;
    let mut saw_worn = false;
    for _ in 0..300 {
        for s in w.roll_loot("base:ruin_artifacts", 1, &mut rng) {
            let max = reg.item(s.item).durability;
            if max > 0 && s.durability < max {
                saw_worn = true;
                assert!(s.durability <= max / 4, "old tools are truly old");
            }
        }
    }
    assert!(saw_worn, "artifact tools roll worn");
}

#[test]
fn ruins_generate_deterministically() {
    let reg = base_reg();
    let mut w = test_world_with("ruingen1", reg.clone());
    let markers = [
        reg.block_id("base:mossy_cobblestone").unwrap(),
        reg.block_id("base:packed_earth").unwrap(),
        reg.block_id("base:cracked_masonry").unwrap(),
    ];
    for cx in -10..10 {
        for cz in -10..10 {
            w.ensure_chunk(tchunk(cx, cz));
        }
    }
    let mut found_at = None;
    'outer: for (pos, c) in w.chunks().iter() {
        for (i, b) in c.raw().iter().enumerate() {
            if markers.contains(&crate::registry::BlockId(*b)) {
                found_at = Some((*pos, i));
                break 'outer;
            }
        }
    }
    let (pos, idx) = found_at.expect("some ruin generated in 400 chunks");
    // Same seed, fresh world: the same block sits in the same place.
    let mut w2 = test_world_with("ruingen2", reg.clone());
    w2.ensure_chunk(pos);
    assert_eq!(
        w2.chunks().get(&pos).unwrap().raw()[idx],
        w.chunks().get(&pos).unwrap().raw()[idx],
        "structure placement is deterministic"
    );
}

#[test]
fn strata_layer_the_world_sanely() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("strata"), reg.clone());
    for x in -3..=3 {
        for z in -3..=3 {
            w.ensure_chunk(tchunk(x, z));
        }
    }
    let b = |n: &str| reg.block_id(n).unwrap();
    let (sandstone, limestone, shale) = (b("base:sandstone"), b("base:limestone"), b("base:shale"));
    let (granite, marble, basalt) = (b("base:granite"), b("base:marble"), b("base:basalt"));

    let mut count = std::collections::HashMap::new();
    let mut y_sum = std::collections::HashMap::new();
    let mut marble_cells = Vec::new();
    for x in -48..48 {
        for z in -48..48 {
            for y in 1..140 {
                let blk = w.get_block(x, y, z);
                *count.entry(blk).or_insert(0u32) += 1;
                *y_sum.entry(blk).or_insert(0i64) += y as i64;
                if blk == marble && marble_cells.len() < 400 {
                    marble_cells.push((x, y, z));
                }
            }
        }
    }
    let n = |blk| count.get(&blk).copied().unwrap_or(0);
    for (name, blk) in [
        ("sandstone", sandstone),
        ("limestone", limestone),
        ("shale", shale),
        ("basalt", basalt),
    ] {
        assert!(n(blk) > 500, "{name} present in the sample ({})", n(blk));
    }
    // Granite is regional: locate it over the complete finite address space,
    // then inspect that face rather than asking the retired planar probe.
    let mut pluton = None;
    'pluton: for face in crate::planet::Face::ALL {
        for u in (64..crate::planet::FACE_BLOCKS).step_by(96) {
            for v in (64..crate::planet::FACE_BLOCKS).step_by(96) {
                let pos = crate::planet::SurfacePos::new(face, u, v).unwrap();
                if w.generator.pluton_at_surface(pos) {
                    pluton = Some(pos);
                    break 'pluton;
                }
            }
        }
    }
    let pluton = pluton.expect("a batholith on the finite planet");
    let gp = ChunkPos::from_surface(pluton);
    for du in -2..=2 {
        for dv in -2..=2 {
            w.ensure_chunk(gp.offset(du, dv));
        }
    }
    let mut n_granite = 0u32;
    let mut contact_marble = Vec::new();
    for du in -32..32 {
        for dv in -32..32 {
            let pos = surface_offset(pluton, du, dv);
            for y in 1..140 {
                let blk = block_at(&w, pos, y);
                if blk == granite {
                    n_granite += 1;
                }
                if blk == marble && contact_marble.len() < 400 {
                    contact_marble.push((pos, y));
                }
            }
        }
    }
    assert!(
        n_granite > 500,
        "granite present in its province ({n_granite})"
    );
    // Basalt floods only the deeps.
    let mean = |blk| y_sum.get(&blk).copied().unwrap_or(0) as f64 / n(blk).max(1) as f64;
    assert!(
        mean(basalt) < 14.0,
        "basalt is a deep layer ({})",
        mean(basalt)
    );
    // The sedimentary stack is ordered: shale under limestone under
    // sandstone.
    assert!(
        mean(shale) < mean(limestone) && mean(limestone) < mean(sandstone),
        "bedding order holds: {:.1} < {:.1} < {:.1}",
        mean(shale),
        mean(limestone),
        mean(sandstone)
    );
    // Marble is contact rock: granite bakes it, so granite is near.
    assert!(!contact_marble.is_empty(), "contact marble exists");
    let mut hits = 0;
    let sample: Vec<_> = contact_marble.iter().step_by(7).take(30).collect();
    for &&(surface, y) in &sample {
        let mut near = false;
        'scan: for du in -16i32..=16 {
            for dy in -16i32..=16 {
                for dv in -16i32..=16 {
                    let pos = surface_offset(surface, du, dv);
                    if block_at(&w, pos, y + dy) == granite {
                        near = true;
                        break 'scan;
                    }
                }
            }
        }
        if near {
            hits += 1;
        }
    }
    assert!(
        hits * 10 >= sample.len() * 8,
        "marble hugs granite ({hits}/{} within 16 blocks)",
        sample.len()
    );
}

#[test]
fn temporary_planet_gen_does_not_stamp_planar_volcano_regions() {
    let reg = base_reg();
    let generator = Generator::new(42, &reg);
    // Goal 1 deliberately retains caves/ores/strata but not landmarks whose
    // region addressing was planar. The scientific spherical volcano pass is
    // a later goal; silently stamping the old grid onto each face would make
    // six visible square patterns and seam discontinuities.
    for face in crate::planet::Face::ALL {
        let pos = crate::planet::SurfacePos::new(
            face,
            crate::planet::FACE_BLOCKS / 2,
            crate::planet::FACE_BLOCKS / 2,
        )
        .unwrap();
        assert!(
            generator.prospect_at(pos).volcano.is_none(),
            "temporary generator must not advertise a planar volcano on {face:?}"
        );
    }
}

#[test]
fn pipes_and_geodes_seed_the_deep() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("pipes"), reg.clone());
    let b = |n: &str| reg.block_id(n).unwrap();

    // A kimberlite pipe, found by the locator, generated, and shaped
    // like a carrot: wide near its top, a thread at depth.
    // Treasure-band rarity: the locator is cheap (a hash), so the
    // search square is simply large now.
    let mut pipe = None;
    'p: for face in crate::planet::Face::ALL {
        for u in 0..crate::planet::FACE_CHUNKS {
            for v in 0..crate::planet::FACE_CHUNKS {
                let cp = ChunkPos::new(face, u, v).unwrap();
                if w.generator.pipe_at(cp).is_some() {
                    pipe = Some(cp);
                    break 'p;
                }
            }
        }
    }
    let cp = pipe.expect("a pipe on the finite planet");
    w.ensure_chunk(cp);
    let kim = b("base:kimberlite");
    let count_at = |w: &World, y: i32| -> i32 {
        let mut n = 0;
        for lx in 0..16u16 {
            for lz in 0..16u16 {
                let pos =
                    crate::planet::SurfacePos::new(cp.face(), cp.u() * 16 + lx, cp.v() * 16 + lz)
                        .unwrap();
                if block_at(w, pos, y) == kim {
                    n += 1;
                }
            }
        }
        n
    };
    let total: i32 = (2..200).map(|y| count_at(&w, y)).sum();
    assert!(total > 80, "the pipe has body ({total} cells)");
    let deep = count_at(&w, 8);
    let shallow_y = (2..200).rev().find(|&y| count_at(&w, y) > 0).unwrap();
    let shallow = count_at(&w, shallow_y - 4);
    assert!(
        deep <= shallow,
        "carrot profile: {deep} at depth vs {shallow} near the top"
    );

    // A geode: quartz shell, amethyst lining, hollow heart.
    let mut placed = false;
    let mut tried = 0;
    'g: for face in crate::planet::Face::ALL {
        for u in 0..crate::planet::FACE_CHUNKS {
            for v in 0..crate::planet::FACE_CHUNKS {
                let cp = ChunkPos::new(face, u, v).unwrap();
                if w.generator.geode_at(cp).is_none() {
                    continue;
                }
                tried += 1;
                if tried > 14 {
                    break 'g;
                }
                w.ensure_chunk(cp);
                let mut amethyst = 0;
                let mut quartz = 0;
                for lx in 0..16u16 {
                    for lz in 0..16u16 {
                        let pos = crate::planet::SurfacePos::new(
                            cp.face(),
                            cp.u() * 16 + lx,
                            cp.v() * 16 + lz,
                        )
                        .unwrap();
                        for y in 40..80 {
                            let block = block_at(&w, pos, y);
                            if block == b("base:amethyst_block") {
                                amethyst += 1;
                            } else if block == b("base:quartz_block") {
                                quartz += 1;
                            }
                        }
                    }
                }
                if amethyst > 4 && quartz > 8 {
                    placed = true;
                    break 'g;
                }
            }
        }
    }
    assert!(placed, "a geode placed in limestone country");
}

#[test]
fn ores_stay_in_their_host_rocks() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("hosts"), reg.clone());
    for x in -4..=4 {
        for z in -4..=4 {
            w.ensure_chunk(tchunk(x, z));
        }
    }
    let b = |n: &str| reg.block_id(n).unwrap();
    let coal = b("base:coal_ore");
    let gold = b("base:gold_quartz");
    let quartz = b("base:quartz_vein");
    let diamond = b("base:diamond_ore");
    let shale = b("base:shale");
    let slate = b("base:slate");
    let kim = b("base:kimberlite");
    let mut coal_n = 0;
    let mut quartz_n = 0;
    let (mut coal_hosted, mut gold_neighbors_quartz) = (0, 0);
    let mut gold_n = 0;
    for x in -72..72 {
        for z in -72..72 {
            for y in 1..90 {
                let blk = w.get_block(x, y, z);
                if blk == coal {
                    coal_n += 1;
                    // A coal cell should sit in shale country: some
                    // neighbor is shale (or its cooked twin).
                    let hosted = [
                        (1, 0, 0),
                        (-1, 0, 0),
                        (0, 1, 0),
                        (0, -1, 0),
                        (0, 0, 1),
                        (0, 0, -1),
                    ]
                    .iter()
                    .any(|&(dx, dy, dz)| {
                        let n = w.get_block(x + dx, y + dy, z + dz);
                        n == shale || n == slate || n == coal
                    });
                    if hosted {
                        coal_hosted += 1;
                    }
                } else if blk == quartz {
                    quartz_n += 1;
                } else if blk == gold {
                    gold_n += 1;
                    let near = [
                        (1, 0, 0),
                        (-1, 0, 0),
                        (0, 1, 0),
                        (0, -1, 0),
                        (0, 0, 1),
                        (0, 0, -1),
                    ]
                    .iter()
                    .any(|&(dx, dy, dz)| {
                        let n = w.get_block(x + dx, y + dy, z + dz);
                        n == quartz || n == gold
                    });
                    if near {
                        gold_neighbors_quartz += 1;
                    }
                } else if blk == diamond {
                    // Diamonds only ever sit inside kimberlite.
                    let near_kim = [
                        (1, 0, 0),
                        (-1, 0, 0),
                        (0, 1, 0),
                        (0, -1, 0),
                        (0, 0, 1),
                        (0, 0, -1),
                    ]
                    .iter()
                    .any(|&(dx, dy, dz)| {
                        let n = w.get_block(x + dx, y + dy, z + dz);
                        n == kim || n == diamond
                    });
                    assert!(near_kim, "diamond outside kimberlite at ({x},{y},{z})");
                }
            }
        }
    }
    assert!(coal_n > 30, "coal seams exist ({coal_n})");
    assert!(quartz_n > 20, "quartz veins exist ({quartz_n})");
    assert!(
        coal_hosted * 10 >= coal_n * 8,
        "coal keeps shale company ({coal_hosted}/{coal_n})"
    );
    if gold_n > 0 {
        assert!(
            gold_neighbors_quartz * 10 >= gold_n * 7,
            "gold stays in its veins ({gold_neighbors_quartz}/{gold_n})"
        );
    }
}

#[test]
fn rivers_lakes_and_magma_chambers() {
    let reg = base_reg();
    let mut world = World::new(42, tmp_dir("hydro"), reg.clone());
    let candidates = find_water_features(&world.generator, 4);
    assert!(!candidates.is_empty(), "rivers or lakes above the sea");
    let mut water_cells = 0;
    for (center, fill) in &candidates {
        let cp = ChunkPos::from_surface(*center);
        for du in -1..=1 {
            for dv in -1..=1 {
                world.ensure_chunk(cp.offset(du, dv));
            }
        }
        for du in -6..=6 {
            for dv in -6..=6 {
                let pos = surface_offset(*center, du, dv);
                for y in crate::chunk::SEA_LEVEL + 2..=*fill + 2 {
                    if reg.is_water(block_at(&world, pos, y)) {
                        water_cells += 1;
                    }
                }
            }
        }
        if water_cells > 4 {
            break;
        }
    }
    assert!(
        water_cells > 4,
        "fresh water fills the carve ({water_cells})"
    );

    // Goal 1 keeps seam-safe deep magma pockets even though the old planar
    // volcano landmark pass is intentionally absent.
    let mut magma = 0;
    for (center, _) in candidates {
        let cp = ChunkPos::from_surface(center);
        for du in -2..=2 {
            for dv in -2..=2 {
                let chunk = cp.offset(du, dv);
                world.ensure_chunk(chunk);
                for lx in 0..16u16 {
                    for lz in 0..16u16 {
                        let pos = crate::planet::SurfacePos::new(
                            chunk.face(),
                            chunk.u() * 16 + lx,
                            chunk.v() * 16 + lz,
                        )
                        .unwrap();
                        magma += (2..12)
                            .filter(|&y| reg.is_lava(block_at(&world, pos, y)))
                            .count();
                    }
                }
            }
        }
    }
    assert!(magma > 0, "deep planetary magma pockets exist ({magma})");
}

/// Dev tooling, not a check: prints where to find each biome for a
/// given seed (screenshot framing). Run with:
/// cargo test print_biome_atlas -- --ignored --nocapture
#[test]
#[ignore]
fn print_biome_atlas() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    for biome in [
        Biome::Jungle,
        Biome::Swamp,
        Biome::Savanna,
        Biome::Tundra,
        Biome::Badlands,
        Biome::Mountains,
    ] {
        if let Some(pos) = find_biome(&g, biome) {
            println!(
                "{}: ({},{},{}) est {}",
                biome.name(),
                pos.face().name(),
                pos.u(),
                pos.v(),
                g.surface_estimate_at(pos)
            );
        }
    }
}

#[test]
fn rivers_settle_instead_of_churning() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("riversettle"), reg.clone());
    // Find a river above the sea and load a 3x3 of chunks around it —
    // ensure_chunk's seam wake is exactly what set real rivers off.
    let mut wet = None;
    'r: for r in 1..400 {
        let d = r * 16;
        for (x, z) in [(d, 0), (-d, 0), (0, d), (0, -d), (d, d), (-d, -d)] {
            if let Some(fill) = w.generator.water_features(x, z)
                && fill > crate::chunk::SEA_LEVEL + 3
            {
                wet = Some((x, z));
                break 'r;
            }
        }
    }
    let (x, z) = wet.expect("a river above the sea");
    let cp = ChunkPos::of_world(x, z);
    for dx in -1..=1 {
        for dz in -1..=1 {
            w.ensure_chunk(tchunk(cp.centered_u() + dx, cp.centered_v() + dz));
        }
    }
    // Terraced reaches shed a little water at their lips, then rest.
    // Before the fix this loop never went quiet.
    let mut quiet = false;
    for _ in 0..300 {
        if !w.tick_water(10_000) {
            quiet = true;
            break;
        }
    }
    assert!(quiet, "the river settles instead of churning forever");
}

/// Dev tooling: coarse load-path timing. Run with:
/// cargo test --release bench_load_path -- --ignored --nocapture
#[test]
#[ignore]
fn bench_load_path() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let t0 = std::time::Instant::now();
    for i in 0..50 {
        let _ = g.generate(tchunk(i, -i), &reg);
    }
    let gen_dt = t0.elapsed();
    let mut w = World::new(42, tmp_dir("bench"), reg.clone());
    let t1 = std::time::Instant::now();
    for i in 0..50 {
        w.ensure_chunk(tchunk(i, -i));
    }
    let ensure = t1.elapsed();
    println!(
        "generate: {:?}/chunk   ensure (gen+light+seams): {:?}/chunk",
        gen_dt / 50,
        ensure / 50
    );
}

#[test]
fn adopted_worker_chunks_match_ensure() {
    let reg = base_reg();
    let mut a = World::new(42, tmp_dir("adopt-a"), reg.clone());
    let mut b2 = World::new(42, tmp_dir("adopt-b"), reg.clone());
    let pos = tchunk(3, -2);
    a.ensure_chunk(pos);
    // Generation is pure: a worker's chunk equals the sync path.
    let chunk = b2.generator.generate(pos, &reg);
    assert!(b2.adopt_generated(pos, chunk));
    assert_eq!(a.chunks()[&pos].raw(), b2.chunks()[&pos].raw());
    // A saved copy on disk beats the worker's fresh terrain.
    let stone = b(&reg, "base:stone");
    a.set_block(3 * 16 + 4, 200, -2 * 16 + 4, stone);
    save_world(&mut a);
    let mut c = World::load_or_create(a.save_dir_for_test(), reg.clone()).unwrap();
    let fresh = c.generator.generate(pos, &reg);
    assert!(c.adopt_generated(pos, fresh));
    assert_eq!(
        c.get_block(3 * 16 + 4, 200, -2 * 16 + 4),
        stone,
        "disk wins over the worker"
    );
}

/// Dev tooling: settled-world per-tick sim cost, mob-heavy.
/// cargo test --release bench_sim_tick -- --ignored --nocapture
#[test]
#[ignore]
fn bench_sim_tick() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("simbench"), reg.clone());
    for x in -6..=6 {
        for z in -6..=6 {
            w.ensure_chunk(tchunk(x, z));
        }
    }
    println!("mobs after seeding 169 chunks: {}", w.mob_count());
    let mut srv = crate::server::Server::new(w, 0.3, 7);
    let players = [crate::server::PlayerCtx {
        id: 0,
        pos: ep(glam::Vec3::new(8.0, 80.0, 8.0)),
        spawn: ep(glam::Vec3::new(8.0, 80.0, 8.0)),
        attackable: true,
        aggro_mod: 0.0,
    }];
    let mut evs = Vec::new();
    // Warm up, then time 300 fixed ticks.
    for _ in 0..30 {
        srv.advance(1.0 / 30.0, &players, &mut evs);
        evs.clear();
    }
    let t0 = std::time::Instant::now();
    for _ in 0..300 {
        srv.advance(1.0 / 30.0, &players, &mut evs);
        evs.clear();
    }
    let per_tick = t0.elapsed() / 300;
    // Isolate mobs: time tick_mobs alone.
    let mut rng = 5u32;
    let t1 = std::time::Instant::now();
    for _ in 0..300 {
        srv.world.tick_mobs(&players, 1.0, 1.0 / 30.0, &mut rng);
    }
    let mobs_only = t1.elapsed() / 300;
    println!(
        "advance: {per_tick:?}/tick   tick_mobs alone: {mobs_only:?}/tick   mobs: {}",
        srv.world.mob_count()
    );
}

/// Dev tooling: which part of the tick is eating the frame.
/// cargo test --release bench_tick_parts -- --ignored --nocapture
#[test]
#[ignore]
fn bench_tick_parts() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("tickparts"), reg.clone());
    for x in -6..=6 {
        for z in -6..=6 {
            w.ensure_chunk(tchunk(x, z));
        }
    }
    let mut srv = crate::server::Server::new(w, 0.3, 7);
    let players = [crate::server::PlayerCtx {
        id: 0,
        pos: ep(glam::Vec3::new(8.0, 80.0, 8.0)),
        spawn: ep(glam::Vec3::new(8.0, 80.0, 8.0)),
        attackable: true,
        aggro_mod: 0.0,
    }];
    let mut evs = Vec::new();
    for _ in 0..30 {
        srv.advance(1.0 / 30.0, &players, &mut evs);
        evs.clear();
    }
    let time = |label: &str, f: &mut dyn FnMut()| {
        let t = std::time::Instant::now();
        for _ in 0..100 {
            f();
        }
        println!("{label}: {:?}", t.elapsed() / 100);
    };
    let mut rng = 5u32;
    time("tick_water(512)", &mut || {
        srv.world.tick_water(512);
    });
    time("tick_lava(256)", &mut || {
        srv.world.tick_lava(256);
    });
    time("tick_entities", &mut || {
        srv.world.tick_entities(1.0 / 30.0);
    });
    time("tick_falling", &mut || {
        srv.world.tick_falling(1.0 / 30.0);
    });
    time("random_tick", &mut || {
        srv.world.random_tick(&mut rng);
    });
    time("hostile_spawns", &mut || {
        srv.world
            .tick_hostile_spawns(players[0].pos, players[0].spawn, 1.0, 1.0 / 30.0, &mut rng);
    });
    time("full advance", &mut || {
        srv.advance(1.0 / 30.0, &players, &mut evs);
        evs.clear();
    });
}

#[test]
fn river_pools_are_sealed() {
    let reg = base_reg();
    let mut world = World::new(42, tmp_dir("weirs"), reg.clone());
    let candidates = find_water_features(&world.generator, 4);
    assert!(!candidates.is_empty(), "rivers above the sea exist");
    let (mut water_cells, mut exposed) = (0, 0);
    for (center, _) in candidates {
        let cp = ChunkPos::from_surface(center);
        for du in -1..=1 {
            for dv in -1..=1 {
                world.ensure_chunk(cp.offset(du, dv));
            }
        }
        for du in -10..=10 {
            for dv in -10..=10 {
                let pos = surface_offset(center, du, dv);
                for y in crate::chunk::SEA_LEVEL + 2..200 {
                    if !reg.is_water(block_at(&world, pos, y)) {
                        continue;
                    }
                    water_cells += 1;
                    for neighbor in crate::planet::neighbors4(pos) {
                        if block_at(&world, neighbor, y) == AIR {
                            exposed += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(water_cells > 20, "pools hold water ({water_cells})");
    // Sealed by construction: worldgen river water never meets air
    // side-on (3D-density undercuts allow a rare stray, nothing more).
    assert!(
        exposed * 50 <= water_cells,
        "pools are sealed: {exposed} exposed of {water_cells}"
    );
}

#[test]
#[ignore]
fn print_weir_spots() {
    let reg = base_reg();
    let w = World::new(42, tmp_dir("weirspots"), reg);
    for r in 1..400 {
        let d = r * 16;
        for (x, z) in [(d, 0), (-d, 0), (0, d), (0, -d), (d, d), (-d, -d)] {
            let Some(f) = w.generator.water_features(x, z) else {
                continue;
            };
            if f <= crate::chunk::SEA_LEVEL + 6 {
                continue;
            }
            let mut weirs = 0;
            for dx in -12i32..=12 {
                for dz in -12i32..=12 {
                    if w.generator.armor_at(x + dx, z + dz).is_some() {
                        weirs += 1;
                    }
                }
            }
            if weirs > 8 {
                println!("river fill {f} at ({x},{z}) with {weirs} armor cols nearby");
            }
        }
    }
}

#[test]
#[ignore]
fn print_river_map() {
    let reg = base_reg();
    let w = World::new(42, tmp_dir("rivermap"), reg);
    for z in (-200i32..=-80).step_by(2) {
        let mut row = String::new();
        for x in -60i32..=60 {
            let f = w.generator.water_features(x, z);
            let a = w.generator.armor_at(x, z);
            let est = w.generator.surface_estimate(x, z);
            row.push(match (f, a) {
                (Some(l), _) if l > est => 'W',
                (Some(_), _) => 'b',
                (None, Some(_)) => '#',
                _ => '.',
            });
        }
        println!("z={z:>5} {row}");
    }
}

#[test]
fn lake_terraces_settle_sealed() {
    // A broad terraced lake (two quantized pool levels and an armor
    // dam between them). Before sealing, waking it shed sheets of
    // partial water over the shores forever.
    let reg = base_reg();
    let mut world = World::new(42, tmp_dir("lakesettle"), reg.clone());
    let (center, _) = find_water_features(&world.generator, 1)
        .into_iter()
        .next()
        .expect("a planetary lake or river terrace");
    let cp = ChunkPos::from_surface(center);
    for du in -2..=2 {
        for dv in -2..=2 {
            world.ensure_chunk(cp.offset(du, dv));
        }
    }
    let mut quiet = false;
    for _ in 0..400 {
        if !world.tick_water(10_000) {
            quiet = true;
            break;
        }
    }
    assert!(quiet, "the lake settles instead of churning");
    // Once settled, the waterline still may not hang in the open: no
    // water cell should sit beside same-height air (films/shelves).
    let (mut cells, mut exposed) = (0, 0);
    for du in -30..30 {
        for dv in -30..30 {
            let pos = surface_offset(center, du, dv);
            for y in crate::chunk::SEA_LEVEL + 2..140 {
                if !reg.is_water(block_at(&world, pos, y)) {
                    continue;
                }
                cells += 1;
                for neighbor in crate::planet::neighbors4(pos) {
                    if block_at(&world, neighbor, y) == AIR {
                        exposed += 1;
                    }
                }
            }
        }
    }
    assert!(
        cells > 0,
        "the generated terrace still holds water ({cells})"
    );
    assert!(
        exposed * 50 <= cells,
        "settled pools stay sealed: {exposed} exposed of {cells}"
    );
}

#[test]
#[ignore]
fn print_lake_transect() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("laketransect"), reg.clone());
    let cp = ChunkPos::of_world(-52, -158);
    for dx in -2..=2 {
        for dz in -2..=2 {
            w.ensure_chunk(tchunk(cp.centered_u() + dx, cp.centered_v() + dz));
        }
    }
    for x in -52..-10 {
        let z = -158;
        for y in (70..86).rev() {
            let b = w.get_block(x, y, z);
            if b != AIR {
                println!("({x},{y},{z}) {}", reg.block(b).name);
                break;
            }
        }
    }
}

#[test]
fn wild_food_and_game_are_scarce() {
    // The forage economy: wild food arrives in occasional patches and
    // wildlife in occasional small groups — enough to survive on
    // while traveling, nowhere near enough to skip agriculture. This
    // pins the density band both ways.
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("scarcity"), reg.clone());
    let food: Vec<crate::registry::BlockId> = [
        "base:wheat_seeds/stage2",
        "base:carrot_crop/stage1",
        "base:berry_bush/stage1",
        "base:potato_crop/stage1",
        "base:wild_mushroom",
        "base:jungle_bush/stage1",
    ]
    .iter()
    .filter_map(|n| reg.block_id(n))
    .collect();
    let mut plants = 0;
    for cx in -5..5 {
        for cz in -5..5 {
            w.ensure_chunk(tchunk(cx, cz));
            for lx in 0..16 {
                for lz in 0..16 {
                    for y in 60..150 {
                        if food.contains(&w.get_block(cx * 16 + lx, y, cz * 16 + lz)) {
                            plants += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(plants >= 4, "forage patches exist ({plants} plants)");
    assert!(
        plants <= 90,
        "wild food stays scarce over 100 chunks ({plants} plants)"
    );
    let animals = w.mob_count();
    // The roster now spans sky, ground, water, and cave; per-species
    // density is unchanged and any one walk still crosses mostly
    // empty country.
    assert!(
        animals <= 80,
        "wildlife stays sparse over 100 chunks ({animals})"
    );
}

/// Dev tooling: how uneven is the geology at play scale? For a spread
/// of land points, the distance to the nearest regional resource:
/// plutons (tin, pitchblende), volcanoes (carbonatite, obsidian),
/// kimberlite pipes (diamonds; breached = surface-findable), geodes,
/// and desert sand (monazite). Shale/limestone-hosted ores (iron,
/// coal, galena) and stone/basalt hosts (copper, quartz) are banded
/// by DEPTH under every column, so distance is not their cost.
/// Run: cargo test --release print_resource_census -- --ignored --nocapture
#[test]
#[ignore]
fn print_resource_census() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let ring_dist = |hit: &mut dyn FnMut(i32, i32) -> bool,
                     x0: i32,
                     z0: i32,
                     step: i32,
                     cap: i32|
     -> Option<i32> {
        if hit(x0, z0) {
            return Some(0);
        }
        let mut r = step;
        while r <= cap {
            let mut i = -r;
            while i <= r {
                for (px, pz) in [
                    (x0 + i, z0 - r),
                    (x0 + i, z0 + r),
                    (x0 - r, z0 + i),
                    (x0 + r, z0 + i),
                ] {
                    if hit(px, pz) {
                        return Some(r);
                    }
                }
                i += step;
            }
            r += step;
        }
        None
    };
    // ~30 land points on a spiral out to ~24k blocks.
    let mut pts: Vec<(i32, i32)> = Vec::new();
    let mut i = 0;
    while pts.len() < 30 && i < 400 {
        i += 1;
        let r = 700.0 + 60.0 * i as f64;
        let a = i as f64 * 2.399963;
        let (x, z) = ((r * a.cos()) as i32, (r * a.sin()) as i32);
        if g.surface_estimate(x, z) > crate::chunk::SEA_LEVEL + 2 {
            pts.push((x, z));
        }
    }
    println!("census over {} land points:", pts.len());
    let report = |name: &str, ds: Vec<Option<i32>>| {
        let mut found: Vec<i32> = ds.iter().flatten().copied().collect();
        found.sort_unstable();
        let miss = ds.len() - found.len();
        if found.is_empty() {
            println!("{name:>10}: none found from any point");
            return;
        }
        println!(
            "{name:>10}: median {:>5}  p90 {:>5}  max {:>5}  (beyond cap: {miss})",
            found[found.len() / 2],
            found[(found.len() * 9 / 10).min(found.len() - 1)],
            found[found.len() - 1]
        );
    };
    report(
        "pluton",
        pts.iter()
            .map(|&(x, z)| ring_dist(&mut |px, pz| g.pluton_at(px, pz), x, z, 32, 4000))
            .collect(),
    );
    report(
        "volcano",
        pts.iter()
            .map(|&(x, z)| {
                ring_dist(
                    &mut |px, pz| g.volcano_near(px, pz).is_some(),
                    x,
                    z,
                    64,
                    6000,
                )
            })
            .collect(),
    );
    report(
        "pipe",
        pts.iter()
            .map(|&(x, z)| {
                let cp = ChunkPos::of_world(x, z);
                ring_dist(
                    &mut |cx, cz| g.pipe_at(tchunk(cx, cz)).is_some(),
                    cp.centered_u(),
                    cp.centered_v(),
                    1,
                    400,
                )
                .map(|c| c * 16)
            })
            .collect(),
    );
    report(
        "pipe-open",
        pts.iter()
            .map(|&(x, z)| {
                let cp = ChunkPos::of_world(x, z);
                ring_dist(
                    &mut |cx, cz| g.pipe_at(tchunk(cx, cz)).is_some_and(|(_, _, b)| b),
                    cp.centered_u(),
                    cp.centered_v(),
                    1,
                    120,
                )
                .map(|c| c * 16)
            })
            .collect(),
    );
    report(
        "geode",
        pts.iter()
            .map(|&(x, z)| {
                let cp = ChunkPos::of_world(x, z);
                ring_dist(
                    &mut |cx, cz| g.geode_at(tchunk(cx, cz)).is_some(),
                    cp.centered_u(),
                    cp.centered_v(),
                    1,
                    60,
                )
                .map(|c| c * 16)
            })
            .collect(),
    );
    report(
        "desert",
        pts.iter()
            .map(|&(x, z)| {
                ring_dist(
                    &mut |px, pz| g.biome(px, pz) == Biome::Desert,
                    x,
                    z,
                    128,
                    9000,
                )
            })
            .collect(),
    );
}

#[test]
fn regional_resources_hold_their_distance_bands() {
    // Economy plan stage 1: commons under every column; regionals a
    // real hike; treasures an expedition. Locator-based so it stays
    // fast — this is the geological counterpart of the food census.
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let mut pipes = 0usize;
    let mut geodes = 0usize;
    let mut sampled = 0usize;
    for face in crate::planet::Face::ALL {
        for u in 0..crate::planet::FACE_CHUNKS {
            for v in 0..crate::planet::FACE_CHUNKS {
                let chunk = ChunkPos::new(face, u, v).unwrap();
                pipes += usize::from(g.pipe_at(chunk).is_some());
                geodes += usize::from(g.geode_at(chunk).is_some());
                sampled += 1;
            }
        }
    }
    assert!(
        (6..=35).contains(&pipes),
        "pipes stay treasure-band over {sampled} chunks ({pipes})"
    );
    assert!(
        (1200..=3400).contains(&geodes),
        "geodes stay a local luxury over {sampled} chunks ({geodes})"
    );
    let mut plutons = 0usize;
    let mut columns = 0usize;
    for face in crate::planet::Face::ALL {
        for u in (32..crate::planet::FACE_BLOCKS).step_by(128) {
            for v in (32..crate::planet::FACE_BLOCKS).step_by(128) {
                let pos = crate::planet::SurfacePos::new(face, u, v).unwrap();
                plutons += usize::from(g.pluton_at_surface(pos));
                columns += 1;
            }
        }
    }
    assert!(
        plutons > 5 && plutons * 2 < columns,
        "batholiths stay regional ({plutons}/{columns} sampled columns)"
    );
}

#[test]
fn prospect_readings_reveal_the_country() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    // Stand a known distance from a located pipe: the reading points
    // at it. (Locator first, then read from 8 chunks west of it.)
    let mut pipe = None;
    'p: for face in crate::planet::Face::ALL {
        for u in 0..crate::planet::FACE_CHUNKS {
            for v in 0..crate::planet::FACE_CHUNKS {
                let chunk = ChunkPos::new(face, u, v).unwrap();
                if g.pipe_at(chunk).is_some() {
                    pipe = Some(chunk);
                    break 'p;
                }
            }
        }
    }
    let cp = pipe.expect("a pipe in range");
    let strike_chunk = cp.offset(-8, 0);
    let strike = crate::planet::SurfacePos::new(
        strike_chunk.face(),
        strike_chunk.u() * 16 + 8,
        strike_chunk.v() * 16 + 8,
    )
    .unwrap();
    let r = g.prospect_at(strike);
    let hit = r.pipe.expect("the pick smells blue ground");
    assert!(
        (96..=192).contains(&hit.distance),
        "8 chunks out reads ~128 ({})",
        hit.distance
    );
    assert!(
        hit.bearing.is_some_and(|bearing| bearing.sin() > 0.0),
        "the pipe lies east of the strike ({:?})",
        hit.bearing
    );
    // Standing inside a batholith reads "underfoot"; the same pick
    // far outside one reads a bearing or nothing. Deterministic.
    let mut inside = None;
    'g: for face in crate::planet::Face::ALL {
        for u in (32..crate::planet::FACE_BLOCKS).step_by(64) {
            for v in (32..crate::planet::FACE_BLOCKS).step_by(64) {
                let pos = crate::planet::SurfacePos::new(face, u, v).unwrap();
                if g.pluton_at_surface(pos) {
                    inside = Some(pos);
                    break 'g;
                }
            }
        }
    }
    let inside = inside.expect("a batholith in range");
    assert_eq!(
        g.prospect_at(inside).pluton.map(|hit| hit.distance),
        Some(0),
        "underfoot"
    );
    let again = g.prospect_at(strike);
    assert_eq!(again.pipe, r.pipe, "readings are pure functions");
}

#[test]
fn the_waterline_grows_its_own() {
    // Cattails at the margins, kelp in the deeps — present, scarce.
    let reg = base_reg();
    let generator = Generator::new(42, &reg);
    let anchor = find_biome_where(&generator, Biome::Ocean, |_| true)
        .expect("ocean country for flora census");
    let center = ChunkPos::from_surface(anchor);
    let mut world = World::new(42, tmp_dir("waterflora"), reg.clone());
    let reeds = reg.block_id("base:cattail").unwrap();
    let kelp = reg.block_id("base:kelp_frond").unwrap();
    let lily = reg.block_id("base:water_lily").unwrap();
    let (mut r, mut k, mut l) = (0, 0, 0);
    for du in -5..=5 {
        for dv in -5..=5 {
            let chunk = center.offset(du, dv);
            world.ensure_chunk(chunk);
            for raw in world.chunks()[&chunk].raw() {
                let block = crate::registry::BlockId(raw);
                r += i32::from(block == reeds);
                k += i32::from(block == kelp);
                l += i32::from(block == lily);
            }
        }
    }
    assert!(r + k + l > 0, "the waterline grows something ({r}/{k}/{l})");
    assert!(
        r + k + l < 12000,
        "and it stays vegetation, not carpet ({r}/{k}/{l})"
    );
}

#[test]
fn provinces_make_biomes_into_places() {
    // The patchwork test: a long walk should cross a handful of
    // countries, not dozens. (Before provinces, temperature and
    // humidity turned over every ~385 blocks and every column voted
    // for itself, so a forest, a desert and a taiga could share a
    // few hundred paces.)
    let reg = base_reg();
    let w = World::new(42, tmp_dir("provinces"), reg.clone());
    let g = &w.generator;
    let mut runs = 0;
    let mut prev = None;
    for u in (1096..7096).step_by(25) {
        let pos = crate::planet::SurfacePos::new(crate::planet::Face::PosZ, u, 4096).unwrap();
        let key = g.province_at(pos).key;
        if Some(key) != prev {
            runs += 1;
            prev = Some(key);
        }
    }
    assert!(
        (2..=12).contains(&runs),
        "a 6000-block walk crosses a few countries ({runs})"
    );
    // Outside the border fringe the label is the country's, always —
    // that is the whole fix for the confetti.
    let mut checked = 0;
    let mut same = 0;
    for u in (1096..7096).step_by(7) {
        let pos = crate::planet::SurfacePos::new(crate::planet::Face::PosZ, u, 4096).unwrap();
        let p = g.province_at(pos);
        if p.edge > 90.0
            && g.plate_relief(&g.climate_at(pos)) <= 30.0
            && g.surface_estimate_at(pos) > crate::chunk::SEA_LEVEL
        {
            checked += 1;
            if g.biome_at(pos) == p.biome {
                same += 1;
            }
        }
    }
    assert!(checked > 60, "plenty of dry interior sampled ({checked})");
    assert!(
        same * 10 >= checked * 7,
        "interiors read as their country ({same}/{checked})"
    );
    // But the world is not one monotonous field either.
    let mut seen = std::collections::HashSet::new();
    for face in crate::planet::Face::ALL {
        for u in (128..crate::planet::FACE_BLOCKS).step_by(512) {
            for v in (128..crate::planet::FACE_BLOCKS).step_by(512) {
                let pos = crate::planet::SurfacePos::new(face, u, v).unwrap();
                seen.insert(g.biome_at(pos).name());
            }
        }
    }
    assert!(seen.len() >= 6, "the world still holds variety ({seen:?})");
    // A province is a coherent territory: sampling inside one, well
    // clear of its fringe, gives one answer.
    let anchor = find_biome(g, Biome::Plains).expect("a coherent land province");
    let p = g.province_at(anchor);
    let mut inside = 0;
    let mut agree = 0;
    for du in (-300..=300).step_by(60) {
        for dv in (-300..=300).step_by(60) {
            let pos = surface_offset(p.site, du, dv);
            let q = g.province_at(pos);
            // Terrain keeps its veto inside a country too: a fold
            // range reads as Mountains wherever it rises.
            let vetoed = g.plate_relief(&g.climate_at(pos)) > 30.0
                || g.surface_estimate_at(pos) <= crate::chunk::SEA_LEVEL;
            if q.key == p.key && q.edge > 80.0 && !vetoed {
                inside += 1;
                if g.biome_at(pos) == p.biome {
                    agree += 1;
                }
            }
        }
    }
    assert!(
        inside > 4,
        "the province has an interior ({inside} samples)"
    );
    assert!(
        agree * 10 >= inside * 7,
        "and one biome through most of it ({agree}/{inside}) - terrain \
         keeps its local exceptions, but the country sets the tone"
    );
}

#[test]
fn province_borders_are_organic_and_terrain_still_vetoes() {
    let reg = base_reg();
    let w = World::new(7, tmp_dir("province-edges"), reg.clone());
    let g = &w.generator;
    // Borders are not grid-aligned: walking a straight line, the
    // province changes at irregular offsets, not on a 900 lattice.
    let mut changes = Vec::new();
    let mut prev = g.province(-4000, 12).key;
    for x in -4000..4000 {
        let k = g.province(x, 12).key;
        if k != prev {
            changes.push(x);
            prev = k;
        }
    }
    assert!(changes.len() >= 3, "several borders crossed");
    let offsets: std::collections::HashSet<i32> =
        changes.iter().map(|x| x.rem_euclid(900)).collect();
    assert!(
        offsets.len() > 1,
        "borders sit at varied offsets, not on the lattice ({offsets:?})"
    );
    // Terrain keeps its veto: young fold ranges read as Mountains
    // whatever country they cross.
    let mut mountain_seen = false;
    for x in (-8000..8000).step_by(97) {
        for z in (-8000..8000).step_by(211) {
            let cl = g.climate(x, z);
            if g.plate_relief(&cl) > 30.0 {
                assert_eq!(
                    g.biome(x, z),
                    crate::worldgen::Biome::Mountains,
                    "a fold range is Mountains at {x},{z}"
                );
                mountain_seen = true;
            }
        }
    }
    assert!(mountain_seen, "the sample found a fold range");
}

#[test]
fn dbg_chunkgen_cost() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("bench-gen"), reg.clone());
    let t0 = std::time::Instant::now();
    for cx in 0..6 {
        for cz in 0..6 {
            w.ensure_chunk(tchunk(cx, cz));
        }
    }
    eprintln!("36 chunks in {:?}", t0.elapsed());
}

#[test]
#[ignore = "dev tool: writes a biome map to the scratchpad"]
fn dev_biome_map() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let side = 512usize;
    let step = 16i32; // 8192 blocks across
    let mut px = vec![0u8; side * side * 3];
    for iz in 0..side {
        for ix in 0..side {
            let x = (ix as i32 - side as i32 / 2) * step;
            let z = (iz as i32 - side as i32 / 2) * step;
            let c = match g.biome(x, z) {
                Biome::Forest => [34, 110, 44],
                Biome::Plains => [126, 190, 82],
                Biome::Desert => [226, 206, 128],
                Biome::Jungle => [22, 130, 60],
                Biome::Scrubland => [170, 160, 96],
                Biome::Taiga => [58, 110, 96],
                Biome::Arctic => [236, 240, 246],
                Biome::Mountains => [130, 128, 132],
                Biome::Swamp => [80, 96, 66],
                Biome::Savanna => [198, 176, 84],
                Biome::Tundra => [166, 176, 168],
                Biome::Badlands => [186, 112, 68],
                // The generator labels land; the sea overlay below is
                // what paints water, so this arm never fires here.
                Biome::Ocean => [40, 66, 120],
            };
            let sea = g.surface_estimate(x, z) <= crate::chunk::SEA_LEVEL;
            let c = if sea { [40, 66, 120] } else { c };
            let o = (iz * side + ix) * 3;
            px[o..o + 3].copy_from_slice(&c);
        }
    }
    let mut out = format!("P6\n{side} {side}\n255\n").into_bytes();
    out.extend_from_slice(&px);
    let path = std::env::var("WILDFORGE_MAP_OUT").unwrap_or_else(|_| "/tmp/biomes.ppm".into());
    std::fs::write(path, out).unwrap();
}
