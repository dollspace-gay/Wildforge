//! Holdfast scenarios.

use super::*;

#[test]
fn holdfast_only_slows_real_elapsed_age_and_ends_on_schedule() {
    let (mut world, source, wand) = workings_world("workings-holdfast");
    let actor = [26; 16];
    let mut inventory = Inventory::new();
    inventory.slots[1] = Some(ItemStack::new(&world.reg, it(&world.reg, "base:bread"), 1));
    let initial = inventory.slots[1].unwrap();
    let started = world
        .begin_holdfast_working(
            actor,
            "holdfast-worker",
            source,
            wand.arcane_id,
            &inventory,
            1,
            20 * 60,
            false,
        )
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Working(started.stable_id))
            .is_some()
    );
    let first = world.holdfast_age_step(actor, 1, initial, 20, 20);
    assert_eq!(first, 5);
    let aged = ItemStack {
        durability: initial.durability - first,
        ..initial
    };
    let second = world.holdfast_age_step(actor, 1, aged, 20, 20);
    assert_eq!(second, 5);
    let effect = &world.workings_state.as_ref().unwrap().active[&started.stable_id].effect;
    assert!(matches!(
        effect,
        crate::workings::WorkingEffect::Preserve {
            elapsed_ticks: 40,
            age_advance_ticks: 10,
            ..
        }
    ));
    assert!(second <= 20, "Holdfast may never reverse elapsed age");

    world.set_simulation_clock(61.0);
    let outcomes = world.tick_workings();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].0.stable_id, started.stable_id);
    assert_eq!(
        outcomes[0].0.cue,
        crate::workings::WorkingCueKind::Complete,
        "{}",
        outcomes[0].0.message
    );
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Working(started.stable_id))
            .is_none()
    );
    assert_eq!(
        world.holdfast_age_step(actor, 1, aged, 20, 20),
        20,
        "an ended binding must grant no permanent passive benefit"
    );
}

#[test]
fn holdfast_slows_a_real_mounted_heart_seed_and_expired_seed_stays_dead() {
    let (mut world, source, wand) = workings_world("workings-holdfast-mounted-seed");
    let mount = source.offset(2, 0, 0).unwrap();
    assert!(world.place_block_at(mount, b(&world.reg, "base:experiment_apparatus")));
    let seed_item = it(&world.reg, "base:forest_seed");
    let seed = ItemStack::new(&world.reg, seed_item, 1);
    let crate::world::BlockEntity::DiscoveryApparatus(apparatus) =
        world.block_entity_mut_at(&mount).unwrap()
    else {
        panic!("the physical sample mount has no authoritative state")
    };
    apparatus.sample = Some(seed);
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;

    let started = world
        .begin_wand_working(
            [43; 16],
            "mounted-seed-worker",
            source,
            wand.arcane_id,
            "base:holdfast",
            crate::workings::WorkingTargetIntent::Block {
                pos: mount,
                adjacent: None,
            },
            None,
            false,
        )
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    let cue = world
        .working_cues()
        .into_iter()
        .find(|cue| cue.stable_id == started.stable_id)
        .unwrap();
    assert_eq!(cue.path, vec![source, mount]);

    world.tick_entities(20.1);
    let preserved = match world.block_entity_at(&mount).unwrap() {
        crate::world::BlockEntity::DiscoveryApparatus(apparatus) => apparatus.sample.unwrap(),
        _ => unreachable!(),
    };
    assert_eq!(seed.durability - preserved.durability, 3);

    world.release_working(started.stable_id).unwrap();
    world.tick_entities(20.0);
    let ordinary = match world.block_entity_at(&mount).unwrap() {
        crate::world::BlockEntity::DiscoveryApparatus(apparatus) => apparatus.sample.unwrap(),
        _ => unreachable!(),
    };
    assert_eq!(preserved.durability - ordinary.durability, 10);
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        total_before
    );

    let expired = ItemStack {
        durability: 0,
        ..seed
    };
    let refusal = world
        .plant_heart_seed_stack_at(source, expired)
        .expect("an expired heart cutting must not root");
    assert!(refusal.contains("living interval has spent itself"));
}

#[test]
fn holdfast_slows_real_botanical_charge_leakage_and_only_spends_while_working() {
    use crate::arcane::ArcaneOwner;

    let (mut world, source, wand) = workings_world("workings-holdfast-botanical");
    let actor = [35; 16];
    let item = it(&world.reg, "base:rainbell_dew");
    let definition = world.reg.item(item).clone();
    let arcane = definition.arcane.clone().unwrap();
    let item_id = world
        .arcane_ledger
        .as_mut()
        .unwrap()
        .bind_new_item(
            ArcaneOwner::Deep,
            &arcane,
            &definition.name,
            "Holdfast charged botanical fixture",
        )
        .unwrap();
    let mut specimen = ItemStack::new(&world.reg, item, 1);
    specimen.arcane_id = item_id;
    let mut inventory = Inventory::new();
    inventory.slots[2] = Some(specimen);
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;

    let started = world
        .begin_holdfast_working(
            actor,
            "holdfast-botanical",
            source,
            wand.arcane_id,
            &inventory,
            2,
            20 * 60,
            false,
        )
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    let slowed = world
        .leak_fragile_item_charge(actor, 2, specimen, source, 20)
        .unwrap();
    assert_eq!(slowed, 2, "480 permille instability leaks 5 units normally");
    let transaction = &world.workings_state.as_ref().unwrap().active[&started.stable_id];
    assert!(matches!(
        transaction.effect,
        crate::workings::WorkingEffect::Preserve {
            preservation_kind: crate::workings::PreservationKind::ChargeLeakage,
            elapsed_ticks: 5,
            age_advance_ticks: 2,
            charge_spent_units: 60,
            ..
        }
    ));
    let reserved_clean = transaction.return_current.total();
    assert!(reserved_clean > 60);

    world.release_working(started.stable_id).unwrap();
    let ordinary = world
        .leak_fragile_item_charge(actor, 2, specimen, source, 20)
        .unwrap();
    assert_eq!(ordinary, 5, "ended Holdfast must leave no passive benefit");
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        total_before,
        "preservation, leakage, refunds, and dross must conserve Current"
    );
}
