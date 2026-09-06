//! Block edits scenarios.

use super::*;

#[test]
fn block_edit_fans_out_through_one_authoritative_boundary() {
    use crate::world::{BlockEntity, ChestState};

    let reg = base_reg();
    let mut w = test_world_with("edit-side-effects", reg.clone());
    let here = tchunk(0, 0);
    let west = tchunk(-1, 0);
    for pos in [here, west] {
        let chunk = w.chunks_mut().get_mut(&pos).unwrap();
        chunk.dirty = false;
        chunk.modified = false;
    }

    let stone = b(&reg, "base:stone");
    w.set_edit_logging(true);
    w.set_block(0, 200, 4, stone);
    assert_eq!(w.get_block(0, 200, 4), stone);
    assert!(w.chunks()[&here].dirty && w.chunks()[&west].dirty);
    assert_eq!(
        w.edits().last(),
        Some(&(
            crate::planet::BlockPos::of_world(0, 200, 4).unwrap(),
            stone,
            0,
            0,
            0
        ))
    );

    let chest = b(&reg, "base:chest");
    let stick = it(&reg, "base:stick");
    let mut state = ChestState::default();
    state.slots[0] = Some(ItemStack::new(&reg, stick, 3));
    w.set_block(2, 200, 4, chest);
    w.insert_block_entity((2, 200, 4), BlockEntity::Chest(state));
    w.set_block(2, 200, 4, AIR);
    assert!(!w.has_block_entity(&(2, 200, 4)));
    assert!(w.pending_drops().iter().any(|(pos, stack)| {
        *pos == crate::planet::BlockPos::of_world(2, 200, 4).unwrap()
            && stack.item == stick
            && stack.count == 3
    }));

    let sand = b(&reg, "base:sand");
    w.set_block(4, 202, 4, sand);
    assert_eq!(w.get_block(4, 202, 4), AIR);
    assert!(
        w.falling_blocks()
            .iter()
            .any(|falling| falling.block == sand)
    );
}

#[test]
fn set_block_roundtrip_and_cross_chunk_access() {
    let reg = base_reg();
    let mut w = test_world_with("set", reg.clone());
    let planks = b(&reg, "base:planks");
    w.set_block(3, 70, 3, planks);
    assert_eq!(w.get_block(3, 70, 3), planks);
    w.set_block(-1, 70, -1, b(&reg, "base:cobblestone"));
    assert_eq!(w.get_block(-1, 70, -1), b(&reg, "base:cobblestone"));
}

#[test]
fn material_checkpoint_failure_cancels_voxel_placement() {
    let reg = base_reg();
    let dir = tmp_dir("material-checkpoint-cancel").join("world");
    crate::world::create_world_fixture_atomic(
        &dir,
        42,
        "survival",
        8,
        &crate::planet_atlas::CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    let mut world = World::load_or_create(dir.clone(), reg.clone()).unwrap();
    let chunk = tchunk(0, 0);
    world.ensure_chunk(chunk);
    let surface = crate::planet::SurfacePos::new(
        chunk.face(),
        chunk.u() * crate::chunk::CHUNK_X as u16 + crate::chunk::CHUNK_X as u16 / 2,
        chunk.v() * crate::chunk::CHUNK_Z as u16 + crate::chunk::CHUNK_Z as u16 / 2,
    )
    .unwrap();
    let target = block_pos(surface, world.surface_height_at(surface) + 1);
    assert_eq!(world.get_block_at(target), AIR);
    let break_target = target.offset(0, 1, 0).unwrap();
    let copper = reg.block_id("base:copper_block").unwrap();
    world.set_block_at(break_target, copper);

    // An existing directory cannot be atomically replaced by a ledger file.
    // This deterministically exercises the same failed-checkpoint path as a
    // Windows sharing/access denial without depending on host permissions.
    let blocked = dir.join("blocked-ledger-path");
    std::fs::create_dir(&blocked).unwrap();
    world
        .material_ledger
        .as_mut()
        .unwrap()
        .force_checkpoint_failure_at(blocked);
    assert!(!world.place_block_at(target, copper));
    assert_eq!(
        world.get_block_at(target),
        AIR,
        "a failed material journal must leave the voxel untouched"
    );
    assert!(
        world
            .break_block_at(break_target, None, true, true)
            .is_none()
    );
    assert_eq!(
        world.get_block_at(break_target),
        copper,
        "a failed material journal must not remove the voxel"
    );
    assert!(
        !world.player_touched.contains(&chunk),
        "a cancelled action must not suppress safe retrogen"
    );
}

#[test]
fn blocks_place_into_water_and_never_vanish_on_refusal() {
    let reg = base_reg();
    let mut w = test_world_with("place-water", reg.clone());
    let h = w.surface_height(4, 4);
    let stone = b(&reg, "base:stone");
    let water = reg.water_block(0);
    // A block takes a water cell — the water is displaced, not the
    // player's hand. (The bug: place_block refused any non-AIR cell,
    // so the click spent the item and put nothing down.)
    w.set_block(4, h + 1, 4, water);
    assert!(w.place_block((4, h + 1, 4), stone), "stone displaces water");
    assert_eq!(w.get_block(4, h + 1, 4), stone);
    // Thin layers give way the same, and air of course.
    if let Some(layer) = reg.block_id("base:snow_layer") {
        w.set_block(5, h + 1, 5, layer);
        assert!(w.place_block((5, h + 1, 5), stone), "stone over a drift");
    }
    w.set_block(6, h + 1, 6, AIR);
    assert!(w.place_block((6, h + 1, 6), stone));
    // What stands does NOT give way: a placement must never quietly
    // eat a crop, and refusal must be honest so callers keep the item.
    let crop = b(&reg, "base:wheat_seeds");
    w.set_block(7, h + 1, 7, crop);
    assert!(!w.place_block((7, h + 1, 7), stone), "the wheat stands");
    assert_eq!(w.get_block(7, h + 1, 7), crop);
    assert!(!w.place_block((8, h + 1, 8), stone) || w.get_block(8, h + 1, 8) == stone);
    // And the registry's own account of what gives way.
    assert!(reg.is_replaceable(AIR));
    assert!(reg.is_replaceable(water));
    assert!(!reg.is_replaceable(stone));
    assert!(!reg.is_replaceable(crop));
}
