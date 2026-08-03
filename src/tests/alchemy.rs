use super::*;

use crate::alchemy::{
    ActivePreparationStatus, AgitationKind, AlchemyCue, AlchemyCueKind, AlchemyRequest,
    AlchemyState, ApparatusAction, BatchOutcome, CarrierKind, ExactLiquid, PreparationDose,
    PreparationHandler, PreparationPhysiology, ProcessStep, RootTreatment, SpecimenCoating,
    status_owner_id,
};
use crate::arcane::{ArcaneAuthority, ArcaneOwner, ArcaneTransaction, Current, ResonanceTransform};

fn fund_ambient_current(
    world: &mut World,
    pos: crate::planet::BlockPos,
    resonance: &str,
    units: u64,
) {
    let destination = ArcaneOwner::Ambient(world.planet_atlas().unwrap().atlas_pos(pos.surface()));
    fund_owner_current(world, destination, resonance, units, None);
}

fn fund_owner_current(
    world: &mut World,
    destination: ArcaneOwner,
    resonance: &str,
    units: u64,
    content_id: Option<&str>,
) {
    let source = ArcaneOwner::Deep;
    let (source_version, destination_version, current) = {
        let ledger = world.arcane_ledger.as_ref().unwrap();
        let account = ledger.account(&source).unwrap();
        let mut available = account.current.clone();
        let current = available
            .take_units(units, [resonance.to_string()])
            .unwrap();
        (account.version, ledger.version_of(&destination), current)
    };
    let ledger = world.arcane_ledger.as_mut().unwrap();
    let mut transaction = ArcaneTransaction::transfer(
        ledger.system_transaction_id().unwrap(),
        source,
        source_version,
        destination,
        destination_version,
        current.clone(),
        ArcaneAuthority::System,
        "alchemy fixture funded one measured ambient resonance",
    );
    transaction.credits[0].current = Current::single(resonance.to_string(), units);
    transaction.credits[0].content_id = content_id.map(str::to_string);
    transaction.transforms = current
        .parts()
        .iter()
        .filter(|(source_resonance, _)| source_resonance.as_str() != resonance)
        .map(|(source_resonance, units)| ResonanceTransform {
            from: source_resonance.clone(),
            to: resonance.to_string(),
            units: *units,
        })
        .collect();
    ledger.commit(transaction).unwrap();
}

fn put(world: &World, inventory: &mut Inventory, slot: usize, item: &str) {
    inventory.slots[slot] = Some(ItemStack::new(&world.reg, it(&world.reg, item), 1));
}

fn seed_water(
    world: &mut World,
    class: crate::planet_atlas::WaterClass,
    units: u64,
    portable: bool,
) {
    let weather = world.planetary_weather_for_test_mut().unwrap();
    let reservoir = weather
        .water
        .reservoirs
        .iter()
        .find(|reservoir| {
            reservoir.coarse.water_hu >= units && reservoir.coarse.water_class() == class
        })
        .unwrap_or_else(|| panic!("fixture has no {class:?} reservoir with {units} HU"))
        .id;
    let parcel = weather.materialize_surface_water(reservoir, units);
    if portable {
        assert_eq!(weather.move_detailed_to_portable(parcel), Some(class));
    } else {
        assert!(weather.move_detailed_to_industrial(parcel));
    }
}

fn operate(
    world: &mut World,
    inventory: &mut Inventory,
    pos: crate::planet::BlockPos,
    revision: &mut Option<u64>,
    action: ApparatusAction,
) -> crate::alchemy::AlchemyResult {
    operate_as(
        world,
        inventory,
        pos,
        revision,
        [42; 16],
        "apothecary fixture",
        action,
    )
}

fn operate_as(
    world: &mut World,
    inventory: &mut Inventory,
    pos: crate::planet::BlockPos,
    revision: &mut Option<u64>,
    actor: [u8; 16],
    actor_label: &str,
    action: ApparatusAction,
) -> crate::alchemy::AlchemyResult {
    let result = world
        .operate_alchemy(
            pos,
            inventory,
            AlchemyRequest {
                actor,
                actor_label: actor_label.into(),
                expected_revision: *revision,
                action,
            },
        )
        .unwrap();
    *revision = Some(result.revision);
    result
}

fn mint_ready_dose(
    world: &mut World,
    inventory: &mut Inventory,
    slot: usize,
    preparation_id: &str,
    clean_units: u64,
    dross_units: u64,
) -> u64 {
    use crate::planet_atlas::{ReservoirMass, WaterClass};
    use crate::workings::WaterCarrier;

    let definition = world.reg.preparations[preparation_id].clone();
    let container_id = world
        .arcane_ledger
        .as_mut()
        .unwrap()
        .allocate_item_id()
        .unwrap();
    let water = match definition.carrier {
        CarrierKind::FreshWater | CarrierKind::Brine => {
            let class = if definition.carrier == CarrierKind::FreshWater {
                WaterClass::Fresh
            } else {
                WaterClass::Salt
            };
            seed_water(world, class, definition.dose_units, false);
            let mut available = world
                .planetary_weather_for_test_mut()
                .unwrap()
                .water
                .ledger
                .industrial;
            let parcel = available.take(definition.dose_units);
            assert_eq!(parcel.water_hu, definition.dose_units);
            parcel
        }
        CarrierKind::Alcohol | CarrierKind::PlantOil => ReservoirMass::default(),
    };
    let now = (world.clock.max(0.0) * 20.0).round() as u64;
    let empty = it(&world.reg, &definition.empty_vessel);
    let vessel_materials =
        crate::materials::stack_materials(&world.reg, ItemStack::new(&world.reg, empty, 1));
    let alchemy = world.alchemy_state.as_mut().unwrap();
    // The fixture refers to one already-completed installation and batch;
    // keep the same allocator-floor invariant production decanting preserves.
    alchemy.next_installation_id = alchemy.next_installation_id.max(2);
    alchemy.next_batch_id = alchemy.next_batch_id.max(2);
    alchemy.containers.insert(
        container_id,
        PreparationDose {
            container_id,
            preparation_id: definition.id.clone(),
            definition_version: definition.version,
            item_name: definition.output_item.clone(),
            liquid: ExactLiquid {
                carrier: Some(definition.carrier),
                volume_units: definition.dose_units,
                water,
                carrier_state: WaterCarrier {
                    thermal_millic_hu: i64::try_from(definition.dose_units).unwrap() * 20_000,
                    dross_subunits: 0,
                },
                solutes: std::collections::BTreeMap::from([(
                    definition.solvent_item.clone(),
                    definition.dose_units,
                )]),
            },
            vessel_materials,
            materials: crate::registry::MaterialVector::new(),
            current_units: clean_units,
            dross_units,
            born_tick: now,
            expires_tick: now.saturating_add(definition.shelf_life_ticks),
            last_storage_tick: now,
            outcome: BatchOutcome::Ready,
            source_installation: 1,
            source_batch: 1,
        },
    );
    if clean_units != 0 {
        fund_owner_current(
            world,
            ArcaneOwner::Item(container_id),
            &definition.resonance,
            clean_units,
            Some(&definition.output_item),
        );
    }
    if dross_units != 0 {
        fund_owner_current(
            world,
            ArcaneOwner::ItemDross(container_id),
            &definition.resonance,
            dross_units,
            Some(&definition.output_item),
        );
    }
    inventory.slots[slot] = Some(ItemStack {
        item: it(&world.reg, &definition.output_item),
        count: 1,
        durability: world
            .reg
            .item(it(&world.reg, &definition.output_item))
            .durability,
        arcane_id: container_id,
    });
    container_id
}

#[test]
fn base_apothecary_roster_is_closed_physical_and_volume_balanced() {
    let reg = base_reg();
    assert_eq!(reg.preparations.len(), PreparationHandler::ALL.len());
    let handlers = reg
        .preparations
        .values()
        .map(|definition| definition.handler)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(handlers.len(), PreparationHandler::ALL.len());
    for definition in reg.preparations.values() {
        assert_eq!(
            definition.solvent_units,
            u64::from(definition.doses) * definition.dose_units,
            "{} copied or lost carrier volume",
            definition.id
        );
        assert!(!definition.ingredients.is_empty());
        assert!(!definition.steps.is_empty());
        assert_eq!(definition.steps[0], crate::alchemy::ProcessStep::Grind);
        assert!(
            definition
                .steps
                .contains(&crate::alchemy::ProcessStep::Charge)
        );
        for item in [
            &definition.solvent_item,
            &definition.output_item,
            &definition.empty_vessel,
            &definition.residue_item,
        ] {
            assert!(
                reg.item_id(item).is_some(),
                "{} references missing {item}",
                definition.id
            );
        }
        let output = reg.item(reg.item_id(&definition.output_item).unwrap());
        assert_eq!(output.max_stack, 1);
        assert!(
            output.arcane.is_some(),
            "{} dose has no durable Current shell",
            definition.id
        );
        assert!(definition.dross_units <= definition.charge_units);
    }
}

#[test]
fn every_base_preparation_completes_through_its_real_apparatus_sequence() {
    use crate::alchemy::{ApparatusKind, CarrierKind};

    let ids = base_reg().preparations.keys().cloned().collect::<Vec<_>>();
    for preparation_id in ids {
        let tag = format!("alchemy-roster-{}", preparation_id.replace(':', "-"));
        let mut world = super::implements::embodied_implements_world(&tag);
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
                    world.clock = world.clock.max((due + 1) as f64 / 20.0);
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
                    world.clock = world.clock.max((due + 1) as f64 / 20.0);
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

#[test]
fn hearth_healing_pays_hunger_and_nutrients_as_health_moves() {
    let mut world = super::implements::embodied_implements_world("alchemy-hearth-effect");
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
    world.clock = definition.effect.duration_ticks as f64 / 40.0;
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
fn root_wash_improves_only_viable_growth_and_overconcentration_burns() {
    let mut world = super::implements::embodied_implements_world("alchemy-root-wash");
    let plot = bp(8, 100, 8);
    let now = (world.clock * 20.0).round() as u64;
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
    world.clock += 6.0;
    assert_eq!(world.root_uptake_multiplier_at(plot), 1.0);
}

#[test]
fn embodied_root_wash_returns_water_feeds_only_soil_and_salts_on_overdose() {
    let mut world = super::implements::embodied_implements_world("alchemy-root-wash-use");
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
    let mut world = super::implements::embodied_implements_world("alchemy-ashlace-transfer");
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

#[test]
fn antidote_status_survives_reopen_then_death_settles_exactly_once() {
    let mut world = super::implements::embodied_implements_world("alchemy-antidote-lifecycle");
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
    loaded.clock = status
        .started_tick
        .saturating_add(definition.effect.duration_ticks / 2) as f64
        / 20.0;
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
    let mut world = super::implements::embodied_implements_world("alchemy-status-exclusion");
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
    world.clock = status.due_tick.saturating_add(1) as f64 / 20.0;
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

    world.clock = status.recovery_until_tick.saturating_add(1) as f64 / 20.0;
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
fn hearth_overdose_is_one_bounded_sickness_independent_of_tick_cadence() {
    let mut world = super::implements::embodied_implements_world("alchemy-hearth-overdose");
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
    world.clock = sickness_end.saturating_add(200) as f64 / 20.0;
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

    world.clock = sickness_end.saturating_add(400) as f64 / 20.0;
    let settled = world
        .tick_preparation_statuses(actor, at, result.physiology)
        .unwrap();
    assert!((settled.physiology.hunger - 9.5).abs() < f32::EPSILON);
}

#[test]
fn frostlace_never_reverses_age_and_its_current_owner_survives_reopen_reconcile() {
    let mut world = super::implements::embodied_implements_world("alchemy-frostlace");
    let item_id = 77;
    let status_id = world
        .alchemy_state
        .as_mut()
        .unwrap()
        .allocate_status_id()
        .unwrap();
    world.alchemy_state.as_mut().unwrap().coatings.insert(
        item_id,
        SpecimenCoating {
            status_id,
            item_id,
            source_batch: 1,
            actor: [5; 16],
            applied_pos: bp(8, 100, 8),
            applied_tick: 0,
            expires_tick: 10_000,
            preservation_permille: 250,
            maximum_temperature_millic: 12_000,
            age_paid: 0,
        },
    );
    assert_eq!(world.coated_specimen_age_advance(item_id, 100, 5_000), 25);
    assert_eq!(world.coated_specimen_age_advance(item_id, 1, 5_000), 1);
    assert_eq!(world.coated_specimen_age_advance(item_id, 100, 13_000), 100);
    assert!(
        world
            .alchemy_state()
            .unwrap()
            .active_arcane_ids()
            .contains(&status_owner_id(status_id))
    );
}

#[test]
fn embodied_frostlace_coats_one_botanical_and_returns_jar_and_spent_carrier() {
    let mut world = super::implements::embodied_implements_world("alchemy-frostlace-use");
    let actor = [26; 16];
    let at = bp(8, 100, 8);
    let definition = world.reg.preparations["base:frostlace_suspension"].clone();
    let mut inventory = Inventory::new();
    mint_ready_dose(&mut world, &mut inventory, 0, &definition.id, 8, 2);
    let specimen_id = world
        .arcane_ledger
        .as_mut()
        .unwrap()
        .allocate_item_id()
        .unwrap();
    fund_owner_current(
        &mut world,
        ArcaneOwner::Item(specimen_id),
        "base:stone",
        1,
        Some("base:frostlace_frond"),
    );
    let specimen_item = it(&world.reg, "base:frostlace_frond");
    assert!(world.reg.item(specimen_item).arcane_ecology.is_some());
    inventory.slots[1] = Some(ItemStack {
        item: specimen_item,
        count: 1,
        durability: world.reg.item(specimen_item).durability,
        arcane_id: specimen_id,
    });
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let result = world
        .use_preparation(
            actor,
            "frostlace fixture",
            at,
            &mut inventory,
            0,
            crate::alchemy::AlchemyTarget::Item(specimen_id),
        )
        .unwrap();
    assert_eq!(result.returned_vessel.unwrap().item_name, "base:glass_jar");
    assert_eq!(result.byproduct.unwrap().item_name, "base:spent_carrier");
    let coating = world.alchemy_state().unwrap().coatings[&specimen_id].clone();
    assert_eq!(
        world.coated_specimen_age_advance(specimen_id, 100, 5_000),
        25
    );
    assert!(world.coated_specimen_age_advance(specimen_id, 100, 5_000) > 0);
    world.clock = coating.expires_tick.saturating_add(1) as f64 / 20.0;
    for _ in 0..4 {
        world.tick_alchemy(128).unwrap();
    }
    assert!(
        !world
            .alchemy_state()
            .unwrap()
            .coatings
            .contains_key(&specimen_id)
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
}

#[test]
fn alchemy_state_backup_recovery_and_wire_budget_are_bounded() {
    let dir = tmp_dir("alchemy-state-backup");
    let mut state = AlchemyState::load_or_initialize(&dir, 42).unwrap();
    state.save().unwrap();
    let first_next = state.next_operation_id;
    state.allocate_operation_id().unwrap();
    state.save().unwrap();
    std::fs::write(dir.join(crate::alchemy::ALCHEMY_FILE), b"corrupt").unwrap();
    let recovered = AlchemyState::load_or_initialize(&dir, 42).unwrap();
    assert_eq!(recovered.next_operation_id, first_next);
    assert!(recovered.encode().unwrap().len() as u64 <= crate::alchemy::MAX_ALCHEMY_FILE_BYTES);

    let cue = AlchemyCue {
        pos: bp(1, 80, 1),
        installation_id: 1,
        batch_id: 1,
        revision: 1,
        kind: AlchemyCueKind::Pour,
        intensity: 100,
        color: [1, 2, 3],
        message: "bounded".into(),
    };
    let packet = crate::net::S2C::AlchemyEvent(cue);
    let encoded = postcard::to_allocvec(&packet).unwrap();
    assert!(encoded.len() < 1_024);
}

#[test]
fn alchemy_wire_requests_carry_intent_not_client_authored_chemistry() {
    let pos = bp(3, 90, -4);
    let operate = crate::net::C2S::OperateAlchemy {
        pos,
        expected_revision: Some(17),
        action: ApparatusAction::SetHeat {
            temperature_millic: 42_000,
        },
    };
    let bytes = postcard::to_allocvec(&operate).unwrap();
    assert!(bytes.len() < 512);
    let decoded: crate::net::C2S = postcard::from_bytes(&bytes).unwrap();
    match decoded {
        crate::net::C2S::OperateAlchemy {
            pos: decoded_pos,
            expected_revision,
            action: ApparatusAction::SetHeat { temperature_millic },
        } => {
            assert_eq!(decoded_pos, pos);
            assert_eq!(expected_revision, Some(17));
            assert_eq!(temperature_millic, 42_000);
        }
        other => panic!("wrong alchemy intent packet: {other:?}"),
    }

    let apply = crate::net::C2S::UsePreparation {
        slot: 6,
        target: crate::alchemy::AlchemyTarget::Plot(pos),
    };
    let bytes = postcard::to_allocvec(&apply).unwrap();
    assert!(bytes.len() < 256);
    let decoded: crate::net::C2S = postcard::from_bytes(&bytes).unwrap();
    match decoded {
        crate::net::C2S::UsePreparation {
            slot,
            target: crate::alchemy::AlchemyTarget::Plot(decoded_pos),
        } => {
            assert_eq!(slot, 6);
            assert_eq!(decoded_pos, pos);
        }
        other => panic!("wrong preparation intent packet: {other:?}"),
    }
}

#[test]
fn chest_storage_uses_the_same_preparation_clock_and_cellars_slow_it() {
    use crate::world::{BlockEntity, ChestState};

    let mut world = super::implements::embodied_implements_world("alchemy-cellar-storage");
    let open_pos = bp(8, 100, 8);
    let cellar_pos = bp(12, 20, 8);
    let center = open_pos.chunk();
    world.insert_empty_chunks_for_test(
        (-1..=1)
            .flat_map(|du| (-1..=1).map(move |dv| center.offset(du, dv)))
            .filter(|chunk| !world.has_chunk(*chunk))
            .collect::<Vec<_>>(),
    );
    let chest_block = b(&world.reg, "base:chest");
    let stone = b(&world.reg, "base:stone");
    world.set_block_authored_at(open_pos, chest_block, "alchemy open-storage fixture");
    if let Some(above) = open_pos.offset(0, 1, 0) {
        world.set_block_authored_at(above, crate::registry::AIR, "open sky fixture");
    }
    for du in -1..=1 {
        for dy in 0..=2 {
            for dv in -1..=1 {
                if (du, dy, dv) == (0, 0, 0) || (du, dy, dv) == (0, 1, 0) {
                    continue;
                }
                world.set_block_authored_at(
                    cellar_pos.offset(du, dy, dv).unwrap(),
                    stone,
                    "alchemy cellar enclosure",
                );
            }
        }
    }
    world.set_block_authored_at(cellar_pos, chest_block, "alchemy cellar-storage fixture");
    world.relight_and_cascade(center);
    assert_eq!(world.light_at_pos(open_pos.offset(0, 1, 0).unwrap()).1, 15);
    assert_eq!(world.light_at_pos(cellar_pos.offset(0, 1, 0).unwrap()).1, 0);

    let mut inventory = Inventory::new();
    let open_id = mint_ready_dose(
        &mut world,
        &mut inventory,
        0,
        "base:clear_eye_tincture",
        1,
        0,
    );
    let cellar_id = mint_ready_dose(
        &mut world,
        &mut inventory,
        1,
        "base:clear_eye_tincture",
        1,
        0,
    );
    let before = world.alchemy_state().unwrap().containers[&open_id].expires_tick;
    assert_eq!(
        world.alchemy_state().unwrap().containers[&cellar_id].expires_tick,
        before
    );
    let mut open_chest = ChestState::default();
    open_chest.slots[0] = inventory.slots[0].take();
    world.insert_block_entity_at(open_pos, BlockEntity::Chest(open_chest));
    let mut cellar_chest = ChestState::default();
    cellar_chest.slots[0] = inventory.slots[1].take();
    world.insert_block_entity_at(cellar_pos, BlockEntity::Chest(cellar_chest));

    world.clock = 20.0;
    world.tick_entities(20.0);
    let open = &world.alchemy_state().unwrap().containers[&open_id];
    let cellar = &world.alchemy_state().unwrap().containers[&cellar_id];
    assert_eq!(open.last_storage_tick, 400);
    assert_eq!(cellar.last_storage_tick, 400);
    assert!(
        cellar.expires_tick >= open.expires_tick.saturating_add(300),
        "cellar storage did not quarter the same 400-tick aging interval: open {}, cellar {}",
        open.expires_tick,
        cellar.expires_tick
    );
}

#[test]
fn large_alchemy_census_stays_inside_save_wire_and_server_tick_budgets() {
    use std::time::{Duration, Instant};

    use crate::alchemy::{
        AlchemyApparatusState, AlchemyAuditEvent, ApparatusKind, MAX_ALCHEMY_APPARATUS,
        MAX_ALCHEMY_CONTAINERS, MAX_ALCHEMY_FILE_BYTES, MAX_ALCHEMY_HISTORY, MAX_ALCHEMY_STATUSES,
    };
    use crate::planet::Face;
    use crate::workings::WaterCarrier;

    let dir = tmp_dir("alchemy-large-census");
    let mut state = AlchemyState::load_or_initialize(&dir, 42).unwrap();
    let definition = crate::registry::load(std::path::Path::new("__no_alchemy_budget_mods__"))
        .preparations["base:clear_eye_tincture"]
        .clone();

    for index in 0..MAX_ALCHEMY_APPARATUS {
        let pos = crate::planet::BlockPos::new(
            Face::PosZ,
            u16::try_from(index % 512).unwrap(),
            80,
            u16::try_from(index / 512).unwrap(),
        )
        .unwrap();
        let installation_id = u64::try_from(index).unwrap() + 1;
        state.apparatus.insert(
            pos,
            AlchemyApparatusState {
                installation_id,
                kind: ApparatusKind::Mortar,
                pos,
                revision: 1,
                integrity_permille: 1_000,
                cleanliness_permille: 1_000,
                temperature_millic: 20_000,
                agitation: AgitationKind::Still,
                batch: None,
                residue_materials: Default::default(),
                filter_burden: 0,
                filter_medium: None,
                filter_owner_id: 0,
                filter_medium_materials: Default::default(),
                last_operator: [1; 16],
            },
        );
    }
    for index in 0..MAX_ALCHEMY_STATUSES {
        let id = u64::try_from(index).unwrap() + 1;
        let mut actor = [0u8; 16];
        actor[..8].copy_from_slice(&id.to_le_bytes());
        state.statuses.insert(
            actor,
            vec![ActivePreparationStatus {
                status_id: id,
                preparation_id: definition.id.clone(),
                definition_version: definition.version,
                source_batch: id,
                actor,
                dose_volume_units: definition.dose_units,
                active_current: Current::default(),
                dross_current: Current::default(),
                started_tick: 0,
                last_tick: 0,
                due_tick: 10_000,
                recovery_until_tick: 20_000,
                stack_group: definition.stack_group.clone(),
                completed_units: 0,
                refresh_count: 0,
                overdose_until_tick: 0,
            }],
        );
    }
    for index in 0..MAX_ALCHEMY_HISTORY {
        let id = u64::try_from(index).unwrap() + 1;
        state.history.push_back(AlchemyAuditEvent {
            operation_id: id,
            installation_id: id,
            batch_id: id,
            actor: [1; 16],
            action: "budget_sample".into(),
            preparation_id: definition.id.clone(),
            volume_units: definition.dose_units,
            current_units: definition.charge_units,
            dross_units: definition.dross_units,
            tick: id,
            note: "bounded representative audit event".into(),
        });
    }
    state.next_installation_id = u64::try_from(MAX_ALCHEMY_APPARATUS).unwrap() + 1;
    state.next_batch_id = u64::try_from(MAX_ALCHEMY_STATUSES).unwrap() + 1;
    state.next_status_id = u64::try_from(MAX_ALCHEMY_STATUSES).unwrap() + 1;
    state.next_operation_id = u64::try_from(MAX_ALCHEMY_HISTORY).unwrap() + 1;

    let fixed_bytes = state.encode().unwrap().len();
    const REPRESENTATIVE_CONTAINERS: usize = 8_192;
    for index in 0..REPRESENTATIVE_CONTAINERS {
        let id = u64::try_from(index).unwrap() + 1;
        state.containers.insert(
            id,
            PreparationDose {
                container_id: id,
                preparation_id: definition.id.clone(),
                definition_version: definition.version,
                item_name: definition.output_item.clone(),
                liquid: ExactLiquid {
                    carrier: Some(CarrierKind::Alcohol),
                    volume_units: definition.dose_units,
                    water: Default::default(),
                    carrier_state: WaterCarrier {
                        thermal_millic_hu: 20_000 * i64::try_from(definition.dose_units).unwrap(),
                        dross_subunits: 0,
                    },
                    solutes: std::collections::BTreeMap::from([(
                        definition.solvent_item.clone(),
                        definition.dose_units,
                    )]),
                },
                vessel_materials: Default::default(),
                materials: Default::default(),
                current_units: 1,
                dross_units: 0,
                born_tick: 1,
                expires_tick: 100_000,
                last_storage_tick: 1,
                outcome: BatchOutcome::Ready,
                source_installation: 1,
                source_batch: 1,
            },
        );
    }
    let encode_started = Instant::now();
    let encoded = state.encode().unwrap();
    assert!(
        encode_started.elapsed() < Duration::from_secs(5),
        "representative large alchemy census took too long to validate and encode"
    );
    let sampled_container_bytes = encoded.len().saturating_sub(fixed_bytes);
    // Add eight bytes per entry for larger varints and the final map-length
    // prefix. This projects the supported full container ceiling without
    // allocating 65k heap-heavy Rust records in an ordinary test runner.
    let projected_per_container = sampled_container_bytes.div_ceil(REPRESENTATIVE_CONTAINERS) + 8;
    let projected_full_bytes = fixed_bytes
        + projected_per_container
            .checked_mul(MAX_ALCHEMY_CONTAINERS)
            .unwrap();
    assert!(
        u64::try_from(projected_full_bytes).unwrap() <= MAX_ALCHEMY_FILE_BYTES,
        "declared full container census projects to {projected_full_bytes} bytes"
    );

    let mut world = super::implements::embodied_implements_world("alchemy-budget-ticks");
    world.alchemy_state = Some(state);
    world.clock = 1.0;
    let maintenance_started = Instant::now();
    world.tick_alchemy(128).unwrap();
    assert!(
        maintenance_started.elapsed() < Duration::from_secs(1),
        "bounded maintenance scaled with the whole census instead of its visit budget"
    );
    let mut first_actor = [0u8; 16];
    first_actor[..8].copy_from_slice(&1u64.to_le_bytes());
    let status_started = Instant::now();
    world
        .tick_preparation_statuses(first_actor, bp(8, 100, 8), PreparationPhysiology::default())
        .unwrap();
    assert!(
        status_started.elapsed() < Duration::from_secs(1),
        "one actor status tick cloned or scanned the whole alchemy census"
    );
}

#[test]
fn preparation_modifiers_are_closed_and_expire_without_hidden_state() {
    let mut world = super::implements::embodied_implements_world("alchemy-modifiers");
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
    world.clock = 6.0;
    assert_eq!(world.preparation_modifiers(actor).trace_sight, 0);
}

#[test]
fn settling_draught_limits_new_working_strain_without_erasing_existing_strain() {
    let mut world = super::implements::embodied_implements_world("alchemy-settling-strain");
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
    world.clock = 1.0;
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

#[test]
fn embodied_hearth_batch_is_exact_transactional_and_cannot_overfill() {
    let mut world = super::implements::embodied_implements_world("alchemy-hearth-lifecycle");
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
    world.clock = 100.0;
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
    world.clock += 20.0;
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

#[test]
fn bounded_maintenance_is_round_robin_and_damaged_apparatus_leaks_legibly() {
    let mut world = super::implements::embodied_implements_world("alchemy-maintenance-fairness");
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
    let mut world = super::implements::embodied_implements_world("alchemy-break-guard");
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

    let mut clean_world = super::implements::embodied_implements_world("alchemy-clean-break");
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
    let mut world = super::implements::embodied_implements_world("alchemy-apparatus-repair");
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

#[test]
fn ordinary_carriers_use_timed_apparatus_without_free_water_or_magic_yield() {
    use crate::planet_atlas::{HYDRO_UNITS_PER_BLOCK, WaterClass};

    let mut world = super::implements::embodied_implements_world("alchemy-ordinary-carriers");
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
    world.clock = (due + 1) as f64 / 20.0;
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
    world.clock = (due + 1) as f64 / 20.0;
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
    let mut world = super::implements::embodied_implements_world("alchemy-automation-parity");
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
    world.clock = (manual_job.due_tick + 1) as f64 / 20.0;
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
