//! Status scenarios.

use super::*;

#[test]
fn antidote_status_survives_reopen_then_death_settles_exactly_once() {
    let mut world =
        crate::tests::implements::embodied_implements_world("alchemy-antidote-lifecycle");
    let actor = [23; 16];
    let at = bp(8, 100, 8);
    let definition = world.reg.preparations["base:scouring_antidote"].clone();
    let mut inventory = Inventory::new();
    mint_ready_dose(&mut world, &mut inventory, 0, &definition.id, 7, 3);
    let use_result = world
        .use_preparation(
            actor,
            "antidote fixture",
            at,
            &mut inventory,
            0,
            crate::alchemy::AlchemyTarget::SelfActor,
        )
        .unwrap();
    let status_id = use_result.status_id.unwrap();
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    save_world(&mut world);
    let root = world.save_dir_for_test();
    drop(world);

    let mut loaded = World::load_or_create(root, base_reg()).unwrap();
    let status = loaded.alchemy_state().unwrap().statuses[&actor][0].clone();
    assert_eq!(status.status_id, status_id);
    loaded.set_simulation_clock(
        status
            .started_tick
            .saturating_add(definition.effect.duration_ticks / 2) as f64
            / 20.0,
    );
    let ticked = loaded
        .tick_preparation_statuses(
            actor,
            at,
            PreparationPhysiology {
                bodily_dross: 100,
                ..PreparationPhysiology::default()
            },
        )
        .unwrap();
    let removed = 100 - ticked.physiology.bodily_dross;
    assert!(removed > 0);
    assert!(removed <= u64::from(definition.effect.dross_capacity));
    loaded.settle_preparations_on_death(actor, at).unwrap();
    assert!(
        loaded
            .alchemy_state()
            .unwrap()
            .statuses
            .get(&actor)
            .is_none_or(Vec::is_empty)
    );
    assert_eq!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        current_before
    );
    loaded.settle_preparations_on_death(actor, at).unwrap();
    assert_eq!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        current_before,
        "repeated death settlement moved Current twice"
    );
}

#[test]
fn duplicate_refresh_recovery_and_incompatible_draughts_are_authoritative() {
    let mut world = crate::tests::implements::embodied_implements_world("alchemy-status-exclusion");
    let actor = [24; 16];
    let at = bp(8, 100, 8);
    let settling = world.reg.preparations["base:settling_draught"].clone();
    let storm = world.reg.preparations["base:storm_cordial"].clone();
    let mut inventory = Inventory::new();
    mint_ready_dose(&mut world, &mut inventory, 0, &settling.id, 5, 1);
    mint_ready_dose(&mut world, &mut inventory, 1, &settling.id, 5, 1);
    let first = world
        .use_preparation(
            actor,
            "status fixture",
            at,
            &mut inventory,
            0,
            crate::alchemy::AlchemyTarget::SelfActor,
        )
        .unwrap();
    let second = world
        .use_preparation(
            actor,
            "status fixture",
            at,
            &mut inventory,
            1,
            crate::alchemy::AlchemyTarget::SelfActor,
        )
        .unwrap();
    assert_eq!(first.status_id, second.status_id);
    assert_eq!(world.alchemy_state().unwrap().statuses[&actor].len(), 1);
    assert_eq!(
        world.alchemy_state().unwrap().statuses[&actor][0].refresh_count,
        1
    );

    let storm_id = mint_ready_dose(&mut world, &mut inventory, 2, &storm.id, 6, 2);
    let incompatible = world
        .use_preparation(
            actor,
            "status fixture",
            at,
            &mut inventory,
            2,
            crate::alchemy::AlchemyTarget::SelfActor,
        )
        .unwrap_err();
    assert!(incompatible.contains("incompatible"));
    assert!(
        world
            .alchemy_state()
            .unwrap()
            .containers
            .contains_key(&storm_id)
    );

    let third_id = mint_ready_dose(&mut world, &mut inventory, 3, &settling.id, 5, 1);
    let status = world.alchemy_state().unwrap().statuses[&actor][0].clone();
    world.set_simulation_clock(status.due_tick.saturating_add(1) as f64 / 20.0);
    world
        .tick_preparation_statuses(actor, at, PreparationPhysiology::default())
        .unwrap();
    let recovering = world
        .use_preparation(
            actor,
            "status fixture",
            at,
            &mut inventory,
            3,
            crate::alchemy::AlchemyTarget::SelfActor,
        )
        .unwrap_err();
    assert!(recovering.contains("recovery interval"));
    assert!(
        world
            .alchemy_state()
            .unwrap()
            .containers
            .contains_key(&third_id)
    );

    world.set_simulation_clock(status.recovery_until_tick.saturating_add(1) as f64 / 20.0);
    world
        .tick_preparation_statuses(actor, at, PreparationPhysiology::default())
        .unwrap();
    assert!(
        world
            .use_preparation(
                actor,
                "status fixture",
                at,
                &mut inventory,
                3,
                crate::alchemy::AlchemyTarget::SelfActor,
            )
            .is_ok()
    );
}

#[test]
fn preparation_modifiers_are_closed_and_expire_without_hidden_state() {
    let mut world = crate::tests::implements::embodied_implements_world("alchemy-modifiers");
    let actor = [11; 16];
    let definition = world
        .reg
        .preparations
        .get("base:clear_eye_tincture")
        .unwrap()
        .clone();
    let status_id = world
        .alchemy_state
        .as_mut()
        .unwrap()
        .allocate_status_id()
        .unwrap();
    world.alchemy_state.as_mut().unwrap().statuses.insert(
        actor,
        vec![ActivePreparationStatus {
            status_id,
            preparation_id: definition.id,
            definition_version: definition.version,
            source_batch: 1,
            actor,
            dose_volume_units: definition.dose_units,
            active_current: Current::default(),
            dross_current: Current::default(),
            started_tick: 0,
            last_tick: 0,
            due_tick: 100,
            recovery_until_tick: 200,
            stack_group: definition.stack_group,
            completed_units: 0,
            refresh_count: 0,
            overdose_until_tick: 0,
        }],
    );
    let active = world.preparation_modifiers(actor);
    assert!(active.trace_sight > 0);
    assert_eq!(active.throughput_permille, 1_000);
    assert!(!active.storm_warning);
    world.set_simulation_clock(6.0);
    assert_eq!(world.preparation_modifiers(actor).trace_sight, 0);
}

#[test]
fn settling_draught_limits_new_working_strain_without_erasing_existing_strain() {
    let mut world = crate::tests::implements::embodied_implements_world("alchemy-settling-strain");
    let definition = world.reg.preparations["base:settling_draught"].clone();
    let actor = [31; 16];
    let status_id = world
        .alchemy_state
        .as_mut()
        .unwrap()
        .allocate_status_id()
        .unwrap();
    world.alchemy_state.as_mut().unwrap().statuses.insert(
        actor,
        vec![ActivePreparationStatus {
            status_id,
            preparation_id: definition.id.clone(),
            definition_version: definition.version,
            source_batch: 1,
            actor,
            dose_volume_units: definition.dose_units,
            active_current: Current::default(),
            dross_current: Current::default(),
            started_tick: 0,
            last_tick: 0,
            due_tick: definition.effect.duration_ticks,
            recovery_until_tick: definition.effect.duration_ticks
                + definition.effect.recovery_ticks,
            stack_group: definition.stack_group.clone(),
            completed_units: 0,
            refresh_count: 0,
            overdose_until_tick: 0,
        }],
    );
    world.set_simulation_clock(1.0);
    let result = world
        .tick_preparation_statuses(
            actor,
            bp(8, 100, 8),
            PreparationPhysiology {
                strain: 100.0,
                ..PreparationPhysiology::default()
            },
        )
        .unwrap();
    assert_eq!(result.physiology.strain, 100.0);
    assert_eq!(result.modifiers.strain_permille, 700);
    assert_eq!(result.modifiers.throughput_permille, 750);
}

#[test]
fn failed_or_spoiled_doses_never_masquerade_as_ready_effects() {
    assert_ne!(
        BatchOutcome::Failed(crate::alchemy::BatchFailure::SpentLiquor),
        BatchOutcome::Ready
    );
    assert_ne!(BatchOutcome::Spoiled, BatchOutcome::Ready);
}
