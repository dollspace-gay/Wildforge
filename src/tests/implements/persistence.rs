//! Persistence scenarios.

use super::*;

#[test]
fn interrupted_physical_owner_save_rolls_back_ledger_and_implement_sidecar_together() {
    let mut world = embodied_implements_world("implements-owner-crash-recovery");
    let (frame, _) = install_frame_fixture(&mut world);
    let (wand, _) = assemble_fixture_wand(&mut world, frame);
    let total = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    save_world(&mut world);
    let root = world.save_dir_for_test();
    assert!(
        world
            .implements_state
            .as_ref()
            .unwrap()
            .instance(wand.arcane_id)
            .is_some()
    );
    drop(world);

    // Simulate stopping after the linked ledger + implements commit but
    // before entities.toml materialized the finished object.
    std::fs::remove_file(root.join("entities.toml")).unwrap();
    let recovered = World::load_or_create(root, base_reg()).unwrap();
    assert!(
        recovered
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_current_total(wand.arcane_id)
            .is_none()
    );
    assert!(
        recovered
            .implements_state
            .as_ref()
            .unwrap()
            .instance(wand.arcane_id)
            .is_none(),
        "ledger rollback left ghost implement construction metadata"
    );
    assert_eq!(
        recovered
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        total
    );
    assert!(
        recovered
            .implements_state
            .as_ref()
            .unwrap()
            .audit
            .iter()
            .any(|event| event.kind == "crash_recovery")
    );
}

#[test]
fn implement_save_wire_and_operation_budgets_are_measured_and_enforced() {
    use crate::implements::{
        MAX_IMPLEMENT_FILE_BYTES, MAX_IMPLEMENT_ID_BYTES, MAX_IMPLEMENT_INSTANCE_BYTES,
        MAX_IMPLEMENT_INSTANCES, MAX_IMPLEMENT_PROVENANCE_BYTES, MAX_IMPLEMENT_PUBLIC_BYTES,
    };

    let mut world = embodied_implements_world("implements-budget-audit");
    let (frame, _) = install_frame_fixture(&mut world);
    let (wand, _) = assemble_fixture_wand(&mut world, frame);
    save_world(&mut world);
    let root = world.save_dir_for_test();
    let audit = crate::implements::audit_world(&root).unwrap();
    assert!(audit.is_qualified(), "{}", audit.render());
    assert!(audit.max_instance_bytes <= MAX_IMPLEMENT_INSTANCE_BYTES);
    assert!(audit.max_public_bytes <= MAX_IMPLEMENT_PUBLIC_BYTES);
    assert!(audit.file_bytes <= MAX_IMPLEMENT_FILE_BYTES);

    // Measure a 10,000-instance representative checkpoint using worst-width
    // u64 identities, then conservatively extrapolate the fixed-format base
    // roster to the entire declared census. The hard encoder limit remains
    // authoritative if future content grows per-instance metadata.
    let mut scale = world.implements_state.as_ref().unwrap().clone();
    let template = scale.instance(wand.arcane_id).unwrap().clone();
    scale.instances.clear();
    const SAMPLE: usize = 10_000;
    for index in 0..SAMPLE {
        let mut instance = template.clone();
        instance.instance_id = u64::MAX - index as u64;
        scale.instances.insert(instance.instance_id, instance);
    }
    let encode_started = std::time::Instant::now();
    let bytes = scale.encode().unwrap();
    assert!(
        encode_started.elapsed() < std::time::Duration::from_secs(2),
        "representative implement checkpoint encoded in {:?}",
        encode_started.elapsed()
    );
    let projected = (bytes.len() as u128)
        .saturating_mul(MAX_IMPLEMENT_INSTANCES as u128)
        .div_ceil(SAMPLE as u128);
    assert!(
        projected <= u128::from(MAX_IMPLEMENT_FILE_BYTES),
        "measured full-census projection {projected} exceeds {MAX_IMPLEMENT_FILE_BYTES} bytes"
    );

    let public = crate::implements::ImplementPublicState::from_authority(&template, 17);
    assert!(postcard::to_allocvec(&template).unwrap().len() <= MAX_IMPLEMENT_INSTANCE_BYTES);
    assert!(postcard::to_allocvec(&public).unwrap().len() <= MAX_IMPLEMENT_PUBLIC_BYTES);

    let mut oversized = scale;
    oversized.instances.clear();
    let mut bad_instance = template;
    bad_instance.provenance = "x".repeat(MAX_IMPLEMENT_PROVENANCE_BYTES + 1);
    oversized
        .instances
        .insert(bad_instance.instance_id, bad_instance);
    assert!(oversized.encode().is_err());
    let mut bad_component = world
        .reg
        .item(it(&world.reg, "base:seasoned_wand_body"))
        .wand_component
        .clone()
        .unwrap();
    bad_component.repair_material = "x".repeat(MAX_IMPLEMENT_ID_BYTES + 1);
    assert!(crate::implements::validate_component("fixture:body", &bad_component).is_err());
}
