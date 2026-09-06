//! Rootwake scenarios.

use super::*;

#[test]
fn rootwake_advances_one_stage_and_debits_real_water_and_fertility() {
    let (mut world, wand_pos, wand) = workings_world("workings-rootwake");
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let plant = wand_pos.offset(2, 1, 0).unwrap();
    let soil = plant.offset(0, -1, 0).unwrap();
    let water = plant.offset(1, 0, 0).unwrap();
    let farmland = b(&world.reg, "base:farmland");
    let seed = b(&world.reg, "base:wheat_seeds");
    let next = world.reg.block(seed).crop_next.unwrap();
    world.set_block_meta_at(soil, farmland, crate::world::soil::soil_meta(42, 0));
    world.set_block_at(plant, seed);
    world.set_block_water_at(water, world.reg.water_block(0), 0, 0);
    let water_before = world.water_mass_at(water).unwrap();
    let fertility_before = world.fertility_at_pos(soil);

    let started = world
        .begin_rootwake_working(
            [9; 16],
            "rootwake-fixture",
            wand_pos,
            wand.arcane_id,
            plant,
            false,
        )
        .unwrap();
    let legitimate = world.workings_state.as_ref().unwrap().active[&started.stable_id].clone();
    let mut input_free_forgery = legitimate.clone();
    input_free_forgery.physical_debits.clear();
    assert!(
        input_free_forgery.validate().is_err(),
        "a forged biological effect must not omit its water and nutrient debits"
    );
    let mut cross_capability_forgery = legitimate;
    cross_capability_forgery.effect = crate::workings::WorkingEffect::PointLight {
        source: wand_pos,
        target: plant,
        intensity: 1,
        expires_tick: 1,
    };
    assert!(
        cross_capability_forgery.validate().is_err(),
        "a Rootwake shell must not obtain a light or arbitrary mutation capability"
    );
    world.complete_working(started.stable_id).unwrap();

    assert_eq!(world.get_block_at(plant), next);
    assert_eq!(world.fertility_at_pos(soil), fertility_before - 1);
    assert_eq!(
        world.water_mass_at(water).unwrap().water_hu,
        water_before.water_hu - crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
}

#[test]
fn rootwake_cannot_override_dead_hearts_winter_or_missing_habitat_inputs() {
    let (mut dead_world, source, wand) = workings_world("workings-rootwake-dead-heart");
    let plant = source.offset(2, 1, 0).unwrap();
    let soil = plant.offset(0, -1, 0).unwrap();
    let water = plant.offset(1, 0, 0).unwrap();
    dead_world.set_block_at(soil, b(&dead_world.reg, "base:dirt"));
    dead_world.set_block_at(plant, b(&dead_world.reg, "base:berry_bush"));
    dead_world.set_block_water_at(water, dead_world.reg.water_block(0), 0, 0);
    let province = dead_world.generator.province_at(plant.surface()).key;
    dead_world.hearts.insert(
        province,
        crate::world::Heart {
            pos: plant,
            stage: 0,
            strain: 100.0,
            rooting: 0.0,
            graft: None,
            drift: 0.0,
            regrow: 0.0,
        },
    );
    let current_before = dead_world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .total;
    let water_before = dead_world.water_mass_at(water).unwrap();
    let refusal = dead_world
        .begin_rootwake_working(
            [42; 16],
            "dead-heart-worker",
            source,
            wand.arcane_id,
            plant,
            false,
        )
        .unwrap_err();
    assert!(refusal.contains("dead heart"), "{refusal}");
    assert_eq!(
        dead_world.get_block_at(plant),
        b(&dead_world.reg, "base:berry_bush")
    );
    assert_eq!(dead_world.water_mass_at(water).unwrap(), water_before);
    assert_eq!(
        dead_world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        current_before
    );

    let (mut winter_world, winter_source, winter_wand) = workings_world("workings-rootwake-winter");
    let winter_plant = winter_source.offset(2, 1, 0).unwrap();
    let winter_soil = winter_plant.offset(0, -1, 0).unwrap();
    let winter_water = winter_plant.offset(1, 0, 0).unwrap();
    winter_world.set_block_meta_at(
        winter_soil,
        b(&winter_world.reg, "base:farmland"),
        crate::world::soil::soil_meta(42, 0),
    );
    winter_world.set_block_at(winter_plant, b(&winter_world.reg, "base:wheat_seeds"));
    winter_world.set_block_water_at(winter_water, winter_world.reg.water_block(0), 0, 0);
    winter_world.set_long_winter_for_test(true);
    let winter_current_before = winter_world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .total;
    let winter_water_before = winter_world.water_mass_at(winter_water).unwrap();
    let winter_fertility_before = winter_world.fertility_at_pos(winter_soil);
    let refusal = winter_world
        .begin_rootwake_working(
            [43; 16],
            "winter-worker",
            winter_source,
            winter_wand.arcane_id,
            winter_plant,
            false,
        )
        .unwrap_err();
    assert!(refusal.contains("Winter stops"), "{refusal}");
    assert_eq!(
        winter_world.water_mass_at(winter_water).unwrap(),
        winter_water_before
    );
    assert_eq!(
        winter_world.fertility_at_pos(winter_soil),
        winter_fertility_before
    );
    assert_eq!(
        winter_world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        winter_current_before
    );

    winter_world.set_long_winter_for_test(false);
    winter_world.set_block_at(winter_water, AIR);
    let refusal = winter_world
        .begin_rootwake_working(
            [43; 16],
            "dry-habitat-worker",
            winter_source,
            winter_wand.arcane_id,
            winter_plant,
            false,
        )
        .unwrap_err();
    assert!(
        refusal.contains("real adjacent water reservoir"),
        "{refusal}"
    );
    assert_eq!(
        winter_world.fertility_at_pos(winter_soil),
        winter_fertility_before
    );
    assert_eq!(
        winter_world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        winter_current_before
    );
}
