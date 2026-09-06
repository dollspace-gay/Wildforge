//! Soil scenarios.

use super::*;

#[test]
fn root_wash_improves_only_viable_growth_and_overconcentration_burns() {
    let mut world = crate::tests::implements::embodied_implements_world("alchemy-root-wash");
    let plot = bp(8, 100, 8);
    let now = (world.clock() * 20.0).round() as u64;
    world
        .alchemy_state
        .as_mut()
        .unwrap()
        .root_treatments
        .insert(
            plot,
            RootTreatment {
                source_batch: 1,
                actor: [3; 16],
                applied_tick: now,
                expires_tick: now + 100,
                water_hu: 64,
                nutrient_units: 4,
                uptake_permille: 500,
                concentration_permille: 1_000,
            },
        );
    assert_eq!(world.root_uptake_multiplier_at(plot), 1.5);
    world
        .alchemy_state
        .as_mut()
        .unwrap()
        .root_treatments
        .get_mut(&plot)
        .unwrap()
        .concentration_permille = 1_750;
    assert!(world.root_uptake_multiplier_at(plot) < 1.0);
    world.set_simulation_clock(world.clock() + 6.0);
    assert_eq!(world.root_uptake_multiplier_at(plot), 1.0);
}

#[test]
fn embodied_root_wash_returns_water_feeds_only_soil_and_salts_on_overdose() {
    let mut world = crate::tests::implements::embodied_implements_world("alchemy-root-wash-use");
    let plot = bp(8, 100, 8);
    world.insert_empty_chunks_for_test(vec![plot.chunk()]);
    world.set_block_authored_at(plot, b(&world.reg, "base:farmland"), "root wash plot");
    let definition = world.reg.preparations["base:root_wash"].clone();
    let mut inventory = Inventory::new();
    mint_ready_dose(&mut world, &mut inventory, 0, &definition.id, 7, 1);
    mint_ready_dose(&mut world, &mut inventory, 1, &definition.id, 7, 1);
    let water_before = world.live_water_audit().unwrap().current_water_hu;
    let meta_before = world.get_meta_at(plot);
    world
        .use_preparation(
            [25; 16],
            "root fixture",
            plot,
            &mut inventory,
            0,
            crate::alchemy::AlchemyTarget::Plot(plot),
        )
        .unwrap();
    assert_eq!(world.get_block_at(plot), b(&world.reg, "base:farmland"));
    assert!(world.get_meta_at(plot) >= meta_before);
    assert_eq!(
        world.root_uptake_multiplier_at(plot),
        1.0 + definition.effect.strength as f32 / 1_000.0
    );
    world
        .use_preparation(
            [25; 16],
            "root fixture",
            plot,
            &mut inventory,
            1,
            crate::alchemy::AlchemyTarget::Plot(plot),
        )
        .unwrap();
    assert!(world.get_soil_salinity_at(plot) > 0);
    assert!(world.root_uptake_multiplier_at(plot) < 1.0);
    assert_eq!(
        world.live_water_audit().unwrap().current_water_hu,
        water_before
    );
}

#[test]
fn ashlace_wash_moves_bounded_dross_into_one_recoverable_sludge() {
    let mut world = crate::tests::implements::embodied_implements_world("alchemy-ashlace-transfer");
    let at = bp(8, 100, 8);
    let definition = world.reg.preparations["base:ashlace_wash"].clone();
    let mut inventory = Inventory::new();
    let dose_id = mint_ready_dose(&mut world, &mut inventory, 0, &definition.id, 6, 2);
    let target_id = world
        .arcane_ledger
        .as_mut()
        .unwrap()
        .allocate_item_id()
        .unwrap();
    let target_units = u64::from(definition.effect.dross_capacity) + 7;
    fund_owner_current(
        &mut world,
        ArcaneOwner::ItemDross(target_id),
        &definition.resonance,
        target_units,
        Some("base:charge_vessel"),
    );
    inventory.slots[1] = Some(ItemStack {
        item: it(&world.reg, "base:charge_vessel"),
        count: 1,
        durability: world
            .reg
            .item(it(&world.reg, "base:charge_vessel"))
            .durability,
        arcane_id: target_id,
    });
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let water_before = world.live_water_audit().unwrap().current_water_hu;
    let result = world
        .use_preparation(
            [22; 16],
            "wash fixture",
            at,
            &mut inventory,
            0,
            crate::alchemy::AlchemyTarget::Item(target_id),
        )
        .unwrap();
    let sludge = result.byproduct.expect("wash returned no physical sludge");
    assert_eq!(sludge.item_name, "base:dross_sludge");
    assert_ne!(sludge.arcane_id, 0);
    assert!(result.returned_vessel.is_some());
    assert!(
        !world
            .alchemy_state()
            .unwrap()
            .containers
            .contains_key(&dose_id)
    );
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::ItemDross(target_id))
            .unwrap()
            .current
            .total(),
        7
    );
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::ItemDross(sludge.arcane_id))
            .unwrap()
            .current
            .total(),
        u64::from(definition.effect.dross_capacity) + 2
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
    assert_eq!(
        world.live_water_audit().unwrap().current_water_hu,
        water_before
    );
}
