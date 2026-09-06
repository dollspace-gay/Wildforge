//! Imports scenarios.

use super::*;

#[test]
fn deep_origin_dross_import_extends_geography_boundary_and_keeps_provenance() {
    let (mut world, root) = dross_world("dross-external-geography-import", 0xd205_5200);
    let region = AtlasPos {
        face: Face::PosZ,
        u: 8,
        v: 8,
    };
    let owner = ArcaneOwner::Dross {
        region,
        medium: DrossMedium::Air,
    };
    let actor = [0x42; 16];
    let (transaction_id, from_version, to_version, current) = {
        let ledger = world.arcane_ledger.as_mut().unwrap();
        let mut current = ledger.account(&ArcaneOwner::Deep).unwrap().current.clone();
        let current = current
            .take_units(37, BASE_RESONANCES.into_iter().map(str::to_string))
            .unwrap();
        (
            ledger.system_transaction_id().unwrap(),
            ledger.version_of(&ArcaneOwner::Deep),
            ledger.version_of(&owner),
            current,
        )
    };
    let mut transaction = ArcaneTransaction::transfer(
        transaction_id,
        ArcaneOwner::Deep,
        from_version,
        owner.clone(),
        to_version,
        current.clone(),
        ArcaneAuthority::SystemForPlayer(actor),
        "fixture external deep waste",
    );
    transaction.content_id = "base:test_deep_waste".into();
    world
        .arcane_ledger
        .as_mut()
        .unwrap()
        .commit(transaction)
        .unwrap();
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .dross_generated_by_process["base:test_deep_waste"],
        37
    );

    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&owner)
            .unwrap()
            .current,
        current
    );
    let imported = world.tick_dross(31).unwrap();
    assert_eq!(imported.completed_hour, None);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&owner)
            .is_none_or(|account| account.current.is_empty())
    );

    let geography = world.arcane_geography.as_ref().unwrap();
    assert_eq!(
        geography
            .dynamic
            .dross_state
            .external_imported
            .into_iter()
            .sum::<u64>(),
        37
    );
    assert_eq!(
        geography.dense_dross_current_at(region, crate::dross::DrossCarrier::Air),
        current
    );
    assert_eq!(
        geography.dynamic.dross_state.generated_by_process["base:test_deep_waste: fixture external deep waste"],
        37
    );
    let provenance = &geography.dynamic.dross_state.provenance[&region];
    assert_eq!(provenance.total_units(), 37);
    assert_eq!(provenance.entries[0].actor, Some(actor));
    assert_eq!(provenance.entries[0].confidence_permille, 1_000);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .is_balanced()
    );
    assert!(geography.audit().unwrap().is_balanced());

    let surface = surface_atlas_center(&world, region);
    let at = crate::planet::BlockPos::new(surface.face(), surface.u(), 100, surface.v()).unwrap();
    let mut field_ledger =
        crate::inventory::ItemStack::new(&world.reg, it(&world.reg, "base:field_ledger"), 1);
    world
        .bind_discovery_stack_at(at, &mut field_ledger)
        .unwrap();
    let observation = world
        .record_observation(
            field_ledger.arcane_id,
            (crate::identity::PlayerId([0x11; 16]), "FIELDWORKER"),
            crate::world::ObservationTarget::Region(at),
            crate::discovery::CalibrationGrade::Field,
            Some("downwind sample".into()),
            None,
        )
        .unwrap();
    let signature = observation
        .properties
        .iter()
        .find_map(|(key, value)| (key == "source signature").then_some(value))
        .expect("tuning-lens record omitted bounded source evidence");
    assert!(signature.contains("likely"));
    assert!(!signature.contains("42"));

    save_world(&mut world);
    drop(world);
    let operator = crate::dross::audit_world(&root).unwrap();
    assert_eq!(operator.generated_by_process["base:test_deep_waste"], 37);
    assert!(operator.render().contains("base:test_deep_waste"));
    let loaded = World::load_or_create(root, base_reg()).unwrap();
    let geography = loaded.arcane_geography.as_ref().unwrap();
    assert_eq!(
        geography
            .dynamic
            .dross_state
            .external_imported
            .into_iter()
            .sum::<u64>(),
        37
    );
    assert_eq!(
        geography.dynamic.dross_state.provenance[&region].total_units(),
        37
    );
    assert_eq!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .dross_generated_by_process["base:test_deep_waste"],
        37
    );
    assert!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .is_balanced()
    );
    assert!(geography.audit().unwrap().is_balanced());
}

#[test]
fn destructive_scar_disposition_enters_warning_ladder_without_orphan_owner() {
    let (mut world, _) = dross_world("dross-scar-disposition-ladder", 0xd205_5204);
    let region = AtlasPos {
        face: Face::PosZ,
        u: 8,
        v: 8,
    };
    let surface = surface_atlas_center(&world, region);
    world.ensure_chunk(ChunkPos::from_surface(surface));
    let at = crate::planet::BlockPos::new(
        surface.face(),
        surface.u(),
        (world.surface_height_at(surface) + 1) as u8,
        surface.v(),
    )
    .unwrap();
    let mut stack =
        crate::inventory::ItemStack::new(&world.reg, it(&world.reg, "base:heartwood"), 1);
    world
        .bind_arcane_stack_at(at, &mut stack, "scar-disposition fixture")
        .unwrap();
    let units = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .account(&ArcaneOwner::Item(stack.arcane_id))
        .unwrap()
        .current
        .total();
    assert!(units > 0);
    world.retire_arcane_stack_at(at, stack, "destructive scar-disposition fixture");

    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert!(
        ledger
            .accounts
            .keys()
            .all(|owner| !matches!(owner, ArcaneOwner::Scar(_)))
    );
    let sparse = ArcaneOwner::Dross {
        region,
        medium: DrossMedium::Soil,
    };
    assert_eq!(ledger.account(&sparse).unwrap().current.total(), units);
    assert!(
        world
            .arcane_geography
            .as_ref()
            .unwrap()
            .dynamic
            .dross_state
            .scars
            .is_empty()
    );

    world.tick_dross(31).unwrap();
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&sparse)
            .is_none_or(|account| account.current.is_empty())
    );
    let geography = world.arcane_geography.as_ref().unwrap();
    assert_eq!(geography.dense_dross_total_at(region), units);
    assert_eq!(geography.dross_band_at(region), DrossBand::Clear);
    assert!(geography.dynamic.dross_state.scars.is_empty());
    assert!(geography.audit().unwrap().is_balanced());
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .is_balanced()
    );
}

#[test]
fn concurrent_dump_and_cleanup_commit_once_and_attribute_the_actual_dumper() {
    let (mut world, _) = dross_world("dross-concurrent-dump-cleanup", 0xd205_5205);
    let region = AtlasPos {
        face: Face::PosZ,
        u: 8,
        v: 8,
    };
    let process = ArcaneOwner::Alchemy(99);
    let apparatus = ArcaneOwner::AlchemyDross(99);
    let regional = ArcaneOwner::Dross {
        region,
        medium: DrossMedium::Water,
    };
    let captured = ArcaneOwner::ItemDross(500);
    let dumper = [0xa1; 16];
    let cleaner = [0xb2; 16];

    let mut current = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .account(&ArcaneOwner::Deep)
        .unwrap()
        .current
        .clone();
    let current = current
        .take_units(20, BASE_RESONANCES.into_iter().map(str::to_string))
        .unwrap();
    let transaction_id = world
        .arcane_ledger
        .as_mut()
        .unwrap()
        .system_transaction_id()
        .unwrap();
    let ledger = world.arcane_ledger.as_mut().unwrap();
    ledger
        .commit(ArcaneTransaction::transfer(
            transaction_id,
            ArcaneOwner::Deep,
            ledger.version_of(&ArcaneOwner::Deep),
            process.clone(),
            ledger.version_of(&process),
            current.clone(),
            ArcaneAuthority::System,
            "fund concurrent dross fixture",
        ))
        .unwrap();
    let transaction_id = ledger.system_transaction_id().unwrap();
    let mut generated = ArcaneTransaction::transfer(
        transaction_id,
        process.clone(),
        ledger.version_of(&process),
        apparatus.clone(),
        ledger.version_of(&apparatus),
        current.clone(),
        ArcaneAuthority::SystemForPlayer(dumper),
        "unsafe apparatus generated dross",
    );
    generated.content_id = "base:test_unsafe_apparatus".into();
    ledger.commit(generated).unwrap();
    let mut first_half = current.clone();
    let first_half = first_half.take_units(10, std::iter::empty()).unwrap();
    let mut second_half = current;
    second_half.checked_sub(&first_half).unwrap();
    let transaction_id = ledger.system_transaction_id().unwrap();
    let first_dump = ArcaneTransaction::transfer(
        transaction_id,
        apparatus.clone(),
        ledger.version_of(&apparatus),
        regional.clone(),
        ledger.version_of(&regional),
        first_half.clone(),
        ArcaneAuthority::SystemForPlayer(dumper),
        "first attributed wastewater dump",
    );
    ledger.commit(first_dump).unwrap();

    let dump_id = ledger.system_transaction_id().unwrap();
    let cleanup_id = ledger.system_transaction_id().unwrap();
    let concurrent_dump = ArcaneTransaction::transfer(
        dump_id,
        apparatus.clone(),
        ledger.version_of(&apparatus),
        regional.clone(),
        ledger.version_of(&regional),
        second_half.clone(),
        ArcaneAuthority::SystemForPlayer(dumper),
        "concurrent attributed wastewater dump",
    );
    let stale_cleanup = ArcaneTransaction::transfer(
        cleanup_id,
        regional.clone(),
        ledger.version_of(&regional),
        captured.clone(),
        ledger.version_of(&captured),
        first_half.clone(),
        ArcaneAuthority::SystemForPlayer(cleaner),
        "capture regional dross into finite cargo",
    );
    assert!(matches!(
        ledger.commit(concurrent_dump.clone()).unwrap(),
        crate::arcane::CommitOutcome::Applied(_)
    ));
    assert!(matches!(
        ledger.commit(stale_cleanup),
        Err(crate::arcane::ArcaneError::VersionConflict { .. })
    ));
    let retry = ArcaneTransaction::transfer(
        cleanup_id,
        regional.clone(),
        ledger.version_of(&regional),
        captured.clone(),
        ledger.version_of(&captured),
        first_half,
        ArcaneAuthority::SystemForPlayer(cleaner),
        "capture regional dross into finite cargo",
    );
    assert!(matches!(
        ledger.commit(retry).unwrap(),
        crate::arcane::CommitOutcome::Applied(_)
    ));
    assert!(matches!(
        ledger.commit(concurrent_dump).unwrap(),
        crate::arcane::CommitOutcome::AlreadyApplied(_)
    ));
    assert_eq!(ledger.account(&captured).unwrap().current.total(), 10);
    assert_eq!(ledger.account(&regional).unwrap().current.total(), 10);
    assert_eq!(ledger.dross_generated_total, 20);
    assert_eq!(
        ledger.dross_generated_by_process["base:test_unsafe_apparatus"],
        20
    );

    world.tick_dross(31).unwrap();
    let provenance = &world
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .dross_state
        .provenance[&region];
    assert_eq!(provenance.total_units(), 10);
    assert_eq!(provenance.entries.len(), 1);
    assert_eq!(provenance.entries[0].actor, Some(dumper));
    assert_eq!(provenance.entries[0].installation_id, Some(99));
    assert_ne!(provenance.entries[0].actor, Some(cleaner));
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .is_balanced()
    );
    assert!(
        world
            .arcane_geography
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .is_balanced()
    );
}
