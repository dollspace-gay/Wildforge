//! Ordinary carriers scenarios.

use super::*;

#[test]
fn ordinary_carriers_use_timed_apparatus_without_free_water_or_magic_yield() {
    use crate::planet_atlas::{HYDRO_UNITS_PER_BLOCK, WaterClass};

    let mut world =
        crate::tests::implements::embodied_implements_world("alchemy-ordinary-carriers");
    let basin = bp(8, 100, 8);
    let mortar = bp(9, 100, 8);
    world.insert_empty_chunks_for_test(vec![basin.chunk()]);
    for (pos, block) in [
        (basin, "base:infusion_basin"),
        (mortar, "base:alchemy_mortar"),
    ] {
        world.set_block_authored_at(pos, b(&world.reg, block), "ordinary carrier fixture");
    }
    seed_water(&mut world, WaterClass::Fresh, HYDRO_UNITS_PER_BLOCK, true);
    let water_before = world.live_water_audit().unwrap();
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let mut inventory = Inventory::new();
    let mut basin_revision = None;
    let mut mortar_revision = None;
    operate(
        &mut world,
        &mut inventory,
        basin,
        &mut basin_revision,
        ApparatusAction::Inspect,
    );
    operate(
        &mut world,
        &mut inventory,
        mortar,
        &mut mortar_revision,
        ApparatusAction::Inspect,
    );
    put(&world, &mut inventory, 0, "base:bucket_water");
    inventory.slots[1] = Some(ItemStack::new(&world.reg, it(&world.reg, "base:wheat"), 2));
    inventory.slots[2] = Some(ItemStack::new(&world.reg, it(&world.reg, "base:berry"), 2));
    operate(
        &mut world,
        &mut inventory,
        basin,
        &mut basin_revision,
        ApparatusAction::FermentAlcohol {
            water_slot: 0,
            wheat_slot: 1,
            berry_slot: 2,
        },
    );
    assert!(
        world
            .alchemy_state()
            .unwrap()
            .ordinary_jobs
            .contains_key(&basin)
    );
    assert!(
        inventory
            .slots
            .iter()
            .flatten()
            .any(|stack| { world.reg.item(stack.item).name == "base:bucket" && stack.count == 1 })
    );
    let early = world.operate_alchemy(
        basin,
        &mut inventory,
        AlchemyRequest {
            actor: [42; 16],
            actor_label: "ordinary apothecary fixture".into(),
            expected_revision: basin_revision,
            action: ApparatusAction::FermentAlcohol {
                water_slot: 0,
                wheat_slot: 1,
                berry_slot: 2,
            },
        },
    );
    assert!(early.unwrap_err().contains("still needs"));
    let due = world.alchemy_state().unwrap().ordinary_jobs[&basin].due_tick;
    world.set_simulation_clock((due + 1) as f64 / 20.0);
    let fermented = operate(
        &mut world,
        &mut inventory,
        basin,
        &mut basin_revision,
        ApparatusAction::FermentAlcohol {
            water_slot: 0,
            wheat_slot: 1,
            berry_slot: 2,
        },
    );
    assert_eq!(fermented.produced.unwrap().count, 4);
    assert_eq!(
        inventory
            .slots
            .iter()
            .flatten()
            .filter(|stack| world.reg.item(stack.item).name == "base:fermented_alcohol")
            .map(|stack| stack.count)
            .sum::<u32>(),
        4
    );

    inventory.slots[0] = Some(ItemStack::new(
        &world.reg,
        it(&world.reg, "base:wheat_seeds"),
        4,
    ));
    operate(
        &mut world,
        &mut inventory,
        mortar,
        &mut mortar_revision,
        ApparatusAction::PressOil { seed_slot: 0 },
    );
    let due = world.alchemy_state().unwrap().ordinary_jobs[&mortar].due_tick;
    world.set_simulation_clock((due + 1) as f64 / 20.0);
    let oil = operate(
        &mut world,
        &mut inventory,
        mortar,
        &mut mortar_revision,
        ApparatusAction::PressOil { seed_slot: 0 },
    );
    assert_eq!(oil.produced.unwrap().count, 4);
    assert_eq!(
        inventory
            .slots
            .iter()
            .flatten()
            .filter(|stack| world.reg.item(stack.item).name == "base:plant_oil")
            .map(|stack| stack.count)
            .sum::<u32>(),
        4
    );
    assert_eq!(
        world.live_water_audit().unwrap().current_water_hu,
        water_before.current_water_hu
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
}

#[test]
fn ordinary_automation_uses_the_same_timing_inputs_and_yield_as_manual_control() {
    let mut world =
        crate::tests::implements::embodied_implements_world("alchemy-automation-parity");
    let manual = bp(8, 100, 8);
    let automated = bp(10, 100, 8);
    world.insert_empty_chunks_for_test(vec![manual.chunk()]);
    for pos in [manual, automated] {
        world.set_block_authored_at(
            pos,
            b(&world.reg, "base:alchemy_mortar"),
            "automation parity fixture",
        );
    }
    let mut manual_inventory = Inventory::new();
    let mut automated_inventory = Inventory::new();
    let mut manual_revision = None;
    let mut automated_revision = None;
    operate_as(
        &mut world,
        &mut manual_inventory,
        manual,
        &mut manual_revision,
        [41; 16],
        "manual apothecary",
        ApparatusAction::Inspect,
    );
    operate_as(
        &mut world,
        &mut automated_inventory,
        automated,
        &mut automated_revision,
        [43; 16],
        "ordinary machine control",
        ApparatusAction::Inspect,
    );
    for inventory in [&mut manual_inventory, &mut automated_inventory] {
        inventory.slots[0] = Some(ItemStack::new(
            &world.reg,
            it(&world.reg, "base:wheat_seeds"),
            4,
        ));
    }
    operate_as(
        &mut world,
        &mut manual_inventory,
        manual,
        &mut manual_revision,
        [41; 16],
        "manual apothecary",
        ApparatusAction::PressOil { seed_slot: 0 },
    );
    operate_as(
        &mut world,
        &mut automated_inventory,
        automated,
        &mut automated_revision,
        [43; 16],
        "ordinary machine control",
        ApparatusAction::PressOil { seed_slot: 0 },
    );
    let manual_job = world.alchemy_state().unwrap().ordinary_jobs[&manual].clone();
    let automated_job = world.alchemy_state().unwrap().ordinary_jobs[&automated].clone();
    assert_eq!(manual_job.due_tick, automated_job.due_tick);
    assert_eq!(manual_job.input_materials, automated_job.input_materials);
    assert_eq!(manual_job.output_count, automated_job.output_count);
    world.set_simulation_clock((manual_job.due_tick + 1) as f64 / 20.0);
    let manual_result = operate_as(
        &mut world,
        &mut manual_inventory,
        manual,
        &mut manual_revision,
        [41; 16],
        "manual apothecary",
        ApparatusAction::PressOil { seed_slot: 0 },
    );
    let automated_result = operate_as(
        &mut world,
        &mut automated_inventory,
        automated,
        &mut automated_revision,
        [43; 16],
        "ordinary machine control",
        ApparatusAction::PressOil { seed_slot: 0 },
    );
    assert_eq!(manual_result.produced, automated_result.produced);
    assert_eq!(manual_result.produced.unwrap().count, 4);
    for inventory in [&manual_inventory, &automated_inventory] {
        assert_eq!(
            inventory
                .slots
                .iter()
                .flatten()
                .filter(|stack| world.reg.item(stack.item).name == "base:plant_oil")
                .map(|stack| stack.count)
                .sum::<u32>(),
            4
        );
    }
}
