//! Discovery scenarios.

use super::*;

#[test]
fn mounted_survey_folio_preserves_signed_records_through_save_and_spill() {
    let reg = base_reg();
    let root = tmp_dir("discovery-folio-persistence").join("world");
    let mut world = World::load_or_create(root.clone(), reg.clone()).unwrap();
    let surface = world.qualified_spawn_surface().unwrap_or_else(|| {
        crate::planet::SurfacePos::new(
            crate::planet::Face::PosZ,
            crate::planet::FACE_BLOCKS / 2,
            crate::planet::FACE_BLOCKS / 2,
        )
        .unwrap()
    });
    world.ensure_chunk(ChunkPos::from_surface(surface));
    let y = (world.surface_height_at(surface) + 1) as u8;
    let pos = crate::planet::BlockPos::new(surface.face(), surface.u(), y, surface.v()).unwrap();

    let mut ledger = ItemStack::new(&reg, reg.item_id("base:field_ledger").unwrap(), 1);
    world.bind_discovery_stack_at(pos, &mut ledger).unwrap();
    let record = world
        .record_observation(
            ledger.arcane_id,
            (crate::identity::PlayerId([3; 16]), "FERN"),
            crate::world::ObservationTarget::Region(pos),
            crate::discovery::CalibrationGrade::Field,
            Some("spring well".into()),
            None,
        )
        .unwrap();
    let mut folio = ItemStack::new(&reg, reg.item_id("base:survey_folio").unwrap(), 1);
    world.bind_discovery_stack_at(pos, &mut folio).unwrap();
    world
        .copy_discovery_record(ledger.arcane_id, record.record_id, folio.arcane_id, false)
        .unwrap();
    assert!(world.place_item_block_at(pos, folio));
    save_world(&mut world);

    let mut loaded = World::load_or_create(root, reg.clone()).unwrap();
    loaded.ensure_chunk(ChunkPos::from_surface(surface));
    assert!(matches!(
        loaded.block_entity_at(&pos),
        Some(crate::world::BlockEntity::SurveyFolio(state)) if state.object_id == folio.arcane_id
    ));
    let records = loaded.discovery_summaries(folio.arcane_id, true).unwrap();
    assert_eq!(records.len(), 1);
    assert!(records[0].location.is_none());
    assert_eq!(records[0].label.as_deref(), Some("spring well"));

    loaded.set_block_at(pos, AIR);
    assert!(
        loaded
            .pending_drops()
            .iter()
            .any(|(_, stack)| { stack.item == folio.item && stack.arcane_id == folio.arcane_id })
    );
    assert_eq!(
        loaded.discovery_summaries(folio.arcane_id, true).unwrap(),
        records
    );
}

#[test]
fn worn_lens_keeps_its_fitted_plate_and_accepts_a_replacement_element() {
    let reg = base_reg();
    let root = tmp_dir("discovery-lens-refit").join("world");
    let mut world = World::load_or_create(root, reg.clone()).unwrap();
    let surface = world.qualified_spawn_surface().unwrap_or_else(|| {
        crate::planet::SurfacePos::new(
            crate::planet::Face::PosZ,
            crate::planet::FACE_BLOCKS / 2,
            crate::planet::FACE_BLOCKS / 2,
        )
        .unwrap()
    });
    world.ensure_chunk(ChunkPos::from_surface(surface));
    let pos = crate::planet::BlockPos::new(surface.face(), surface.u(), 100, surface.v()).unwrap();
    for kind in crate::discovery::ExperimentKind::ALL {
        let mut reference = ItemStack::new(&reg, reg.item_id(kind.reference_item()).unwrap(), 1);
        world.bind_discovery_stack_at(pos, &mut reference).unwrap();
        assert_ne!(reference.arcane_id, 0);
        assert!(matches!(
            &world
                .discovery_state
                .as_ref()
                .unwrap()
                .object(reference.arcane_id)
                .unwrap()
                .kind,
            crate::discovery::KnowledgeKind::ReferenceObject { experiment }
                if *experiment == kind
        ));
    }
    let mut inventory = crate::inventory::Inventory::new();
    inventory.slots[0] = Some(ItemStack::new(
        &reg,
        reg.item_id("base:tuning_lens_frame").unwrap(),
        1,
    ));
    inventory.slots[1] = Some(ItemStack::new(
        &reg,
        reg.item_id("base:echo_slate").unwrap(),
        1,
    ));
    inventory.slots[2] = Some(ItemStack::new(
        &reg,
        reg.item_id("base:wellglass_shard").unwrap(),
        1,
    ));
    let lens = world.assemble_tuning_lens_at(pos, &mut inventory).unwrap();
    assert_ne!(lens.arcane_id, 0);
    inventory.slots[0].as_mut().unwrap().durability = 1;
    assert!(world.wear_tuning_lens_at(pos, &mut inventory, 0));
    let mount = inventory.slots[0].unwrap();
    assert_eq!(reg.item(mount.item).name, "base:tuning_lens_mount");
    assert_eq!(
        reg.item(mount.item).materials,
        reg.item(lens.item).materials,
        "the fitted plate remains physical when the element wears out"
    );
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_current_total(lens.arcane_id),
        None
    );

    inventory.slots[2] = Some(ItemStack::new(
        &reg,
        reg.item_id("base:wellglass_shard").unwrap(),
        1,
    ));
    let replacement = world.assemble_tuning_lens_at(pos, &mut inventory).unwrap();
    assert_eq!(replacement.item, lens.item);
    assert!(
        inventory
            .slots
            .iter()
            .flatten()
            .all(|stack| stack.item != reg.item_id("base:echo_slate").unwrap())
    );
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
fn experiment_apparatus_holds_repeatable_samples_through_save_and_spill() {
    let reg = base_reg();
    let root = tmp_dir("discovery-apparatus-persistence").join("world");
    let mut world = World::load_or_create(root.clone(), reg.clone()).unwrap();
    let surface = world.qualified_spawn_surface().unwrap_or_else(|| {
        crate::planet::SurfacePos::new(
            crate::planet::Face::PosZ,
            crate::planet::FACE_BLOCKS / 2,
            crate::planet::FACE_BLOCKS / 2,
        )
        .unwrap()
    });
    world.ensure_chunk(ChunkPos::from_surface(surface));
    let y = (world.surface_height_at(surface) + 1) as u8;
    let pos = crate::planet::BlockPos::new(surface.face(), surface.u(), y, surface.v()).unwrap();
    assert!(world.place_block_at(pos, reg.block_id("base:experiment_apparatus").unwrap()));
    let mut inventory = crate::inventory::Inventory::new();
    inventory.slots[0] = Some(ItemStack::new(&reg, reg.item_id("base:dirt").unwrap(), 3));
    inventory.slots[1] = Some(ItemStack::new(
        &reg,
        reg.item_id("base:capacity_reference").unwrap(),
        1,
    ));
    world
        .exchange_experiment_item_at(pos, &mut inventory, 0)
        .unwrap();
    world
        .exchange_experiment_item_at(pos, &mut inventory, 1)
        .unwrap();
    assert_eq!(inventory.slots[0].unwrap().count, 2);
    assert!(inventory.slots[1].is_none());

    let first = world
        .experiment_sample_at(pos, crate::discovery::ExperimentKind::Capacity)
        .unwrap();
    let second = world
        .experiment_sample_at(pos, crate::discovery::ExperimentKind::Capacity)
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(first.item, reg.item_id("base:dirt").unwrap());
    save_world(&mut world);

    let mut loaded = World::load_or_create(root, reg.clone()).unwrap();
    loaded.ensure_chunk(ChunkPos::from_surface(surface));
    assert_eq!(
        loaded
            .experiment_sample_at(pos, crate::discovery::ExperimentKind::Capacity)
            .unwrap(),
        first
    );
    loaded.set_block_at(pos, AIR);
    let spills = loaded
        .pending_drops()
        .iter()
        .filter(|(_, stack)| {
            matches!(
                reg.item(stack.item).name.as_str(),
                "base:dirt" | "base:capacity_reference"
            )
        })
        .count();
    assert_eq!(spills, 2);
}
