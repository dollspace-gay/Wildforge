//! Recovery scenarios.

use super::*;

#[test]
fn crash_after_current_settlement_replays_world_effect_exactly_once_on_restart() {
    let (mut world, dir, source, wand) = persistent_workings_world("workings-crash-replay");
    let fuel = source.offset(2, 0, 0).unwrap();
    let fire_cell = fuel.offset(0, 1, 0).unwrap();
    world.set_block_authored_at(fuel, b(&world.reg, "base:log"), "crash fixture fuel");
    world.set_block_at(fire_cell, AIR);
    save_world(&mut world);
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_kindle_working(
            [35; 16],
            "crash-worker",
            source,
            wand.arcane_id,
            fuel,
            fire_cell,
            false,
        )
        .unwrap();
    world.fail_chunk_save_for_test(fire_cell.chunk(), true);
    let error = world.complete_working(started.stable_id).unwrap_err();
    assert!(error.contains("injected chunk save failure"));
    let pending = &world.workings_state.as_ref().unwrap().active[&started.stable_id];
    assert_eq!(pending.phase, crate::workings::WorkingPhase::PendingApply);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Working(started.stable_id))
            .is_none(),
        "Current settlement committed before the idempotent world half"
    );
    drop(world);

    let mut reloaded = World::load_or_create(dir, base_reg()).unwrap();
    reloaded.ensure_chunk(fire_cell.chunk());
    assert_eq!(
        reloaded.get_block_at(fire_cell),
        b(&reloaded.reg, "base:fire")
    );
    assert!(
        !reloaded
            .workings_state
            .as_ref()
            .unwrap()
            .active
            .contains_key(&started.stable_id)
    );
    assert_eq!(
        reloaded
            .workings_state
            .as_ref()
            .unwrap()
            .history
            .iter()
            .filter(|event| event.id == started.stable_id)
            .count(),
        1
    );
    assert!(reloaded.complete_working(started.stable_id).is_err());
    assert_eq!(
        reloaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        total_before
    );
}

#[test]
fn restart_interrupts_unowned_wand_channel_once_and_conserves_its_reservation() {
    let (mut world, dir, source, wand) = persistent_workings_world("workings-restart-interrupt");
    let target = source.offset(2, 0, 0).unwrap();
    world.set_block_at(target, AIR);
    save_world(&mut world);
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_gleam_working(
            [36; 16],
            "restart-worker",
            source,
            wand.arcane_id,
            target,
            4,
            20 * 30,
            false,
        )
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Working(started.stable_id))
            .is_some()
    );
    drop(world);

    let reloaded = World::load_or_create(dir, base_reg()).unwrap();
    assert!(
        !reloaded
            .workings_state
            .as_ref()
            .unwrap()
            .active
            .contains_key(&started.stable_id)
    );
    let events = reloaded
        .workings_state
        .as_ref()
        .unwrap()
        .history
        .iter()
        .filter(|event| event.id == started.stable_id)
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].outcome, "interrupted");
    assert!(
        reloaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Working(started.stable_id))
            .is_none()
    );
    assert_eq!(
        reloaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        total_before
    );
}
