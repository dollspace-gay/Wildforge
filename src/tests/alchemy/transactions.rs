//! Transactions scenarios.

use super::*;

#[test]
fn embodied_hearth_batch_is_exact_transactional_and_cannot_overfill() {
    let mut world = crate::tests::implements::embodied_implements_world("alchemy-hearth-lifecycle");
    let mortar = bp(8, 100, 8);
    let basin = bp(9, 100, 8);
    let conductor = bp(10, 100, 8);
    let fire = bp(9, 100, 9);
    let center = mortar.chunk();
    let missing = (-1..=1)
        .flat_map(|du| (-1..=1).map(move |dv| center.offset(du, dv)))
        .filter(|chunk| !world.has_chunk(*chunk))
        .collect::<Vec<_>>();
    world.insert_empty_chunks_for_test(missing);
    for (pos, block) in [
        (mortar, "base:alchemy_mortar"),
        (basin, "base:infusion_basin"),
        (conductor, "base:arcane_conductor"),
        (fire, "base:fire"),
    ] {
        world.set_block_authored_at(pos, b(&world.reg, block), "alchemy lifecycle fixture");
    }

    let definition = world.reg.preparations["base:hearth_tonic"].clone();
    fund_ambient_current(
        &mut world,
        basin,
        &definition.resonance,
        definition.charge_units,
    );
    {
        let weather = world.planetary_weather_for_test_mut().unwrap();
        let reservoir = weather
            .water
            .reservoirs
            .iter()
            .find(|reservoir| {
                reservoir.coarse.water_hu >= crate::planet_atlas::HYDRO_UNITS_PER_BLOCK
                    && reservoir.coarse.water_class() == crate::planet_atlas::WaterClass::Fresh
            })
            .unwrap()
            .id;
        let parcel = weather
            .materialize_surface_water(reservoir, crate::planet_atlas::HYDRO_UNITS_PER_BLOCK);
        assert_eq!(
            weather.move_detailed_to_portable(parcel),
            Some(crate::planet_atlas::WaterClass::Fresh)
        );
    }
    let water_before = world.live_water_audit().unwrap();
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;

    let mut inventory = Inventory::new();
    let mut mortar_revision = None;
    let mut basin_revision = None;
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
        ApparatusAction::Begin {
            preparation_id: definition.id.clone(),
        },
    );
    for ingredient in &definition.ingredients {
        for _ in 0..ingredient.count {
            put(&world, &mut inventory, 0, &ingredient.item);
            operate(
                &mut world,
                &mut inventory,
                mortar,
                &mut mortar_revision,
                ApparatusAction::Grind { inventory_slot: 0 },
            );
        }
    }
    let transferred = operate(
        &mut world,
        &mut inventory,
        mortar,
        &mut mortar_revision,
        ApparatusAction::TransferMash { destination: basin },
    );
    basin_revision = Some(transferred.revision);

    put(&world, &mut inventory, 0, "base:bucket_water");
    operate(
        &mut world,
        &mut inventory,
        basin,
        &mut basin_revision,
        ApparatusAction::LoadCarrier { inventory_slot: 0 },
    );
    operate(
        &mut world,
        &mut inventory,
        basin,
        &mut basin_revision,
        ApparatusAction::SetHeat {
            temperature_millic: 50_000,
        },
    );
    operate(
        &mut world,
        &mut inventory,
        basin,
        &mut basin_revision,
        ApparatusAction::Advance {
            step: ProcessStep::Heat,
        },
    );
    operate(
        &mut world,
        &mut inventory,
        basin,
        &mut basin_revision,
        ApparatusAction::SetAgitation {
            agitation: AgitationKind::Stirred,
        },
    );
    operate(
        &mut world,
        &mut inventory,
        basin,
        &mut basin_revision,
        ApparatusAction::Advance {
            step: ProcessStep::Agitate,
        },
    );
    for _ in 0..6 {
        operate(
            &mut world,
            &mut inventory,
            basin,
            &mut basin_revision,
            ApparatusAction::Charge {
                inventory_slot: None,
                units: 4,
            },
        );
    }
    world.set_simulation_clock(100.0);
    operate(
        &mut world,
        &mut inventory,
        basin,
        &mut basin_revision,
        ApparatusAction::SetAgitation {
            agitation: AgitationKind::Still,
        },
    );
    let ready = operate(
        &mut world,
        &mut inventory,
        basin,
        &mut basin_revision,
        ApparatusAction::Advance {
            step: ProcessStep::Settle,
        },
    );
    assert_eq!(ready.outcome, Some(BatchOutcome::Ready));
    assert_eq!(ready.volume_units, definition.solvent_units);

    let stale_revision = basin_revision.unwrap().saturating_sub(1);
    put(&world, &mut inventory, 9, "base:glass_bottle");
    let stale = world.operate_alchemy(
        basin,
        &mut inventory,
        AlchemyRequest {
            actor: [42; 16],
            actor_label: "apothecary fixture".into(),
            expected_revision: Some(stale_revision),
            action: ApparatusAction::Decant { vessel_slot: 9 },
        },
    );
    assert!(stale.is_err());
    assert_eq!(
        world.alchemy_state().unwrap().apparatus[&basin]
            .batch
            .as_ref()
            .unwrap()
            .liquid
            .volume_units,
        definition.solvent_units
    );

    let mut dose_ids = Vec::new();
    for slot in 10..14 {
        put(&world, &mut inventory, slot, "base:glass_bottle");
        let result = operate(
            &mut world,
            &mut inventory,
            basin,
            &mut basin_revision,
            ApparatusAction::Decant {
                vessel_slot: slot as u8,
            },
        );
        dose_ids.push(result.produced.unwrap().arcane_id);
    }
    assert_eq!(dose_ids.len(), usize::from(definition.doses));
    assert_eq!(
        world.alchemy_state().unwrap().apparatus[&basin]
            .batch
            .as_ref()
            .unwrap()
            .liquid
            .volume_units,
        0
    );
    put(&world, &mut inventory, 14, "base:glass_bottle");
    assert!(
        world
            .operate_alchemy(
                basin,
                &mut inventory,
                AlchemyRequest {
                    actor: [42; 16],
                    actor_label: "apothecary fixture".into(),
                    expected_revision: basin_revision,
                    action: ApparatusAction::Decant { vessel_slot: 14 },
                },
            )
            .is_err()
    );
    assert_eq!(world.alchemy_state().unwrap().containers.len(), 4);
    assert_eq!(
        world.live_water_audit().unwrap().current_water_hu,
        water_before.current_water_hu
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );

    let stored_id = dose_ids[0];
    let stored_stack = inventory
        .slots
        .iter()
        .flatten()
        .find(|stack| stack.arcane_id == stored_id)
        .copied()
        .unwrap();
    let expires_before = world.alchemy_state().unwrap().containers[&stored_id].expires_tick;
    world.set_simulation_clock(world.clock() + 20.0);
    assert_eq!(
        world
            .age_preparation_storage(stored_stack, 50_000, 400)
            .unwrap(),
        Some(false)
    );
    assert_eq!(
        world.alchemy_state().unwrap().containers[&stored_id].expires_tick,
        expires_before - 1_200,
        "hot storage did not accelerate the declared shelf-life clock"
    );
    let protected_id = dose_ids[1];
    let protected_stack = inventory
        .slots
        .iter()
        .flatten()
        .find(|stack| stack.arcane_id == protected_id)
        .copied()
        .unwrap();
    let protected_before = world.alchemy_state().unwrap().containers[&protected_id].expires_tick;
    world
        .age_preparation_storage(protected_stack, 10_000, 100)
        .unwrap();
    assert_eq!(
        world.alchemy_state().unwrap().containers[&protected_id].expires_tick,
        protected_before + 300,
        "Holdfast-style protected time reset or failed to slow the clock"
    );

    // A hostile wash request cannot name a guessed remote ItemDross account.
    // Re-label one host-owned fixture dose as the closed wash handler so the
    // assertion exercises target authorization without constructing a second
    // full laboratory.
    let wash_id = dose_ids[1];
    let wash_slot = inventory
        .slots
        .iter()
        .position(|stack| stack.is_some_and(|stack| stack.arcane_id == wash_id))
        .unwrap();
    inventory.slots[wash_slot].as_mut().unwrap().item = it(&world.reg, "base:ashlace_wash");
    {
        let wash = world
            .alchemy_state
            .as_mut()
            .unwrap()
            .containers
            .get_mut(&wash_id)
            .unwrap();
        wash.preparation_id = "base:ashlace_wash".into();
        wash.item_name = "base:ashlace_wash".into();
    }
    let forged = world.use_preparation(
        [42; 16],
        "hostile fixture",
        basin,
        &mut inventory,
        wash_slot,
        crate::alchemy::AlchemyTarget::Item(9_999_999),
    );
    assert!(
        forged
            .unwrap_err()
            .contains("not in authoritative carried custody")
    );
    assert!(
        world
            .alchemy_state()
            .unwrap()
            .containers
            .contains_key(&wash_id)
    );

    let broken_id = dose_ids[0];
    let broken_slot = inventory
        .slots
        .iter()
        .position(|stack| stack.is_some_and(|stack| stack.arcane_id == broken_id))
        .unwrap();
    let broken = inventory.take_one_stack(broken_slot).unwrap();
    let industrial_before_break = world.live_water_audit().unwrap().industrial.water_hu;
    assert!(world.retire_arcane_stack_at(basin, broken, "fixture bottle shattered"));
    assert!(
        !world
            .alchemy_state()
            .unwrap()
            .containers
            .contains_key(&broken_id)
    );
    assert_eq!(
        industrial_before_break - world.live_water_audit().unwrap().industrial.water_hu,
        definition.dose_units
    );
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_current_total(broken_id)
            .is_none()
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
    let pollution = &world.alchemy_state().unwrap().pollution[&basin];
    assert_eq!(pollution.water.water_hu, definition.dose_units);
    assert!(pollution.dross.total() > 0);

    // Throwing carries the stable dose identity in flight. Impact invokes
    // the same exact destruction settlement rather than deleting a cosmetic
    // bottle or duplicating its sidecar.
    let thrown_id = dose_ids[2];
    let thrown_slot = inventory
        .slots
        .iter()
        .position(|stack| stack.is_some_and(|stack| stack.arcane_id == thrown_id))
        .unwrap();
    let thrown = inventory.take_one_stack(thrown_slot).unwrap();
    let wall = bp(12, 100, 8);
    world.set_block_authored_at(
        wall,
        b(&world.reg, "base:cobblestone"),
        "thrown preparation impact fixture",
    );
    let launch = bp(11, 100, 8).entity_center();
    world.spawn_projectile(crate::mobs::Projectile {
        stable_id: 0,
        pos: launch,
        vel: glam::Vec3::new(6.0, 0.0, 0.0),
        tile: world.reg.item(thrown.item).icon,
        damage: 0.0,
        damage_type: None,
        age: 0.0,
        from_player: true,
        drop_item: None,
        preparation_payload: Some(thrown),
        owner: 0,
    });
    for _ in 0..20 {
        world.tick_projectiles(&[], 0.05);
        if world.projectiles().is_empty() {
            break;
        }
    }
    assert!(
        world.projectiles().is_empty(),
        "the bottle never reached the wall"
    );
    assert!(
        !world
            .alchemy_state()
            .unwrap()
            .containers
            .contains_key(&thrown_id),
        "impact left a detached preparation sidecar"
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );

    let spoiled_id = dose_ids[3];
    let spoiled_slot = inventory
        .slots
        .iter()
        .position(|stack| stack.is_some_and(|stack| stack.arcane_id == spoiled_id))
        .unwrap();
    world
        .alchemy_state
        .as_mut()
        .unwrap()
        .containers
        .get_mut(&spoiled_id)
        .unwrap()
        .outcome = BatchOutcome::Spoiled;
    let identified = world
        .use_preparation(
            [42; 16],
            "apothecary fixture",
            basin,
            &mut inventory,
            spoiled_slot,
            crate::alchemy::AlchemyTarget::SelfActor,
        )
        .unwrap();
    assert_eq!(identified.cue.kind, AlchemyCueKind::Spoil);
    assert!(identified.returned_vessel.is_none());
    assert_eq!(
        world
            .reg
            .item(inventory.slots[spoiled_slot].unwrap().item)
            .name,
        "base:spent_liquor"
    );
    assert_eq!(
        world.alchemy_state().unwrap().containers[&spoiled_id].item_name,
        "base:spent_liquor"
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
}
