//! Nest/dens spawn-gate tests (capability E9): a nest block is the only
//! source of its bound species; clearing the nest stops the respawns, and
//! the records survive a save/reload like any persisted gate.

use super::*;

fn pad(w: &mut World, reg: &Registry, x0: i32, x1: i32, z0: i32, z1: i32, h: i32) {
    let grass = b(reg, "base:grass");
    for x in x0..=x1 {
        for z in z0..=z1 {
            w.set_block(x, h, z, grass);
            for dy in 1..5 {
                if w.get_block(x, h + dy, z) != AIR {
                    w.set_block(x, h + dy, z, AIR);
                }
            }
        }
    }
}

const NEST_MOD: &str = r#"id = "denworld"
world_api = 2
depends = ["base"]
"#;

const NEST_BLOCKS: &str = r#"
[[block]]
id = "denwolf_nest"
name = "Denwolf Nest"
texture = "@stone"
hardness = 2.0
"#;

const NEST_ANIMALS: &str = r#"
[[animal]]
id = "denwolf"
name = "Denwolf"
hostile = true
biomes = ["plains"]
health = 12
speed = 2.6
attack = 4
tex = "@deer"
aggro_range = 16
ire_min = 0
"#;

const NEST_FILE: &str = r#"
[[nest]]
id = "denwolf_den"
block = "denworld:denwolf_nest"
species = "denworld:denwolf"
radius = 16.0
interval = 4.0
cap = 3
"#;

fn nest_reg() -> Arc<Registry> {
    let root = tmp_dir("nest-quest");
    let dir = root.join("denworld");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), NEST_MOD).unwrap();
    std::fs::write(dir.join("blocks.toml"), NEST_BLOCKS).unwrap();
    std::fs::write(dir.join("animals.toml"), NEST_ANIMALS).unwrap();
    std::fs::write(dir.join("nests.toml"), NEST_FILE).unwrap();
    Arc::new(registry::load(&root))
}

#[test]
fn a_nest_spawns_its_species_until_it_is_cleared() {
    let reg = nest_reg();
    let wolf = reg.animal_id("denworld:denwolf").expect("nest species registers");
    assert_eq!(reg.nests.len(), 1, "the nest def resolved");
    let mut w = test_world_with("nest-spawn", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 30, 0, 30, h);
    // Place the nest block; the record appears with the block.
    w.set_block_at(bp(8, h, 8), b(&reg, "denworld:denwolf_nest"));
    assert_eq!(w.nests().count(), 1, "placing the nest recorded it");
    let player = ep(Vec3::new(8.0, (h + 1) as f32, 8.0));
    let mut rng = 21u32;
    // Night cycles near the nest: the denwolf manifests from its den.
    for _ in 0..160 {
        w.tick_nest_spawns(player, 0.12, 5.0, &mut rng);
    }
    let spawned = w
        .mobs()
        .iter()
        .filter(|m| m.species == wolf)
        .collect::<Vec<_>>();
    assert!(!spawned.is_empty(), "the nest spawned its denwolf");
    for m in &spawned {
        let d = m.pos.horizontal_distance_to(ep(Vec3::new(8.0, (h + 1) as f32, 8.0)));
        assert!(d <= 20.0, "a denwolf manifested near its den ({d})");
    }
    // Clear the den: the record goes with the block, and no denwolf ever
    // manifests again (it is nest-bound, so the ire ring can't replace it).
    w.set_block_at(bp(8, h, 8), AIR);
    assert_eq!(w.nests().count(), 0, "breaking the nest cleared the record");
    w.mobs_mut().retain(|m| m.species != wolf);
    for _ in 0..200 {
        w.tick_nest_spawns(player, 0.12, 5.0, &mut rng);
    }
    assert_eq!(
        w.mobs().iter().filter(|m| m.species == wolf).count(),
        0,
        "clearing the nest stopped the respawns"
    );
}

#[test]
fn nests_survive_save_and_reload() {
    let reg = nest_reg();
    let mut w = test_world_with("nest-save", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 30, 0, 30, h);
    w.set_block_at(bp(8, h, 8), b(&reg, "denworld:denwolf_nest"));
    save_world(&mut w);
    let dir = w.save_dir_for_test();
    let mut reloaded = World::load_or_create(dir, reg.clone()).expect("world reloads");
    for x in -2..=2 {
        for z in -2..=2 {
            reloaded.ensure_chunk(tchunk(x, z));
        }
    }
    assert_eq!(reloaded.nests().count(), 1, "the nest record persisted");
    // And a live nest spawns after reload.
    let wolf = reg.animal_id("denworld:denwolf").unwrap();
    let player = ep(Vec3::new(8.0, (h + 1) as f32, 8.0));

    let mut rng = 5u32;
    for _ in 0..120 {
        reloaded.tick_nest_spawns(player, 0.12, 5.0, &mut rng);
    }
    assert!(
        reloaded.mobs().iter().any(|m| m.species == wolf),
        "the reloaded den still spawns"
    );
}
