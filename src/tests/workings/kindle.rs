//! Kindle scenarios.

use super::*;

#[test]
fn kindle_reserves_current_then_uses_ordinary_player_fire_provenance() {
    let (mut world, source, wand) = workings_world("workings-kindle");
    let fuel = source.offset(2, 0, 0).unwrap();
    let fire_cell = fuel.offset(0, 1, 0).unwrap();
    world.set_block_authored_at(fuel, b(&world.reg, "base:log"), "kindle fixture fuel");
    world.set_block_at(fire_cell, AIR);
    let before_total = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;

    let started = world
        .begin_kindle_working(
            [7; 16],
            "kindle-fixture",
            source,
            wand.arcane_id,
            fuel,
            fire_cell,
            false,
        )
        .unwrap();
    let reserved = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .account(&crate::arcane::ArcaneOwner::Working(started.stable_id))
        .unwrap()
        .current
        .clone();
    assert_eq!(
        reserved,
        world.workings_state.as_ref().unwrap().active[&started.stable_id].reserved_current
    );

    world.complete_working(started.stable_id).unwrap();
    assert_eq!(world.get_block_at(fire_cell), b(&world.reg, "base:fire"));
    assert_ne!(world.get_meta_at(fire_cell) & 0x80, 0);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Working(started.stable_id))
            .is_none()
    );
    let after = world.arcane_ledger.as_ref().unwrap().audit().unwrap();
    assert_eq!(after.total, before_total);
    assert_eq!(after.unexplained_delta, 0);
    let event = world
        .workings_state
        .as_ref()
        .unwrap()
        .history
        .iter()
        .find(|event| event.id == started.stable_id)
        .unwrap();
    assert_eq!(
        event.actor, [7; 16],
        "harmful fire keeps stable player identity"
    );
}
