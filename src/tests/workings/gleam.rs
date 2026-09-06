//! Gleam scenarios.

use super::*;

#[test]
fn gleam_is_a_releasable_temporary_cue_and_never_places_a_light_block() {
    let (mut world, source, wand) = workings_world("workings-gleam");
    let target = source.offset(3, 0, 0).unwrap();
    world.set_block_at(target, AIR);
    let before = world.get_block_at(target);
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_gleam_working(
            [27; 16],
            "gleam-worker",
            source,
            wand.arcane_id,
            target,
            6,
            20 * 30,
            false,
        )
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    let cue = world
        .working_cues()
        .into_iter()
        .find(|cue| cue.stable_id == started.stable_id)
        .unwrap();
    assert_eq!(cue.handler, crate::workings::WorkingHandler::Gleam);
    assert_eq!(cue.path, vec![source, target]);
    assert_eq!(world.get_block_at(target), before);
    world.release_working(started.stable_id).unwrap();
    assert!(
        world
            .working_cues()
            .iter()
            .all(|cue| cue.stable_id != started.stable_id)
    );
    assert_eq!(world.get_block_at(target), before);
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );

    let ranged = world
        .begin_gleam_working(
            [27; 16],
            "gleam-range-worker",
            source,
            wand.arcane_id,
            target,
            2,
            20 * 30,
            false,
        )
        .unwrap();
    settle_channel(&mut world, ranged.stable_id);
    let far_actor = source.offset(20, 0, 0).unwrap();
    assert!(!world.wand_working_reachable_from(ranged.stable_id, far_actor));
    world.interrupt_working(ranged.stable_id).unwrap();

    let (mut depleted_world, depleted_source, depleted_wand) =
        workings_world("workings-gleam-depletion");
    let depleted_target = depleted_source.offset(2, 0, 0).unwrap();
    depleted_world.set_block_at(depleted_target, AIR);
    let depleted_total = depleted_world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .total;
    let depleted = depleted_world
        .begin_gleam_working(
            [27; 16],
            "gleam-depletion-worker",
            depleted_source,
            depleted_wand.arcane_id,
            depleted_target,
            2,
            20,
            false,
        )
        .unwrap();
    settle_channel(&mut depleted_world, depleted.stable_id);
    let due = depleted_world.workings_state.as_ref().unwrap().active[&depleted.stable_id].due_tick;
    depleted_world.set_simulation_clock(due as f64 / 20.0 + 0.01);
    let outcomes = depleted_world.tick_workings();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].0.stable_id, depleted.stable_id);
    assert_eq!(outcomes[0].0.cue, crate::workings::WorkingCueKind::Complete);
    assert_eq!(
        depleted_world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        depleted_total
    );
}
