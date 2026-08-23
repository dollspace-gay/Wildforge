//! World persistence, block mutation, ticks, fluids, lighting, and weather.

use super::*;
use std::collections::HashMap;

fn ecology_world(tag: &str, seed: u32) -> World {
    let dir = tmp_dir(tag);
    let atlas = std::sync::Arc::new(crate::planet_atlas::PlanetAtlas::fixture(seed, 16).unwrap());
    atlas.write_new(&dir).unwrap();
    World::new_with_atlas(seed, dir, base_reg(), atlas)
}

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

fn materialize_ecology_site(world: &mut World, content: &str) -> crate::planet::BlockPos {
    let surface = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .ecology
        .sites
        .iter()
        .find(|site| site.content_id == content)
        .and_then(|site| site.surface())
        .unwrap_or_else(|| panic!("fixture has no {content} site"));
    world.ensure_chunk(ChunkPos::from_surface(surface));
    let geography = world.arcane_geography.as_mut().unwrap();
    let site = geography
        .dynamic
        .ecology
        .sites
        .iter_mut()
        .find(|site| site.content_id == content && site.surface() == Some(surface))
        .unwrap();
    site.stage = crate::arcane_ecology::EcologyStage::Mature;
    if site.materialized_y == 0 {
        site.materialized_y = 80;
    }
    let pos = site.block_pos().unwrap();
    let block = world.reg.block_id(content).unwrap();
    world.set_block_at(pos, block);
    pos
}

#[test]
fn lantern_reed_light_is_driven_by_its_stored_current() {
    let mut world = ecology_world("lantern-current-state", 10_099);
    let pos = materialize_ecology_site(&mut world, "base:lantern_reed");
    let lit = world.reg.block_id("base:lantern_reed").unwrap();
    let dim = world.reg.block_id("base:lantern_reed_dim").unwrap();

    let original_charge = {
        let site = world
            .arcane_geography
            .as_mut()
            .unwrap()
            .dynamic
            .ecology
            .sites
            .iter_mut()
            .find(|site| site.block_pos() == Some(pos))
            .unwrap();
        let charge = site.charge;
        assert!(
            site.charge_total() > 0,
            "genesis lantern has stored Current"
        );
        site.charge = [0; 6];
        charge
    };
    world.refresh_arcane_ecology_for_test();
    assert_eq!(world.get_block_at(pos), dim);
    assert_eq!(world.reg.block(dim).light_emit, 0);
    assert_eq!(world.light_at_pos(pos).0, 0);

    world
        .arcane_geography
        .as_mut()
        .unwrap()
        .dynamic
        .ecology
        .sites
        .iter_mut()
        .find(|site| site.block_pos() == Some(pos))
        .unwrap()
        .charge = original_charge;
    world.refresh_arcane_ecology_for_test();
    assert_eq!(world.get_block_at(pos), lit);
    assert_eq!(world.reg.block(lit).light_emit, 4);
    assert_eq!(world.light_at_pos(pos).0, 4);
}

#[test]
fn simultaneous_wellglass_harvest_resolves_once_and_both_custodies_balance() {
    // Seed 76 has qualified glass-heath habitat in the deliberately tiny
    // fixture; not every finite test planet should be forced to have one.
    let mut world = ecology_world("wellglass-host-once", 76);
    let pos = materialize_ecology_site(&mut world, "base:wellglass_bud");
    let site_id = {
        let geography = world.arcane_geography.as_mut().unwrap();
        let site = geography
            .dynamic
            .ecology
            .sites
            .iter_mut()
            .find(|site| site.block_pos() == Some(pos))
            .unwrap();
        site.crystal_stage = 3;
        site.id
    };
    let before = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .ecology
        .sites
        .iter()
        .find(|site| site.id == site_id)
        .unwrap()
        .charge_total();
    assert!(before > 0);
    let pick = it(&world.reg, "base:iron_pickaxe");
    let ire_before = world.ire;
    let first = world
        .break_block_at(pos, Some(pick), true, true)
        .expect("host accepts first mature harvest");
    assert!(
        world.ire > ire_before,
        "taking, not Current arithmetic, raises Ire"
    );
    let shard = first.drop.expect("wellglass shard");
    assert_ne!(shard.arcane_id, 0);
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_current_total(shard.arcane_id),
        Some(before)
    );
    assert_eq!(world.get_block_at(pos), b(&world.reg, "base:wellglass_bud"));
    assert!(
        world.break_block_at(pos, Some(pick), true, true).is_none(),
        "second same-state host command must not fall through to generic loot"
    );
    let geography = world.arcane_geography.as_ref().unwrap();
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Geography)
            .unwrap()
            .current,
        geography.custody_current().unwrap()
    );
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
fn replanting_a_collapsed_site_restores_it_and_credits_tending() {
    let mut world = ecology_world("ecology-common-seed-restoration", 10_105);
    let pos = materialize_ecology_site(&mut world, "base:rainbell");
    {
        let geography = world.arcane_geography.as_mut().unwrap();
        let site = geography
            .dynamic
            .ecology
            .sites
            .iter_mut()
            .find(|site| site.block_pos() == Some(pos))
            .unwrap();
        site.population = 0;
        site.seed_bank = 0;
        site.stage = crate::arcane_ecology::EcologyStage::Collapsed;
    }
    world.set_block_at(pos, AIR);
    world.ire = 10.0;
    let seed = ItemStack::new(&world.reg, it(&world.reg, "base:rainbell_dew"), 1);
    assert!(world.place_item_block_at(pos, seed));
    let site = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .ecology
        .sites
        .iter()
        .find(|site| site.block_pos() == Some(pos))
        .unwrap();
    assert_eq!(
        site.ownership,
        crate::arcane_ecology::EcologyOwnership::Genesis
    );
    assert_eq!(site.stage, crate::arcane_ecology::EcologyStage::Recovering);
    assert_eq!(site.population, 1);
    assert!(site.seed_bank > 0);
    assert!(
        world.ire < 10.0,
        "successful establishment uses the tending hook"
    );
}

#[test]
fn ashlace_harvest_keeps_sequestered_dross_on_the_item_and_composting_returns_it() {
    let mut world = ecology_world("ashlace-item-dross", 10_102);
    let pos = materialize_ecology_site(&mut world, "base:ashlace");
    {
        let geography = world.arcane_geography.as_mut().unwrap();
        let site_index = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .position(|site| site.block_pos() == Some(pos))
            .unwrap();
        let cell_index = geography.dynamic.ecology.sites[site_index]
            .atlas_pos
            .index(geography.manifest.side);
        let slot = geography.dynamic.cells[cell_index]
            .ambient
            .iter()
            .enumerate()
            .max_by_key(|(_, units)| **units)
            .map(|(slot, _)| slot)
            .unwrap();
        let moved = geography.dynamic.cells[cell_index].ambient[slot].min(90);
        geography.dynamic.cells[cell_index].ambient[slot] -= moved;
        geography.dynamic.ecology.sites[site_index].dross[slot] += u32::from(moved);
        geography.dynamic.ecology.sites[site_index].charge = [0; 6];
    }
    let drop = world
        .break_block_at(pos, None, true, false)
        .unwrap()
        .drop
        .unwrap();
    let ledger = world.arcane_ledger.as_ref().unwrap();
    let dross = ledger.item_dross_total(drop.arcane_id);
    assert!(dross > 0);
    assert!(
        ledger
            .account(&crate::arcane::ArcaneOwner::Item(drop.arcane_id))
            .is_none(),
        "sequestered dross must not be relabeled as clean bound Current"
    );
    assert_eq!(crate::world::soil::compost_value("base:ashlace_tissue"), 1);
    let total_before = ledger.audit().unwrap().total;
    world.retire_arcane_stack_at(pos, drop, "ashlace tissue composted");
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert_eq!(ledger.item_current_total(drop.arcane_id), None);
    assert_eq!(ledger.audit().unwrap().total, total_before);
    assert!(ledger.audit().unwrap().is_balanced());
}

#[test]
fn rainbell_dew_moves_real_soil_water_into_circulating_custody() {
    let mut world = ecology_world("rainbell-real-water", 10_103);
    let pos = materialize_ecology_site(&mut world, "base:rainbell");
    let before = world.live_water_audit().unwrap();
    let soil_before = world.ecology_soil_water_hu_at(pos.surface()).unwrap();
    assert!(
        soil_before >= crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL,
        "wetland fixture needs one dew measure of live soil water"
    );
    let drop = world
        .break_block_at(pos, None, true, false)
        .expect("water-backed rainbell harvest")
        .drop
        .unwrap();
    assert_eq!(drop.item, it(&world.reg, "base:rainbell_dew"));
    let after = world.live_water_audit().unwrap();
    assert_eq!(after.current_water_hu, before.current_water_hu);
    assert_eq!(
        after.unexplained_water_delta_hu, before.unexplained_water_delta_hu,
        "dew harvest must introduce no new water-audit delta"
    );
    assert_eq!(
        after.industrial.water_hu - before.industrial.water_hu,
        crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
    );
    assert_eq!(
        soil_before - world.ecology_soil_water_hu_at(pos.surface()).unwrap(),
        crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
    );

    let planted = pos.offset(1, 0, 0).unwrap();
    world.set_block_at(planted, AIR);
    assert!(world.place_item_block_at(planted, drop));
    let returned = world.live_water_audit().unwrap();
    assert_eq!(returned.current_water_hu, before.current_water_hu);
    assert_eq!(
        returned.unexplained_water_delta_hu,
        before.unexplained_water_delta_hu
    );
    assert_eq!(returned.industrial.water_hu, before.industrial.water_hu);
    assert_eq!(
        world.ecology_soil_water_hu_at(pos.surface()).unwrap(),
        soil_before,
        "planting the dew closes its local real-water loop"
    );
}

#[test]
fn resonant_mineral_break_reconciles_physical_and_arcane_ledgers_together() {
    let mut world = ecology_world("resonant-mineral-dual-ledger", 10_104);
    let surface = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .ecology
        .sites
        .first()
        .and_then(|site| site.surface())
        .unwrap();
    world.ensure_chunk(ChunkPos::from_surface(surface));
    let pos = crate::planet::BlockPos::new(surface.face(), surface.u(), 220, surface.v()).unwrap();
    let mineral = b(&world.reg, "base:choirstone");
    world.set_block_authored_at(pos, mineral, "dual-ledger fixture");
    let before = world.material_ledger.as_ref().unwrap().audit();
    assert_eq!(before.placed.get("choirstone"), Some(&1_200));
    let pick = it(&world.reg, "base:iron_pickaxe");
    let drop = world
        .break_block_at(pos, Some(pick), true, false)
        .expect("resonant mineral extraction")
        .drop
        .unwrap();
    assert_ne!(drop.arcane_id, 0);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_current_total(drop.arcane_id)
            .is_some_and(|units| units > 0)
    );
    let materials = world.material_ledger.as_ref().unwrap().audit();
    assert_eq!(materials.placed.get("choirstone").copied().unwrap_or(0), 0);
    assert_eq!(materials.circulating.get("choirstone"), Some(&1_200));
    assert!(materials.is_balanced());
    let geography = world.arcane_geography.as_ref().unwrap();
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Geography)
            .unwrap()
            .current,
        geography.custody_current().unwrap()
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
fn plausible_grazer_bite_collapses_magical_forage_without_rematerializing_charge() {
    let mut world = ecology_world("magical-forage-bite", 10_106);
    let pos = materialize_ecology_site(&mut world, "base:rainbell");
    let total_before = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .accounted_total;
    world.apply_bite_at(pos);
    assert_eq!(world.get_block_at(pos), AIR);
    let geography = world.arcane_geography.as_ref().unwrap();
    let site = geography
        .dynamic
        .ecology
        .sites
        .iter()
        .find(|site| site.block_pos() == Some(pos))
        .unwrap();
    assert!(matches!(
        site.stage,
        crate::arcane_ecology::EcologyStage::Collapsed
            | crate::arcane_ecology::EcologyStage::Harvested
    ));
    assert_eq!(geography.audit().unwrap().accounted_total, total_before);
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Geography)
            .unwrap()
            .current,
        geography.custody_current().unwrap()
    );
    let dir = world.save_dir_for_test().to_path_buf();
    let reg = world.reg.clone();
    save_world(&mut world);
    drop(world);
    let mut loaded = World::load_or_create(dir, reg).unwrap();
    loaded.ensure_chunk(pos.chunk());
    assert_eq!(loaded.get_block_at(pos), AIR);
}

#[test]
fn explosive_wellglass_loss_destroys_the_bud_and_cannot_rematerialize_charge() {
    let mut world = ecology_world("wellglass-explosion-persistence", 10_107);
    let pos = materialize_ecology_site(&mut world, "base:wellglass_bud");
    let site_id = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .ecology
        .sites
        .iter()
        .find(|site| site.block_pos() == Some(pos))
        .unwrap()
        .id;
    let total_before = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .accounted_total;
    world
        .break_block_at(pos, None, false, false)
        .expect("explosion-style loss settles the persistent site");
    assert_ne!(world.get_block_at(pos), b(&world.reg, "base:wellglass_bud"));
    let geography = world.arcane_geography.as_ref().unwrap();
    let site = geography
        .dynamic
        .ecology
        .sites
        .iter()
        .find(|site| site.id == site_id)
        .unwrap();
    assert_eq!(site.stage, crate::arcane_ecology::EcologyStage::Harvested);
    assert_eq!(site.seed_bank, 0);
    assert_eq!(site.charge_total(), 0);
    assert_eq!(geography.audit().unwrap().accounted_total, total_before);
    let dir = world.save_dir_for_test().to_path_buf();
    let reg = world.reg.clone();
    save_world(&mut world);
    drop(world);
    let mut loaded = World::load_or_create(dir, reg).unwrap();
    loaded.ensure_chunk(pos.chunk());
    assert_ne!(
        loaded.get_block_at(pos),
        b(&loaded.reg, "base:wellglass_bud")
    );
    let geography = loaded.arcane_geography.as_ref().unwrap();
    let site = geography
        .dynamic
        .ecology
        .sites
        .iter()
        .find(|site| site.id == site_id)
        .unwrap();
    assert_eq!(site.stage, crate::arcane_ecology::EcologyStage::Harvested);
    assert_eq!(site.charge_total(), 0);
    assert_eq!(geography.audit().unwrap().accounted_total, total_before);
}

#[test]
fn arcane_ire_and_dross_four_states_persist_independently() {
    let reg = base_reg();
    let mut readings = std::collections::BTreeSet::new();
    for (angry, polluted) in [(false, false), (false, true), (true, false), (true, true)] {
        let dir = tmp_dir(&format!("arcane-separation-{angry}-{polluted}"));
        let atlas = std::sync::Arc::new(
            crate::planet_atlas::PlanetAtlas::fixture(8_811 + angry as u32, 8).unwrap(),
        );
        atlas.write_new(&dir).unwrap();
        let site = atlas.biomes.countries[0].heart_site.center(atlas.side());
        let surface =
            crate::planet::SurfacePos::new(site.face, site.u.floor() as u16, site.v.floor() as u16)
                .unwrap();
        let at = crate::planet::BlockPos::new(surface.face(), surface.u(), 1, surface.v()).unwrap();
        let region = atlas.atlas_pos(surface);
        let mut world =
            World::new_with_atlas(8_811 + angry as u32, dir.clone(), reg.clone(), atlas);
        world.ire = if angry { 80.0 } else { 0.0 };
        if polluted {
            let mut ember = ItemStack::new(&reg, it(&reg, "base:ember"), 1);
            world
                .bind_arcane_stack_at(at, &mut ember, "separation fixture")
                .unwrap();
            world.retire_arcane_stack_at(at, ember, "separation pollution");
        }
        save_world(&mut world);
        let loaded = World::load_or_create(dir, reg.clone()).unwrap();
        let bands = loaded.arcane_ledger.as_ref().unwrap().local_bands(region);
        assert_eq!(loaded.ire, if angry { 80.0 } else { 0.0 });
        assert_eq!(bands[1] > 0, polluted);
        assert!(
            loaded
                .arcane_ledger
                .as_ref()
                .unwrap()
                .audit()
                .unwrap()
                .is_balanced()
        );
        readings.insert((loaded.ire as u8, bands[1]));
    }
    assert_eq!(readings.len(), 4);
}

#[test]
fn dross_changes_ire_only_when_it_causes_real_ecology_population_loss() {
    let mut world = ecology_world("dross-habitat-harm-ire", 10_108);
    let (site_index, tolerance) = {
        let geography = world.arcane_geography.as_ref().unwrap();
        geography
            .dynamic
            .ecology
            .sites
            .iter()
            .enumerate()
            .find_map(|(index, site)| {
                let definition = world.reg.arcane_ecology.get(&site.content_id)?;
                (site.charge_total() > u64::from(definition.dross_tolerance))
                    .then_some((index, definition.dross_tolerance))
            })
            .expect("fixture needs one charged ecology site")
    };
    let needed = u64::from(tolerance).saturating_add(1);
    let (population_before, completed_before) = {
        let geography = world.arcane_geography.as_mut().unwrap();
        let site = &mut geography.dynamic.ecology.sites[site_index];
        site.population = 1;
        site.stage = crate::arcane_ecology::EcologyStage::Mature;
        let mut remaining = needed;
        for slot in 0..6 {
            let moved = u64::from(site.charge[slot]).min(remaining);
            site.charge[slot] -= moved as u32;
            site.dross[slot] = site.dross[slot].checked_add(moved as u32).unwrap();
            remaining -= moved;
        }
        assert_eq!(remaining, 0);
        (site.population, geography.dynamic.ecology.completed_days)
    };
    let ire_before = world.ire;
    assert_eq!(
        world.ire, ire_before,
        "moving Current into Dross changed Ire"
    );
    world.day = u32::try_from(completed_before.saturating_add(1)).unwrap();

    world.tick_arcane_ecology(usize::MAX).unwrap();

    let geography = world.arcane_geography.as_ref().unwrap();
    let site = &geography.dynamic.ecology.sites[site_index];
    assert!(site.population < population_before);
    assert!(
        world.ire > ire_before,
        "accounted Dross killed habitat population without invoking Ire"
    );
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
fn magical_harvests_and_bonus_drops_are_funded_before_delivery() {
    use crate::arcane::ArcaneOwner;

    let reg = base_reg();
    let mut world = World::load_or_create(
        tmp_dir("funded-magical-harvests").join("world"),
        reg.clone(),
    )
    .unwrap();
    world.ensure_chunk(tchunk(0, 0));
    let at = crate::planet::BlockPos::of_world(3, 150, 3).unwrap();
    world.set_block_at(at, b(&reg, "base:lantern_fungus"));
    let drop = world
        .break_block_at(at, None, true, false)
        .unwrap()
        .drop
        .unwrap();
    assert_eq!(drop.item, it(&reg, "base:lantern_fungus"));
    assert_ne!(drop.arcane_id, 0);
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::Item(drop.arcane_id))
            .unwrap()
            .current
            .total(),
        192
    );
    let placed_at = crate::planet::BlockPos::of_world(4, 200, 3).unwrap();
    let ambient_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .reservoirs[&crate::arcane::Reservoir::Ambient];
    assert!(world.place_item_block_at(placed_at, drop));
    assert_eq!(
        world.get_block_at(placed_at),
        b(&reg, "base:lantern_fungus")
    );
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::Item(drop.arcane_id))
            .is_none()
    );
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .reservoirs[&crate::arcane::Reservoir::Ambient],
        ambient_before + 192
    );

    let gold_quartz = b(&reg, "base:gold_quartz");
    assert!(
        reg.block(gold_quartz).arcane.is_none(),
        "this fixture proves the output item, not the source block, controls binding"
    );
    let mut rng = 0;
    let bonus = world
        .roll_bonus_drop_at(at, gold_quartz, &mut rng)
        .expect("seed zero rolls below the declared half chance");
    assert_eq!(bonus.item, it(&reg, "base:quartz_shard"));
    assert_ne!(bonus.arcane_id, 0);
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::Item(bonus.arcane_id))
            .unwrap()
            .current
            .total(),
        384
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

/// A deterministic offering-fixture planet: worlds created through a bare
/// `load_or_create` seed from the wall clock, which made these tests roll a
/// different planet every CI run.
fn charged_offering_world(name: &str, seed: u32) -> (Arc<Registry>, World) {
    let reg = base_reg();
    let root = tmp_dir(name).join("world");
    crate::world::create_world_fixture_atomic(
        &root,
        seed,
        "survival",
        8,
        &crate::planet_atlas::CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    let world = World::load_or_create(root, reg.clone()).unwrap();
    (reg, world)
}

/// The first country heart whose reserve satisfies `keep`, as the surface of
/// its heart site, its country id, and its current total.
fn country_heart_holding(
    world: &World,
    keep: impl Fn(u64) -> bool,
) -> Option<(crate::planet::SurfacePos, u16, u64)> {
    let atlas = world.planet_atlas().unwrap();
    atlas.biomes.countries.iter().find_map(|candidate| {
        let site = candidate.heart_site.center(atlas.side());
        let surface =
            crate::planet::SurfacePos::new(site.face, site.u.floor() as u16, site.v.floor() as u16)
                .ok()?;
        let country = atlas.country_at(surface)?.id;
        let total = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Heart(country))?
            .current
            .total();
        keep(total).then_some((surface, country, total))
    })
}

#[test]
fn charged_offering_returns_exact_current_to_its_country_heart() {
    use crate::arcane::ArcaneOwner;
    use crate::world::{BlockEntity, OfferingState};

    let (reg, mut world) = charged_offering_world("charged-country-offering", 42);
    // A heart holding at least two bindings stays open while the gift is out
    // on loan, so the mid-loan balance below is observable. The drained case
    // is covered by offering_revives_a_country_heart_drained_by_its_own_gift.
    let (surface, country, heart_before) = country_heart_holding(&world, |total| total >= 512)
        .expect("fixture seed 42 offers a country heart holding at least two bindings");
    let at = crate::planet::BlockPos::new(surface.face(), surface.u(), 100, surface.v()).unwrap();
    let heart = ArcaneOwner::Heart(country);
    let mut gift = ItemStack::new(&reg, it(&reg, "base:thorn_fiber"), 1);
    world
        .bind_arcane_stack_at(at, &mut gift, "charged offering fixture")
        .unwrap();
    let gift_owner = ArcaneOwner::Item(gift.arcane_id);
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&heart)
            .unwrap()
            .current
            .total(),
        heart_before - 256
    );
    let mut offering = OfferingState::default();
    offering.slots[0] = Some(gift);
    world.insert_block_entity_at(at, BlockEntity::Offering(offering));
    assert!(world.accept_offerings() > 0.0);
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert!(ledger.account(&gift_owner).is_none());
    assert_eq!(
        ledger.account(&heart).unwrap().current.total(),
        heart_before
    );
    assert!(ledger.audit().unwrap().is_balanced());
}

#[test]
fn offering_revives_a_country_heart_drained_by_its_own_gift() {
    use crate::arcane::ArcaneOwner;
    use crate::world::{BlockEntity, OfferingState};

    // Seed 24's first country is a single atlas cell, so its heart holds
    // exactly one binding (256 units): lending the gift drains it to zero
    // and the ledger prunes the empty account. Accepting the offering must
    // still return the exact current and revive the heart.
    let (reg, mut world) = charged_offering_world("charged-offering-drained-heart", 24);
    let (surface, country, heart_before) = country_heart_holding(&world, |total| total == 256)
        .expect("fixture seed 24 offers a one-cell country heart");
    let at = crate::planet::BlockPos::new(surface.face(), surface.u(), 100, surface.v()).unwrap();
    let heart = ArcaneOwner::Heart(country);
    let mut gift = ItemStack::new(&reg, it(&reg, "base:thorn_fiber"), 1);
    world
        .bind_arcane_stack_at(at, &mut gift, "drained offering fixture")
        .unwrap();
    let gift_owner = ArcaneOwner::Item(gift.arcane_id);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&heart)
            .is_none(),
        "a heart drained to zero is pruned while its gift is out on loan"
    );
    let mut offering = OfferingState::default();
    offering.slots[0] = Some(gift);
    world.insert_block_entity_at(at, BlockEntity::Offering(offering));
    assert!(world.accept_offerings() > 0.0);
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert!(ledger.account(&gift_owner).is_none());
    assert_eq!(
        ledger.account(&heart).unwrap().current.total(),
        heart_before
    );
    assert!(ledger.audit().unwrap().is_balanced());
}

#[test]
#[ignore = "operator probe for WILDFORGE_PROBE_WORLD production save"]
fn production_idle_tick_performance_probe() {
    let root = std::env::var("WILDFORGE_PROBE_WORLD")
        .map(std::path::PathBuf::from)
        .expect("set WILDFORGE_PROBE_WORLD to a qualified production save");
    let mut world = World::load_or_create(root, base_reg()).unwrap();
    world.prepare_common_spawn(|_, _, _| {}).unwrap();
    let resident = world.chunk_count();
    let mut server = crate::server::Server::new(world, 0.3, 0x1d1e);
    let mut events = Vec::new();
    let start = std::time::Instant::now();
    for _ in 0..1_200 {
        server.advance(1.0 / 60.0, &[], &mut events);
        std::hint::black_box(&events);
        events.clear();
    }
    let each = start.elapsed().as_nanos() / 1_200;
    println!("idle profile: resident_chunks={resident} tick={each} ns");
}

#[test]
fn block_edit_fans_out_through_one_authoritative_boundary() {
    use crate::world::{BlockEntity, ChestState};

    let reg = base_reg();
    let mut w = test_world_with("edit-side-effects", reg.clone());
    let here = tchunk(0, 0);
    let west = tchunk(-1, 0);
    for pos in [here, west] {
        let chunk = w.chunks_mut().get_mut(&pos).unwrap();
        chunk.dirty = false;
        chunk.modified = false;
    }

    let stone = b(&reg, "base:stone");
    w.set_edit_logging(true);
    w.set_block(0, 200, 4, stone);
    assert_eq!(w.get_block(0, 200, 4), stone);
    assert!(w.chunks()[&here].dirty && w.chunks()[&west].dirty);
    assert_eq!(
        w.edits().last(),
        Some(&(
            crate::planet::BlockPos::of_world(0, 200, 4).unwrap(),
            stone,
            0,
            0,
            0
        ))
    );

    let chest = b(&reg, "base:chest");
    let stick = it(&reg, "base:stick");
    let mut state = ChestState::default();
    state.slots[0] = Some(ItemStack::new(&reg, stick, 3));
    w.set_block(2, 200, 4, chest);
    w.insert_block_entity((2, 200, 4), BlockEntity::Chest(state));
    w.set_block(2, 200, 4, AIR);
    assert!(!w.has_block_entity(&(2, 200, 4)));
    assert!(w.pending_drops().iter().any(|(pos, stack)| {
        *pos == crate::planet::BlockPos::of_world(2, 200, 4).unwrap()
            && stack.item == stick
            && stack.count == 3
    }));

    let sand = b(&reg, "base:sand");
    w.set_block(4, 202, 4, sand);
    assert_eq!(w.get_block(4, 202, 4), AIR);
    assert!(
        w.falling_blocks()
            .iter()
            .any(|falling| falling.block == sand)
    );
}

#[test]
fn removing_a_torch_leaves_no_residual_block_light() {
    let reg = base_reg();
    let mut w = test_world_with("torch_residual", reg.clone());
    let torch = b(&reg, "base:torch");
    // A chunk corner: the torch (light 14) glows into all four chunks that
    // meet here, so removing it must drain every one of them.
    let (tx, ty, tz) = (15, 100, 15);

    // Baseline block light over the region the torch can possibly touch.
    let region = |f: &mut dyn FnMut(i32, i32, i32)| {
        for x in (tx - 16)..=(tx + 16) {
            for y in (ty - 16)..=(ty + 16) {
                for z in (tz - 16)..=(tz + 16) {
                    f(x, y, z);
                }
            }
        }
    };
    let mut before: HashMap<(i32, i32, i32), [u8; 3]> = HashMap::new();
    region(&mut |x, y, z| {
        before.insert((x, y, z), w.light_rgb_at(x, y, z).0);
    });

    w.set_block(tx, ty, tz, torch);
    assert!(
        w.light_rgb_at(tx, ty, tz).0[0] > 0,
        "torch lights its own cell"
    );
    assert!(
        w.light_rgb_at(tx + 3, ty, tz + 3).0[0] > 0,
        "glow crosses the seam into the diagonal chunk"
    );

    w.set_block(tx, ty, tz, AIR);

    // Every cell must return to exactly its pre-torch block light.
    region(&mut |x, y, z| {
        let now = w.light_rgb_at(x, y, z).0;
        assert_eq!(
            now,
            before[&(x, y, z)],
            "residual block light at {x},{y},{z}: {now:?}"
        );
    });
}

#[test]
fn remote_world_neither_generates_nor_saves_authoritative_state() {
    let reg = base_reg();
    let dir = tmp_dir("remote-authority");
    let mut w = World::new(7, dir.clone(), reg);
    w.set_remote(true);

    assert!(!w.ensure_chunk(tchunk(0, 0)));
    assert!(w.chunks().is_empty());
    save_world(&mut w);

    assert!(!dir.join("world.toml").exists());
    assert!(!dir.join("chunks").exists());
}

#[test]
fn replicated_block_burst_preserves_state_and_settles_shared_lighting() {
    let reg = base_reg();
    let mut world = test_world_with("remote-block-batch", reg.clone());
    let torch = b(&reg, "base:torch");
    let water = b(&reg, "base:water");
    let torch_pos = bp(8, 90, 8);
    let water_pos = bp(9, 90, 8);
    world.set_remote(true);
    world.set_remote_arcane_cue([u8::MAX; 2], u8::MAX, None);
    assert_eq!(world.remote_arcane_cue(), [4; 2]);
    assert_eq!(world.remote_arcane_dominant(), 6);

    world.set_remote_arcane_cue(
        [2, 1],
        4,
        Some(("Rainbells fold shut beside the marsh.".into(), true)),
    );
    let ecology = world
        .perceived_arcane_ecology_at(torch_pos.surface(), 72.0)
        .expect("remote ecology observation");
    assert_eq!(ecology.text, "Rainbells fold shut beside the marsh.");
    assert!(ecology.damped);
    world.set_remote_arcane_items(vec![(41, 73)]);
    assert_eq!(world.inspectable_item_current(41), Some(73));

    world.apply_remote_block_states([(torch_pos, torch, 0, 0, 0), (water_pos, water, 3, 41, 0)]);

    assert_eq!(world.get_block_at(torch_pos), torch);
    assert_eq!(world.get_block_at(water_pos), water);
    assert_eq!(world.get_meta_at(water_pos), 3);
    assert_eq!(world.get_water_salt_at(water_pos), 41);
    assert!(
        world.light_rgb_at_pos(torch_pos).0[0] > 0,
        "the shared batch must finish its derived lighting before returning"
    );

    world.apply_remote_block_states([(torch_pos, AIR, 0, 0, 0)]);
    assert_eq!(world.light_rgb_at_pos(torch_pos).0, [0; 3]);
}

#[test]
fn save_v2_roundtrip_with_palette() {
    let reg = base_reg();
    let mut w = test_world_with("v2save", reg.clone());
    let log = b(&reg, "base:log");
    w.set_block(1, 80, 1, log);
    w.set_block(-20, 33, 7, b(&reg, "base:sand"));
    save_world(&mut w);
    assert!(w.save_dir_for_test().join("palette").exists());

    let mut w2 = World::load_or_create(w.save_dir_for_test(), reg.clone()).unwrap();
    for x in -2..=2 {
        for z in -2..=2 {
            w2.ensure_chunk(tchunk(x, z));
        }
    }
    assert_eq!(w2.get_block(1, 80, 1), log);
    assert_eq!(w2.get_block(-20, 33, 7), b(&reg, "base:sand"));
    assert_eq!(w2.get_block(0, 0, 0), b(&reg, "base:bedrock"));
}

#[test]
fn pre_v3_saves_regenerate_cleanly() {
    let reg = base_reg();
    let dir = tmp_dir("oldsave");
    std::fs::write(dir.join("seed"), "42").unwrap();
    // A stale v2 chunk file must be ignored (regenerated), not crash.
    std::fs::write(dir.join("c.0.0.wfc"), b"WFC2garbagegarbage").unwrap();
    let error = World::load_or_create(dir.clone(), reg).err().unwrap();
    assert!(error.to_string().contains("legacy flat"));
    assert!(
        dir.join("c.0.0.wfc").exists(),
        "refusal leaves old data alone"
    );
    assert!(!dir.join("world.toml").exists());
}

#[test]
fn palette_less_v3_chunks_keep_their_legacy_numeric_ids() {
    let reg = base_reg();
    let dir = tmp_dir("palette-less-v3");
    std::fs::write(dir.join("seed"), "42").unwrap();
    let stone = b(&reg, "base:stone");
    let mut data = Vec::new();
    data.extend_from_slice(b"WFC3");
    let total = 16 * 16 * 256usize;
    let mut left = total;
    while left > 0 {
        let run = left.min(u16::MAX as usize) as u16;
        data.extend_from_slice(&run.to_le_bytes());
        data.extend_from_slice(&stone.0.to_le_bytes());
        left -= run as usize;
    }
    std::fs::write(dir.join("c.0.0.wfc"), data).unwrap();

    let error = World::load_or_create(dir, reg).err().unwrap();
    assert!(error.to_string().contains("legacy flat"));
}

#[test]
fn unknown_palette_entries_become_placeholder() {
    let reg = base_reg();
    let dir = tmp_dir("unknown");
    std::fs::write(dir.join("seed"), "42").unwrap();
    // Palette maps id 1 to a mod block that no longer exists.
    std::fs::write(dir.join("palette"), "0 base:air\n1 gonemod:ore\n").unwrap();
    let mut data = Vec::new();
    data.extend_from_slice(b"WFC3");
    let total = 16 * 16 * 256usize;
    for (count, id) in [(60u16, 0u16), (1, 1), ((total - 61) as u16, 0)] {
        data.extend_from_slice(&count.to_le_bytes());
        data.extend_from_slice(&id.to_le_bytes());
    }
    let _ = std::fs::write(dir.join("c.0.0.wfc"), data);
    let error = World::load_or_create(dir, reg).err().unwrap();
    assert!(error.to_string().contains("legacy flat"));
}

#[test]
fn all_placeholder_chunks_regenerate_instead_of_becoming_obelisks() {
    let reg = base_reg();
    let dir = tmp_dir("placeholder-obelisk");
    std::fs::write(dir.join("seed"), "42").unwrap();
    std::fs::write(
        dir.join("palette"),
        format!("{} base:unknown\n", reg.unknown_block.0),
    )
    .unwrap();
    let mut data = Vec::new();
    data.extend_from_slice(b"WFC3");
    let total = 16 * 16 * 256usize;
    let mut left = total;
    while left > 0 {
        let run = left.min(u16::MAX as usize) as u16;
        data.extend_from_slice(&run.to_le_bytes());
        data.extend_from_slice(&reg.unknown_block.0.to_le_bytes());
        left -= run as usize;
    }
    std::fs::write(dir.join("c.0.0.wfc"), data).unwrap();

    let error = World::load_or_create(dir, reg).err().unwrap();
    assert!(error.to_string().contains("legacy flat"));
}

#[test]
fn set_block_roundtrip_and_cross_chunk_access() {
    let reg = base_reg();
    let mut w = test_world_with("set", reg.clone());
    let planks = b(&reg, "base:planks");
    w.set_block(3, 70, 3, planks);
    assert_eq!(w.get_block(3, 70, 3), planks);
    w.set_block(-1, 70, -1, b(&reg, "base:cobblestone"));
    assert_eq!(w.get_block(-1, 70, -1), b(&reg, "base:cobblestone"));
}

#[test]
fn world_remaps_after_registry_change() {
    let root = tmp_dir("reloadmod");
    write_demo_mod(&root);
    let reg_with = Arc::new(registry::load(&root));
    let ore = reg_with.block_id("testium:ore").unwrap();
    let mut w = test_world_with("reload-w", reg_with.clone());
    w.set_block(2, 70, 2, ore);
    // "Remove" the mod: rebuild registry from an empty dir, remap the world.
    let reg_without = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    w.reg = reg_without.clone();
    w.remap_from(&reg_with);
    assert_eq!(
        w.get_block(2, 70, 2),
        reg_without.unknown_block,
        "mod block becomes placeholder after mod removal"
    );
    // Vanilla blocks survive the remap unchanged (by name).
    assert_eq!(w.get_block(0, 0, 0), b(&reg_without, "base:bedrock"));
}

#[test]
fn crops_grow_on_farmland_via_random_ticks() {
    let reg = base_reg();
    let mut w = test_world_with("crops", reg.clone());
    let h = w.surface_height(4, 4);
    let farm = b(&reg, "base:farmland");
    let seed0 = b(&reg, "base:wheat_seeds");
    // A field on farmland grows; a control column on dirt doesn't.
    for x in 0..16 {
        for z in 0..16 {
            if (x, z) == (6, 6) {
                continue;
            }
            w.set_block(x, h, z, farm);
            w.set_block(x, h + 1, z, seed0);
        }
    }
    w.set_block(6, h, 6, b(&reg, "base:dirt"));
    w.set_block(6, h + 1, 6, seed0);
    let mut rng = 12345u32;
    for _ in 0..3000 {
        w.random_tick(&mut rng);
    }
    let mut advanced = 0;
    for x in 0..16 {
        for z in 0..16 {
            if (x, z) != (6, 6) && w.get_block(x, h + 1, z) != seed0 {
                advanced += 1;
            }
        }
    }
    let sample = w.weather_at_surface(bp(4, h + 1, 4).surface());
    assert!(
        advanced > 0,
        "farmland crops should advance (local temperature {:.2} C, season {})",
        sample.temperature_c,
        w.season_at_surface(bp(4, h + 1, 4).surface())
    );
    assert_eq!(w.get_block(6, h + 1, 6), seed0, "dirt crop must not grow");
    // Stage chain terminates at ripe (stage2) with a harvest def.
    let ripe = b(&reg, "base:wheat_seeds/stage2");
    assert!(reg.block(ripe).crop_next.is_none());
    let (item, _, becomes) = reg.block(ripe).harvest.expect("ripe wheat harvests");
    assert_eq!(item, it(&reg, "base:wheat"));
    assert_eq!(becomes, seed0);
    // Bushes regrow anywhere - but only in season (summer/autumn).
    w.day = crate::world::SEASON_DAYS; // summer
    let bare = b(&reg, "base:berry_bush");
    for x in 0..16 {
        for z in 8..11 {
            w.set_block(x, h + 3, z, bare);
        }
    }
    for _ in 0..30000 {
        w.random_tick(&mut rng);
    }
    let fruited = b(&reg, "base:berry_bush/stage1");
    let refruited = (0..16)
        .flat_map(|x| (8..11).map(move |z| (x, z)))
        .filter(|&(x, z)| w.get_block(x, h + 3, z) == fruited)
        .count();
    assert!(refruited > 0, "bushes should refruit anywhere");
    // Cross rendering flags.
    assert!(reg.block(seed0).cross);
    assert!(!reg.is_solid(seed0));
}

#[test]
fn world_meta_roundtrip_and_legacy_refusal() {
    use crate::world::{
        WORLD_GENERATOR_VERSION, WORLD_TOPOLOGY, load_world_meta, read_world_meta, write_world_meta,
    };
    let dir = tmp_dir("meta");
    write_world_meta(&dir, 777, "creative", 0.0).unwrap();
    assert_eq!(
        read_world_meta(&dir),
        (Some(777), "creative".to_string(), 0.0)
    );
    let text = std::fs::read_to_string(dir.join("world.toml")).unwrap();
    assert!(text.contains(&format!("topology = \"{WORLD_TOPOLOGY}\"")));
    assert!(text.contains("face_blocks = 8192"));
    assert!(text.contains("world_height = 256"));
    assert!(text.contains("planet_radius = 5215."));
    assert!(text.contains(&format!("generator_version = {WORLD_GENERATOR_VERSION}")));
    // Camera mode round-trips through the same header.
    let mut meta = load_world_meta(&dir).unwrap().unwrap();
    assert_eq!(meta.camera, "first", "default camera is first-person");
    meta.camera = "orbit".into();
    crate::world::write_world_meta_full(&dir, meta.seed, &meta.mode, meta.ire, meta.day, &meta.camera)
        .unwrap();
    assert_eq!(
        load_world_meta(&dir).unwrap().unwrap().camera,
        "orbit",
        "chosen camera mode is persisted per world"
    );
    let previous = text.replace(
        &format!("generator_version = {WORLD_GENERATOR_VERSION}"),
        "generator_version = 9",
    );
    std::fs::write(dir.join("world.toml"), previous).unwrap();
    assert_eq!(
        load_world_meta(&dir).unwrap().unwrap().seed,
        777,
        "the biome correction must not strand version-9 planets"
    );

    // A flat save is rejected without modifying it.
    let dir2 = tmp_dir("meta2");
    std::fs::write(dir2.join("seed"), "42").unwrap();
    let before = std::fs::read(dir2.join("seed")).unwrap();
    let error = load_world_meta(&dir2).unwrap_err();
    assert!(error.to_string().contains("legacy flat"));
    let reg = base_reg();
    assert!(World::load_or_create(dir2.clone(), reg).is_err());
    assert!(!dir2.join("world.toml").exists());
    assert_eq!(std::fs::read(dir2.join("seed")).unwrap(), before);

    // An old world.toml is likewise not inferred to be planetary.
    let dir3 = tmp_dir("meta3");
    std::fs::write(dir3.join("world.toml"), "seed = 9\nmode = \"survival\"\n").unwrap();
    assert!(
        load_world_meta(&dir3)
            .unwrap_err()
            .to_string()
            .contains("missing topology")
    );
}

#[test]
fn water_conserves_and_spreads_finite() {
    let reg = base_reg();
    assert_eq!(reg.water_volume(reg.water_block(0)), Some(8));
    assert_eq!(reg.water_volume(reg.water_block(7)), Some(1));
    assert_eq!(reg.water_for_volume(8), reg.water_block(0));
    assert_eq!(reg.water_for_volume(0), AIR);

    let mut w = test_world_with("finitewater", reg.clone());
    let h = w.surface_height(4, 4);
    let y = h + 5;
    let stone = b(&reg, "base:stone");
    for x in -8..=16 {
        for z in -8..=16 {
            w.set_block(x, y - 1, z, stone);
        }
    }
    let before = total_water(&w);
    w.set_block(4, y, 4, reg.water_block(0));
    settle_water(&mut w);
    assert_eq!(total_water(&w), before + 8, "volume neither made nor lost");
    // One cell can't stay full on open ground: it spread into a film.
    assert!(reg.water_volume(w.get_block(4, y, 4)).unwrap_or(0) < 8);
}

#[test]
fn flowing_water_moves_exact_salt_mass_with_deterministic_remainders() {
    let reg = base_reg();
    let mut world = test_world_with("saltwater-flow", reg.clone());
    let y = world.surface_height(4, 4) + 5;
    let stone = b(&reg, "base:stone");
    for x in -4..=12 {
        for z in -4..=12 {
            world.set_block(x, y - 1, z, stone);
        }
    }
    let source = crate::planet::BlockPos::of_world(4, y, 4).unwrap();
    world.set_block_water_at(source, reg.water_block(0), 156, 40_000);
    settle_water(&mut world);
    let mut water_hu = 0u64;
    let mut salt_mass = 0u64;
    for x in -4..=12 {
        for z in -4..=12 {
            let at = crate::planet::BlockPos::of_world(x, y, z).unwrap();
            if let Some(mass) = world.water_mass_at(at) {
                water_hu += mass.water_hu;
                salt_mass += mass.salt_mass;
            }
        }
    }
    assert_eq!(water_hu, 256);
    assert_eq!(salt_mass, 40_000);
}

#[test]
fn finite_water_crosses_a_real_planet_face_seam() {
    use crate::planet::{BlockPos, Direction6, step6};

    let reg = base_reg();
    let mut world = World::new(42, tmp_dir("planet-water-all-seams"), reg.clone());
    let stone = b(&reg, "base:stone");

    for seam in directed_planet_seams() {
        let source =
            BlockPos::new(seam.source.face(), seam.source.u(), 100, seam.source.v()).unwrap();
        let across =
            BlockPos::new(seam.across.face(), seam.across.u(), 100, seam.across.v()).unwrap();
        let missing: Vec<_> = [source.chunk(), across.chunk()]
            .into_iter()
            .filter(|chunk| !world.has_chunk(*chunk))
            .collect();
        world.insert_empty_chunks_for_test(missing);

        for cell in [source, across] {
            world.set_block_at(step6(cell, Direction6::Down).unwrap().pos, stone);
            for direction in [
                Direction6::East,
                Direction6::North,
                Direction6::West,
                Direction6::South,
            ] {
                let neighbor = step6(cell, direction).unwrap().pos;
                if neighbor != source && neighbor != across {
                    world.set_block_at(neighbor, stone);
                }
            }
        }

        world.set_block_at(source, reg.water_for_volume(8));
        for _ in 0..32 {
            if !world.tick_water(64) {
                break;
            }
        }
        let source_volume = reg.water_volume(world.get_block_at(source)).unwrap_or(0);
        let across_volume = reg.water_volume(world.get_block_at(across)).unwrap_or(0);
        assert_eq!(
            source_volume + across_volume,
            8,
            "volume is conserved at {:?} {:?}",
            seam.face,
            seam.direction
        );
        assert_eq!(
            (source_volume, across_volume),
            (4, 4),
            "fluid did not treat {:?} {:?} as an ordinary neighbor",
            seam.face,
            seam.direction
        );
    }
}

#[test]
fn finite_lava_crosses_every_directed_planet_seam() {
    use crate::planet::{BlockPos, Direction6, step6};

    let reg = base_reg();
    let mut world = World::new(53, tmp_dir("planet-lava-all-seams"), reg.clone());
    let stone = b(&reg, "base:stone");

    for seam in directed_planet_seams() {
        let source =
            BlockPos::new(seam.source.face(), seam.source.u(), 104, seam.source.v()).unwrap();
        let across =
            BlockPos::new(seam.across.face(), seam.across.u(), 104, seam.across.v()).unwrap();
        let missing: Vec<_> = [source.chunk(), across.chunk()]
            .into_iter()
            .filter(|chunk| !world.has_chunk(*chunk))
            .collect();
        world.insert_empty_chunks_for_test(missing);

        for cell in [source, across] {
            world.set_block_at(step6(cell, Direction6::Down).unwrap().pos, stone);
            for direction in [
                Direction6::East,
                Direction6::North,
                Direction6::West,
                Direction6::South,
            ] {
                let neighbor = step6(cell, direction).unwrap().pos;
                if neighbor != source && neighbor != across {
                    world.set_block_at(neighbor, stone);
                }
            }
        }

        world.set_block_at(source, reg.lava_for_volume(8));
        for _ in 0..32 {
            if !world.tick_lava(64) {
                break;
            }
        }
        let source_volume = reg.lava_volume(world.get_block_at(source)).unwrap_or(0);
        let across_volume = reg.lava_volume(world.get_block_at(across)).unwrap_or(0);
        assert_eq!(
            source_volume + across_volume,
            8,
            "lava volume changed at {:?} {:?}",
            seam.face,
            seam.direction
        );
        assert_eq!(
            (source_volume, across_volume),
            (4, 4),
            "lava did not cross {:?} {:?}",
            seam.face,
            seam.direction
        );
    }
}

#[test]
fn block_light_crosses_and_clears_at_a_real_planet_face_seam() {
    use crate::planet::BlockPos;

    let reg = base_reg();
    let mut world = World::new(43, tmp_dir("planet-light-all-seams"), reg.clone());
    let torch = b(&reg, "base:torch");
    let emission = reg.block(torch).light_rgb;

    for seam in directed_planet_seams() {
        let source =
            BlockPos::new(seam.source.face(), seam.source.u(), 100, seam.source.v()).unwrap();
        let across =
            BlockPos::new(seam.across.face(), seam.across.u(), 100, seam.across.v()).unwrap();
        let missing: Vec<_> = [source.chunk(), across.chunk()]
            .into_iter()
            .filter(|chunk| !world.has_chunk(*chunk))
            .collect();
        world.insert_empty_chunks_for_test(missing);

        world.set_block_at(source, torch);
        assert_eq!(world.light_rgb_at_pos(source).0, emission);
        assert_eq!(
            world.light_rgb_at_pos(across).0,
            emission.map(|channel| channel.saturating_sub(1)),
            "light did not cross {:?} {:?}",
            seam.face,
            seam.direction
        );

        world.set_block_at(source, AIR);
        assert_eq!(
            world.light_rgb_at_pos(across).0,
            [0; 3],
            "light did not drain across {:?} {:?}",
            seam.face,
            seam.direction
        );
    }
}

#[test]
fn water_equalizes_within_the_band() {
    let reg = base_reg();
    let mut w = test_world_with("equalize", reg.clone());
    let h = w.surface_height(4, 4);
    let y = h + 5;
    let stone = b(&reg, "base:stone");
    // A sealed two-cell trench.
    for x in 3..=6 {
        for z in 3..=5 {
            for yy in (y - 1)..=y {
                w.set_block(x, yy, z, stone);
            }
        }
    }
    w.set_block(4, y, 4, AIR);
    w.set_block(5, y, 4, AIR);
    w.set_block(4, y, 4, reg.water_block(0));
    settle_water(&mut w);
    let a = reg.water_volume(w.get_block(4, y, 4)).unwrap_or(0);
    let c = reg.water_volume(w.get_block(5, y, 4)).unwrap_or(0);
    assert_eq!(a + c, 8, "the trench holds all 8 units");
    assert!(a.abs_diff(c) < 2, "levels equalized: {a} vs {c}");
}

#[test]
fn breached_pond_drains_only_what_left() {
    let reg = base_reg();
    let mut w = test_world_with("breach", reg.clone());
    let h = w.surface_height(8, 8);
    let y = h + 6;
    let stone = b(&reg, "base:stone");
    // A platform, a walled basin on it, a full 3x3 pond inside.
    for x in 0..=16 {
        for z in 0..=16 {
            w.set_block(x, y - 1, z, stone);
        }
    }
    for x in 6..=10 {
        for z in 6..=10 {
            if x == 6 || x == 10 || z == 6 || z == 10 {
                w.set_block(x, y, z, stone);
            }
        }
    }
    for x in 7..=9 {
        for z in 7..=9 {
            w.set_block(x, y, z, reg.water_block(0));
        }
    }
    settle_water(&mut w);
    assert_eq!(reg.water_volume(w.get_block(8, y, 8)), Some(8));
    let total = total_water(&w);
    // Breach the rim: the pond genuinely lowers, nothing duplicates.
    w.set_block(10, y, 8, AIR);
    settle_water(&mut w);
    assert_eq!(total_water(&w), total, "no volume created by the breach");
    assert!(
        reg.water_volume(w.get_block(8, y, 8)).unwrap_or(0) < 8,
        "the pond actually dropped"
    );
    assert!(
        (11..=14).any(|x| reg.is_water(w.get_block(x, y, 8))),
        "water escaped through the breach"
    );
}

#[test]
fn random_ticks_budget_stamps_and_persist() {
    let reg = base_reg();
    let dir = tmp_dir("stamps");
    let mut w = World::new(42, dir.clone(), reg.clone());
    for x in 0..3 {
        for z in 0..3 {
            w.ensure_chunk(tchunk(x, z));
        }
    }
    w.clock = 100.0;
    let mut rng = 7u32;
    let burst = w.random_tick(&mut rng);
    assert_eq!(burst, 9 * 256, "long-waited chunks catch up at the cap");
    let again = w.random_tick(&mut rng);
    assert_eq!(again, 9 * 8, "freshly stamped chunks take the floor burst");
    assert_eq!(w.chunk_stamp(0, 0), Some(100.0));
    save_world(&mut w);
    let w2 = World::load_or_create(dir, reg.clone()).unwrap();
    assert_eq!(w2.chunk_stamp(0, 0), Some(100.0), "stamps persist");
}

#[test]
fn random_ticks_visit_a_bounded_cohort() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("cohort"), reg.clone());
    for x in 0..9 {
        for z in 0..9 {
            w.ensure_chunk(tchunk(x, z));
        }
    }
    // 81 chunks loaded, all stamped at clock 0; K = 64 caps the visit.
    w.clock = 5.0;
    let mut rng = 3u32;
    // Five seconds of waiting, at the world's sample rate; the 47
    // chunks already visited this clock fall back to the floor of 8.
    let waited = (5.0 * crate::world::RANDOM_TICKS_PER_CHUNK_SEC) as usize;
    assert_eq!(
        w.random_tick(&mut rng),
        64 * waited,
        "K chunks, elapsed-scaled"
    );
    assert_eq!(
        w.random_tick(&mut rng),
        17 * waited + 47 * 8,
        "oldest first"
    );
}

#[test]
fn exposed_water_evaporates_without_a_depth_or_basin_exemption() {
    let reg = base_reg();
    let mut w = test_world_with("evap", reg.clone());
    w.day = crate::world::SEASON_DAYS; // summer
    let stone = b(&reg, "base:stone");
    let h = w.surface_height(4, 4);
    let y = h + 8;
    // A walled 2x2 shallow pan, sky open.
    for x in 2..=7 {
        for z in 2..=7 {
            w.set_block(x, y - 1, z, stone);
            w.set_block(x, y, z, stone);
        }
    }
    w.mobs_mut().clear();
    for (x, z) in [(4, 4), (5, 4), (4, 5), (5, 5)] {
        w.set_block(x, y, z, reg.water_block(0));
    }
    // A deep walled shaft: three stacked cells, sky open.
    for x in 10..=12 {
        for z in 3..=5 {
            for yy in (y - 3)..=y {
                w.set_block(x, yy, z, stone);
            }
        }
    }
    for yy in (y - 2)..=y {
        w.set_block(11, yy, 4, reg.water_block(0));
    }
    // An open spill: a lone film on flat ground.
    w.set_block(9, y - 1, 9, stone);
    w.set_block(9, y, 9, reg.water_for_volume(1));
    let mut rng = 11u32;
    for _ in 0..40_000 {
        w.random_tick(&mut rng);
        w.tick_water(1_000);
    }
    let pan_after = [(4, 4), (5, 4), (4, 5), (5, 5)]
        .into_iter()
        .map(|(x, z)| u32::from(reg.water_volume(w.get_block(x, y, z)).unwrap_or(0)))
        .sum::<u32>();
    assert!(pan_after < 32, "the exposed pan lost water to evaporation");
    let shaft_after = ((y - 2)..=y)
        .map(|yy| u32::from(reg.water_volume(w.get_block(11, yy, 4)).unwrap_or(0)))
        .sum::<u32>();
    assert!(
        shaft_after < 24,
        "surface depth is no longer a magical evaporation exemption"
    );
    assert_eq!(w.get_block(9, y, 9), AIR, "open spills dry entirely");
}

#[test]
fn rain_refills_surface_water() {
    let reg = base_reg();
    let mut w = test_world_with("rain", reg.clone());
    w.force_local_weather("rain");
    let stone = b(&reg, "base:stone");
    let h = w.surface_height(4, 4);
    let y = h + 8;
    w.set_block(4, y - 1, 4, stone);
    for (x, z) in [(3, 4), (5, 4), (4, 3), (4, 5)] {
        w.set_block(x, y, z, stone);
    }
    w.set_block(4, y, 4, reg.water_for_volume(2));
    for _ in 0..12 {
        w.rain_fill(4, 4);
    }
    assert_eq!(
        reg.water_volume(w.get_block(4, y, 4)),
        Some(8),
        "rain topped the cell back up to full"
    );
}

#[test]
fn reconcile_catches_up_an_absent_chunk() {
    let reg = base_reg();
    let dir = tmp_dir("reconcile").join("world");
    crate::world::create_world_fixture_atomic(
        &dir,
        42,
        "survival",
        8,
        &crate::planet_atlas::CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    let mut w = World::load_or_create(dir.clone(), reg.clone()).unwrap();
    let anchor = crate::planet::Face::ALL
        .into_iter()
        .flat_map(|face| {
            (128u16..crate::planet::FACE_BLOCKS)
                .step_by(256)
                .flat_map(move |u| {
                    (128u16..crate::planet::FACE_BLOCKS)
                        .step_by(256)
                        .map(move |v| crate::planet::SurfacePos::new(face, u, v).unwrap())
                })
        })
        .find(|&pos| {
            let latitude = w.latitude_at_surface(pos);
            let (summer_day, winter_day) = if latitude < 0.0 {
                (3 * crate::world::SEASON_DAYS, crate::world::SEASON_DAYS)
            } else {
                (crate::world::SEASON_DAYS, 3 * crate::world::SEASON_DAYS)
            };
            w.temperature_at_surface_on_day(pos, f64::from(winter_day)) < -0.5
                && w.temperature_at_surface_on_day(pos, f64::from(summer_day)) > 8.0
                && w.soil_moisture_at_surface(pos) > 0.35
                && w.generator.surface_estimate_at(pos) > SEA_LEVEL + 2
        })
        .expect("seasonally freezing agricultural country");
    ensure_surface_neighborhood(&mut w, anchor, 1);
    let b = |n: &str| reg.block_id(n).unwrap();
    let y = 200;
    // A supported sky-open pool (the shelf the live winter test uses)
    // and a farmland strip about to miss three growing seasons.
    let pool: Vec<_> = (0..8)
        .map(|du| block_pos(surface_offset(anchor, du, 4), y + 1))
        .collect();
    let crops: Vec<_> = (0..8)
        .map(|du| block_pos(surface_offset(anchor, du, -4), y + 1))
        .collect();
    for (&water, &crop) in pool.iter().zip(&crops) {
        w.set_block_at(water.offset(0, -1, 0).unwrap(), b("base:planks"));
        w.set_block_at(water, reg.water_block(0));
        w.set_block_at(crop.offset(0, -1, 0).unwrap(), b("base:farmland"));
        w.set_block_at(crop, b("base:wheat_seeds"));
    }
    save_world(&mut w);

    // Reopen the world more than a year later, at the start of local winter.
    let mut w2 = World::load_or_create(dir, reg.clone()).unwrap();
    w2.day = local_season_day(&w2, anchor, 3) + crate::planet_atlas::YEAR_DAYS;
    w2.clock = w2.day as f64 * 600.0;
    ensure_surface_neighborhood(&mut w2, anchor, 1);
    let iced = pool
        .iter()
        .filter(|&&pos| w2.get_block_at(pos) == b("base:ice"))
        .count();
    let winter_weather = w2.weather_at_surface(anchor);
    assert!(
        iced >= 6,
        "the pool froze while you were away ({iced}/8 at {:.2} C, season {}, t {:.3}, latitude {:.3}, day {})",
        winter_weather.temperature_c,
        w2.season_at_surface(anchor),
        w2.generator.climate_at(anchor).t,
        w2.latitude_at_surface(anchor),
        w2.day
    );
    let grown = crops
        .iter()
        .filter(|&&pos| w2.get_block_at(pos) != b("base:wheat_seeds"))
        .count();
    assert!(grown > 0, "crops advanced over the missed seasons");
}

#[test]
fn water_defers_at_the_worlds_edge() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("borderwater"), reg.clone());
    w.ensure_chunk(tchunk(0, 0));
    let stone = b(&reg, "base:stone");
    let y = 250;
    // A shelf against the +x seam, walled on every loaded side.
    w.set_block(15, y - 1, 4, stone);
    w.set_block(14, y, 4, stone);
    w.set_block(15, y, 3, stone);
    w.set_block(15, y, 5, stone);
    w.set_block(15, y, 4, reg.water_block(0));
    for _ in 0..50 {
        w.tick_water(10_000);
    }
    assert_eq!(
        reg.water_volume(w.get_block(15, y, 4)),
        Some(8),
        "water waits at the ungenerated seam instead of vanishing"
    );
    // The neighbor generates: the seam wakes and the flow resumes.
    w.ensure_chunk(tchunk(1, 0));
    let t1 = total_water(&w);
    settle_water(&mut w);
    assert_eq!(total_water(&w), t1, "crossing the seam conserved volume");
    assert!(
        reg.water_volume(w.get_block(15, y, 4)).unwrap_or(0) < 8,
        "the seam wake resumed the flow"
    );
}

#[test]
fn material_checkpoint_failure_cancels_voxel_placement() {
    let reg = base_reg();
    let dir = tmp_dir("material-checkpoint-cancel").join("world");
    crate::world::create_world_fixture_atomic(
        &dir,
        42,
        "survival",
        8,
        &crate::planet_atlas::CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    let mut world = World::load_or_create(dir.clone(), reg.clone()).unwrap();
    let chunk = tchunk(0, 0);
    world.ensure_chunk(chunk);
    let surface = crate::planet::SurfacePos::new(
        chunk.face(),
        chunk.u() * crate::chunk::CHUNK_X as u16 + crate::chunk::CHUNK_X as u16 / 2,
        chunk.v() * crate::chunk::CHUNK_Z as u16 + crate::chunk::CHUNK_Z as u16 / 2,
    )
    .unwrap();
    let target = block_pos(surface, world.surface_height_at(surface) + 1);
    assert_eq!(world.get_block_at(target), AIR);
    let break_target = target.offset(0, 1, 0).unwrap();
    let copper = reg.block_id("base:copper_block").unwrap();
    world.set_block_at(break_target, copper);

    // An existing directory cannot be atomically replaced by a ledger file.
    // This deterministically exercises the same failed-checkpoint path as a
    // Windows sharing/access denial without depending on host permissions.
    let blocked = dir.join("blocked-ledger-path");
    std::fs::create_dir(&blocked).unwrap();
    world
        .material_ledger
        .as_mut()
        .unwrap()
        .force_checkpoint_failure_at(blocked);
    assert!(!world.place_block_at(target, copper));
    assert_eq!(
        world.get_block_at(target),
        AIR,
        "a failed material journal must leave the voxel untouched"
    );
    assert!(
        world
            .break_block_at(break_target, None, true, true)
            .is_none()
    );
    assert_eq!(
        world.get_block_at(break_target),
        copper,
        "a failed material journal must not remove the voxel"
    );
    assert!(
        !world.player_touched.contains(&chunk),
        "a cancelled action must not suppress safe retrogen"
    );
}

#[test]
fn world_listing_only_includes_compatible_planets() {
    // Regression: the title list only read the legacy `seed` file, so
    // world.toml worlds were invisible and their folder names got reused
    // by NEW WORLD — inheriting the old player.toml (inventory carryover).
    let root = tmp_dir("listworlds");
    crate::world::create_world_fixture_atomic(
        &root.join("world1"),
        42,
        "survival",
        4,
        &crate::planet_atlas::CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    std::fs::create_dir_all(root.join("old")).unwrap();
    std::fs::write(root.join("old/seed"), "7").unwrap();
    std::fs::create_dir_all(root.join("junk")).unwrap();
    std::fs::write(root.join("stray.txt"), "x").unwrap();
    let worlds = crate::world::list_worlds(&root);
    assert_eq!(
        worlds,
        vec![("world1".to_string(), 42)],
        "only validated planetary worlds are selectable"
    );
    let inspected = crate::world::inspect_worlds(&root);
    let ready = inspected
        .iter()
        .find(|entry| entry.name == "world1")
        .unwrap();
    assert!(ready.playable);
    assert!(ready.status.contains(&format!(
        "GENERATOR {}",
        crate::world::WORLD_GENERATOR_VERSION
    )));
    assert!(ready.status.contains(&format!(
        "ATLAS {}",
        crate::planet_atlas::ATLAS_FORMAT_VERSION
    )));
    assert!(ready.status.contains("CONTENT"));
    let old = inspected.iter().find(|entry| entry.name == "old").unwrap();
    assert!(!old.playable);
    assert!(old.status.contains("INCOMPATIBLE"));
    assert!(old.status.contains("legacy flat"));
    let junk = inspected.iter().find(|entry| entry.name == "junk").unwrap();
    assert!(!junk.playable);
    assert!(junk.status.contains("INCOMPLETE"));
}

#[test]
fn new_world_name_never_reuses_existing_folder() {
    let root = tmp_dir("nextworld");
    crate::world::write_world_meta(&root.join("world1"), 1, "survival", 0.0).unwrap();
    std::fs::write(root.join("world1/player.toml"), "leftover inventory").unwrap();
    let listed = crate::world::list_worlds(&root);
    assert_eq!(crate::next_world_name(&root, &listed), "world2");
    // Even a folder the listing can't see must not be adopted as "new".
    assert_eq!(crate::next_world_name(&root, &[]), "world2");
    assert_eq!(
        crate::next_world_name(&tmp_dir("nextworld-empty"), &[]),
        "world1"
    );
}

#[test]
fn torch_light_propagates_and_walls_block_it() {
    let reg = base_reg();
    let mut w = test_world("lighttorch");
    let stone = reg.block_id("base:stone").unwrap();
    let torch = reg.block_id("base:torch").unwrap();
    // Sealed 9x9x9 stone box, hollow interior, well above terrain.
    for x in 0..9 {
        for z in 0..9 {
            for y in 150..159 {
                let shell = x == 0 || x == 8 || z == 0 || z == 8 || y == 150 || y == 158;
                w.set_block(x, y, z, if shell { stone } else { AIR });
            }
        }
    }
    assert_eq!(w.light_at(4, 154, 4), (0, 0), "sealed box is pitch black");
    w.set_block(4, 151, 4, torch);
    assert_eq!(w.light_at(4, 151, 4).0, 14, "torch emits 14");
    assert_eq!(w.light_at(5, 151, 4).0, 13, "one step dims by one");
    assert_eq!(w.light_at(7, 151, 4).0, 11, "three steps");
    assert_eq!(w.light_at(4, 153, 4).0, 12, "propagates vertically too");
    assert_eq!(w.light_at(10, 151, 4).0, 0, "opaque wall stops it");
    w.set_block(4, 151, 4, AIR);
    assert_eq!(
        w.light_at(5, 151, 4).0,
        0,
        "removing the torch relights dark"
    );
}

#[test]
fn sky_light_surface_cave_and_roof_opening() {
    let reg = base_reg();
    let mut w = test_world("lightsky");
    let stone = reg.block_id("base:stone").unwrap();
    // Open surface reads full sky (a built block, sea-proof).
    w.set_block(2, 140, 2, stone);
    assert_eq!(w.light_at(2, 141, 2).1, 15, "surface is full daylight");
    // Sealed box: no sky inside; opening the roof floods it.
    for x in 20..29 {
        for z in 20..29 {
            for yy in 150..159 {
                let shell = x == 20 || x == 28 || z == 20 || z == 28 || yy == 150 || yy == 158;
                w.set_block(x, yy, z, if shell { stone } else { AIR });
            }
        }
    }
    assert_eq!(w.light_at(24, 154, 24).1, 0, "sealed roof blocks sky");
    w.set_block(24, 158, 24, AIR); // skylight hole
    assert_eq!(
        w.light_at(24, 154, 24).1,
        15,
        "column under the hole is lit"
    );
    assert_eq!(
        w.light_at(26, 154, 24).1,
        13,
        "and floods sideways, dimming"
    );
}

#[test]
fn light_crosses_chunk_borders() {
    let reg = base_reg();
    let mut w = test_world("lightseam");
    let torch = reg.block_id("base:torch").unwrap();
    // Torch on the last column of chunk (0,0); the neighbor chunk must see it.
    w.set_block(15, 200, 8, torch);
    assert_eq!(w.light_at(15, 200, 8).0, 14);
    assert_eq!(w.light_at(16, 200, 8).0, 13, "crosses the seam");
    assert_eq!(w.light_at(19, 200, 8).0, 10, "keeps dimming next door");
}

#[test]
fn water_dims_sky_and_mod_blocks_can_glow() {
    let reg = base_reg();
    let mut w = test_world("lightwater");
    let water = reg.water_ids[0];
    let stone = reg.block_id("base:stone").unwrap();
    // A water-filled shaft walled in stone: light only enters from above,
    // dimming one level per water block.
    for x in 2..7 {
        for z in 2..7 {
            for y in 179..183 {
                w.set_block(x, y, z, stone);
            }
        }
    }
    for y in 180..183 {
        w.set_block(4, y, 4, water);
    }
    assert_eq!(w.light_at(4, 182, 4).1, 14, "first water block dims to 14");
    assert_eq!(w.light_at(4, 180, 4).1, 12, "third dims to 12");

    // Mod block with light = 9.
    let root = tmp_dir("glowmod");
    let dir = root.join("glow");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"glow\"\nworld_api = 2\n").unwrap();
    std::fs::write(
        dir.join("blocks.toml"),
        "[[block]]\nid = \"lamp\"\ntexture = \"@stone\"\nlight = 9\n",
    )
    .unwrap();
    let reg2 = Arc::new(registry::load(&root));
    let lamp = reg2.block_id("glow:lamp").unwrap();
    assert_eq!(reg2.block(lamp).light_emit, 9);
    let mut w2 = test_world_with("lightmod", reg2);
    w2.set_block(4, 200, 4, lamp);
    assert_eq!(w2.light_at(4, 200, 4).0, 9, "emitter itself");
    assert_eq!(w2.light_at(4, 202, 4).0, 7, "two steps out");
}

#[test]
fn placing_a_roof_casts_shadow() {
    let reg = base_reg();
    let mut w = test_world("lightshadow");
    let stone = reg.block_id("base:stone").unwrap();
    let y = w.surface_height(8, 8);
    assert_eq!(w.light_at(8, y + 1, 8).1, 15);
    w.set_block(8, y + 3, 8, stone); // roof two above the ground cell
    let shaded = w.light_at(8, y + 1, 8).1;
    assert!(shaded < 15, "column shadowed, got {shaded}");
    assert!(shaded >= 12, "but side-lit by flood, got {shaded}");
}

#[test]
fn relight_perf_sane() {
    let mut w = test_world("lightperf");
    let t0 = std::time::Instant::now();
    for _ in 0..10 {
        w.relight_and_cascade(tchunk(0, 0));
    }
    let per = t0.elapsed().as_secs_f32() / 10.0;
    assert!(per < 0.05, "relight cascade averaged {per:.4}s");
}

#[test]
fn torch_needs_ground_and_pops_without_it() {
    let reg = base_reg();
    let mut w = test_world("torchpop");
    let stone = reg.block_id("base:stone").unwrap();
    let torch = reg.block_id("base:torch").unwrap();
    w.set_block(5, 150, 5, stone);
    w.set_block(5, 151, 5, torch);
    assert_eq!(w.get_block(5, 151, 5), torch);
    // Mining the support pops the torch off as a drop.
    w.set_block(5, 150, 5, AIR);
    assert_eq!(w.get_block(5, 151, 5), AIR, "torch popped");
    assert!(
        w.pending_drops()
            .iter()
            .any(|(_, s)| reg.item(s.item).name == "base:torch"),
        "torch dropped as an item"
    );
    // Torch recipe: charcoal over stick -> 4.
    let mut g = vec![None; 9];
    g[0] = Some(ItemStack::new(&reg, it(&reg, "base:charcoal"), 1));
    g[3] = Some(ItemStack::new(&reg, it(&reg, "base:stick"), 1));
    let r = crate::crafting::match_recipe(&reg, &g, 3).expect("torch recipe");
    assert_eq!(r.output, it(&reg, "base:torch"));
    assert_eq!(r.count, 4);
}

#[test]
fn ire_gains_decay_tiers_and_persistence() {
    let reg = base_reg();
    let dir = tmp_dir("iresave");
    let mut w = World::new(3, dir.clone(), reg.clone());
    // Block classes.
    assert_eq!(w.ire_for_block(reg.block_id("base:log").unwrap()), 0.3);
    assert_eq!(
        w.ire_for_block(reg.block_id("base:copper_ore").unwrap()),
        0.4
    );
    assert_eq!(w.ire_for_block(reg.block_id("base:stone").unwrap()), 0.05);
    assert_eq!(w.ire_for_block(reg.block_id("base:planks").unwrap()), 0.0);
    // Tier thresholds.
    w.add_ire(30.0);
    assert_eq!(w.ire_tier(), 1, "uneasy");
    w.add_ire(60.0);
    assert_eq!(w.ire_tier(), 3, "wrathful");
    w.add_ire(500.0);
    assert_eq!(w.ire, 100.0, "clamped");
    // Decay: -4 per day.
    w.tick_ire(0.5);
    assert!((w.ire - 98.0).abs() < 0.01);
    // Planting refunds, capped at 8/day.
    for _ in 0..100 {
        w.plant_ire(0.5);
    }
    assert!((w.ire - 90.0).abs() < 0.01, "daily cap of 8, got {}", w.ire);
    w.tick_ire(0.6); // day rolls over -> cap resets
    w.plant_ire(0.5);
    assert!(w.ire < 90.0 - 2.0, "cap reset next day");
    // Persistence via world.toml.
    save_world(&mut w);
    let w2 = World::load_or_create(dir, reg).unwrap();
    assert!((w2.ire - w.ire).abs() < 0.01, "ire round-trips");
}

#[test]
fn saplings_parse_drop_and_grow() {
    let reg = base_reg();
    // Leaves carry sapling bonus drops of their own species.
    let oak_leaves = reg.block(reg.block_id("base:leaves").unwrap());
    let (bd_item, ch) = oak_leaves.bonus_drop.expect("leaves drop saplings");
    assert_eq!(reg.item(bd_item).name, "base:oak_sapling");
    assert!((ch - 0.1).abs() < 0.001);
    let spruce = reg.block(reg.block_id("base:spruce_leaves").unwrap());
    assert_eq!(
        reg.item(spruce.bonus_drop.unwrap().0).name,
        "base:spruce_sapling"
    );
    // Grow: sapling on dirt in open air becomes a real tree.
    let mut w = test_world("sapgrow");
    let dirt = reg.block_id("base:dirt").unwrap();
    let sap = reg.block_id("base:oak_sapling").unwrap();
    w.set_block(4, 199, 4, dirt);
    w.set_block(4, 200, 4, sap);
    let ire0 = {
        w.ire = 50.0;
        w.ire
    };
    assert!(w.try_grow_sapling(4, 200, 4, 7), "clear sky: grows");
    let log = reg.block_id("base:log").unwrap();
    assert_eq!(w.get_block(4, 200, 4), log, "trunk replaces the sapling");
    let leaves = reg.block_id("base:leaves").unwrap();
    let mut leaf_count = 0;
    for x in 0..9 {
        for z in 0..9 {
            for y in 200..212 {
                if w.get_block(x, y, z) == leaves {
                    leaf_count += 1;
                }
            }
        }
    }
    assert!(leaf_count > 8, "canopy grew ({leaf_count} leaves)");
    assert!(
        (w.ire - (ire0 - 2.0)).abs() < 0.01,
        "maturation refunds 2 ire"
    );
    // Blocked trunk: stays a sapling.
    let stone = reg.block_id("base:stone").unwrap();
    w.set_block(8, 199, 8, dirt);
    w.set_block(8, 200, 8, sap);
    w.set_block(8, 202, 8, stone);
    assert!(!w.try_grow_sapling(8, 200, 8, 7), "blocked: stays");
    assert_eq!(w.get_block(8, 200, 8), sap);
}

#[test]
fn offering_stone_values_and_dawn() {
    let reg = base_reg();
    let mut w = test_world("offer");
    let stone = reg.block_id("base:offering_stone").unwrap();
    assert_eq!(reg.block(stone).interaction.as_deref(), Some("offering"));
    assert_eq!(reg.block(stone).light_emit, 5, "faint wildlight");
    assert!(!reg.recipes_for(it(&reg, "base:offering_stone")).is_empty());
    // Value table: the wild's own materials 2.0, meat 1.0, bread hunger*0.25.
    let v = |name: &str, n: u32| w.offering_value(&ItemStack::new(&reg, it(&reg, name), n));
    assert_eq!(v("base:heartwood", 1), 2.0);
    assert_eq!(v("base:raw_venison", 2), 2.0);
    assert!(
        (v("base:bread", 1) - 1.5).abs() < 0.01,
        "bread hunger 6 * 0.25"
    );
    assert_eq!(v("base:oak_sapling", 1), 1.0);
    // Dawn: items taken, refund capped at 10.
    w.ire = 60.0;
    let mut st = crate::world::OfferingState::default();
    st.slots[0] = Some(ItemStack::new(&reg, it(&reg, "base:raw_venison"), 6)); // 6.0
    st.slots[1] = Some(ItemStack::new(&reg, it(&reg, "base:raw_rabbit"), 5)); // 5.0
    w.insert_block_entity((3, 90, 3), crate::world::BlockEntity::Offering(st));
    let r = w.accept_offerings();
    assert!((r - 10.0).abs() < 0.01, "capped at 10, got {r}");
    assert!((w.ire - 50.0).abs() < 0.01);
    let Some(crate::world::BlockEntity::Offering(o)) = w.block_entity(&(3, 90, 3)) else {
        panic!()
    };
    assert!(
        o.slots.iter().all(|s| s.is_none()),
        "the wild took everything"
    );
    assert_eq!(w.accept_offerings(), 0.0, "empty stone gives nothing");
}

#[test]
fn server_ticks_at_fixed_rate_and_runs_the_world() {
    let reg = base_reg();
    let world = test_world("simsplit");
    let mut sv = crate::server::Server::new(world, 0.3, 42);
    let ctx = crate::server::PlayerCtx {
        id: 0,
        pos: ep(Vec3::new(8.0, 80.0, 8.0)),
        spawn: ep(Vec3::new(-500.0, 70.0, -500.0)),
        attackable: true,
        aggro_mod: 0.0,
        quiet_charm: None,
    };
    let t0 = sv.time_of_day;
    let mut evs = Vec::new();
    // 2 wall-seconds in odd chunks: the fixed tick must absorb it evenly.
    for _ in 0..120 {
        sv.advance(1.0 / 60.0, &[ctx], &mut evs);
    }
    let advanced = sv.time_of_day - t0;
    assert!(
        (advanced - 2.0 / crate::server::DAY_LENGTH).abs() < 0.0005,
        "clock advanced by the simulated time, got {advanced}"
    );
    // A hitch doesn't spiral the simulation.
    sv.advance(30.0, &[ctx], &mut evs);
    assert!(sv.time_of_day - t0 < 0.01, "hitch capped, not replayed");
    // Ire tier events flow through the server.
    sv.world.ire = 95.0;
    let mut evs2 = Vec::new();
    sv.advance(0.1, &[ctx], &mut evs2);
    assert!(
        evs2.iter()
            .any(|e| matches!(e, crate::server::SimEvent::IreTier { rose: true, .. })),
        "tier change surfaced as a SimEvent"
    );
    let _ = reg;
}

#[test]
fn calendar_advances_and_persists_without_a_global_weather_state() {
    let reg = base_reg();
    let w = World::new(42, tmp_dir("wx-day"), reg.clone());
    let mut sim = crate::server::Server::new(w, 0.999, 5);
    let mut ev = Vec::new();
    for _ in 0..40 {
        sim.advance(0.1, &[], &mut ev); // the hitch cap swallows big steps
    }
    assert_eq!(sim.world.day, 1, "midnight rolls the calendar");
    sim.sleep_to_dawn();
    assert_eq!(sim.world.day, 2, "sleeping skips into tomorrow");

    // Only the calendar rides world.toml. Local weather persists in the
    // dynamic atlas snapshot and has no world-wide enum to serialize.
    let dir = tmp_dir("wx-persist");
    let mut w = World::new(42, dir.clone(), reg.clone());
    let midsummer = crate::world::SEASON_DAYS + crate::world::SEASON_DAYS / 2;
    w.day = midsummer;
    save_world(&mut w);
    let w2 = World::load_or_create(dir, reg).unwrap();
    assert_eq!(w2.day, midsummer);
    assert_eq!(w2.season(), 1, "a day and a half of seasons in is summer");
}

#[test]
fn winter_gates_growth_and_freezes_exposed_water() {
    use crate::worldgen::Biome;

    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("wx-winter"), reg.clone());
    let b = |n: &str| reg.block_id(n).unwrap();
    let anchor = find_biome_where(&w.generator, Biome::Plains, |pos| {
        let t = w.generator.climate_at(pos).t;
        let latitude = w.latitude_at_surface(pos);
        let winter_temperature = t * 22.0 + 8.0 - latitude.sin().abs() as f32 * 14.0;
        winter_temperature < -0.5 && w.generator.surface_estimate_at(pos) > SEA_LEVEL + 2
    })
    .expect("seasonally freezing planetary country");
    w.day = local_season_day(&w, anchor, 3);
    ensure_surface_neighborhood(&mut w, anchor, 1);
    let y = 200;

    // A strip of sky-open wheat on farmland never advances in winter...
    let open: Vec<_> = (0..16)
        .map(|du| block_pos(surface_offset(anchor, du - 8, -4), y + 1))
        .collect();
    for &crop in &open {
        w.set_block_at(crop.offset(0, -1, 0).unwrap(), b("base:farmland"));
        w.set_block_at(crop, b("base:wheat_seeds"));
    }
    // ...while a roofed, torchlit one still creeps (the greenhouse).
    let roofed: Vec<_> = (0..16)
        .map(|du| block_pos(surface_offset(anchor, du - 8, 0), y + 1))
        .collect();
    for (index, &crop) in roofed.iter().enumerate() {
        w.set_block_at(crop.offset(0, -1, 0).unwrap(), b("base:farmland"));
        w.set_block_at(crop, b("base:wheat_seeds"));
        w.set_block_at(crop.offset(0, 2, 0).unwrap(), b("base:planks"));
        if index % 3 == 0 {
            let torch = block_pos(surface_offset(crop.surface(), 0, 1), y + 1);
            w.set_block_at(torch.offset(0, -1, 0).unwrap(), b("base:planks"));
            w.set_block_at(torch, b("base:torch"));
        }
    }
    let mut rng = 7u32;
    for _ in 0..1_000 {
        w.clock += 100.0;
        w.random_tick(&mut rng);
    }
    let open_grown = open
        .iter()
        .filter(|&&pos| w.get_block_at(pos) != b("base:wheat_seeds"))
        .count();
    let roofed_grown = roofed
        .iter()
        .filter(|&&pos| {
            let g = w.get_block_at(pos);
            g != b("base:wheat_seeds") && g != AIR
        })
        .count();
    assert_eq!(open_grown, 0, "winter halts sky-open crops");
    assert!(
        roofed_grown > 0,
        "roof + torchlight keeps a greenhouse alive"
    );

    // Exposed still water freezes over in winter...
    let pool: Vec<_> = (0..8)
        .map(|du| block_pos(surface_offset(anchor, du - 4, 6), y + 1))
        .collect();
    for &water in &pool {
        w.set_block_at(water.offset(0, -1, 0).unwrap(), b("base:planks"));
        w.set_block_at(water, reg.water_block(0));
    }
    // (support keeps it a still pool; sky above is open)
    for _ in 0..1_000 {
        w.clock += 100.0;
        w.random_tick(&mut rng);
    }
    let iced = pool
        .iter()
        .filter(|&&pos| w.get_block_at(pos) == b("base:ice"))
        .count();
    let winter_weather = w.weather_at_surface(anchor);
    assert!(
        iced > 0,
        "winter freezes exposed pools, froze {iced} at {:.2} C and latitude {:.1} degrees",
        winter_weather.temperature_c,
        w.latitude_at_surface(anchor).to_degrees()
    );

    // ...and spring gives them back.
    w.day = local_season_day(&w, anchor, 0);
    for _ in 0..1_000 {
        w.clock += 100.0;
        w.random_tick(&mut rng);
    }
    let thawed = pool
        .iter()
        .filter(|&&pos| w.get_block_at(pos) == reg.water_block(0))
        .count();
    assert!(thawed > 0, "spring thaws the ice, thawed {thawed}");
}

#[test]
fn snow_settles_melts_and_snowballs_fly() {
    use crate::worldgen::Biome;
    use glam::Vec3;

    let reg = base_reg();
    let mut w = test_world_with("wx-snow", reg.clone());
    let b = |n: &str| reg.block_id(n).unwrap();
    let layer = b("base:snow_layer");
    assert_eq!(
        reg.block(layer).height,
        Some(0.125),
        "snow layers render thin"
    );

    // Snowfall settles one layer on a cold, sky-open column - once.
    let cold = find_biome(&w.generator, Biome::Arctic).expect("cold land on the planet");
    let temperate = find_biome_where(&w.generator, Biome::Plains, |pos| {
        let t = w.generator.climate_at(pos).t;
        (0.32..=0.5).contains(&t) && w.generator.surface_estimate_at(pos) > SEA_LEVEL + 2
    })
    .expect("temperate land on the planet");
    w.day = local_season_day(&w, cold, 3);
    w.force_local_weather("precip");
    ensure_surface_neighborhood(&mut w, cold, 1);
    ensure_surface_neighborhood(&mut w, temperate, 1);
    let cy = w.surface_height_at(cold);
    let snow = block_pos(cold, cy + 1);
    w.settle_snow_at(cold);
    assert_eq!(
        w.get_block_at(snow),
        layer,
        "snow settled on the cold column"
    );
    w.settle_snow_at(cold);
    assert_eq!(block_at(&w, cold, cy + 2), AIR, "layers never stack");
    let wy = w.surface_height_at(temperate);
    w.settle_snow_at(temperate);
    assert_ne!(
        block_at(&w, temperate, wy + 1),
        layer,
        "temperate columns shrug it off"
    );

    // Torchlight melts layers even in an arctic winter.
    let torch_surface = surface_offset(cold, 1, 0);
    w.set_block_at(block_pos(torch_surface, cy), b("base:stone"));
    w.set_block_at(block_pos(torch_surface, cy + 1), b("base:torch"));
    let mut rng = 9u32;
    for _ in 0..30_000 {
        w.random_tick(&mut rng);
        if w.get_block_at(snow) != layer {
            break;
        }
    }
    assert_eq!(
        reg.water_volume(w.get_block_at(snow)),
        Some(1),
        "bright light turns snow into its exact meltwater"
    );

    // Breaking a snow block yields snowballs; the crafting loop closes.
    assert_eq!(
        reg.block(b("base:snow")).drops,
        Some((reg.item_id("base:snowball").unwrap(), 4))
    );
    let ball = reg.item_id("base:snowball").unwrap();
    assert_eq!(
        reg.item(ball).throw_speed,
        Some(18.0),
        "snowballs are throwable"
    );
    let grid = [Some(ItemStack::new(&reg, ball, 1)); 4];
    let r = crate::crafting::match_recipe(&reg, &grid, 2).expect("4 snowballs pack a block");
    assert_eq!(r.output, reg.item_id("base:snow").unwrap());

    // A zero-damage projectile still shoves: snowball knockback.
    // Staged high in open sky so terrain can't intercept the shot.
    let sy = 140.0;
    let wild = reg.animals.iter().position(|a| !a.hostile).unwrap();
    let mi = w.mob_count();
    let mut m = crate::mobs::Mob::new(wild, Vec3::new(4.5, sy, 4.5), 0.0);
    m.health = 10.0;
    w.spawn_mob(m);
    w.spawn_projectile(crate::mobs::Projectile {
        stable_id: 0,
        pos: ep(Vec3::new(4.5, sy + 0.4, 3.0)),
        vel: Vec3::new(0.0, 0.0, 12.0),
        tile: 0,
        damage: 0.0,
        damage_type: None,
        age: 0.0,
        from_player: true,
        drop_item: None,
        preparation_payload: None,
        owner: 0,
    });
    for _ in 0..60 {
        w.tick_projectiles(&[], 1.0 / 30.0);
    }
    assert_eq!(w.mobs()[mi].health, 10.0, "a snowball draws no blood");
    assert!(
        w.mobs()[mi].hurt_flash > 0.0 || w.mobs()[mi].vel.length() > 0.1,
        "but it definitely lands"
    );

    // Removing a layer's support pops it as a drop.
    let py = w.surface_height(10, 10);
    w.set_block(10, py + 2, 10, b("base:planks"));
    w.set_block(10, py + 3, 10, layer);
    w.clear_pending_drops();
    w.set_block(10, py + 2, 10, AIR);
    assert_eq!(
        w.get_block(10, py + 3, 10),
        AIR,
        "unsupported layers fall away"
    );
    assert!(
        w.pending_drops().iter().any(|(_, s)| s.item == ball),
        "and hand back their snowball"
    );
}

#[test]
fn weather_and_season_touch_the_sim() {
    let reg = base_reg();
    // Winter pauses breeding even for fed adults side by side.
    let mut w = test_world_with("wx-breed", reg.clone());
    let wild = reg
        .animals
        .iter()
        .position(|a| !a.hostile && a.breed_food.is_some())
        .expect("breedable wildlife");
    let stone2 = reg.block_id("base:stone").unwrap();
    for x in 2..=7 {
        for z in 2..=7 {
            w.set_block(x, 139, z, stone2);
        }
    }
    let y = 140.05f32;
    let breeding_surface = ep(glam::Vec3::new(4.5, y, 4.5)).surface();
    w.day = local_season_day(&w, breeding_surface, 3);
    assert_eq!(w.season_at_surface(breeding_surface), 3);
    let before = w.mob_count();
    for dx in 0..2 {
        let mut m = crate::mobs::Mob::new(wild, glam::Vec3::new(4.5 + dx as f32, y, 4.5), 0.0);
        m.health = 10.0;
        m.fed = true;
        w.spawn_mob(m);
    }
    let mut rng = 3u32;
    for _ in 0..120 {
        w.tick_mobs(&[], 1.0, 1.0 / 30.0, &mut rng);
    }
    assert!(
        w.mobs().iter().all(|m| m.growth >= 1.0),
        "no winter litters"
    );
    assert!(w.mob_count() <= before + 2, "no winter births");
    // Summer: the same pair bears young. Winter wander drifts them
    // apart, so stand them back side by side first.
    w.day = local_season_day(&w, breeding_surface, 1);
    assert_eq!(w.season_at_surface(breeding_surface), 1);
    for m in w.mobs_mut() {
        m.fed = true;
        m.breed_cd = 0.0;
    }
    for (moved, m) in w.mobs_mut().iter_mut().enumerate() {
        m.pos = ep(glam::Vec3::new(4.5 + moved as f32, 140.05, 4.5));
        m.vel = glam::Vec3::ZERO;
    }
    let before = w.mob_count();
    for _ in 0..120 {
        w.tick_mobs(&[], 1.0, 1.0 / 30.0, &mut rng);
    }
    assert!(w.mob_count() > before, "summer births arrive");
}

#[test]
fn stained_glass_filters_torchlight_by_channel() {
    let reg = base_reg();
    let mut w = test_world_with("gw-stain", reg.clone());
    let b = |n: &str| reg.block_id(n).unwrap();
    let my = 120;
    // A sealed corridor: torch | red glass | probe cell.
    let stone = b("base:stone");
    for x in 8..15 {
        for y in my - 1..my + 3 {
            for z in 8..12 {
                w.set_block(x, y, z, stone);
            }
        }
    }
    for x in 9..14 {
        w.set_block(x, my, 10, AIR);
        w.set_block(x, my + 1, 10, AIR);
    }
    w.set_block(9, my, 10, b("base:torch"));
    w.set_block(11, my, 10, b("base:red_glass"));
    w.set_block(11, my + 1, 10, b("base:red_glass"));
    let (rgb, _) = w.light_rgb_at(13, my, 10);
    assert!(rgb[0] > 0, "red passes red glass: {rgb:?}");
    assert_eq!(rgb[1], 0, "green dies at red glass: {rgb:?}");
    assert_eq!(rgb[2], 0, "blue dies at red glass: {rgb:?}");
    // Clear glass passes everything.
    w.set_block(11, my, 10, b("base:glass"));
    w.set_block(11, my + 1, 10, b("base:glass"));
    let (rgb, _) = w.light_rgb_at(13, my, 10);
    // A torch burns warm: blue is already spent at this range, so the
    // proof is red and green surviving where red glass killed green.
    assert!(
        rgb[0] > 0 && rgb[1] > 0,
        "clear passes the torch's warmth: {rgb:?}"
    );
}

#[test]
fn bedrock_floor_is_unbreakable_and_reseals_on_load() {
    let reg = base_reg();
    let root = reg.block_id("base:bedrock").expect("bedrock registered");
    let dir = tmp_dir("floor");
    let mut w = World::new(42, dir.clone(), reg.clone());
    w.ensure_chunk(tchunk(0, 0));
    // Every column of a fresh chunk is floored.
    for x in 0..16 {
        for z in 0..16 {
            assert_eq!(w.get_block(x, 0, z), root, "floor at ({x},0,{z})");
        }
    }
    // No tool gives it a hardness: it cannot be mined.
    let pick = reg.item_id("base:wood_pickaxe");
    assert!(
        reg.effective_hardness(root, pick).is_none(),
        "bedrock unbreakable"
    );
    // A hole knocked in the floor (a creative dig, an old bug) heals
    // when the chunk loads again.
    w.set_block(4, 0, 4, AIR);
    save_world(&mut w);
    drop(w);
    let mut w2 = World::new(42, dir, reg);
    w2.ensure_chunk(tchunk(0, 0));
    assert_eq!(w2.get_block(4, 0, 4), root, "floor resealed on load");
}

#[test]
fn snow_trod_swaps_persists_melts_and_drops() {
    let reg = base_reg();
    let root = tmp_dir("snow-trod").join("world");
    crate::world::create_world_fixture_atomic(
        &root,
        42,
        "survival",
        8,
        &crate::planet_atlas::CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    let mut w = World::load_or_create(root, reg.clone()).unwrap();
    let layer = b(&reg, "base:snow_layer");
    let trod = b(&reg, "base:snow_layer_trod");
    let dirt = b(&reg, "base:dirt");
    let surface =
        crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, 3, 3).unwrap();
    w.ensure_chunk(crate::planet::ChunkPos::from_surface(surface));
    let y = 200;
    let ground = block_pos(surface, y);
    let print = block_pos(surface, y + 1);
    let empty = block_pos(surface, y + 5);
    w.set_block_at(ground, dirt);
    w.set_block_at(print, layer);
    w.set_block_at(empty, AIR);

    // Walking through presses the layer into a print; treading again
    // (or treading air/dirt) changes nothing.
    w.tread_at(print);
    assert_eq!(w.get_block_at(print), trod, "layer pressed to trod");
    w.tread_at(print);
    assert_eq!(w.get_block_at(print), trod, "idempotent");
    w.tread_at(empty);
    assert_eq!(w.get_block_at(empty), AIR, "air stays air");

    // Same shovel yield as fresh snow — the content graph is unmoved.
    assert_eq!(
        reg.drops_for(trod, None),
        reg.drops_for(layer, None),
        "trodden snow drops the same snowball"
    );

    // The trail persists across save/load.
    save_world(&mut w);
    let mut w2 = World::load_or_create(w.save_dir_for_test(), reg.clone()).unwrap();
    w2.ensure_chunk(crate::planet::ChunkPos::from_surface(surface));
    assert_eq!(w2.get_block_at(print), trod, "footprints persist");

    // And melts by the same rule as the untouched layer: torchlight.
    let torch_surface = surface_offset(surface, 1, 0);
    w2.set_block_at(block_pos(torch_surface, y), dirt);
    w2.set_block_at(block_pos(torch_surface, y + 1), b(&reg, "base:torch"));
    let mut rng = 5u32;
    for _ in 0..30_000 {
        w2.random_tick(&mut rng);
        if w2.get_block_at(print) != trod {
            break;
        }
    }
    assert_eq!(
        reg.water_volume(w2.get_block_at(print)),
        Some(1),
        "prints thaw into the same exact meltwater as fresh snow"
    );

    // Guests never tread locally; the host stamps prints for them.
    let mut wr = test_world_with("snow-trod-remote", reg.clone());
    wr.set_remote(true);
    wr.set_block_at(ground, dirt);
    wr.set_block_at(print, layer);
    wr.tread_at(print);
    assert_eq!(
        wr.get_block_at(print),
        layer,
        "remote worlds wait for the echo"
    );
}

#[test]
fn settle_spawn_frees_buried_and_dug_out_spawns() {
    let reg = base_reg();
    let mut w = test_world_with("settlespawn", reg.clone());
    let stone = b(&reg, "base:stone");
    let h = w.surface_height(4, 4);

    // A valid standing spot comes back untouched (bedroll exactness).
    let stand = Vec3::new(4.5, (h + 1) as f32 + 0.2, 4.5);
    assert_eq!(w.settle_spawn(stand), stand);

    // Built over: a hill grows across the spawn cells. The player
    // must come out of the solid, standing on real ground.
    for y in (h + 1)..=(h + 6) {
        w.set_block(4, y, 4, stone);
    }
    let freed = w.settle_spawn(stand);
    let fy = freed.y.floor() as i32;
    assert!(!reg.is_solid(w.get_block(4, fy, 4)), "feet clear");
    assert!(!reg.is_solid(w.get_block(4, fy + 1, 4)), "head clear");
    assert!(reg.is_solid(w.get_block(4, fy - 1, 4)), "ground underfoot");

    // Dug out: the floor under a high spawn is mined away — settle
    // down to the first real floor instead of leaving a free-fall.
    let (px, pz) = (10, 10);
    let ph = w.surface_height(px, pz);
    w.set_block(px, ph + 8, pz, stone);
    let perch = Vec3::new(px as f32 + 0.5, (ph + 9) as f32 + 0.2, pz as f32 + 0.5);
    assert_eq!(w.settle_spawn(perch), perch, "platform stand is valid");
    w.set_block(px, ph + 8, pz, AIR);
    let landed = w.settle_spawn(perch);
    let ly = landed.y.floor() as i32;
    assert!(ly < ph + 9, "came down off the vanished platform");
    assert!(
        reg.is_solid(w.get_block(px, ly - 1, pz)),
        "onto real ground"
    );

    // A column sealed solid from bedrock to sky: walk to a neighbor
    // column rather than teleporting into the fill.
    let (qx, qz) = (20, 20);
    let qh = w.surface_height(qx, qz);
    for y in 1..crate::chunk::CHUNK_Y as i32 - 1 {
        w.set_block(qx, y, qz, stone);
    }
    let sealed = Vec3::new(qx as f32 + 0.5, (qh + 1) as f32 + 0.2, qz as f32 + 0.5);
    let moved = w.settle_spawn(sealed);
    let (mx, mz) = (moved.x.floor() as i32, moved.z.floor() as i32);
    let my = moved.y.floor() as i32;
    assert!((mx, mz) != (qx, qz), "left the sealed column");
    assert!(!reg.is_solid(w.get_block(mx, my, mz)), "feet clear");
    assert!(
        reg.is_solid(w.get_block(mx, my - 1, mz)),
        "ground underfoot"
    );
}

#[test]
fn free_position_rescues_embedded_but_leaves_air_and_water_alone() {
    let reg = base_reg();
    let mut w = test_world_with("freepos", reg.clone());
    let stone = b(&reg, "base:stone");
    let h = w.surface_height(4, 4);

    // Embedded in a hill: rescued to clear cells.
    for y in (h + 1)..=(h + 5) {
        w.set_block(4, y, 4, stone);
    }
    let buried = Vec3::new(4.5, (h + 2) as f32 + 0.2, 4.5);
    let freed = w.free_position(buried);
    let fy = freed.y.floor() as i32;
    assert!(!reg.is_solid(w.get_block(4, fy, 4)), "feet freed");
    assert!(!reg.is_solid(w.get_block(4, fy + 1, 4)), "head freed");

    // A legitimate mid-air save is not touched (physics owns falling).
    let midair = Vec3::new(10.5, (h + 20) as f32, 10.5);
    assert_eq!(w.free_position(midair), midair);

    // A swimmer stays floating where they saved: build a water shaft
    // and confirm no teleport to its floor.
    for y in (h + 1)..=(h + 4) {
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            w.set_block(15 + dx, y, 15 + dz, stone);
        }
    }
    for y in (h + 1)..=(h + 4) {
        w.set_block(15, y, 15, reg.water_block(0));
    }
    let swimming = Vec3::new(15.5, (h + 3) as f32 + 0.5, 15.5);
    assert_eq!(w.free_position(swimming), swimming, "swimmers float on");
}

#[test]
fn lava_conserves_creeps_and_quenches() {
    let reg = base_reg();
    let mut w = test_world_with("lava", reg.clone());
    let stone = b(&reg, "base:stone");
    let h = w.surface_height(4, 4);
    let y = h + 8;
    // A sealed 3x1 trench holds its lava: conservation + stiffer
    // hysteresis (a full cell spreads, films don't crawl).
    for x in 2..=8 {
        for z in 3..=5 {
            for yy in (y - 1)..=y {
                w.set_block(x, yy, z, stone);
            }
        }
    }
    for x in 4..=6 {
        w.set_block(x, y, 4, AIR);
    }
    w.set_block(4, y, 4, reg.lava_for_volume(8));
    for _ in 0..200 {
        if !w.tick_lava(10_000) {
            break;
        }
    }
    let total: u32 = (4..=6)
        .map(|x| reg.lava_volume(w.get_block(x, y, 4)).unwrap_or(0) as u32)
        .sum();
    assert_eq!(total, 8, "lava volume conserved in the trench");

    // Full lava + water neighbor -> obsidian, water boiled away.
    let (ox, oz) = (12, 12);
    w.set_block(ox, y - 1, oz, stone);
    w.set_block(ox + 1, y - 1, oz, stone);
    for (dx, dz) in [(-1, 0), (0, 1), (0, -1)] {
        w.set_block(ox + dx, y, oz + dz, stone);
        w.set_block(ox + 1 - dx.min(0) * 2, y, oz + dz, stone);
    }
    w.set_block(ox + 2, y, oz, stone);
    w.set_block(ox, y, oz, reg.lava_for_volume(8));
    w.set_block(ox + 1, y, oz, reg.water_block(0));
    for _ in 0..50 {
        w.tick_lava(1_000);
        w.tick_water(1_000);
    }
    assert_eq!(
        w.get_block(ox, y, oz),
        b(&reg, "base:obsidian"),
        "full lava quenched to obsidian"
    );
    assert_eq!(w.get_block(ox + 1, y, oz), AIR, "the water boiled away");

    // Partial lava + water -> basalt.
    let (px, pz) = (2, 12);
    w.set_block(px, y - 1, pz, stone);
    w.set_block(px + 1, y - 1, pz, stone);
    w.set_block(px - 1, y, pz, stone);
    w.set_block(px + 2, y, pz, stone);
    w.set_block(px, y, pz - 1, stone);
    w.set_block(px, y, pz + 1, stone);
    w.set_block(px + 1, y, pz - 1, stone);
    w.set_block(px + 1, y, pz + 1, stone);
    w.set_block(px, y, pz, reg.lava_for_volume(3));
    w.set_block(px + 1, y, pz, reg.water_block(0));
    for _ in 0..50 {
        w.tick_lava(1_000);
        w.tick_water(1_000);
    }
    assert_eq!(
        w.get_block(px, y, pz),
        b(&reg, "base:basalt"),
        "partial lava quenched to basalt"
    );
}

#[test]
fn magma_pools_the_deep_chambers() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("magma"), reg.clone());
    let mut lava_cells = 0;
    // Deep magma is a sparse planetary field. Sample the same finite
    // hydrology/geology neighborhoods used by the generator census instead
    // of assuming the old flat origin happens to cut through a chamber.
    for (center, _) in find_water_features(&w.generator, 4) {
        let chunk = crate::planet::ChunkPos::from_surface(center);
        for du in -2..=2 {
            for dv in -2..=2 {
                let sample = chunk.offset(du, dv);
                w.ensure_chunk(sample);
                for lx in 0..crate::chunk::CHUNK_X as u16 {
                    for lz in 0..crate::chunk::CHUNK_Z as u16 {
                        let surface = crate::planet::SurfacePos::new(
                            sample.face(),
                            sample.u() * crate::chunk::CHUNK_X as u16 + lx,
                            sample.v() * crate::chunk::CHUNK_Z as u16 + lz,
                        )
                        .unwrap();
                        lava_cells += (1..12)
                            .filter(|&y| reg.is_lava(block_at(&w, surface, y)))
                            .count();
                    }
                }
            }
        }
        if lava_cells > 20 {
            break;
        }
    }
    assert!(
        lava_cells > 20,
        "deep cheese chambers pool magma ({lava_cells})"
    );
}

#[test]
fn stale_saved_water_wakes_on_load() {
    // Water saved in an unstable pose (a full cube whose sides face
    // air — what pre-seal worldgen left behind) must resume settling
    // when its chunk returns from disk, not hang frozen forever.
    let reg = base_reg();
    let dir = tmp_dir("stalewake");
    let y = 180;
    {
        let mut w = World::new(7, dir.clone(), reg.clone());
        w.ensure_chunk(tchunk(0, 0));
        // A 3x3 stone shelf holding one exposed full water cube.
        for x in 3..=5 {
            for z in 3..=5 {
                w.set_block(x, y, z, b(&reg, "base:stone"));
            }
        }
        w.set_block(4, y + 1, 4, reg.water_block(0));
        // Unload without ticking: the save captures it mid-flow, and
        // this world's pending queues die with it.
        w.unload_chunk(tchunk(0, 0));
    }
    let mut w = World::new(7, dir, reg.clone());
    w.ensure_chunk(tchunk(0, 0));
    let mut quiet = false;
    for _ in 0..200 {
        if !w.tick_water(10_000) {
            quiet = true;
            break;
        }
    }
    assert!(quiet, "reloaded water settles");
    assert!(
        reg.water_volume(w.get_block(4, y + 1, 4)).unwrap_or(0) < 8,
        "the stranded cube spread instead of hanging as a square"
    );
}

#[test]
fn breached_pool_pours_over_the_edge() {
    // Break a dam and the pool should empty through the gap: the drop
    // rule pushes even the last unit over an edge (the moved water
    // falls, so it can never slosh back), leaving at most a thin
    // glaze on cells with no path to a fall.
    let reg = base_reg();
    let mut w = test_world_with("breach", reg.clone());
    let stone = b(&reg, "base:stone");
    let y = 200;
    // 7x7 sky platform, 5x5 wall ring, 3x3 pool of full water.
    for x in 0..7 {
        for z in 0..7 {
            w.set_block(x, y, z, stone);
        }
    }
    for x in 1..6 {
        for z in 1..6 {
            if x == 1 || x == 5 || z == 1 || z == 5 {
                w.set_block(x, y + 1, z, stone);
            }
        }
    }
    for x in 2..5 {
        for z in 2..5 {
            w.set_block(x, y + 1, z, reg.water_block(0));
        }
    }
    while w.tick_water(10_000) {}
    w.set_block(3, y + 1, 1, AIR); // the breach
    let mut quiet = false;
    for _ in 0..2000 {
        if !w.tick_water(10_000) {
            quiet = true;
            break;
        }
    }
    assert!(quiet, "the breached pool settles");
    let total = |w: &World| -> u32 {
        let mut sum = 0u32;
        for x in -1..8 {
            for z in -1..8 {
                sum += reg.water_volume(w.get_block(x, y + 1, z)).unwrap_or(0) as u32;
            }
        }
        sum
    };
    // 72 units started; the pour takes the majority with it, leaving
    // only the shallow hysteresis gradient behind.
    let left = total(&w);
    assert!(
        left <= 36,
        "pool mostly drained through the breach ({left} left)"
    );
    // The apron ring borders the platform rim on every side: the drop
    // rule empties it completely — even the last unit goes over.
    for x in 0..7 {
        assert_eq!(
            reg.water_volume(w.get_block(x, y + 1, 0)),
            None,
            "apron cell ({x},0) drained dry"
        );
    }
    // Then the sun finishes the job: the residue is shallow and open,
    // so it draws down and dries through — the basin empties fully.
    w.day = crate::world::SEASON_DAYS; // summer
    let mut rng = 5u32;
    for _ in 0..40_000 {
        w.random_tick(&mut rng);
        w.tick_water(1_000);
    }
    assert_eq!(total(&w), 0, "the breached basin dries out completely");
}

#[test]
fn exposed_glaze_and_marsh_films_both_evaporate() {
    let reg = base_reg();
    let mut w = test_world_with("glaze", reg.clone());
    w.day = 2 * crate::world::SEASON_DAYS; // autumn: not summer, not winter
    let stone = b(&reg, "base:stone");
    let y = 200;
    // An open film sheet on a flat slab — a drained pool's residue...
    for x in 0..8 {
        for z in 0..8 {
            w.set_block(x, y, z, stone);
        }
    }
    for x in 2..6 {
        for z in 2..6 {
            w.set_block(x, y + 1, z, reg.water_for_volume(1));
        }
    }
    // ...and a solid-walled pocket holding a marsh film.
    for x in 10..13 {
        for z in 0..3 {
            w.set_block(x, y, z, stone);
            w.set_block(x, y + 1, z, stone);
        }
    }
    w.set_block(11, y + 1, 1, reg.water_for_volume(1));
    let mut rng = 7u32;
    for _ in 0..30_000 {
        w.random_tick(&mut rng);
    }
    let sheet: u32 = (2..6)
        .flat_map(|x| (2..6).map(move |z| (x, z)))
        .map(|(x, z)| reg.water_volume(w.get_block(x, y + 1, z)).unwrap_or(0) as u32)
        .sum();
    assert_eq!(sheet, 0, "the open glaze dries away");
    assert_eq!(
        reg.water_volume(w.get_block(11, y + 1, 1)),
        None,
        "walls do not grant a magical exemption from evaporation"
    );
    // And rain can start a pond from nothing in a walled pocket: dry
    // the pocket by hand, then let a shower find it.
    w.set_block(11, y + 1, 1, AIR);
    w.force_local_weather("rain");
    w.rain_fill(11, 1);
    assert_eq!(
        reg.water_volume(w.get_block(11, y + 1, 1)),
        Some(1),
        "rain seeds a film in a dry pothole"
    );
}

#[test]
fn pools_level_through_a_submerged_gap() {
    // Two wells share a wall; the breach sits at the bottom layer,
    // below both surfaces once the low side backs up. Equalization
    // alone stalls there (every layer at the gap is full) — pressure
    // has to carry the difference, and the surfaces must meet.
    let reg = base_reg();
    let mut w = test_world_with("utube", reg.clone());
    let stone = b(&reg, "base:stone");
    let y = 200;
    // Solid 7x4 block from y..y+5, wells carved at x 1..=2 and 4..=5.
    for x in 0..7 {
        for z in 0..4 {
            for yy in y..=y + 5 {
                w.set_block(x, yy, z, stone);
            }
        }
    }
    for z in 1..=2 {
        for x in [1, 2, 4, 5] {
            for yy in y + 1..=y + 5 {
                w.set_block(x, yy, z, AIR);
            }
        }
    }
    // A holds four blocks of water, B one.
    for z in 1..=2 {
        for x in [1, 2] {
            for yy in y + 1..=y + 4 {
                w.set_block(x, yy, z, reg.water_block(0));
            }
        }
        for x in [4, 5] {
            w.set_block(x, y + 1, z, reg.water_block(0));
        }
    }
    while w.tick_water(10_000) {}
    // Knock one wall block out of the bottom layer. The wall above
    // the hole stays: this link is a sealed-top pipe mouth.
    w.set_block(3, y + 1, 1, AIR);
    let mut quiet = false;
    for _ in 0..4000 {
        if !w.tick_water(10_000) {
            quiet = true;
            break;
        }
    }
    assert!(quiet, "the linked pools settle");
    // Column heads across both wells and the gap must agree within
    // the 2-unit hysteresis: the surfaces have met.
    let head = |w: &World, x: i32, z: i32| -> i64 {
        let mut h = 0;
        for yy in y + 1..=y + 5 {
            if let Some(v) = reg.water_volume(w.get_block(x, yy, z)) {
                h = yy as i64 * 8 + v as i64;
            }
        }
        h
    };
    let cols: Vec<i64> = [
        (1, 1),
        (2, 1),
        (1, 2),
        (2, 2),
        (4, 1),
        (5, 1),
        (4, 2),
        (5, 2),
    ]
    .iter()
    .map(|&(x, z)| head(&w, x, z))
    .collect();
    let (lo, hi) = (*cols.iter().min().unwrap(), *cols.iter().max().unwrap());
    assert!(hi > 0, "water survived");
    assert!(hi - lo <= 2, "pressure levels the pools (heads {lo}..{hi})");
    // And the low side genuinely rose: from one block to over two.
    assert!(
        head(&w, 5, 2) >= (y as i64 + 2) * 8,
        "the far well rose ({})",
        head(&w, 5, 2)
    );
}

#[test]
fn saved_mid_drain_pools_resume_leveling() {
    // Quit the game mid-drain and the queues die with the session;
    // the load sweep must notice the head cliff at the gap and set
    // the pools leveling again.
    let reg = base_reg();
    let dir = tmp_dir("utube-resume");
    let y = 200;
    {
        let mut w = World::new(7, dir.clone(), reg.clone());
        w.ensure_chunk(tchunk(0, 0));
        let stone = b(&reg, "base:stone");
        for x in 0..7 {
            for z in 0..4 {
                for yy in y..=y + 5 {
                    w.set_block(x, yy, z, stone);
                }
            }
        }
        for z in 1..=2 {
            for x in [1, 2, 4, 5] {
                for yy in y + 1..=y + 5 {
                    w.set_block(x, yy, z, AIR);
                }
            }
        }
        for z in 1..=2 {
            for x in [1, 2] {
                for yy in y + 1..=y + 4 {
                    w.set_block(x, yy, z, reg.water_block(0));
                }
            }
            for x in [4, 5] {
                w.set_block(x, y + 1, z, reg.water_block(0));
            }
        }
        while w.tick_water(10_000) {}
        w.set_block(3, y + 1, 1, AIR);
        w.tick_water(50); // a few strokes of the pour, then quit
        w.unload_chunk(tchunk(0, 0));
    }
    let mut w = World::new(7, dir, reg.clone());
    w.ensure_chunk(tchunk(0, 0));
    let mut quiet = false;
    for _ in 0..4000 {
        if !w.tick_water(10_000) {
            quiet = true;
            break;
        }
    }
    assert!(quiet, "the reloaded pools settle");
    let head = |x: i32, z: i32| -> i64 {
        let mut h = 0;
        for yy in y + 1..=y + 5 {
            if let Some(v) = reg.water_volume(w.get_block(x, yy, z)) {
                h = yy as i64 * 8 + v as i64;
            }
        }
        h
    };
    let (a, b2) = (head(1, 1), head(5, 2));
    assert!(
        (a - b2).abs() <= 2,
        "reload resumes the leveling (heads {a} vs {b2})"
    );
}

#[test]
fn the_land_remembers_where() {
    let reg = base_reg();
    let dir = tmp_dir("rire");
    {
        let mut w = World::new(42, dir.clone(), reg.clone());
        // A clearcut in one valley, a garden in another.
        for _ in 0..40 {
            w.add_ire_at(100, 100, 1.0);
        }
        for _ in 0..40 {
            w.plant_ire_at(3000, 3000, 1.0);
        }
        assert_eq!(w.regional_ire_at(100, 100), 20.0, "grudge clamps at 20");
        assert_eq!(w.regional_ire_at(3000, 3000), -20.0, "grace clamps at -20");
        assert_eq!(w.regional_ire_at(100, 3000), 0.0, "elsewhere is neutral");
        // The same world mood feels different on different ground.
        w.ire = 30.0; // globally tier 1
        assert_eq!(w.ire_tier(), 1);
        assert_eq!(w.ire_tier_at(100, 100), 3, "the angry forest hunts");
        assert_eq!(w.ire_tier_at(3000, 3000), 0, "the tended valley forgives");
        // Grudges fade: a full day decays 2 points.
        w.tick_ire(1.0);
        assert!(
            (w.regional_ire_at(100, 100) - 18.0).abs() < 0.01,
            "decay toward zero ({})",
            w.regional_ire_at(100, 100)
        );
        save_world(&mut w);
    }
    let w = World::load_or_create(dir, reg).unwrap();
    assert!(
        w.regional_ire_at(100, 100) > 17.0,
        "the ledger persists ({})",
        w.regional_ire_at(100, 100)
    );
}

#[test]
fn the_stone_states_its_season_and_doubles_it() {
    use crate::world::{BlockEntity, OfferingState, SEASON_DAYS};
    let reg = base_reg();
    let mut w = test_world_with("wants", reg.clone());
    w.day = 3 * SEASON_DAYS; // winter: the wild hungers
    let (want, line) = w.season_want();
    assert_eq!(want, 3);
    assert!(line.contains("Food"), "the stone speaks plainly: {line}");
    let bread = it(&reg, "base:bread");
    let stone_ore = it(&reg, "base:raw_copper");
    assert!(
        w.satisfies_want(want, &ItemStack::new(&reg, bread, 1)),
        "bread feeds"
    );
    assert!(
        !w.satisfies_want(want, &ItemStack::new(&reg, stone_ore, 1)),
        "ore does not"
    );
    // A wanted offering credits double, and the stone's own valley
    // remembers the kindness.
    let sy = w.surface_height(4, 4);
    let mut o = OfferingState::default();
    o.slots[0] = Some(ItemStack::new(&reg, bread, 2));
    w.insert_block_entity((4, sy + 1, 4), BlockEntity::Offering(o));
    w.ire = 50.0;
    let refund = w.accept_offerings();
    // bread: hunger 6 -> 1.5 value each, doubled = 3.0 x2 loaves = 6.
    assert!(
        (refund - 6.0).abs() < 0.01,
        "winter bread counts double ({refund})"
    );
    assert!(
        w.regional_ire_at(4, 4) <= -5.9,
        "the valley remembers ({})",
        w.regional_ire_at(4, 4)
    );
    // Out of season the same loaves count single.
    w.day = SEASON_DAYS; // summer wants water, not bread
    let mut o2 = OfferingState::default();
    o2.slots[0] = Some(ItemStack::new(&reg, bread, 2));
    w.insert_block_entity((4, sy + 1, 4), BlockEntity::Offering(o2));
    let refund2 = w.accept_offerings();
    assert!(
        (refund2 - 3.0).abs() < 0.01,
        "unwanted still counts, singly ({refund2})"
    );
}

#[test]
fn the_green_tide_seeds_only_natural_kind_ground() {
    use crate::world::SEASON_DAYS;
    let reg = base_reg();
    let mut w = test_world_with("greentide", reg.clone());
    w.day = SEASON_DAYS; // summer: growth season
    // Force chunk (0,0) UNMODIFIED after our stage-setting: we build
    // via raw grass/log placement then clear the flag through save.
    let grass = b(&reg, "base:grass");
    let log = b(&reg, "base:log");
    let sy = 140;
    for x in 0..16 {
        for z in 0..16 {
            w.set_block(x, sy, z, grass);
        }
    }
    for dy in 1..=4 {
        w.set_block(8, sy + dy, 8, log);
    }
    // Blessed country; set_block is the sim's hand, not a player's,
    // so the chunk stays natural in the green tide's eyes.
    for _ in 0..10 {
        w.plant_ire_at(8, 8, 1.0);
    }
    let mut rng = 11u32;
    for _ in 0..6000 {
        w.random_tick(&mut rng);
    }
    let mut saplings = 0;
    for x in 0..16 {
        for z in 0..16 {
            if reg.block(w.get_block(x, sy + 1, z)).sapling.is_some() {
                saplings += 1;
            }
        }
    }
    assert!(saplings >= 1, "the forest thickens ({saplings})");
    assert!(saplings <= 8, "but never marches ({saplings})");
    // The same ground, resented: nothing seeds.
    let mut w2 = test_world_with("greentide2", reg.clone());
    w2.day = SEASON_DAYS;
    for x in 0..16 {
        for z in 0..16 {
            w2.set_block(x, sy, z, grass);
        }
    }
    for dy in 1..=4 {
        w2.set_block(8, sy + dy, 8, log);
    }
    for _ in 0..10 {
        w2.add_ire_at(8, 8, 1.0);
    }
    let mut rng2 = 11u32;
    for i in 0..3000 {
        // A valley full of fruiting bushes credits its own cell, and
        // the natural country around this pad is thick with them —
        // keep the grievance fresh, which is what the test is about.
        if i % 100 == 0 {
            w2.add_ire_at(8, 8, 5.0);
        }
        w2.random_tick(&mut rng2);
    }
    let mut saplings2 = 0;
    for x in 0..16 {
        for z in 0..16 {
            if reg.block(w2.get_block(x, sy + 1, z)).sapling.is_some() {
                saplings2 += 1;
            }
        }
    }
    assert_eq!(saplings2, 0, "angry or touched ground stays bare");
}

#[test]
fn blocks_place_into_water_and_never_vanish_on_refusal() {
    let reg = base_reg();
    let mut w = test_world_with("place-water", reg.clone());
    let h = w.surface_height(4, 4);
    let stone = b(&reg, "base:stone");
    let water = reg.water_block(0);
    // A block takes a water cell — the water is displaced, not the
    // player's hand. (The bug: place_block refused any non-AIR cell,
    // so the click spent the item and put nothing down.)
    w.set_block(4, h + 1, 4, water);
    assert!(w.place_block((4, h + 1, 4), stone), "stone displaces water");
    assert_eq!(w.get_block(4, h + 1, 4), stone);
    // Thin layers give way the same, and air of course.
    if let Some(layer) = reg.block_id("base:snow_layer") {
        w.set_block(5, h + 1, 5, layer);
        assert!(w.place_block((5, h + 1, 5), stone), "stone over a drift");
    }
    w.set_block(6, h + 1, 6, AIR);
    assert!(w.place_block((6, h + 1, 6), stone));
    // What stands does NOT give way: a placement must never quietly
    // eat a crop, and refusal must be honest so callers keep the item.
    let crop = b(&reg, "base:wheat_seeds");
    w.set_block(7, h + 1, 7, crop);
    assert!(!w.place_block((7, h + 1, 7), stone), "the wheat stands");
    assert_eq!(w.get_block(7, h + 1, 7), crop);
    assert!(!w.place_block((8, h + 1, 8), stone) || w.get_block(8, h + 1, 8) == stone);
    // And the registry's own account of what gives way.
    assert!(reg.is_replaceable(AIR));
    assert!(reg.is_replaceable(water));
    assert!(!reg.is_replaceable(stone));
    assert!(!reg.is_replaceable(crop));
}

#[test]
fn old_worlds_keep_their_partial_sand_as_ordinary_sand() {
    // Partial (sub-voxel) sand is gone. A world saved when it existed
    // must not come back full of unknown blocks — the alias converts
    // it to ordinary sand, keeping the volume the player had.
    let reg = base_reg();
    let sand = reg.block_id("base:sand").expect("sand exists");
    assert_eq!(
        reg.block_id("base:surface_sand"),
        Some(sand),
        "an old save's surface sand resolves to plain sand"
    );
    // And nothing claims the octant geometry any more.
    assert!(
        reg.blocks.iter().all(|b| b.name != "base:surface_sand"),
        "the block itself is gone from the registry"
    );
    // The metadata byte survives it: soil still carries fertility.
    let mut w = test_world_with("post-sand-meta", reg.clone());
    let farm = b(&reg, "base:farmland");
    let h = w.surface_height(4, 4);
    w.set_block_meta(4, h, 4, farm, crate::world::soil::soil_meta(31, 2));
    assert_eq!(w.fertility_at(4, h, 4), 31, "the meta plane still works");
}

#[test]
fn nobody_spawns_in_the_water_the_sky_or_a_wall() {
    let reg = base_reg();
    let mut w = test_world_with("spawn-safe", reg.clone());
    let stone = b(&reg, "base:stone");
    let water = reg.water_block(0);
    let check = |w: &World, p: Vec3| {
        let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        assert!(
            reg.is_solid(w.get_block(x, y - 1, z)),
            "solid ground underfoot at {p:?}"
        );
        for dy in 0..2 {
            let b = w.get_block(x, y + dy, z);
            assert!(!reg.is_solid(b), "body not inside a wall at {p:?}");
            assert!(!reg.is_fluid(b), "body not in fluid at {p:?}");
        }
        assert!(y > SEA_LEVEL, "above the tideline at {p:?}");
    };
    // Ordinary ground: taken as-is.
    let p = w.safe_spawn(4, 4);
    check(&w, p);
    // A drowned column: the search walks ashore instead of standing
    // the player on the seabed. (This is the bug — fluid is not solid,
    // so a seabed column used to read as somewhere to stand.)
    for x in 0..10 {
        for z in 0..10 {
            for y in (SEA_LEVEL - 6)..=SEA_LEVEL {
                w.set_block(x, y, z, water);
            }
            for y in SEA_LEVEL + 1..SEA_LEVEL + 5 {
                w.set_block(x, y, z, AIR);
            }
            w.set_block(x, SEA_LEVEL - 7, z, stone);
        }
    }
    let p = w.safe_spawn(5, 5);
    check(&w, p);
}

#[test]
fn open_ocean_gets_an_island_rather_than_a_drowning() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("spawn-isle"), reg.clone());
    let water = reg.water_block(0);
    let stone = b(&reg, "base:stone");
    let center = 'search: {
        for face in crate::planet::Face::ALL {
            for cu in (1..crate::planet::FACE_CHUNKS - 1).step_by(11) {
                for cv in (1..crate::planet::FACE_CHUNKS - 1).step_by(11) {
                    let pos = crate::planet::SurfacePos::new(
                        face,
                        cu * crate::chunk::CHUNK_X as u16 + 8,
                        cv * crate::chunk::CHUNK_Z as u16 + 8,
                    )
                    .unwrap();
                    if w.generator.surface_estimate_at(pos) >= SEA_LEVEL - 4 {
                        continue;
                    }
                    ensure_surface_neighborhood(&mut w, pos, 1);
                    if w.is_open_water_at(pos) {
                        break 'search pos;
                    }
                }
            }
        }
        panic!("seed 42 has no sampled deep ocean");
    };

    // A small patch of open sea: seabed just down, water to the
    // tideline. Kept tight on purpose — every water cell set here
    // wakes the fluid sim, and a big test sea starves the whole
    // parallel suite.
    for du in -9..=9 {
        for dv in -9..=9 {
            let surface = surface_offset(center, du, dv);
            for y in (SEA_LEVEL - 2)..=(SEA_LEVEL + 4) {
                w.set_block_at(
                    block_pos(surface, y),
                    if y <= SEA_LEVEL { water } else { AIR },
                );
            }
            w.set_block_at(block_pos(surface, SEA_LEVEL - 3), stone);
        }
    }
    let crest = w.raise_castaway_isle_at(center);
    assert!(crest > SEA_LEVEL, "landfall rises out of the water");
    // Sand, not a plinth of whatever was underneath.
    assert_eq!(
        block_at(&w, center, crest),
        b(&reg, "base:sand"),
        "a little sand island"
    );
    // Dry overhead, so a castaway is actually standing in air.
    for dy in 1..=2 {
        assert!(
            !reg.is_fluid(block_at(&w, center, crest + dy)),
            "the island is dry at +{dy}"
        );
    }
    // It shelves back into the sea rather than dropping off a tower.
    let rim = w.surface_height_at(surface_offset(center, 4, 0));
    assert!(
        rim < crest && rim >= SEA_LEVEL - 1,
        "the rim shelves ({rim} vs crest {crest})"
    );
    // And it is an island, not a continent: nothing was raised
    // beyond its shore.
    let sand = b(&reg, "base:sand");
    let beyond = surface_offset(center, 8, 0);
    assert!(
        (SEA_LEVEL - 2..=SEA_LEVEL + 3).all(|y| block_at(&w, beyond, y) != sand),
        "no landfill beyond the island's shore"
    );
}

#[test]
#[ignore = "dev tool: times the spawn search"]
fn dev_time_safe_spawn() {
    let reg = base_reg();
    let mut w = test_world_with("spawn-timing", reg.clone());
    let t = std::time::Instant::now();
    let p = w.safe_spawn(0, 0);
    eprintln!("safe_spawn(0,0) = {p:?} in {:?}", t.elapsed());
    eprintln!("surface at 0,0 = {}", w.surface_height(0, 0));
    eprintln!("estimate at 0,0 = {}", w.generator.surface_estimate(0, 0));
}

/// The 20-second autosave used to rewrite every chunk the player had
/// ever loaded, forever — the `modified` flag was set on load and never
/// cleared, so an hour of walking turned each autosave into a few
/// hundred synchronous file writes on the main thread. That is the
/// periodic hitch, and this is the shape of the fix: a write clears the
/// flag, and only a real edit sets it again.
#[test]
fn the_autosave_writes_only_what_changed_since_the_last_one() {
    let reg = base_reg();
    let dir = tmp_dir("autosave-churn");
    let mut w = World::new(9, dir.clone(), reg.clone());
    w.ensure_chunk(tchunk(0, 0));
    let stone = b(&reg, "base:stone");
    let top = w.surface_height(3, 3);
    w.set_block(3, top + 1, 3, stone);
    save_world(&mut w);
    let file = crate::world::region::region_path(&dir, tchunk(0, 0));
    assert!(file.exists(), "the edited chunk is written");

    // Deleting the file is the probe: if the next autosave puts it
    // back, the chunk was queued for writing with nothing to write.
    std::fs::remove_file(&file).unwrap();
    save_world(&mut w);
    assert!(!file.exists(), "an unchanged chunk is not rewritten");

    // ...and one more edit puts it straight back in the queue.
    w.set_block(4, top + 1, 4, stone);
    save_world(&mut w);
    assert!(file.exists(), "an edited chunk saves again");
}

#[test]
fn save_reports_every_failed_component_and_retries_dirty_chunks() {
    let reg = base_reg();
    let root = tmp_dir("save-report");
    let blocked = root.join("not-a-directory");
    std::fs::write(&blocked, b"occupied").unwrap();
    let mut w = World::new(9, blocked.clone(), reg.clone());
    let pos = tchunk(0, 0);
    w.ensure_chunk(pos);
    let stone = b(&reg, "base:stone");
    let top = w.surface_height(3, 3);
    w.set_block(3, top + 1, 3, stone);

    let failed = w.save_modified();
    let components: std::collections::HashSet<&str> = failed
        .failures
        .iter()
        .map(|failure| failure.component.as_str())
        .collect();
    for component in [
        "save directory",
        "world metadata",
        "block palette",
        "block entities",
        "animals",
        "regional ire",
        "bloom ledger",
        "long winter",
        "bloom exhaustion",
        "hearts",
        "animal seed marks",
        "player-touched marks",
        "random-tick stamps",
    ] {
        assert!(
            components.contains(component),
            "{component} failure is visible: {}",
            failed.summary()
        );
    }
    assert_eq!(
        failed.chunks_saved, 0,
        "a chunk cannot precede its failed palette"
    );

    std::fs::remove_file(&blocked).unwrap();
    std::fs::create_dir_all(&blocked).unwrap();
    let retry = w.save_modified();
    assert!(retry.is_ok(), "retry succeeds: {}", retry.summary());
    assert_eq!(retry.chunks_saved, 1, "the dirty chunk was retained");

    w.set_block(4, top + 1, 4, stone);
    w.fail_chunk_save_for_test(pos, true);
    let chunk_failure = w.save_modified();
    assert_eq!(chunk_failure.chunks_saved, 0);
    assert!(
        chunk_failure
            .failures
            .iter()
            .any(|failure| failure.component == format!("chunk {pos:?}")),
        "chunk failure has coordinates: {}",
        chunk_failure.summary()
    );
    w.fail_chunk_save_for_test(pos, false);
    let chunk_retry = w.save_modified();
    assert!(chunk_retry.is_ok(), "chunk retry succeeds");
    assert_eq!(chunk_retry.chunks_saved, 1, "failed chunk stayed dirty");
}

/// A chunk that came off disk already matches its file. Reloading a
/// world and walking around must not re-dirty everything it touches.
#[test]
fn a_chunk_read_from_disk_is_clean_until_something_edits_it() {
    let reg = base_reg();
    let dir = tmp_dir("autosave-reload");
    let stone = b(&reg, "base:stone");
    let top = {
        let mut w = World::new(9, dir.clone(), reg.clone());
        w.ensure_chunk(tchunk(0, 0));
        let top = w.surface_height(3, 3);
        w.set_block(3, top + 1, 3, stone);
        save_world(&mut w);
        top
    };
    let file = crate::world::region::region_path(&dir, tchunk(0, 0));
    assert!(file.exists());

    let mut w = World::load_or_create(dir.clone(), reg.clone()).unwrap();
    w.ensure_chunk(tchunk(0, 0));
    assert_eq!(w.get_block(3, top + 1, 3), stone, "the edit came back");
    std::fs::remove_file(&file).unwrap();
    save_world(&mut w);
    assert!(
        !file.exists(),
        "a freshly loaded, untouched chunk is not rewritten"
    );
}

/// The palette describes the registry, not the world, so writing it on
/// the autosave timer was 4 KB of churn every twenty seconds saying the
/// same thing. It still has to be written when it would differ — a
/// fresh world, or one whose block registry changed since last time.
#[test]
fn the_palette_is_written_when_it_would_differ_and_not_on_a_timer() {
    let reg = base_reg();
    let dir = tmp_dir("palette-churn");
    let palette = dir.join("palette");
    let mut w = World::new(9, dir.clone(), reg.clone());
    w.ensure_chunk(tchunk(0, 0));
    save_world(&mut w);
    assert!(palette.exists(), "a fresh world owes a palette");

    std::fs::remove_file(&palette).unwrap();
    save_world(&mut w);
    assert!(!palette.exists(), "an unchanged palette is not rewritten");

    // Reopening against a save whose palette is missing or stale counts
    // as a difference, so the next save puts one back — and every chunk
    // that loads is rewritten in the ids it names.
    let mut w = World::load_or_create(dir.clone(), reg.clone()).unwrap();
    w.ensure_chunk(tchunk(0, 0));
    save_world(&mut w);
    assert!(palette.exists(), "a stale palette is replaced");
}

/// The calendar runs on DAY_LENGTH; growth, spoilage and recovery run
/// on the wall clock. Those are two clocks, and every one of these
/// pairs silently changes what it MEANS if only one of them is turned.
/// A crop that took four days to ripen taking two, a larder that fed
/// you through winter running out in autumn — neither shows up as a
/// failure anywhere, which is exactly why they are pinned here.
#[test]
fn the_calendar_and_the_wall_clock_stay_in_step() {
    use crate::server::DAY_LENGTH;
    use crate::world::{FRESHNESS_PER_SEC, RANDOM_TICKS_PER_CHUNK_SEC};
    let reg = base_reg();

    // A chunk gets the same random-tick visits per in-game day at any
    // day length: this is what makes a crop take N days rather than N
    // minutes. Growth, thaw, fungus and grass regrowth all ride it.
    let visits_per_day = RANDOM_TICKS_PER_CHUNK_SEC * DAY_LENGTH as f64;
    assert!(
        (visits_per_day - 9600.0).abs() < 1.0,
        "a chunk should see ~9600 random ticks a day, sees {visits_per_day:.0}"
    );

    // Food is authored in freshness points and spent on the wall
    // clock, so its shelf life in DAYS is the product of three
    // numbers. "Will this last the winter" has to keep its answer.
    for (item, want_days) in [("base:potato", 3.0f32), ("base:raw_venison", 1.5)] {
        let id = reg.item_id(item).unwrap_or_else(|| panic!("no {item}"));
        let days = reg.item(id).durability as f32 / FRESHNESS_PER_SEC / DAY_LENGTH;
        assert!(
            (days - want_days).abs() < 0.05,
            "{item} should keep {want_days} in-game days, keeps {days:.2}"
        );
    }

    // And a season is a season's worth of days, not a hardcoded 12.
    assert_eq!(crate::world::ROOT_DAYS, crate::world::SEASON_DAYS as f32);
    assert_eq!(
        crate::world::HEART_CUTTING_DAYS,
        crate::world::SEASON_DAYS as f32 / 2.0
    );
    assert_eq!(
        crate::world::BLOOM_EXHAUSTION,
        crate::world::SEASON_DAYS as f32
    );
}

/// Stamps mean nothing without the clock they were written against.
#[test]
fn stamps_from_a_different_day_length_are_dropped_not_misread() {
    let reg = base_reg();
    let dir = tmp_dir("stamps-version");
    {
        let mut w = World::new(9, dir.clone(), reg.clone());
        w.ensure_chunk(tchunk(0, 0));
        let mut rng = 1u32;
        w.random_tick(&mut rng);
        save_world(&mut w);
    }
    let w = World::load_or_create(dir.clone(), reg.clone()).unwrap();
    assert!(w.stamp_count() > 0, "same clock, stamps come back");

    // A pre-retune file: the old headerless (x, z, time) triples.
    let mut old = Vec::new();
    old.extend_from_slice(&0i32.to_le_bytes());
    old.extend_from_slice(&0i32.to_le_bytes());
    old.extend_from_slice(&123.0f64.to_le_bytes());
    std::fs::write(dir.join("stamps"), &old).unwrap();
    let w = World::load_or_create(dir.clone(), reg.clone()).unwrap();
    assert_eq!(
        w.stamp_count(),
        0,
        "a stamp on another clock is discarded, not read as days of absence"
    );
}

/// A lava flow down a slope has to read as one ribbon, not a row of
/// islands. It used to pour its whole volume over each edge and leave
/// air behind, so what you got was a single cell perched on each step
/// with bare rock between them — a staircase of disconnected blobs.
/// A viscous fluid coats what it runs over: every step the flow
/// crosses stays covered, and diagonal cells share a corner, which is
/// what the mesher's surface smoothing needs to join them into one
/// surface.
#[test]
fn a_lava_flow_coats_the_slope_it_runs_down() {
    let reg = base_reg();
    let mut w = test_world_with("lava-ribbon", reg.clone());
    let stone = b(&reg, "base:stone");
    // Keep the hand-built experiment in guaranteed open shell-space. At the
    // old y=80 the new jungle terrain could occupy the channel and make this
    // a test of the spawn country's canopy instead of lava viscosity.
    let y0 = 220;
    const STEPS: i32 = 10;
    // A staircase descending in +x, two cells deep per tread.
    for step in 0..STEPS {
        let top = y0 - step;
        for x in (step * 2)..(step * 2 + 2) {
            for z in -2..=2 {
                for fill in 0..10 {
                    w.set_block(x, top - fill, z, stone);
                }
                if z.abs() == 2 {
                    w.set_block(x, top + 1, z, stone);
                }
            }
        }
    }
    // A crater's worth behind it: reach is a question of volume.
    for z in -1..=1 {
        for x in 0..2 {
            for up in 1..=3 {
                w.set_block(x, y0 + up, z, reg.lava_for_volume(8));
            }
        }
    }
    for _ in 0..300 {
        w.tick_lava(256);
    }

    // Which treads the flow touched, and how much of each.
    let coated = |step: i32| -> usize {
        let top = y0 - step;
        ((step * 2)..(step * 2 + 2))
            .filter(|&x| reg.is_lava(w.get_block(x, top + 1, -1)))
            .count()
    };
    let reached: Vec<i32> = (0..STEPS).filter(|&s| coated(s) > 0).collect();
    let front = *reached.last().expect("the flow left the crest");
    assert!(
        front >= 5,
        "a crater's worth should run several treads down, got {front}"
    );
    // No bare tread between the vent and the front: that gap IS the
    // jankiness, and it is what the trail exists to close.
    for step in 0..=front {
        assert!(
            coated(step) > 0,
            "tread {step} is bare between the vent and the front at {front}"
        );
    }
    // And each tread is covered across, not perched on its lip — that
    // is what puts a shared corner under every diagonal join.
    let full = (0..=front).filter(|&s| coated(s) == 2).count();
    assert!(
        full * 2 >= (front as usize + 1),
        "most treads should be covered across, {full} of {} are",
        front + 1
    );
}

/// A patch of forest floor: grass under a stand of leaves, with the
/// chunk left wild unless the caller says otherwise.
fn kindling(name: &str) -> (World, std::sync::Arc<Registry>, i32) {
    let reg = base_reg();
    let mut w = test_world_with(name, reg.clone());
    let grass = b(&reg, "base:grass");
    let leaves = b(&reg, "base:leaves");
    // One flat level for the whole patch, captured BEFORE anything is
    // built on it — leaves are solid, so surface_height stops meaning
    // "the ground" the moment a canopy goes up.
    let ground = w.surface_height(4, 4);
    let dirt = b(&reg, "base:dirt");
    for x in 0..8 {
        for z in 0..8 {
            // Flatten it. Natural terrain is not level, and a patch
            // built at one height across a slope leaves half the fuel
            // buried in rock and half hanging in the air.
            for y in (ground - 3)..ground {
                w.set_block(x, y, z, dirt);
            }
            for y in (ground + 1)..(ground + 6) {
                w.set_block(x, y, z, AIR);
            }
            w.set_block(x, ground, z, grass);
            w.set_block(x, ground + 2, z, leaves);
        }
    }
    // Wilderness: an edit marks a chunk touched, so undo that.
    w.player_touched.clear();
    (w, reg, ground)
}

fn burn(w: &mut World, rounds: usize) {
    let mut rng = 99u32;
    for _ in 0..rounds {
        w.tick_fire(512, &mut rng);
    }
}

/// The wild's fire renews what it takes, and refuses to set foot on
/// ground a player has worked. That invariant already governed
/// lightning; fire inherits it whole.
#[test]
fn the_wilds_fire_pays_bloom_and_stops_at_worked_ground() {
    let (mut w, reg, y) = kindling("fire-wild");
    assert!(w.light_fire(2, y + 1, 2, false), "lightning takes");
    burn(&mut w, 60);
    assert!(w.bloom_at(2, 2) > 0.0, "a natural burn leaves regrowth");
    assert_eq!(w.regional_ire_at(2, 2), 0.0, "and costs the player nothing");
    assert!(
        reg.block_id("base:charred_soil") == Some(w.get_block(2, y, 2)),
        "it chars the ground it cleared"
    );

    // Now claim the ground and try again: the wild will not light it.
    let (mut w, _, y) = kindling("fire-wild-stop");
    w.player_touched.insert(tchunk(0, 0));
    assert!(
        !w.light_fire(2, y + 1, 2, false),
        "the wild does not burn what you built on"
    );
}

/// Your fire in the wild is arson: ire for every wild thing it eats,
/// and no bloom at all. Burn it yourself and the ground gives you ash
/// rather than renewal — the exploit that closes.
#[test]
fn your_fire_in_the_wild_costs_ire_and_pays_no_bloom() {
    let (mut w, _reg, y) = kindling("fire-arson");
    assert!(w.light_fire(2, y + 1, 2, true), "a striker takes anywhere");
    burn(&mut w, 60);
    assert!(
        w.regional_ire_at(2, 2) > 0.0,
        "the country holds it against you"
    );
    assert_eq!(
        w.bloom_at(2, 2),
        0.0,
        "and gives back nothing: ash, not renewal"
    );
}

/// Burning your own field on your own ground is agriculture, not
/// arson, and the wild has no opinion about it.
#[test]
fn burning_your_own_crop_on_your_own_ground_is_husbandry() {
    let reg = base_reg();
    let mut w = test_world_with("fire-stubble", reg.clone());
    let farm = b(&reg, "base:farmland");
    let crop = reg
        .block_id("base:wheat_seeds")
        .expect("wheat is a crop block");
    assert!(reg.block(crop).burns > 0, "a field is easy to lose");
    for x in 0..6 {
        for z in 0..6 {
            let y = w.surface_height(x, z);
            w.set_block(x, y, z, farm);
            w.set_block(x, y + 1, z, crop);
        }
    }
    // Worked ground: in play, planting a field marks it. set_block is
    // the raw poke that does not, so say it outright.
    w.player_touched.insert(tchunk(0, 0));
    let y = w.surface_height(2, 2);
    assert!(w.light_fire(2, y + 2, 2, true));
    burn(&mut w, 60);
    assert_eq!(
        w.regional_ire_at(2, 2),
        0.0,
        "your own stubble is nobody's business"
    );
}

/// A fire remembers whose it is all the way down the hill. Without
/// that, an arsonist lights a fire on their own field and lets it walk
/// into the forest to collect the bloom.
#[test]
fn guilt_is_inherited_by_spread() {
    let (mut w, _reg, y) = kindling("fire-inherit");
    assert!(w.light_fire(0, y + 1, 0, true));
    burn(&mut w, 120);
    // It reached across the patch. Counted in canopy, not in charred
    // ground: fire climbs, and a crown fire eats the leaves while the
    // grass under it stays green.
    let leaves = w.reg.block_id("base:leaves");
    let left = (0..8)
        .flat_map(|x| (0..8).map(move |z| (x, z)))
        .filter(|&(x, z)| leaves == Some(w.get_block(x, y + 2, z)))
        .count();
    assert!(left < 60, "the fire ran ({left} of 64 leaf cells left)");
    // ...and every cell it reached is still on the arsonist's account.
    assert_eq!(w.bloom_at(6, 6), 0.0, "no bloom anywhere it went");
}

#[test]
fn fire_spreads_across_a_real_planet_face_seam() {
    use crate::planet::BlockPos;

    let reg = base_reg();
    let crop = b(&reg, "base:wheat_seeds");
    let burns = reg.block(crop).burns;

    for (index, seam) in directed_planet_seams().into_iter().enumerate() {
        let mut world = World::new(
            44,
            tmp_dir(&format!("planet-fire-seam-{index}")),
            reg.clone(),
        );
        let flame =
            BlockPos::new(seam.source.face(), seam.source.u(), 100, seam.source.v()).unwrap();
        let fuel =
            BlockPos::new(seam.across.face(), seam.across.u(), 100, seam.across.v()).unwrap();

        world.insert_empty_chunks_for_test([flame.chunk(), fuel.chunk()]);
        world.set_block_at(fuel, crop);
        assert!(world.light_fire_at(flame, true));

        // Pick a deterministic predecessor whose next fire roll catches.
        let mut rng = (0..10_000u32)
            .find(|candidate| {
                let next = candidate
                    .wrapping_mul(1_664_525)
                    .wrapping_add(1_013_904_223);
                (next >> 16) % 10 < u32::from(burns)
            })
            .unwrap();
        assert!(world.tick_fire(1, &mut rng));
        assert_eq!(
            reg.block(world.get_block_at(fuel)).name,
            "base:fire",
            "fire did not cross {:?} {:?}",
            seam.face,
            seam.direction
        );
    }
}

#[test]
fn regional_ledgers_are_face_aware_and_round_trip() {
    use crate::planet::{Face, SurfacePos};

    let reg = base_reg();
    let dir = tmp_dir("planet-ledgers");
    let pos_z = SurfacePos::new(Face::PosZ, 12, 34).unwrap();
    let pos_x = SurfacePos::new(Face::PosX, 12, 34).unwrap();
    let mut world = World::new(45, dir.clone(), reg.clone());
    world.add_ire_at_surface(pos_z, 3.0);
    world.add_ire_at_surface(pos_x, 7.0);
    world.add_bloom_at_surface(pos_z, 2.0);
    world.add_bloom_at_surface(pos_x, 5.0);
    assert_eq!(world.regional_ire_at_surface(pos_z), 3.0);
    assert_eq!(world.regional_ire_at_surface(pos_x), 7.0);
    assert_eq!(world.bloom_at_surface(pos_z), 2.0);
    assert_eq!(world.bloom_at_surface(pos_x), 5.0);

    let report = world.save_modified();
    assert!(report.is_ok(), "ledger save failed: {}", report.summary());
    drop(world);

    let loaded = World::load_or_create(dir, reg).unwrap();
    assert_eq!(loaded.regional_ire_at_surface(pos_z), 3.0);
    assert_eq!(loaded.regional_ire_at_surface(pos_x), 7.0);
    assert_eq!(loaded.bloom_at_surface(pos_z), 2.0);
    assert_eq!(loaded.bloom_at_surface(pos_x), 5.0);
}

#[test]
fn a_chunk_pays_only_for_what_it_actually_holds() {
    use crate::chunk::{CHUNK_CELLS, Chunk};

    // Every plane used to be allocated dense and unconditionally: 128 KB of
    // blocks, 64 KB of metadata, 192 KB of block light and 64 KB of sky light
    // for every chunk in memory, whether or not any of it said anything.
    const DENSE: usize = CHUNK_CELLS * (2 + 1 + 3 + 1);
    assert_eq!(DENSE, 458_752, "the old unconditional cost, 448 KiB");

    // A fresh chunk says nothing at all and costs nothing.
    let fresh = Chunk::new();
    assert_eq!(fresh.heap_bytes(), 0, "open air is free");

    // Real generated terrain, lit.
    let mut w = test_world("chunk-bytes");
    let pos = tchunk(0, 0);
    w.ensure_chunk(pos);
    let real = w.chunks()[&pos].heap_bytes();
    assert!(real > 0, "terrain costs something");
    assert!(
        real < DENSE,
        "a real chunk ({real} bytes) must cost less than the old flat {DENSE}"
    );
    // Blocks and sky light genuinely vary with terrain; block light and
    // metadata almost never do, and they were more than half the bill.
    assert!(
        real <= DENSE / 2,
        "a chunk with no torch and no block state should cost at most half \
         the old {DENSE} bytes, got {real}"
    );
}

#[test]
fn the_view_distance_slider_stops_where_the_memory_does() {
    use crate::config::{
        CHUNK_RESIDENT_BYTES, Config, MAX_VIEW_DIST, MIN_VIEW_DIST, max_view_dist_for_memory,
    };

    let cap = max_view_dist_for_memory();
    assert!(
        (MIN_VIEW_DIST..=MAX_VIEW_DIST).contains(&cap),
        "the cap stays inside the playable range, got {cap}"
    );

    // Whatever this machine allows, the loaded set at that distance has to be
    // a number of bytes it could plausibly hold. The slider used to offer 64
    // everywhere — over four gigabytes of resident chunks.
    let chunks = (2u64 * cap as u64 + 1).pow(2);
    let bytes = chunks * CHUNK_RESIDENT_BYTES;
    assert!(
        bytes < 64 * 1024 * 1024 * 1024,
        "a {cap}-chunk view wants {} GiB",
        bytes / (1024 * 1024 * 1024)
    );

    // A config file asking for more than the machine can hold is clamped on
    // the way in rather than honoured into an out-of-memory kill.
    let greedy = Config::from_text(&format!("view_dist={MAX_VIEW_DIST}\n"));
    assert!(
        greedy.view_dist <= cap,
        "config asked {} and got {}, past the {cap} cap",
        MAX_VIEW_DIST,
        greedy.view_dist
    );
    // And one below the floor comes up to it.
    let tiny = Config::from_text("view_dist=1\n");
    assert_eq!(tiny.view_dist, MIN_VIEW_DIST);
}

#[test]
fn the_lands_ledgers_do_not_grow_without_bound() {
    // regional_ire, bloom and blessed_streak are keyed per 256-block cell and
    // written every time anyone takes or tends anything. They are only
    // bounded because each one decays to nothing and drops its entry when it
    // gets there — behaviour nothing tested, in maps that are also persisted
    // and rewritten whole on every save.
    let mut w = test_world("ledger-bounds");

    // A thousand distinct regional cells distributed over the six finite
    // faces, all charged. The old arithmetic ran ever farther off PosZ and
    // was exactly the unbounded-world assumption this test now guards
    // against.
    for i in 0..1000 {
        let face = crate::planet::Face::ALL[i % crate::planet::Face::ALL.len()];
        let cell = i / crate::planet::Face::ALL.len();
        let u = ((cell % 32) * 256 + 128) as u16;
        let v = (((cell / 32) % 32) * 256 + 128) as u16;
        let pos = crate::planet::SurfacePos::new(face, u, v).unwrap();
        w.add_ire_at_surface(pos, 5.0);
        w.add_bloom_at_surface(pos, 2.0);
    }
    assert!(w.ledger_len() > 0, "charging the land records something");

    // A season passes with nobody touching any of it.
    for _ in 0..(crate::world::SEASON_DAYS * 4) {
        w.tick_ire(1.0);
    }
    assert_eq!(
        w.ledger_len(),
        0,
        "grudges, gratitude and blooms all fade to nothing and stop being \
         stored — a ledger that only ever grew would outlive the world"
    );
}

#[test]
fn a_world_survives_a_save_and_reload_across_several_regions() {
    // Chunks live 32x32 to a face-local region file. Sample explicit chunks
    // on both sides of several region boundaries and on all six faces, edit
    // each, save, drop the world, and read it all back.
    let reg = base_reg();
    let dir = tmp_dir("region-round-trip");
    let stone = b(&reg, "base:stone");
    let planks = b(&reg, "base:planks");

    let mut edits = Vec::new();
    {
        let mut w = World::new(4242, dir.clone(), reg.clone());
        for face in crate::planet::Face::ALL {
            for cu in [0u16, 31, 32, 255, 256, 480, 511] {
                for cv in [0u16, 31, 32, 255, 256] {
                    let chunk = crate::planet::ChunkPos::new(face, cu, cv).unwrap();
                    w.ensure_chunk(chunk);
                    let surface = crate::planet::SurfacePos::new(
                        face,
                        cu * crate::chunk::CHUNK_X as u16 + 3,
                        cv * crate::chunk::CHUNK_Z as u16 + 5,
                    )
                    .unwrap();
                    let y = w.surface_height_at(surface) + 1;
                    let block = if cu.is_multiple_of(2) { stone } else { planks };
                    let at = block_pos(surface, y);
                    w.set_block_at(at, block);
                    edits.push((at, block));
                }
            }
        }
        save_world(&mut w);
    }
    assert!(edits.len() > 200, "enough chunks to span many regions");

    // Several region files, not hundreds of chunk files.
    let files: Vec<String> = crate::planet::Face::ALL
        .into_iter()
        .flat_map(|face| {
            std::fs::read_dir(dir.join(face.name()))
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
        })
        .collect();
    let regions = files.iter().filter(|f| f.ends_with(".wfr")).count();
    let loose = files.iter().filter(|f| f.ends_with(".wfc")).count();
    assert!(regions > 1, "the span crosses region boundaries");
    assert_eq!(loose, 0, "no per-chunk files are written any more");
    assert!(
        regions < edits.len(),
        "{regions} region files for {} chunks — the whole point is fewer",
        edits.len()
    );

    let mut w = World::load_or_create(dir, reg.clone()).unwrap();
    for (at, want) in edits {
        w.ensure_chunk(crate::planet::ChunkPos::from_surface(at.surface()));
        assert_eq!(
            w.get_block_at(at),
            want,
            "block at {at:?} did not survive the round trip"
        );
    }
}

#[test]
#[ignore = "measurement probe, not an assertion"]
fn measure_chunk_composition() {
    use crate::chunk::CHUNK_CELLS;
    use std::collections::HashSet;
    let mut w = test_world("compose");
    let (mut ids, mut meta_nz, mut sky_vals, mut lb_nz, mut n) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut worst_ids = 0usize;
    for cx in -3..=3 {
        for cz in -3..=3 {
            let pos = tchunk(cx, cz);
            w.ensure_chunk(pos);
            let c = &w.chunks()[&pos];
            let mut set: HashSet<u16> = HashSet::new();
            let mut sky: HashSet<u8> = HashSet::new();
            let (mut mnz, mut lnz) = (0usize, 0usize);
            for x in 0..16 {
                for z in 0..16 {
                    for y in 0..256 {
                        set.insert(c.get(x, y, z).0);
                        if c.meta(x, y, z) != 0 {
                            mnz += 1;
                        }
                        let (lb, ls) = c.light(x, y, z);
                        sky.insert(ls);
                        if lb != [0, 0, 0] {
                            lnz += 1;
                        }
                    }
                }
            }
            ids += set.len();
            worst_ids = worst_ids.max(set.len());
            meta_nz += mnz;
            sky_vals += sky.len();
            lb_nz += lnz;
            n += 1;
        }
    }
    println!("PROBE over {n} chunks ({CHUNK_CELLS} cells each):");
    println!(
        "  distinct block ids/chunk: avg {:.1}, worst {worst_ids}  -> palette bits {}",
        ids as f64 / n as f64,
        (worst_ids as f64).log2().ceil() as u32
    );
    println!(
        "  meta non-zero: avg {:.2}% of cells",
        100.0 * meta_nz as f64 / (n * CHUNK_CELLS) as f64
    );
    println!(
        "  block-light non-zero: avg {:.3}% of cells",
        100.0 * lb_nz as f64 / (n * CHUNK_CELLS) as f64
    );
    println!(
        "  distinct sky-light values/chunk: avg {:.1} (needs 4 bits)",
        sky_vals as f64 / n as f64
    );
}

#[test]
fn industrial_machines_and_buildings_feed_regional_ire() {
    use crate::world::{BlockEntity, MachineInstance};
    let reg = base_reg();
    let mut w = test_world_with("e12-industrial", reg.clone());
    let my = 120;
    assert_eq!(w.ire, 0.0);
    // Raising a machine mouth costs the valley once (capability E12).
    let mouth = reg.block_id("base:bloomery").unwrap();
    let spot = bp(20, my, 20);
    w.ensure_chunk(spot.chunk());
    w.set_block_at(spot, AIR); // make sure the cell is replaceable
    assert!(w.place_block_at(spot, mouth), "the building places");
    let after_build = w.ire;
    assert!(
        (after_build - crate::world::World::INDUSTRIAL_BUILDING_IRE).abs() < 0.001,
        "raising an industrial building charges ire: {after_build}"
    );
    // A lit bloomery feeds ire while it runs: ~0.01/s.
    build_bloomery(&mut w, &reg, 12, my, 12);
    let iron = it(&reg, "base:iron_ingot");
    let coal = it(&reg, "base:charcoal");
    let mut st = MachineInstance {
        kind: reg.machine_kind("base:bloomery").unwrap_or_default(),
        ..Default::default()
    };
    st.charge[0] = Some(ItemStack::new(&reg, iron, 2));
    st.fuel[0] = Some(ItemStack::new(&reg, coal, 2));
    w.insert_block_entity((12, my, 12), BlockEntity::Multiblock(st));
    w.force_local_weather("clear");
    w.light_bloomery(12, my, 12).expect("lights when charged");
    let before = w.ire;
    for _ in 0..60 {
        w.tick_entities(1.0);
    }
    let fed = w.ire - before;
    assert!(
        fed > 0.5,
        "a minute of firing feeds regional ire: {fed}"
    );
}

#[test]
fn a_mode_can_repoint_the_industrial_feed_off() {
    // Modes parse `industrial_ire`; verify the override plumbing directly.
    let mut ruleset = crate::ruleset::Ruleset::survival();
    assert!(ruleset.industrial_ire, "survival keeps the feed live");
    ruleset.apply_overrides(&crate::registry::ModeDef {
        id: "quiet".into(),
        base: None,
        creative: None,
        hunger: None,
        fall_damage: None,
        drowning: None,
        lava_burn: None,
        hostile_spawns: None,
        ire: None,
        hearts: None,
        weather_extremes: None,
        pvp: None,
        skills: None,
        equipment: None,
        industrial_ire: Some(false),
    });
    assert!(!ruleset.industrial_ire, "the mode repoints the feed off");
}
