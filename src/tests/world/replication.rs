//! Replication scenarios.

use super::*;

#[test]
fn replica_queries_do_not_materialize_authoritative_terrain() {
    let world = ReplicaWorld::new(7, base_reg(), 0.0);
    let position = bp(8, 90, 8);
    assert!(!world.has_chunk(position.chunk()));
    assert_eq!(world.get_block_at(position), AIR);
    let _ = world.weather_at_surface(position.surface());
    let _ = world.prospect_at(position.surface());
    assert_eq!(world.chunk_count(), 0);
    assert!(world.dirty_chunks().is_empty());
}

#[test]
fn replicated_block_burst_preserves_state_and_settles_shared_lighting() {
    let reg = base_reg();
    let mut world = ReplicaWorld::new(0, reg.clone(), 0.0);
    let torch = b(&reg, "base:torch");
    let water = b(&reg, "base:water");
    let torch_pos = bp(8, 90, 8);
    let water_pos = bp(9, 90, 8);
    let bytes = crate::world::encode_chunk_for_test(&crate::chunk::Chunk::new());
    world.insert_remote_chunks([(torch_pos.chunk(), bytes.as_slice())], &[AIR]);
    world
        .observations_mut()
        .set_arcane_cue([u8::MAX; 2], u8::MAX, None);
    assert_eq!(world.remote_arcane_cue(), [4; 2]);
    assert_eq!(world.remote_arcane_dominant(), 6);

    world.observations_mut().set_arcane_cue(
        [2, 1],
        4,
        Some(("Rainbells fold shut beside the marsh.".into(), true)),
    );
    let ecology = world.arcane_ecology().expect("remote ecology observation");
    assert_eq!(ecology.text, "Rainbells fold shut beside the marsh.");
    assert!(ecology.damped);
    world.observations_mut().extend_charges(vec![(41, 73)]);
    assert_eq!(world.inspectable_item_current(41), Some(73));

    world.apply_remote_block_states([(torch_pos, torch, 0, 0, 0), (water_pos, water, 3, 41, 0)]);

    assert_eq!(world.get_block_at(torch_pos), torch);
    assert_eq!(world.get_block_at(water_pos), water);
    assert_eq!(world.get_meta_at(water_pos), 3);
    assert_eq!(world.get_water_salt_at(water_pos), 41);
    assert!(
        world.light_rgb_at_pos(torch_pos).0[0] > 0,
        "the shared batch must finish its derived lighting before returning"
    );

    world.apply_remote_block_states([(torch_pos, AIR, 0, 0, 0)]);
    assert_eq!(world.light_rgb_at_pos(torch_pos).0, [0; 3]);
}
