//! Cancellation scenarios.

use super::*;

#[test]
fn cancellation_returns_clean_current_once_and_disorders_a_declared_remainder() {
    let (mut world, source, wand) = workings_world("workings-cancel");
    let target = source.offset(1, 0, 0).unwrap();
    let wand_owner = crate::arcane::ArcaneOwner::Item(wand.arcane_id);
    let before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .account(&wand_owner)
        .unwrap()
        .current
        .total();
    let started = world
        .begin_trace_working(
            [10; 16],
            "cancel-fixture",
            source,
            wand.arcane_id,
            target,
            20,
            false,
        )
        .unwrap();
    world.cancel_working(started.stable_id).unwrap();
    assert!(world.cancel_working(started.stable_id).is_err());
    let after = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .account(&wand_owner)
        .unwrap()
        .current
        .total();
    assert!(after < before);
    assert!(world.workings_state.as_ref().unwrap().active.is_empty());
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .unexplained_delta,
        0
    );
}

#[test]
fn early_release_cancels_instead_of_bypassing_host_settle_time() {
    let (mut world, source, wand) = workings_world("workings-early-release");
    let fuel = source.offset(2, 0, 0).unwrap();
    let fire_cell = fuel.offset(0, 1, 0).unwrap();
    world.set_block_authored_at(fuel, b(&world.reg, "base:log"), "early-release fuel");
    world.set_block_at(fire_cell, AIR);
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_wand_working(
            [11; 16],
            "impatient-worker",
            source,
            wand.arcane_id,
            "base:kindle",
            crate::workings::WorkingTargetIntent::Block {
                pos: fuel,
                adjacent: Some(fire_cell),
            },
            None,
            false,
        )
        .unwrap();

    let result = world.release_working(started.stable_id).unwrap();
    assert_eq!(result.cue, crate::workings::WorkingCueKind::Cancel);
    assert_eq!(world.get_block_at(fire_cell), AIR);
    assert_eq!(
        world
            .workings_state
            .as_ref()
            .unwrap()
            .history
            .iter()
            .filter(|event| event.id == started.stable_id)
            .count(),
        1
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        total_before
    );
}

#[test]
fn hosted_actor_death_or_disconnect_interrupts_every_reservation_exactly_once() {
    let (mut world, source, wand) = workings_world("workings-host-lifecycle");
    let actor = [38; 16];
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let first = world
        .begin_gleam_working(
            actor,
            "host-lifecycle-worker",
            source,
            wand.arcane_id,
            source,
            3,
            20 * 30,
            false,
        )
        .unwrap();
    settle_channel(&mut world, first.stable_id);

    let results = world.interrupt_actor_workings(actor).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].stable_id, first.stable_id);
    assert_eq!(results[0].cue, crate::workings::WorkingCueKind::Strain);
    assert!(world.interrupt_actor_workings(actor).unwrap().is_empty());
    assert!(world.interrupt_working(first.stable_id).is_err());
    assert_eq!(
        world
            .workings_state
            .as_ref()
            .unwrap()
            .history
            .iter()
            .filter(|event| event.id == first.stable_id)
            .count(),
        1
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        total_before
    );
}

#[test]
fn unloaded_wand_target_interrupts_once_without_rewriting_the_chunk() {
    let (mut world, source, wand) = workings_world("workings-unload-lifecycle");
    let target = source.offset(2, 0, 0).unwrap();
    world.set_block_at(target, AIR);
    let target_before = world.get_block_at(target);
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_gleam_working(
            [39; 16],
            "unload-worker",
            source,
            wand.arcane_id,
            target,
            3,
            20 * 30,
            false,
        )
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    world.unload_chunk(target.chunk());

    let first = world.tick_workings();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].0.stable_id, started.stable_id);
    assert_eq!(first[0].0.cue, crate::workings::WorkingCueKind::Strain);
    assert!(world.tick_workings().is_empty());
    world.ensure_chunk(target.chunk());
    assert_eq!(world.get_block_at(target), target_before);
    assert_eq!(
        world
            .workings_state
            .as_ref()
            .unwrap()
            .history
            .iter()
            .filter(|event| event.id == started.stable_id)
            .count(),
        1
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        total_before
    );
}
