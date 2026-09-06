//! Trace scenarios.

use super::*;

#[test]
fn trace_requires_the_real_fitted_lens_and_one_wand_cannot_double_channel() {
    let (mut world, source, wand) = workings_world("workings-trace-lens-authority");
    let empty = Inventory::new();
    let request = crate::workings::WorkingTargetIntent::None;
    let refusal = world
        .begin_wand_working(
            [41; 16],
            "lensless-worker",
            source,
            wand.arcane_id,
            "base:trace",
            request,
            Some(&empty),
            false,
        )
        .unwrap_err();
    assert!(refusal.contains("fitted tuning lens"), "{refusal}");

    let mut inventory = Inventory::new();
    inventory.slots[2] = Some(fitted_lens_stack(&mut world));
    let started = world
        .begin_wand_working(
            [41; 16],
            "equipped-worker",
            source,
            wand.arcane_id,
            "base:trace",
            request,
            Some(&inventory),
            false,
        )
        .unwrap();
    let other_target = source.offset(1, 0, 0).unwrap();
    world.set_block_at(other_target, AIR);
    let conflict = world
        .begin_gleam_working(
            [42; 16],
            "competing-worker",
            source,
            wand.arcane_id,
            other_target,
            1,
            20,
            false,
        )
        .unwrap_err();
    assert!(conflict.contains("apparatus or target is already reserved"));
    world.cancel_working(started.stable_id).unwrap();
    let refusal = world
        .begin_wand_working(
            [41; 16],
            "focus-worker",
            source,
            wand.arcane_id,
            "base:gleam",
            crate::workings::WorkingTargetIntent::Block {
                pos: other_target,
                adjacent: None,
            },
            Some(&inventory),
            false,
        )
        .unwrap_err();
    assert!(refusal.contains("physical apparatus focus"));
}

#[test]
fn trace_reports_only_bounded_local_drift_leakage_and_recent_working_evidence() {
    let (mut world, source, wand) = workings_world("workings-trace-reading");
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let fuel = source.offset(2, 0, 0).unwrap();
    let fire_cell = fuel.offset(0, 1, 0).unwrap();
    world.set_block_authored_at(fuel, b(&world.reg, "base:log"), "trace fixture fuel");
    world.set_block_at(fire_cell, AIR);
    let prior = world
        .begin_kindle_working(
            [21; 16],
            "prior-worker",
            source,
            wand.arcane_id,
            fuel,
            fire_cell,
            false,
        )
        .unwrap();
    world.complete_working(prior.stable_id).unwrap();

    let started = world
        .begin_trace_working(
            [22; 16],
            "trace-worker",
            source,
            wand.arcane_id,
            fuel,
            20 * 30,
            false,
        )
        .unwrap();
    let reading = settle_channel(&mut world, started.stable_id);
    assert!(reading.message.contains("Trace resolves"));
    assert!(reading.message.contains("drift"));
    assert!(reading.message.contains("one faint recent working trace"));
    assert!(reading.message.contains("charge leak"));
    assert!(reading.message.contains("uncertainty 5%"));
    assert!(!reading.message.contains("prior-worker"));
    assert!(!reading.message.contains("Kindle"));
    assert!(!reading.message.contains("base:kindle"));

    let history = &world.workings_state.as_ref().unwrap().history;
    let prior_event = history
        .iter()
        .find(|event| event.id == prior.stable_id)
        .unwrap();
    assert_eq!(prior_event.source, source);
    assert_eq!(prior_event.path, vec![fuel, fire_cell]);
    world.release_working(started.stable_id).unwrap();
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
}
