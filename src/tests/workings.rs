use super::*;

fn settle_channel(world: &mut World, id: u64) -> crate::workings::WorkingResult {
    world.set_simulation_clock(
        world.clock() + f64::from(crate::workings::MIN_WAND_SETTLE_SECONDS) + 0.01,
    );
    world.activate_working(id).unwrap()
}

fn workings_world(tag: &str) -> (World, crate::planet::BlockPos, ItemStack) {
    let mut world = super::implements::embodied_implements_world(tag);
    let (frame, _) = super::implements::install_frame_fixture(&mut world);
    let (wand, _) = super::implements::assemble_fixture_wand(&mut world, frame);
    let player_source = frame.offset(0, 2, 0).unwrap();
    world.set_block_at(player_source, AIR);
    (world, player_source, wand)
}

fn ritual_world(
    tag: &str,
) -> (
    World,
    crate::planet::BlockPos,
    crate::planet::BlockPos,
    crate::planet::BlockPos,
    ItemStack,
) {
    use crate::implements::FrameAction;

    let mut world = super::implements::embodied_implements_world(tag);
    let (controller, first_vessel) = super::implements::install_frame_fixture(&mut world);
    world.ensure_block_entity_at(
        controller,
        crate::world::BlockEntity::BindingFrame(Default::default()),
    );
    let wand = super::implements::assemble_fixture_wand(&mut world, controller).0;

    // Calibrate a second vessel through a separate complete physical frame,
    // then move that exact stable item beside the ritual controller.
    let second_frame = controller.offset(8, 0, 0).unwrap();
    let second_vessel_fixture = second_frame.offset(-1, 0, 0).unwrap();
    for (pos, block) in [
        (second_frame, b(&world.reg, "base:binding_frame")),
        (
            second_frame.offset(1, 0, 0).unwrap(),
            b(&world.reg, "base:focus_mount"),
        ),
        (
            second_frame.offset(0, 0, 1).unwrap(),
            b(&world.reg, "base:arcane_conductor"),
        ),
        (
            second_frame.offset(0, 0, -1).unwrap(),
            b(&world.reg, "base:containment_post"),
        ),
    ] {
        world.set_block_authored_at(pos, block, "second ritual fixture");
    }
    assert!(world.place_item_block_at(
        second_vessel_fixture,
        ItemStack::new(&world.reg, it(&world.reg, "base:charge_vessel"), 1),
    ));
    world
        .operate_binding_frame(
            second_frame,
            &mut Inventory::new(),
            0,
            FrameAction::Calibrate,
            None,
            "ritual-fixture",
        )
        .unwrap();
    let second_id = match world.block_entity_at(&second_vessel_fixture).unwrap() {
        crate::world::BlockEntity::ChargeVessel(vessel) => vessel.vessel.unwrap().arcane_id,
        _ => panic!("second ritual fixture did not embody its vessel"),
    };
    assert_ne!(second_id, 0);
    let pick = it(&world.reg, "base:bronze_pickaxe");
    assert!(
        world
            .break_block_at(second_vessel_fixture, Some(pick), true, false)
            .is_some()
    );
    let second_stack = world
        .take_pending_drops()
        .into_iter()
        .find_map(|(_, stack)| (stack.arcane_id == second_id).then_some(stack))
        .unwrap();
    let second_vessel = controller.offset(-2, 0, 0).unwrap();
    assert!(world.place_item_block_at(second_vessel, second_stack));
    assert!(world.binding_frame_layout(controller).valid);
    (world, controller, first_vessel, second_vessel, wand)
}

fn vessel_stack_at(world: &World, pos: crate::planet::BlockPos) -> ItemStack {
    match world.block_entity_at(&pos).unwrap() {
        crate::world::BlockEntity::ChargeVessel(vessel) => vessel.vessel.unwrap(),
        _ => panic!("fixture vessel has the wrong physical state"),
    }
}

fn charge_fixture_vessel(world: &mut World, vessel_id: u64, desired_clean: u64) {
    use crate::arcane::{ArcaneAuthority, ArcaneOwner, ArcaneTransaction};

    let destination = ArcaneOwner::Item(vessel_id);
    let clean = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_clean_total(vessel_id)
        .unwrap_or_default();
    let needed = desired_clean.saturating_sub(clean);
    if needed == 0 {
        return;
    }
    let source = ArcaneOwner::Deep;
    let (source_version, destination_version, current) = {
        let ledger = world.arcane_ledger.as_ref().unwrap();
        let account = ledger.account(&source).unwrap();
        let mut available = account.current.clone();
        let current = available.take_units(needed, std::iter::empty()).unwrap();
        (account.version, ledger.version_of(&destination), current)
    };
    let ledger = world.arcane_ledger.as_mut().unwrap();
    let transaction = ArcaneTransaction::transfer(
        ledger.system_transaction_id().unwrap(),
        source,
        source_version,
        destination,
        destination_version,
        current,
        ArcaneAuthority::System,
        "ritual test fixture charged a real bounded vessel",
    );
    ledger.commit(transaction).unwrap();
}

fn fitted_lens_stack(world: &mut World) -> ItemStack {
    let item = it(&world.reg, "base:tuning_lens");
    let definition = world.reg.item(item).clone();
    let arcane = definition.arcane.as_ref().unwrap();
    let arcane_id = world
        .arcane_ledger
        .as_mut()
        .unwrap()
        .bind_new_item(
            crate::arcane::ArcaneOwner::Deep,
            arcane,
            &definition.name,
            "workings fitted tuning-lens fixture",
        )
        .unwrap();
    ItemStack {
        arcane_id,
        ..ItemStack::new(&world.reg, item, 1)
    }
}

fn move_fixture_current_to_deep(
    world: &mut World,
    owner: crate::arcane::ArcaneOwner,
    retained_units: u64,
) {
    use crate::arcane::{ArcaneAuthority, ArcaneOwner, ArcaneTransaction};

    let Some(account) = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .account(&owner)
        .cloned()
    else {
        return;
    };
    let amount = account.current.total().saturating_sub(retained_units);
    if amount == 0 {
        return;
    }
    let mut available = account.current.clone();
    let moved = available.take_units(amount, std::iter::empty()).unwrap();
    let destination = ArcaneOwner::Deep;
    let ledger = world.arcane_ledger.as_mut().unwrap();
    let transaction = ArcaneTransaction::transfer(
        ledger.system_transaction_id().unwrap(),
        owner,
        account.version,
        destination.clone(),
        ledger.version_of(&destination),
        moved,
        ArcaneAuthority::System,
        "workings fixture isolated an unsafe local draw",
    );
    ledger.commit(transaction).unwrap();
}

fn persistent_workings_world(
    tag: &str,
) -> (
    World,
    std::path::PathBuf,
    crate::planet::BlockPos,
    ItemStack,
) {
    let reg = base_reg();
    let dir = tmp_dir(tag);
    let atlas = std::sync::Arc::new(crate::planet_atlas::PlanetAtlas::fixture(8_806, 16).unwrap());
    atlas.write_new(&dir).unwrap();
    let mut world = World::new_with_atlas(8_806, dir.clone(), reg, atlas);
    let frame = bp(8, 100, 8);
    let center = frame.chunk();
    for chunk in (-1..=1).flat_map(|du| (-1..=1).map(move |dv| center.offset(du, dv))) {
        world.ensure_chunk(chunk);
    }
    for (pos, block) in [
        (frame, b(&world.reg, "base:binding_frame")),
        (bp(9, 100, 8), b(&world.reg, "base:focus_mount")),
        (bp(8, 100, 9), b(&world.reg, "base:arcane_conductor")),
        (bp(8, 100, 7), b(&world.reg, "base:containment_post")),
    ] {
        world.set_block_authored_at(pos, block, "persistent workings fixture");
    }
    assert!(world.place_item_block_at(
        bp(7, 100, 8),
        ItemStack::new(&world.reg, it(&world.reg, "base:charge_vessel"), 1),
    ));
    let wand = super::implements::assemble_fixture_wand(&mut world, frame).0;
    let source = frame.offset(0, 2, 0).unwrap();
    world.set_block_at(source, AIR);
    save_world(&mut world);
    (world, dir, source, wand)
}

mod admission;
mod cancellation;
mod draw;
mod fieldmend;
mod forced_wand_draw_crosses_the_safe_floor_visibly_and_is_audited;
mod gleam;
mod holdfast;
mod kindle;
mod nudge;
mod pricing;
mod recovery;
mod rituals;
mod rootwake;
mod trace;
mod ward_is_closed_supplied_ire_costed_and_breaks_without_rewriting_construction;
