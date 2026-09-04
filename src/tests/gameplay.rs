//! Gameplay-effect regressions: exercise the same world operations used by
//! Server, including observable den spawning after a dungeon resets.

use super::*;
use crate::planet::{BlockPos, EntityPos, Face};

fn proof_registry() -> Arc<Registry> {
    let reg =
        registry::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/gameplay/mods"));
    assert!(reg.material_errors.is_empty(), "{:?}", reg.material_errors);
    assert!(reg.arcane_errors.is_empty(), "{:?}", reg.arcane_errors);
    Arc::new(reg)
}

#[test]
fn gameplay_belt_depot_preserves_unaccepted_cargo() {
    let reg = proof_registry();
    let mut world = World::new(42, tmp_dir("proof-belt-depot"), reg.clone());
    let belt = bp(8, 200, 8);
    let depot = bp(8, 200, 9);
    world.ensure_chunk(belt.chunk());
    world.set_block_at(belt, b(&reg, "base:belt"));
    world.set_block_at(depot, AIR);
    assert!(world.place_block_at(depot, b(&reg, "proof:depot")));
    let clay = it(&reg, "base:clay_ball");
    assert_eq!(
        world.depot_deposit(depot, &ItemStack::new(&reg, clay, 63)),
        63
    );
    assert!(world.belt_insert_at(belt, ItemStack::new(&reg, clay, 2)));
    world.tick_entities(5.0);
    let Some(crate::world::BlockEntity::Depot(state)) = world.block_entity_at(&depot) else {
        panic!("depot remains present");
    };
    let stock: u32 = state.storage.iter().flatten().map(|s| s.count).sum();
    let drops: u32 = world
        .pending_drops()
        .iter()
        .filter(|(_, s)| s.item == clay)
        .map(|(_, s)| s.count)
        .sum();
    let riding: u32 = world
        .belt_cell_at(belt)
        .map_or(0, |s| s.cargo.iter().map(|s| s.count).sum());
    eprintln!(
        "PROOF belt depot: before staged=63 cargo=2; after staged={stock} dropped={drops} riding={riding}"
    );
    assert_eq!(
        (stock, drops, riding),
        (64, 1, 0),
        "surplus leaves the belt as a collectible item instead of disappearing"
    );
}

#[test]
fn dungeon_reset_preserves_overworld_den_spawning() {
    let reg = proof_registry();
    let mut world = World::new(42, tmp_dir("proof-reset-nests"), reg.clone());
    let nest = BlockPos::new(Face::PosZ, 8, 200, 8).unwrap();
    world.ensure_chunk(nest.chunk());
    for y in 200..=255 {
        world.set_block_at(BlockPos::new(Face::PosZ, 8, y, 8).unwrap(), AIR);
    }
    world.set_block_at(nest, b(&reg, "proof:nest"));
    let player = EntityPos::new(Face::PosZ, 8.5, 201.0, 8.5).unwrap();
    let wolf = reg.animal_id("proof:wolf").unwrap();
    let mut rng = 21;
    world.tick_nest_spawns(player, 0.0, 5.0, &mut rng);
    let before = world.mobs().iter().filter(|m| m.species == wolf).count();
    assert_eq!(
        before, 1,
        "the intact overworld den spawns before the dungeon visit"
    );
    world.mobs_mut().clear();
    let spawn = world
        .enter_dungeon(0, player, "proof:dungeon")
        .expect("dungeon opens");
    assert_eq!(world.exit_dungeon(0, spawn), Some(player));
    world.tick_dungeon_runs(4.0, 0);
    assert!(world.dungeon_runs.is_empty(), "the deserted run reset");
    assert_eq!(
        world.get_block_at(nest),
        b(&reg, "proof:nest"),
        "the overworld nest block was never broken"
    );
    world.tick_nest_spawns(player, 0.0, 5.0, &mut rng);
    let after = world.mobs().iter().filter(|m| m.species == wolf).count();
    eprintln!("PROOF dungeon reset: intact overworld den spawned before={before} after={after}");
    assert_eq!(
        after, 1,
        "visiting another face must not stop an intact overworld den from spawning"
    );
}
