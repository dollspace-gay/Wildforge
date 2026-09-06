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
    let now = (world.clock().max(0.0) * 20.0).round() as u64;
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

mod apparatus_sequences;
mod maintenance;
mod nutrition;
mod ordinary_carriers;
mod persistence;
mod preservation;
mod roster;
mod soil;
mod status;
mod transactions;
