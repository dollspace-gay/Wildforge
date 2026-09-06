//! Rituals scenarios.

use super::*;

#[test]
fn transfer_circle_moves_only_exact_adjacent_charge_with_lower_bounded_loss() {
    let (mut world, controller, first_pos, second_pos, _) =
        ritual_world("workings-transfer-circle");
    let first = vessel_stack_at(&world, first_pos);
    let second = vessel_stack_at(&world, second_pos);
    charge_fixture_vessel(&mut world, first.arcane_id, 128);
    crate::tests::implements::drain_fixture_item_to_spark(&mut world, second_pos, second.arcane_id);
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let first_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_current_total(first.arcane_id)
        .unwrap();
    let second_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_current_total(second.arcane_id)
        .unwrap();
    assert!(first_before > second_before);

    let started = world
        .begin_transfer_circle_ritual([30; 16], "circle-worker", controller, 32)
        .unwrap();
    let payload = match &world.workings_state.as_ref().unwrap().active[&started.stable_id].effect {
        crate::workings::WorkingEffect::TransferCurrent { current, .. } => current.total(),
        _ => panic!("transfer circle reserved the wrong typed effect"),
    };
    settle_channel(&mut world, started.stable_id);
    world.set_simulation_clock(5.0);
    let outcomes = world.tick_workings();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].0.cue, crate::workings::WorkingCueKind::Complete);
    let second_after = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_current_total(second.arcane_id)
        .unwrap();
    assert_eq!(second_after, second_before + payload);
    assert_eq!(
        vessel_stack_at(&world, first_pos).arcane_id,
        first.arcane_id
    );
    assert_eq!(
        vessel_stack_at(&world, second_pos).arcane_id,
        second.arcane_id
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        total_before
    );
    assert!(payload <= 32);
}

#[test]
fn rooting_bed_advances_loaded_or_unloaded_only_after_reserving_real_budgets() {
    let (mut world, controller, first_pos, _, _) = ritual_world("workings-rooting-bed");
    let source_vessel = vessel_stack_at(&world, first_pos);
    charge_fixture_vessel(&mut world, source_vessel.arcane_id, 512);
    let post = controller.offset(1, 0, -1).unwrap();
    world.set_block_authored_at(
        post,
        b(&world.reg, "base:containment_post"),
        "rooting focus post",
    );
    let plant = controller.offset(2, 0, 2).unwrap();
    let soil = plant.offset(0, -1, 0).unwrap();
    let water = plant.offset(-1, 0, 0).unwrap();
    let seed = b(&world.reg, "base:wheat_seeds");
    let next = world.reg.block(seed).crop_next.unwrap();
    world.set_block_meta_at(
        soil,
        b(&world.reg, "base:farmland"),
        crate::world::soil::soil_meta(42, 0),
    );
    world.set_block_at(plant, seed);
    world.set_block_water_at(water, world.reg.water_block(0), 0, 0);
    let water_before = world.water_mass_at(water).unwrap().water_hu;
    let fertility_before = world.fertility_at_pos(soil);
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;

    let started = world
        .begin_rooting_bed_ritual([31; 16], "bed-worker", controller)
        .unwrap();
    assert_eq!(
        world.get_block_at(plant),
        seed,
        "reservation is not early growth"
    );
    assert_eq!(world.water_mass_at(water).unwrap().water_hu, water_before);
    assert_eq!(world.fertility_at_pos(soil), fertility_before);
    settle_channel(&mut world, started.stable_id);
    save_world(&mut world);
    world.unload_chunk(plant.chunk());
    assert!(world.chunk(plant.chunk()).is_none());
    world.set_simulation_clock(13.0);
    let outcomes = world.tick_workings();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].0.stable_id, started.stable_id);
    assert_eq!(
        outcomes[0].0.cue,
        crate::workings::WorkingCueKind::Complete,
        "{}",
        outcomes[0].0.message
    );
    assert_eq!(world.get_block_at(plant), next);
    assert_eq!(
        world.water_mass_at(water).unwrap().water_hu,
        water_before - 32
    );
    assert_eq!(world.fertility_at_pos(soil), fertility_before - 1);
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
}

#[test]
fn active_unloaded_ritual_resumes_after_process_restart_and_settles_once() {
    let (mut world, controller, first_pos, _, _) = ritual_world("workings-ritual-restart");
    let dir = world.save_dir_for_saving();
    let source_vessel = vessel_stack_at(&world, first_pos);
    charge_fixture_vessel(&mut world, source_vessel.arcane_id, 512);
    let post = controller.offset(1, 0, -1).unwrap();
    world.set_block_authored_at(
        post,
        b(&world.reg, "base:containment_post"),
        "restart rooting focus post",
    );
    let plant = controller.offset(2, 0, 2).unwrap();
    let soil = plant.offset(0, -1, 0).unwrap();
    let water = plant.offset(-1, 0, 0).unwrap();
    let seed = b(&world.reg, "base:wheat_seeds");
    let next = world.reg.block(seed).crop_next.unwrap();
    world.set_block_meta_at(
        soil,
        b(&world.reg, "base:farmland"),
        crate::world::soil::soil_meta(42, 0),
    );
    world.set_block_at(plant, seed);
    world.set_block_water_at(water, world.reg.water_block(0), 0, 0);
    save_world(&mut world);

    let water_before = world.water_mass_at(water).unwrap().water_hu;
    let fertility_before = world.fertility_at_pos(soil);
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_rooting_bed_ritual([44; 16], "restart-bed-worker", controller)
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    let due_tick = world.workings_state.as_ref().unwrap().active[&started.stable_id].due_tick;
    drop(world);

    let mut reloaded = World::load_or_create(dir, base_reg()).unwrap();
    let resumed = &reloaded.workings_state.as_ref().unwrap().active[&started.stable_id];
    assert_eq!(resumed.phase, crate::workings::WorkingPhase::Active);
    assert_eq!(resumed.due_tick, due_tick);
    assert!(reloaded.chunk(plant.chunk()).is_none());
    reloaded.set_simulation_clock(due_tick as f64 / 20.0 + 0.1);
    let outcomes = reloaded.tick_workings();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].0.stable_id, started.stable_id);
    assert_eq!(outcomes[0].0.cue, crate::workings::WorkingCueKind::Complete);
    assert_eq!(reloaded.get_block_at(plant), next);
    assert_eq!(
        reloaded.water_mass_at(water).unwrap().water_hu,
        water_before - 32
    );
    assert_eq!(reloaded.fertility_at_pos(soil), fertility_before - 1);
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
        current_before
    );
}

#[test]
fn ward_overload_is_deterministic_exhausts_supply_and_never_changes_ire() {
    let (mut world, controller, first_pos, _, _) = ritual_world("workings-ward-overload");
    let source_vessel = vessel_stack_at(&world, first_pos);
    charge_fixture_vessel(&mut world, source_vessel.arcane_id, 768);
    let conductor = b(&world.reg, "base:arcane_conductor");
    let mut boundary = Vec::new();
    for du in -3i32..=3 {
        for dv in -3i32..=3 {
            if du.abs().max(dv.abs()) == 3 {
                let pos = controller.offset(du, 0, dv).unwrap();
                world.set_block_authored_at(pos, conductor, "ward overload boundary");
                boundary.push(pos);
            }
        }
    }
    world.add_ire(80.0);
    let ire_before = world.ire;
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_ward_boundary_ritual([45; 16], "overload-worker", controller)
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    assert!(!world.resist_supernatural_pressure_at(controller, "warden", u64::MAX));
    assert!(
        !world
            .workings_state
            .as_ref()
            .unwrap()
            .active
            .contains_key(&started.stable_id)
    );
    let events = world
        .workings_state
        .as_ref()
        .unwrap()
        .history
        .iter()
        .filter(|event| event.id == started.stable_id)
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].outcome, "interrupted");
    assert_eq!(world.ire, ire_before);
    assert!(
        boundary
            .iter()
            .all(|pos| world.get_block_at(*pos) == conductor)
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
}

#[test]
fn settling_rite_slows_one_process_routes_dross_and_wears_the_real_vessel() {
    let (mut world, controller, first_pos, second_pos, wand) = ritual_world("workings-settling");
    let source_vessel = vessel_stack_at(&world, first_pos);
    charge_fixture_vessel(&mut world, source_vessel.arcane_id, 512);
    let source = controller.offset(0, 2, 0).unwrap();
    world.set_block_at(source, AIR);
    let target = source.offset(2, 0, 0).unwrap();
    world.set_block_at(target, AIR);
    let process = world
        .begin_gleam_working(
            [33; 16],
            "process-worker",
            source,
            wand.arcane_id,
            target,
            4,
            20 * 30,
            false,
        )
        .unwrap();
    settle_channel(&mut world, process.stable_id);
    let due_before = world.workings_state.as_ref().unwrap().active[&process.stable_id].due_tick;
    let process_dross_before = world.workings_state.as_ref().unwrap().active[&process.stable_id]
        .dross_current
        .total();
    assert!(process_dross_before > 0);

    let rite = world
        .begin_settling_rite([34; 16], "settling-worker", controller)
        .unwrap();
    settle_channel(&mut world, rite.stable_id);
    assert!(
        world.workings_state.as_ref().unwrap().active[&process.stable_id].due_tick > due_before
    );
    let dross_vessel_id =
        match world.workings_state.as_ref().unwrap().active[&rite.stable_id].effect {
            crate::workings::WorkingEffect::Settle {
                dross_vessel_id, ..
            } => dross_vessel_id,
            _ => panic!("settling rite reserved the wrong typed effect"),
        };
    assert!(
        [
            vessel_stack_at(&world, first_pos).arcane_id,
            vessel_stack_at(&world, second_pos).arcane_id
        ]
        .contains(&dross_vessel_id)
    );
    let dross_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_dross_total(dross_vessel_id);
    let wear_before = world
        .implements_state
        .as_ref()
        .unwrap()
        .instance(dross_vessel_id)
        .unwrap()
        .wear;
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    world.release_working(process.stable_id).unwrap();
    let routed = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_dross_total(dross_vessel_id)
        .saturating_sub(dross_before);
    assert_eq!(routed, process_dross_before - process_dross_before / 4);
    let rite_tx = &world.workings_state.as_ref().unwrap().active[&rite.stable_id];
    assert!(matches!(
        rite_tx.effect,
        crate::workings::WorkingEffect::Settle {
            dross_routed,
            stabilizer_wear,
            ..
        } if dross_routed == routed && stabilizer_wear > 0
    ));
    assert!(
        world
            .implements_state
            .as_ref()
            .unwrap()
            .instance(dross_vessel_id)
            .unwrap()
            .wear
            > wear_before
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        total_before
    );

    let containment = controller.offset(0, 0, -1).unwrap();
    world.set_block_at(containment, AIR);
    let outcomes = world.tick_workings();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].0.stable_id, rite.stable_id);
    assert_eq!(outcomes[0].0.cue, crate::workings::WorkingCueKind::Strain);
}
