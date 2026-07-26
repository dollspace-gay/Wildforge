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
mod ecology;
mod hearts;
mod identity;
mod machines;
mod mobs;
mod multiplayer;
mod player;
#[path = "registry.rs"]
mod registry_tests;
mod rendering;
mod soil;
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

fn test_world_with(name: &str, reg: Arc<Registry>) -> World {
    let mut w = World::new(42, tmp_dir(name), reg);
    for x in -2..=2 {
        for z in -2..=2 {
            w.ensure_chunk(ChunkPos { x, z });
        }
    }
    w
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
        "id = \"testium\"\nname = \"Testium\"\nversion = \"1.0.0\"\ndepends = [\"base\"]\n",
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
    std::fs::write(dir.join("mod.toml"), "id = \"scripty\"\n").unwrap();
    std::fs::write(dir.join("main.rhai"), script).unwrap();
    vec![("scripty".to_string(), dir)]
}

// ---------------- phase 4: hot reload remap ----------------

// ---------------- biomes ----------------

use crate::worldgen::{Biome, Generator};

/// Find a column of the given biome near the origin (deterministic per seed).
/// Several well-separated countries of one biome — for tests that
/// want more than one sample before concluding anything.
fn find_biomes(g: &Generator, want: Biome, n: usize) -> Vec<(i32, i32)> {
    let mut out: Vec<(i32, i32)> = Vec::new();
    for ring in 0..40i32 {
        let mut keys: Vec<(i32, i32)> = Vec::new();
        if ring == 0 {
            keys.push((0, 0));
        } else {
            for i in -ring..=ring {
                keys.push((i, -ring));
                keys.push((i, ring));
            }
            for j in -ring + 1..ring {
                keys.push((-ring, j));
                keys.push((ring, j));
            }
        }
        for key in keys {
            let (cx, cz) = g.province_center(key.0, key.1);
            // Off the site: the country's HEART stands at its center,
            // and a landmark is not a sample of the ground around it.
            let (x, z) = (cx + 48, cz + 48);
            if g.biome(x, z) == want
                && g.province(x, z).key == key
                && g.surface_estimate(x, z) > crate::chunk::SEA_LEVEL + 2
                && g.plate_relief(&g.climate(x, z)) <= 30.0
            {
                out.push((x, z));
                if out.len() >= n {
                    return out;
                }
            }
        }
    }
    out
}

fn find_biome(g: &Generator, want: Biome) -> Option<(i32, i32)> {
    // Dry land off a fold range: a province center can sit under the
    // sea or on a peak, and neither grows what the country grows.
    find_biome_where(g, want, |x, z| {
        g.surface_estimate(x, z) > crate::chunk::SEA_LEVEL + 2
            && g.plate_relief(&g.climate(x, z)) <= 30.0
    })
}

/// As `find_biome`, but the caller adds conditions (dry, inland, off
/// a plate boundary). Rays miss whole countries now that provinces
/// are ~900 blocks; this spirals a grid at half-province spacing.
fn find_biome_where(
    g: &Generator,
    want: Biome,
    pred: impl Fn(i32, i32) -> bool,
) -> Option<(i32, i32)> {
    // Walk province CENTERS, not arbitrary columns: a country has one
    // label, so one sample answers for all of it — and the center is
    // the deepest interior point there is, never a border fringe.
    for ring in 0..40i32 {
        let mut keys: Vec<(i32, i32)> = Vec::new();
        if ring == 0 {
            keys.push((0, 0));
        } else {
            for i in -ring..=ring {
                keys.push((i, -ring));
                keys.push((i, ring));
            }
            for j in -ring + 1..ring {
                keys.push((-ring, j));
                keys.push((ring, j));
            }
        }
        for key in keys {
            let (cx, cz) = g.province_center(key.0, key.1);
            // Off the site: the country's HEART stands at its center,
            // and a landmark is not a sample of the ground around it.
            let (x, z) = (cx + 48, cz + 48);
            if g.biome(x, z) == want && g.province(x, z).key == key && pred(x, z) {
                return Some((x, z));
            }
        }
    }
    None
}

/// Generate the chunk containing a column and return (world, surface y).
fn gen_at(reg: &Arc<Registry>, name: &str, x: i32, z: i32) -> (World, i32) {
    let mut w = World::new(42, tmp_dir(name), reg.clone());
    let cp = ChunkPos::of_world(x, z);
    for dx in -1..=1 {
        for dz in -1..=1 {
            w.ensure_chunk(ChunkPos {
                x: cp.x + dx,
                z: cp.z + dz,
            });
        }
    }
    let h = w.surface_height(x, z);
    (w, h)
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
