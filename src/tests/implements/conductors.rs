//! Conductors scenarios.

use super::*;

#[test]
fn conductor_networks_obey_adjacency_unloaded_boundaries_size_breaks_and_throughput() {
    use crate::implements::{FrameAction, MAX_CONDUCTOR_NETWORK};

    // A conductor endpoint at a loaded chunk edge sees the missing neighbor
    // and pauses the whole frame. Loading that exact boundary restores it;
    // a separated wire still cannot count as an endpoint.
    let mut boundary = embodied_implements_world("implements-conductor-boundary");
    let frame = bp(14, 100, 8);
    boundary.insert_empty_chunks_for_test([frame.chunk()]);
    let focus = frame.offset(0, 0, 1).unwrap();
    let vessel_pos = frame.offset(0, 0, -1).unwrap();
    let conductor = frame.offset(1, 0, 0).unwrap();
    let containment = frame.offset(-1, 0, 0).unwrap();
    boundary.set_block_at(frame, b(&boundary.reg, "base:binding_frame"));
    boundary.set_block_at(focus, b(&boundary.reg, "base:focus_mount"));
    boundary.set_block_at(conductor, b(&boundary.reg, "base:arcane_conductor"));
    boundary.set_block_at(containment, b(&boundary.reg, "base:containment_post"));
    assert!(boundary.place_item_block_at(
        vessel_pos,
        ItemStack::new(&boundary.reg, it(&boundary.reg, "base:charge_vessel"), 1),
    ));
    let layout = boundary.binding_frame_layout(frame);
    assert!(layout.touches_unloaded);
    assert!(!layout.valid);
    let across: Vec<_> = (-1..=1)
        .flat_map(|du| {
            (-1..=1).filter_map(move |dv| conductor.offset(du, 0, dv).map(|at| at.chunk()))
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter(|chunk| !boundary.has_chunk(*chunk))
        .collect();
    boundary.insert_empty_chunks_for_test(across);
    assert!(boundary.binding_frame_layout(frame).valid);
    boundary.set_block_at(conductor, AIR);
    assert!(
        boundary
            .binding_frame_layout(frame)
            .problems
            .iter()
            .any(|problem| problem.contains("endpoint"))
    );

    // A connected component has a hard bounded census. Breaking one segment
    // deterministically disconnects the far tail and restores a bounded local
    // network rather than retaining stale connectivity.
    let mut capped = embodied_implements_world("implements-conductor-cap");
    let (frame, _) = install_frame_fixture(&mut capped);
    let first = frame.offset(0, 0, 1).unwrap();
    let positions: Vec<_> = (0..=MAX_CONDUCTOR_NETWORK)
        .map(|step| first.offset(0, 0, step as i32).unwrap())
        .collect();
    let chunks = positions
        .iter()
        .flat_map(|pos| {
            (-1..=1).flat_map(move |du| {
                (-1..=1).filter_map(move |dv| pos.offset(du, 0, dv).map(|at| at.chunk()))
            })
        })
        .collect::<std::collections::BTreeSet<_>>();
    let chunks: Vec<_> = chunks
        .into_iter()
        .filter(|chunk| !capped.has_chunk(*chunk))
        .collect();
    capped.insert_empty_chunks_for_test(chunks);
    for pos in &positions {
        capped.set_block_at(*pos, b(&capped.reg, "base:arcane_conductor"));
    }
    let overflow = capped.binding_frame_layout(frame);
    assert_eq!(overflow.network_size as usize, MAX_CONDUCTOR_NETWORK + 1);
    assert!(!overflow.valid);
    assert!(
        overflow
            .problems
            .iter()
            .any(|problem| problem.contains("exceeds"))
    );
    let cut = positions[MAX_CONDUCTOR_NETWORK / 2];
    capped.set_block_at(cut, AIR);
    let rebuilt = capped.binding_frame_layout(frame);
    assert!(rebuilt.valid, "{:?}", rebuilt.problems);
    assert_eq!(rebuilt.network_size as usize, MAX_CONDUCTOR_NETWORK / 2);
    let solve_started = std::time::Instant::now();
    for _ in 0..1_000 {
        let measured = capped.binding_frame_layout(frame);
        assert_eq!(measured.network_size as usize, MAX_CONDUCTOR_NETWORK / 2);
    }
    assert!(
        solve_started.elapsed() < std::time::Duration::from_secs(2),
        "1,000 bounded conductor solves took {:?}",
        solve_started.elapsed()
    );

    // One directly attached segment provides exactly one 32-unit route budget
    // before source/target conductivity and dross reduce the useful delivery.
    let mut throughput = embodied_implements_world("implements-conductor-throughput");
    let (frame, vessel_pos) = install_frame_fixture(&mut throughput);
    let (wand, revision) = assemble_fixture_wand(&mut throughput, frame);
    let vessel = match throughput.block_entity_at(&vessel_pos).unwrap() {
        crate::world::BlockEntity::ChargeVessel(state) => state.vessel.unwrap(),
        _ => panic!("wrong vessel entity"),
    };
    let source_before = throughput
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_clean_total(vessel.arcane_id)
        .unwrap();
    let target_before = throughput
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_clean_total(wand.arcane_id)
        .unwrap();
    let total = throughput
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .total;
    let moved = throughput
        .operate_binding_frame(
            frame,
            &mut Inventory::new(),
            0,
            FrameAction::Transfer,
            Some(revision),
            "test",
        )
        .unwrap();
    assert!(moved.success);
    let source_after = throughput
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_clean_total(vessel.arcane_id)
        .unwrap();
    let target_after = throughput
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_clean_total(wand.arcane_id)
        .unwrap();
    assert!((1..=32).contains(&(source_before - source_after)));
    assert!((1..=32).contains(&(target_after - target_before)));
    assert_eq!(
        throughput
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        total
    );
}

#[test]
fn a_loaded_conductor_extracts_only_from_a_real_local_confluence_or_well() {
    use crate::implements::FrameAction;

    let mut world = embodied_implements_world("implements-place-conductor-source");
    let (site, side) = {
        let geography = world.arcane_geography.as_ref().unwrap();
        let site = geography
            .catalog
            .sites
            .iter()
            .find(|site| {
                site.active
                    && matches!(
                        site.kind,
                        crate::arcane_geography::ArcanePlaceType::Confluence
                            | crate::arcane_geography::ArcanePlaceType::Well
                    )
                    && geography.dynamic.cells[site.center.index(geography.manifest.side)]
                        .ambient_total()
                        > 64
            })
            .cloned()
            .expect("fixture has no charged confluence/well");
        (site, geography.manifest.side)
    };
    let center = site.center.center(side);
    let surface = crate::planet::SurfacePos::new(
        center.face,
        center.u.floor() as u16,
        center.v.floor() as u16,
    )
    .unwrap();
    let frame =
        crate::planet::BlockPos::new(surface.face(), surface.u(), 100, surface.v()).unwrap();
    let focus = frame.offset(1, 0, 0).unwrap();
    let vessel_pos = frame.offset(-1, 0, 0).unwrap();
    let conductor = frame.offset(0, 0, 1).unwrap();
    let containment = frame.offset(0, 0, -1).unwrap();
    let chunks = [frame, focus, vessel_pos, conductor, containment]
        .into_iter()
        .flat_map(|pos| {
            (-1..=1).flat_map(move |du| {
                (-1..=1).filter_map(move |dv| pos.offset(du, 0, dv).map(|at| at.chunk()))
            })
        })
        .collect::<std::collections::BTreeSet<_>>();
    world.insert_empty_chunks_for_test(chunks);
    world.set_block_at(frame, b(&world.reg, "base:binding_frame"));
    world.set_block_at(focus, b(&world.reg, "base:focus_mount"));
    world.set_block_at(conductor, b(&world.reg, "base:arcane_conductor"));
    world.set_block_at(containment, b(&world.reg, "base:containment_post"));
    assert!(world.place_item_block_at(
        vessel_pos,
        ItemStack::new(&world.reg, it(&world.reg, "base:charge_vessel"), 1),
    ));
    assert!(world.binding_frame_layout(frame).valid);
    let (wand, revision) = assemble_fixture_wand(&mut world, frame);
    let vessel = match world.block_entity_at(&vessel_pos).unwrap() {
        crate::world::BlockEntity::ChargeVessel(state) => state.vessel.unwrap(),
        _ => panic!("wrong vessel entity"),
    };

    // A conductor request has no authority to name or scan arbitrary item
    // owners. Put a separately charged charm in a nearby chest and prove the
    // place route neither debits it nor moves its stable identity, even though
    // it is closer than part of the connected conductor component.
    let chest = frame.offset(2, 0, 0).unwrap();
    world.set_block_authored_at(
        chest,
        b(&world.reg, "base:chest"),
        "unowned conductor authority fixture",
    );
    world.insert_block_entity_at(chest, crate::world::BlockEntity::Chest(Default::default()));
    let mut unowned = Inventory::new();
    let legacy_charm = ItemStack::new(&world.reg, it(&world.reg, "base:charm_quiet"), 1);
    world
        .record_external_stack(legacy_charm, "unowned conductor authority fixture")
        .unwrap();
    unowned.slots[0] = Some(legacy_charm);
    assert_eq!(
        world.migrate_legacy_player_charms(
            frame,
            &mut unowned,
            &mut Default::default(),
            &mut None,
            "a different chest owner",
        ),
        1
    );
    let unowned_charm = unowned.slots[0].take().unwrap();
    let unowned_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_current_total(unowned_charm.arcane_id)
        .unwrap();
    match world.block_entity_mut_at(&chest).unwrap() {
        crate::world::BlockEntity::Chest(state) => state.slots[0] = Some(unowned_charm),
        _ => panic!("unowned fixture changed block-entity kind"),
    }

    drain_fixture_item_to_spark(&mut world, frame, wand.arcane_id);
    drain_fixture_item_to_spark(&mut world, frame, vessel.arcane_id);
    let total = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let geography_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .account(&crate::arcane::ArcaneOwner::Geography)
        .unwrap()
        .current
        .total();
    let result = world
        .operate_binding_frame(
            frame,
            &mut Inventory::new(),
            0,
            FrameAction::Transfer,
            Some(revision),
            "test",
        )
        .unwrap();
    assert!(result.success, "{}", result.message);
    assert!(result.message.contains(site.kind.label()));
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_clean_total(wand.arcane_id)
            .map(crate::implements::usable_charge)
            .unwrap_or(0)
            > 0
    );
    let geography_after = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .account(&crate::arcane::ArcaneOwner::Geography)
        .unwrap()
        .current
        .total();
    assert!(geography_after < geography_before);
    let chest_charm = match world.block_entity_at(&chest).unwrap() {
        crate::world::BlockEntity::Chest(state) => state.slots[0].unwrap(),
        _ => panic!("unowned fixture changed block-entity kind"),
    };
    assert_eq!(chest_charm, unowned_charm);
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_current_total(unowned_charm.arcane_id),
        Some(unowned_before),
        "a place conductor extracted from an unowned item"
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        total
    );
}
