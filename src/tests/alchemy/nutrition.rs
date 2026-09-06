//! Nutrition scenarios.

use super::*;

#[test]
fn hearth_healing_pays_hunger_and_nutrients_as_health_moves() {
    let mut world = crate::tests::implements::embodied_implements_world("alchemy-hearth-effect");
    let definition = world
        .reg
        .preparations
        .get("base:hearth_tonic")
        .unwrap()
        .clone();
    let actor = [9; 16];
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
    world.set_simulation_clock(definition.effect.duration_ticks as f64 / 40.0);
    let result = world
        .tick_preparation_statuses(
            actor,
            bp(8, 100, 8),
            PreparationPhysiology {
                health: 6.0,
                max_health: 14.0,
                hunger: 10.0,
                nutrition: [10.0; 5],
                strain: 0.0,
                bodily_dross: 0,
            },
        )
        .unwrap();
    assert!(result.physiology.health > 6.0);
    assert!(result.physiology.hunger < 10.0);
    assert!(result.physiology.nutrition.iter().sum::<f32>() < 50.0);

    let starving = world
        .tick_preparation_statuses(
            actor,
            bp(8, 100, 8),
            PreparationPhysiology {
                health: 2.0,
                max_health: 14.0,
                hunger: 0.0,
                nutrition: [0.0; 5],
                strain: 0.0,
                bodily_dross: 0,
            },
        )
        .unwrap();
    assert_eq!(starving.physiology.health, 2.0);
}

#[test]
fn hearth_overdose_is_one_bounded_sickness_independent_of_tick_cadence() {
    let mut world = crate::tests::implements::embodied_implements_world("alchemy-hearth-overdose");
    let actor = [44; 16];
    let at = bp(8, 100, 8);
    let definition = world.reg.preparations["base:hearth_tonic"].clone();
    let mut inventory = Inventory::new();
    mint_ready_dose(&mut world, &mut inventory, 0, &definition.id, 5, 1);
    mint_ready_dose(&mut world, &mut inventory, 1, &definition.id, 5, 1);
    for slot in [0, 1] {
        world
            .use_preparation(
                actor,
                "hearth overdose fixture",
                at,
                &mut inventory,
                slot,
                crate::alchemy::AlchemyTarget::SelfActor,
            )
            .unwrap();
    }
    let status = &world.alchemy_state().unwrap().statuses[&actor][0];
    assert_eq!(world.alchemy_state().unwrap().statuses[&actor].len(), 1);
    assert_eq!(status.refresh_count, 1);
    assert_eq!(
        status
            .overdose_until_tick
            .saturating_sub(status.started_tick),
        600
    );
    let sickness_end = status.overdose_until_tick;

    // One delayed host tick crossing the entire sickness interval must pay
    // exactly the same bounded cost as many small live-play ticks.
    world.set_simulation_clock(sickness_end.saturating_add(200) as f64 / 20.0);
    let result = world
        .tick_preparation_statuses(
            actor,
            at,
            PreparationPhysiology {
                health: 14.0,
                max_health: 14.0,
                hunger: 10.0,
                nutrition: [10.0; 5],
                strain: 0.0,
                bodily_dross: 0,
            },
        )
        .unwrap();
    assert!((result.physiology.hunger - 9.5).abs() < f32::EPSILON);
    assert_eq!(result.physiology.health, 14.0);

    world.set_simulation_clock(sickness_end.saturating_add(400) as f64 / 20.0);
    let settled = world
        .tick_preparation_statuses(actor, at, result.physiology)
        .unwrap();
    assert!((settled.physiology.hunger - 9.5).abs() < f32::EPSILON);
}
