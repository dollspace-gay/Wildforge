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
fn gameplay_depot_transfer_is_atomic_for_accepted_and_refused_goods() {
    let reg = proof_registry();
    for (case, name, held, staged, accepted) in [
        ("whole", "base:clay_ball", 32, 0, 32),
        ("partial", "base:clay_ball", 16, 60, 4),
        ("full", "base:clay_ball", 8, 64, 0),
        ("unwanted", "base:cobblestone", 8, 0, 0),
        ("bread", "base:bread", 8, 0, 8),
    ] {
        let mut world = World::new(42, tmp_dir(&format!("proof-transfer-{case}")), reg.clone());
        let pos = bp(8, 200, 8);
        world.ensure_chunk(pos.chunk());
        world.set_block_at(pos, AIR);
        assert!(world.place_block_at(pos, b(&reg, "proof:depot")));
        let item = it(&reg, name);
        if staged > 0 {
            assert_eq!(
                world.depot_deposit(pos, &ItemStack::new(&reg, item, staged)),
                staged
            );
        }
        let mut inventory = Inventory::new();
        inventory.slots[0] = Some(ItemStack::new(&reg, item, 7));
        inventory.slots[3] = Some(ItemStack::new(&reg, item, held));
        let receipt = world.deliver_to_depot(pos, &mut inventory, 3);
        assert_eq!(receipt.as_ref().map_or(0, |r| r.2), accepted, "{case}");
        assert_eq!(
            inventory.slots[0].map(|s| s.count),
            Some(7),
            "{case}: unrelated slot remains intact"
        );
        assert_eq!(
            inventory.slots[3].map_or(0, |s| s.count),
            held - accepted,
            "{case}: exact source debit"
        );
        let Some(crate::world::BlockEntity::Depot(state)) = world.block_entity_at(&pos) else {
            panic!("depot remains")
        };
        assert_eq!(
            state.storage.iter().flatten().map(|s| s.count).sum::<u32>(),
            staged + accepted,
            "{case}: exact destination credit"
        );
    }
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
    world.replace_mobs(Vec::new());
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
