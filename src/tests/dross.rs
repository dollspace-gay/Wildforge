use super::*;

use crate::alchemy::{PreparationModifiers, PreparationPhysiology};
use crate::arcane::{
    ArcaneAuthority, ArcaneOwner, ArcaneTransaction, BASE_RESONANCES, DrossMedium,
};
use crate::dross::{DrossBand, DrossCellState};
use crate::planet::{Face, SurfacePos};
use crate::planet_atlas::AtlasPos;

fn dross_world(tag: &str, seed: u32) -> (World, std::path::PathBuf) {
    let dir = tmp_dir(tag);
    let atlas = std::sync::Arc::new(crate::planet_atlas::PlanetAtlas::fixture(seed, 16).unwrap());
    atlas.write_new(&dir).unwrap();
    (
        World::new_with_atlas(seed, dir.clone(), base_reg(), atlas),
        dir,
    )
}

fn surface_atlas_center(world: &World, region: AtlasPos) -> SurfacePos {
    let atlas = world.planet_atlas().unwrap();
    let center = region.center(atlas.side());
    SurfacePos::new(
        center.face,
        center
            .u
            .floor()
            .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1)) as u16,
        center
            .v
            .floor()
            .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1)) as u16,
    )
    .unwrap()
}

/// Re-label existing Geography Current inside one cell. This preserves the
/// parent ledger's exact owner and resonance totals while creating a bounded
/// environmental test burden.
fn seed_dense_soil_dross(world: &mut World, region: AtlasPos, requested: u64, band: DrossBand) {
    let geography = world.arcane_geography.as_mut().unwrap();
    geography
        .dynamic
        .dross_state
        .ensure_cells(geography.dynamic.cells.len());
    let index = region.index(geography.manifest.side);
    let mut remaining = requested;
    for slot in 0..6 {
        let ambient = u64::from(geography.dynamic.cells[index].ambient[slot]).min(remaining);
        geography.dynamic.cells[index].ambient[slot] -= ambient as u16;
        geography.dynamic.cells[index].dross[slot] += ambient as u16;
        remaining -= ambient;
        let deep = u64::from(geography.dynamic.cells[index].deep[slot]).min(remaining);
        geography.dynamic.cells[index].deep[slot] -= deep as u16;
        geography.dynamic.cells[index].dross[slot] += deep as u16;
        remaining -= deep;
        if remaining == 0 {
            break;
        }
    }
    assert_eq!(
        remaining, 0,
        "fixture cell lacked {requested} Current units"
    );
    geography.dynamic.dross_state.cells[index] = DrossCellState {
        band,
        band_since_step: geography.dynamic.dross_state.completed_steps,
        ..DrossCellState::default()
    };
}

fn complete_next_dross_hour(world: &mut World) {
    let next = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .dross_state
        .completed_steps
        .saturating_add(1);
    world
        .planetary_weather_for_test_mut()
        .unwrap()
        .completed_hours = next;
    while world
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .dross_state
        .completed_steps
        < next
    {
        world.tick_dross(31).unwrap();
    }
}

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

#[test]
fn scar_manifestation_preserves_structure_persists_once_and_excavates_exactly() {
    let (mut world, root) = dross_world("dross-scar-host-lifecycle", 0xd205_5201);
    let region = AtlasPos {
        face: Face::PosZ,
        u: 8,
        v: 8,
    };
    let surface = surface_atlas_center(&world, region);
    world.ensure_chunk(ChunkPos::from_surface(surface));
    let scar_pos = crate::planet::BlockPos::new(
        surface.face(),
        surface.u(),
        (world.surface_height_at(surface) + 1) as u8,
        surface.v(),
    )
    .unwrap();
    let structure = b(&world.reg, "base:planks");
    // Exact occupancy remains a second line of defense even when this fixture
    // deliberately leaves the chunk outside the authored/touched index.
    world.set_block_at(scar_pos, structure);
    world.set_block_meta_at(scar_pos, structure, 173);
    seed_dense_soil_dross(&mut world, region, 1_200, DrossBand::Seep);

    complete_next_dross_hour(&mut world);
    let site = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .dross_state
        .scars
        .values()
        .next()
        .cloned()
        .expect("seep produced one canonical persistent site");
    assert_eq!(site.region, region);
    assert!(!site.content_id.is_empty());
    assert_eq!(world.get_block_at(scar_pos), structure);
    assert_eq!(world.get_meta_at(scar_pos), 173);
    let manifestation = site
        .materialized_at
        .expect("bounded handler search found an empty supported cell");
    assert_ne!(manifestation, scar_pos);
    let scar_block = world
        .reg
        .resolve_dross_scar(&site.content_id, site.kind)
        .unwrap()
        .block;
    assert_eq!(world.get_block_at(manifestation), scar_block);
    assert_eq!(
        world
            .arcane_geography
            .as_ref()
            .unwrap()
            .dynamic
            .dross_state
            .materialized
            .get(&manifestation),
        Some(&site.id)
    );
    let scar_current = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .account(&ArcaneOwner::Scar(site.id))
        .unwrap()
        .current
        .clone();
    assert!(scar_current.total() > 0);

    save_world(&mut world);
    drop(world);
    let mut loaded = World::load_or_create(root.clone(), base_reg()).unwrap();
    loaded.ensure_chunk(ChunkPos::from_surface(surface));
    let state = &loaded
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .dross_state;
    assert_eq!(state.scars.len(), 1);
    assert_eq!(state.materialized.len(), 1);
    assert_eq!(
        world_block_name(&loaded, manifestation),
        loaded.reg.block(scar_block).name
    );
    assert_eq!(loaded.get_block_at(scar_pos), structure);
    assert_eq!(loaded.get_meta_at(scar_pos), 173);

    let broken = loaded
        .break_block_at(manifestation, None, true, false)
        .expect("ordinary excavation removes a recoverable scar");
    let fragment = broken.drop.expect("scar excavation returns one fragment");
    assert_eq!(fragment.item, it(&loaded.reg, "base:scar_fragment"));
    assert_eq!(fragment.count, 1);
    assert_ne!(fragment.arcane_id, 0);
    assert_eq!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::ItemDross(fragment.arcane_id))
            .unwrap()
            .current,
        scar_current
    );
    assert!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::Scar(site.id))
            .is_none_or(|account| account.current.is_empty())
    );
    let site = &loaded
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .dross_state
        .scars[&site.id];
    let resolved_scar_id = site.id;
    assert!(site.resolved_step.is_some());
    assert!(site.materialized_at.is_none());
    let replacement = loaded.get_block_at(manifestation);
    assert_ne!(replacement, scar_block);
    assert!(
        replacement == AIR || loaded.reg.is_water(replacement),
        "excavation may expose air or physically backed seep water, not {:?}",
        loaded.reg.block(replacement).name
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

    loaded.retire_arcane_stack_at(manifestation, fragment, "burned in fire");
    assert!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::ItemDross(fragment.arcane_id))
            .is_none_or(|account| account.current.is_empty())
    );
    assert_eq!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::Dross {
                region,
                medium: DrossMedium::Air,
            })
            .unwrap()
            .current,
        scar_current,
        "burning a recovered scar changes its carrier instead of deleting it"
    );
    assert!(
        loaded
            .arcane_geography
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .is_balanced()
    );

    save_world(&mut loaded);
    drop(loaded);
    let mut reopened = World::load_or_create(root, base_reg()).unwrap();
    reopened.ensure_chunk(ChunkPos::from_surface(surface));
    assert_ne!(reopened.get_block_at(manifestation), scar_block);
    assert_eq!(reopened.get_block_at(scar_pos), structure);
    assert_eq!(reopened.get_meta_at(scar_pos), 173);
    let state = &reopened
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .dross_state;
    assert!(state.materialized.is_empty());
    assert_eq!(
        state.scars.len(),
        1,
        "resolved evidence remains bounded and durable"
    );
    assert!(state.scars[&resolved_scar_id].resolved_step.is_some());
    assert!(
        reopened
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::Scar(resolved_scar_id))
            .is_none_or(|account| account.current.is_empty())
    );
}

#[test]
fn touched_chunk_prevents_first_scar_site_and_preserves_authored_bytes() {
    let (mut world, _) = dross_world("dross-touched-chunk-protection", 0xd205_5206);
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
    let structure = b(&world.reg, "base:planks");
    world.set_block_authored_at(at, structure, "authored scar exclusion fixture");
    world.set_block_meta_at(at, structure, 211);
    seed_dense_soil_dross(&mut world, region, 1_200, DrossBand::Seep);

    complete_next_dross_hour(&mut world);

    assert_eq!(world.get_block_at(at), structure);
    assert_eq!(world.get_meta_at(at), 211);
    let state = &world.arcane_geography.as_ref().unwrap().dynamic.dross_state;
    assert!(state.scars.is_empty());
    assert!(state.materialized.is_empty());
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .accounts
            .keys()
            .all(|owner| !matches!(owner, ArcaneOwner::Scar(_)))
    );
}

#[test]
fn closed_scar_handlers_choose_bounded_physical_habitats() {
    use crate::dross::{DrossProvenance, ScarKind, ScarSite};

    let qualify = |tag: &str,
                   content_id: &str,
                   kind: ScarKind,
                   anchor_support: &str,
                   preferred_support: &str| {
        let (mut world, _) = dross_world(tag, 0xd205_5207);
        let region = AtlasPos {
            face: Face::PosZ,
            u: 8,
            v: 8,
        };
        let surface = surface_atlas_center(&world, region);
        let chunk = ChunkPos::from_surface(surface);
        world.insert_empty_chunks_for_test([chunk]);
        let anchor =
            crate::planet::BlockPos::new(surface.face(), surface.u(), 0, surface.v()).unwrap();
        let preferred = anchor.offset(2, 0, 0).unwrap();
        assert_eq!(preferred.chunk(), chunk);
        for (du, dv) in [
            (0, 0),
            (1, 0),
            (0, 1),
            (-1, 0),
            (0, -1),
            (1, 1),
            (-1, 1),
            (-1, -1),
            (1, -1),
            (2, 0),
            (0, 2),
            (-2, 0),
            (0, -2),
        ] {
            let support = anchor.offset(du, 0, dv).unwrap();
            world.set_block_at(support, AIR);
            world.set_block_at(support.offset(0, 1, 0).unwrap(), AIR);
        }
        world.set_block_at(anchor, b(&world.reg, anchor_support));
        world.set_block_at(preferred, b(&world.reg, preferred_support));
        world
            .arcane_geography
            .as_mut()
            .unwrap()
            .dynamic
            .dross_state
            .scars
            .insert(
                1,
                ScarSite {
                    id: 1,
                    region,
                    kind,
                    content_id: content_id.into(),
                    site_slot: 0,
                    created_step: 1,
                    last_changed_step: 1,
                    breach_count: 0,
                    materialized_at: None,
                    resolved_step: None,
                    actor_hint: None,
                    installation_hint: None,
                    provenance: DrossProvenance::default(),
                },
            );

        world.refresh_dross_scars_for_test();

        let site = &world
            .arcane_geography
            .as_ref()
            .unwrap()
            .dynamic
            .dross_state
            .scars[&1];
        let materialized = site
            .materialized_at
            .expect("closed placement handler found a bounded site");
        if kind == ScarKind::WetFilm {
            let below = materialized.offset(0, -1, 0).unwrap();
            assert!(
                world.reg.is_water(world.get_block_at(below))
                    || [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .into_iter()
                        .filter_map(|(du, dv)| below.offset(du, 0, dv))
                        .any(|neighbor| world.reg.is_water(world.get_block_at(neighbor))),
                "water-margin handler selected a dry unsupported cell"
            );
        } else {
            assert_eq!(materialized, preferred.offset(0, 1, 0).unwrap());
        }
        assert_eq!(
            world.get_block_at(materialized),
            world
                .reg
                .resolve_dross_scar(content_id, kind)
                .unwrap()
                .block
        );
        assert_eq!(world.get_block_at(anchor), b(&world.reg, anchor_support));
        assert_eq!(
            world.get_block_at(preferred),
            b(&world.reg, preferred_support)
        );
    };

    qualify(
        "dross-handler-water-margin",
        "base:scar_wet_film",
        ScarKind::WetFilm,
        "base:stone",
        "base:water",
    );
    qualify(
        "dross-handler-filament",
        "base:scar_forest_threads",
        ScarKind::ForestThreads,
        "base:stone",
        "base:log",
    );
    qualify(
        "dross-handler-mineral",
        "base:scar_industrial_scale",
        ScarKind::IndustrialScale,
        "base:log",
        "base:stone",
    );
}

fn world_block_name(world: &World, pos: crate::planet::BlockPos) -> String {
    world.reg.block(world.get_block_at(pos)).name.clone()
}

#[test]
fn scar_burden_stalls_cultivation_without_deleting_block_or_metadata() {
    let (mut world, _) = dross_world("dross-crop-stall", 0xd205_5202);
    let region = AtlasPos {
        face: Face::PosZ,
        u: 8,
        v: 8,
    };
    let surface = surface_atlas_center(&world, region);
    world.ensure_chunk(ChunkPos::from_surface(surface));
    let soil = crate::planet::BlockPos::new(
        surface.face(),
        surface.u(),
        (world.surface_height_at(surface) + 1) as u8,
        surface.v(),
    )
    .unwrap();
    let crop = soil.offset(0, 1, 0).unwrap();
    world.set_block_at(soil, b(&world.reg, "base:farmland"));
    let wheat = b(&world.reg, "base:wheat_seeds");
    world.set_block_meta_at(crop, wheat, 117);
    let index = region.index(world.planet_atlas().unwrap().side());
    world
        .arcane_geography
        .as_mut()
        .unwrap()
        .dynamic
        .dross_state
        .cells[index]
        .band = DrossBand::Scar;
    let mut rng = 7;
    for _ in 0..1_000 {
        world.random_tick(&mut rng);
    }
    assert_eq!(world.get_block_at(crop), wheat);
    assert_eq!(world.get_meta_at(crop), 117);
}

#[test]
fn environmental_exposure_is_bounded_reversible_and_does_not_change_ire() {
    let (mut world, _) = dross_world("dross-reversible-exposure", 0xd205_5203);
    let polluted = AtlasPos {
        face: Face::PosZ,
        u: 8,
        v: 8,
    };
    let clean = polluted
        .step(
            crate::planet::Direction4::East,
            world.planet_atlas().unwrap().side(),
        )
        .pos;
    let side = world.planet_atlas().unwrap().side();
    world
        .arcane_geography
        .as_mut()
        .unwrap()
        .dynamic
        .dross_state
        .cells[polluted.index(side)]
    .band = DrossBand::Scar;
    let polluted_surface = surface_atlas_center(&world, polluted);
    let clean_surface = surface_atlas_center(&world, clean);
    let polluted_pos = crate::planet::BlockPos::new(
        polluted_surface.face(),
        polluted_surface.u(),
        100,
        polluted_surface.v(),
    )
    .unwrap();
    let clean_pos = crate::planet::BlockPos::new(
        clean_surface.face(),
        clean_surface.u(),
        100,
        clean_surface.v(),
    )
    .unwrap();
    let actor = [42; 16];
    let ire = world.ire;
    let initial = PreparationPhysiology {
        health: 20.0,
        max_health: 20.0,
        hunger: 20.0,
        nutrition: [1.0; 5],
        strain: 0.0,
        bodily_dross: 0,
    };
    let _ = world
        .tick_preparation_statuses(actor, polluted_pos, initial)
        .unwrap();
    world.set_simulation_clock(world.clock() + 20.0);
    let exposed = world
        .tick_preparation_statuses(actor, polluted_pos, initial)
        .unwrap();
    assert!(exposed.physiology.bodily_dross > 0);
    assert_eq!(exposed.modifiers.dross_band, DrossBand::Scar.ordinal());
    assert!(exposed.modifiers.recovery_permille < 1_000);
    assert!(exposed.modifiers.perception_permille < 1_000);
    assert!(exposed.modifiers.stamina_permille < 1_000);
    assert_eq!(world.ire, ire);

    world.set_simulation_clock(world.clock() + 120.0);
    let recovered = world
        .tick_preparation_statuses(actor, clean_pos, exposed.physiology)
        .unwrap();
    assert_eq!(recovered.physiology.bodily_dross, 0);
    assert_eq!(recovered.modifiers, PreparationModifiers::default());
    assert_eq!(world.ire, ire);
}
