//! Shared fixtures and subsystem test modules.

use std::path::Path;
use std::sync::Arc;

use glam::Vec3;

use crate::chunk::{CHUNK_Y, ChunkPos, SEA_LEVEL};
use crate::inventory::{Inventory, ItemStack, click_stack};
use crate::physics::{Input, Player};
use crate::raycast::raycast;
use crate::registry::{self, AIR, Registry};
use crate::world::World;

mod agent;
mod alchemy;
mod archetypes;
mod atlas;
mod belt;
mod climate;
mod climate_audit;
mod dross;
mod dungeon;
mod ecology;
pub(crate) mod fixtures;
mod gameplay;
mod geology;
mod hearts;
mod hydrology;
mod identity;
mod implements;
mod interiors;
mod local_structure;
mod machines;
mod mobs;
mod multiblock;
mod multiplayer;
mod nests;
mod player;
mod playtest;
mod power_draw;
mod qualification;
mod rail;
#[path = "registry.rs"]
mod registry_tests;
mod rendering;
mod soil;
mod template;
mod terrain_io;
mod water_cycle;
mod workings;
mod world;
mod worldgen;

fn base_reg() -> Arc<Registry> {
    Arc::new(registry::load(Path::new("/nonexistent-mods-dir")))
}

fn tmp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("wildforge-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tchunk(x: i32, z: i32) -> ChunkPos {
    ChunkPos::from_centered(crate::planet::Face::PosZ, x, z)
        .expect("test chunk coordinate must be inside the finite planet")
}

fn ep(local: Vec3) -> crate::planet::EntityPos {
    crate::planet::EntityPos::from_local(crate::planet::Face::PosZ, local)
        .expect("test position must fit on the finite PosZ face")
}

fn bp(x: i32, y: i32, z: i32) -> crate::planet::BlockPos {
    crate::planet::BlockPos::of_world(x, y, z).expect("positive-Z test coordinate")
}

fn surface_offset(pos: crate::planet::SurfacePos, du: i32, dv: i32) -> crate::planet::SurfacePos {
    crate::planet::SurfacePos::canonicalized(
        pos.face(),
        i32::from(pos.u()) + du,
        i32::from(pos.v()) + dv,
    )
    .expect("small test offset canonicalizes on the finite planet")
}

fn local_season_day(world: &World, pos: crate::planet::SurfacePos, season: usize) -> u32 {
    let northern_season = if world.latitude_at_surface(pos) < 0.0 {
        (season + 2) % 4
    } else {
        season
    };
    northern_season as u32 * crate::world::SEASON_DAYS
}

fn block_pos(pos: crate::planet::SurfacePos, y: i32) -> crate::planet::BlockPos {
    crate::planet::BlockPos::new(pos.face(), pos.u(), y as u8, pos.v())
        .expect("test surface and shell height form a block position")
}

fn block_at(world: &World, pos: crate::planet::SurfacePos, y: i32) -> crate::registry::BlockId {
    if !(0..CHUNK_Y as i32).contains(&y) {
        return AIR;
    }
    world.get_block_at(block_pos(pos, y))
}

/// Every directed cube-face transition at a non-corner coordinate.
///
/// A physical cube edge appears twice in this list, once from either incident
/// face.  Keeping both directions is important: several transitions rotate
/// the destination chart, and a subsystem can accidentally work in one
/// direction while failing on the reciprocal walk.
#[derive(Clone, Copy, Debug)]
struct DirectedSeam {
    face: crate::planet::Face,
    direction: crate::planet::Direction4,
    source: crate::planet::SurfacePos,
    across: crate::planet::SurfacePos,
    heading: Vec3,
}

fn directed_planet_seams() -> Vec<DirectedSeam> {
    use crate::planet::{Direction4, FACE_BLOCKS, Face, SurfacePos, step4};

    Face::ALL
        .into_iter()
        .flat_map(|face| Direction4::ALL.map(move |direction| (face, direction)))
        .enumerate()
        .map(|(index, (face, direction))| {
            // Spread fixtures around each edge so systems that retain queues or
            // cached light cannot interact with the next case.
            let varying = 320 + index as u16 * 300;
            let (u, v, heading) = match direction {
                Direction4::East => (FACE_BLOCKS - 1, varying, Vec3::X),
                Direction4::North => (varying, FACE_BLOCKS - 1, Vec3::Z),
                Direction4::West => (0, varying, Vec3::NEG_X),
                Direction4::South => (varying, 0, Vec3::NEG_Z),
            };
            let source = SurfacePos::new(face, u, v).unwrap();
            let across = step4(source, direction).pos;
            assert_ne!(source.face(), across.face());
            DirectedSeam {
                face,
                direction,
                source,
                across,
                heading,
            }
        })
        .collect()
}

fn ensure_surface_neighborhood(world: &mut World, pos: crate::planet::SurfacePos, radius: i32) {
    let center = ChunkPos::from_surface(pos);
    for du in -radius..=radius {
        for dv in -radius..=radius {
            world.ensure_chunk(center.offset(du, dv));
        }
    }
}

fn save_world(world: &mut World) {
    let report = world.save_modified();
    assert!(report.is_ok(), "test save failed: {}", report.summary());
}

fn test_world_with(name: &str, reg: Arc<Registry>) -> World {
    let mut w = World::new(42, tmp_dir(name), reg);
    for x in -2..=2 {
        for z in -2..=2 {
            w.ensure_chunk(tchunk(x, z));
        }
    }
    w
}

fn test_world_seeded(name: &str, reg: Arc<Registry>, seed: u32) -> World {
    World::new(seed, tmp_dir(name), reg)
}

fn test_world(name: &str) -> World {
    test_world_with(name, base_reg())
}

fn b(reg: &Registry, name: &str) -> crate::registry::BlockId {
    reg.block_id(name)
        .unwrap_or_else(|| panic!("missing block {name}"))
}

fn it(reg: &Registry, name: &str) -> crate::registry::ItemId {
    reg.item_id(name)
        .unwrap_or_else(|| panic!("missing item {name}"))
}

// ---------------- phase 1: registry & saves ----------------

// ---------------- phase 2: data mods ----------------

fn write_demo_mod(root: &Path) {
    let dir = root.join("testium");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mod.toml"),
        "id = \"testium\"\nname = \"Testium\"\nversion = \"1.0.0\"\nworld_api = 2\ndepends = [\"base\"]\nretrogen = \"untouched_host_only\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("blocks.toml"),
        r#"
[[block]]
id = "ore"
name = "Testium Ore"
texture = "@stone"
hardness = 5.0
tool = "pickaxe"
requires_tool = true
drops = "testium:shard"
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("items.toml"),
        r#"
[[item]]
id = "shard"
name = "Testium Shard"
texture = "@stick"
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("recipes.toml"),
        r#"
[[recipe]]
pattern = ["ss", "ss"]
keys = { s = "testium:shard" }
output = "testium:ore"
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("features.toml"),
        r#"
[[feature]]
type = "ore"
block = "testium:ore"
replaces = "base:stone"
vein_size = 8
per_chunk = 24
y_range = [4, 60]
"#,
    )
    .unwrap();
}

// ---------------- phase 3: scripts ----------------

fn write_script_mod(root: &Path, script: &str) -> Vec<(String, std::path::PathBuf)> {
    let dir = root.join("scripty");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"scripty\"\nworld_api = 2\n").unwrap();
    std::fs::write(dir.join("main.rhai"), script).unwrap();
    vec![("scripty".to_string(), dir)]
}

// ---------------- phase 4: hot reload remap ----------------

// ---------------- biomes ----------------

use crate::worldgen::{Biome, Generator};

/// Several well-separated countries of one biome on the finite planet.
/// Enumerating all 6×9×9 country cells is cheap, deterministic, and avoids
/// accidentally testing the retired positive-Z planar adapter.
fn find_biomes(g: &Generator, want: Biome, n: usize) -> Vec<crate::planet::SurfacePos> {
    let mut out = Vec::new();
    for face in crate::planet::Face::ALL {
        for u in 0..Generator::PROVINCE_CELLS {
            for v in 0..Generator::PROVINCE_CELLS {
                let key = crate::worldgen::ProvinceKey { face, u, v };
                let site = g.province_center_at(key);
                let sample = crate::planet::SurfacePos::canonicalized(
                    site.face(),
                    i32::from(site.u()) + 48,
                    i32::from(site.v()) + 48,
                )
                .expect("a country-interior sample canonicalizes");
                if g.biome_at(sample) == want
                    && g.province_at(sample).key == g.province_at(site).key
                    && g.surface_estimate_at(sample) > crate::chunk::SEA_LEVEL + 2
                    && g.plate_relief(&g.climate_at(sample)) <= 30.0
                {
                    out.push(sample);
                    if out.len() >= n {
                        return out;
                    }
                }
            }
        }
    }
    out
}

fn find_biome(g: &Generator, want: Biome) -> Option<crate::planet::SurfacePos> {
    // Dry land off a fold range: a province center can sit under the
    // sea or on a peak, and neither grows what the country grows.
    find_biome_where(g, want, |pos| {
        g.surface_estimate_at(pos) > crate::chunk::SEA_LEVEL + 2
            && g.plate_relief(&g.climate_at(pos)) <= 30.0
    })
}

/// As `find_biome`, but the caller adds conditions (dry, inland, off
/// a plate boundary). Rays miss whole countries now that provinces
/// are ~900 blocks; this spirals a grid at half-province spacing.
fn find_biome_where(
    g: &Generator,
    want: Biome,
    pred: impl Fn(crate::planet::SurfacePos) -> bool,
) -> Option<crate::planet::SurfacePos> {
    for face in crate::planet::Face::ALL {
        for u in 0..Generator::PROVINCE_CELLS {
            for v in 0..Generator::PROVINCE_CELLS {
                let key = crate::worldgen::ProvinceKey { face, u, v };
                let site = g.province_center_at(key);
                let sample = crate::planet::SurfacePos::canonicalized(
                    site.face(),
                    i32::from(site.u()) + 48,
                    i32::from(site.v()) + 48,
                )
                .expect("a country-interior sample canonicalizes");
                if g.biome_at(sample) == want
                    && g.province_at(sample).key == g.province_at(site).key
                    && pred(sample)
                {
                    return Some(sample);
                }
            }
        }
    }
    None
}

fn find_water_features(
    generator: &crate::worldgen::Generator,
    wanted: usize,
) -> Vec<(crate::planet::SurfacePos, i32)> {
    let mut found: Vec<(crate::planet::SurfacePos, i32)> = Vec::new();
    'faces: for face in crate::planet::Face::ALL {
        for u in (8..crate::planet::FACE_BLOCKS).step_by(16) {
            for v in (8..crate::planet::FACE_BLOCKS).step_by(16) {
                let pos = crate::planet::SurfacePos::new(face, u, v).unwrap();
                let Some(fill) = generator.water_features_at(pos) else {
                    continue;
                };
                if fill <= crate::chunk::SEA_LEVEL + 3
                    || found.iter().any(|(other, _)| {
                        crate::planet::geodesic_distance(pos.center(), other.center()) < 200.0
                    })
                {
                    continue;
                }
                found.push((pos, fill));
                if found.len() >= wanted {
                    break 'faces;
                }
            }
        }
    }
    found
}

/// Planetary counterpart used by generator tests after the topology break.
fn gen_at_surface(reg: &Arc<Registry>, name: &str, pos: crate::planet::SurfacePos) -> (World, i32) {
    let mut world = World::new(42, tmp_dir(name), reg.clone());
    let chunk = ChunkPos::from_surface(pos);
    for du in -1..=1 {
        for dv in -1..=1 {
            world.ensure_chunk(chunk.offset(du, dv));
        }
    }
    let height = world.surface_height_at(pos);
    (world, height)
}

// ---------------- terrain v2 ----------------

// ---------------- bronze age ----------------

// ---------------- food & farming ----------------

// ---------------- gameplay (regression) ----------------

/// Sum every loaded water cell's volume (units, 8 per full cell).
fn total_water(w: &World) -> u32 {
    let mut sum = 0u32;
    for c in w.chunks().values() {
        for lx in 0..crate::chunk::CHUNK_X {
            for lz in 0..crate::chunk::CHUNK_Z {
                for y in 0..CHUNK_Y {
                    if let Some(v) = w.reg.water_volume(c.get(lx, y, lz)) {
                        sum += v as u32;
                    }
                }
            }
        }
    }
    sum
}

fn settle_water(w: &mut World) {
    for _ in 0..400 {
        if !w.tick_water(100_000) {
            break;
        }
    }
}

// ---------------- texture packs ----------------

fn write_solid_png(path: &std::path::Path, w: u32, h: u32, rgba: [u8; 4]) {
    let mut data = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut data, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .unwrap()
            .write_image_data(&rgba.repeat((w * h) as usize))
            .unwrap();
    }
    std::fs::write(path, data).unwrap();
}

fn tile_center(img: &[u8], px: u32, slot: u16) -> [u8; 4] {
    let tp = px / crate::atlas::ATLAS_TILES;
    let cx = (slot as u32 % crate::atlas::ATLAS_TILES) * tp + tp / 2;
    let cy = (slot as u32 / crate::atlas::ATLAS_TILES) * tp + tp / 2;
    let i = ((cy * px + cx) * 4) as usize;
    [img[i], img[i + 1], img[i + 2], img[i + 3]]
}

// ---------------- world listing / new-world naming ----------------

// ---------------- animals: species, mobs, hunting ----------------

// ---------------- lighting ----------------

// ---------------- chests ----------------

// ---------------- hostiles: ire, wardens, projectiles ----------------

// ---------------- bows & armor ----------------

// ---------------- stewardship ----------------

// ---------------- iron & steel ----------------

// ---------------- ruins & archaeology ----------------

// ---------------- the server (sim/client split) ----------------

// ---------------- multiplayer: protocol + loopback ----------------

// ---------------- glassworks ----------------

// ---------------- steelworks ----------------

/// Build a valid bloomery at (x,y,z)=mouth with core on +X, in air.
fn build_bloomery(w: &mut World, reg: &Registry, mx: i32, my: i32, mz: i32) {
    let fb = reg.block_id("base:firebrick").unwrap();
    let mouth = reg.block_id("base:bloomery").unwrap();
    let (cx, cz) = (mx + 1, mz);
    for ly in 0..3 {
        for rx in -1..=1i32 {
            for rz in -1..=1i32 {
                if rx == 0 && rz == 0 {
                    continue;
                }
                w.set_block(cx + rx, my + ly, cz + rz, fb);
            }
        }
        w.set_block(cx, my + ly, cz, AIR);
    }
    w.set_block(mx, my, mz, mouth);
}

/// The forge: the bloomery shell with a forge mouth, three more
/// courses of chimney over the core, and a stone anvil by the mouth.
#[allow(dead_code)]
fn build_forge(w: &mut World, reg: &Registry, mx: i32, my: i32, mz: i32) {
    let fb = reg.block_id("base:firebrick").unwrap();
    let mouth = reg.block_id("base:forge").unwrap();
    let anvil = reg.block_id("base:stone_anvil").unwrap();
    let (cx, cz) = (mx + 1, mz);
    for ly in 0..6 {
        for rx in -1..=1i32 {
            for rz in -1..=1i32 {
                if rx == 0 && rz == 0 {
                    continue;
                }
                w.set_block(cx + rx, my + ly, cz + rz, fb);
            }
        }
        w.set_block(cx, my + ly, cz, AIR);
    }
    w.set_block(mx, my, mz, mouth);
    w.set_block(mx - 1, my, mz, anvil);
}

// ---------------- weather & seasons ----------------

// ---------------- game feel (the juice layer) ----------------

// ---------------- point lights (the director) ----------------

// ---------------- the player, seen ----------------
