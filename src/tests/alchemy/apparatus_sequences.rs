//! Apparatus sequences scenarios.

use super::*;

#[test]
fn every_base_preparation_completes_through_its_real_apparatus_sequence() {
    use crate::alchemy::{ApparatusKind, CarrierKind};

    let ids = base_reg().preparations.keys().cloned().collect::<Vec<_>>();
    for preparation_id in ids {
        let tag = format!("alchemy-roster-{}", preparation_id.replace(':', "-"));
        let mut world = crate::tests::implements::embodied_implements_world(&tag);
        let positions = std::collections::BTreeMap::from([
            (ApparatusKind::Mortar, bp(8, 100, 8)),
            (ApparatusKind::InfusionBasin, bp(9, 100, 8)),
            (ApparatusKind::Alembic, bp(8, 100, 9)),
            (ApparatusKind::FilterStand, bp(9, 100, 9)),
        ]);
        let center = positions[&ApparatusKind::Mortar].chunk();
        world.insert_empty_chunks_for_test(
            (-1..=1)
                .flat_map(|du| (-1..=1).map(move |dv| center.offset(du, dv)))
                .filter(|chunk| !world.has_chunk(*chunk))
                .collect::<Vec<_>>(),
        );
        for (kind, block) in [
            (ApparatusKind::Mortar, "base:alchemy_mortar"),
            (ApparatusKind::InfusionBasin, "base:infusion_basin"),
            (ApparatusKind::Alembic, "base:alembic"),
            (ApparatusKind::FilterStand, "base:filter_stand"),
        ] {
            world.set_block_authored_at(
                positions[&kind],
                b(&world.reg, block),
                "complete alchemy roster fixture",
            );
        }
        // Every process station can be physically heated or cooled and has an
        // adjacent conductor. The fixture is intentionally compact so all
        // transfers remain inside ordinary laboratory reach.
        for pos in [bp(7, 100, 8), bp(10, 100, 8), bp(7, 100, 9), bp(10, 100, 9)] {
            world.set_block_authored_at(
                pos,
                b(&world.reg, "base:arcane_conductor"),
                "alchemy roster conductor",
            );
        }
        for pos in [bp(8, 99, 8), bp(9, 99, 8), bp(8, 99, 9), bp(9, 99, 9)] {
            world.set_block_authored_at(
                pos,
                b(&world.reg, "base:fire"),
                "alchemy roster heat control",
            );
        }
        for pos in [bp(8, 101, 8), bp(9, 101, 8), bp(8, 101, 9), bp(9, 101, 9)] {
            world.set_block_authored_at(
                pos,
                b(&world.reg, "base:ice"),
                "alchemy roster cooling control",
            );
        }

        let definition = world.reg.preparations[&preparation_id].clone();
        let rainbell_count = definition
            .ingredients
            .iter()
            .find(|ingredient| ingredient.item == "base:rainbell_dew")
            .map_or(0, |ingredient| u64::from(ingredient.count));
        if rainbell_count != 0 {
            seed_water(
                &mut world,
                crate::planet_atlas::WaterClass::Fresh,
                rainbell_count * crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL,
                false,
            );
        }
        if definition.carrier.is_water() {
            let class = match definition.carrier {
                CarrierKind::FreshWater => crate::planet_atlas::WaterClass::Fresh,
                CarrierKind::Brine => crate::planet_atlas::WaterClass::Salt,
                CarrierKind::Alcohol | CarrierKind::PlantOil => unreachable!(),
            };
            seed_water(
                &mut world,
                class,
                crate::planet_atlas::HYDRO_UNITS_PER_BLOCK,
                true,
            );
        }
        fund_ambient_current(
            &mut world,
            positions[&definition.process.apparatus()],
            &definition.resonance,
            definition.charge_units,
        );
        let water_before = world.live_water_audit().unwrap();
        let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;

        let mut inventory = Inventory::new();
        let mut revisions = std::collections::BTreeMap::new();
        for pos in positions.values() {
            let mut revision = None;
            operate(
                &mut world,
                &mut inventory,
                *pos,
                &mut revision,
                ApparatusAction::Inspect,
            );
            revisions.insert(*pos, revision);
        }
        let mut at = positions[&ApparatusKind::Mortar];
        operate(
            &mut world,
            &mut inventory,
            at,
            revisions.get_mut(&at).unwrap(),
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
                    at,
                    revisions.get_mut(&at).unwrap(),
                    ApparatusAction::Grind { inventory_slot: 0 },
                );
            }
        }

        loop {
            let batch = world.alchemy_state().unwrap().apparatus[&at]
                .batch
                .as_ref()
                .unwrap()
                .clone();
            let Some(step) = definition.steps.get(usize::from(batch.step_index)).copied() else {
                break;
            };
            let required_kind = match step {
                ProcessStep::Grind => ApparatusKind::Mortar,
                ProcessStep::Filter => ApparatusKind::FilterStand,
                ProcessStep::Distill => ApparatusKind::Alembic,
                _ => definition.process.apparatus(),
            };
            let destination = positions[&required_kind];
            if destination != at {
                let result = operate(
                    &mut world,
                    &mut inventory,
                    at,
                    revisions.get_mut(&at).unwrap(),
                    ApparatusAction::TransferMash { destination },
                );
                at = destination;
                revisions.insert(at, Some(result.revision));
                continue;
            }

            let target_temperature = match step {
                ProcessStep::Cool => {
                    definition.storage_temperature_millic[0]
                        + (definition.storage_temperature_millic[1]
                            - definition.storage_temperature_millic[0])
                            / 2
                }
                _ => {
                    definition.temperature_millic[0]
                        + (definition.temperature_millic[1] - definition.temperature_millic[0]) / 2
                }
            };
            if matches!(
                step,
                ProcessStep::Heat
                    | ProcessStep::Agitate
                    | ProcessStep::Settle
                    | ProcessStep::Distill
                    | ProcessStep::Filter
                    | ProcessStep::Cool
            ) {
                for _ in 0..16 {
                    let actual = world.alchemy_state().unwrap().apparatus[&at].temperature_millic;
                    if actual == target_temperature {
                        break;
                    }
                    operate(
                        &mut world,
                        &mut inventory,
                        at,
                        revisions.get_mut(&at).unwrap(),
                        ApparatusAction::SetHeat {
                            temperature_millic: target_temperature,
                        },
                    );
                }
            }

            match step {
                ProcessStep::Load => {
                    let loads = if definition.carrier.is_water() {
                        1
                    } else {
                        definition.solvent_units / 64
                    };
                    for _ in 0..loads {
                        put(&world, &mut inventory, 0, &definition.solvent_item);
                        operate(
                            &mut world,
                            &mut inventory,
                            at,
                            revisions.get_mut(&at).unwrap(),
                            ApparatusAction::LoadCarrier { inventory_slot: 0 },
                        );
                    }
                }
                ProcessStep::Agitate => {
                    operate(
                        &mut world,
                        &mut inventory,
                        at,
                        revisions.get_mut(&at).unwrap(),
                        ApparatusAction::SetAgitation {
                            agitation: definition.agitation,
                        },
                    );
                    operate(
                        &mut world,
                        &mut inventory,
                        at,
                        revisions.get_mut(&at).unwrap(),
                        ApparatusAction::Advance { step },
                    );
                }
                ProcessStep::Charge => {
                    while world.alchemy_state().unwrap().apparatus[&at]
                        .batch
                        .as_ref()
                        .unwrap()
                        .charge_input_units
                        < definition.charge_units
                    {
                        let remaining = definition.charge_units
                            - world.alchemy_state().unwrap().apparatus[&at]
                                .batch
                                .as_ref()
                                .unwrap()
                                .charge_input_units;
                        let units = remaining.min(u64::from(definition.charge_rate[1]));
                        operate(
                            &mut world,
                            &mut inventory,
                            at,
                            revisions.get_mut(&at).unwrap(),
                            ApparatusAction::Charge {
                                inventory_slot: None,
                                units,
                            },
                        );
                    }
                }
                ProcessStep::Filter => {
                    if definition.id == "base:ashlace_wash" {
                        let media_id = world
                            .arcane_ledger
                            .as_mut()
                            .unwrap()
                            .allocate_item_id()
                            .unwrap();
                        fund_owner_current(
                            &mut world,
                            ArcaneOwner::ItemDross(media_id),
                            &definition.resonance,
                            1,
                            Some("base:ashlace_tissue"),
                        );
                        put(&world, &mut inventory, 0, "base:ashlace_tissue");
                        inventory.slots[0].as_mut().unwrap().arcane_id = media_id;
                    } else {
                        put(&world, &mut inventory, 0, "base:filter_cloth");
                    }
                    operate(
                        &mut world,
                        &mut inventory,
                        at,
                        revisions.get_mut(&at).unwrap(),
                        ApparatusAction::LoadFilter { inventory_slot: 0 },
                    );
                    let due = world.alchemy_state().unwrap().apparatus[&at]
                        .batch
                        .as_ref()
                        .unwrap()
                        .due_tick;
                    world.set_simulation_clock(world.clock().max((due + 1) as f64 / 20.0));
                    operate(
                        &mut world,
                        &mut inventory,
                        at,
                        revisions.get_mut(&at).unwrap(),
                        ApparatusAction::Advance { step },
                    );
                }
                ProcessStep::Settle | ProcessStep::Distill => {
                    let due = world.alchemy_state().unwrap().apparatus[&at]
                        .batch
                        .as_ref()
                        .unwrap()
                        .due_tick;
                    world.set_simulation_clock(world.clock().max((due + 1) as f64 / 20.0));
                    if step == ProcessStep::Settle {
                        operate(
                            &mut world,
                            &mut inventory,
                            at,
                            revisions.get_mut(&at).unwrap(),
                            ApparatusAction::SetAgitation {
                                agitation: AgitationKind::Still,
                            },
                        );
                    }
                    operate(
                        &mut world,
                        &mut inventory,
                        at,
                        revisions.get_mut(&at).unwrap(),
                        ApparatusAction::Advance { step },
                    );
                }
                ProcessStep::Heat | ProcessStep::Cool => {
                    operate(
                        &mut world,
                        &mut inventory,
                        at,
                        revisions.get_mut(&at).unwrap(),
                        ApparatusAction::Advance { step },
                    );
                }
                ProcessStep::Grind => unreachable!("fixture already ground every ingredient"),
            }
        }
        assert_eq!(
            world.alchemy_state().unwrap().apparatus[&at]
                .batch
                .as_ref()
                .unwrap()
                .outcome,
            BatchOutcome::Ready,
            "{} did not finish ready",
            definition.id
        );
        for slot in 10..10 + usize::from(definition.doses) {
            put(&world, &mut inventory, slot, &definition.empty_vessel);
            operate(
                &mut world,
                &mut inventory,
                at,
                revisions.get_mut(&at).unwrap(),
                ApparatusAction::Decant {
                    vessel_slot: slot as u8,
                },
            );
        }
        assert_eq!(
            world.alchemy_state().unwrap().apparatus[&at]
                .batch
                .as_ref()
                .unwrap()
                .liquid
                .volume_units,
            0,
            "{} retained copyable dose volume",
            definition.id
        );
        if definition.id == "base:hearth_tonic" {
            let dose_slot = inventory
                .slots
                .iter()
                .position(|stack| {
                    stack.is_some_and(|stack| {
                        stack.arcane_id != 0
                            && world.reg.item(stack.item).name == definition.output_item
                    })
                })
                .expect("the embodied Hearth batch produced no stable dose");
            let applied = world
                .use_preparation(
                    [42; 16],
                    "embodied apothecary fixture",
                    at,
                    &mut inventory,
                    dose_slot,
                    crate::alchemy::AlchemyTarget::SelfActor,
                )
                .unwrap();
            assert_eq!(applied.preparation_id, definition.id);
            assert!(applied.status_id.is_some());
            assert_eq!(
                applied.returned_vessel.unwrap().item_name,
                "base:glass_bottle"
            );
        }
        seed_water(
            &mut world,
            crate::planet_atlas::WaterClass::Fresh,
            crate::planet_atlas::HYDRO_UNITS_PER_BLOCK,
            true,
        );
        put(&world, &mut inventory, 0, "base:bucket_water");
        operate(
            &mut world,
            &mut inventory,
            at,
            revisions.get_mut(&at).unwrap(),
            ApparatusAction::Clean {
                water_slot: 0,
                filter_slot: None,
            },
        );
        assert!(
            world.alchemy_state().unwrap().apparatus[&at]
                .batch
                .is_none()
        );
        if definition.steps.contains(&ProcessStep::Filter) {
            let spent = inventory
                .slots
                .iter()
                .flatten()
                .find(|stack| {
                    world.reg.item(stack.item).name == "base:spent_filter" && stack.arcane_id != 0
                })
                .copied()
                .expect("filtered recipe returned no physical spent medium");
            assert!(
                world
                    .arcane_ledger
                    .as_ref()
                    .unwrap()
                    .account(&ArcaneOwner::ItemDross(spent.arcane_id))
                    .is_some_and(|account| !account.current.is_empty())
            );
        }
        assert_eq!(
            world.live_water_audit().unwrap().current_water_hu,
            water_before.current_water_hu,
            "{} changed planetary water total",
            definition.id
        );
        assert_eq!(
            world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
            current_before,
            "{} changed finite Current total",
            definition.id
        );
    }
}
