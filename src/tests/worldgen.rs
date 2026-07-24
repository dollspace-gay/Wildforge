//! Terrain, biome, ore, vegetation, and structure generation.

use super::*;

#[test]
fn generation_is_deterministic() {
    let mut a = test_world("det-a");
    let mut b2 = test_world("det-b");
    a.ensure_chunk(ChunkPos { x: 5, z: -3 });
    b2.ensure_chunk(ChunkPos { x: 5, z: -3 });
    assert_eq!(
        a.chunks()[&ChunkPos { x: 5, z: -3 }].raw(),
        b2.chunks()[&ChunkPos { x: 5, z: -3 }].raw()
    );
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
            w.ensure_chunk(ChunkPos { x: cx, z: cz });
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
        let (x, z) = find_biome(&g, biome)
            .unwrap_or_else(|| panic!("{biome:?} not found within search radius"));
        assert_eq!(g.biome(x, z), g2.biome(x, z), "same seed, same biome");
    }
    // Different seeds shuffle the layout.
    let g3 = Generator::new(1337, &reg);
    let mut diff = 0;
    for i in 0..40 {
        let (x, z) = (i * 173, i * -211);
        if g.biome(x, z) != g3.biome(x, z) {
            diff += 1;
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
    // Search rings directly for a solid inland desert column: the
    // plate map makes some deserts coastal or boundary-broken.
    let mut spot = None;
    'scan: for r in 0..400 {
        let d = r * 24;
        for (cx, cz) in [
            (d, 0),
            (-d, 0),
            (0, d),
            (0, -d),
            (d, d),
            (-d, -d),
            (d, -d),
            (-d, d),
        ] {
            if g.biome(cx, cz) == Biome::Desert
                && g.surface_estimate(cx, cz) > crate::chunk::SEA_LEVEL + 8
                && g.tectonics(cx, cz).boundary_dist > 160.0
            {
                spot = Some((cx, cz));
                break 'scan;
            }
        }
    }
    let (x, z) = spot.expect("dry desert column");
    let (w, h) = gen_at(&reg, "desert", x, z);
    assert_eq!(
        w.get_block(x, h, z),
        b(&reg, "base:sand"),
        "desert surface is sand"
    );
    assert_eq!(
        w.get_block(x, h - 2, z),
        b(&reg, "base:sand"),
        "desert subsoil is sand"
    );
    // Cacti generate somewhere in desert chunks (deterministic for seed 42).
    let cactus = b(&reg, "base:cactus");
    let cp = ChunkPos::of_world(x, z);
    let mut w2 = World::new(42, tmp_dir("cacti"), reg.clone());
    let mut found = false;
    'chunks: for dx in -4..=4 {
        for dz in -4..=4 {
            let p = ChunkPos {
                x: cp.x + dx,
                z: cp.z + dz,
            };
            w2.ensure_chunk(p);
            let bx = p.x * 16;
            let bz = p.z * 16;
            for lx in 0..16 {
                for lz in 0..16 {
                    for y in 60..90 {
                        if w2.get_block(bx + lx, y, bz + lz) == cactus {
                            found = true;
                            break 'chunks;
                        }
                    }
                }
            }
        }
    }
    assert!(found, "cacti should generate in deserts");
}

#[test]
fn arctic_has_snow_and_frozen_ocean() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let (x, z) = find_biome(&g, Biome::Arctic).unwrap();
    let mut land = None;
    let mut ocean = None;
    for dx in 0..96 {
        for dz in 0..96 {
            let (cx, cz) = (x + dx, z + dz);
            if g.biome(cx, cz) != Biome::Arctic {
                continue;
            }
            let h = g.surface_estimate(cx, cz);
            if h > crate::chunk::SEA_LEVEL + 1 && land.is_none() {
                land = Some((cx, cz));
            }
            if h < crate::chunk::SEA_LEVEL - 2 && ocean.is_none() {
                ocean = Some((cx, cz));
            }
        }
    }
    if let Some((cx, cz)) = land {
        let (w, h) = gen_at(&reg, "arctic-land", cx, cz);
        assert_eq!(
            w.get_block(cx, h, cz),
            b(&reg, "base:snow"),
            "arctic surface is snow"
        );
    }
    if let Some((cx, cz)) = ocean {
        let (w, _) = gen_at(&reg, "arctic-sea", cx, cz);
        assert_eq!(
            w.get_block(cx, crate::chunk::SEA_LEVEL, cz),
            b(&reg, "base:ice"),
            "arctic ocean surface is ice"
        );
        assert!(
            reg.is_water(w.get_block(cx, crate::chunk::SEA_LEVEL - 1, cz)),
            "water under the ice"
        );
    }
    assert!(
        land.is_some() || ocean.is_some(),
        "found neither arctic land nor ocean"
    );
}

#[test]
fn jungle_denser_than_plains() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    let count_logs = |name: &str, biome: Biome, log_name: &str| -> (u32, u32) {
        let (x, z) = find_biome(&g, biome).unwrap();
        let cp = ChunkPos::of_world(x, z);
        let mut w = World::new(42, tmp_dir(name), reg.clone());
        let log = b(&reg, log_name);
        let mut logs = 0;
        let mut cols = 0;
        for dx in -3..=3 {
            for dz in -3..=3 {
                let p = ChunkPos {
                    x: cp.x + dx,
                    z: cp.z + dz,
                };
                w.ensure_chunk(p);
                for lx in 0..16 {
                    for lz in 0..16 {
                        let (wx, wz) = (p.x * 16 + lx, p.z * 16 + lz);
                        if w.generator.biome(wx, wz) == biome {
                            cols += 1;
                            for y in 60..100 {
                                if w.get_block(wx, y, wz) == log {
                                    logs += 1;
                                    break; // one per column
                                }
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
            w.ensure_chunk(ChunkPos { x: cx, z: cz });
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
    let sample_max = |biome: Biome| -> i32 {
        let mut best = 0;
        let mut n = 0;
        for r in 0..400 {
            let d = r * 16;
            for (x, z) in [(d, 0), (-d, 0), (0, d), (0, -d), (d, d), (-d, -d)] {
                if g.biome(x, z) == biome {
                    best = best.max(g.surface_estimate(x, z));
                    n += 1;
                    if n > 200 {
                        return best;
                    }
                }
            }
        }
        best
    };
    let p = sample_max(Biome::Plains);
    // The young ranges live on convergent continental boundaries now;
    // hunt one through the plate map and measure its crest.
    let mut m = 0;
    'tect: for r in 0..60 {
        let d = r * 128;
        for (x, z) in [
            (d, 0),
            (-d, 0),
            (0, d),
            (0, -d),
            (d, d),
            (-d, -d),
            (d, -d),
            (-d, d),
        ] {
            let tec = g.tectonics(x, z);
            let cl = g.climate(x, z);
            if tec.convergence > 0.25
                && tec.boundary_dist < 60.0
                && !tec.oceanic
                && !tec.neighbor_oceanic
                && cl.c > 0.1
            {
                for dx in -48..=48 {
                    for dz in -48..=48 {
                        m = m.max(g.surface_estimate(x + dx * 2, z + dz * 2));
                    }
                }
                if m > 150 {
                    break 'tect;
                }
            }
        }
    }
    assert!(m > 150, "fold ranges should reach high ({m})");
    assert!(m > p + 30, "ranges ({m}) far above plains ({p})");
}

#[test]
fn oceans_exist_and_fill_with_water() {
    let reg = base_reg();
    let g = Generator::new(42, &reg);
    // Find a deep-ocean column via continentalness.
    let mut spot = None;
    'outer: for r in 0..300 {
        let d = r * 24;
        for (x, z) in [(d, 0), (-d, 0), (0, d), (0, -d), (d, d), (-d, -d)] {
            if g.surface_estimate(x, z) < 46 {
                spot = Some((x, z));
                break 'outer;
            }
        }
    }
    let (x, z) = spot.expect("an ocean should exist");
    let mut w = World::new(42, tmp_dir("ocean"), reg.clone());
    w.ensure_chunk(ChunkPos::of_world(x, z));
    assert!(reg.is_water(w.get_block(x, 63, z)) || w.get_block(x, 63, z) == b(&reg, "base:ice"));
    let floor = w.surface_height(x, z);
    assert!(floor < 62, "ocean floor below sea level ({floor})");
    let fb = w.get_block(x, floor, z);
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
            w.ensure_chunk(ChunkPos { x: cx, z: cz });
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
    // Find a mountain area and generate it.
    let (mx, mz) = {
        let mut best = (0, 0);
        let mut best_h = 0;
        for r in 0..300 {
            let d = r * 16;
            for (x, z) in [(d, 0), (-d, 0), (0, d), (0, -d), (d, d), (-d, -d)] {
                let h = g.surface_estimate(x, z);
                if h > best_h {
                    best_h = h;
                    best = (x, z);
                }
            }
            if best_h > 165 {
                break;
            }
        }
        best
    };
    let mut w = World::new(42, tmp_dir("peaks"), reg.clone());
    let cp = ChunkPos::of_world(mx, mz);
    for dx in -1..=1 {
        for dz in -1..=1 {
            w.ensure_chunk(ChunkPos {
                x: cp.x + dx,
                z: cp.z + dz,
            });
        }
    }
    let snow = b(&reg, "base:snow");
    let stone = b(&reg, "base:stone");
    let (mut snowy, mut stony, mut grassy_high) = (0, 0, 0);
    for lx in 0..16 {
        for lz in 0..16 {
            let (x, z) = (cp.x * 16 + lx, cp.z * 16 + lz);
            let top = w.surface_height(x, z);
            let tb = w.get_block(x, top, z);
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
        let (x, z) = find_biome(&g, biome).unwrap();
        let cp = ChunkPos::of_world(x, z);
        let mut w = World::new(42, tmp_dir(name), reg.clone());
        let want = b(&reg, log);
        let oak = b(&reg, "base:log");
        let (mut hits, mut oaks) = (0, 0);
        for dx in -4..=4 {
            for dz in -4..=4 {
                let p = ChunkPos {
                    x: cp.x + dx,
                    z: cp.z + dz,
                };
                w.ensure_chunk(p);
                for lx in 0..16 {
                    for lz in 0..16 {
                        let (wx, wz) = (p.x * 16 + lx, p.z * 16 + lz);
                        if w.generator.biome(wx, wz) != biome {
                            continue;
                        }
                        for y in 64..200 {
                            let blk = w.get_block(wx, y, wz);
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
            w.ensure_chunk(ChunkPos { x: cx, z: cz });
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
        // The plate map can make the first biome hit a sliver; try a
        // few well-separated patches before giving up.
        let mut anchors: Vec<(i32, i32)> = Vec::new();
        for r in 0..200 {
            let d = r * 24;
            for (x, z) in [(d, 0), (-d, 0), (0, d), (0, -d), (d, d), (-d, -d)] {
                if g.biome(x, z) == biome
                    && anchors
                        .iter()
                        .all(|&(ax, az)| (ax - x).abs() + (az - z).abs() > 400)
                {
                    anchors.push((x, z));
                }
            }
            if anchors.len() >= 3 {
                break;
            }
        }
        let ids: Vec<_> = blocks.iter().filter_map(|n| reg.block_id(n)).collect();
        for (ai, (x, z)) in anchors.into_iter().enumerate() {
            let cp = ChunkPos::of_world(x, z);
            let mut w = World::new(42, tmp_dir(&format!("{name}{ai}")), reg.clone());
            for dx in -4..=4 {
                for dz in -4..=4 {
                    let p = ChunkPos {
                        x: cp.x + dx,
                        z: cp.z + dz,
                    };
                    w.ensure_chunk(p);
                    for lx in 0..16 {
                        for lz in 0..16 {
                            for y in 64..140 {
                                if ids.contains(&w.get_block(p.x * 16 + lx, y, p.z * 16 + lz)) {
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
        // Prefer a dry column so the screenshot shows land.
        let (mut bx, mut bz) = find_biome(&g, biome).unwrap();
        'scan: for dx in 0..200 {
            for dz in 0..200 {
                let (x, z) = (bx + dx, bz + dz);
                // Deep interior: same biome 32 blocks in every direction.
                let deep = [(0, 0), (32, 0), (-32, 0), (0, 32), (0, -32)]
                    .iter()
                    .all(|(ox, oz)| g.biome(x + ox, z + oz) == biome);
                if deep && g.surface_estimate(x, z) > crate::chunk::SEA_LEVEL + 2 {
                    bx = x;
                    bz = z;
                    break 'scan;
                }
            }
        }
        println!("{biome:?}: {bx},{bz}");
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
            w.ensure_chunk(ChunkPos { x: cx, z: cz });
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
            w.ensure_chunk(ChunkPos { x, z });
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
        ("granite", granite),
        ("basalt", basalt),
    ] {
        assert!(n(blk) > 500, "{name} present in the sample ({})", n(blk));
    }
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
    assert!(!marble_cells.is_empty(), "contact marble exists");
    let mut hits = 0;
    let sample: Vec<_> = marble_cells.iter().step_by(7).take(30).collect();
    for &&(mx, my, mz) in &sample {
        let mut near = false;
        'scan: for dx in -16i32..=16 {
            for dy in -16i32..=16 {
                for dz in -16i32..=16 {
                    if w.get_block(mx + dx, my + dy, mz + dz) == granite {
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
fn volcanoes_rise_pool_and_dress() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("volcano"), reg.clone());
    // Find the nearest deterministic volcano to the origin.
    let mut found = None;
    'search: for r in 0..30 {
        let d = r * 96;
        for (x, z) in [
            (d, 0),
            (-d, 0),
            (0, d),
            (0, -d),
            (d, d),
            (-d, -d),
            (d, -d),
            (-d, d),
        ] {
            if let Some(v) = w.generator.volcano_near(x, z) {
                found = Some(v);
                break 'search;
            }
        }
    }
    let v = found.expect("a volcano within the search ring");
    println!(
        "volcano at ({}, {}) r={} h={}",
        v.x, v.z, v.radius, v.height
    );
    let vc = ChunkPos::of_world(v.x, v.z);
    for dx in -3..=3 {
        for dz in -3..=3 {
            w.ensure_chunk(ChunkPos {
                x: vc.x + dx,
                z: vc.z + dz,
            });
        }
    }
    // The cone rises well above the surrounding country.
    let rim = w.surface_height(v.x + v.crater_r() as i32 + 1, v.z);
    let baseline = w.surface_height(v.x + v.radius as i32 + 24, v.z);
    assert!(
        rim > baseline + 15,
        "the cone rises: rim {rim} vs baseline {baseline}"
    );
    // The crater pools lava behind an obsidian rim.
    let b = |n: &str| reg.block_id(n).unwrap();
    let mut lava_cells = 0;
    let mut obsidian_cells = 0;
    let mut sulfur_cells = 0;
    let mut basalt_cells = 0;
    let scan = v.radius as i32;
    for dx in -scan..=scan {
        for dz in -scan..=scan {
            let (x, z) = (v.x + dx, v.z + dz);
            for y in 40..CHUNK_Y as i32 {
                let blk = w.get_block(x, y, z);
                if reg.is_lava(blk) {
                    lava_cells += 1;
                } else if blk == b("base:obsidian") {
                    obsidian_cells += 1;
                } else if blk == b("base:sulfur_ore") {
                    sulfur_cells += 1;
                } else if blk == b("base:basalt") {
                    basalt_cells += 1;
                }
            }
        }
    }
    assert!(lava_cells > 30, "the crater pools lava ({lava_cells})");
    assert!(obsidian_cells > 10, "an obsidian rim ({obsidian_cells})");
    assert!(
        sulfur_cells > 3,
        "sulfur crusts the flanks ({sulfur_cells})"
    );
    assert!(basalt_cells > 3000, "the cone is basalt ({basalt_cells})");
}

#[test]
fn pipes_and_geodes_seed_the_deep() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("pipes"), reg.clone());
    let b = |n: &str| reg.block_id(n).unwrap();

    // A kimberlite pipe, found by the locator, generated, and shaped
    // like a carrot: wide near its top, a thread at depth.
    let mut pipe = None;
    'p: for x in -40..=40 {
        for z in -40..=40 {
            let cp = ChunkPos { x, z };
            if w.generator.pipe_at(cp).is_some() {
                pipe = Some(cp);
                break 'p;
            }
        }
    }
    let cp = pipe.expect("a pipe within the search square");
    w.ensure_chunk(cp);
    let kim = b("base:kimberlite");
    let count_at = |w: &World, y: i32| -> i32 {
        let mut n = 0;
        for lx in 0..16 {
            for lz in 0..16 {
                if w.get_block(cp.x * 16 + lx, y, cp.z * 16 + lz) == kim {
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
    'g: for x in -60..=60 {
        for z in -60..=60 {
            let cp = ChunkPos { x, z };
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
            for lx in 0..16 {
                for lz in 0..16 {
                    for y in 40..80 {
                        let blk = w.get_block(cp.x * 16 + lx, y, cp.z * 16 + lz);
                        if blk == b("base:amethyst_block") {
                            amethyst += 1;
                        } else if blk == b("base:quartz_block") {
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
    assert!(placed, "a geode placed in limestone country");
}

#[test]
fn ores_stay_in_their_host_rocks() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("hosts"), reg.clone());
    for x in -4..=4 {
        for z in -4..=4 {
            w.ensure_chunk(ChunkPos { x, z });
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
    let mut w = World::new(42, tmp_dir("hydro"), reg.clone());

    // Rivers and lakes above sea level, found through the same helper
    // worldgen uses. Terraced fills leave honest dry washes where a
    // reach lip outruns its floor, so try several wet candidates.
    let mut candidates: Vec<(i32, i32, i32)> = Vec::new();
    'r: for r in 1..400 {
        let d = r * 16;
        for (x, z) in [(d, 0), (-d, 0), (0, d), (0, -d), (d, d), (-d, -d)] {
            if let Some(fill) = w.generator.water_features(x, z)
                && fill > crate::chunk::SEA_LEVEL + 3
                && candidates
                    .iter()
                    .all(|&(ax, az, _)| (ax - x).abs() + (az - z).abs() > 200)
            {
                candidates.push((x, z, fill));
                if candidates.len() >= 6 {
                    break 'r;
                }
            }
        }
    }
    assert!(!candidates.is_empty(), "rivers or lakes above the sea");
    let mut water_cells = 0;
    for (x, z, fill) in candidates {
        let cp = ChunkPos::of_world(x, z);
        for dx in -1..=1 {
            for dz in -1..=1 {
                w.ensure_chunk(ChunkPos {
                    x: cp.x + dx,
                    z: cp.z + dz,
                });
            }
        }
        for dx in -6..=6 {
            for dz in -6..=6 {
                for y in crate::chunk::SEA_LEVEL + 2..=fill + 2 {
                    if reg.is_water(w.get_block(x + dx, y, z + dz)) {
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

    // The volcano's magma chamber: lava under the throat.
    let mut found = None;
    'v: for r in 0..30 {
        let d = r * 96;
        for (x, z) in [
            (d, 0),
            (-d, 0),
            (0, d),
            (0, -d),
            (d, d),
            (-d, -d),
            (d, -d),
            (-d, d),
        ] {
            if let Some(v) = w.generator.volcano_near(x, z) {
                found = Some(v);
                break 'v;
            }
        }
    }
    let v = found.expect("a volcano within the search ring");
    w.ensure_chunk(ChunkPos::of_world(v.x, v.z));
    let mut chamber = 0;
    for dx in -8..=8 {
        for dz in -8..=8 {
            for y in 12..20 {
                if reg.is_lava(w.get_block(v.x + dx, y, v.z + dz)) {
                    chamber += 1;
                }
            }
        }
    }
    assert!(chamber > 30, "a magma chamber breathes below ({chamber})");
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
        if let Some((x, z)) = find_biome(&g, biome) {
            println!(
                "{}: ({x}, {z}) est {}",
                biome.name(),
                g.surface_estimate(x, z)
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
            w.ensure_chunk(ChunkPos {
                x: cp.x + dx,
                z: cp.z + dz,
            });
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
        let _ = g.generate(ChunkPos { x: i, z: -i }, &reg);
    }
    let gen_dt = t0.elapsed();
    let mut w = World::new(42, tmp_dir("bench"), reg.clone());
    let t1 = std::time::Instant::now();
    for i in 0..50 {
        w.ensure_chunk(ChunkPos { x: i, z: -i });
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
    let pos = ChunkPos { x: 3, z: -2 };
    a.ensure_chunk(pos);
    // Generation is pure: a worker's chunk equals the sync path.
    let chunk = b2.generator.generate(pos, &reg);
    assert!(b2.adopt_generated(pos, chunk));
    assert_eq!(a.chunks()[&pos].raw(), b2.chunks()[&pos].raw());
    // A saved copy on disk beats the worker's fresh terrain.
    let stone = b(&reg, "base:stone");
    a.set_block(3 * 16 + 4, 200, -2 * 16 + 4, stone);
    a.save_modified();
    let mut c = World::load_or_create(a.save_dir_for_test(), reg.clone());
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
            w.ensure_chunk(ChunkPos { x, z });
        }
    }
    println!("mobs after seeding 169 chunks: {}", w.mob_count());
    let mut srv = crate::server::Server::new(w, 0.3, 7);
    let players = [crate::server::PlayerCtx {
        pos: glam::Vec3::new(8.0, 80.0, 8.0),
        spawn: glam::Vec3::new(8.0, 80.0, 8.0),
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
            w.ensure_chunk(ChunkPos { x, z });
        }
    }
    let mut srv = crate::server::Server::new(w, 0.3, 7);
    let players = [crate::server::PlayerCtx {
        pos: glam::Vec3::new(8.0, 80.0, 8.0),
        spawn: glam::Vec3::new(8.0, 80.0, 8.0),
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
    let mut w = World::new(42, tmp_dir("weirs"), reg.clone());
    let mut candidates: Vec<(i32, i32)> = Vec::new();
    'r: for r in 1..400 {
        let d = r * 16;
        for (x, z) in [(d, 0), (-d, 0), (0, d), (0, -d), (d, d), (-d, -d)] {
            if let Some(fill) = w.generator.water_features(x, z)
                && fill > crate::chunk::SEA_LEVEL + 3
                && candidates
                    .iter()
                    .all(|&(ax, az)| (ax - x).abs() + (az - z).abs() > 300)
            {
                candidates.push((x, z));
                if candidates.len() >= 4 {
                    break 'r;
                }
            }
        }
    }
    assert!(!candidates.is_empty(), "rivers above the sea exist");
    let (mut water_cells, mut exposed) = (0, 0);
    for (x, z) in candidates {
        let cp = ChunkPos::of_world(x, z);
        for dx in -1..=1 {
            for dz in -1..=1 {
                w.ensure_chunk(ChunkPos {
                    x: cp.x + dx,
                    z: cp.z + dz,
                });
            }
        }
        for dx in -10..=10 {
            for dz in -10..=10 {
                for y in crate::chunk::SEA_LEVEL + 2..200 {
                    if !reg.is_water(w.get_block(x + dx, y, z + dz)) {
                        continue;
                    }
                    water_cells += 1;
                    for (nx, nz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        if w.get_block(x + dx + nx, y, z + dz + nz) == AIR {
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
    let mut w = World::new(42, tmp_dir("lakesettle"), reg.clone());
    let cp = ChunkPos::of_world(-52, -158);
    for dx in -2..=2 {
        for dz in -2..=2 {
            w.ensure_chunk(ChunkPos {
                x: cp.x + dx,
                z: cp.z + dz,
            });
        }
    }
    let mut quiet = false;
    for _ in 0..400 {
        if !w.tick_water(10_000) {
            quiet = true;
            break;
        }
    }
    assert!(quiet, "the lake settles instead of churning");
    // Once settled, the waterline still may not hang in the open: no
    // water cell should sit beside same-height air (films/shelves).
    let (mut cells, mut exposed) = (0, 0);
    for x in -52 - 30..-52 + 30 {
        for z in -158 - 30..-158 + 30 {
            for y in crate::chunk::SEA_LEVEL + 2..140 {
                if !reg.is_water(w.get_block(x, y, z)) {
                    continue;
                }
                cells += 1;
                for (nx, nz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    if w.get_block(x + nx, y, z + nz) == AIR {
                        exposed += 1;
                    }
                }
            }
        }
    }
    assert!(cells > 100, "the pools hold water ({cells})");
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
            w.ensure_chunk(ChunkPos {
                x: cp.x + dx,
                z: cp.z + dz,
            });
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
