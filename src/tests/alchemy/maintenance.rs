//! Maintenance scenarios.

use super::*;

#[test]
fn bounded_maintenance_is_round_robin_and_damaged_apparatus_leaks_legibly() {
    let mut world =
        crate::tests::implements::embodied_implements_world("alchemy-maintenance-fairness");
    let center = bp(8, 100, 8).chunk();
    world.insert_empty_chunks_for_test(
        (-1..=1)
            .flat_map(|du| (-1..=1).map(move |dv| center.offset(du, dv)))
            .filter(|chunk| !world.has_chunk(*chunk))
            .collect::<Vec<_>>(),
    );
    let positions = [bp(8, 100, 8), bp(10, 100, 8), bp(12, 100, 8)];
    let mortar_block = b(&world.reg, "base:alchemy_mortar");
    for pos in positions {
        world.set_block_authored_at(pos, mortar_block, "alchemy fairness fixture");
        let mut inventory = Inventory::new();
        let mut revision = None;
        operate(
            &mut world,
            &mut inventory,
            pos,
            &mut revision,
            ApparatusAction::Inspect,
        );
    }
    world.alchemy_state.as_mut().unwrap().maintenance_phase = 0;
    world
        .alchemy_state
        .as_mut()
        .unwrap()
        .maintenance_apparatus_cursor = None;
    let mut visited = std::collections::BTreeSet::new();
    for _ in 0..3 {
        world.tick_alchemy(1).unwrap();
        visited.insert(
            world
                .alchemy_state()
                .unwrap()
                .maintenance_apparatus_cursor
                .unwrap(),
        );
        // Advance the other three bounded categories back to apparatus.
        world.tick_alchemy(1).unwrap();
        world.tick_alchemy(1).unwrap();
        world.tick_alchemy(1).unwrap();
    }
    assert_eq!(visited, positions.into_iter().collect());

    let leaky = positions[0];
    let revision = world.alchemy_state().unwrap().apparatus[&leaky].revision;
    let mut inventory = Inventory::new();
    world
        .operate_alchemy(
            leaky,
            &mut inventory,
            AlchemyRequest {
                actor: [42; 16],
                actor_label: "leak fixture".into(),
                expected_revision: Some(revision),
                action: ApparatusAction::Begin {
                    preparation_id: "base:hearth_tonic".into(),
                },
            },
        )
        .unwrap();
    {
        let state = world.alchemy_state.as_mut().unwrap();
        state.maintenance_phase = 0;
        state.maintenance_apparatus_cursor = positions.last().copied();
        state.apparatus.get_mut(&leaky).unwrap().integrity_permille = 250;
    }
    let cues = world.tick_alchemy(1).unwrap();
    assert!(cues.iter().any(|cue| cue.kind == AlchemyCueKind::Leak));
    assert!(
        world.alchemy_state().unwrap().apparatus[&leaky]
            .batch
            .is_none()
    );
    assert!(
        world
            .alchemy_state()
            .unwrap()
            .pollution
            .contains_key(&leaky)
    );
}

#[test]
fn ordinary_block_break_cannot_orphan_dirty_alchemy_state() {
    let mut world = crate::tests::implements::embodied_implements_world("alchemy-break-guard");
    let pos = bp(8, 100, 8);
    world.insert_empty_chunks_for_test(vec![pos.chunk()]);
    world.set_block_authored_at(
        pos,
        b(&world.reg, "base:alchemy_mortar"),
        "alchemy break fixture",
    );
    let mut inventory = Inventory::new();
    let mut revision = None;
    operate(
        &mut world,
        &mut inventory,
        pos,
        &mut revision,
        ApparatusAction::Inspect,
    );
    operate(
        &mut world,
        &mut inventory,
        pos,
        &mut revision,
        ApparatusAction::Begin {
            preparation_id: "base:hearth_tonic".into(),
        },
    );
    assert!(world.break_block_at(pos, None, true, false).is_none());
    assert!(
        world.alchemy_state().unwrap().apparatus[&pos]
            .batch
            .is_some()
    );

    let mut clean_world =
        crate::tests::implements::embodied_implements_world("alchemy-clean-break");
    clean_world.insert_empty_chunks_for_test(vec![pos.chunk()]);
    clean_world.set_block_authored_at(
        pos,
        b(&clean_world.reg, "base:alchemy_mortar"),
        "alchemy clean break fixture",
    );
    let mut clean_inventory = Inventory::new();
    let mut clean_revision = None;
    operate(
        &mut clean_world,
        &mut clean_inventory,
        pos,
        &mut clean_revision,
        ApparatusAction::Inspect,
    );
    assert!(clean_world.break_block_at(pos, None, true, false).is_some());
    assert!(
        !clean_world
            .alchemy_state()
            .unwrap()
            .apparatus
            .contains_key(&pos)
    );
}

#[test]
fn damaged_apparatus_requires_matching_matter_and_hammer_to_repair() {
    let mut world = crate::tests::implements::embodied_implements_world("alchemy-apparatus-repair");
    let pos = bp(8, 100, 8);
    world.insert_empty_chunks_for_test(vec![pos.chunk()]);
    world.set_block_authored_at(
        pos,
        b(&world.reg, "base:alchemy_mortar"),
        "alchemy repair fixture",
    );
    let mut inventory = Inventory::new();
    let mut revision = None;
    operate(
        &mut world,
        &mut inventory,
        pos,
        &mut revision,
        ApparatusAction::Inspect,
    );
    world
        .alchemy_state
        .as_mut()
        .unwrap()
        .apparatus
        .get_mut(&pos)
        .unwrap()
        .integrity_permille = 400;
    put(&world, &mut inventory, 0, "base:cobblestone");
    put(&world, &mut inventory, 1, "base:smith_hammer");
    let hammer_before = inventory.slots[1].unwrap().durability;
    let repaired = operate(
        &mut world,
        &mut inventory,
        pos,
        &mut revision,
        ApparatusAction::Repair { material_slot: 0 },
    );
    assert_eq!(
        world.alchemy_state().unwrap().apparatus[&pos].integrity_permille,
        650
    );
    assert!(inventory.slots[0].is_none());
    assert_eq!(inventory.slots[1].unwrap().durability, hammer_before - 1);
    assert!(repaired.cue.message.contains("Matching material"));

    operate(
        &mut world,
        &mut inventory,
        pos,
        &mut revision,
        ApparatusAction::Begin {
            preparation_id: "base:hearth_tonic".into(),
        },
    );
    put(&world, &mut inventory, 0, "base:cobblestone");
    let refused = world
        .operate_alchemy(
            pos,
            &mut inventory,
            AlchemyRequest {
                actor: [42; 16],
                actor_label: "apothecary fixture".into(),
                expected_revision: revision,
                action: ApparatusAction::Repair { material_slot: 0 },
            },
        )
        .unwrap_err();
    assert!(refused.contains("Drain and clean"));
    assert!(
        inventory.slots[0].is_some(),
        "refused repair consumed matter"
    );
}
